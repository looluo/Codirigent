use super::*;
use codirigent_core::{DefaultEventBus, ImageData, ImageFormat};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

fn temp_fixture_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(name)
}

#[test]
fn detector_maintenance_merge_dedupes_and_preserves_priority() {
    let merged = merge_detector_maintenance_session_ids(
        vec![SessionId(2), SessionId(1), SessionId(2)],
        vec![SessionId(3), SessionId(1), SessionId(4)],
    );

    assert_eq!(
        merged,
        vec![SessionId(2), SessionId(1), SessionId(3), SessionId(4)]
    );
}

#[test]
fn detector_maintenance_batch_includes_stale_cached_sessions() {
    let detector = Arc::new(Mutex::new(codirigent_detector::InputDetector::new(
        codirigent_detector::DetectorConfig::default(),
        Arc::new(DefaultEventBus::new(16)),
    )));
    let cli_readers = Arc::new(Mutex::new(super::super::types::CliReaders::new()));
    let stale_id = SessionId(17);

    cli_readers
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .cached_status
        .insert(
            stale_id,
            CachedCliStatus {
                status: SessionStatus::NeedsAttention,
                seen_at: Instant::now(),
                source: CliStatusSource::Hook,
                status_since: Instant::now(),
                ttl: Duration::from_secs(30),
            },
        );

    let batch = collect_detector_maintenance_batch(&detector, &cli_readers);
    assert_eq!(batch.session_ids, vec![stale_id]);
}

fn codex_input(
    session_id: u64,
    working_dir: &str,
    has_explicit_codex_started_at: bool,
) -> JsonlCheckInput {
    JsonlCheckInput {
        session_id: SessionId(session_id),
        working_dir: std::path::PathBuf::from(working_dir),
        child_pid: None,
        cli_type: CliType::CodexCli,
        codex_session_id: None,
        codex_execution_mode: None,
        has_explicit_codex_started_at,
        current_status: SessionStatus::Idle,
        created_at_millis: 0,
    }
}

fn sig(status: &str, codirigent_session_id: Option<&str>, ts: u64) -> HookSignal {
    HookSignal {
        status: status.to_owned(),
        cli_type: None,
        cli_session_id: None,
        approval_policy: None,
        sandbox_policy_type: None,
        codirigent_session_id: codirigent_session_id.map(str::to_owned),
        ts,
    }
}

#[test]
fn hook_signal_without_codirigent_id_is_ignored() {
    // Signals without codirigent_session_id come from Claude Code started
    // outside Codirigent and should be silently discarded.
    let signal = sig("working", None, 100);
    assert!(signal.codirigent_session_id.is_none());
}

#[test]
fn hook_signal_with_codirigent_id_is_valid() {
    let signal = sig("working", Some("42"), 100);
    assert_eq!(signal.codirigent_session_id.as_deref(), Some("42"));
    assert_eq!(signal.status, "working");
}

#[test]
fn hook_signal_codirigent_id_parses_to_session_id() {
    let signal = sig("needs_attention", Some("7"), 100);
    let id: u64 = signal
        .codirigent_session_id
        .unwrap()
        .parse()
        .expect("should parse");
    assert_eq!(id, 7);
}

#[test]
fn hook_signal_invalid_codirigent_id_not_parseable() {
    // Non-numeric IDs are rejected at parse time in hook signal processing.
    let bad_id = "not-a-number".to_owned();
    assert!(bad_id.parse::<u64>().is_err());
}

#[test]
fn hook_signal_deserializes_from_json() {
    let json = r#"{"status":"working","cli_session_id":"codex-session","codirigent_session_id":"3","ts":1234567890}"#;
    let signal: HookSignal = serde_json::from_str(json).unwrap();
    assert_eq!(signal.status, "working");
    assert_eq!(signal.cli_session_id.as_deref(), Some("codex-session"));
    assert_eq!(signal.codirigent_session_id.as_deref(), Some("3"));
    assert_eq!(signal.ts, 1234567890);
}

#[test]
fn hook_signal_deserializes_without_codirigent_id() {
    // Backwards-compatible: old signal files without the field deserialize fine.
    let json = r#"{"status":"idle","ts":100}"#;
    let signal: HookSignal = serde_json::from_str(json).unwrap();
    assert!(signal.cli_session_id.is_none());
    assert!(signal.codirigent_session_id.is_none());
}

#[test]
fn hook_signal_context_infers_bypass_mode() {
    assert_eq!(
        codex_execution_mode_from_approval_and_sandbox(Some("never"), Some("danger-full-access"),),
        Some(CodexExecutionMode::Bypass)
    );
}

#[test]
fn hook_signal_context_infers_full_auto_mode() {
    assert_eq!(
        codex_execution_mode_from_approval_and_sandbox(Some("never"), Some("workspace-write")),
        Some(CodexExecutionMode::FullAuto)
    );
}

#[test]
fn hook_signal_is_applied_when_timestamp_advances() {
    let fp = hook_signal_fingerprint("working", Some(CLI_TYPE_CLAUDE), None, None);
    assert!(should_apply_hook_signal(None, 100, fp));
    assert!(should_apply_hook_signal(
        Some(ProcessedHookSignal {
            ts: 99,
            fingerprint: fp,
        }),
        100,
        fp,
    ));
}

#[test]
fn identical_hook_signal_is_ignored_when_timestamp_does_not_advance() {
    let fp = hook_signal_fingerprint("working", Some(CLI_TYPE_CLAUDE), None, None);
    assert!(!should_apply_hook_signal(
        Some(ProcessedHookSignal {
            ts: 100,
            fingerprint: fp,
        }),
        100,
        fp,
    ));
    assert!(!should_apply_hook_signal(
        Some(ProcessedHookSignal {
            ts: 101,
            fingerprint: fp,
        }),
        100,
        fp,
    ));
}

#[test]
fn changed_hook_signal_with_same_timestamp_is_still_applied() {
    let old_fp = hook_signal_fingerprint("working", Some(CLI_TYPE_CLAUDE), None, None);
    let new_fp = hook_signal_fingerprint("response_ready", Some(CLI_TYPE_CLAUDE), None, None);

    assert!(should_apply_hook_signal(
        Some(ProcessedHookSignal {
            ts: 100,
            fingerprint: old_fp,
        }),
        100,
        new_fp,
    ));
}

#[test]
fn numeric_signal_file_id_is_not_treated_as_codex_session_id() {
    assert_eq!(resolve_hook_cli_session_id("3", None, SessionId(3)), None);
}

#[test]
fn non_numeric_signal_file_id_can_backfill_cli_session_id() {
    assert_eq!(
        resolve_hook_cli_session_id("codex-uuid", None, SessionId(3)),
        Some("codex-uuid".to_string())
    );
}

#[test]
fn explicit_cli_session_id_wins_over_signal_file_id() {
    assert_eq!(
        resolve_hook_cli_session_id("3", Some("real-codex-id"), SessionId(3)),
        Some("real-codex-id".to_string())
    );
}

#[test]
fn unsafe_hook_cli_session_id_is_rejected() {
    assert_eq!(
        resolve_hook_cli_session_id("3", Some("bad;id"), SessionId(3)),
        None
    );
    assert_eq!(
        resolve_hook_cli_session_id("bad;id", None, SessionId(3)),
        None
    );
}

#[test]
fn ambiguous_codex_probe_is_deferred_without_explicit_start_time() {
    let inputs = vec![
        codex_input(1, "C:/repo", false),
        codex_input(2, "C:/repo", false),
    ];
    let counts = count_codex_sessions_without_session_id_per_working_dir(&inputs);

    assert!(should_defer_ambiguous_codex_probe(&inputs[0], &counts));
    assert!(should_defer_ambiguous_codex_probe(&inputs[1], &counts));
}

#[test]
fn ambiguous_codex_probe_uses_timestamp_when_start_time_is_known() {
    let inputs = vec![
        codex_input(1, "C:/repo", true),
        codex_input(2, "C:/repo", true),
    ];
    let counts = count_codex_sessions_without_session_id_per_working_dir(&inputs);

    assert!(!should_defer_ambiguous_codex_probe(&inputs[0], &counts));
    assert!(!should_defer_ambiguous_codex_probe(&inputs[1], &counts));
}

#[test]
fn ambiguous_codex_probe_only_defers_session_missing_start_time() {
    let inputs = vec![
        codex_input(1, "C:/repo", true),
        codex_input(2, "C:/repo", false),
    ];
    let counts = count_codex_sessions_without_session_id_per_working_dir(&inputs);

    assert!(!should_defer_ambiguous_codex_probe(&inputs[0], &counts));
    assert!(should_defer_ambiguous_codex_probe(&inputs[1], &counts));
}

#[test]
fn git_refresh_updates_git_info_without_overwriting_custom_group() {
    let project_path = temp_fixture_path("project");
    let mut session = Session::new(SessionId(1), "Session 1".to_string(), project_path.clone());
    session.group = Some("custom-group".to_string());
    session.color = Some("#f43f5e".to_string());

    let git_info = Some(GitRepoInfo {
        repo_root: project_path,
        branch: "feature/custom-group".to_string(),
        dirty_count: 2,
        has_staged: false,
        head_sha: Some("deadbeef".to_string()),
        unstaged_files: Vec::new(),
        staged_files: Vec::new(),
    });

    assert!(update_cached_session_git_info(&mut session, &git_info));
    assert_eq!(session.group.as_deref(), Some("custom-group"));
    assert_eq!(session.color.as_deref(), Some("#f43f5e"));
    assert_eq!(session.git_info, git_info);
}

#[test]
fn cwd_session_update_preserves_custom_group_from_manager() {
    let project_path = temp_fixture_path("project");
    let other_project_path = temp_fixture_path("other-project");
    let mut workspace_session =
        Session::new(SessionId(1), "Session 1".to_string(), project_path.clone());
    workspace_session.group = Some("custom-group".to_string());
    workspace_session.color = Some("#f43f5e".to_string());
    workspace_session.git_info = Some(GitRepoInfo {
        repo_root: project_path,
        branch: "main".to_string(),
        dirty_count: 1,
        has_staged: false,
        head_sha: Some("deadbeef".to_string()),
        unstaged_files: Vec::new(),
        staged_files: Vec::new(),
    });

    let mut manager_session = workspace_session.clone();
    manager_session.working_directory = other_project_path.clone();

    apply_cwd_session_update_from_manager(&mut workspace_session, &manager_session);

    assert_eq!(workspace_session.working_directory, other_project_path);
    assert_eq!(workspace_session.group.as_deref(), Some("custom-group"));
    assert_eq!(workspace_session.color.as_deref(), Some("#f43f5e"));
    assert!(workspace_session.git_info.is_none());
}

#[test]
fn hook_signal_cli_type_maps_to_codex() {
    assert_eq!(
        cli_type_from_hook_signal_name(CLI_TYPE_CODEX),
        Some(CliType::CodexCli)
    );
}

#[test]
fn hook_signal_cli_type_maps_to_claude_and_gemini() {
    assert_eq!(
        cli_type_from_hook_signal_name(CLI_TYPE_CLAUDE),
        Some(CliType::ClaudeCode)
    );
    assert_eq!(
        cli_type_from_hook_signal_name(CLI_TYPE_GEMINI),
        Some(CliType::GeminiCli)
    );
}

#[test]
fn tiny_dib_preview_is_suppressed() {
    let image = ImageData {
        bytes: vec![0; 8 * 1024],
        width: 32,
        height: 32,
        format: ImageFormat::Dib,
    };

    assert!(!should_show_clipboard_preview(&image));
}

#[test]
fn larger_dib_preview_is_allowed() {
    let image = ImageData {
        bytes: vec![0; 40 * 1024],
        width: 320,
        height: 240,
        format: ImageFormat::Dib,
    };

    assert!(should_show_clipboard_preview(&image));
}

#[test]
fn focused_schedulable_output_is_prioritized() {
    let session_ids = vec![SessionId(1), SessionId(2), SessionId(3)];
    let schedulable = HashSet::from([SessionId(2), SessionId(3)]);

    let (ready, deferred) =
        prioritize_and_partition_output_sessions(session_ids, Some(SessionId(2)), |id| {
            schedulable.contains(&id)
        });

    assert_eq!(ready, vec![SessionId(2), SessionId(3)]);
    assert_eq!(deferred, vec![SessionId(1)]);
}

#[test]
fn unschedulable_output_sessions_are_deferred_instead_of_dropped() {
    let session_ids = vec![SessionId(1), SessionId(2), SessionId(3)];
    let schedulable = HashSet::from([SessionId(3)]);

    let (ready, deferred) =
        prioritize_and_partition_output_sessions(session_ids, Some(SessionId(2)), |id| {
            schedulable.contains(&id)
        });

    assert_eq!(ready, vec![SessionId(3)]);
    assert_eq!(deferred, vec![SessionId(2), SessionId(1)]);
}
