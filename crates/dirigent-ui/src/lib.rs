//! Dirigent UI
//!
//! GPUI-based user interface crate providing the main application window,
//! grid layout system, terminal rendering, session sidebar, and theming
//! for Dirigent.
//!
//! # Modules
//!
//! - [`layout`] - Grid layout system with profiles and cell calculation
//! - [`theme`] - Color and theming system (HSLA-based)
//! - [`theme_config`] - Serializable theme configuration (JSON-based)
//! - [`theme_manager`] - Theme loading and management
//! - [`keybindings`] - Keyboard shortcut management
//! - [`layout_profile`] - Saved layout profile management
//! - [`workspace`] - Main workspace view managing sessions
//! - [`sidebar`] - Session sidebar with grouping and status indicators
//! - [`actions`] - UI action definitions and keybindings
//!
//! # Layout System
//!
//! The layout system supports multiple predefined profiles for organizing
//! session panes in the workspace:
//!
//! - 2x2 Grid (default): 4 sessions in a square layout
//! - 1x4 Stack: 4 sessions in a vertical column
//! - 2x3 Grid: 6 sessions in 2 rows, 3 columns
//! - 3x3 Grid: 9 sessions in a 3x3 grid
//! - Single: One session takes the full workspace
//!
//! # Keybinding System
//!
//! The keybinding system supports configurable keyboard shortcuts with:
//! - Modifier keys (Ctrl, Alt, Shift, Cmd)
//! - Parsing from strings like "Cmd+Shift+N"
//! - Default bindings per the Dirigent spec
//! - Custom action bindings including plugins
//!
//! # Theme System
//!
//! Two complementary theme systems are provided:
//! - [`theme`] - HSLA colors for runtime rendering
//! - [`theme_config`] - Hex color strings for serialization
//!
//! # Example
//!
//! ```
//! use dirigent_ui::{
//!     layout::{LayoutProfile, GridLayout, Bounds},
//!     theme::DirigentTheme,
//!     workspace::Workspace,
//! };
//! use dirigent_core::SessionId;
//!
//! // Create a workspace
//! let mut workspace = Workspace::new();
//! workspace.set_layout(LayoutProfile::Grid2x2);
//!
//! // Get layout information
//! let profile = workspace.layout_profile();
//! assert_eq!(profile.max_sessions(), 4);
//!
//! // Create a grid layout calculator
//! let bounds = Bounds::from_size(1000.0, 800.0);
//! let grid = GridLayout::from_profile(profile, bounds, 4.0);
//! assert_eq!(grid.cell_count(), 4);
//!
//! // Get theme colors
//! let theme = DirigentTheme::default();
//! let status_color = theme.status_color(dirigent_core::SessionStatus::Working);
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

// Core modules (ready)
pub mod actions;
pub mod integration;
pub mod keybindings;
pub mod layout;
pub mod layout_profile;
pub mod sidebar;
pub mod theme;
pub mod theme_config;
pub mod theme_manager;
pub mod workspace;

// Modules that require dependencies not yet available
// TODO(Stage 12+): Enable when GPUI/alacritty_terminal are available
// pub mod app;
// pub mod clipboard;
// pub mod input;
// pub mod terminal;
// pub mod terminal_colors;
// pub mod terminal_view;

// Re-export commonly used items
pub use actions::{
    CloseSession, CreateSession, FocusNextSession, FocusPreviousSession, FocusSession, NextLayout,
    PreviousLayout, Quit, ToggleSidebar,
};
pub use layout::{Bounds, FocusDirection, GridLayout, LayoutProfile, LayoutState, Point, Size};
pub use sidebar::{
    SessionGroup, SessionSidebar, SidebarEvent, SidebarItem, SidebarRenderHints, StatusColors,
};
pub use theme::{DirigentTheme, Hsla, Rgba};
pub use workspace::{CellInfo, Workspace};
pub use integration::{DirigentIntegration, IntegrationConfig};

// Re-export new advanced UI modules
pub use keybindings::{Action, KeybindingManager, Modifiers};
pub use keybindings::KeyBinding as AdvancedKeyBinding;
pub use layout_profile::{LayoutProfileManager, SavedLayoutProfile};
pub use theme_config::{Theme as ConfigTheme, ThemeColors, ThemeSpacing, ThemeTypography, TerminalColors as ConfigTerminalColors};
pub use theme_manager::ThemeManager;
