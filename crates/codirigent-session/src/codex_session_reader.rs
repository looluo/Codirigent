//! Codex CLI session status reader via JSONL rollout logs.
//!
//! Reads Codex CLI's per-session JSONL files from `~/.codex/sessions/` to determine
//! session status with higher fidelity than OSC 133 alone.
//!
//! Codex CLI writes append-only JSONL rollout files partitioned by date:
//! `~/.codex/sessions/YYYY/MM/DD/rollout-<timestamp>-<uuid>.jsonl`
//!
//! Each line has `{ timestamp, type, payload }` with types:
//! - `session_meta` — first line with `cwd`, `approval_mode`, `sandbox_policy`
//! - `response_item` — messages and function calls
//! - `event_msg` — events like turn completion

use crate::session_reader_common::{is_file_recent, read_file_tail};
use crate::CliSessionStatus;
use codirigent_core::CodexExecutionMode;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, trace};

type SessionFileCacheKey = (PathBuf, Option<String>, Option<u128>);

/// Status derived from Codex CLI's JSONL rollout logs.
pub type CodexSessionStatus = CliSessionStatus;

/// Matched Codex rollout status plus metadata needed by the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexStatusSnapshot {
    /// Derived status for the matched rollout file.
    pub status: CodexSessionStatus,
    /// Codex CLI session id from `session_meta`, if present.
    pub session_id: Option<String>,
    /// Approval mode from `session_meta`, if present.
    pub approval_mode: Option<String>,
}

/// Reads Codex CLI session data to determine status.
pub struct CodexSessionReader {
    /// Path to ~/.codex
    codex_home: PathBuf,
    /// Cache: CWD → rollout file path (avoid re-scanning on every poll).
    session_file_cache: HashMap<SessionFileCacheKey, (PathBuf, SystemTime)>,
}

/// Approval mode from Codex CLI session metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ApprovalMode {
    /// Approval policy could not be determined from saved state or rollout metadata.
    Unknown,
    /// All tool calls require user approval.
    Suggest,
    /// File edits are auto-approved, other tools require approval.
    AutoEdit,
    /// All tool calls are auto-approved.
    FullAuto,
}

/// Parsed session metadata from the first line of a rollout file.
#[derive(Debug, Clone, Deserialize)]
struct SessionMeta {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    approval_mode: Option<String>,
}

/// A JSONL entry from a Codex rollout file.
#[derive(Debug, Deserialize)]
struct RolloutEntry {
    /// Entry type: "session_meta", "response_item", "event_msg".
    #[serde(rename = "type", default)]
    entry_type: String,
    /// Payload varies by type.
    #[serde(default)]
    payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
struct RolloutMatch {
    path: PathBuf,
    mtime: SystemTime,
    meta: SessionMeta,
}

impl CodexSessionReader {
    /// Create a new reader. Returns `None` if `~/.codex` doesn't exist.
    pub fn new() -> Option<Self> {
        let codex_home = Self::resolve_codex_home()?;
        if !codex_home.is_dir() {
            debug!("Codex home not found, Codex session reader disabled");
            return None;
        }
        Some(Self {
            codex_home,
            session_file_cache: HashMap::new(),
        })
    }

    /// Resolve the Codex home directory.
    /// Uses `CODEX_HOME` env var if set, otherwise `~/.codex`.
    fn resolve_codex_home() -> Option<PathBuf> {
        if let Ok(home) = std::env::var("CODEX_HOME") {
            return Some(PathBuf::from(home));
        }
        let home = dirs::home_dir()?;
        Some(home.join(".codex"))
    }

    /// Get the status of a Codex CLI session by reading its rollout JSONL log.
    ///
    /// `working_dir` is the session's working directory, used to locate the
    /// project-specific rollout file under `~/.codex/sessions/`.
    /// `_pid` is accepted for API consistency but not used (Codex uses different file naming).
    pub fn get_status(&mut self, working_dir: &Path, _pid: Option<u32>) -> CodexSessionStatus {
        self.get_status_snapshot_if_recent(working_dir, None, _pid, Duration::MAX, None, None)
            .map(|snapshot| snapshot.status)
            .unwrap_or(CodexSessionStatus::Unknown)
    }

    /// Get the status of a Codex CLI session if its rollout file is recent enough.
    ///
    /// This avoids false positives from old rollout files on generic shells.
    pub fn get_status_if_recent(
        &mut self,
        working_dir: &Path,
        session_id: Option<&str>,
        _pid: Option<u32>,
        max_age: Duration,
        created_after: Option<SystemTime>,
    ) -> Option<CodexSessionStatus> {
        self.get_status_snapshot_if_recent(
            working_dir,
            session_id,
            _pid,
            max_age,
            created_after,
            None,
        )
        .map(|snapshot| snapshot.status)
    }

    /// Get the status and matched `session_meta` details for a Codex CLI session.
    pub fn get_status_snapshot_if_recent(
        &mut self,
        working_dir: &Path,
        session_id: Option<&str>,
        _pid: Option<u32>,
        max_age: Duration,
        created_after: Option<SystemTime>,
        saved_mode: Option<CodexExecutionMode>,
    ) -> Option<CodexStatusSnapshot> {
        let Some(rollout_match) = self.find_rollout_match(working_dir, session_id, created_after)
        else {
            trace!(?working_dir, "No Codex rollout file found");
            return None;
        };

        if !is_file_recent(&rollout_match.path, max_age) {
            debug!(
                rollout_path = ?rollout_match.path,
                ?max_age,
                "Skip stale Codex rollout file while probing GenericShell"
            );
            return None;
        }

        let approval_mode =
            Self::resolve_approval_mode(saved_mode, rollout_match.meta.approval_mode.as_deref());

        let Some(tail) = read_file_tail(&rollout_match.path, 131_072) else {
            return Some(CodexStatusSnapshot {
                status: CodexSessionStatus::Unknown,
                session_id: rollout_match.meta.id,
                approval_mode: rollout_match.meta.approval_mode,
            });
        };

        Some(CodexStatusSnapshot {
            status: self.determine_status(&tail, &approval_mode),
            session_id: rollout_match.meta.id,
            approval_mode: rollout_match.meta.approval_mode,
        })
    }

    /// Find the rollout file for a given working directory.
    ///
    /// Codex stores rollout files under `~/.codex/sessions/YYYY/MM/DD/`.
    /// We scan in reverse date order (newest first) and check the first line
    /// of each file for the `cwd` field.
    #[cfg(test)]
    #[cfg(test)]
    fn find_rollout_file(
        &mut self,
        working_dir: &Path,
        session_id: Option<&str>,
        created_after: Option<SystemTime>,
    ) -> Option<PathBuf> {
        self.find_rollout_match(working_dir, session_id, created_after)
            .map(|rollout_match| rollout_match.path)
    }

    fn find_rollout_match(
        &mut self,
        working_dir: &Path,
        session_id: Option<&str>,
        created_after: Option<SystemTime>,
    ) -> Option<RolloutMatch> {
        let cache_key = (
            working_dir.to_path_buf(),
            session_id.map(str::to_owned),
            created_after.and_then(Self::system_time_cache_key),
        );

        // Check cache first
        if let Some((cached_path, cached_mtime)) = self.session_file_cache.get(&cache_key) {
            if let Ok(meta) = fs::metadata(cached_path) {
                if let Ok(mtime) = meta.modified() {
                    if mtime == *cached_mtime
                        && Self::is_rollout_recent_enough(mtime, created_after)
                    {
                        if let Some(meta) = Self::read_session_meta(cached_path) {
                            return Some(RolloutMatch {
                                path: cached_path.clone(),
                                mtime,
                                meta,
                            });
                        }
                    }
                }
            }
        }

        let sessions_dir = self.codex_home.join("sessions");
        if !sessions_dir.is_dir() {
            return None;
        }

        // Collect date directories and sort in reverse order (newest first)
        let mut date_dirs = Vec::new();
        Self::collect_date_dirs(&sessions_dir, &mut date_dirs);
        date_dirs.sort_unstable_by(|a, b| b.cmp(a));

        let working_dir_str = working_dir.to_string_lossy();
        let expected_suffix = session_id
            .filter(|id| !id.is_empty())
            .map(|id| format!("-{id}.jsonl"));
        let mut best_candidate: Option<(RolloutMatch, bool, Option<Duration>)> = None;

        for date_dir in date_dirs {
            // List rollout files in this date directory, sorted by mtime (newest first)
            let mut rollout_files = Vec::new();
            if let Ok(entries) = fs::read_dir(&date_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
                    {
                        if let Ok(meta) = path.metadata() {
                            if let Ok(mtime) = meta.modified() {
                                rollout_files.push((path, mtime));
                            }
                        }
                    }
                }
            }
            rollout_files.sort_unstable_by(|a, b| b.1.cmp(&a.1));

            for (path, mtime) in rollout_files {
                if !Self::is_rollout_recent_enough(mtime, created_after) {
                    continue;
                }

                // Read first line for session_meta
                if let Some(meta) = Self::read_session_meta(&path) {
                    if meta.cwd.as_deref() != Some(working_dir_str.as_ref()) {
                        continue;
                    }

                    if let Some(expected_session_id) = session_id.filter(|id| !id.is_empty()) {
                        let session_id_matches_meta =
                            meta.id.as_deref() == Some(expected_session_id);
                        let session_id_matches_filename = expected_suffix
                            .as_deref()
                            .and_then(|suffix| {
                                path.file_name()
                                    .and_then(|name| name.to_str())
                                    .map(|name| name.ends_with(suffix))
                            })
                            .unwrap_or(false);
                        if !session_id_matches_meta && !session_id_matches_filename {
                            continue;
                        }

                        self.session_file_cache
                            .insert(cache_key.clone(), (path.clone(), mtime));
                        return Some(RolloutMatch { path, mtime, meta });
                    }

                    let claimed_by_other =
                        Self::path_claimed_by_other(&self.session_file_cache, &path, &cache_key);
                    let start_delta = created_after.and_then(|created_after| {
                        Self::session_meta_started_at(&meta)
                            .and_then(|started_at| started_at.duration_since(created_after).ok())
                    });
                    let should_replace_best = match best_candidate.as_ref() {
                        None => true,
                        Some((_best_match, best_claimed, _best_delta))
                            if *best_claimed && !claimed_by_other =>
                        {
                            true
                        }
                        Some((_best_match, best_claimed, _best_delta))
                            if !*best_claimed && claimed_by_other =>
                        {
                            false
                        }
                        Some((_best_match, _best_claimed, Some(best_delta))) => {
                            start_delta.is_some_and(|delta| delta < *best_delta)
                        }
                        Some((_best_match, _best_claimed, None)) => start_delta.is_some(),
                    };
                    if should_replace_best {
                        best_candidate = Some((
                            RolloutMatch { path, mtime, meta },
                            claimed_by_other,
                            start_delta,
                        ));
                    }
                }
            }
        }

        let (rollout_match, _claimed, _start_delta) = best_candidate?;
        self.session_file_cache
            .insert(cache_key, (rollout_match.path.clone(), rollout_match.mtime));
        Some(rollout_match)
    }

    fn path_claimed_by_other(
        cache: &HashMap<SessionFileCacheKey, (PathBuf, SystemTime)>,
        path: &Path,
        cache_key: &SessionFileCacheKey,
    ) -> bool {
        cache
            .iter()
            .any(|(other_key, (other_path, _))| other_key != cache_key && other_path == path)
    }

    /// Recursively collect date directories (YYYY/MM/DD structure).
    fn collect_date_dirs(base: &Path, out: &mut Vec<PathBuf>) {
        // Walk YYYY dirs
        let Ok(year_entries) = fs::read_dir(base) else {
            return;
        };
        for year_entry in year_entries.flatten() {
            let year_path = year_entry.path();
            if !year_path.is_dir() {
                continue;
            }
            // Walk MM dirs
            let Ok(month_entries) = fs::read_dir(&year_path) else {
                continue;
            };
            for month_entry in month_entries.flatten() {
                let month_path = month_entry.path();
                if !month_path.is_dir() {
                    continue;
                }
                // Walk DD dirs
                let Ok(day_entries) = fs::read_dir(&month_path) else {
                    continue;
                };
                for day_entry in day_entries.flatten() {
                    let day_path = day_entry.path();
                    if day_path.is_dir() {
                        out.push(day_path);
                    }
                }
            }
        }
    }

    fn read_session_meta(path: &Path) -> Option<SessionMeta> {
        let file = fs::File::open(path).ok()?;
        let mut reader = std::io::BufReader::new(file);
        let mut first_line = String::new();
        std::io::BufRead::read_line(&mut reader, &mut first_line).ok()?;

        let entry: RolloutEntry = serde_json::from_str(first_line.trim()).ok()?;
        if entry.entry_type != "session_meta" {
            return None;
        }

        let payload = entry.payload?;
        serde_json::from_value(payload).ok()
    }

    /// Read the CWD from the first line (session_meta) of a rollout file.
    #[cfg(test)]
    fn read_cwd_from_first_line(path: &Path) -> Option<String> {
        Self::read_session_meta(path)?.cwd
    }

    fn resolve_approval_mode(
        saved_mode: Option<CodexExecutionMode>,
        rollout_approval_mode: Option<&str>,
    ) -> ApprovalMode {
        match saved_mode {
            Some(CodexExecutionMode::FullAuto | CodexExecutionMode::Bypass) => {
                ApprovalMode::FullAuto
            }
            None => match rollout_approval_mode {
                Some(mode) if is_full_auto_mode(mode) || is_bypass_mode(mode) => {
                    ApprovalMode::FullAuto
                }
                Some("auto-edit") => ApprovalMode::AutoEdit,
                Some(_) => ApprovalMode::Suggest,
                None => ApprovalMode::Unknown,
            },
        }
    }

    /// Read the approval mode from the first line of a rollout file.
    #[cfg(test)]
    #[cfg(test)]
    fn read_approval_mode(&self, path: &Path) -> ApprovalMode {
        let Some(meta) = Self::read_session_meta(path) else {
            return ApprovalMode::Unknown;
        };

        Self::resolve_approval_mode(None, meta.approval_mode.as_deref())
    }

    /// Core status determination algorithm.
    ///
    /// 1. Find last `response_item` with `function_call` that has no matching
    ///    `function_call_output` → pending tool
    /// 2. If pending tool exists, check approval mode
    /// 3. If no pending tools and last event indicates turn complete → NeedsAttention
    /// 4. Otherwise → Unknown
    fn determine_status(&self, tail: &str, approval_mode: &ApprovalMode) -> CodexSessionStatus {
        let mut pending_calls: HashMap<String, String> = HashMap::new(); // call_id → tool name
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum LastActivity {
            Progress,
            Terminal,
        }

        let mut last_activity = None;

        for line in tail.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let Ok(entry) = serde_json::from_str::<RolloutEntry>(line) else {
                continue;
            };

            if entry.entry_type == "response_item" {
                if let Some(ref payload) = entry.payload {
                    // Check for function_call
                    if let Some(item_type) = payload.get("type").and_then(|v| v.as_str()) {
                        if item_type == "function_call" {
                            if let Some(call_id) = payload.get("call_id").and_then(|v| v.as_str()) {
                                let name = payload
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                pending_calls.insert(call_id.to_string(), name);
                                last_activity = Some(LastActivity::Progress);
                            }
                        } else if item_type == "function_call_output" {
                            if let Some(call_id) = payload.get("call_id").and_then(|v| v.as_str()) {
                                pending_calls.remove(call_id);
                            }
                            last_activity = Some(LastActivity::Progress);
                        } else if matches!(
                            item_type,
                            "reasoning" | "custom_tool_call" | "custom_tool_call_output"
                        ) || (item_type == "message"
                            && payload.get("role").and_then(|v| v.as_str()) == Some("assistant"))
                        {
                            last_activity = Some(LastActivity::Progress);
                        }
                    }
                }
            } else if entry.entry_type == "event_msg" {
                if let Some(ref payload) = entry.payload {
                    if let Some(event_type) = payload.get("type").and_then(|v| v.as_str()) {
                        if matches!(
                            event_type,
                            "task_started" | "user_message" | "agent_message" | "token_count"
                        ) {
                            last_activity = Some(LastActivity::Progress);
                        } else if matches!(
                            event_type,
                            "task_complete"
                                | "turn_complete"
                                | "response.completed"
                                | "response.done"
                        ) {
                            last_activity = Some(LastActivity::Terminal);
                        }
                    }
                }
            }
        }

        // Check for pending tool calls
        if let Some((_call_id, tool_name)) = pending_calls.iter().next() {
            return match approval_mode {
                ApprovalMode::FullAuto => CodexSessionStatus::Working,
                ApprovalMode::Unknown => CodexSessionStatus::Working,
                ApprovalMode::AutoEdit => {
                    if Self::is_file_edit_tool(tool_name) {
                        CodexSessionStatus::Working
                    } else {
                        CodexSessionStatus::NeedsAttention {
                            detail: Some(tool_name.clone()),
                        }
                    }
                }
                ApprovalMode::Suggest => CodexSessionStatus::NeedsAttention {
                    detail: Some(tool_name.clone()),
                },
            };
        }

        // No pending tools — check if turn is complete
        if last_activity == Some(LastActivity::Progress) {
            return CodexSessionStatus::Working;
        }

        CodexSessionStatus::Unknown
    }

    fn is_rollout_recent_enough(mtime: SystemTime, created_after: Option<SystemTime>) -> bool {
        match created_after {
            Some(created_after) => mtime >= created_after,
            None => true,
        }
    }

    fn system_time_cache_key(time: SystemTime) -> Option<u128> {
        time.duration_since(UNIX_EPOCH).ok().map(|d| d.as_millis())
    }

    fn session_meta_started_at(meta: &SessionMeta) -> Option<SystemTime> {
        let timestamp = meta.timestamp.as_deref()?;
        let started_at = chrono::DateTime::parse_from_rfc3339(timestamp)
            .ok()?
            .with_timezone(&chrono::Utc);
        let secs = u64::try_from(started_at.timestamp()).ok()?;
        Some(
            UNIX_EPOCH
                + Duration::from_secs(secs)
                + Duration::from_nanos(u64::from(started_at.timestamp_subsec_nanos())),
        )
    }

    /// Check if a tool name represents a file edit operation.
    fn is_file_edit_tool(name: &str) -> bool {
        let lower = name.to_lowercase();
        lower.contains("write")
            || lower.contains("edit")
            || lower.contains("patch")
            || lower.contains("apply")
    }
}

fn is_full_auto_mode(mode: &str) -> bool {
    mode.eq_ignore_ascii_case("full-auto")
        || mode.eq_ignore_ascii_case("full_auto")
        || mode.eq_ignore_ascii_case("fullauto")
}

fn is_bypass_mode(mode: &str) -> bool {
    mode.eq_ignore_ascii_case("yolo")
        || mode.eq_ignore_ascii_case("bypass")
        || mode.eq_ignore_ascii_case("dangerously-bypass-approvals-and-sandbox")
        || mode.eq_ignore_ascii_case("dangerously_bypass_approvals_and_sandbox")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_unknown_on_empty() {
        let reader = CodexSessionReader {
            codex_home: PathBuf::from("/nonexistent"),
            session_file_cache: HashMap::new(),
        };
        assert_eq!(
            reader.determine_status("", &ApprovalMode::Suggest),
            CodexSessionStatus::Unknown
        );
    }

    #[test]
    fn test_turn_complete_without_pending_tools_is_unknown() {
        let tail = r#"{"type":"response_item","payload":{"type":"function_call","call_id":"c1","name":"shell"}}
{"type":"response_item","payload":{"type":"function_call_output","call_id":"c1"}}
{"type":"event_msg","payload":{"type":"turn_complete"}}"#;

        let reader = CodexSessionReader {
            codex_home: PathBuf::from("/nonexistent"),
            session_file_cache: HashMap::new(),
        };
        assert_eq!(
            reader.determine_status(tail, &ApprovalMode::Suggest),
            CodexSessionStatus::Unknown
        );
    }

    #[test]
    fn test_get_status_if_recent_applies_age_gate() {
        let tmp = TempDir::new().unwrap();
        let sessions_dir = tmp
            .path()
            .join("sessions")
            .join("2026")
            .join("02")
            .join("14");
        std::fs::create_dir_all(&sessions_dir).unwrap();

        let rollout_path = sessions_dir.join("rollout-recent.jsonl");
        std::fs::write(
            &rollout_path,
            r#"{"type":"session_meta","payload":{"cwd":"/Users/test/project","approval_mode":"suggest"}}
{"type":"event_msg","payload":{"type":"turn_complete"}}"#,
        )
        .unwrap();

        let mut reader = CodexSessionReader {
            codex_home: tmp.path().to_path_buf(),
            session_file_cache: HashMap::new(),
        };

        let fresh = reader.get_status_if_recent(
            Path::new("/Users/test/project"),
            None,
            None,
            std::time::Duration::from_secs(10),
            None,
        );
        assert_eq!(fresh, Some(CodexSessionStatus::Unknown));

        std::thread::sleep(std::time::Duration::from_millis(20));
        let stale = reader.get_status_if_recent(
            Path::new("/Users/test/project"),
            None,
            None,
            std::time::Duration::from_nanos(1),
            None,
        );
        assert!(stale.is_none());
    }

    #[test]
    fn test_pending_tool_suggest_mode() {
        let tail = r#"{"type":"response_item","payload":{"type":"function_call","call_id":"c1","name":"shell"}}"#;

        let reader = CodexSessionReader {
            codex_home: PathBuf::from("/nonexistent"),
            session_file_cache: HashMap::new(),
        };
        assert_eq!(
            reader.determine_status(tail, &ApprovalMode::Suggest),
            CodexSessionStatus::NeedsAttention {
                detail: Some("shell".to_string()),
            }
        );
    }

    #[test]
    fn test_saved_bypass_mode_keeps_pending_tool_working_without_rollout_approval_mode() {
        let tmp = TempDir::new().unwrap();
        let sessions_dir = tmp
            .path()
            .join("sessions")
            .join("2026")
            .join("03")
            .join("07");
        fs::create_dir_all(&sessions_dir).unwrap();

        let rollout_path = sessions_dir.join("rollout-123456-codex-session.jsonl");
        fs::write(
            &rollout_path,
            r#"{"type":"session_meta","payload":{"id":"codex-session","cwd":"/Users/test/project"}}
{"type":"response_item","payload":{"type":"function_call","call_id":"c1","name":"shell"}}"#,
        )
        .unwrap();

        let mut reader = CodexSessionReader {
            codex_home: tmp.path().to_path_buf(),
            session_file_cache: HashMap::new(),
        };

        let snapshot = reader
            .get_status_snapshot_if_recent(
                Path::new("/Users/test/project"),
                Some("codex-session"),
                None,
                Duration::from_secs(60),
                None,
                Some(CodexExecutionMode::Bypass),
            )
            .unwrap();

        assert_eq!(snapshot.status, CodexSessionStatus::Working);
        assert_eq!(snapshot.session_id.as_deref(), Some("codex-session"));
    }

    #[test]
    fn test_pending_tool_full_auto() {
        let tail = r#"{"type":"response_item","payload":{"type":"function_call","call_id":"c1","name":"shell"}}"#;

        let reader = CodexSessionReader {
            codex_home: PathBuf::from("/nonexistent"),
            session_file_cache: HashMap::new(),
        };
        assert_eq!(
            reader.determine_status(tail, &ApprovalMode::FullAuto),
            CodexSessionStatus::Working
        );
    }

    #[test]
    fn test_pending_file_edit_auto_edit_mode() {
        let tail = r#"{"type":"response_item","payload":{"type":"function_call","call_id":"c1","name":"file_write"}}"#;

        let reader = CodexSessionReader {
            codex_home: PathBuf::from("/nonexistent"),
            session_file_cache: HashMap::new(),
        };
        assert_eq!(
            reader.determine_status(tail, &ApprovalMode::AutoEdit),
            CodexSessionStatus::Working
        );
    }

    #[test]
    fn test_pending_shell_auto_edit_mode() {
        let tail = r#"{"type":"response_item","payload":{"type":"function_call","call_id":"c1","name":"shell"}}"#;

        let reader = CodexSessionReader {
            codex_home: PathBuf::from("/nonexistent"),
            session_file_cache: HashMap::new(),
        };
        assert_eq!(
            reader.determine_status(tail, &ApprovalMode::AutoEdit),
            CodexSessionStatus::NeedsAttention {
                detail: Some("shell".to_string()),
            }
        );
    }

    #[test]
    fn test_resolved_tool_without_terminal_event_is_working() {
        let tail = r#"{"type":"response_item","payload":{"type":"function_call","call_id":"c1","name":"shell"}}
{"type":"response_item","payload":{"type":"function_call_output","call_id":"c1"}}"#;

        let reader = CodexSessionReader {
            codex_home: PathBuf::from("/nonexistent"),
            session_file_cache: HashMap::new(),
        };
        // All tools are resolved, but the rollout still shows active progress.
        assert_eq!(
            reader.determine_status(tail, &ApprovalMode::Suggest),
            CodexSessionStatus::Working
        );
    }

    #[test]
    fn test_progress_event_msg_is_working() {
        let tail = r#"{"type":"event_msg","payload":{"type":"token_count"}}"#;

        let reader = CodexSessionReader {
            codex_home: PathBuf::from("/nonexistent"),
            session_file_cache: HashMap::new(),
        };
        assert_eq!(
            reader.determine_status(tail, &ApprovalMode::Suggest),
            CodexSessionStatus::Working
        );
    }

    #[test]
    fn test_task_complete_without_pending_tools_is_unknown() {
        let tail = r#"{"type":"event_msg","payload":{"type":"task_complete"}}"#;

        let reader = CodexSessionReader {
            codex_home: PathBuf::from("/nonexistent"),
            session_file_cache: HashMap::new(),
        };
        assert_eq!(
            reader.determine_status(tail, &ApprovalMode::Suggest),
            CodexSessionStatus::Unknown
        );
    }

    #[test]
    fn test_is_file_edit_tool() {
        assert!(CodexSessionReader::is_file_edit_tool("file_write"));
        assert!(CodexSessionReader::is_file_edit_tool("file_edit"));
        assert!(CodexSessionReader::is_file_edit_tool("apply_patch"));
        assert!(!CodexSessionReader::is_file_edit_tool("shell"));
        assert!(!CodexSessionReader::is_file_edit_tool("read_file"));
    }

    #[test]
    fn test_read_cwd_from_first_line() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("rollout.jsonl");
        let mut file = fs::File::create(&path).unwrap();
        writeln!(
            file,
            r#"{{"type":"session_meta","payload":{{"cwd":"/Users/test/project","approval_mode":"suggest"}}}}"#
        )
        .unwrap();

        let cwd = CodexSessionReader::read_cwd_from_first_line(&path);
        assert_eq!(cwd, Some("/Users/test/project".to_string()));
    }

    #[test]
    fn test_read_approval_mode() {
        let tmp = TempDir::new().unwrap();
        let reader = CodexSessionReader {
            codex_home: tmp.path().to_path_buf(),
            session_file_cache: HashMap::new(),
        };

        // full-auto
        let path = tmp.path().join("full_auto.jsonl");
        fs::write(
            &path,
            r#"{"type":"session_meta","payload":{"cwd":"/tmp","approval_mode":"full-auto"}}"#,
        )
        .unwrap();
        assert_eq!(reader.read_approval_mode(&path), ApprovalMode::FullAuto);

        // auto-edit
        let path = tmp.path().join("auto_edit.jsonl");
        fs::write(
            &path,
            r#"{"type":"session_meta","payload":{"cwd":"/tmp","approval_mode":"auto-edit"}}"#,
        )
        .unwrap();
        assert_eq!(reader.read_approval_mode(&path), ApprovalMode::AutoEdit);

        // suggest (default)
        let path = tmp.path().join("suggest.jsonl");
        fs::write(
            &path,
            r#"{"type":"session_meta","payload":{"cwd":"/tmp","approval_mode":"suggest"}}"#,
        )
        .unwrap();
        assert_eq!(reader.read_approval_mode(&path), ApprovalMode::Suggest);
    }

    #[test]
    fn test_find_rollout_file_with_date_dirs() {
        let tmp = TempDir::new().unwrap();
        let sessions_dir = tmp
            .path()
            .join("sessions")
            .join("2025")
            .join("01")
            .join("15");
        fs::create_dir_all(&sessions_dir).unwrap();

        let rollout_path = sessions_dir.join("rollout-12345-uuid.jsonl");
        let mut file = fs::File::create(&rollout_path).unwrap();
        writeln!(
            file,
            r#"{{"type":"session_meta","payload":{{"cwd":"/Users/test/project","approval_mode":"suggest"}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"type":"event_msg","payload":{{"type":"turn_complete"}}}}"#
        )
        .unwrap();

        let mut reader = CodexSessionReader {
            codex_home: tmp.path().to_path_buf(),
            session_file_cache: HashMap::new(),
        };

        let found = reader.find_rollout_file(Path::new("/Users/test/project"), None, None);
        assert_eq!(found, Some(rollout_path));
    }

    #[test]
    fn test_find_rollout_file_no_match() {
        let tmp = TempDir::new().unwrap();
        let sessions_dir = tmp
            .path()
            .join("sessions")
            .join("2025")
            .join("01")
            .join("15");
        fs::create_dir_all(&sessions_dir).unwrap();

        let rollout_path = sessions_dir.join("rollout-12345-uuid.jsonl");
        let mut file = fs::File::create(&rollout_path).unwrap();
        writeln!(
            file,
            r#"{{"type":"session_meta","payload":{{"cwd":"/Users/other/project","approval_mode":"suggest"}}}}"#
        )
        .unwrap();

        let mut reader = CodexSessionReader {
            codex_home: tmp.path().to_path_buf(),
            session_file_cache: HashMap::new(),
        };

        let found = reader.find_rollout_file(Path::new("/Users/test/project"), None, None);
        assert!(found.is_none());
    }

    #[test]
    fn test_session_file_cache_hit() {
        let tmp = TempDir::new().unwrap();
        let sessions_dir = tmp
            .path()
            .join("sessions")
            .join("2025")
            .join("01")
            .join("15");
        fs::create_dir_all(&sessions_dir).unwrap();

        let rollout_path = sessions_dir.join("rollout-12345-uuid.jsonl");
        let mut file = fs::File::create(&rollout_path).unwrap();
        writeln!(
            file,
            r#"{{"type":"session_meta","payload":{{"cwd":"/Users/test/project","approval_mode":"suggest"}}}}"#
        )
        .unwrap();

        let mtime = fs::metadata(&rollout_path).unwrap().modified().unwrap();

        let mut reader = CodexSessionReader {
            codex_home: tmp.path().to_path_buf(),
            session_file_cache: HashMap::new(),
        };

        // Pre-populate cache
        reader.session_file_cache.insert(
            (PathBuf::from("/Users/test/project"), None, None),
            (rollout_path.clone(), mtime),
        );

        // Should return cached path without scanning
        let found = reader.find_rollout_file(Path::new("/Users/test/project"), None, None);
        assert_eq!(found, Some(rollout_path));
    }

    #[test]
    fn test_find_rollout_file_filters_out_older_same_cwd_session() {
        let tmp = TempDir::new().unwrap();
        let sessions_dir = tmp
            .path()
            .join("sessions")
            .join("2025")
            .join("01")
            .join("15");
        fs::create_dir_all(&sessions_dir).unwrap();

        let rollout_path = sessions_dir.join("rollout-old-session.jsonl");
        fs::write(
            &rollout_path,
            r#"{"type":"session_meta","payload":{"cwd":"/Users/test/project","approval_mode":"suggest"}}"#,
        )
        .unwrap();

        let mtime = fs::metadata(&rollout_path).unwrap().modified().unwrap();
        let created_after = mtime.checked_add(Duration::from_secs(1)).unwrap();

        let mut reader = CodexSessionReader {
            codex_home: tmp.path().to_path_buf(),
            session_file_cache: HashMap::new(),
        };

        let found =
            reader.find_rollout_file(Path::new("/Users/test/project"), None, Some(created_after));
        assert!(found.is_none());
    }

    #[test]
    fn test_find_rollout_file_prefers_closest_session_timestamp_without_session_id() {
        let tmp = TempDir::new().unwrap();
        let sessions_dir = tmp
            .path()
            .join("sessions")
            .join("2025")
            .join("01")
            .join("15");
        fs::create_dir_all(&sessions_dir).unwrap();

        let first_rollout_path = sessions_dir.join("rollout-111111-first-session.jsonl");
        fs::write(
            &first_rollout_path,
            r#"{"type":"session_meta","payload":{"id":"first-session","timestamp":"2026-03-07T05:41:46.072Z","cwd":"/Users/test/project","approval_mode":"suggest"}}"#,
        )
        .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(20));

        let second_rollout_path = sessions_dir.join("rollout-222222-second-session.jsonl");
        fs::write(
            &second_rollout_path,
            r#"{"type":"session_meta","payload":{"id":"second-session","timestamp":"2026-03-07T05:42:33.256Z","cwd":"/Users/test/project","approval_mode":"suggest"}}"#,
        )
        .unwrap();

        let created_after = CodexSessionReader::session_meta_started_at(&SessionMeta {
            id: None,
            timestamp: Some("2026-03-07T05:41:45.500Z".to_string()),
            cwd: None,
            approval_mode: None,
        })
        .unwrap();

        let mut reader = CodexSessionReader {
            codex_home: tmp.path().to_path_buf(),
            session_file_cache: HashMap::new(),
        };

        let found =
            reader.find_rollout_file(Path::new("/Users/test/project"), None, Some(created_after));
        assert_eq!(found, Some(first_rollout_path));
    }

    #[test]
    fn test_find_rollout_file_prefers_matching_session_id_suffix() {
        let tmp = TempDir::new().unwrap();
        let sessions_dir = tmp
            .path()
            .join("sessions")
            .join("2025")
            .join("01")
            .join("15");
        fs::create_dir_all(&sessions_dir).unwrap();

        let older_rollout_path = sessions_dir.join("rollout-111111-old-session.jsonl");
        fs::write(
            &older_rollout_path,
            r#"{"type":"session_meta","payload":{"cwd":"/Users/test/project","approval_mode":"suggest"}}"#,
        )
        .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(20));

        let matching_rollout_path = sessions_dir.join("rollout-222222-target-session.jsonl");
        fs::write(
            &matching_rollout_path,
            r#"{"type":"session_meta","payload":{"cwd":"/Users/test/project","approval_mode":"suggest"}}"#,
        )
        .unwrap();

        let mut reader = CodexSessionReader {
            codex_home: tmp.path().to_path_buf(),
            session_file_cache: HashMap::new(),
        };

        let found = reader.find_rollout_file(
            Path::new("/Users/test/project"),
            Some("target-session"),
            None,
        );
        assert_eq!(found, Some(matching_rollout_path));
    }

    #[test]
    fn test_read_file_tail() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.jsonl");

        let mut file = fs::File::create(&path).unwrap();
        for i in 0..100 {
            writeln!(file, "line {i}").unwrap();
        }

        let tail = read_file_tail(&path, 50).unwrap();
        assert!(tail.contains("line 99"));
        assert!(!tail.contains("line 0\n"));
    }

    #[test]
    fn test_multiple_pending_calls_only_last_matters() {
        // Two function calls, one resolved, one pending
        let tail = r#"{"type":"response_item","payload":{"type":"function_call","call_id":"c1","name":"read_file"}}
{"type":"response_item","payload":{"type":"function_call_output","call_id":"c1"}}
{"type":"response_item","payload":{"type":"function_call","call_id":"c2","name":"shell"}}"#;

        let reader = CodexSessionReader {
            codex_home: PathBuf::from("/nonexistent"),
            session_file_cache: HashMap::new(),
        };
        assert_eq!(
            reader.determine_status(tail, &ApprovalMode::Suggest),
            CodexSessionStatus::NeedsAttention {
                detail: Some("shell".to_string()),
            }
        );
    }
}
