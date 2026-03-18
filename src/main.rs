#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Codirigent - AI Coding Agent Orchestration IDE
//!
//! Entry point for the Codirigent application.
//!
//! # Features
//!
//! - `gpui-full`: Enable the full GPUI-based user interface
//! - `terminal`: Enable terminal rendering with alacritty_terminal
//!
//! # Running
//!
//! ```bash
//! # Run with GPUI interface
//! cargo run --features gpui-full
//!
//! # Run with GPUI and terminal support
//! cargo run --features "gpui-full,terminal"
//! ```

use anyhow::Result;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Install a panic hook that writes to a log file so panics are not lost
/// in release builds where `windows_subsystem = "windows"` discards stderr.
fn install_panic_log_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Write to crash log in the platform data directory
        if let Some(data_dir) = dirs::data_dir().map(|d| d.join("Codirigent")) {
            let _ = std::fs::create_dir_all(&data_dir);
            let crash_path = data_dir.join("crash.log");
            let msg = format!(
                "[{}] PANIC: {}\n",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                info,
            );
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&crash_path)
                .and_then(|mut f| std::io::Write::write_all(&mut f, msg.as_bytes()));
        }
        // Also call the default hook (prints to stderr when available)
        default_hook(info);
    }));
}

fn main() -> Result<()> {
    install_panic_log_hook();

    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("Starting Codirigent...");

    // Launch GPUI application if feature is enabled
    #[cfg(feature = "gpui-full")]
    {
        tracing::info!("Launching GPUI application...");
        codirigent_ui::CodirigentApp::new().run();
    }

    // Without GPUI, just print info and exit
    #[cfg(not(feature = "gpui-full"))]
    {
        tracing::info!("Codirigent started without GPUI.");
        tracing::info!("To launch the GUI, run: cargo run --features gpui-full");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use tracing_subscriber::layer::SubscriberExt;

    #[test]
    fn test_tracing_subscriber_builds() {
        // Verify tracing subscriber can be built without panicking
        // Note: We don't call .init() because global subscriber can only be set once
        let result = tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer())
            .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_default());
        // If we get here without panic, the subscriber built successfully
        drop(result);
    }
}
