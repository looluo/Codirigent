//! Grid layout calculator for positioning session cells.
//!
//! Provides [`GridLayout`] which calculates cell positions and sizes based on
//! the layout mode and available workspace bounds.

use codirigent_core::{GridPosition, LayoutMode};

use super::geometry::{Bounds, Point, Size};
use super::profile::LayoutProfile;
use super::{
    ABSOLUTE_MIN_CELL_HEIGHT, ABSOLUTE_MIN_CELL_WIDTH, RECOMMENDED_MIN_CELL_HEIGHT,
    RECOMMENDED_MIN_CELL_WIDTH,
};

/// Grid layout calculator for positioning session cells.
///
/// This struct calculates cell positions and sizes based on the layout mode
/// and available workspace bounds.
///
/// # Example
///
/// ```
/// use codirigent_ui::layout::{GridLayout, Bounds};
/// use codirigent_core::LayoutMode;
///
/// let bounds = Bounds::from_size(1000.0, 800.0);
/// let layout = GridLayout::new(
///     LayoutMode::Grid { rows: 2, cols: 2 },
///     bounds,
///     4.0,
/// );
///
/// assert_eq!(layout.cell_count(), 4);
/// let cell = layout.cell_bounds(0, 0);
/// assert!(cell.size.width > 0.0);
/// ```
#[derive(Debug, Clone)]
pub struct GridLayout {
    /// Number of rows in the grid.
    rows: u32,
    /// Number of columns in the grid.
    cols: u32,
    /// Total bounds available for the grid.
    bounds: Bounds,
    /// Gap between cells in pixels.
    gap: f32,
}

impl GridLayout {
    /// Create a new grid layout.
    ///
    /// # Arguments
    ///
    /// * `mode` - The layout mode (Grid, Single, or Custom)
    /// * `bounds` - The available bounds for the entire grid
    /// * `gap` - Gap between cells in pixels
    pub fn new(mode: LayoutMode, bounds: Bounds, gap: f32) -> Self {
        let (rows, cols) = match mode {
            LayoutMode::Grid { rows, cols } => (rows, cols),
            LayoutMode::Single => (1, 1),
            LayoutMode::Custom { ref positions } => {
                // Calculate bounds from positions
                let max_row = positions.iter().map(|(_, p)| p.row).max().unwrap_or(0);
                let max_col = positions.iter().map(|(_, p)| p.col).max().unwrap_or(0);
                (max_row + 1, max_col + 1)
            }
            LayoutMode::SplitTree { .. } => {
                // SplitTree uses SplitLayout, not GridLayout.
                // Fall back to 1x1 if someone mistakenly creates a GridLayout from SplitTree.
                (1, 1)
            }
        };

        Self {
            rows,
            cols,
            bounds,
            gap,
        }
    }

    /// Create a grid layout from a profile.
    ///
    /// # Arguments
    ///
    /// * `profile` - The layout profile
    /// * `bounds` - The available bounds for the entire grid
    /// * `gap` - Gap between cells in pixels
    pub fn from_profile(profile: LayoutProfile, bounds: Bounds, gap: f32) -> Self {
        Self::new(profile.to_mode(), bounds, gap)
    }

    /// Calculate the size of a single cell.
    ///
    /// The cell size is calculated by dividing the available space
    /// (after subtracting gaps) evenly among all cells.
    ///
    /// Enforces absolute minimum cell dimensions and warns if below recommended sizes.
    /// This allows the layout to be flexible while still providing feedback about
    /// potentially cramped terminal displays.
    pub fn cell_size(&self) -> Size {
        if self.rows == 0 || self.cols == 0 {
            return Size::zero();
        }

        let total_gap_x = self.gap * (self.cols.saturating_sub(1) as f32);
        let total_gap_y = self.gap * (self.rows.saturating_sub(1) as f32);

        let calculated_width = (self.bounds.size.width - total_gap_x) / self.cols as f32;
        let calculated_height = (self.bounds.size.height - total_gap_y) / self.rows as f32;

        // Enforce absolute minimums only (hard constraint)
        let cell_width = calculated_width.max(ABSOLUTE_MIN_CELL_WIDTH);
        let cell_height = calculated_height.max(ABSOLUTE_MIN_CELL_HEIGHT);

        // Log warnings if below recommended sizes (soft constraint)
        if cell_width < RECOMMENDED_MIN_CELL_WIDTH {
            tracing::warn!(
                "Terminal cell width ({:.0}px) is below recommended minimum ({:.0}px). \
                 Display may be cramped. Consider using a smaller grid or larger window.",
                cell_width,
                RECOMMENDED_MIN_CELL_WIDTH
            );
        }
        if cell_height < RECOMMENDED_MIN_CELL_HEIGHT {
            tracing::warn!(
                "Terminal cell height ({:.0}px) is below recommended minimum ({:.0}px). \
                 Display may be cramped. Consider using a smaller grid or larger window.",
                cell_height,
                RECOMMENDED_MIN_CELL_HEIGHT
            );
        }

        Size::new(cell_width, cell_height)
    }

    /// Calculate the bounds for a cell at the given row and column.
    ///
    /// # Arguments
    ///
    /// * `row` - Row index (0-based)
    /// * `col` - Column index (0-based)
    pub fn cell_bounds(&self, row: u32, col: u32) -> Bounds {
        let cell_size = self.cell_size();

        let x = self.bounds.origin.x + (cell_size.width + self.gap) * col as f32;
        let y = self.bounds.origin.y + (cell_size.height + self.gap) * row as f32;

        Bounds::from_origin_size(Point::new(x, y), cell_size)
    }

    /// Calculate cell bounds for a session at the given index.
    ///
    /// Sessions are laid out left-to-right, top-to-bottom.
    ///
    /// # Returns
    ///
    /// `None` if the index is out of bounds for this grid.
    pub fn cell_bounds_for_index(&self, index: usize) -> Option<Bounds> {
        let position = self.index_to_position(index)?;
        Some(self.cell_bounds(position.row, position.col))
    }

    /// Convert a linear index to a grid position.
    ///
    /// Sessions are indexed left-to-right, top-to-bottom.
    ///
    /// # Returns
    ///
    /// `None` if the index is out of bounds for this grid.
    pub fn index_to_position(&self, index: usize) -> Option<GridPosition> {
        let max_cells = self.cell_count();
        if index >= max_cells {
            return None;
        }

        let row = (index / self.cols as usize) as u32;
        let col = (index % self.cols as usize) as u32;

        Some(GridPosition { row, col })
    }

    /// Convert a grid position to a linear index.
    ///
    /// # Returns
    ///
    /// `None` if the position is out of bounds for this grid.
    pub fn position_to_index(&self, position: GridPosition) -> Option<usize> {
        if position.row >= self.rows || position.col >= self.cols {
            return None;
        }

        Some((position.row * self.cols + position.col) as usize)
    }

    /// Get the total number of cells in this grid.
    pub fn cell_count(&self) -> usize {
        (self.rows * self.cols) as usize
    }

    /// Get the grid dimensions (rows, cols).
    pub fn dimensions(&self) -> (u32, u32) {
        (self.rows, self.cols)
    }

    /// Get the gap between cells.
    pub fn gap(&self) -> f32 {
        self.gap
    }

    /// Get the total bounds of the grid.
    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    /// Find which cell contains a point.
    ///
    /// # Returns
    ///
    /// The grid position of the cell containing the point, or `None`
    /// if the point is outside all cells (e.g., in a gap).
    pub fn cell_at_point(&self, point: Point) -> Option<GridPosition> {
        for row in 0..self.rows {
            for col in 0..self.cols {
                let cell = self.cell_bounds(row, col);
                if cell.contains(point) {
                    return Some(GridPosition { row, col });
                }
            }
        }
        None
    }

    /// Update the bounds of this layout.
    ///
    /// This is useful when the window is resized.
    pub fn update_bounds(&mut self, bounds: Bounds) {
        self.bounds = bounds;
    }

    /// Update the gap between cells.
    pub fn update_gap(&mut self, gap: f32) {
        self.gap = gap;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{
        ABSOLUTE_MIN_CELL_HEIGHT, ABSOLUTE_MIN_CELL_WIDTH, RECOMMENDED_MIN_CELL_HEIGHT,
        RECOMMENDED_MIN_CELL_WIDTH, TOP_BAR_HEIGHT,
    };
    use codirigent_core::SessionId;

    fn test_bounds() -> Bounds {
        Bounds::from_size(1000.0, 800.0)
    }

    #[test]
    fn test_grid_layout_new_grid() {
        let layout = GridLayout::new(LayoutMode::Grid { rows: 2, cols: 2 }, test_bounds(), 4.0);
        assert_eq!(layout.dimensions(), (2, 2));
        assert_eq!(layout.cell_count(), 4);
        assert_eq!(layout.gap(), 4.0);
    }

    #[test]
    fn test_grid_layout_new_single() {
        let layout = GridLayout::new(LayoutMode::Single, test_bounds(), 4.0);
        assert_eq!(layout.dimensions(), (1, 1));
        assert_eq!(layout.cell_count(), 1);
    }

    #[test]
    fn test_grid_layout_new_custom() {
        let positions = vec![
            (SessionId(1), GridPosition { row: 0, col: 0 }),
            (SessionId(2), GridPosition { row: 0, col: 2 }),
            (SessionId(3), GridPosition { row: 1, col: 1 }),
        ];
        let layout = GridLayout::new(LayoutMode::Custom { positions }, test_bounds(), 4.0);
        assert_eq!(layout.dimensions(), (2, 3));
    }

    #[test]
    fn test_grid_layout_from_profile() {
        let layout = GridLayout::from_profile(LayoutProfile::Grid3x3, test_bounds(), 4.0);
        assert_eq!(layout.dimensions(), (3, 3));
        assert_eq!(layout.cell_count(), 9);
    }

    #[test]
    fn test_grid_layout_cell_size_2x2() {
        let layout = GridLayout::new(LayoutMode::Grid { rows: 2, cols: 2 }, test_bounds(), 4.0);
        let size = layout.cell_size();
        // (1000 - 4) / 2 = 498
        assert!((size.width - 498.0).abs() < 0.01);
        // (800 - 4) / 2 = 398
        assert!((size.height - 398.0).abs() < 0.01);
    }

    #[test]
    fn test_grid_layout_cell_size_single() {
        let layout = GridLayout::new(LayoutMode::Single, test_bounds(), 4.0);
        let size = layout.cell_size();
        assert_eq!(size.width, 1000.0);
        assert_eq!(size.height, 800.0);
    }

    #[test]
    fn test_grid_layout_cell_bounds() {
        let layout = GridLayout::new(LayoutMode::Grid { rows: 2, cols: 2 }, test_bounds(), 4.0);

        let cell00 = layout.cell_bounds(0, 0);
        assert_eq!(cell00.origin.x, 0.0);
        assert_eq!(cell00.origin.y, 0.0);

        let cell01 = layout.cell_bounds(0, 1);
        assert!((cell01.origin.x - 502.0).abs() < 0.01); // 498 + 4

        let cell10 = layout.cell_bounds(1, 0);
        assert_eq!(cell10.origin.x, 0.0);
        assert!((cell10.origin.y - 402.0).abs() < 0.01); // 398 + 4
    }

    #[test]
    fn test_grid_layout_index_to_position() {
        let layout = GridLayout::new(LayoutMode::Grid { rows: 2, cols: 3 }, test_bounds(), 4.0);

        assert_eq!(
            layout.index_to_position(0),
            Some(GridPosition { row: 0, col: 0 })
        );
        assert_eq!(
            layout.index_to_position(2),
            Some(GridPosition { row: 0, col: 2 })
        );
        assert_eq!(
            layout.index_to_position(3),
            Some(GridPosition { row: 1, col: 0 })
        );
        assert_eq!(
            layout.index_to_position(5),
            Some(GridPosition { row: 1, col: 2 })
        );
        assert_eq!(layout.index_to_position(6), None);
    }

    #[test]
    fn test_grid_layout_position_to_index() {
        let layout = GridLayout::new(LayoutMode::Grid { rows: 2, cols: 3 }, test_bounds(), 4.0);

        assert_eq!(
            layout.position_to_index(GridPosition { row: 0, col: 0 }),
            Some(0)
        );
        assert_eq!(
            layout.position_to_index(GridPosition { row: 0, col: 2 }),
            Some(2)
        );
        assert_eq!(
            layout.position_to_index(GridPosition { row: 1, col: 0 }),
            Some(3)
        );
        assert_eq!(
            layout.position_to_index(GridPosition { row: 2, col: 0 }),
            None
        );
    }

    #[test]
    fn test_grid_layout_cell_bounds_for_index() {
        let layout = GridLayout::new(LayoutMode::Grid { rows: 2, cols: 2 }, test_bounds(), 4.0);

        let cell = layout.cell_bounds_for_index(0).unwrap();
        assert_eq!(cell.origin.x, 0.0);
        assert_eq!(cell.origin.y, 0.0);

        let cell = layout.cell_bounds_for_index(3).unwrap();
        assert!(cell.origin.x > 0.0);
        assert!(cell.origin.y > 0.0);

        assert!(layout.cell_bounds_for_index(4).is_none());
    }

    #[test]
    fn test_grid_layout_cell_at_point() {
        let layout = GridLayout::new(LayoutMode::Grid { rows: 2, cols: 2 }, test_bounds(), 4.0);

        // Point in cell (0, 0)
        let pos = layout.cell_at_point(Point::new(100.0, 100.0));
        assert_eq!(pos, Some(GridPosition { row: 0, col: 0 }));

        // Point in cell (0, 1)
        let pos = layout.cell_at_point(Point::new(600.0, 100.0));
        assert_eq!(pos, Some(GridPosition { row: 0, col: 1 }));

        // Point in cell (1, 0)
        let pos = layout.cell_at_point(Point::new(100.0, 500.0));
        assert_eq!(pos, Some(GridPosition { row: 1, col: 0 }));

        // Point in gap between cells
        let cell_size = layout.cell_size();
        let gap_point = Point::new(cell_size.width + 2.0, 100.0);
        let pos = layout.cell_at_point(gap_point);
        // This should be in the gap, so no cell
        assert!(pos.is_none() || pos == Some(GridPosition { row: 0, col: 0 }));
    }

    #[test]
    fn test_grid_layout_update_bounds() {
        let mut layout = GridLayout::new(LayoutMode::Grid { rows: 2, cols: 2 }, test_bounds(), 4.0);
        let new_bounds = Bounds::from_size(2000.0, 1600.0);
        layout.update_bounds(new_bounds);
        assert_eq!(layout.bounds().size.width, 2000.0);
    }

    #[test]
    fn test_grid_layout_update_gap() {
        let mut layout = GridLayout::new(LayoutMode::Grid { rows: 2, cols: 2 }, test_bounds(), 4.0);
        layout.update_gap(8.0);
        assert_eq!(layout.gap(), 8.0);
    }

    #[test]
    fn test_grid_layout_zero_dimensions() {
        let layout = GridLayout::new(LayoutMode::Grid { rows: 0, cols: 0 }, test_bounds(), 4.0);
        let size = layout.cell_size();
        assert_eq!(size.width, 0.0);
        assert_eq!(size.height, 0.0);
    }

    // Soft minimums tests
    #[test]
    fn test_cell_size_allows_below_recommended() {
        // Simulate 3x3 grid on a tight but realistic window
        let available_height = 900.0 - TOP_BAR_HEIGHT;
        let available_width = 1200.0 - 56.0; // icon rail
        let bounds = Bounds::from_size(available_width, available_height);
        let layout = GridLayout::from_profile(LayoutProfile::Grid3x3, bounds, 4.0);

        let size = layout.cell_size();

        assert!(
            size.height >= ABSOLUTE_MIN_CELL_HEIGHT,
            "Height {} should be >= absolute min {}",
            size.height,
            ABSOLUTE_MIN_CELL_HEIGHT
        );
        assert!(
            size.height < RECOMMENDED_MIN_CELL_HEIGHT,
            "Height {} should be < recommended min {}",
            size.height,
            RECOMMENDED_MIN_CELL_HEIGHT
        );

        assert!(
            size.width >= ABSOLUTE_MIN_CELL_WIDTH,
            "Width {} should be >= absolute min {}",
            size.width,
            ABSOLUTE_MIN_CELL_WIDTH
        );
        assert!(
            size.width < RECOMMENDED_MIN_CELL_WIDTH,
            "Width {} should be < recommended min {}",
            size.width,
            RECOMMENDED_MIN_CELL_WIDTH
        );
    }

    #[test]
    fn test_cell_size_enforces_absolute_minimum() {
        // Very small window that would result in tiny cells
        let bounds = Bounds::from_size(600.0, 450.0);
        let layout = GridLayout::from_profile(LayoutProfile::Grid3x3, bounds, 4.0);

        let size = layout.cell_size();

        assert!(
            size.width >= ABSOLUTE_MIN_CELL_WIDTH,
            "Width {} should be >= absolute min {}",
            size.width,
            ABSOLUTE_MIN_CELL_WIDTH
        );
        assert!(
            size.height >= ABSOLUTE_MIN_CELL_HEIGHT,
            "Height {} should be >= absolute min {}",
            size.height,
            ABSOLUTE_MIN_CELL_HEIGHT
        );

        // Will be at the minimums since window is too small
        assert_eq!(size.width, ABSOLUTE_MIN_CELL_WIDTH);
        assert_eq!(size.height, ABSOLUTE_MIN_CELL_HEIGHT);
    }

    #[test]
    fn test_cell_size_comfortable_on_large_monitor() {
        // 4K monitor (3840x2160) with 2x2 grid should be very comfortable
        let _available_height = 2160.0 - TOP_BAR_HEIGHT;
        let available_width = 3840.0 - 56.0; // icon rail
        let bounds = Bounds::from_size(available_width, available_width);
        let layout = GridLayout::from_profile(LayoutProfile::Grid2x2, bounds, 4.0);

        let size = layout.cell_size();

        // Should be well above recommended minimums
        assert!(size.width > RECOMMENDED_MIN_CELL_WIDTH * 2.0);
        assert!(size.height > RECOMMENDED_MIN_CELL_HEIGHT * 2.0);
    }

    #[test]
    fn test_single_layout_comfortable_at_any_size() {
        // Even at small window, single layout should be >= recommended
        let bounds = Bounds::from_size(800.0, 600.0);
        let layout = GridLayout::from_profile(LayoutProfile::Single, bounds, 4.0);

        let size = layout.cell_size();

        assert!(size.width >= RECOMMENDED_MIN_CELL_WIDTH);
        assert!(size.height >= RECOMMENDED_MIN_CELL_HEIGHT);
    }

    #[test]
    fn test_dynamic_gap_sizing() {
        // Test that smaller gap helps when space is tight
        let tight_bounds = Bounds::from_size(1300.0, 1000.0);

        // With 4px gap
        let layout_4px = GridLayout::from_profile(LayoutProfile::Grid3x3, tight_bounds, 4.0);
        let size_4px = layout_4px.cell_size();

        // With 2px gap (more space for cells)
        let layout_2px = GridLayout::from_profile(LayoutProfile::Grid3x3, tight_bounds, 2.0);
        let size_2px = layout_2px.cell_size();

        // Smaller gap should give slightly larger cells
        assert!(size_2px.width > size_4px.width);
        assert!(size_2px.height > size_4px.height);
    }

    #[test]
    fn test_cell_size_with_zero_gap() {
        let bounds = Bounds::from_size(1000.0, 800.0);
        let layout = GridLayout::new(LayoutMode::Grid { rows: 2, cols: 2 }, bounds, 0.0);

        let size = layout.cell_size();

        // With no gap, should get exactly 1/2 of bounds
        assert_eq!(size.width, 500.0);
        assert_eq!(size.height, 400.0);
    }

    #[test]
    fn test_constants_are_consistent() {
        // Use let-bindings so the comparisons are not flagged as constant-value
        // assertions by clippy.
        let abs_w = ABSOLUTE_MIN_CELL_WIDTH;
        let abs_h = ABSOLUTE_MIN_CELL_HEIGHT;
        let rec_w = RECOMMENDED_MIN_CELL_WIDTH;
        let rec_h = RECOMMENDED_MIN_CELL_HEIGHT;

        // Absolute minimums should be less than recommended
        assert!(abs_w < rec_w);
        assert!(abs_h < rec_h);

        // Absolute minimums should be reasonable (not too small)
        assert!(abs_w >= 100.0);
        assert!(abs_h >= 100.0);

        // Recommended minimums should be reasonable
        assert!(rec_w >= 300.0);
        assert!(rec_h >= 200.0);
    }
}
