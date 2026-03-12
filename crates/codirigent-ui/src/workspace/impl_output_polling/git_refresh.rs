//! Future home for background git refresh scheduling and apply helpers.
//!
//! Expected move targets in Phase B:
//! - bulk git refresh scheduling
//! - per-session git refresh follow-up
//! - git-info apply helpers

use super::WorkspaceView;
use codirigent_core::{GitRepoInfo, Session, SessionId, SessionManager};
use gpui::Context;
use std::path::PathBuf;
use std::time::Instant;

fn update_cached_session_git_info(session: &mut Session, git_info: &Option<GitRepoInfo>) -> bool {
    if session.git_info == *git_info {
        return false;
    }

    session.git_info = git_info.clone();
    true
}

pub(super) fn apply_cwd_session_update_from_manager(
    workspace_session: &mut Session,
    manager_session: &Session,
) {
    workspace_session.working_directory = manager_session.working_directory.clone();
    workspace_session.group = manager_session.group.clone();
    workspace_session.color = manager_session.color.clone();
    workspace_session.git_info = None;
}

impl WorkspaceView {
    /// Spawn a background git-status refresh for all sessions if the last
    /// refresh was more than 3 seconds ago and no refresh is in-flight.
    pub(super) fn schedule_background_git_refresh(&mut self, cx: &mut Context<Self>) {
        if self.polling.last_git_refresh.elapsed() < Self::BACKGROUND_REFRESH_INTERVAL
            || self.polling.git_refresh_in_flight
        {
            return;
        }
        self.polling.last_git_refresh = Instant::now();
        self.polling.git_refresh_in_flight = true;
        let session_ids: Vec<SessionId> = self.workspace.sessions().iter().map(|s| s.id).collect();
        let session_manager = self.session_manager.clone();

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let git_infos = cx
                .background_executor()
                .spawn(async move {
                    let mgr = match session_manager.lock() {
                        Ok(m) => m,
                        Err(_) => return Vec::new(),
                    };
                    session_ids
                        .iter()
                        .map(|id| (*id, mgr.refresh_git_status(*id)))
                        .collect::<Vec<_>>()
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.polling.git_refresh_in_flight = false;
                let mut git_changed = false;
                for (id, git_info) in &git_infos {
                    if let Some(header) = this.terminal_headers.get_mut(id) {
                        let branch = git_info.as_ref().map(|info| info.branch.clone());
                        let dirty_count = git_info.as_ref().map(|info| info.dirty_count);
                        if header.git_branch != branch || header.git_dirty_count != dirty_count {
                            header.git_branch = branch;
                            header.git_dirty_count = dirty_count;
                            git_changed = true;
                        }
                    }
                    if let Some(session) = this.workspace.session_mut(*id) {
                        git_changed |= update_cached_session_git_info(session, git_info);
                    }
                }
                if git_changed {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn spawn_session_git_refresh(
        &mut self,
        session_id: SessionId,
        expected_cwd: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let session_manager = self.session_manager.clone();

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let expected_cwd_for_bg = expected_cwd.clone();
            let git_info = cx
                .background_executor()
                .spawn(async move {
                    let mgr = session_manager.lock().ok()?;
                    let session = mgr.get_session(session_id)?;
                    if session.working_directory != expected_cwd_for_bg {
                        return None;
                    }
                    Some((
                        session_id,
                        expected_cwd_for_bg,
                        mgr.refresh_git_status_fresh(session_id),
                    ))
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                let Some((session_id, expected_cwd, git_info)) = git_info else {
                    return;
                };
                if !this
                    .workspace
                    .session(session_id)
                    .is_some_and(|session| session.working_directory == expected_cwd)
                {
                    return;
                }

                let branch = git_info.as_ref().map(|info| info.branch.clone());
                let dirty_count = git_info.as_ref().map(|info| info.dirty_count);
                let mut changed = false;
                if let Some(header) = this.terminal_headers.get_mut(&session_id) {
                    if header.git_branch != branch || header.git_dirty_count != dirty_count {
                        header.git_branch = branch.clone();
                        header.git_dirty_count = dirty_count;
                        changed = true;
                    }
                }
                if let Some(session) = this.workspace.session_mut(session_id) {
                    changed |= update_cached_session_git_info(session, &git_info);
                }
                if changed {
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_cwd_session_update_from_manager, update_cached_session_git_info};
    use codirigent_core::{GitRepoInfo, Session, SessionId};
    use std::path::PathBuf;

    fn temp_fixture_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
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
}
