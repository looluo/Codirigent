//! File tree and worktree event handlers for WorkspaceView.
//!
//! This module contains all methods related to:
//! - File tree panel refresh and synchronization
//! - File tree event handling (selection, activation, drag-drop)
//! - Worktree panel management
//! - File tree context menu operations
//! - Path insertion and clipboard operations

use super::gpui::WorkspaceView;
use super::types::FileTreeContextMenu;
use crate::sidebar::{FileTreeEntryData, FileTreeEvent};
use codirigent_core::{SessionId, SessionManager};
use codirigent_filetree::FileTree;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

impl WorkspaceView {
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

    /// Refresh worktree panel from the worktree manager.
    pub(super) fn refresh_worktree_panel(&mut self) {
        if let Some(ref manager) = self.project.worktree_manager {
            if let Ok(mut mgr) = manager.lock() {
                let _: Result<(), anyhow::Error> = mgr.refresh();
                self.project
                    .worktree_panel
                    .set_worktrees(mgr.list().to_vec());
                if let Ok(branches) = mgr.list_local_branches() {
                    self.project.worktree_panel.set_available_branches(branches);
                }
                return;
            }
        }

        self.project.worktree_panel.set_worktrees(Vec::new());
        self.project
            .worktree_panel
            .set_available_branches(Vec::new());
    }

    /// Set the current project root and update dependent UI.
    ///
    /// Sets the root immediately (cheap) and spawns the expensive directory
    /// walk on a background thread to avoid blocking the UI.
    pub(super) fn set_project_root(&mut self, path: PathBuf, cx: &mut gpui::Context<Self>) {
        self.project.project_root = Some(path.clone());
        self.project.file_tree.set_root(path.clone());

        // Worktree manager init is cheap (just reads .git), do it synchronously
        self.project.worktree_manager = codirigent_session::WorktreeManager::new(&path)
            .ok()
            .map(|manager| Arc::new(Mutex::new(manager)));
        self.refresh_worktree_panel();

        // Spawn the expensive FileTree::new (recursive dir walk) on background
        if !self.polling.file_tree_rebuild_in_flight {
            self.polling.file_tree_rebuild_in_flight = true;
            let bg_path = path;
            cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
                let tree_result = cx
                    .background_executor()
                    .spawn(async move { FileTree::new(bg_path.clone()) })
                    .await;

                let _ = this.update(cx, |this, cx| {
                    this.polling.file_tree_rebuild_in_flight = false;
                    match tree_result {
                        Ok(tree) => {
                            this.project.file_tree_model = Some(tree);
                        }
                        Err(e) => {
                            warn!("Failed to initialize file tree: {}", e);
                            this.project.file_tree_model = None;
                        }
                    }
                    this.refresh_file_tree_panel();
                    cx.notify();
                });
            })
            .detach();
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
                // C3 implementation: insert path into terminal
                let path_str = self.project.format_path_for_terminal(&path);
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
        if let Some(session_id) = self.workspace.focused_session_id() {
            let path_str = self.project.format_path_for_terminal(path);
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
        let path_str = self.project.format_path_for_terminal(path);
        if let Err(e) = self.clipboard.smart_clipboard.write_text(path_str) {
            warn!("Failed to copy path to clipboard: {}", e);
        }
    }
}
