//! Grid layout system for Codirigent.
//!
//! This module provides the layout profile and grid calculation system
//! for organizing multiple session panes in the workspace.
//!
//! # Layout Profiles
//!
//! The system supports several predefined layout configurations:
//! - 2x2 Grid: 4 sessions in a square layout (default)
//! - 1x4 Stack: 4 sessions in a vertical column
//! - 2x3 Grid: 6 sessions in 2 rows, 3 columns
//! - 3x3 Grid: 9 sessions in a 3x3 grid
//! - Single: One session takes the full workspace
//! - Custom: User-defined grid with any rows x columns (1-10 each)
//!
//! # Example
//!
//! ```
//! use codirigent_ui::layout::{LayoutProfile, GridLayout, Bounds, Size, Point};
//! use codirigent_core::LayoutMode;
//!
//! let profile = LayoutProfile::Grid2x2;
//! assert_eq!(profile.max_sessions(), 4);
//!
//! // Custom layout example
//! let custom = LayoutProfile::custom(4, 3).unwrap();
//! assert_eq!(custom.max_sessions(), 12);
//!
//! let bounds = Bounds::new(0.0, 0.0, 1000.0, 800.0);
//! let layout = GridLayout::new(profile.to_mode(), bounds, 4.0);
//! assert_eq!(layout.cell_count(), 4);
//! ```

mod geometry;
mod grid;
mod profile;
mod split;
mod state;

// Re-export all public types for backward compatibility.
// All types remain accessible via `crate::layout::TypeName`.

pub use geometry::{Bounds, Point, Size};
pub use grid::GridLayout;
pub use profile::LayoutProfile;
pub use split::{DividerInfo, SplitLayout};
pub use state::{FocusDirection, LayoutState, SplitLayoutState, WorkspaceLayoutState};

/// Recommended minimum cell width in pixels for comfortable terminal display.
/// This is a soft limit - cells can be smaller, but will log warnings.
pub const RECOMMENDED_MIN_CELL_WIDTH: f32 = 400.0;

/// Recommended minimum cell height in pixels for comfortable terminal display.
/// This is a soft limit - cells can be smaller, but will log warnings.
pub const RECOMMENDED_MIN_CELL_HEIGHT: f32 = 300.0;

/// Absolute minimum cell width in pixels for functional terminal display.
/// This is a hard limit - cells will not be smaller than this.
pub const ABSOLUTE_MIN_CELL_WIDTH: f32 = 200.0;

/// Absolute minimum cell height in pixels for functional terminal display.
/// This is a hard limit - cells will not be smaller than this.
pub const ABSOLUTE_MIN_CELL_HEIGHT: f32 = 150.0;

/// Height of the top bar in pixels (replaces TITLE_BAR_HEIGHT + TOOLBAR_HEIGHT).
pub const TOP_BAR_HEIGHT: f32 = 48.0;

/// Width of the right task board panel in pixels.
pub const RIGHT_PANEL_WIDTH: f32 = 288.0;

/// Maximum rows/columns allowed for custom layouts.
pub const MAX_GRID_DIMENSION: u32 = 10;

/// Minimum rows/columns for custom layouts.
pub const MIN_GRID_DIMENSION: u32 = 1;
