//! Settings page module.
//!
//! Provides a full-page overlay settings panel with category sidebar
//! and scrollable content area. Accessed via gear icon or `Ctrl+,`.

pub(crate) mod controls;
mod page;

pub use page::{SettingsCategory, SettingsPage};
