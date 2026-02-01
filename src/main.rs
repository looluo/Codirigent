//! Dirigent - AI Coding Agent Orchestration IDE
//!
//! Entry point for the Dirigent application.

use anyhow::Result;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("Starting Dirigent...");

    // TODO: Initialize application
    // This will be implemented in Stage 9 (GPUI Application)

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
