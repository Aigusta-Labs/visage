use std::sync::Arc;
use tokio::sync::Mutex;

use anyhow::{Context, Result};
use tracing_subscriber::EnvFilter;

mod config;
mod dbus_interface;
mod engine;
mod rate_limiter;
mod store;

use config::Config;
use dbus_interface::{AppState, VisageService};
use engine::spawn_engine;
use rate_limiter::RateLimiter;
use store::FaceModelStore;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::info!("visaged starting");

    // 1. Load configuration
    let config = Config::from_env();
    tracing::info!(
        camera = %config.camera_device,
        model_dir = %config.model_dir.display(),
        db_path = %config.db_path.display(),
        threshold = config.similarity_threshold,
        session_bus = config.session_bus,
        "configuration loaded"
    );

    visage_models::verify_models_dir(&config.model_dir)
        .map_err(anyhow::Error::from)
        .with_context(|| {
            format!(
                "model integrity verification failed for {}; run `sudo visage setup` to download verified ONNX models",
                config.model_dir.display()
            )
        })?;

    // 2. Spawn engine (opens camera, loads models — fail-fast)
    let engine = spawn_engine(
        &config.camera_device,
        &config.scrfd_model_path(),
        &config.arcface_model_path(),
        config.warmup_frames,
        config.emitter_enabled,
    )?;
    tracing::info!("engine started");

    // 3. Open face model store (creates DB if needed)
    let store = FaceModelStore::open(&config.db_path).await?;
    let model_count = store.count_all().await.unwrap_or(0);
    tracing::info!(db = %config.db_path.display(), models = model_count, "store opened");

    // 4. Register D-Bus service on system bus (or session bus in development mode).
    //    Set VISAGE_SESSION_BUS=1 to use the session bus without elevated privileges.
    let session_bus = config.session_bus;
    let state = Arc::new(Mutex::new(AppState {
        config,
        engine,
        store,
        rate_limiter: RateLimiter::new(),
    }));

    let service = VisageService { state };

    let _conn = if session_bus {
        zbus::connection::Builder::session()?
    } else {
        zbus::connection::Builder::system()?
    }
    .name("org.freedesktop.Visage1")?
    .serve_at("/org/freedesktop/Visage1", service)?
    .build()
    .await?;

    let bus_name = if session_bus { "session" } else { "system" };
    tracing::info!(
        bus = bus_name,
        "visaged ready — listening on org.freedesktop.Visage1"
    );

    // 5. Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    tracing::info!("visaged shutting down");

    Ok(())
}
