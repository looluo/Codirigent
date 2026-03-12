//! GPUI View Testing Strategy
//!
//! # Why Limited Tests
//!
//! `WorkspaceView` is a GPUI view component that requires the GPUI runtime
//! for rendering and interaction. Testing GPUI views requires:
//! - GPUI test harness (`gpui::TestAppContext`)
//! - Window creation for rendering tests
//! - Focus simulation for interaction tests
//!
//! # Test Coverage Strategy
//!
//! 1. **Core Business Logic** - Fully tested in `workspace/tests.rs` (29 tests)
//!    - Layout management, session handling, focus navigation
//!    - Bounds calculation, cell info generation
//!    - All non-GPUI logic has 100% test coverage
//!
//! 2. **GPUI Integration** - Deferred to integration tests
//!    - Rendering correctness requires visual inspection or snapshot tests
//!    - Action handlers require GPUI action dispatch simulation
//!
//! # Future: GPUI Test Infrastructure
//!
//! When GPUI test helpers are available, add tests for:
//! - [ ] WorkspaceView renders without panic
//! - [ ] Action handlers (NewSession, CloseSession, etc.) work correctly
//! - [ ] Focus delegation to child components
//! - [ ] Layout changes trigger re-render

use std::collections::HashMap;

#[test]
fn test_core_workspace_is_tested_separately() {
    // Reminder: Core workspace logic has dedicated tests in workspace/tests.rs
    // Run `cargo test workspace::tests` to see all 29 tests pass
    use crate::workspace::Workspace;

    // Quick sanity check that we can create a workspace
    let ws = Workspace::new();
    assert!(ws.sessions().is_empty());
}

#[test]
fn test_skip_collapsed_resize_when_current_is_usable() {
    assert!(super::WorkspaceView::should_skip_collapsed_resize(
        40, 120, 40, 1
    ));
    assert!(super::WorkspaceView::should_skip_collapsed_resize(
        40, 120, 1, 120
    ));
    assert!(super::WorkspaceView::should_skip_collapsed_resize(
        40, 120, 1, 1
    ));
}

#[test]
fn test_do_not_skip_collapsed_resize_if_already_collapsed() {
    assert!(!super::WorkspaceView::should_skip_collapsed_resize(
        1, 1, 1, 1
    ));
    assert!(!super::WorkspaceView::should_skip_collapsed_resize(
        1, 80, 1, 1
    ));
}

#[test]
fn test_do_not_skip_non_collapsed_resize() {
    assert!(!super::WorkspaceView::should_skip_collapsed_resize(
        40, 120, 30, 100
    ));
}

#[test]
fn test_render_focus_signature_tracks_focus_in_single_layout() {
    assert_eq!(
        super::WorkspaceView::render_focus_signature_for_layout(
            crate::layout::LayoutProfile::Single,
            Some(codirigent_core::SessionId(2)),
        ),
        Some(codirigent_core::SessionId(2))
    );
}

#[test]
fn test_render_focus_signature_ignores_focus_outside_single_layout() {
    assert_eq!(
        super::WorkspaceView::render_focus_signature_for_layout(
            crate::layout::LayoutProfile::Grid2x2,
            Some(codirigent_core::SessionId(2)),
        ),
        None
    );
}

#[test]
fn test_normalize_codex_execution_mode_detects_bypass_alias() {
    assert_eq!(
        super::WorkspaceView::normalize_codex_execution_mode("codex --yolo"),
        Some(codirigent_core::CodexExecutionMode::Bypass)
    );
}

#[test]
fn test_normalize_codex_execution_mode_detects_full_auto() {
    assert_eq!(
        super::WorkspaceView::normalize_codex_execution_mode("codex resume abc --full-auto"),
        Some(codirigent_core::CodexExecutionMode::FullAuto)
    );
}

#[test]
fn test_normalize_codex_execution_mode_detects_explicit_never_and_danger() {
    assert_eq!(
        super::WorkspaceView::normalize_codex_execution_mode(
            "codex -a never -s danger-full-access"
        ),
        Some(codirigent_core::CodexExecutionMode::Bypass)
    );
}

#[test]
fn test_session_project_name_prefers_git_repo_root_name() {
    let mut session = codirigent_core::Session::new(
        codirigent_core::SessionId(1),
        "Session 1".to_string(),
        std::path::PathBuf::from("/workspace/subdir"),
    );
    session.git_info = Some(codirigent_core::GitRepoInfo {
        repo_root: std::path::PathBuf::from("/workspace/project-root"),
        branch: "main".to_string(),
        dirty_count: 0,
        has_staged: false,
        head_sha: None,
        unstaged_files: Vec::new(),
        staged_files: Vec::new(),
    });

    assert_eq!(
        super::session_project_name(&session),
        Some("project-root".to_string())
    );
}

#[test]
fn test_session_project_name_falls_back_to_working_directory_name() {
    let session = codirigent_core::Session::new(
        codirigent_core::SessionId(1),
        "Session 1".to_string(),
        std::path::PathBuf::from("/workspace/focused-pane"),
    );

    assert_eq!(
        super::session_project_name(&session),
        Some("focused-pane".to_string())
    );
}

#[test]
fn test_resolved_task_title_prefers_cached_title_and_falls_back_to_id() {
    let task_id = codirigent_core::TaskId::from("task-123");
    let mut titles = HashMap::new();
    titles.insert(task_id.clone(), "Review parser".to_string());

    assert_eq!(
        super::resolved_task_title(&task_id, Some(&titles)),
        "Review parser".to_string()
    );
    assert_eq!(
        super::resolved_task_title(&codirigent_core::TaskId::from("task-456"), Some(&titles)),
        "task-456".to_string()
    );
    assert_eq!(
        super::resolved_task_title(&task_id, None),
        "task-123".to_string()
    );
}

#[test]
fn test_keystroke_is_text_input_for_plain_printable_without_key_char() {
    let event = gpui::KeyDownEvent {
        keystroke: gpui::Keystroke {
            modifiers: gpui::Modifiers::default(),
            key: "a".to_string(),
            key_char: None,
        },
        is_held: false,
    };

    assert!(super::WorkspaceView::keystroke_is_text_input(&event));
}

#[test]
fn test_keystroke_is_not_text_input_for_named_terminal_key() {
    let event = gpui::KeyDownEvent {
        keystroke: gpui::Keystroke {
            modifiers: gpui::Modifiers::default(),
            key: "enter".to_string(),
            key_char: None,
        },
        is_held: false,
    };

    assert!(!super::WorkspaceView::keystroke_is_text_input(&event));
}
