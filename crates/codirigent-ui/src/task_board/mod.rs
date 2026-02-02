//! Task board panel component.
//!
//! Provides a task queue management panel at the bottom of the workspace,
//! with tabs for different task states and auto-assignment controls.

mod panel;
mod task_item;

#[cfg(test)]
mod tests;

pub use panel::*;
pub use task_item::*;
