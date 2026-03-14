//! Lightweight session metadata helpers.

use codirigent_core::CliType;
use std::collections::HashMap;

fn path_display_name(path: &std::path::Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .or_else(|| {
            let display = path.as_os_str().to_string_lossy();
            (!display.is_empty()).then(|| display.into_owned())
        })
}

pub(in crate::workspace) fn session_project_name(
    session: &codirigent_core::Session,
) -> Option<String> {
    session
        .git_info
        .as_ref()
        .and_then(|git_info| path_display_name(&git_info.repo_root))
        .or_else(|| path_display_name(&session.working_directory))
}

pub(super) fn resolved_task_title(
    task_id: &codirigent_core::TaskId,
    task_titles: Option<&HashMap<codirigent_core::TaskId, String>>,
) -> String {
    task_titles
        .and_then(|titles| titles.get(task_id))
        .cloned()
        .unwrap_or_else(|| task_id.0.to_string())
}

pub(in crate::workspace) fn cli_type_badge_name(cli_type: CliType) -> Option<&'static str> {
    match cli_type {
        CliType::ClaudeCode => Some("Claude Code"),
        CliType::GeminiCli => Some("Gemini"),
        CliType::CodexCli => Some("Codex"),
        CliType::GenericShell => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_project_name_prefers_git_repo_root_name() {
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
            session_project_name(&session),
            Some("project-root".to_string())
        );
    }

    #[test]
    fn session_project_name_falls_back_to_working_directory_name() {
        let session = codirigent_core::Session::new(
            codirigent_core::SessionId(1),
            "Session 1".to_string(),
            std::path::PathBuf::from("/workspace/focused-pane"),
        );

        assert_eq!(
            session_project_name(&session),
            Some("focused-pane".to_string())
        );
    }

    #[test]
    fn session_project_name_handles_root_workspaces() {
        let session = codirigent_core::Session::new(
            codirigent_core::SessionId(1),
            "Session 1".to_string(),
            std::path::PathBuf::from("/"),
        );

        assert_eq!(session_project_name(&session), Some("/".to_string()));
    }

    #[test]
    fn resolved_task_title_prefers_cached_title_and_falls_back_to_id() {
        let task_id = codirigent_core::TaskId::from("task-123");
        let mut titles = HashMap::new();
        titles.insert(task_id.clone(), "Review parser".to_string());

        assert_eq!(
            resolved_task_title(&task_id, Some(&titles)),
            "Review parser".to_string()
        );
        assert_eq!(
            resolved_task_title(&codirigent_core::TaskId::from("task-456"), Some(&titles)),
            "task-456".to_string()
        );
        assert_eq!(resolved_task_title(&task_id, None), "task-123".to_string());
    }

    #[test]
    fn cli_type_badge_name_hides_generic_shell() {
        assert_eq!(
            cli_type_badge_name(codirigent_core::CliType::ClaudeCode),
            Some("Claude Code")
        );
        assert_eq!(
            cli_type_badge_name(codirigent_core::CliType::GeminiCli),
            Some("Gemini")
        );
        assert_eq!(
            cli_type_badge_name(codirigent_core::CliType::CodexCli),
            Some("Codex")
        );
        assert_eq!(
            cli_type_badge_name(codirigent_core::CliType::GenericShell),
            None
        );
    }
}
