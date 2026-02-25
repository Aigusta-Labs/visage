use std::path::PathBuf;

/// Daemon configuration, loaded from environment variables.
pub struct Config {
    /// V4L2 device path (default: /dev/video2).
    pub camera_device: String,
    /// Directory containing ONNX model files.
    pub model_dir: PathBuf,
    /// Path to the SQLite database file.
    pub db_path: PathBuf,
    /// Cosine similarity threshold for a positive match.
    pub similarity_threshold: f32,
    /// Timeout in seconds for a verify operation.
    pub verify_timeout_secs: u64,
    /// Number of warmup frames to discard at startup (camera AGC/AE stabilization).
    pub warmup_frames: usize,
    /// Number of frames to capture per verify attempt.
    pub frames_per_verify: usize,
    /// Number of frames to capture per enroll attempt.
    pub frames_per_enroll: usize,
    /// Whether to activate the IR emitter around each capture sequence.
    pub emitter_enabled: bool,
    /// Whether passive liveness detection (landmark stability) is enabled.
    pub liveness_enabled: bool,
    /// Minimum mean eye landmark displacement (pixels) for liveness check.
    /// Lower values are more permissive; higher values reject more aggressively.
    /// Only used when `liveness_enabled` is true.
    pub liveness_min_displacement: f32,
    /// Whether the daemon is running on the session bus (development mode).
    /// UID validation is skipped on the session bus — all callers share the same user.
    pub session_bus: bool,
}

impl Config {
    /// Load configuration from `VISAGE_*` environment variables with defaults.
    pub fn from_env() -> Self {
        let model_dir = std::env::var("VISAGE_MODEL_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| visage_core::default_model_dir());

        let data_dir = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                PathBuf::from(home).join(".local/share")
            })
            .join("visage");

        let db_path = std::env::var("VISAGE_DB_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| data_dir.join("faces.db"));

        Self {
            camera_device: std::env::var("VISAGE_CAMERA_DEVICE")
                .unwrap_or_else(|_| "/dev/video2".to_string()),
            model_dir,
            db_path,
            similarity_threshold: env_f32("VISAGE_SIMILARITY_THRESHOLD", 0.40),
            verify_timeout_secs: env_u64("VISAGE_VERIFY_TIMEOUT_SECS", 10),
            warmup_frames: env_usize("VISAGE_WARMUP_FRAMES", 4),
            frames_per_verify: env_usize("VISAGE_FRAMES_PER_VERIFY", 3),
            frames_per_enroll: env_usize("VISAGE_FRAMES_PER_ENROLL", 5),
            emitter_enabled: std::env::var("VISAGE_EMITTER_ENABLED")
                .map(|v| v != "0")
                .unwrap_or(true),
            liveness_enabled: std::env::var("VISAGE_LIVENESS_ENABLED")
                .map(|v| v != "0")
                .unwrap_or(true),
            liveness_min_displacement: env_f32("VISAGE_LIVENESS_MIN_DISPLACEMENT", 0.8),
            session_bus: std::env::var("VISAGE_SESSION_BUS").is_ok(),
        }
    }

    /// Path to the SCRFD detection model.
    pub fn scrfd_model_path(&self) -> String {
        self.model_dir
            .join("det_10g.onnx")
            .to_string_lossy()
            .into_owned()
    }

    /// Path to the ArcFace recognition model.
    pub fn arcface_model_path(&self) -> String {
        self.model_dir
            .join("w600k_r50.onnx")
            .to_string_lossy()
            .into_owned()
    }
}

fn env_f32(key: &str, default: f32) -> f32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
