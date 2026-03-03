//! Claude Code session status reader via JSONL conversation logs.
//!
//! Reads Claude Code's own data files from `~/.claude/projects/` to determine
//! session status with higher fidelity than OSC 133 alone. Inspired by the
//! [c9watch](https://github.com/minchenlee/c9watch) project.
//!
//! This gives direct visibility into Claude's internal state:
//! - Whether it's actively streaming a response
//! - Whether it has a pending tool use awaiting permission
//! - Whether it has finished its turn and is idle

use crate::session_reader_common::{is_file_recent, is_timestamp_recent, read_file_tail};
use crate::CliSessionStatus;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, trace};

/// Status derived from Claude Code's JSONL logs.
pub type ClaudeSessionStatus = CliSessionStatus;

/// Reads Claude Code session data to determine status.
pub struct ClaudeSessionReader {
    /// Path to ~/.claude
    claude_home: PathBuf,
    /// Cached session file selection per working directory.
    ///
    /// Claude can reuse a project directory across many sessions. Keeping the last
    /// identified session file avoids re-anchoring to stale history files between
    /// status polls.
    session_file_cache: HashMap<String, SessionFileHint>,
    /// Cached parsed status per JSONL file path, keyed by (path, mtime).
    /// Avoids re-reading and re-parsing 512KB of JSONL data every second
    /// when the file hasn't been modified.
    status_cache: HashMap<PathBuf, (SystemTime, ClaudeSessionStatus)>,
}

#[derive(Debug, Clone)]
struct SessionFileHint {
    /// Last resolved Claude session file path for the working directory.
    path: PathBuf,
    /// Optional session identifier parsed from the file.
    session_id: Option<String>,
    /// When this cache entry was last validated (to skip re-reading file headers).
    validated_at: Instant,
}

/// Parsed entry from a Claude Code JSONL conversation log.
///
/// Uses `#[serde(deny_unknown_fields)]` is intentionally NOT set; JSONL entries
/// contain many fields we don't need; lenient deserialization skips them.
#[derive(Debug, Deserialize)]
struct JsonlEntry {
    /// Entry type: "assistant", "human", "system", "progress", etc.
    #[serde(rename = "type", default)]
    entry_type: String,
    /// The message content (for assistant/human entries).
    #[serde(default)]
    message: Option<JsonlMessage>,
    /// ISO 8601 timestamp of this entry.
    #[serde(default)]
    timestamp: Option<String>,
    /// Nested payload for newer "progress" envelope format.
    #[serde(default)]
    data: Option<JsonlData>,
}

#[derive(Debug, Deserialize)]
struct JsonlProbe {
    #[serde(rename = "sessionId", default)]
    session_id: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
}

/// `sessions-index.json` entry used by Claude CLI.
#[derive(Debug, Deserialize)]
struct SessionsIndex {
    #[serde(default)]
    entries: Vec<SessionsIndexEntry>,
}

/// One entry in `sessions-index.json`.
#[derive(Debug, Deserialize)]
struct SessionsIndexEntry {
    #[serde(rename = "sessionId", default)]
    _session_id: Option<String>,
    #[serde(rename = "fullPath", default)]
    full_path: Option<String>,
    #[serde(rename = "fileMtime", default)]
    file_mtime: Option<i64>,
    #[serde(rename = "projectPath", default)]
    project_path: Option<String>,
}

/// Data payload in newer Claude JSONL "progress" envelope format.
#[derive(Debug, Deserialize)]
struct JsonlData {
    /// Nested message/event payload.
    #[serde(default)]
    message: Option<JsonlDataMessage>,
}

/// Nested message payload in newer Claude JSONL "progress" format.
#[derive(Debug, Deserialize)]
struct JsonlDataMessage {
    /// Entry type: "assistant", "user", etc.
    #[serde(rename = "type", default)]
    entry_type: String,
    /// Nested message object (assistant/user message payload).
    #[serde(default)]
    message: Option<JsonlMessage>,
    /// Optional nested timestamp.
    #[serde(default)]
    timestamp: Option<String>,
}

/// Message payload within a JSONL entry.
#[derive(Debug, Deserialize, Clone)]
struct JsonlMessage {
    /// Role of the message sender.
    #[serde(default)]
    role: String,
    /// Content blocks (text, tool_use, tool_result, etc.)
    #[serde(default)]
    content: Vec<JsonlContent>,
    /// Stop reason: "end_turn", "tool_use", "max_tokens", etc.
    #[serde(default)]
    stop_reason: Option<String>,
}

/// A content block within a message.
#[derive(Debug, Deserialize, Clone)]
struct JsonlContent {
    /// Content type: "text", "tool_use", "tool_result".
    #[serde(rename = "type", default)]
    content_type: String,
    /// Text content (only for text blocks).
    #[serde(default)]
    text: Option<String>,
    /// Tool name (only for tool_use blocks).
    #[serde(default)]
    name: Option<String>,
    /// Tool use ID (for correlating tool_use with tool_result).
    #[serde(default)]
    id: Option<String>,
    /// Tool use ID reference in tool_result blocks.
    #[serde(default)]
    tool_use_id: Option<String>,
}

impl ClaudeSessionReader {
    /// Create a new reader. Returns `None` if `~/.claude` doesn't exist.
    pub fn new() -> Option<Self> {
        let home = dirs::home_dir()?;
        let claude_home = home.join(".claude");
        if !claude_home.is_dir() {
            debug!("~/.claude not found, Claude session reader disabled");
            return None;
        }
        Some(Self {
            claude_home,
            session_file_cache: HashMap::new(),
            status_cache: HashMap::new(),
        })
    }

    /// Get the status of a Claude Code session by reading its JSONL log.
    ///
    /// `working_dir` is the session's working directory, used to locate the
    /// project-specific JSONL files under `~/.claude/projects/`.
    /// `pid` is the Claude Code child PID, used to find the correct JSONL file
    /// via `lsof` when multiple sessions share the same project directory.
    pub fn get_status(&mut self, working_dir: &Path, pid: Option<u32>) -> ClaudeSessionStatus {
        self.get_status_if_recent(working_dir, pid, Duration::MAX)
            .unwrap_or(ClaudeSessionStatus::Unknown)
    }

    /// Get the status of a Claude Code session if its JSONL file is recent enough.
    ///
    /// This is used to avoid false positives from stale log files on sessions
    /// that are actually still generic shells.
    pub fn get_status_if_recent(
        &mut self,
        working_dir: &Path,
        pid: Option<u32>,
        max_age: Duration,
    ) -> Option<ClaudeSessionStatus> {
        let Some(session_dir) = self.find_session_dir(working_dir) else {
            debug!(?working_dir, "No Claude session dir found");
            return None;
        };

        let Some(jsonl_path) = self.find_jsonl_for_pid(&session_dir, working_dir, pid) else {
            debug!(?session_dir, ?pid, "No Claude JSONL file found");
            return None;
        };

        if !is_file_recent(&jsonl_path, max_age) {
            self.clear_cached_session_file_for_working_dir(working_dir);
            debug!(
                ?jsonl_path,
                ?max_age,
                "Skip stale Claude JSONL file while in GenericShell probe"
            );
            return None;
        }

        self.cache_session_file(working_dir, &jsonl_path);

        // Check if we already have a cached status for this file at the same mtime.
        // Avoids re-reading 512KB and re-parsing JSON every second when the file
        // hasn't been modified (i.e., Claude Code isn't actively writing).
        let current_mtime = jsonl_path.metadata().ok().and_then(|m| m.modified().ok());
        if let Some(mtime) = current_mtime {
            if let Some((cached_mtime, cached_status)) = self.status_cache.get(&jsonl_path) {
                if *cached_mtime == mtime {
                    return Some(cached_status.clone());
                }
            }
        }

        let entries = Self::read_recent_entries_from_path(&jsonl_path, 20);
        if entries.is_empty() {
            debug!(
                ?jsonl_path,
                "JSONL: no entries parsed from Claude session file"
            );
            let status = ClaudeSessionStatus::Unknown;
            if let Some(mtime) = current_mtime {
                self.status_cache
                    .insert(jsonl_path, (mtime, status.clone()));
            }
            return Some(status);
        }

        let status = self.determine_status(&entries);
        if let Some(mtime) = current_mtime {
            self.status_cache
                .insert(jsonl_path, (mtime, status.clone()));
        }
        Some(status)
    }

    fn clear_cached_session_file_for_working_dir(&mut self, working_dir: &Path) {
        self.session_file_cache
            .remove(&Self::path_lookup_key(working_dir));
    }

    fn cache_session_file(&mut self, working_dir: &Path, jsonl_path: &Path) {
        self.session_file_cache.insert(
            Self::path_lookup_key(working_dir),
            SessionFileHint {
                path: jsonl_path.to_path_buf(),
                session_id: Self::read_session_id_from_file(jsonl_path),
                validated_at: Instant::now(),
            },
        );
    }

    /// Find the Claude Code session directory for a given working directory.
    ///
    /// Claude Code stores project data under `~/.claude/projects/<hashed-path>/`.
    /// The hashed path replaces `/` with `-` and strips the leading separator.
    fn find_session_dir(&self, working_dir: &Path) -> Option<PathBuf> {
        let projects_dir = self.claude_home.join("projects");
        if !projects_dir.is_dir() {
            return None;
        }

        let mut candidates = Self::build_project_dir_candidates(working_dir);
        // Most recent first gives better hit rate when multiple variants exist.
        candidates.reverse();

        // First: fast direct lookup for generated candidates.
        for key in &candidates {
            let direct = projects_dir.join(key);
            if direct.is_dir() {
                return Some(direct);
            }
        }

        // Second: tolerate prefixes like "\\?\" normalized to extra dashes.
        for key in &candidates {
            for prefix in ["-", "--", "---", "----"] {
                let candidate = projects_dir.join(format!("{prefix}{key}"));
                if candidate.is_dir() {
                    return Some(candidate);
                }
            }
        }

        // Final fallback: scan and match on normalized suffix, then pick most recent.
        let wanted: Vec<String> = candidates
            .iter()
            .map(|c| c.trim_start_matches('-').to_ascii_lowercase())
            .collect();
        let mut best: Option<(PathBuf, SystemTime)> = None;

        for entry in fs::read_dir(&projects_dir)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
        {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let normalized = name.trim_start_matches('-').to_ascii_lowercase();
            let matched = wanted
                .iter()
                .any(|w| normalized == *w || normalized.ends_with(w));
            if !matched {
                continue;
            }

            let mtime = Self::find_most_recent_jsonl(&path)
                .and_then(|p| p.metadata().ok())
                .and_then(|m| m.modified().ok())
                .unwrap_or(SystemTime::UNIX_EPOCH);

            if best.as_ref().map_or(true, |(_, t)| mtime > *t) {
                best = Some((path, mtime));
            }
        }

        best.map(|(path, _)| path)
    }

    /// Build candidate project directory names used by Claude under ~/.claude/projects.
    fn build_project_dir_candidates(working_dir: &Path) -> Vec<String> {
        fn normalize(path: &str) -> String {
            path.chars()
                .map(|c| match c {
                    '/' | '\\' | ':' | '?' => '-',
                    _ => c,
                })
                .collect()
        }

        fn push_unique(candidates: &mut Vec<String>, seen: &mut HashSet<String>, value: String) {
            if value.is_empty() {
                return;
            }
            let key = value.to_ascii_lowercase();
            if seen.insert(key) {
                candidates.push(value);
            }
        }

        let mut out = Vec::new();
        let mut seen = HashSet::new();

        let raw = working_dir.to_string_lossy().to_string();
        push_unique(&mut out, &mut seen, normalize(&raw));

        #[cfg(windows)]
        {
            if let Some(stripped) = raw.strip_prefix(r"\\?\") {
                push_unique(&mut out, &mut seen, normalize(stripped));
            }
            push_unique(&mut out, &mut seen, normalize(&raw.replace('\\', "/")));
        }

        if let Ok(canonical) = working_dir.canonicalize() {
            let canonical = canonical.to_string_lossy().to_string();
            push_unique(&mut out, &mut seen, normalize(&canonical));
            #[cfg(windows)]
            {
                if let Some(stripped) = canonical.strip_prefix(r"\\?\") {
                    push_unique(&mut out, &mut seen, normalize(stripped));
                }
            }
        }

        out
    }

    /// Normalize a raw JSONL entry into assistant/user message form used by status logic.
    fn normalize_entry(mut entry: JsonlEntry) -> Option<JsonlEntry> {
        // Newer envelope: {"type":"progress","data":{"message":{"type":"assistant|user","message":{...}}}}
        if entry.entry_type == "progress" {
            if let Some(data_msg) = entry.data.take().and_then(|d| d.message) {
                return Some(JsonlEntry {
                    entry_type: data_msg.entry_type,
                    message: data_msg.message,
                    timestamp: data_msg.timestamp.or(entry.timestamp),
                    data: None,
                });
            }
        }
        Some(entry)
    }

    /// Read recent entries from the JSONL file for this session.
    ///
    /// Uses PID-based matching (via `lsof`) when available, falling back to
    /// most-recently-modified file. Uses seek-from-end for efficiency.
    #[cfg(test)]
    fn read_recent_entries(
        &mut self,
        session_dir: &Path,
        max_entries: usize,
        pid: Option<u32>,
    ) -> Vec<JsonlEntry> {
        let Some(jsonl_path) = self.find_jsonl_for_pid(session_dir, session_dir, pid) else {
            return Vec::new();
        };
        Self::read_recent_entries_from_path(&jsonl_path, max_entries)
    }

    /// Read recent entries from a specific Claude JSONL file.
    fn read_recent_entries_from_path(jsonl_path: &Path, max_entries: usize) -> Vec<JsonlEntry> {
        let Some(tail) = read_file_tail(jsonl_path, 524_288) else {
            return Vec::new();
        };

        // Parse JSONL lines (last N entries).
        // Only keep assistant and human entries --progress/system/queue-operation
        // entries are never used by determine_status() and can flood the buffer
        // (Claude Code emits many large progress entries during streaming).
        let mut entries = Vec::new();
        for line in tail.lines().rev() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<JsonlEntry>(line) {
                Ok(entry) => {
                    let Some(entry) = Self::normalize_entry(entry) else {
                        continue;
                    };

                    let entry_type = entry.entry_type.as_str();
                    let role = entry
                        .message
                        .as_ref()
                        .map(|m| m.role.as_str())
                        .unwrap_or_default();
                    let is_assistant_or_user = matches!(entry_type, "assistant" | "human" | "user")
                        || matches!(role, "assistant" | "user");
                    if !is_assistant_or_user {
                        continue;
                    }

                    entries.push(entry);
                    if entries.len() >= max_entries {
                        break;
                    }
                }
                Err(e) => {
                    trace!(?e, "Skipping unparseable JSONL line");
                }
            }
        }

        // Reverse so entries are in chronological order
        entries.reverse();
        debug!(
            tail_bytes = tail.len(),
            parsed_entries = entries.len(),
            "JSONL: read_recent_entries"
        );
        entries
    }

    /// Find the JSONL file for the active Claude session.
    ///
    /// Resolution strategy:
    /// 1. Return cached session file if it still matches.
    /// 2. PID-based file-handle match if possible.
    /// 3. Project `sessions-index.json` mapping.
    /// 4. Working-directory scan.
    /// 5. Most-recently-modified `.jsonl` fallback.
    fn find_jsonl_for_pid(
        &mut self,
        dir: &Path,
        working_dir: &Path,
        pid: Option<u32>,
    ) -> Option<PathBuf> {
        let working_dir_key = Self::path_lookup_key(working_dir);

        if let Some(cached) = self.session_file_cache.get(&working_dir_key).cloned() {
            // Skip expensive file-header re-read and directory scan if validated recently
            let recently_validated = cached.validated_at.elapsed() < Duration::from_secs(5);
            let still_valid = recently_validated
                || (self.is_working_dir_session_match(
                    &cached.path,
                    working_dir,
                    cached.session_id.as_deref(),
                ) && !Self::has_newer_jsonl(dir, &cached.path));

            if still_valid {
                if !recently_validated {
                    // Refresh the validated_at timestamp
                    if let Some(entry) = self.session_file_cache.get_mut(&working_dir_key) {
                        entry.validated_at = Instant::now();
                    }
                }
                debug!(
                    ?working_dir_key,
                    ?cached.path,
                    recently_validated,
                    "Using cached Claude session file by working directory"
                );
                return Some(cached.path);
            }
            self.session_file_cache.remove(&working_dir_key);
        }

        if let Some(pid) = pid {
            let candidates: Vec<PathBuf> = fs::read_dir(dir)
                .ok()
                .into_iter()
                .flatten()
                .flatten()
                .filter(|entry| entry.path().extension().and_then(|x| x.to_str()) == Some("jsonl"))
                .map(|entry| entry.path())
                .collect();

            if !candidates.is_empty() {
                if let Some(match_path) =
                    codirigent_detector::find_file_opened_by_pid(&candidates, pid)
                {
                    debug!(pid, ?match_path, "PID-based JSONL match found");
                    self.cache_session_file(working_dir, &match_path);
                    return Some(match_path);
                }
                debug!(
                    pid,
                    "No PID-based JSONL match, falling back to directory scan"
                );
            }
        }

        if let Some(index_path) = Self::find_jsonl_from_index(dir, working_dir) {
            self.cache_session_file(working_dir, &index_path);
            return Some(index_path);
        }

        // Try the most-recently-modified file first — it's almost always the
        // active session and only requires mtime comparisons (no file reading).
        // This avoids the expensive find_jsonl_by_working_dir which reads
        // headers of every JSONL file in directories with 100+ files.
        if let Some(session_file) = Self::find_most_recent_jsonl(dir) {
            self.cache_session_file(working_dir, &session_file);
            return Some(session_file);
        }

        None
    }

    /// Validate a candidate session file for the provided working directory.
    /// If an expected session_id is provided, require that id to match.
    fn is_working_dir_session_match(
        &self,
        path: &Path,
        working_dir: &Path,
        expected_session_id: Option<&str>,
    ) -> bool {
        let working_dir_key = Self::path_lookup_key(working_dir);
        if working_dir_key.is_empty() {
            return false;
        }

        let Some(probe) = Self::read_session_probe(path) else {
            return false;
        };

        if !Self::matches_working_dir_by_probe(&probe, &working_dir_key) {
            return false;
        }

        if let Some(expected_session_id) = expected_session_id {
            return Self::extract_session_id(&probe, path).as_deref() == Some(expected_session_id);
        }

        true
    }

    /// Read sessionId from probe/path so cache can track continuity across polls.
    fn read_session_id(path: &Path) -> Option<String> {
        let Some(probe) = Self::read_session_probe(path) else {
            return path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(std::string::ToString::to_string);
        };

        Self::extract_session_id(&probe, path)
    }

    fn read_session_id_from_file(path: &Path) -> Option<String> {
        Self::read_session_id(path)
    }

    fn extract_session_id(probe: &JsonlProbe, path: &Path) -> Option<String> {
        probe.session_id.clone().or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(std::string::ToString::to_string)
        })
    }

    /// Look up the current session file using Claude's `sessions-index.json`.
    fn find_jsonl_from_index(dir: &Path, working_dir: &Path) -> Option<PathBuf> {
        let index_path = dir.join("sessions-index.json");
        let index_text = fs::read_to_string(index_path).ok()?;
        let index: SessionsIndex = serde_json::from_str(&index_text).ok()?;

        let working_dir_key = Self::path_lookup_key(working_dir);
        if working_dir_key.is_empty() {
            return None;
        }

        let mut best: Option<(i64, PathBuf)> = None;
        for entry in index.entries {
            if !entry
                .project_path
                .as_ref()
                .is_some_and(|project_path| Self::path_lookup_key(project_path) == working_dir_key)
            {
                continue;
            }

            let Some(full_path) = entry.full_path else {
                continue;
            };
            let path = PathBuf::from(full_path);
            if !path.is_file() {
                continue;
            }

            let mtime = entry
                .file_mtime
                .or_else(|| {
                    path.metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())
                })
                .unwrap_or(0);

            if best
                .as_ref()
                .map_or(true, |(best_mtime, _)| mtime >= *best_mtime)
            {
                best = Some((mtime, path));
            }
        }

        let best_path = best.map(|(_, path)| path);
        if best_path.is_some() {
            debug!(
                ?working_dir,
                ?best_path,
                "Using sessions-index session mapping"
            );
        }
        best_path
    }

    fn matches_working_dir_by_probe(probe: &JsonlProbe, working_dir_key: &str) -> bool {
        probe
            .cwd
            .as_ref()
            .is_some_and(|cwd| Self::path_lookup_key(cwd) == working_dir_key)
    }

    fn read_session_probe(path: &Path) -> Option<JsonlProbe> {
        use std::io::{BufRead, BufReader};

        let file = fs::File::open(path).ok()?;
        let reader = BufReader::new(file);
        for line in reader.lines().take(25) {
            let line = line.ok()?;
            if let Ok(probe) = serde_json::from_str::<JsonlProbe>(&line) {
                if probe.cwd.is_some() || probe.session_id.is_some() {
                    return Some(probe);
                }
            }
        }
        None
    }

    fn path_lookup_key<P: AsRef<Path>>(path: P) -> String {
        let path = path.as_ref();
        let normalized = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .to_ascii_lowercase()
            .replace('\\', "/")
            .trim_end_matches('/')
            .trim_end_matches('\\')
            .to_string();
        normalized
    }

    /// Check if a new session file has appeared in `dir` since `reference` was cached.
    ///
    /// Uses the directory's own mtime as a cheap signal: when a new file is
    /// created the OS updates the directory mtime, so a single metadata() call
    /// on the directory replaces scanning hundreds of individual files.
    fn has_newer_jsonl(dir: &Path, reference: &Path) -> bool {
        let ref_mtime = match reference.metadata().ok().and_then(|m| m.modified().ok()) {
            Some(t) => t,
            None => return true,
        };

        // Directory mtime updates when files are created/deleted/renamed.
        // If the directory hasn't changed since the reference file was written,
        // no new session file could have appeared.
        let dir_mtime = match dir.metadata().ok().and_then(|m| m.modified().ok()) {
            Some(t) => t,
            None => return false,
        };

        dir_mtime > ref_mtime
    }

    /// Find the most recently modified .jsonl file in a directory.
    fn find_most_recent_jsonl(dir: &Path) -> Option<PathBuf> {
        let entries = fs::read_dir(dir).ok()?;
        let mut best: Option<(PathBuf, SystemTime)> = None;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                if let Ok(meta) = path.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        if best.as_ref().map_or(true, |(_, t)| mtime > *t) {
                            best = Some((path, mtime));
                        }
                    }
                }
            }
        }

        best.map(|(p, _)| p)
    }

    /// Check if the last text content block in a message ends with a question mark.
    /// Returns `Some("question")` if yes, `None` otherwise.
    fn last_text_ends_with_question(msg: &JsonlMessage) -> Option<String> {
        let last_text = msg
            .content
            .iter()
            .rev()
            .find(|c| c.content_type == "text")
            .and_then(|c| c.text.as_deref())?;

        let last_line = last_text.trim_end().lines().last()?.trim();
        if last_line.ends_with('?') {
            Some("question".to_string())
        } else {
            None
        }
    }

    /// Core status determination algorithm.
    ///
    /// Uses JSONL as the sole source of truth:
    /// 1. Find the last assistant message
    /// 2. If it has a tool_use with no tool_result --NeedsAttention
    ///    (if it's actually auto-approved, the result appears within ~1s
    ///    and the next poll will update the status)
    /// 3. If stop_reason is "end_turn" --NeedsAttention
    ///    (if the last text ends with `?`, detail is set to "question")
    /// 4. Otherwise --Unknown (fall through to other detectors)
    fn determine_status(&self, entries: &[JsonlEntry]) -> ClaudeSessionStatus {
        debug!(
            entry_count = entries.len(),
            entry_types = ?entries.iter().map(|e| e.entry_type.as_str()).collect::<Vec<_>>(),
            "JSONL determine_status input"
        );

        // Walk entries in reverse to find the last meaningful entry
        let mut last_assistant: Option<&JsonlEntry> = None;
        let mut last_assistant_idx: usize = 0;
        let mut tool_results: Vec<String> = Vec::new();

        for (i, entry) in entries.iter().enumerate().rev() {
            // Collect tool_result IDs from human/tool entries
            if let Some(ref msg) = entry.message {
                for content in &msg.content {
                    if content.content_type == "tool_result" {
                        if let Some(ref id) = content.tool_use_id {
                            tool_results.push(id.clone());
                        }
                    }
                }
            }

            // Find the last assistant message
            if last_assistant.is_none()
                && (entry.entry_type == "assistant"
                    || entry
                        .message
                        .as_ref()
                        .is_some_and(|m| m.role == "assistant"))
            {
                last_assistant = Some(entry);
                last_assistant_idx = i;
            }
        }

        // Context-aware refinement: if the last 2 entries are both assistant messages
        // and the most recent has stop_reason + no pending tools --NeedsAttention.
        // This catches edge cases where the main heuristic would return Working.
        if entries.len() >= 2 {
            let last_two: Vec<_> = entries.iter().rev().take(2).collect();
            let both_assistant = last_two.iter().all(|e| {
                e.entry_type == "assistant"
                    || e.message.as_ref().is_some_and(|m| m.role == "assistant")
            });
            if both_assistant {
                if let Some(msg) = &last_two[0].message {
                    let has_tools = msg.content.iter().any(|c| c.content_type == "tool_use");
                    if !has_tools && msg.stop_reason.is_some() {
                        let detail = Self::last_text_ends_with_question(msg);
                        debug!("JSONL: consecutive assistant messages with stop_reason --NeedsAttention");
                        return ClaudeSessionStatus::NeedsAttention { detail };
                    }
                }
            }
        }

        let Some(assistant) = last_assistant else {
            return ClaudeSessionStatus::Unknown;
        };
        let Some(msg) = &assistant.message else {
            return ClaudeSessionStatus::Unknown;
        };

        // Check for pending tool_use (no corresponding tool_result).
        // Only report NeedsAttention when stop_reason is "tool_use" --this means
        // the API has finished and the tool is blocked on permission.
        // When stop_reason is None, the model is still streaming or the tool is
        // auto-approved and executing --report Working.
        // Check for pending tool_use (no corresponding tool_result).
        // Treat tool_use as active work by default, and only report NeedsAttention
        // when we can infer an explicit permission-style prompt.
        let is_tool_stop = msg.stop_reason.as_deref() == Some("tool_use");

        for content in &msg.content {
            if content.content_type == "tool_use" {
                let tool_id = content.id.as_deref().unwrap_or("");
                let has_result = tool_results.iter().any(|r| r == tool_id);

                if !has_result {
                    let tool_name = content.name.as_deref().unwrap_or("unknown");

                    if is_tool_stop {
                        // stop_reason="tool_use" means the API turn ended with a
                        // tool call that needs permission approval.
                        // Give a brief grace period for auto-approved tools.
                        if let Some(ts) = assistant.timestamp.as_deref() {
                            if is_timestamp_recent(ts, 3) == Some(true) {
                                debug!(
                                    ?tool_name,
                                    "JSONL: pending tool_use (stop=tool_use) < 3s --Working (grace period)"
                                );
                                return ClaudeSessionStatus::Working;
                            }
                        }

                        debug!(
                            ?tool_name,
                            "JSONL: pending tool_use (stop=tool_use) --3s --NeedsAttention"
                        );
                        return ClaudeSessionStatus::NeedsAttention {
                            detail: Some(tool_name.to_string()),
                        };
                    } else {
                        // stop_reason is None or something else --tool is still
                        // executing or model is still streaming.
                        debug!(
                            ?tool_name,
                            stop_reason=?msg.stop_reason,
                            "JSONL: pending tool_use (not tool_stop) --Working"
                        );
                        return ClaudeSessionStatus::Working;
                    }
                }
            }
        }

        // Check stop_reason
        if let Some(ref stop_reason) = msg.stop_reason {
            if stop_reason == "end_turn" {
                let detail = Self::last_text_ends_with_question(msg);
                return ClaudeSessionStatus::NeedsAttention { detail };
            }
        }

        // Check if there's a human entry after the last assistant (tool results
        // sent back --Claude is processing the next turn).
        let has_human_after_assistant = entries[last_assistant_idx + 1..].iter().any(|e| {
            matches!(e.entry_type.as_str(), "human" | "user")
                || e.message.as_ref().is_some_and(|m| m.role == "user")
        });

        if has_human_after_assistant {
            // Tool results were sent back; Claude should be generating.
            // Use the timestamp of the last entry to gauge recency.
            if let Some(ts) = entries.last().and_then(|e| e.timestamp.as_deref()) {
                return match is_timestamp_recent(ts, 15) {
                    Some(true) => ClaudeSessionStatus::Working,
                    Some(false) => ClaudeSessionStatus::NeedsAttention { detail: None },
                    None => ClaudeSessionStatus::Unknown,
                };
            }
            // No timestamp available --assume working (human just sent input)
            return ClaudeSessionStatus::Working;
        }

        // stop_reason is null, no pending tools, no human after assistant.
        // Claude may still be streaming or may have finished without "end_turn".
        // Use the assistant entry's timestamp to decide.
        if let Some(ts) = assistant.timestamp.as_deref() {
            return match is_timestamp_recent(ts, 10) {
                Some(true) => ClaudeSessionStatus::Working,
                Some(false) => ClaudeSessionStatus::NeedsAttention { detail: None },
                None => ClaudeSessionStatus::Unknown,
            };
        }

        ClaudeSessionStatus::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn make_assistant_entry(content: Vec<JsonlContent>, stop_reason: Option<&str>) -> JsonlEntry {
        JsonlEntry {
            entry_type: "assistant".to_string(),
            message: Some(JsonlMessage {
                role: "assistant".to_string(),
                content,
                stop_reason: stop_reason.map(|s| s.to_string()),
            }),
            timestamp: None,
            data: None,
        }
    }

    fn make_assistant_entry_ts(
        content: Vec<JsonlContent>,
        stop_reason: Option<&str>,
        timestamp: &str,
    ) -> JsonlEntry {
        JsonlEntry {
            entry_type: "assistant".to_string(),
            message: Some(JsonlMessage {
                role: "assistant".to_string(),
                content,
                stop_reason: stop_reason.map(|s| s.to_string()),
            }),
            timestamp: Some(timestamp.to_string()),
            data: None,
        }
    }

    fn make_text(text: &str) -> JsonlContent {
        JsonlContent {
            content_type: "text".to_string(),
            text: Some(text.to_string()),
            name: None,
            id: None,
            tool_use_id: None,
        }
    }

    fn make_tool_use(name: &str, id: &str) -> JsonlContent {
        JsonlContent {
            content_type: "tool_use".to_string(),
            text: None,
            name: Some(name.to_string()),
            id: Some(id.to_string()),
            tool_use_id: None,
        }
    }

    fn make_tool_result(tool_use_id: &str) -> JsonlContent {
        JsonlContent {
            content_type: "tool_result".to_string(),
            text: None,
            name: None,
            id: None,
            tool_use_id: Some(tool_use_id.to_string()),
        }
    }

    fn make_human_entry(content: Vec<JsonlContent>) -> JsonlEntry {
        JsonlEntry {
            entry_type: "human".to_string(),
            message: Some(JsonlMessage {
                role: "user".to_string(),
                content,
                stop_reason: None,
            }),
            timestamp: None,
            data: None,
        }
    }

    fn make_human_entry_ts(content: Vec<JsonlContent>, timestamp: &str) -> JsonlEntry {
        JsonlEntry {
            entry_type: "human".to_string(),
            message: Some(JsonlMessage {
                role: "user".to_string(),
                content,
                stop_reason: None,
            }),
            timestamp: Some(timestamp.to_string()),
            data: None,
        }
    }

    fn test_reader() -> ClaudeSessionReader {
        ClaudeSessionReader {
            claude_home: PathBuf::from("/nonexistent"),
            session_file_cache: std::collections::HashMap::new(),
            status_cache: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_get_status_if_recent_uses_new_file_age_gate() {
        let tmp = TempDir::new().unwrap();
        let tmp_home = tmp.path();
        let projects_dir = tmp_home.join("projects");
        std::fs::create_dir_all(&projects_dir).unwrap();

        let working_dir = tmp_home.join("work");
        std::fs::create_dir_all(&working_dir).unwrap();
        let session_dir = projects_dir.join("-Users-test-project");
        std::fs::create_dir_all(&session_dir).unwrap();

        let session_jsonl = session_dir.join("session.jsonl");
        std::fs::write(
            &session_jsonl,
            r#"{"type":"assistant","message":{"role":"assistant","content":[],"stop_reason":"end_turn"}}"#,
        )
        .unwrap();

        let mut reader = ClaudeSessionReader {
            claude_home: tmp_home.to_path_buf(),
            session_file_cache: std::collections::HashMap::new(),
            status_cache: std::collections::HashMap::new(),
        };

        let status_fresh = reader.get_status_if_recent(
            Path::new("/Users/test/project"),
            None,
            std::time::Duration::from_secs(10),
        );
        assert_eq!(
            status_fresh,
            Some(ClaudeSessionStatus::NeedsAttention { detail: None })
        );

        std::thread::sleep(std::time::Duration::from_millis(10));
        let status_stale = reader.get_status_if_recent(
            Path::new("/Users/test/project"),
            None,
            std::time::Duration::from_nanos(1),
        );
        assert!(status_stale.is_none());
    }

    #[test]
    fn test_end_turn_returns_waiting() {
        let entries = vec![make_assistant_entry(vec![], Some("end_turn"))];
        let reader = test_reader();
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::NeedsAttention { detail: None }
        );
    }

    #[test]
    fn test_pending_tool_use_with_tool_stop_needs_attention() {
        // Pending tool_use with stop_reason="tool_use" = NeedsAttention (permission blocked).
        let entries = vec![make_assistant_entry(
            vec![make_tool_use("Bash", "tu_1")],
            Some("tool_use"),
        )];
        let reader = test_reader();
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::NeedsAttention {
                detail: Some("Bash".to_string()),
            }
        );
    }

    #[test]
    fn test_pending_tool_use_without_tool_stop_is_working() {
        // Pending tool_use with stop_reason=None = Working (tool still executing)
        let entries = vec![make_assistant_entry(
            vec![make_tool_use("Bash", "tu_1")],
            None,
        )];
        let reader = test_reader();
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::Working
        );
    }

    #[test]
    fn test_pending_read_tool_without_stop_is_working() {
        // stop_reason=None means tool is auto-approved and executing
        let entries = vec![make_assistant_entry(
            vec![make_tool_use("Read", "tu_1")],
            None,
        )];
        let reader = test_reader();
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::Working
        );
    }

    #[test]
    fn test_tool_use_with_result_is_not_pending() {
        let entries = vec![
            make_assistant_entry(vec![make_tool_use("Bash", "tu_3")], None),
            make_human_entry(vec![make_tool_result("tu_3")]),
        ];
        let reader = test_reader();
        // The tool_use has a corresponding result, so it's not pending.
        // Human entry after assistant with no timestamp --Working (assumes processing)
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::Working
        );
    }

    #[test]
    fn test_empty_entries_returns_unknown() {
        let reader = test_reader();
        assert_eq!(reader.determine_status(&[]), ClaudeSessionStatus::Unknown);
    }

    #[test]
    fn test_find_session_dir() {
        let tmp = TempDir::new().unwrap();
        let projects_dir = tmp.path().join("projects");
        fs::create_dir_all(&projects_dir).unwrap();

        let reader = ClaudeSessionReader {
            claude_home: tmp.path().to_path_buf(),
            session_file_cache: std::collections::HashMap::new(),
            status_cache: std::collections::HashMap::new(),
        };

        // No matching dir
        assert!(reader
            .find_session_dir(Path::new("/Users/foo/project"))
            .is_none());

        // Create matching dir
        let session_dir = projects_dir.join("-Users-foo-project");
        fs::create_dir_all(&session_dir).unwrap();

        let found = reader.find_session_dir(Path::new("/Users/foo/project"));
        assert_eq!(found, Some(session_dir));
    }

    #[test]
    fn test_find_session_dir_windows_style_path() {
        let tmp = TempDir::new().unwrap();
        let projects_dir = tmp.path().join("projects");
        fs::create_dir_all(&projects_dir).unwrap();

        let reader = ClaudeSessionReader {
            claude_home: tmp.path().to_path_buf(),
            session_file_cache: std::collections::HashMap::new(),
            status_cache: std::collections::HashMap::new(),
        };

        let session_dir = projects_dir.join("C--Users-foo-project");
        fs::create_dir_all(&session_dir).unwrap();

        let found = reader.find_session_dir(Path::new(r"C:\Users\foo\project"));
        assert_eq!(found, Some(session_dir));
    }

    #[test]
    fn test_find_session_dir_windows_extended_prefix_variant() {
        let tmp = TempDir::new().unwrap();
        let projects_dir = tmp.path().join("projects");
        fs::create_dir_all(&projects_dir).unwrap();

        let reader = ClaudeSessionReader {
            claude_home: tmp.path().to_path_buf(),
            session_file_cache: std::collections::HashMap::new(),
            status_cache: std::collections::HashMap::new(),
        };

        let session_dir = projects_dir.join("----C--Users-foo-project");
        fs::create_dir_all(&session_dir).unwrap();

        let found = reader.find_session_dir(Path::new(r"C:\Users\foo\project"));
        assert_eq!(found, Some(session_dir));
    }

    #[test]
    fn test_read_recent_entries_from_jsonl() {
        let tmp = TempDir::new().unwrap();
        let jsonl_path = tmp.path().join("conversation.jsonl");

        // Write some JSONL entries
        let mut file = fs::File::create(&jsonl_path).unwrap();
        writeln!(
            file,
            r#"{{"type":"assistant","message":{{"role":"assistant","content":[],"stop_reason":"end_turn"}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"type":"human","message":{{"role":"user","content":[]}}}}"#
        )
        .unwrap();

        let mut reader = test_reader();

        let entries = reader.read_recent_entries(tmp.path(), 10, None);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].entry_type, "assistant");
        assert_eq!(entries[1].entry_type, "human");
    }

    #[test]
    fn test_read_recent_entries_normalizes_progress_envelope() {
        let tmp = TempDir::new().unwrap();
        let jsonl_path = tmp.path().join("conversation.jsonl");

        let mut file = fs::File::create(&jsonl_path).unwrap();
        writeln!(
            file,
            r#"{{"type":"progress","timestamp":"2026-02-14T00:00:00Z","data":{{"message":{{"type":"assistant","timestamp":"2026-02-14T00:00:00Z","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"tu_1","name":"Read"}}],"stop_reason":null}}}}}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"type":"progress","timestamp":"2026-02-14T00:00:01Z","data":{{"message":{{"type":"user","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"tu_1"}}]}}}}}}}}"#
        )
        .unwrap();

        let mut reader = test_reader();
        let entries = reader.read_recent_entries(tmp.path(), 10, None);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].entry_type, "assistant");
        assert_eq!(entries[1].entry_type, "user");
    }

    #[test]
    fn test_read_recent_entries_filters_progress() {
        let tmp = TempDir::new().unwrap();
        let jsonl_path = tmp.path().join("conversation.jsonl");

        let mut file = fs::File::create(&jsonl_path).unwrap();
        // Write assistant entry, then many progress entries, then human entry
        writeln!(
            file,
            r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"tool_use","name":"Grep","id":"tu_1"}}]}}}}"#
        )
        .unwrap();
        for _ in 0..50 {
            writeln!(
                file,
                r#"{{"type":"progress","timestamp":"2026-01-01T00:00:00Z"}}"#
            )
            .unwrap();
        }
        writeln!(
            file,
            r#"{{"type":"human","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"tu_1"}}]}}}}"#
        )
        .unwrap();

        let mut reader = test_reader();
        let entries = reader.read_recent_entries(tmp.path(), 10, None);
        // Progress entries should be filtered out --only assistant + human remain
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].entry_type, "assistant");
        assert_eq!(entries[1].entry_type, "human");
    }

    #[test]
    fn test_read_file_tail() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.jsonl");

        let mut file = fs::File::create(&path).unwrap();
        for i in 0..100 {
            writeln!(file, "line {i}").unwrap();
        }

        // Read tail should get the last lines
        let tail = read_file_tail(&path, 50).unwrap();
        assert!(tail.contains("line 99"));
        // Should not contain very early lines
        assert!(!tail.contains("line 0\n"));
    }

    #[test]
    fn test_find_most_recent_jsonl() {
        let tmp = TempDir::new().unwrap();

        // Create two jsonl files
        let path1 = tmp.path().join("old.jsonl");
        fs::write(&path1, "old").unwrap();

        // Ensure different mtime
        std::thread::sleep(std::time::Duration::from_millis(50));

        let path2 = tmp.path().join("new.jsonl");
        fs::write(&path2, "new").unwrap();

        let found = ClaudeSessionReader::find_most_recent_jsonl(tmp.path()).unwrap();
        assert_eq!(found, path2);
    }

    #[test]
    fn test_find_jsonl_for_pid_ignores_stale_cached_session_id() {
        let tmp = TempDir::new().unwrap();
        let session_dir = tmp.path().join("session-dir");
        let working_dir = tmp.path().join("project");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::create_dir_all(&working_dir).unwrap();

        let working_dir_str = working_dir.to_string_lossy();

        let stale_path = session_dir.join("7e813124-ce33-48f9-9205-c4e876b96085.jsonl");
        std::fs::write(
            &stale_path,
            format!(
                r#"{{"type":"assistant","sessionId":"7e813124-ce33-48f9-9205-c4e876b96085","cwd":"{}","timestamp":"2026-02-14T00:00:00Z","message":{{"role":"assistant","content":[],"stop_reason":"end_turn"}}}}"#,
                working_dir_str
            ),
        )
        .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(20));

        let fresh_path = session_dir.join("2e2e2ca9-7a65-4618-855e-5b2a3aaf6623.jsonl");
        std::fs::write(
            &fresh_path,
            format!(
                r#"{{"type":"assistant","sessionId":"2e2e2ca9-7a65-4618-855e-5b2a3aaf6623","cwd":"{}","timestamp":"2026-02-14T00:00:01Z","message":{{"role":"assistant","content":[],"stop_reason":"end_turn"}}}}"#,
                working_dir_str
            ),
        )
        .unwrap();

        let mut reader = ClaudeSessionReader {
            claude_home: PathBuf::from("/nonexistent"),
            session_file_cache: std::collections::HashMap::new(),
            status_cache: std::collections::HashMap::new(),
        };
        reader.session_file_cache.insert(
            ClaudeSessionReader::path_lookup_key(&working_dir),
            SessionFileHint {
                path: stale_path.clone(),
                session_id: Some("7e813124-ce33-48f9-9205-c4e876b96085".to_string()),
                validated_at: Instant::now() - Duration::from_secs(10),
            },
        );

        let selected = reader
            .find_jsonl_for_pid(&session_dir, &working_dir, None)
            .expect("A jsonl file should be selected");
        assert_eq!(selected, fresh_path);
    }

    #[test]
    fn test_null_stop_reason_recent_timestamp_returns_working() {
        // Assistant with no stop_reason but a recent timestamp --Working (still streaming)
        let recent = chrono::Utc::now().to_rfc3339();
        let entries = vec![make_assistant_entry_ts(vec![], None, &recent)];
        let reader = test_reader();
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::Working
        );
    }

    #[test]
    fn test_null_stop_reason_old_timestamp_returns_waiting() {
        // Assistant with no stop_reason and an old timestamp --NeedsAttention (done)
        let old = (chrono::Utc::now() - chrono::Duration::seconds(30)).to_rfc3339();
        let entries = vec![make_assistant_entry_ts(vec![], None, &old)];
        let reader = test_reader();
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::NeedsAttention { detail: None }
        );
    }

    #[test]
    fn test_human_after_assistant_recent_returns_working() {
        // Tool result sent back recently --Claude is processing
        let recent = chrono::Utc::now().to_rfc3339();
        let entries = vec![
            make_assistant_entry(vec![make_tool_use("Bash", "tu_5")], None),
            make_human_entry_ts(vec![make_tool_result("tu_5")], &recent),
        ];
        let reader = test_reader();
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::Working
        );
    }

    #[test]
    fn test_human_after_assistant_old_returns_waiting() {
        // Tool result sent back long ago --Claude is done
        let old = (chrono::Utc::now() - chrono::Duration::seconds(30)).to_rfc3339();
        let entries = vec![
            make_assistant_entry(vec![make_tool_use("Bash", "tu_6")], None),
            make_human_entry_ts(vec![make_tool_result("tu_6")], &old),
        ];
        let reader = test_reader();
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::NeedsAttention { detail: None }
        );
    }

    #[test]
    fn test_is_timestamp_recent() {
        let now = chrono::Utc::now().to_rfc3339();
        assert_eq!(is_timestamp_recent(&now, 10), Some(true));

        let old = (chrono::Utc::now() - chrono::Duration::seconds(60)).to_rfc3339();
        assert_eq!(is_timestamp_recent(&old, 10), Some(false));

        assert_eq!(is_timestamp_recent("not-a-timestamp", 10), None);
    }

    #[test]
    fn test_pending_tool_recent_timestamp_returns_working() {
        // Pending tool_use with recent timestamp --Working (grace period)
        let recent = chrono::Utc::now().to_rfc3339();
        let entries = vec![make_assistant_entry_ts(
            vec![make_tool_use("Bash", "tu_gp1")],
            None,
            &recent,
        )];
        let reader = test_reader();
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::Working
        );
    }

    #[test]
    fn test_pending_tool_old_timestamp_no_stop_returns_working() {
        // Pending tool_use with old timestamp but stop_reason=None --Working
        // (tool is still executing, just taking a while)
        let old = (chrono::Utc::now() - chrono::Duration::seconds(10)).to_rfc3339();
        let entries = vec![make_assistant_entry_ts(
            vec![make_tool_use("Bash", "tu_gp2")],
            None,
            &old,
        )];
        let reader = test_reader();
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::Working
        );
    }

    #[test]
    fn test_pending_tool_old_timestamp_with_tool_stop_returns_needs_attention() {
        // Pending tool_use with old timestamp and stop_reason="tool_use" stays NeedsAttention.
        let old = (chrono::Utc::now() - chrono::Duration::seconds(10)).to_rfc3339();
        let entries = vec![make_assistant_entry_ts(
            vec![make_tool_use("Bash", "tu_gp2")],
            Some("tool_use"),
            &old,
        )];
        let reader = test_reader();
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::NeedsAttention {
                detail: Some("Bash".to_string()),
            }
        );
    }

    #[test]
    fn test_consecutive_assistant_with_stop_reason_returns_waiting() {
        // Two assistant messages, latest has stop_reason and no tools --NeedsAttention
        let entries = vec![
            make_assistant_entry(vec![make_tool_use("Read", "tu_ca1")], None),
            make_assistant_entry(vec![], Some("end_turn")),
        ];
        let reader = test_reader();
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::NeedsAttention { detail: None }
        );
    }

    #[test]
    fn test_consecutive_assistant_with_tools_does_not_override() {
        // Two assistant messages, but latest has tool_use --normal logic applies
        let entries = vec![
            make_assistant_entry(vec![], Some("end_turn")),
            make_assistant_entry(vec![make_tool_use("Bash", "tu_ca2")], None),
        ];
        let reader = test_reader();
        // Latest assistant has pending tool_use with stop_reason=None --Working
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::Working
        );
    }

    // Question detection tests
    #[test]
    fn test_end_turn_with_question_returns_needs_attention_with_detail() {
        let entries = vec![make_assistant_entry(
            vec![make_text("I found the bug. Would you like me to fix it?")],
            Some("end_turn"),
        )];
        let reader = test_reader();
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::NeedsAttention {
                detail: Some("question".to_string()),
            }
        );
    }

    #[test]
    fn test_end_turn_without_question_returns_needs_attention_no_detail() {
        let entries = vec![make_assistant_entry(
            vec![make_text("Done, all tests pass.")],
            Some("end_turn"),
        )];
        let reader = test_reader();
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::NeedsAttention { detail: None }
        );
    }

    #[test]
    fn test_end_turn_multiline_question_on_last_line() {
        let entries = vec![make_assistant_entry(
            vec![make_text(
                "I've analyzed the code and found 3 issues.\n\nShould I proceed with the fix?",
            )],
            Some("end_turn"),
        )];
        let reader = test_reader();
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::NeedsAttention {
                detail: Some("question".to_string()),
            }
        );
    }

    #[test]
    fn test_end_turn_question_mid_message_not_last_line() {
        // Question appears in the middle, but last line is a statement
        let entries = vec![make_assistant_entry(
            vec![make_text(
                "Why does this matter? Because it affects performance.\n\nHere's the fix applied.",
            )],
            Some("end_turn"),
        )];
        let reader = test_reader();
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::NeedsAttention { detail: None }
        );
    }

    #[test]
    fn test_end_turn_no_text_content_returns_no_detail() {
        // end_turn with no text content blocks at all
        let entries = vec![make_assistant_entry(vec![], Some("end_turn"))];
        let reader = test_reader();
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::NeedsAttention { detail: None }
        );
    }

    #[test]
    fn test_end_turn_text_after_resolved_tool_use_question() {
        // Tool_use was already resolved, assistant ends turn with a question
        let entries = vec![
            make_assistant_entry(
                vec![
                    make_tool_use("Read", "tu_q1"),
                    make_text("Batch 1 completed. Do you want me to continue?"),
                ],
                Some("end_turn"),
            ),
            make_human_entry(vec![make_tool_result("tu_q1")]),
        ];
        let reader = test_reader();
        // stop_reason="end_turn" with question text --NeedsAttention with detail
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::NeedsAttention {
                detail: Some("question".to_string()),
            }
        );
    }

    #[test]
    fn test_end_turn_plain_text_question() {
        // The realistic case: assistant sends only text ending with a question
        let entries = vec![make_assistant_entry(
            vec![make_text("Batch 1 completed. Do you want me to continue?")],
            Some("end_turn"),
        )];
        let reader = test_reader();
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::NeedsAttention {
                detail: Some("question".to_string()),
            }
        );
    }

    #[test]
    fn test_consecutive_assistant_question_detected() {
        let entries = vec![
            make_assistant_entry(vec![make_tool_use("Read", "tu_cq1")], None),
            make_assistant_entry(
                vec![make_text("Want me to look into this?")],
                Some("end_turn"),
            ),
        ];
        let reader = test_reader();
        assert_eq!(
            reader.determine_status(&entries),
            ClaudeSessionStatus::NeedsAttention {
                detail: Some("question".to_string()),
            }
        );
    }
}
