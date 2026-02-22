use std::sync::Arc;
use tokio::sync::Mutex;
use zbus::interface;

use crate::config::Config;
use crate::engine::EngineHandle;
use crate::rate_limiter::RateLimiter;
use crate::store::FaceModelStore;

/// Shared state accessible by D-Bus method handlers.
pub struct AppState {
    pub config: Config,
    pub engine: EngineHandle,
    pub store: FaceModelStore,
    pub rate_limiter: RateLimiter,
}

/// D-Bus interface for the Visage biometric daemon.
///
/// Bus name: org.freedesktop.Visage1
/// Object path: /org/freedesktop/Visage1
pub struct VisageService {
    pub state: Arc<Mutex<AppState>>,
}

/// Retrieve the UID of the D-Bus peer identified by `sender_str` (a unique bus name).
async fn get_caller_uid(
    sender_str: &str,
    conn: &zbus::Connection,
) -> zbus::fdo::Result<u32> {
    let dbus_proxy = zbus::fdo::DBusProxy::new(conn)
        .await
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
    let bus_name = zbus::names::BusName::try_from(sender_str)
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
    dbus_proxy
        .get_connection_unix_user(bus_name)
        .await
        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
}

/// Look up the numeric UID for a local username by parsing `/etc/passwd`.
fn uid_for_name(name: &str) -> Option<u32> {
    let contents = std::fs::read_to_string("/etc/passwd").ok()?;
    for line in contents.lines() {
        let mut parts = line.split(':');
        let uname = parts.next()?;
        if uname != name {
            continue;
        }
        parts.next(); // password field
        let uid_str = parts.next()?;
        return uid_str.parse().ok();
    }
    None
}

#[interface(name = "org.freedesktop.Visage1")]
impl VisageService {
    /// Enroll a new face model for the given user.
    ///
    /// Returns the UUID of the newly created model.
    async fn enroll(&self, user: &str, label: &str) -> zbus::fdo::Result<String> {
        tracing::info!(user, label, "enroll requested");

        // Copy values while holding lock, then release
        let (engine, frames_count) = {
            let state = self.state.lock().await;
            (state.engine.clone(), state.config.frames_per_enroll)
        };

        // Run engine (no lock held)
        let result = engine.enroll(frames_count).await.map_err(|e| {
            tracing::error!(error = %e, "enroll failed");
            zbus::fdo::Error::Failed(e.to_string())
        })?;

        tracing::info!(quality = result.quality_score, "enroll: embedding extracted");

        // Store result (re-acquire lock)
        let state = self.state.lock().await;
        let model_id = state
            .store
            .insert(user, label, &result.embedding, result.quality_score)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "enroll: store insert failed");
                zbus::fdo::Error::Failed(e.to_string())
            })?;

        tracing::info!(model_id = %model_id, user, label, "enrolled successfully");
        Ok(model_id)
    }

    /// Verify the current face against enrolled models for the given user.
    ///
    /// Returns true if the face matches any enrolled model above the threshold.
    ///
    /// Security: on the system bus the caller UID is validated against the target
    /// username before any camera access or rate-limit check.  Root (UID 0) is always
    /// permitted.  On the session bus (development mode) UID validation is skipped.
    async fn verify(
        &self,
        user: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(connection)] conn: &zbus::Connection,
    ) -> zbus::fdo::Result<bool> {
        tracing::info!(user, "verify requested");

        // Read session_bus flag without holding lock across the async UID lookup
        let session_bus = self.state.lock().await.config.session_bus;

        // --- UID validation (system bus only) ---
        if !session_bus {
            let sender = header
                .sender()
                .ok_or_else(|| zbus::fdo::Error::Failed("no sender in message".to_string()))?;
            let caller_uid = get_caller_uid(sender.as_str(), conn).await?;
            if caller_uid != 0 {
                match uid_for_name(user) {
                    Some(expected_uid) if caller_uid == expected_uid => {}
                    Some(_) => {
                        tracing::warn!(
                            user,
                            caller_uid,
                            "verify: caller UID does not match target user UID"
                        );
                        return Err(zbus::fdo::Error::AccessDenied(format!(
                            "caller is not permitted to verify user '{user}'"
                        )));
                    }
                    None => {
                        tracing::warn!(user, "verify: unknown user");
                        return Err(zbus::fdo::Error::Failed(format!(
                            "unknown user '{user}'"
                        )));
                    }
                }
            }
        }

        // --- Rate limit check ---
        {
            let mut state = self.state.lock().await;
            state.rate_limiter.check(user).map_err(|msg| {
                tracing::warn!(user, "verify: rate limited");
                zbus::fdo::Error::Failed(msg)
            })?;
        }

        // --- Fetch gallery and config (release lock before engine call) ---
        let (engine, gallery, threshold, frames_count, timeout_secs) = {
            let state = self.state.lock().await;
            let gallery = state.store.get_gallery_for_user(user).await.map_err(|e| {
                tracing::error!(error = %e, "verify: gallery fetch failed");
                zbus::fdo::Error::Failed(e.to_string())
            })?;
            (
                state.engine.clone(),
                gallery,
                state.config.similarity_threshold,
                state.config.frames_per_verify,
                state.config.verify_timeout_secs,
            )
        };

        if gallery.is_empty() {
            tracing::warn!(user, "verify: no enrolled models");
            return Err(zbus::fdo::Error::Failed(format!(
                "no enrolled models for user '{user}'"
            )));
        }

        // --- Run engine with timeout (no lock held) ---
        // Engine errors (camera failure, timeout) are returned here as Err and do NOT
        // count as rate-limit failures — only a deliberate face-not-matched result does.
        let timeout = std::time::Duration::from_secs(timeout_secs);
        let result = engine
            .verify(gallery, threshold, frames_count, timeout)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "verify failed");
                zbus::fdo::Error::Failed(e.to_string())
            })?;

        // --- Record rate-limit outcome ---
        {
            let mut state = self.state.lock().await;
            if result.result.matched {
                state.rate_limiter.record_success(user);
            } else {
                state.rate_limiter.record_failure(user);
            }
        }

        tracing::info!(
            user,
            matched = result.result.matched,
            similarity = result.result.similarity,
            model_id = ?result.result.model_id,
            "verify complete"
        );

        Ok(result.result.matched)
    }

    /// Return daemon status information as JSON.
    async fn status(&self) -> zbus::fdo::Result<String> {
        let state = self.state.lock().await;
        let model_count = state.store.count_all().await.unwrap_or(0);

        Ok(serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "camera": state.config.camera_device,
            "models_enrolled": model_count,
            "similarity_threshold": state.config.similarity_threshold,
        })
        .to_string())
    }

    /// List enrolled face models for the given user as JSON.
    async fn list_models(&self, user: &str) -> zbus::fdo::Result<String> {
        tracing::info!(user, "list_models requested");
        let state = self.state.lock().await;
        let models = state
            .store
            .list_by_user(user)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        serde_json::to_string(&models).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    /// Remove an enrolled face model by ID (scoped to user).
    async fn remove_model(&self, user: &str, model_id: &str) -> zbus::fdo::Result<bool> {
        tracing::info!(user, model_id, "remove_model requested");
        let state = self.state.lock().await;
        let removed = state
            .store
            .remove(user, model_id)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        if removed {
            tracing::info!(model_id, "model removed");
        } else {
            tracing::warn!(model_id, user, "model not found or not owned by user");
        }
        Ok(removed)
    }
}
