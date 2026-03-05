//! Layout types for workspace grid and split tree arrangements.

use serde::{Deserialize, Serialize};

use super::ids::SessionId;

/// Grid position for custom layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GridPosition {
    /// Row index (0-based).
    pub row: u32,
    /// Column index (0-based).
    pub col: u32,
}

/// Unique identifier for a layout slot in a split tree.
///
/// Slots decouple tree shape from session lifecycle — empty slots are valid,
/// and sessions can be reassigned between slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SlotId(pub u32);

impl std::fmt::Display for SlotId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "slot-{}", self.0)
    }
}

/// Direction of a binary split in the layout tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitDirection {
    /// Children arranged left-to-right.
    Horizontal,
    /// Children arranged top-to-bottom.
    Vertical,
}

/// A node in the binary split layout tree.
///
/// Binary splits are simpler than n-ary splits (one drag handle per split),
/// can represent any layout via nesting, and match the approach of
/// tmux/iTerm2/VS Code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LayoutNode {
    /// An internal split node dividing space between two children.
    Split {
        /// Direction of the split.
        direction: SplitDirection,
        /// Ratio (0.0..1.0) — first child's share of available space.
        ratio: f32,
        /// First child (left or top).
        first: Box<LayoutNode>,
        /// Second child (right or bottom).
        second: Box<LayoutNode>,
    },
    /// A leaf node representing a single pane slot.
    Leaf {
        /// The slot identifier for this pane.
        slot: SlotId,
    },
}

impl LayoutNode {
    /// Count the number of leaf nodes in this tree.
    pub fn leaf_count(&self) -> usize {
        match self {
            LayoutNode::Leaf { .. } => 1,
            LayoutNode::Split { first, second, .. } => first.leaf_count() + second.leaf_count(),
        }
    }

    /// DFS traversal returning slots in left-to-right, top-to-bottom order.
    pub fn slots_in_order(&self) -> Vec<SlotId> {
        let mut result = Vec::new();
        self.collect_slots(&mut result);
        result
    }

    fn collect_slots(&self, out: &mut Vec<SlotId>) {
        match self {
            LayoutNode::Leaf { slot } => out.push(*slot),
            LayoutNode::Split { first, second, .. } => {
                first.collect_slots(out);
                second.collect_slots(out);
            }
        }
    }

    /// Convert a grid (rows x cols) to an equivalent split tree.
    ///
    /// A 2x3 grid becomes:
    /// ```text
    ///   V(0.5)
    ///   ├── H(0.333) → H(0.5) → [slot0] [slot1] [slot2]
    ///   └── H(0.333) → H(0.5) → [slot3] [slot4] [slot5]
    /// ```
    pub fn from_grid(rows: u32, cols: u32) -> Self {
        assert!(
            rows >= 1 && cols >= 1,
            "Grid must have at least 1 row and 1 column"
        );
        let mut next_slot = 0u32;
        Self::build_grid_rows(rows, cols, &mut next_slot)
    }

    fn build_grid_rows(rows: u32, cols: u32, next_slot: &mut u32) -> Self {
        if rows == 1 {
            return Self::build_grid_cols(cols, next_slot);
        }
        let first = Self::build_grid_cols(cols, next_slot);
        let second = Self::build_grid_rows(rows - 1, cols, next_slot);
        LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 1.0 / rows as f32,
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    fn build_grid_cols(cols: u32, next_slot: &mut u32) -> Self {
        if cols == 1 {
            let slot = SlotId(*next_slot);
            *next_slot += 1;
            return LayoutNode::Leaf { slot };
        }
        let slot = SlotId(*next_slot);
        *next_slot += 1;
        let first = LayoutNode::Leaf { slot };
        let second = Self::build_grid_cols(cols - 1, next_slot);
        LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 1.0 / cols as f32,
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    /// Split a leaf node into two new leaves.
    ///
    /// Returns `(new_tree, new_slot_id)` where `new_slot_id` is the second child's slot.
    /// The original slot keeps the first child position.
    /// Returns `None` if the target slot is not found.
    pub fn split_slot(
        &self,
        target: SlotId,
        direction: SplitDirection,
        ratio: f32,
        new_slot: SlotId,
    ) -> Option<LayoutNode> {
        match self {
            LayoutNode::Leaf { slot } if *slot == target => Some(LayoutNode::Split {
                direction,
                ratio,
                first: Box::new(LayoutNode::Leaf { slot: *slot }),
                second: Box::new(LayoutNode::Leaf { slot: new_slot }),
            }),
            LayoutNode::Leaf { .. } => None,
            LayoutNode::Split {
                direction: d,
                ratio: r,
                first,
                second,
            } => {
                if let Some(new_first) = first.split_slot(target, direction, ratio, new_slot) {
                    Some(LayoutNode::Split {
                        direction: *d,
                        ratio: *r,
                        first: Box::new(new_first),
                        second: second.clone(),
                    })
                } else {
                    second
                        .split_slot(target, direction, ratio, new_slot)
                        .map(|new_second| LayoutNode::Split {
                            direction: *d,
                            ratio: *r,
                            first: first.clone(),
                            second: Box::new(new_second),
                        })
                }
            }
        }
    }

    /// Remove a leaf node and promote its sibling.
    ///
    /// Returns `None` if the target slot is not found or if this is the root leaf.
    pub fn close_slot(&self, target: SlotId) -> Option<LayoutNode> {
        match self {
            LayoutNode::Leaf { .. } => {
                // Can't close the root leaf
                None
            }
            LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                // Check if first child is the target leaf
                if let LayoutNode::Leaf { slot } = first.as_ref() {
                    if *slot == target {
                        return Some(second.as_ref().clone());
                    }
                }
                // Check if second child is the target leaf
                if let LayoutNode::Leaf { slot } = second.as_ref() {
                    if *slot == target {
                        return Some(first.as_ref().clone());
                    }
                }
                // Recurse into children
                if let Some(new_first) = first.close_slot(target) {
                    Some(LayoutNode::Split {
                        direction: *direction,
                        ratio: *ratio,
                        first: Box::new(new_first),
                        second: second.clone(),
                    })
                } else {
                    second
                        .close_slot(target)
                        .map(|new_second| LayoutNode::Split {
                            direction: *direction,
                            ratio: *ratio,
                            first: first.clone(),
                            second: Box::new(new_second),
                        })
                }
            }
        }
    }

    /// Adjust the split ratio for the parent of a given slot.
    ///
    /// Finds the split node that directly contains the target slot as a child
    /// and updates its ratio. Returns `None` if the slot is not found or is the root leaf.
    pub fn set_ratio_for_slot(&self, target: SlotId, new_ratio: f32) -> Option<LayoutNode> {
        let clamped = new_ratio.clamp(0.1, 0.9);
        match self {
            LayoutNode::Leaf { .. } => None,
            LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                // Check if target is a direct child
                let first_contains = self.direct_child_has_slot(first, target);
                let second_contains = self.direct_child_has_slot(second, target);

                if first_contains || second_contains {
                    Some(LayoutNode::Split {
                        direction: *direction,
                        ratio: clamped,
                        first: first.clone(),
                        second: second.clone(),
                    })
                } else {
                    // Recurse
                    if let Some(new_first) = first.set_ratio_for_slot(target, new_ratio) {
                        Some(LayoutNode::Split {
                            direction: *direction,
                            ratio: *ratio,
                            first: Box::new(new_first),
                            second: second.clone(),
                        })
                    } else {
                        second
                            .set_ratio_for_slot(target, new_ratio)
                            .map(|new_second| LayoutNode::Split {
                                direction: *direction,
                                ratio: *ratio,
                                first: first.clone(),
                                second: Box::new(new_second),
                            })
                    }
                }
            }
        }
    }

    fn direct_child_has_slot(&self, child: &LayoutNode, target: SlotId) -> bool {
        matches!(child, LayoutNode::Leaf { slot } if *slot == target)
    }

    /// Check if this tree contains a specific slot.
    pub fn contains_slot(&self, target: SlotId) -> bool {
        match self {
            LayoutNode::Leaf { slot } => *slot == target,
            LayoutNode::Split { first, second, .. } => {
                first.contains_slot(target) || second.contains_slot(target)
            }
        }
    }
}

/// Layout mode for the workspace grid.
///
/// Supports standard grid configurations, single-pane mode,
/// custom layouts with explicit positioning, and binary split trees.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LayoutMode {
    /// Standard grid layout with specified rows and columns.
    /// Common configurations: 2x2, 1x4, 2x3, 3x3.
    Grid {
        /// Number of rows.
        rows: u32,
        /// Number of columns.
        cols: u32,
    },
    /// Single session takes full window.
    Single,
    /// Custom layout with explicit session positions.
    Custom {
        /// Session positions.
        positions: Vec<(SessionId, GridPosition)>,
    },
    /// Binary split tree layout for arbitrary asymmetric pane arrangements.
    SplitTree {
        /// Root node of the split tree.
        root: LayoutNode,
    },
}

impl Default for LayoutMode {
    fn default() -> Self {
        LayoutMode::Grid { rows: 2, cols: 2 }
    }
}
