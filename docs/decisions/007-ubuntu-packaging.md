# ADR 007: Ubuntu Packaging & System Integration

**Status:** Accepted
**Date:** 2026-02-22

## Context

Steps 1-5 delivered a working face authentication pipeline: camera capture, ONNX inference,
D-Bus daemon, PAM module, and IR emitter integration. All components compile, 42 tests pass,
and face auth works end-to-end via manual D-Bus calls.

However, installation requires manual steps: building from source, copying files to system
paths, editing PAM configs, creating systemd units, and downloading ONNX models. Step 6
closes the gap between "works for developers" and "works for users."

**Success criteria:** `sudo apt install ./visage_*.deb` on clean Ubuntu 24.04 installs
everything. `sudo echo test` authenticates via face. `sudo apt remove visage` restores
password-only auth cleanly.

## Decisions

### 1. cargo-deb host crate: visaged

**Decision:** Use `visaged` crate's `Cargo.toml` for `[package.metadata.deb]` with `assets`
referencing the CLI binary and PAM .so from sibling crates.

**Rationale:** The daemon is the natural "main" package. cargo-deb's `assets` array handles
multi-binary packages without needing a separate packaging crate.

### 2. Daemon runs as root with systemd hardening

**Decision:** visaged runs as root, protected by `ProtectSystem=strict`, `ProtectHome=true`,
`NoNewPrivileges=true`, `PrivateTmp=true`, and `DeviceAllow=/dev/video* rw`.

**Rationale:** Matches fprintd precedent. Running as a dedicated user would require udev rules
for camera access and group management — complexity deferred to v3.

### 3. Model distribution via `visage setup` CLI command

**Decision:** Models are downloaded on-demand via `sudo visage setup`, not during package
installation (postinst).

**Rationale:** Offline installs should work. Users control when 182 MB downloads happen.
postinst prints a reminder if models are missing.

### 4. PAM logging via libc syslog (LOG_AUTHPRIV)

**Decision:** Use raw `libc::openlog/syslog` with `LOG_AUTHPRIV` facility.

**Rationale:** Standard PAM pattern. No new crate dependencies (libc already in scope).
Messages appear in auth log, not terminal output.

### 5. Daemon logging via tracing (journald captures)

**Decision:** Keep `tracing_subscriber::fmt()` — systemd's `StandardOutput=journal` captures
stdout/stderr automatically.

**Rationale:** Already works. No additional configuration needed.

### 6. D-Bus access control: root-only mutations

**Decision:** Default policy allows only `Verify` and `Status`. `Enroll`, `RemoveModel`, and
`ListModels` are implicitly restricted to root (no `<allow>` in default context).

**Rationale:** Sufficient for v2. In-method UID checks via `GetConnectionCredentials` are
deferred to v3.

### 7. PAM conversation: success feedback only

**Decision:** Send `PAM_TEXT_INFO "Visage: face recognized"` on successful match. Silent on
failure (password prompt speaks for itself).

**Rationale:** Matches fprintd pattern. Avoids confusing error messages when daemon is simply
not configured.

### 8. Client-side D-Bus timeout: 3 seconds

**Decision:** PAM module sets a 3-second `call_timeout` on the zbus proxy builder.

**Rationale:** Most critical safety feature. Without it, a hung daemon blocks sudo for 25+
seconds (D-Bus default timeout). 3 seconds is enough for normal verification (~80ms) but
short enough that users perceive a quick fallback to password.

## Deferred to v3

- Runtime quirk override directory (`/usr/share/visage/quirks/`)
- `visage discover --probe` (test activation pulse)
- `VISAGE_EMITTER_WARM_UP_MS` environment variable
- In-method D-Bus UID validation via `GetConnectionCredentials`
- Dedicated service user with udev rules

## Package Contents

```
/usr/bin/visaged                              — daemon binary
/usr/bin/visage                               — CLI tool
/usr/lib/security/pam_visage.so               — PAM module
/usr/share/dbus-1/system.d/org.freedesktop.Visage1.conf  — D-Bus policy
/usr/lib/systemd/system/visaged.service       — systemd unit
/usr/share/pam-configs/visage                 — pam-auth-update profile
/usr/share/doc/visage/README.md               — documentation
```

## Consequences

- Users install with a single `apt install` command
- PAM configuration is automatic via `pam-auth-update`
- Clean removal restores password-only auth
- Model download is explicit and offline-safe
- Daemon hardening limits blast radius of potential vulnerabilities
