//! Keyboard input handlers for WorkspaceView.
//!
//! This module contains all methods related to:
//! - Custom layout picker keyboard handling
//! - Layout profile persistence to settings

use super::gpui::WorkspaceView;
use crate::layout_profile::SavedLayoutProfile;
use crate::toolbar::CustomLayoutMode;
use codirigent_core::{LayoutMode, LayoutNode, SplitDirection};
use gpui::{Context, KeyDownEvent};

fn saved_grid_layout_profile(rows: u32, cols: u32) -> SavedLayoutProfile {
    SavedLayoutProfile::new(
        format!("custom-{}x{}", rows, cols),
        format!("{}x{}", rows, cols),
        LayoutMode::Grid { rows, cols },
    )
}

fn saved_split_layout_profile(tree: &LayoutNode) -> SavedLayoutProfile {
    let pane_count = tree.leaf_count();
    SavedLayoutProfile::new(
        format!("custom-split-{}", pane_count),
        format!("Split ({})", pane_count),
        LayoutMode::SplitTree { root: tree.clone() },
    )
}

impl WorkspaceView {
    pub(super) fn save_layout_profiles_to_settings(&mut self, cx: &mut Context<Self>) {
        self.persist_layout_profiles_to_settings(cx);
    }

    pub(super) fn apply_custom_layout_from_picker(&mut self, cx: &mut Context<Self>) -> bool {
        let applied = match self.custom_picker.mode {
            CustomLayoutMode::Grid => {
                let Some((rows, cols)) = self.custom_picker.validate() else {
                    return false;
                };

                self.custom_picker.close();
                let profile = crate::layout::LayoutProfile::Custom { rows, cols };
                self.workspace.set_layout(profile);
                true
            }
            CustomLayoutMode::Split => {
                let Some(tree) = self.custom_picker.validate_split() else {
                    return false;
                };

                self.custom_picker.close();
                self.workspace.set_split_tree(tree);
                true
            }
        };

        if applied {
            self.mark_layout_cache_dirty();
            self.mark_ui_sync_dirty();
            self.save_state_to_disk(cx);
        }

        applied
    }

    pub(super) fn save_and_apply_custom_layout_from_picker(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let saved_profile = match self.custom_picker.mode {
            CustomLayoutMode::Grid => {
                let Some((rows, cols)) = self.custom_picker.validate() else {
                    return false;
                };

                self.custom_picker.close();
                let profile = crate::layout::LayoutProfile::Custom { rows, cols };
                self.workspace.set_layout(profile);
                saved_grid_layout_profile(rows, cols)
            }
            CustomLayoutMode::Split => {
                let Some(tree) = self.custom_picker.validate_split() else {
                    return false;
                };

                self.custom_picker.close();
                self.workspace.set_split_tree(tree.clone());
                saved_split_layout_profile(&tree)
            }
        };

        let profile_id = saved_profile.id.clone();
        self.mark_layout_cache_dirty();
        self.mark_ui_sync_dirty();
        self.top_bar.profile_manager.add_profile(saved_profile);
        self.top_bar.set_active_profile_id(&profile_id);
        self.save_layout_profiles_to_settings(cx);
        self.save_state_to_disk(cx);

        true
    }

    pub(super) fn handle_custom_layout_key_down(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.custom_picker.is_open {
            return false;
        }

        let key = event.keystroke.key.to_lowercase();

        match key.as_str() {
            "escape" => {
                self.custom_picker.close();
                cx.notify();
                return true;
            }
            "enter" => {
                self.save_and_apply_custom_layout_from_picker(cx);
                cx.notify();
                return true;
            }
            _ => {}
        }

        match self.custom_picker.mode {
            CustomLayoutMode::Grid => {
                match key.as_str() {
                    "tab" => {
                        let current = self.custom_picker.focused_input().unwrap_or(0);
                        let next = if current == 0 { 1 } else { 0 };
                        self.custom_picker.set_focus(next);
                        cx.notify();
                        return true;
                    }
                    "backspace" => {
                        self.custom_picker.handle_backspace();
                        cx.notify();
                        return true;
                    }
                    _ => {}
                }

                if event.keystroke.modifiers.control
                    || event.keystroke.modifiers.alt
                    || event.keystroke.modifiers.platform
                {
                    return true;
                }

                if let Some(ref key_char) = event.keystroke.key_char {
                    if let Some(ch) = key_char.chars().next() {
                        if ch.is_ascii_digit() {
                            self.custom_picker.handle_char_input(ch);
                            cx.notify();
                        }
                    }
                }
            }
            CustomLayoutMode::Split => match key.as_str() {
                "h" => {
                    self.custom_picker
                        .split_selected(SplitDirection::Horizontal);
                    cx.notify();
                    return true;
                }
                "v" => {
                    self.custom_picker.split_selected(SplitDirection::Vertical);
                    cx.notify();
                    return true;
                }
                "backspace" | "delete" => {
                    self.custom_picker.remove_selected();
                    cx.notify();
                    return true;
                }
                "tab" => {
                    let slots = self.custom_picker.split_tree.slots_in_order();
                    if !slots.is_empty() {
                        let current_idx = self
                            .custom_picker
                            .selected_slot
                            .and_then(|s| slots.iter().position(|&o| o == s));
                        let next = match current_idx {
                            Some(i) => (i + 1) % slots.len(),
                            None => 0,
                        };
                        self.custom_picker.selected_slot = Some(slots[next]);
                    }
                    cx.notify();
                    return true;
                }
                _ => {}
            },
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::{saved_grid_layout_profile, saved_split_layout_profile};
    use codirigent_core::{LayoutMode, LayoutNode};

    #[test]
    fn saved_grid_layout_profile_uses_dimensions() {
        let profile = saved_grid_layout_profile(4, 3);

        assert_eq!(profile.id, "custom-4x3");
        assert_eq!(profile.name, "4x3");
        assert_eq!(profile.layout, LayoutMode::Grid { rows: 4, cols: 3 });
    }

    #[test]
    fn saved_split_layout_profile_uses_leaf_count() {
        let tree = LayoutNode::from_grid(1, 3);
        let profile = saved_split_layout_profile(&tree);

        assert_eq!(profile.id, "custom-split-3");
        assert_eq!(profile.name, "Split (3)");
        assert_eq!(profile.layout, LayoutMode::SplitTree { root: tree });
    }
}
