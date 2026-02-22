use anyhow::Result;
use tracing_subscriber::EnvFilter;

mod dbus_interface;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::info!("visaged starting");

    // STEP 3: Initialize camera, load models, register D-Bus interface, enter main loop
    // Design documented in docs/architecture.md and ADR 003

    tracing::info!("visaged ready");

    // Keep running until signaled
    tokio::signal::ctrl_c().await?;
    tracing::info!("visaged shutting down");

    Ok(())
}
