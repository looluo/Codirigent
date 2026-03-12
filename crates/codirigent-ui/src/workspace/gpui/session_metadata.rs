//! Lightweight session metadata helpers.

use std::collections::HashMap;

pub(super) fn session_project_name(session: &codirigent_core::Session) -> Option<String> {
    session
        .git_info
        .as_ref()
        .and_then(|git_info| git_info.repo_root.file_name())
        .or_else(|| session.working_directory.file_name())
        .and_then(|name| name.to_str())
        .map(str::to_owned)
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
}
