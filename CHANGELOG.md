# Changelog

## v0.1.0 — 2026-02-23

Initial release. All six implementation steps complete and end-to-end tested on Ubuntu 24.04.4 LTS.

### What's included

- **Camera pipeline** — V4L2 capture with GREY, YUYV, and Y16 format support. CLAHE preprocessing. Dark frame detection and rejection.
- **ONNX inference** — SCRFD face detection + ArcFace recognition via ONNX Runtime. CPU-capable, no CUDA required. Models download via `visage setup` with SHA-256 verification.
- **Persistent daemon** — `visaged` holds camera and model weights across auth requests. D-Bus IPC (`org.freedesktop.Visage1`). SQLite model store with WAL mode.
- **PAM module** — `pam-visage` integrates with any PAM-based application (sudo, login, screen lock). `PAM_IGNORE` fallback — face unavailable always falls through to password. Never blocks.
- **IR emitter control** — UVC extension unit control for Windows Hello-compatible IR cameras. Hardware quirks database (TOML). ASUS Zenbook 14 UM3406HA tested and confirmed.
- **Ubuntu packaging** — `.deb` with `pam-auth-update` integration, systemd hardening (`ProtectSystem=strict`, `NoNewPrivileges=yes`), and clean install/remove/purge lifecycle.
- **Security** — AES-256-GCM embedding encryption at rest, rate limiting (5 failures/60s → 5-min lockout), D-Bus caller UID validation.

### Known limitations

- **Ubuntu 24.04 only** — NixOS, AUR, and COPR packages are in progress.
- **~1.4s verify latency** on CPU-only ONNX with USB webcam. Target <500ms requires IR camera and hardware acceleration.
- **No active liveness detection** — IR emitter and multi-frame capture reduce spoofing risk; active challenge-response (blink detection) is planned for a future release.
- **`MemoryDenyWriteExecute=false`** — required for ONNX Runtime JIT compilation. All other sandbox directives are applied.

### Installation

```bash
# Download visage_0.1.0_amd64.deb from the release assets
sudo apt install ./visage_0.1.0_amd64.deb
sudo visage setup       # downloads ONNX models (~182 MB)
visage enroll           # enroll your face
sudo echo test          # verify PAM integration
```

See [docs/hardware-compatibility.md](docs/hardware-compatibility.md) for camera compatibility tiers and IR emitter setup.

### Requirements

- Ubuntu 24.04 LTS (amd64)
- V4L2-compatible camera (UVC preferred)
- libpam0g, libdbus-1-3 (installed automatically via .deb)
