mod setup;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::time::Duration;

#[zbus::proxy(
    interface = "org.freedesktop.Visage1",
    default_service = "org.freedesktop.Visage1",
    default_path = "/org/freedesktop/Visage1"
)]
trait Visage {
    async fn enroll(&self, user: &str, label: &str) -> zbus::fdo::Result<String>;
    async fn verify(&self, user: &str) -> zbus::fdo::Result<bool>;
    async fn status(&self) -> zbus::fdo::Result<String>;
    async fn list_models(&self, user: &str) -> zbus::fdo::Result<String>;
    async fn remove_model(&self, user: &str, model_id: &str) -> zbus::fdo::Result<bool>;
}

#[derive(Parser)]
#[command(name = "visage", about = "Visage biometric authentication CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Enroll a new face model
    Enroll {
        /// Label for this face model (e.g., "normal", "glasses")
        #[arg(short, long)]
        label: String,

        /// User to enroll for. Defaults to the invoking user — SUDO_USER under sudo, NOT root.
        #[arg(short, long)]
        user: Option<String>,
    },
    /// Verify your face against enrolled models
    Verify {
        /// User to verify as. Defaults to the invoking user — SUDO_USER under sudo, NOT root.
        #[arg(short, long)]
        user: Option<String>,
    },
    /// List enrolled face models
    List {
        /// User whose models to list. Defaults to the invoking user — SUDO_USER under sudo, NOT root.
        #[arg(short, long)]
        user: Option<String>,
    },
    /// Remove an enrolled face model
    Remove {
        /// Model ID to remove
        id: String,

        /// User who owns the model. Defaults to the invoking user — SUDO_USER under sudo, NOT root.
        #[arg(short, long)]
        user: Option<String>,
    },
    /// One command: download models, enroll several angles, and verify.
    ///
    /// This is the path most people want. Running `setup` then a bare `enroll`
    /// leaves two traps: enroll defaults to `$USER` (which is `root` under the
    /// sudo these commands require), and a single capture is fragile on
    /// hardware where the IR emitter strobes or has no quirk.
    Onboard {
        /// User to enroll for. Defaults to the invoking user (SUDO_USER).
        #[arg(short, long)]
        user: Option<String>,

        /// Comma-separated capture labels, one prompt per label.
        #[arg(
            long,
            value_delimiter = ',',
            default_value = "normal,left,right,glasses"
        )]
        labels: Vec<String>,

        /// Model directory override, passed through to setup.
        #[arg(long)]
        model_dir: Option<String>,

        /// Skip the interactive prompt between captures.
        #[arg(long)]
        no_prompt: bool,
    },
    /// Download ONNX models required for face detection and recognition
    Setup {
        /// Model directory (default: /var/lib/visage/models when root, ~/.local/share/visage/models otherwise)
        #[arg(short, long)]
        model_dir: Option<String>,
    },
    /// Show daemon status
    Status,
    /// List cameras and their IR emitter quirk status
    Discover,
    /// Run camera diagnostics
    Test {
        /// Camera device path
        #[arg(short, long, default_value = "/dev/video2")]
        device: String,

        /// Number of frames to capture
        #[arg(short = 'n', long, default_value = "10")]
        frames: usize,
    },
}

/// The human who invoked us — NOT `root` when running under `sudo`.
///
/// Every privileged subcommand (enroll, remove, list) is root-only by the D-Bus
/// policy, so in practice they are always run as `sudo visage …`. Reading plain
/// `$USER` there yields `root`, so `sudo visage enroll --label normal` enrolled
/// the face against **root** while PAM went on looking up the real user, found
/// nothing, and fell through to the password prompt.
///
/// That failure is silent in the worst way: enrollment prints "Enrolled
/// successfully", `sudo` keeps asking for a password, and nothing anywhere says
/// the two are about different users. It also leaves a face credential attached
/// to the most privileged account on the machine, created by accident.
///
/// `SUDO_USER` is what sudo itself records about the invoker, so prefer it and
/// fall back to `$USER` when not under sudo. Pass `--user` to override.
fn current_user() -> String {
    std::env::var("SUDO_USER")
        .ok()
        .filter(|u| !u.is_empty() && u != "root")
        .or_else(|| std::env::var("USER").ok())
        .unwrap_or_else(|| "unknown".to_string())
}

fn verify_timeout_secs() -> u64 {
    std::env::var("VISAGE_VERIFY_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10)
}

async fn connect_proxy() -> Result<VisageProxy<'static>> {
    let use_session = std::env::var("VISAGE_SESSION_BUS").is_ok();
    let timeout = Duration::from_secs(verify_timeout_secs());
    let conn = if use_session {
        zbus::connection::Builder::session()?
    } else {
        zbus::connection::Builder::system()?
    }
    .method_timeout(timeout)
    .build()
    .await
    .map_err(|e| anyhow::anyhow!("failed to connect to D-Bus: {e}"))?;

    let proxy = VisageProxy::new(&conn)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create proxy: {e} — is visaged running?"))?;
    Ok(proxy)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Enroll { label, user } => {
            let user = user.unwrap_or_else(current_user);
            let proxy = connect_proxy().await?;
            println!("Enrolling face model '{label}' for user '{user}'...");
            match proxy.enroll(&user, &label).await {
                Ok(model_id) => println!("Enrolled successfully. Model ID: {model_id}"),
                Err(e) => {
                    eprintln!("Enrollment failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        Commands::Verify { user } => {
            let user = user.unwrap_or_else(current_user);
            let proxy = connect_proxy().await?;
            println!("Verifying face for user '{user}'...");
            match proxy.verify(&user).await {
                Ok(true) => {
                    println!("Match: verified");
                    // Exit 0 on match (shell-friendly)
                }
                Ok(false) => {
                    println!("No match");
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("Verification failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        Commands::List { user } => {
            let user = user.unwrap_or_else(current_user);
            let proxy = connect_proxy().await?;
            match proxy.list_models(&user).await {
                Ok(json) => {
                    let models: Vec<serde_json::Value> = serde_json::from_str(&json)?;
                    if models.is_empty() {
                        println!("No models enrolled for user '{user}'");
                    } else {
                        println!("Enrolled models for '{user}':");
                        for m in &models {
                            println!(
                                "  {} — label: {}, quality: {:.3}, created: {}",
                                m["id"].as_str().unwrap_or("?"),
                                m["label"].as_str().unwrap_or("?"),
                                m["quality_score"].as_f64().unwrap_or(0.0),
                                m["created_at"].as_str().unwrap_or("?"),
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to list models: {e}");
                    std::process::exit(1);
                }
            }
        }
        Commands::Remove { id, user } => {
            let user = user.unwrap_or_else(current_user);
            let proxy = connect_proxy().await?;
            match proxy.remove_model(&user, &id).await {
                Ok(true) => println!("Model {id} removed"),
                Ok(false) => {
                    eprintln!("Model {id} not found (or not owned by user '{user}')");
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("Failed to remove model: {e}");
                    std::process::exit(1);
                }
            }
        }
        Commands::Onboard {
            user,
            labels,
            model_dir,
            no_prompt,
        } => {
            let user = user.unwrap_or_else(current_user);
            if user == "root" {
                eprintln!(
                    "Refusing to onboard 'root'. Face auth for root is almost never what you\n\
                     want, and it is the exact mistake this command exists to prevent.\n\
                     Pass --user <name> explicitly if you really mean it."
                );
                std::process::exit(1);
            }
            if labels.is_empty() {
                eprintln!("No labels given — nothing to enroll.");
                std::process::exit(1);
            }

            println!("Onboarding face authentication for user '{user}'.\n");

            // 1. Models. setup::run is idempotent — it reports what is already present.
            println!("[1/3] ONNX models");
            setup::run(model_dir)?;
            println!();

            // 2. Captures. visaged self-heals within its restart interval once the
            //    models land, so connect AFTER setup rather than before.
            println!("[2/3] Face captures ({} to take)", labels.len());
            let proxy = connect_proxy().await?;
            let mut enrolled: Vec<(String, String)> = Vec::new();
            for (i, label) in labels.iter().enumerate() {
                if !no_prompt {
                    println!(
                        "\n  ({}/{}) '{}' — look at the camera{}, then press Enter.",
                        i + 1,
                        labels.len(),
                        label,
                        match label.as_str() {
                            "left" => " and turn your head slightly LEFT",
                            "right" => " and turn your head slightly RIGHT",
                            "glasses" => " wearing your glasses (skip with Ctrl-C if none)",
                            _ => " straight on",
                        }
                    );
                    let mut _l = String::new();
                    let _ = std::io::stdin().read_line(&mut _l);
                }
                match proxy.enroll(&user, label).await {
                    Ok(id) => {
                        println!("  ✓ {label}: {id}");
                        enrolled.push((label.clone(), id));
                    }
                    // One bad capture should not discard the ones that worked.
                    Err(e) => eprintln!("  ✗ {label}: {e}"),
                }
            }
            if enrolled.is_empty() {
                eprintln!("\nNo models enrolled — not going to claim this worked.");
                std::process::exit(1);
            }

            // 3. Prove it against the daemon rather than trusting the enroll return.
            println!("\n[3/3] Verifying");
            match proxy.verify(&user).await {
                Ok(true) => {
                    println!("  ✓ recognised '{user}' from {} model(s)", enrolled.len());
                    println!(
                        "\nDone. Test the real path with:  sudo -k && sudo true\n\
                         Password remains available as a fallback on every PAM service."
                    );
                }
                Ok(false) => {
                    println!("  ✗ enrolled, but verification did not recognise you.");
                    println!(
                        "\n{} model(s) are stored. Re-run to add more angles, or check\n\
                         lighting — `visage discover` will say whether this camera has an\n\
                         IR emitter quirk; without one it depends on ambient light.",
                        enrolled.len()
                    );
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("  ✗ verification call failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        Commands::Setup { model_dir } => {
            setup::run(model_dir)?;
        }
        Commands::Discover => {
            cmd_discover();
        }
        Commands::Status => {
            let proxy = connect_proxy().await?;
            match proxy.status().await {
                Ok(json) => {
                    let status: serde_json::Value = serde_json::from_str(&json)?;
                    println!("visaged status:");
                    println!(
                        "  version:    {}",
                        status["version"].as_str().unwrap_or("?")
                    );
                    println!("  camera:     {}", status["camera"].as_str().unwrap_or("?"));
                    if let Some(model_dir) = status.get("model_dir").and_then(|v| v.as_str()) {
                        println!("  model_dir:  {model_dir}");
                    }
                    if let Some(db_path) = status.get("db_path").and_then(|v| v.as_str()) {
                        println!("  db_path:    {db_path}");
                    }
                    println!(
                        "  models:     {}",
                        status["models_enrolled"].as_u64().unwrap_or(0)
                    );
                    println!(
                        "  threshold:  {:.2}",
                        status["similarity_threshold"].as_f64().unwrap_or(0.0)
                    );
                    if let Some(v) = status.get("verify_timeout_secs").and_then(|v| v.as_u64()) {
                        println!("  timeout:    {v}s");
                    }
                    if let Some(v) = status.get("frames_per_verify").and_then(|v| v.as_u64()) {
                        println!("  verify_n:   {v} frame(s)");
                    }
                    if let Some(v) = status.get("frames_per_enroll").and_then(|v| v.as_u64()) {
                        println!("  enroll_n:   {v} frame(s)");
                    }
                    if let Some(v) = status.get("emitter_enabled").and_then(|v| v.as_bool()) {
                        println!("  emitter:    {}", if v { "enabled" } else { "disabled" });
                    }
                    if let Some(v) = status.get("session_bus").and_then(|v| v.as_bool()) {
                        println!("  bus:        {}", if v { "session" } else { "system" });
                    }
                }
                Err(e) => {
                    eprintln!("visaged: not reachable — {e}");
                    eprintln!("Is visaged running?");
                    std::process::exit(1);
                }
            }
        }
        Commands::Test { device, frames } => {
            run_camera_test(&device, frames)?;
        }
    }

    Ok(())
}

fn cmd_discover() {
    use visage_hw::quirks::{get_driver, get_usb_ids, is_ipu6_camera, lookup_quirk};

    let mut entries: Vec<_> = std::fs::read_dir("/dev")
        .expect("cannot read /dev")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|n| n.starts_with("video"))
                .unwrap_or(false)
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());

    if entries.is_empty() {
        println!("No /dev/video* devices found.");
        return;
    }

    let mut ipu6_detected = false;

    for entry in entries {
        let path = format!("/dev/{}", entry.file_name().to_string_lossy());
        let driver = get_driver(&path);

        if is_ipu6_camera(&path) {
            ipu6_detected = true;
            let driver_name = driver.as_deref().unwrap_or("intel_ipu6");
            println!("{path}  driver={driver_name}  [NOT SUPPORTED — IPU6 camera, not UVC]");
            continue;
        }

        match get_usb_ids(&path) {
            Some((vid, pid)) => {
                let driver_label = driver.as_deref().unwrap_or("unknown");
                let quirk_status = match lookup_quirk(vid, pid) {
                    Some(q) => format!("quirk: {} \u{2713}", q.device.name),
                    None => format!("no quirk (VID={vid:#06x} PID={pid:#06x})"),
                };
                println!(
                    "{path}  driver={driver_label}  VID={vid:#06x} PID={pid:#06x}  {quirk_status}"
                );
            }
            None => {
                let driver_label = driver.as_deref().unwrap_or("unknown");
                println!("{path}  driver={driver_label}  (not USB or no sysfs entry)");
            }
        }
    }

    if ipu6_detected {
        eprintln!();
        eprintln!("WARNING: Intel IPU6 camera(s) detected.");
        eprintln!("  IPU6 cameras use Intel's proprietary camera HAL and require libcamera,");
        eprintln!("  not the V4L2/UVC stack that Visage uses. They are not supported in v0.1.");
        eprintln!();
        eprintln!("  If your laptop has a separate USB IR camera, it may still appear above");
        eprintln!("  under a different /dev/videoN node with driver=uvcvideo.");
        eprintln!();
        eprintln!("  See: https://github.com/sovren-software/visage/blob/main/docs/hardware-compatibility.md");
    }
}

fn run_camera_test(device_path: &str, frame_count: usize) -> Result<()> {
    println!("Camera diagnostics");
    println!("==================");

    // List available devices
    let devices = visage_hw::Camera::list_devices();
    println!("\nDiscovered capture devices:");
    if devices.is_empty() {
        println!("  (none)");
    }
    for dev in &devices {
        println!("  {} — {} [{}]", dev.path, dev.name, dev.driver);
    }

    // Open target device
    println!("\nOpening {device_path}...");
    let camera = visage_hw::Camera::open(device_path)?;
    println!(
        "  Format: {:?} {}x{}",
        camera.fourcc, camera.width, camera.height
    );

    // Prepare output directory
    let out_dir = std::path::PathBuf::from("/tmp/visage-test");
    std::fs::create_dir_all(&out_dir)?;

    // Capture frames
    println!("\nCapturing {frame_count} frames...");
    let (captured_frames, dark_skipped) = camera.capture_frames(frame_count)?;
    println!(
        "  Captured: {} good, {} dark skipped",
        captured_frames.len(),
        dark_skipped
    );

    // Save as PGM and compute stats
    for (i, frame) in captured_frames.iter().enumerate() {
        let filename = out_dir.join(format!("frame-{:03}.pgm", i));
        save_pgm(&filename, &frame.data, frame.width, frame.height)?;
        println!(
            "  [{}] seq={} brightness={:.1} -> {}",
            i,
            frame.sequence,
            frame.avg_brightness(),
            filename.display()
        );
    }

    // Summary
    if !captured_frames.is_empty() {
        let avg: f32 = captured_frames
            .iter()
            .map(|f| f.avg_brightness())
            .sum::<f32>()
            / captured_frames.len() as f32;
        println!("\nAverage brightness: {avg:.1}");
    }

    println!("\nDone. Frames saved to {}", out_dir.display());
    Ok(())
}

/// Write a grayscale image as PGM (Portable Gray Map) — no extra deps needed.
fn save_pgm(path: &std::path::Path, data: &[u8], width: u32, height: u32) -> Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    write!(f, "P5\n{width} {height}\n255\n")?;
    f.write_all(data)?;
    Ok(())
}
