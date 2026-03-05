//! Split tree layout calculator and divider info.
//!
//! Provides [`SplitLayout`] for recursive subdivision of the workspace
//! into asymmetric panes, and [`DividerInfo`] for drag handle hit testing.

use codirigent_core::{LayoutNode, SlotId, SplitDirection};

use super::geometry::{Bounds, Point};
use super::{ABSOLUTE_MIN_CELL_HEIGHT, ABSOLUTE_MIN_CELL_WIDTH};

/// Information about a divider (drag handle) between split children.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DividerInfo {
    /// The slot whose parent split this divider belongs to.
    /// Specifically, the first child's leftmost/topmost slot.
    pub first_slot: SlotId,
    /// The slot on the other side of the divider.
    pub second_slot: SlotId,
    /// Direction of the split (determines whether this is a vertical or horizontal bar).
    pub direction: SplitDirection,
    /// The bounds of the divider hit area.
    pub bounds: Bounds,
}

/// Layout calculator for split tree layouts.
///
/// Sits alongside `GridLayout` — used when the workspace is in `SplitTree` mode.
/// Performs recursive subdivision to compute pixel bounds for each leaf.
#[derive(Debug, Clone)]
pub struct SplitLayout {
    /// The root of the split tree.
    root: LayoutNode,
    /// Total bounds available for the layout.
    bounds: Bounds,
    /// Gap between panes in pixels.
    gap: f32,
}

impl SplitLayout {
    /// Create a new split layout.
    pub fn new(root: LayoutNode, bounds: Bounds, gap: f32) -> Self {
        Self { root, bounds, gap }
    }

    /// Compute pixel bounds for all leaf slots.
    ///
    /// Returns pairs of `(SlotId, Bounds)` in DFS order.
    pub fn leaf_bounds(&self) -> Vec<(SlotId, Bounds)> {
        let mut result = Vec::new();
        self.compute_bounds(&self.root, self.bounds, &mut result);
        result
    }

    fn compute_bounds(
        &self,
        node: &LayoutNode,
        available: Bounds,
        out: &mut Vec<(SlotId, Bounds)>,
    ) {
        match node {
            LayoutNode::Leaf { slot } => {
                // Enforce minimum cell sizes
                let clamped = Bounds::new(
                    available.origin.x,
                    available.origin.y,
                    available.size.width.max(ABSOLUTE_MIN_CELL_WIDTH),
                    available.size.height.max(ABSOLUTE_MIN_CELL_HEIGHT),
                );
                out.push((*slot, clamped));
            }
            LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let (first_bounds, second_bounds) = match direction {
                    SplitDirection::Horizontal => {
                        let total_w = available.size.width - self.gap;
                        let first_w = (total_w * ratio).max(0.0);
                        let second_w = (total_w - first_w).max(0.0);
                        (
                            Bounds::new(
                                available.origin.x,
                                available.origin.y,
                                first_w,
                                available.size.height,
                            ),
                            Bounds::new(
                                available.origin.x + first_w + self.gap,
                                available.origin.y,
                                second_w,
                                available.size.height,
                            ),
                        )
                    }
                    SplitDirection::Vertical => {
                        let total_h = available.size.height - self.gap;
                        let first_h = (total_h * ratio).max(0.0);
                        let second_h = (total_h - first_h).max(0.0);
                        (
                            Bounds::new(
                                available.origin.x,
                                available.origin.y,
                                available.size.width,
                                first_h,
                            ),
                            Bounds::new(
                                available.origin.x,
                                available.origin.y + first_h + self.gap,
                                available.size.width,
                                second_h,
                            ),
                        )
                    }
                };
                self.compute_bounds(first, first_bounds, out);
                self.compute_bounds(second, second_bounds, out);
            }
        }
    }

    /// Find which slot contains a given point.
    pub fn slot_at_point(&self, point: Point) -> Option<SlotId> {
        for (slot, bounds) in self.leaf_bounds() {
            if bounds.contains(point) {
                return Some(slot);
            }
        }
        None
    }

    /// Find a divider (drag handle) near a given point.
    ///
    /// Returns info about the closest divider within the gap region.
    pub fn divider_at_point(&self, point: Point) -> Option<DividerInfo> {
        let mut result = None;
        self.find_divider(&self.root, self.bounds, point, &mut result);
        result
    }

    fn find_divider(
        &self,
        node: &LayoutNode,
        available: Bounds,
        point: Point,
        out: &mut Option<DividerInfo>,
    ) {
        let LayoutNode::Split {
            direction,
            ratio,
            first,
            second,
        } = node
        else {
            return;
        };

        let first_slots = first.slots_in_order();
        let second_slots = second.slots_in_order();
        let first_slot = first_slots.first().copied().unwrap_or(SlotId(0));
        let second_slot = second_slots.first().copied().unwrap_or(SlotId(0));

        match direction {
            SplitDirection::Horizontal => {
                let total_w = available.size.width - self.gap;
                let first_w = total_w * ratio;
                let divider_x = available.origin.x + first_w;
                let divider_bounds = Bounds::new(
                    divider_x,
                    available.origin.y,
                    self.gap,
                    available.size.height,
                );
                if divider_bounds.contains(point) {
                    *out = Some(DividerInfo {
                        first_slot,
                        second_slot,
                        direction: *direction,
                        bounds: divider_bounds,
                    });
                    return;
                }
                // Recurse into children
                let first_bounds = Bounds::new(
                    available.origin.x,
                    available.origin.y,
                    first_w,
                    available.size.height,
                );
                let second_bounds = Bounds::new(
                    divider_x + self.gap,
                    available.origin.y,
                    (total_w - first_w).max(0.0),
                    available.size.height,
                );
                self.find_divider(first, first_bounds, point, out);
                if out.is_none() {
                    self.find_divider(second, second_bounds, point, out);
                }
            }
            SplitDirection::Vertical => {
                let total_h = available.size.height - self.gap;
                let first_h = total_h * ratio;
                let divider_y = available.origin.y + first_h;
                let divider_bounds = Bounds::new(
                    available.origin.x,
                    divider_y,
                    available.size.width,
                    self.gap,
                );
                if divider_bounds.contains(point) {
                    *out = Some(DividerInfo {
                        first_slot,
                        second_slot,
                        direction: *direction,
                        bounds: divider_bounds,
                    });
                    return;
                }
                // Recurse into children
                let first_bounds = Bounds::new(
                    available.origin.x,
                    available.origin.y,
                    available.size.width,
                    first_h,
                );
                let second_bounds = Bounds::new(
                    available.origin.x,
                    divider_y + self.gap,
                    available.size.width,
                    (total_h - first_h).max(0.0),
                );
                self.find_divider(first, first_bounds, point, out);
                if out.is_none() {
                    self.find_divider(second, second_bounds, point, out);
                }
            }
        }
    }

    /// Get the root node.
    pub fn root(&self) -> &LayoutNode {
        &self.root
    }

    /// Get the bounds.
    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    /// Get the gap.
    pub fn gap(&self) -> f32 {
        self.gap
    }

    /// Update the bounds (e.g., on window resize).
    pub fn update_bounds(&mut self, bounds: Bounds) {
        self.bounds = bounds;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::GridLayout;
    use codirigent_core::LayoutMode;

    fn test_bounds() -> Bounds {
        Bounds::from_size(1000.0, 800.0)
    }

    #[test]
    fn test_split_layout_single_leaf() {
        let root = LayoutNode::Leaf { slot: SlotId(0) };
        let layout = SplitLayout::new(root, test_bounds(), 4.0);
        let leaves = layout.leaf_bounds();
        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].0, SlotId(0));
        assert_eq!(leaves[0].1.size.width, 1000.0);
        assert_eq!(leaves[0].1.size.height, 800.0);
    }

    #[test]
    fn test_split_layout_horizontal_split() {
        let root = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf { slot: SlotId(0) }),
            second: Box::new(LayoutNode::Leaf { slot: SlotId(1) }),
        };
        let layout = SplitLayout::new(root, Bounds::from_size(1000.0, 800.0), 4.0);
        let leaves = layout.leaf_bounds();

        assert_eq!(leaves.len(), 2);
        // First child: 0.5 * (1000 - 4) = 498
        assert!((leaves[0].1.size.width - 498.0).abs() < 0.01);
        // Second child: (1000 - 4) - 498 = 498
        assert!((leaves[1].1.size.width - 498.0).abs() < 0.01);
        // Both full height
        assert_eq!(leaves[0].1.size.height, 800.0);
        assert_eq!(leaves[1].1.size.height, 800.0);
        // Second child starts after first + gap
        assert!((leaves[1].1.origin.x - 502.0).abs() < 0.01);
    }

    #[test]
    fn test_split_layout_vertical_split() {
        let root = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf { slot: SlotId(0) }),
            second: Box::new(LayoutNode::Leaf { slot: SlotId(1) }),
        };
        let layout = SplitLayout::new(root, Bounds::from_size(1000.0, 800.0), 4.0);
        let leaves = layout.leaf_bounds();

        assert_eq!(leaves.len(), 2);
        // First child: 0.5 * (800 - 4) = 398
        assert!((leaves[0].1.size.height - 398.0).abs() < 0.01);
        // Second child: (800 - 4) - 398 = 398
        assert!((leaves[1].1.size.height - 398.0).abs() < 0.01);
        // Both full width
        assert_eq!(leaves[0].1.size.width, 1000.0);
        assert_eq!(leaves[1].1.size.width, 1000.0);
    }

    #[test]
    fn test_split_layout_grid_2x2_matches_grid_layout() {
        // A 2x2 grid-equivalent tree should produce similar bounds to GridLayout.
        let tree = LayoutNode::from_grid(2, 2);
        let bounds = Bounds::from_size(1000.0, 800.0);
        let gap = 4.0;

        let split_layout = SplitLayout::new(tree, bounds, gap);
        let split_leaves = split_layout.leaf_bounds();

        let grid_layout = GridLayout::new(LayoutMode::Grid { rows: 2, cols: 2 }, bounds, gap);

        // There should be 4 leaves
        assert_eq!(split_leaves.len(), 4);

        // Compare bounds for each cell
        for (i, (slot_id, split_bounds)) in split_leaves.iter().enumerate() {
            assert_eq!(slot_id.0, i as u32);
            let grid_bounds = grid_layout.cell_bounds_for_index(i).unwrap();

            // Widths and heights should match within floating-point tolerance
            assert!(
                (split_bounds.size.width - grid_bounds.size.width).abs() < 1.0,
                "Cell {} width mismatch: split={}, grid={}",
                i,
                split_bounds.size.width,
                grid_bounds.size.width
            );
            assert!(
                (split_bounds.size.height - grid_bounds.size.height).abs() < 1.0,
                "Cell {} height mismatch: split={}, grid={}",
                i,
                split_bounds.size.height,
                grid_bounds.size.height
            );
        }
    }

    #[test]
    fn test_split_layout_grid_1x4_matches_grid_layout() {
        let tree = LayoutNode::from_grid(1, 4);
        let bounds = Bounds::from_size(2000.0, 800.0);
        let gap = 4.0;

        let split_layout = SplitLayout::new(tree, bounds, gap);
        let split_leaves = split_layout.leaf_bounds();

        let grid_layout = GridLayout::new(LayoutMode::Grid { rows: 1, cols: 4 }, bounds, gap);

        assert_eq!(split_leaves.len(), 4);
        for (i, (_slot, split_b)) in split_leaves.iter().enumerate() {
            let grid_b = grid_layout.cell_bounds_for_index(i).unwrap();
            // Binary tree splits introduce small rounding differences
            // due to nested gap subtraction, so we allow up to 3px tolerance.
            assert!(
                (split_b.size.width - grid_b.size.width).abs() < 3.0,
                "Cell {} width mismatch: split={}, grid={}",
                i,
                split_b.size.width,
                grid_b.size.width
            );
        }
    }

    #[test]
    fn test_split_layout_slot_at_point() {
        let root = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf { slot: SlotId(0) }),
            second: Box::new(LayoutNode::Leaf { slot: SlotId(1) }),
        };
        let layout = SplitLayout::new(root, Bounds::from_size(1000.0, 800.0), 4.0);

        // Point in left half
        assert_eq!(
            layout.slot_at_point(Point::new(100.0, 400.0)),
            Some(SlotId(0))
        );
        // Point in right half
        assert_eq!(
            layout.slot_at_point(Point::new(700.0, 400.0)),
            Some(SlotId(1))
        );
        // Point outside
        assert_eq!(layout.slot_at_point(Point::new(1100.0, 400.0)), None);
    }

    #[test]
    fn test_split_layout_divider_at_point() {
        let root = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf { slot: SlotId(0) }),
            second: Box::new(LayoutNode::Leaf { slot: SlotId(1) }),
        };
        let layout = SplitLayout::new(root, Bounds::from_size(1000.0, 800.0), 4.0);

        // The divider should be at x = 498 (0.5 * (1000-4)), width = 4
        let divider = layout.divider_at_point(Point::new(499.0, 400.0));
        assert!(divider.is_some());
        let d = divider.unwrap();
        assert_eq!(d.first_slot, SlotId(0));
        assert_eq!(d.second_slot, SlotId(1));
        assert_eq!(d.direction, SplitDirection::Horizontal);

        // Point not on divider
        assert!(layout.divider_at_point(Point::new(100.0, 400.0)).is_none());
    }

    #[test]
    fn test_split_layout_asymmetric() {
        // 2 stacked on left + 1 full-height on right
        // H(0.5, V(0.5, slot0, slot1), slot2)
        let root = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf { slot: SlotId(0) }),
                second: Box::new(LayoutNode::Leaf { slot: SlotId(1) }),
            }),
            second: Box::new(LayoutNode::Leaf { slot: SlotId(2) }),
        };
        let layout = SplitLayout::new(root, Bounds::from_size(1000.0, 800.0), 4.0);
        let leaves = layout.leaf_bounds();

        assert_eq!(leaves.len(), 3);
        // Slot 0: top-left
        assert_eq!(leaves[0].0, SlotId(0));
        // Slot 1: bottom-left
        assert_eq!(leaves[1].0, SlotId(1));
        // Slot 2: full right side
        assert_eq!(leaves[2].0, SlotId(2));
        // Right pane should be full height
        assert_eq!(leaves[2].1.size.height, 800.0);
        // Left panes should be half height each (minus gap)
        assert!((leaves[0].1.size.height - leaves[1].1.size.height).abs() < 1.0);
    }

    #[test]
    fn test_split_layout_update_bounds() {
        let root = LayoutNode::Leaf { slot: SlotId(0) };
        let mut layout = SplitLayout::new(root, Bounds::from_size(1000.0, 800.0), 4.0);
        layout.update_bounds(Bounds::from_size(2000.0, 1600.0));
        let leaves = layout.leaf_bounds();
        assert_eq!(leaves[0].1.size.width, 2000.0);
        assert_eq!(leaves[0].1.size.height, 1600.0);
    }

    #[test]
    fn test_split_layout_accessors() {
        let root = LayoutNode::Leaf { slot: SlotId(0) };
        let layout = SplitLayout::new(root.clone(), Bounds::from_size(100.0, 100.0), 2.0);
        assert_eq!(*layout.root(), root);
        assert_eq!(layout.bounds().size.width, 100.0);
        assert_eq!(layout.gap(), 2.0);
    }
}
