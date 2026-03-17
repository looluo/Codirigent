//! Codirigent Updater
//!
//! Automatic update checking and installation for Codirigent.
//!
//! This crate provides:
//! - Background version checking against GitHub Releases
//! - Artifact downloading with SHA256 verification
//! - Platform-specific update application (macOS DMG, Windows MSI)
//!
//! # Overview
//!
//! The updater checks `api.github.com/repos/oso95/Codirigent/releases/latest`
//! on startup and every 24 hours. When a newer stable version is found, it
//! publishes an `UpdateAvailable` event on the EventBus. The UI shows a toast
//! notification, and the user can choose when to download and apply the update.
//!
//! # Modules
//!
//! - [`checker`] - GitHub Releases API polling and semver comparison
//! - [`downloader`] - Artifact download and SHA256 verification
//! - [`service`] - Update state machine and orchestration
//! - [`platform`] - Platform-specific update application

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod checker;
pub mod downloader;
pub mod platform;
pub mod service;
pub mod state;

pub use checker::UpdateInfo;
pub use service::{StagedUpdate, UpdateService, UpdateState};
pub use state::{StagedUpdateState, UpdatePersistentState};
