//! File tree and worktree event handlers for WorkspaceView.
//!
//! This module contains all methods related to:
//! - File tree panel refresh and synchronization
//! - File tree event handling (selection, activation, drag-drop)
//! - Worktree panel management
//! - File tree context menu operations
//! - Path insertion and clipboard operations

use super::gpui::WorkspaceView;
use super::project_state::CachedProjectRootState;
use super::types::FileTreeContextMenu;
use crate::sidebar::{FileTreeEntryData, FileTreeEvent};
use codirigent_core::{SessionId, SessionManager};
use codirigent_filetree::FileTree;
use codirigent_session::WorktreeManager;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

impl WorkspaceView {
    fn cache_current_project_root_state(&mut self) {
        let Some(root) = self.project.project_root.clone() else {
            return;
        };

        self.project.root_cache.insert(
            root,
            CachedProjectRootState {
                file_tree_model: self.project.file_tree_model.take(),
                worktree_manager: self.project.worktree_manager.take(),
                worktrees: self.project.worktree_panel.worktrees().to_vec(),
                available_branches: self.project.worktree_panel.available_branches().to_vec(),
            },
        );
    }

    fn restore_cached_project_root_state(&mut self, path: &PathBuf) -> bool {
        let Some(cached) = self.project.root_cache.remove(path) else {
            return false;
        };

        self.project.file_tree_model = cached.file_tree_model;
        self.project.worktree_manager = cached.worktree_manager;
        self.project.worktree_panel.set_worktrees(cached.worktrees);
        self.project
            .worktree_panel
            .set_available_branches(cached.available_branches);
        self.refresh_file_tree_panel();
        true
    }

    fn clear_project_root_state(&mut self) {
        self.project.file_tree_model = None;
        self.project.worktree_manager = None;
        self.project.worktree_panel.set_worktrees(Vec::new());
        self.project
            .worktree_panel
            .set_available_branches(Vec::new());
        self.refresh_file_tree_panel();
    }

    fn cached_project_root_state_mut(&mut self, path: &Path) -> &mut CachedProjectRootState {
        self.project
            .root_cache
            .entry(path.to_path_buf())
            .or_insert_with(|| CachedProjectRootState {
                file_tree_model: None,
                worktree_manager: None,
                worktrees: Vec::new(),
                available_branches: Vec::new(),
            })
    }

    pub(super) fn refresh_file_tree_panel(&mut self) {
        let entries = if let Some(tree) = &self.project.file_tree_model {
            let tree: &FileTree = tree;
            tree.visible_entries()
                .into_iter()
                .map(|(depth, entry)| {
                    (
                        depth,
                        FileTreeEntryData {
                            path: entry.path.clone(),
                            name: entry.name.clone(),
                            is_dir: entry.is_dir,
                            expanded: entry.expanded,
                        },
                    )
                })
                .collect()
        } else {
            Vec::new()
        };

        self.project.file_tree.update_from_entries(entries);
    }

    fn spawn_file_tree_refresh(
        &mut self,
        path: PathBuf,
        generation: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        self.polling.file_tree_rebuild_in_flight = true;
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let path_for_build = path.clone();
            let tree_result = cx
                .background_executor()
                .spawn(async move { FileTree::new(path_for_build) })
                .await;

            let _ = this.update(cx, |this, cx| {
                let is_current = this.polling.project_refresh_generation == generation
                    && this.project.project_root.as_ref() == Some(&path);
                if is_current {
                    this.polling.file_tree_rebuild_in_flight = false;
                }

                match tree_result {
                    Ok(tree) => {
                        if is_current {
                            this.project.file_tree_model = Some(tree);
                            this.refresh_file_tree_panel();
                            cx.notify();
                        } else {
                            this.cached_project_root_state_mut(&path).file_tree_model = Some(tree);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to initialize file tree: {}", e);
                        if is_current {
                            this.project.file_tree_model = None;
                            this.refresh_file_tree_panel();
                            cx.notify();
                        } else {
                            this.cached_project_root_state_mut(&path).file_tree_model = None;
                        }
                    }
                }
            });
        })
        .detach();
    }

    fn spawn_worktree_refresh(
        &mut self,
        path: PathBuf,
        generation: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let path_for_build = path.clone();
            let worktree_result = cx.background_executor().spawn(async move {
                let manager = WorktreeManager::new(&path_for_build)?;
                let worktrees = manager.list().to_vec();
                let branches = manager.list_local_branches().unwrap_or_default();
                Ok::<_, anyhow::Error>((manager, worktrees, branches))
            });

            let result = worktree_result.await;
            let _ = this.update(cx, |this, cx| {
                let is_current = this.polling.project_refresh_generation == generation
                    && this.project.project_root.as_ref() == Some(&path);

                match result {
                    Ok((manager, worktrees, branches)) => {
                        let manager = Arc::new(Mutex::new(manager));
                        if is_current {
                            this.project.worktree_manager = Some(manager.clone());
                            this.project.worktree_panel.set_worktrees(worktrees.clone());
                            this.project
                                .worktree_panel
                                .set_available_branches(branches.clone());
                            cx.notify();
                        } else {
                            let cached = this.cached_project_root_state_mut(&path);
                            cached.worktree_manager = Some(manager);
                            cached.worktrees = worktrees;
                            cached.available_branches = branches;
                        }
                    }
                    Err(_) => {
                        if is_current {
                            this.project.worktree_manager = None;
                            this.project.worktree_panel.set_worktrees(Vec::new());
                            this.project
                                .worktree_panel
                                .set_available_branches(Vec::new());
                            cx.notify();
                        } else {
                            let cached = this.cached_project_root_state_mut(&path);
                            cached.worktree_manager = None;
                            cached.worktrees.clear();
                            cached.available_branches.clear();
                        }
                    }
                }
            });
        })
        .detach();
    }

    /// Set the current project root and update dependent UI.
    ///
    /// Sets the root immediately (cheap) and spawns the expensive directory
    /// walk on a background thread to avoid blocking the UI.
    pub(super) fn set_project_root(&mut self, path: PathBuf, cx: &mut gpui::Context<Self>) {
        let same_root = self.project.project_root.as_ref() == Some(&path);
        if same_root && self.project.file_tree_model.is_some() {
            return;
        }

        if !same_root {
            self.cache_current_project_root_state();
        }

        self.project.project_root = Some(path.clone());
        self.project.file_tree.set_root(path.clone());
        let restored_cached_state = self.restore_cached_project_root_state(&path);
        if !restored_cached_state {
            self.clear_project_root_state();
        }
        self.mark_ui_sync_dirty();

        self.polling.project_refresh_generation =
            self.polling.project_refresh_generation.saturating_add(1);
        let generation = self.polling.project_refresh_generation;
        self.spawn_file_tree_refresh(path.clone(), generation, cx);
        self.spawn_worktree_refresh(path, generation, cx);
        self.start_settings_background_load(false, cx);
        if restored_cached_state {
            cx.notify();
        }
    }

    /// Sync the file tree panel to show the focused session's working directory.
    ///
    /// Called when focus switches between sessions so the file tree always
    /// reflects the active session's CWD.
    pub(super) fn sync_file_tree_to_focused_session(&mut self, cx: &mut gpui::Context<Self>) {
        let cwd = self
            .workspace
            .focused_session()
            .map(|s| s.working_directory.clone());
        if let Some(cwd) = cwd {
            // Only update if the directory actually differs from the current root
            if self.project.project_root.as_ref() != Some(&cwd) {
                self.set_project_root(cwd, cx);
            }
        }
    }

    pub(super) fn handle_file_tree_event(
        &mut self,
        event: FileTreeEvent,
        cx: &mut gpui::Context<Self>,
    ) {
        match event {
            FileTreeEvent::FileSelected(path) => {
                info!(?path, "File selected");
                self.project.file_tree.select(&path);
                if let Some(tree) = self.project.file_tree_model.as_mut() {
                    let tree: &mut FileTree = tree;
                    tree.select(&path);
                }
                self.refresh_file_tree_panel();
            }
            FileTreeEvent::FileActivated(path) => {
                info!(?path, "File activated");
                self.open_in_editor(&path);
            }
            FileTreeEvent::DirectoryToggled(path) => {
                info!(?path, "Directory toggled");
                if let Some(tree) = self.project.file_tree_model.as_mut() {
                    let tree: &mut FileTree = tree;
                    if let Err(e) = tree.toggle(&path) {
                        warn!("Failed to toggle directory {:?}: {}", path, e);
                    }
                    self.refresh_file_tree_panel();
                } else {
                    self.project.file_tree.toggle_directory(&path);
                }
            }
            FileTreeEvent::PathDraggedToTerminal { path, session_id } => {
                info!(?path, ?session_id, "Path dragged to terminal");
                if !self.project.is_safe_project_path(&path) {
                    warn!(
                        ?path,
                        "Blocked attempt to send a path outside the project root"
                    );
                    cx.notify();
                    return;
                }
                // C3 implementation: insert path into terminal
                let Some(path_str) = self
                    .project
                    .format_path_for_terminal(&path, self.terminal_path_style())
                else {
                    warn!(?path, "Failed to quote dragged path safely for terminal");
                    cx.notify();
                    return;
                };
                let input = format!("{} ", path_str); // Add space after path
                let session_id = SessionId(session_id);
                if let Ok(manager) = self.session_manager.lock() {
                    if let Err(e) = manager.send_input(session_id, input.as_bytes()) {
                        warn!("Failed to send path to terminal: {}", e);
                    }
                }
            }
        }
        cx.notify();
    }

    pub(super) fn open_file_tree_context_menu(
        &mut self,
        path: PathBuf,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.selection.file_tree_context_menu = Some(FileTreeContextMenu { path, position });
        cx.notify();
    }

    /// Close the file tree context menu.
    pub(super) fn close_file_tree_context_menu(&mut self, cx: &mut gpui::Context<Self>) {
        self.selection.file_tree_context_menu = None;
        cx.notify();
    }

    /// Insert a file path into the focused terminal session.
    pub(super) fn insert_path_to_terminal(&mut self, path: &std::path::Path) {
        if !self.project.is_safe_project_path(path) {
            warn!(
                ?path,
                "Blocked attempt to insert a path outside the project root"
            );
            return;
        }
        if let Some(session_id) = self.workspace.focused_session_id() {
            let Some(path_str) = self
                .project
                .format_path_for_terminal(path, self.terminal_path_style())
            else {
                warn!(?path, "Failed to quote file-tree path safely for terminal");
                return;
            };
            let input = format!("{} ", path_str);
            if let Ok(manager) = self.session_manager.lock() {
                if let Err(e) = manager.send_input(session_id, input.as_bytes()) {
                    warn!("Failed to insert path into terminal: {}", e);
                }
            }
        }
    }

    /// Copy a file path to the system clipboard.
    pub(super) fn copy_path_to_clipboard(&self, path: &std::path::Path) {
        if !self.project.is_safe_project_path(path) {
            warn!(
                ?path,
                "Blocked attempt to copy a path outside the project root"
            );
            return;
        }
        let Some(path_str) = self
            .project
            .format_path_for_terminal(path, self.terminal_path_style())
        else {
            warn!(?path, "Failed to quote file-tree path safely for clipboard");
            return;
        };
        if let Err(e) = self.clipboard.smart_clipboard.write_text(path_str) {
            warn!("Failed to copy path to clipboard: {}", e);
        }
    }
}
