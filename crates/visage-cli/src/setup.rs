//! `visage setup` — downloads ONNX models required for face detection and recognition.

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

// libc is a workspace dep (already used by pam-visage)
extern crate libc;

/// Model file descriptor: URL, expected filename, SHA-256 checksum, human-readable size.
struct ModelFile {
    name: &'static str,
    url: &'static str,
    sha256: &'static str,
    size_display: &'static str,
}

// Checksums verified from HuggingFace Git LFS pointer files (oid sha256: field).
// Source: https://huggingface.co/public-data/insightface/raw/main/models/buffalo_l/
const MODELS: &[ModelFile] = &[
    ModelFile {
        name: "det_10g.onnx",
        url: "https://huggingface.co/public-data/insightface/resolve/main/models/buffalo_l/det_10g.onnx",
        sha256: "5838f7fe053675b1c7a08b633df49e7af5495cee0493c7dcf6697200b85b5b91",
        size_display: "16 MB",
    },
    ModelFile {
        name: "w600k_r50.onnx",
        url: "https://huggingface.co/public-data/insightface/resolve/main/models/buffalo_l/w600k_r50.onnx",
        sha256: "4c06341c33c2ca1f86781dab0e829f88ad5b64be9fba56e56bc9ebdefc619e43",
        size_display: "166 MB",
    },
];

/// Determine the model directory.
///
/// When running as root (UID 0), defaults to `/var/lib/visage/models` (system-wide).
/// Otherwise defaults to `$XDG_DATA_HOME/visage/models` (~/.local/share/visage/models).
fn default_model_dir() -> PathBuf {
    if is_root() {
        PathBuf::from("/var/lib/visage/models")
    } else {
        let data_home = std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            format!("{home}/.local/share")
        });
        PathBuf::from(data_home).join("visage/models")
    }
}

fn is_root() -> bool {
    // SAFETY: geteuid is always safe to call.
    unsafe { libc::geteuid() == 0 }
}

/// Compute SHA-256 hex digest of a file.
fn sha256_file(path: &Path) -> Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Download a single model file with progress output.
fn download_model(model: &ModelFile, dest: &Path) -> Result<()> {
    let tmp_path = dest.with_extension("onnx.part");

    println!("  downloading {} ({})...", model.name, model.size_display);

    let resp = ureq::get(model.url)
        .call()
        .with_context(|| format!("failed to download {}", model.url))?;

    let content_length = resp
        .headers()
        .get("Content-Length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    let mut reader = resp.into_body().into_reader();
    let mut file = fs::File::create(&tmp_path)
        .with_context(|| format!("failed to create {}", tmp_path.display()))?;

    let mut buf = [0u8; 65536];
    let mut total: u64 = 0;
    let mut last_pct: u64 = 0;

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        total += n as u64;

        // Print progress every 10%
        if let Some(len) = content_length {
            let pct = (total * 100) / len;
            if pct / 10 > last_pct / 10 {
                print!("  {pct}%\r");
                io::stdout().flush().ok();
                last_pct = pct;
            }
        }
    }

    file.flush()?;
    drop(file);

    // Verify checksum
    print!("  verifying checksum... ");
    io::stdout().flush().ok();
    let digest = sha256_file(&tmp_path)?;
    if digest != model.sha256 {
        fs::remove_file(&tmp_path).ok();
        bail!(
            "checksum mismatch for {}:\n  expected: {}\n  got:      {}",
            model.name,
            model.sha256,
            digest
        );
    }
    println!("ok");

    // Atomic rename
    fs::rename(&tmp_path, dest).with_context(|| {
        format!(
            "failed to rename {} -> {}",
            tmp_path.display(),
            dest.display()
        )
    })?;

    Ok(())
}

/// Run the setup command: download and verify ONNX models.
pub fn run(model_dir: Option<String>) -> Result<()> {
    let dir = match model_dir {
        Some(d) => PathBuf::from(d),
        None => default_model_dir(),
    };

    println!("Model directory: {}", dir.display());

    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create directory {}", dir.display()))?;

    let mut downloaded = 0;
    let mut skipped = 0;

    for model in MODELS {
        let dest = dir.join(model.name);
        if dest.exists() {
            // Verify existing file
            match sha256_file(&dest) {
                Ok(digest) if digest == model.sha256 => {
                    println!("  {} already present (checksum ok)", model.name);
                    skipped += 1;
                    continue;
                }
                Ok(_) => {
                    println!(
                        "  {} exists but checksum differs — re-downloading",
                        model.name
                    );
                }
                Err(_) => {
                    println!("  {} exists but unreadable — re-downloading", model.name);
                }
            }
        }

        download_model(model, &dest)?;
        downloaded += 1;
    }

    println!();
    if downloaded > 0 {
        println!("Setup complete: {downloaded} model(s) downloaded, {skipped} already present.");
    } else {
        println!("All models already present. Nothing to download.");
    }

    Ok(())
}
