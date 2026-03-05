//! Layout profile definitions and cycling logic.
//!
//! Provides [`LayoutProfile`] enum with predefined grid configurations
//! and custom user-defined layouts.

use codirigent_core::LayoutMode;

use super::{
    ABSOLUTE_MIN_CELL_HEIGHT, ABSOLUTE_MIN_CELL_WIDTH, MAX_GRID_DIMENSION, MIN_GRID_DIMENSION,
    RECOMMENDED_MIN_CELL_HEIGHT, RECOMMENDED_MIN_CELL_WIDTH, TOP_BAR_HEIGHT,
};

/// Layout profiles for workspace organization.
///
/// Each profile defines a specific grid configuration that determines
/// how sessions are arranged in the workspace. Includes both predefined
/// profiles and custom user-defined layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum LayoutProfile {
    /// 2x2 grid layout (4 sessions).
    #[default]
    Grid2x2,
    /// 1x4 vertical stack layout (4 sessions).
    Stack1x4,
    /// 2x3 grid layout (6 sessions).
    Grid2x3,
    /// 3x3 grid layout (9 sessions).
    Grid3x3,
    /// Single session taking full workspace.
    Single,
    /// Custom grid layout with user-defined dimensions.
    /// Rows and columns must be between 1 and 10.
    Custom {
        /// Number of rows (1-10).
        rows: u32,
        /// Number of columns (1-10).
        cols: u32,
    },
}

impl LayoutProfile {
    /// All predefined profiles in cycling order (excludes Custom).
    pub const PREDEFINED: &'static [LayoutProfile] = &[
        LayoutProfile::Grid2x2,
        LayoutProfile::Stack1x4,
        LayoutProfile::Grid2x3,
        LayoutProfile::Grid3x3,
        LayoutProfile::Single,
    ];

    /// Create a custom layout profile with the given dimensions.
    ///
    /// # Arguments
    ///
    /// * `rows` - Number of rows (1-10)
    /// * `cols` - Number of columns (1-10)
    ///
    /// # Returns
    ///
    /// `Some(LayoutProfile::Custom)` if dimensions are valid, `None` otherwise.
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_ui::layout::LayoutProfile;
    ///
    /// let custom = LayoutProfile::custom(4, 3).unwrap();
    /// assert_eq!(custom.max_sessions(), 12);
    ///
    /// // Invalid dimensions return None
    /// assert!(LayoutProfile::custom(0, 3).is_none());
    /// assert!(LayoutProfile::custom(11, 3).is_none());
    /// ```
    pub fn custom(rows: u32, cols: u32) -> Option<Self> {
        if (MIN_GRID_DIMENSION..=MAX_GRID_DIMENSION).contains(&rows)
            && (MIN_GRID_DIMENSION..=MAX_GRID_DIMENSION).contains(&cols)
        {
            Some(LayoutProfile::Custom { rows, cols })
        } else {
            None
        }
    }

    /// Check if this is a custom layout.
    pub fn is_custom(self) -> bool {
        matches!(self, LayoutProfile::Custom { .. })
    }

    /// Convert this profile to a [`LayoutMode`].
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_ui::layout::LayoutProfile;
    /// use codirigent_core::LayoutMode;
    ///
    /// let mode = LayoutProfile::Grid2x2.to_mode();
    /// assert!(matches!(mode, LayoutMode::Grid { rows: 2, cols: 2 }));
    ///
    /// let custom = LayoutProfile::custom(4, 3).unwrap();
    /// let mode = custom.to_mode();
    /// assert!(matches!(mode, LayoutMode::Grid { rows: 4, cols: 3 }));
    /// ```
    pub fn to_mode(self) -> LayoutMode {
        match self {
            LayoutProfile::Grid2x2 => LayoutMode::Grid { rows: 2, cols: 2 },
            LayoutProfile::Stack1x4 => LayoutMode::Grid { rows: 4, cols: 1 },
            LayoutProfile::Grid2x3 => LayoutMode::Grid { rows: 2, cols: 3 },
            LayoutProfile::Grid3x3 => LayoutMode::Grid { rows: 3, cols: 3 },
            LayoutProfile::Single => LayoutMode::Single,
            LayoutProfile::Custom { rows, cols } => LayoutMode::Grid { rows, cols },
        }
    }

    /// Get the next profile in the cycling order.
    ///
    /// Custom layouts cycle back to Grid2x2.
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_ui::layout::LayoutProfile;
    ///
    /// let mut profile = LayoutProfile::Grid2x2;
    /// profile = profile.next();
    /// assert_eq!(profile, LayoutProfile::Stack1x4);
    ///
    /// // Custom cycles back to Grid2x2
    /// let custom = LayoutProfile::custom(4, 3).unwrap();
    /// assert_eq!(custom.next(), LayoutProfile::Grid2x2);
    /// ```
    pub fn next(self) -> Self {
        match self {
            LayoutProfile::Custom { .. } => LayoutProfile::Grid2x2,
            _ => {
                let profiles = Self::PREDEFINED;
                let current_idx = profiles.iter().position(|&p| p == self).unwrap_or(0);
                let next_idx = (current_idx + 1) % profiles.len();
                profiles[next_idx]
            }
        }
    }

    /// Get the previous profile in the cycling order.
    ///
    /// Custom layouts cycle back to Single.
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_ui::layout::LayoutProfile;
    ///
    /// let mut profile = LayoutProfile::Grid2x2;
    /// profile = profile.previous();
    /// assert_eq!(profile, LayoutProfile::Single);
    ///
    /// // Custom cycles back to Single
    /// let custom = LayoutProfile::custom(4, 3).unwrap();
    /// assert_eq!(custom.previous(), LayoutProfile::Single);
    /// ```
    pub fn previous(self) -> Self {
        match self {
            LayoutProfile::Custom { .. } => LayoutProfile::Single,
            _ => {
                let profiles = Self::PREDEFINED;
                let current_idx = profiles.iter().position(|&p| p == self).unwrap_or(0);
                let prev_idx = if current_idx == 0 {
                    profiles.len() - 1
                } else {
                    current_idx - 1
                };
                profiles[prev_idx]
            }
        }
    }

    /// Get the display name for this layout.
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_ui::layout::LayoutProfile;
    ///
    /// assert_eq!(LayoutProfile::Grid2x2.display_name(), "2x2");
    /// ```
    pub fn display_name(self) -> String {
        match self {
            LayoutProfile::Grid2x2 => "2x2".to_string(),
            LayoutProfile::Stack1x4 => "1x4".to_string(),
            LayoutProfile::Grid2x3 => "2x3".to_string(),
            LayoutProfile::Grid3x3 => "3x3".to_string(),
            LayoutProfile::Single => "Single".to_string(),
            LayoutProfile::Custom { rows, cols } => format!("{}x{}", rows, cols),
        }
    }

    /// Get the maximum number of sessions this layout can display.
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_ui::layout::LayoutProfile;
    ///
    /// assert_eq!(LayoutProfile::Grid2x2.max_sessions(), 4);
    /// assert_eq!(LayoutProfile::Grid3x3.max_sessions(), 9);
    ///
    /// let custom = LayoutProfile::custom(4, 3).unwrap();
    /// assert_eq!(custom.max_sessions(), 12);
    /// ```
    pub fn max_sessions(self) -> usize {
        match self {
            LayoutProfile::Grid2x2 => 4,
            LayoutProfile::Stack1x4 => 4,
            LayoutProfile::Grid2x3 => 6,
            LayoutProfile::Grid3x3 => 9,
            LayoutProfile::Single => 1,
            LayoutProfile::Custom { rows, cols } => (rows * cols) as usize,
        }
    }

    /// Calculate recommended minimum window size for comfortable terminal display.
    ///
    /// Returns (width, height) in pixels based on recommended cell sizes and UI elements.
    /// The window can be made smaller than this, but terminals may be cramped.
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_ui::layout::LayoutProfile;
    ///
    /// let (width, height) = LayoutProfile::Grid2x2.recommended_window_size();
    /// assert!(width > 800.0);  // At least 2 columns of 400px + sidebar
    /// assert!(height > 600.0); // At least 2 rows of 300px + UI chrome
    /// ```
    pub fn recommended_window_size(self) -> (f32, f32) {
        let (rows, cols) = self.dimensions();
        let gap = 4.0;

        // IconRail (56px) is the minimum left chrome; drawer is optional.
        let icon_rail_width = 56.0;
        let min_width = icon_rail_width
            + (RECOMMENDED_MIN_CELL_WIDTH * cols as f32)
            + (gap * (cols - 1) as f32);

        let min_height = TOP_BAR_HEIGHT
            + (RECOMMENDED_MIN_CELL_HEIGHT * rows as f32)
            + (gap * (rows - 1) as f32);

        (min_width, min_height)
    }

    /// Calculate absolute minimum window size for functional (though cramped) display.
    ///
    /// Returns (width, height) in pixels based on absolute minimum cell sizes.
    /// The window should not be made smaller than this.
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_ui::layout::LayoutProfile;
    ///
    /// let (width, height) = LayoutProfile::Grid3x3.minimum_window_size();
    /// // Minimum allows 200x150px cells - functional but very cramped
    /// assert!(width > 600.0);  // At least 3 columns of 200px + sidebar
    /// assert!(height > 450.0); // At least 3 rows of 150px + UI chrome
    /// ```
    pub fn minimum_window_size(self) -> (f32, f32) {
        let (rows, cols) = self.dimensions();
        let gap = 4.0;

        // IconRail (56px) is the minimum left chrome; drawer is optional.
        let icon_rail_width = 56.0;
        let min_width =
            icon_rail_width + (ABSOLUTE_MIN_CELL_WIDTH * cols as f32) + (gap * (cols - 1) as f32);

        let min_height =
            TOP_BAR_HEIGHT + (ABSOLUTE_MIN_CELL_HEIGHT * rows as f32) + (gap * (rows - 1) as f32);

        (min_width, min_height)
    }

    /// Get the grid dimensions (rows, cols) for this layout.
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_ui::layout::LayoutProfile;
    ///
    /// assert_eq!(LayoutProfile::Grid2x3.dimensions(), (2, 3));
    ///
    /// let custom = LayoutProfile::custom(4, 3).unwrap();
    /// assert_eq!(custom.dimensions(), (4, 3));
    /// ```
    pub fn dimensions(self) -> (u32, u32) {
        match self {
            LayoutProfile::Grid2x2 => (2, 2),
            LayoutProfile::Stack1x4 => (4, 1),
            LayoutProfile::Grid2x3 => (2, 3),
            LayoutProfile::Grid3x3 => (3, 3),
            LayoutProfile::Single => (1, 1),
            LayoutProfile::Custom { rows, cols } => (rows, cols),
        }
    }

    /// Try to create a profile from a LayoutMode.
    ///
    /// Returns a predefined profile if the mode matches one, otherwise
    /// returns a Custom profile for valid Grid modes, or None for
    /// Custom LayoutModes (which use session-specific positioning).
    ///
    /// # Example
    ///
    /// ```
    /// use codirigent_ui::layout::LayoutProfile;
    /// use codirigent_core::LayoutMode;
    ///
    /// let mode = LayoutMode::Grid { rows: 2, cols: 2 };
    /// assert_eq!(LayoutProfile::from_mode(&mode), Some(LayoutProfile::Grid2x2));
    ///
    /// let mode = LayoutMode::Single;
    /// assert_eq!(LayoutProfile::from_mode(&mode), Some(LayoutProfile::Single));
    ///
    /// // Non-predefined grids become Custom
    /// let mode = LayoutMode::Grid { rows: 4, cols: 3 };
    /// let profile = LayoutProfile::from_mode(&mode).unwrap();
    /// assert!(profile.is_custom());
    /// assert_eq!(profile.dimensions(), (4, 3));
    /// ```
    pub fn from_mode(mode: &LayoutMode) -> Option<Self> {
        match mode {
            LayoutMode::Grid { rows: 2, cols: 2 } => Some(LayoutProfile::Grid2x2),
            LayoutMode::Grid { rows: 4, cols: 1 } => Some(LayoutProfile::Stack1x4),
            LayoutMode::Grid { rows: 2, cols: 3 } => Some(LayoutProfile::Grid2x3),
            LayoutMode::Grid { rows: 3, cols: 3 } => Some(LayoutProfile::Grid3x3),
            LayoutMode::Single => Some(LayoutProfile::Single),
            LayoutMode::Grid { rows, cols } => LayoutProfile::custom(*rows, *cols),
            LayoutMode::Custom { .. } => None, // Custom positions not supported
            LayoutMode::SplitTree { .. } => None, // Split trees use SplitLayout, not grid profiles
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_profile_default() {
        assert_eq!(LayoutProfile::default(), LayoutProfile::Grid2x2);
    }

    #[test]
    fn test_layout_profile_to_mode() {
        assert!(matches!(
            LayoutProfile::Grid2x2.to_mode(),
            LayoutMode::Grid { rows: 2, cols: 2 }
        ));
        assert!(matches!(
            LayoutProfile::Stack1x4.to_mode(),
            LayoutMode::Grid { rows: 4, cols: 1 }
        ));
        assert!(matches!(
            LayoutProfile::Grid2x3.to_mode(),
            LayoutMode::Grid { rows: 2, cols: 3 }
        ));
        assert!(matches!(
            LayoutProfile::Grid3x3.to_mode(),
            LayoutMode::Grid { rows: 3, cols: 3 }
        ));
        assert!(matches!(
            LayoutProfile::Single.to_mode(),
            LayoutMode::Single
        ));
    }

    #[test]
    fn test_layout_profile_next() {
        let mut profile = LayoutProfile::Grid2x2;
        profile = profile.next();
        assert_eq!(profile, LayoutProfile::Stack1x4);
        profile = profile.next();
        assert_eq!(profile, LayoutProfile::Grid2x3);
        profile = profile.next();
        assert_eq!(profile, LayoutProfile::Grid3x3);
        profile = profile.next();
        assert_eq!(profile, LayoutProfile::Single);
        profile = profile.next();
        assert_eq!(profile, LayoutProfile::Grid2x2); // wrap around
    }

    #[test]
    fn test_layout_profile_previous() {
        let mut profile = LayoutProfile::Grid2x2;
        profile = profile.previous();
        assert_eq!(profile, LayoutProfile::Single);
        profile = profile.previous();
        assert_eq!(profile, LayoutProfile::Grid3x3);
    }

    #[test]
    fn test_layout_profile_display_name() {
        assert_eq!(LayoutProfile::Grid2x2.display_name(), "2x2");
        assert_eq!(LayoutProfile::Stack1x4.display_name(), "1x4");
        assert_eq!(LayoutProfile::Grid2x3.display_name(), "2x3");
        assert_eq!(LayoutProfile::Grid3x3.display_name(), "3x3");
        assert_eq!(LayoutProfile::Single.display_name(), "Single");

        // Custom layout display name
        let custom = LayoutProfile::custom(4, 3).unwrap();
        assert_eq!(custom.display_name(), "4x3");
    }

    #[test]
    fn test_layout_profile_max_sessions() {
        assert_eq!(LayoutProfile::Grid2x2.max_sessions(), 4);
        assert_eq!(LayoutProfile::Stack1x4.max_sessions(), 4);
        assert_eq!(LayoutProfile::Grid2x3.max_sessions(), 6);
        assert_eq!(LayoutProfile::Grid3x3.max_sessions(), 9);
        assert_eq!(LayoutProfile::Single.max_sessions(), 1);
    }

    #[test]
    fn test_layout_profile_dimensions() {
        assert_eq!(LayoutProfile::Grid2x2.dimensions(), (2, 2));
        assert_eq!(LayoutProfile::Stack1x4.dimensions(), (4, 1));
        assert_eq!(LayoutProfile::Grid2x3.dimensions(), (2, 3));
        assert_eq!(LayoutProfile::Grid3x3.dimensions(), (3, 3));
        assert_eq!(LayoutProfile::Single.dimensions(), (1, 1));
    }

    #[test]
    fn test_layout_profile_from_mode() {
        assert_eq!(
            LayoutProfile::from_mode(&LayoutMode::Grid { rows: 2, cols: 2 }),
            Some(LayoutProfile::Grid2x2)
        );
        assert_eq!(
            LayoutProfile::from_mode(&LayoutMode::Single),
            Some(LayoutProfile::Single)
        );

        // Non-predefined grid becomes Custom
        let profile = LayoutProfile::from_mode(&LayoutMode::Grid { rows: 5, cols: 5 }).unwrap();
        assert!(profile.is_custom());
        assert_eq!(profile.dimensions(), (5, 5));

        // Invalid dimensions return None
        assert_eq!(
            LayoutProfile::from_mode(&LayoutMode::Grid { rows: 11, cols: 5 }),
            None
        );
    }

    // Custom layout tests
    #[test]
    fn test_layout_profile_custom_creation() {
        // Valid custom layouts
        let custom = LayoutProfile::custom(4, 3).unwrap();
        assert!(custom.is_custom());
        assert_eq!(custom.dimensions(), (4, 3));
        assert_eq!(custom.max_sessions(), 12);

        // Boundary values
        assert!(LayoutProfile::custom(1, 1).is_some());
        assert!(LayoutProfile::custom(10, 10).is_some());

        // Invalid dimensions
        assert!(LayoutProfile::custom(0, 3).is_none());
        assert!(LayoutProfile::custom(3, 0).is_none());
        assert!(LayoutProfile::custom(11, 3).is_none());
        assert!(LayoutProfile::custom(3, 11).is_none());
    }

    #[test]
    fn test_layout_profile_custom_to_mode() {
        let custom = LayoutProfile::custom(4, 3).unwrap();
        let mode = custom.to_mode();
        assert!(matches!(mode, LayoutMode::Grid { rows: 4, cols: 3 }));
    }

    #[test]
    fn test_layout_profile_custom_cycling() {
        let custom = LayoutProfile::custom(4, 3).unwrap();

        // Custom cycles to Grid2x2 on next
        assert_eq!(custom.next(), LayoutProfile::Grid2x2);

        // Custom cycles to Single on previous
        assert_eq!(custom.previous(), LayoutProfile::Single);
    }

    #[test]
    fn test_layout_profile_is_custom() {
        assert!(!LayoutProfile::Grid2x2.is_custom());
        assert!(!LayoutProfile::Single.is_custom());
        assert!(LayoutProfile::custom(4, 3).unwrap().is_custom());
    }

    #[test]
    fn test_minimum_window_size_uses_absolute() {
        let (width, height) = LayoutProfile::Grid3x3.minimum_window_size();

        // Should use absolute minimums (200x150) with icon rail (56px)
        let icon_rail_width = 56.0;
        let expected_width = icon_rail_width + (ABSOLUTE_MIN_CELL_WIDTH * 3.0) + (4.0 * 2.0);
        let expected_height = TOP_BAR_HEIGHT + (ABSOLUTE_MIN_CELL_HEIGHT * 3.0) + (4.0 * 2.0);

        assert!((width - expected_width).abs() < 0.01);
        assert!((height - expected_height).abs() < 0.01);

        // 3x3 at absolute minimum should fit on even small laptops
        assert!(width < 1000.0);
        assert!(height < 600.0);
    }

    #[test]
    fn test_recommended_window_size_uses_recommended() {
        let (width, height) = LayoutProfile::Grid3x3.recommended_window_size();

        // Should use recommended minimums (400x300) with icon rail (56px)
        let icon_rail_width = 56.0;
        let expected_width = icon_rail_width + (RECOMMENDED_MIN_CELL_WIDTH * 3.0) + (4.0 * 2.0);
        let expected_height = TOP_BAR_HEIGHT + (RECOMMENDED_MIN_CELL_HEIGHT * 3.0) + (4.0 * 2.0);

        assert!((width - expected_width).abs() < 0.01);
        assert!((height - expected_height).abs() < 0.01);

        // 3x3 at recommended should need ~1264x956 window (smaller than before due to less chrome)
        assert!(width > 1200.0);
        assert!(height > 900.0);
    }
}
