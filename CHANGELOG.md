# Changelog

## Unreleased

### Added

- **`visage onboard` — one command from nothing to working face auth.** Downloads
  the ONNX models, enrolls several labelled angles with a prompt between each, and
  verifies against the daemon before claiming success. Replaces the `setup` →
  `enroll` → `list` → test sequence, which had two traps: `enroll` defaulted to the
  wrong user under sudo (below), and a single capture is fragile on hardware where
  the IR emitter strobes or has no quirk entry.

  It refuses to onboard `root` unless `--user root` is given explicitly, keeps
  whichever captures succeeded if one fails, and exits non-zero if verification does
  not recognise the enrolled face — so a failed onboarding cannot look like a
  successful one.

- **First hardware validation of passive liveness — and it did not discriminate.**
  New hardware report for the ASUS Zenbook 14 UM3406HA / Shinetech `3277:0055` IR
  module, executing the blocking §2 checklist in `docs/liveness-remaining-work.md`
  that had been open since 2026-02-25.

  A hand-held phone-screen spoof produced landmark displacement of **0.681 px** —
  *higher* than two genuine live attempts (0.263, 0.670) — so no threshold on that
  metric admits the live minimum while rejecting the spoof. The identity stage
  matched the same photo at **0.9013**. Live users were falsely rejected 13–17% of
  the time at the default 0.8 px floor — 20/23–20/24 live pass as of the journal at
  2026-08-17 13:43 local, so the ≥9-in-10 reliability criterion is not met. `DEFAULT_MIN_EYE_DISPLACEMENT` is documented
  against 640×480@30fps; this sensor is 640×360, and its "printed photo <0.3 px"
  figure assumes a *rigidly mounted* photo rather than one held in a hand.

  Sample is **n=1** on the spoof side, so `threat-model.md`'s "static photo" claim is
  deliberately left unrevised. Do not lower `liveness.minDisplacement` on this
  evidence — it would widen the hole rather than close it.

  Also documented: this model ships **more than one camera module**, so the
  `04f2:b6d9` quirk does not apply to `3277:0055` units, and a *missing* quirk does
  not imply *missing illumination* — the emitter strobes by firmware default here.

- **`pam_visage.so` now accepts a `timeout=N` module argument**, exposed as
  `services.visage.pam.timeoutSeconds`. The D-Bus method timeout was hard-coded
  at 3 seconds.

  That is not only a hang guard — it is an upper bound on how long
  authentication may take. A verify that exceeds it makes PAM fall through to
  the password prompt, and the outcome is **indistinguishable from a failed
  match**: both return `PAM_IGNORE`, and nothing in the PAM log says "timed
  out". On CPU-only hardware a verify can legitimately take seconds; measured
  median on an ASUS Zenbook 14 was **2273 ms**, leaving under 700 ms of
  headroom against a 3-second ceiling. The symptom of getting that wrong is
  intermittent password prompts that look exactly like recognition failures.

  A malformed, zero, or unparseable value logs a warning and falls back to the
  3-second default rather than failing — a typo in a PAM line must never be
  able to lock a user out.

- **Five daemon settings are now NixOS options** — `framesPerVerify`,
  `framesPerEnroll`, `warmupFrames`, `verifyTimeoutSeconds`, and
  `emitter.enable`. The daemon has always read the corresponding
  `VISAGE_*` environment variables, but the module exposed none of them, so the
  only way to tune a deployment was a hand-written systemd drop-in.

  `framesPerVerify` is the main latency knob, and its documentation now records
  both interactions that make it non-obvious: the PAM timeout above, and the
  fact that passive liveness fails closed below two frames with a detected
  face — so lowering it to 2 can reintroduce false rejects.

- **The hardware quirks database is now tested.** `crates/visage-hw/src/quirks.rs` had no
  tests, and `quirk_db()` skips a malformed TOML rather than panicking — correct for a daemon
  that authenticates logins, but it means an embedded-but-broken quirk compiles, ships, and
  silently never fires. At runtime that is indistinguishable from a camera with no quirk.

  Four assertions now run in CI: every embedded source parses (count compared against
  `QUIRK_SOURCES.len()`, so it cannot drift as cameras are added), every parsed quirk is
  reachable via `lookup_quirk`, no two entries claim the same VID:PID, and emitter payloads are
  coherent (non-empty `control_bytes`; `off_bytes` matching its length). Each was verified to
  **fail** on a deliberately broken input, not merely to pass.

  `contrib/hw/README.md` also gained the registration step it was missing. It previously went
  from "create a TOML file" straight to "submit a PR", never mentioning that a file must be
  added to `QUIRK_SOURCES` to be read at all — so a contributor following it literally would
  ship a quirk that does nothing.

### Changed

- **`services.visage.pam.enable`'s description no longer overstates what it
  wires.** It claimed face auth for "sudo, login, and screen lock"; it wires
  `sudo` and `login` only. Screen lock is covered *indirectly* and only for
  lockers that derive their PAM stack from `login` — DMS/quickshell generates a
  config with an identical module set, and so inherits face auth for free.
  Lockers declaring their own PAM service get nothing and must be wired
  explicitly; the description now shows how, and warns that grepping
  `/etc/pam.d/` alone will report the lock screen as unwired when it is not,
  because a locker's config may live in the user's state directory.

- **`visage onboard` no longer verifies the instant the last capture returns.**
  It paused for zero milliseconds, so `[3/3]` fired while the user was still
  holding the pose they struck to press Enter — the posture passive liveness is
  most likely to reject, since it looks for landmark movement. Onboarding could
  enroll perfectly and then fail its own verification for a reason unrelated to
  enrollment quality. Now waits two seconds and says what to do with them.

- **`visage onboard`'s failure message no longer asserts a cause it cannot
  know.** It told users to check lighting and the IR emitter quirk. The daemon
  deliberately collapses every failure into a plain non-match — distinguishing
  "wrong face" from "identity matched but liveness rejected" would tell an
  attacker holding a photograph that the photograph was recognised — so the CLI
  genuinely does not know why a verify failed. It now says so, points at
  `journalctl -u visaged`, and lists the causes in rough order of likelihood
  with the liveness one first.

### Fixed

- **Nix package: `bindgen` could not find libclang.** `v4l2-sys-mit` (pulled in by
  `visage-hw` for camera capture) runs `bindgen` in its build script, which dlopens
  libclang. `packaging/nix/default.nix` declared only `pkg-config` in
  `nativeBuildInputs`, so `nix build .#visage` failed with *"Unable to find libclang
  … set the `LIBCLANG_PATH` environment variable"*. Added `rustPlatform.bindgenHook`.

  `flake.nix` has carried `llvmPackages.libclang` + `LIBCLANG_PATH` for the devShell
  since it was written, so `cargo build` in a dev shell always worked — the *package*
  never did. Nothing caught the difference because no consumer referenced
  `pkgs.visage`: `services.visage.enable` is set on no host and the package is in no
  `systemPackages`, so the derivation was never realised and every build stayed green
  for reasons unrelated to it.

- **Nix package: version label was three patch releases stale.** The derivation
  hardcoded `version = "0.3.3"` while `[workspace.package]` was at `0.3.6`, so any
  built artifact carried a wrong externally-visible version. Now read from
  `Cargo.toml` so the two cannot drift.

- **Nix package: ONNX Runtime is now fetched hermetically.** `ort-sys` downloads a
  prebuilt runtime from `cdn.pyke.io` in its build script, which a sandboxed build
  correctly blocks — so `nix build .#visage` could never work offline. Pointing
  `ORT_LIB_LOCATION` at nixpkgs' `onnxruntime` does not help either: `ort` links
  **statically** by default and nixpkgs ships only `.so`, giving *"could not link to
  the ONNX Runtime build in …"*.

  Now the same archive `ort-sys` wants is fetched via `fetchurl` with a pinned hash
  and unpacked into a small derivation that `ORT_LIB_LOCATION` points at.
  Version-matched by construction (1.23.2, what `ort 2.0.0-rc.11` expects), so no
  version skew, and reproducible.

  The archive is **raw LZMA2, not an `.xz` container**, and needs
  `xz --format=raw --lzma2=dict=64MiB`. The 64 MiB dictionary is required and is not
  the default: plain `--lzma2` decodes only ~8.9 MB of the ~93 MB payload and exits
  non-zero, which — piped into `tar` — looks like a clean extraction, because a
  truncated `ar` archive still yields a plausible `libonnxruntime.a`. A size floor
  now refuses any decode under 80 MB.

- **Nix package: PAM module path no longer hardcoded.** `postInstall` looked for
  `target/release/libpam_visage.so`, but current `rustPlatform.buildRustPackage`
  passes `--target`, so cargo emits to `target/<triple>/release/`. It is now located
  at install time and the build fails loudly if it is absent or ambiguous — which it
  was, three times over: `release/deps/`, `release-tmp/`, and the real one.

- **`clippy::excessive_precision` on two pre-existing test literals**, newly flagged
  by a more recent clippy and failing `cargo clippy -- -D warnings`. In
  `alignment.rs` the value carried a trailing zero and was trimmed. In `store.rs` the
  excess precision is deliberate — that test asserts a bit-exact f32 round-trip — so
  it is annotated with a justified `#[allow]` rather than truncated, which would have
  quietly weakened the case it exists to test.

- **`--user` defaulted to `root` under `sudo`, silently enrolling the wrong account.**
  Every privileged subcommand is root-only by the D-Bus policy, so they are always
  run as `sudo visage …` — where `$USER` is `root`. `sudo visage enroll --label x`
  therefore registered the face against **root** while PAM went on looking up the
  real user, found nothing, and fell through to the password prompt.

  The failure was silent in the worst way: enrollment printed *"Enrolled
  successfully"*, `sudo` kept asking for a password, and nothing indicated the two
  were about different users. It also left a face credential attached to the most
  privileged account on the machine, created by accident.

  `current_user()` now prefers `SUDO_USER`, falling back to `$USER` when not under
  sudo. The help text on `enroll`, `verify`, `list` and `remove` said "defaults to
  `$USER`" and has been corrected too.

### Developer experience

- **Releases are cut from tags again, and cannot publish empty.** v0.3.4, v0.3.5
  and v0.3.6 shipped with no `.deb` attached (issue #75). The release job gated
  on the head commit message starting with `release:`, and adopting Conventional
  Commits (`chore(release): 0.3.4`) silently severed it — nothing failed,
  because each release was then created by hand and looked correct. The job is
  now triggered by a `v*` tag, asserts the tag matches `Cargo.toml`, attaches
  only that version's changelog section rather than the whole file, derives
  pre-release status from the version string, and refuses to publish at all if
  no `.deb` is present.

- **The toolchain is pinned, so CI cannot change behaviour without a commit.**
  The identical commit passed CI on 2026-08-18 and failed on 2026-08-24, purely
  because `dtolnay/rust-toolchain@stable` floated onto a compiler whose clippy
  added a new lint; with `-D warnings` that halted every merge in the repository
  for six days. `rust-toolchain.toml` now pins the version — tracking what
  `nix develop` provides, so a local `cargo clippy` reproduces CI exactly — and
  the workflow reads it as the single source of truth. A new non-blocking
  `clippy-latest` job still runs against current stable, so new lints surface
  early without being able to block a merge.

### Security

- **The daemon's network isolation is now enforced, not merely asserted.**
  `threat-model.md` justifies its largest documented hardening gap —
  `MemoryDenyWriteExecute=false`, required by ONNX Runtime's JIT — by stating
  that the daemon "has no network access, no inbound connections". No unit
  directive enforced that, so the compensating control for that exception did
  not exist. Reported as issue #78.

  `visaged.service` now sets `PrivateNetwork=true` in both the packaged unit
  and the NixOS module, placing the daemon in its own network namespace with
  loopback only. `visaged` makes no network calls of its own; the only HTTP
  client in the workspace is `ureq`, used solely by `visage-cli` for `visage
  setup` model downloads, which runs as a separate process and is unaffected.
  D-Bus is an AF_UNIX filesystem socket and is not namespaced by
  `PrivateNetwork`.

### Notes

- `nix build .#visage` now succeeds end-to-end: compile, unit tests, and install of
  the daemon, CLI, PAM module, D-Bus policy and both systemd units.

## v0.3.6 — 2026-07-07

Security hardening batch — defense-in-depth on the D-Bus authorization surface,
a fail-open → fail-closed correction in passive liveness, and a runtime-safety
pin on the async executor. No public API or wire-format changes.

### Security

- **In-process root check on the privileged D-Bus methods** (`Enroll`,
  `RemoveModel`, `ListModels`). These were root-only by the system-bus policy
  file (`org.freedesktop.Visage1.conf`) alone; a missing, mis-scoped, or
  overly-permissive policy — or running on the session bus — could let a
  non-root caller invoke enrollment mutation or the enrollment listing. The
  daemon now re-verifies the caller is root (UID 0) inside each handler (skipped
  on the session bus, development mode), mirroring the defense-in-depth `Verify`
  already applied.
- **`VISAGE_SESSION_BUS=0` no longer enables session-bus mode.** Session-bus
  mode *skips* D-Bus caller-UID validation (development only). The flag was read
  with `env::var(..).is_ok()`, so *any* value — including `VISAGE_SESSION_BUS=0`,
  the natural way to turn it off — enabled it and silently disabled UID
  validation (fail-open). It now enables only on a non-empty, non-`"0"` value;
  unset, empty, and `"0"` all keep the secure system-bus default.
- **Passive liveness now fails closed on insufficient landmark data.**
  `check_landmark_stability` reported "live" when fewer than two landmark frames
  were available (fail-open), so a match backed by only a single detectable
  landmark frame bypassed the liveness gate entirely. It now returns not-live in
  that case; the daemon surfaces it as a (rate-limited) non-match and the user
  retries. `frames_per_verify` defaults to 3, so a live subject in normal
  lighting is unaffected.

### Changed

- **Pinned `zbus` to the `tokio` executor** (`default-features = false,
  features = ["tokio"]`; `pam-visage` re-adds `blocking-api`), following zbus's
  own recommendation for tokio integration. `visaged` awaits tokio primitives
  inside `#[zbus::interface]` handlers; on zbus's default `async-io` executor any
  reactor-bound tokio call added later would panic with "no reactor running", and
  a single transitive dependency could silently revert the whole process to
  `async-io` via Cargo feature unification. This also removes the `async-io` /
  `smol` executor stack from the dependency tree.

### Added

- **AES-256-GCM known-answer test + on-disk blob-format guard** (`visaged`).
  Locks the embedding-encryption primitive against the NIST GCM test vector and
  the stored blob layout (12-byte nonce ‖ ciphertext ‖ 16-byte GCM tag, with
  AEAD tamper rejection), so a future `aes-gcm` upgrade cannot silently change
  the on-disk format and orphan existing enrollments.

## v0.3.5 — 2026-07-07

### Added

- **Hardware support: HP OmniBook X Flip IR camera** (`30c9:0120`, Luxvisions).
  Contributed by @mocha in #47. Adds an IR-emitter quirk-schema extension
  (`off_bytes`, `reset_on_close`) for devices whose emitter rejects an all-zero
  "off" write (`ERANGE`); existing quirk files are unaffected (both fields
  default). Quirk file: `contrib/hw/30c9-0120.toml`.
- **Hardware support: Lenovo ThinkBook 14 MP2PQAZG IR camera** (`30c9:00c2`).
  Contributed in #45 (previously not captured in the changelog). Quirk file:
  `contrib/hw/30c9-00c2.toml`.

### Security

- **`openssl` 0.10.75 → 0.10.81** and **`rustls-webpki` 0.103.9 → 0.103.13**
  (Dependabot security updates, #60 / #59) — pull RustSec-advisory fixes into
  the TLS dependency chain (`ort` → `ureq`).

## v0.3.4 — 2026-07-07

### Fixed

- **NixOS / Nix flake build: add `openssl` to `buildInputs`** (issue #38). The
  Nix derivation failed to build because `ort` (ONNX Runtime) pulls in `ureq` →
  `native-tls` → `openssl-sys`, whose build script needs the system OpenSSL
  library at link time. `nativeBuildInputs` already provided `pkg-config`, but
  `buildInputs` was missing `openssl`, so `openssl-sys` could not locate it.
  A follow-up can drop the C TLS dependency entirely by building `ort` with
  `default-features = false, features = ["load-dynamic"]` against
  `pkgs.onnxruntime`.
- **Camera capture no longer degrades over long sessions on a shared webcam**
  (issue #48). `visaged` negotiated the V4L2 capture format only once, at
  `Camera::open`, and never re-asserted it. On a webcam shared with other
  applications (e.g. a video-conferencing app), another process could change the
  device's format via `VIDIOC_S_FMT` and leave it there; `visaged` then captured
  wrong-format frames it decoded as garbage through its stale format cache —
  surfacing as "no face detected" until a manual `systemctl restart`. The daemon
  now re-asserts its format before each capture (a cheap `VIDIOC_G_FMT`; `S_FMT`
  only fires when the device actually drifted) and, as a safety net, re-opens the
  camera in-process after repeated capture failures instead of requiring a
  restart. The per-capture stream is retained, so the camera is still released
  between verifies and remains usable by other applications.
- **AUR install hook: corrected PAM keyword `success=end` → `success=done`.**
  `packaging/aur/visage.install` still printed the setup guidance with the
  invalid `[success=end …]` action — the same bug fixed everywhere else in
  v0.3.2. libpam treats the unknown `end` as `ignore`, so a user following the
  printed line verbatim would get a silent face-auth no-op. Now prints
  `[success=done default=ignore]`.

### Security

- **CI: added a scheduled `cargo audit` workflow** (`.github/workflows/audit.yml`).
  It scans `Cargo.lock` against the RustSec advisory database weekly and on
  demand, surfacing dependency advisories without waiting for a manual check.

## v0.3.3 — 2026-05-28

### Added

- **Hardware support: Lenovo ThinkPad X1 Carbon Gen 9 20XW00FPUS IR camera** (`174f:2454`).
  Verified on hardware. Quirk file at `contrib/hw/174f-2454.toml`. Contributed by
  @themariusus in #29.

### Packaging

- **AUR `PKGBUILD` disables LTO and debug** (`options=(!lto !debug)`). LTO operates on
  LLVM IR, but `ring` ships hand-written assembly via `cc` and `libsqlite3-sys`
  compiles `sqlite3.c` via `cc` — those `.o` files have no LTO-compatible IR, so the
  final link drops or fails to resolve their symbols. Without this, `makepkg -si`
  on a stock Arch system fails at link time with `undefined symbol:
  ring_core_0_17_14__LIMBS_window5_split_window` (and many more from both `ring`
  and `libsqlite3-sys`). Reported and fixed by @SomeCodecat in #25.

### Developer experience

- **`nix develop` shell now ships `rustfmt`, `clippy`, and `libclang`.**
  `inputsFrom = [ visage ]` brought the compiler but not these auxiliaries, so
  contributors hit `error: no such command: fmt` and bindgen failed to find
  `libclang.so`. Devshell now sets `LIBCLANG_PATH` and exposes both cargo
  subcommands matching CI's `dtolnay/rust-toolchain@stable` gates. (#32)

### Dependencies

- `tokio` 1.49.0 → 1.50.0
- `nix` 0.31.1 → 0.31.2
- `uuid` 1.21.0 → 1.23.0
- `image` 0.25.9 → 0.25.10
- `actions/checkout` v4 → v6 (CI)
- `actions/upload-artifact` v4 → v7 (CI)
- `actions/download-artifact` v4 → v8 (CI)

## v0.3.2 — 2026-05-28

### Fixed

- **PAM control keyword corrected: `success=end` → `success=done` across all 9 sites.**
  `pam.conf(5)` documents exactly `ignore | bad | die | ok | done | reset | N` —
  `end` is not a valid keyword. libpam logged a warning and treated it as
  `ignore`, meaning a successful face match silently fell through to the next
  rule (typically `pam_unix.so` → password prompt) instead of terminating the
  auth stack with success. Affected: `README.md`, `docs/operations-guide.md`,
  `docs/architecture.md`, `packaging/debian/pam-auth-update` (Ubuntu),
  `packaging/nix/module.nix` (NixOS — `sudo` and `login` rules), and several
  research docs. Caught by @SelfRef in #27. **Note for existing users:** if your
  PAM stack still references the old keyword (e.g. you manually edited
  `/etc/pam.d/system-auth` on Arch from the prior README, or you're on an old
  Debian/Ubuntu install that hasn't re-run `pam-auth-update`), face auth has
  been working as if Visage weren't installed — replace `success=end` with
  `success=done` and re-test.
- **`visaged` now handles SIGTERM correctly.** The shutdown signal handler in
  `crates/visaged/src/main.rs` previously relied on `tokio::signal::ctrl_c()`,
  which is SIGINT-only on Unix. `systemctl stop` / `systemctl restart` (and
  `visage-resume.service` after suspend/hibernate) send SIGTERM, which the daemon
  ignored — systemd then waited the default `TimeoutStopSec=90s` before escalating
  to SIGKILL, manifesting as a ~90s hang on `systemctl restart visaged.service`
  after hibernate resume. Visaged now installs handlers for both SIGINT and SIGTERM
  via `tokio::signal::unix::signal` and shuts down cleanly. Fixes #26.
- **`visaged.service` adds `TimeoutStopSec=10s`** as defense in depth — covers the
  edge case where a v4l2 capture is mid-flight and not promptly interruptible
  (e.g. a stale camera fd after hibernate resume). Fixes #26.

### Documentation

- Added ASUS ExpertBook B3302FEA/B5302FEA hardware validation showing the built-in
  Azurewave/IMC `13d3:56ea` UVC webcam is RGB-only and not compatible with
  Visage's secure IR-backed PAM authentication path.

## v0.3.0 — 2026-02-23

### What's changed

- **Security-first model integrity** — ONNX model files are now verified via pinned SHA-256.
  `visage setup` verifies checksums on download and `visaged` verifies the model directory at
  startup (fails closed on missing/mismatched models).
- **Shared model manifest** — added `visage-models` crate containing the model list and
  verification helpers used by both the CLI and daemon.
- **OSS contribution governance** — added `SECURITY.md` (private vulnerability reporting
  via GitHub Security Advisories), branch protection on `main` (required PR + CI + review),
  `CODEOWNERS`, issue/PR templates, DCO sign-off policy, Dependabot for dependency updates,
  and documented merge strategy with review timeline commitments. See ADR 010.

## v0.2.0 — 2026-02-23

### What's changed

- **Enterprise identity compatibility** — D-Bus `Verify(user)` caller validation now resolves user IDs via NSS (LDAP/SSSD/AD compatible) instead of parsing `/etc/passwd`.
- **CLI reliability** — `visage` CLI sets a D-Bus method timeout aligned with `VISAGE_VERIFY_TIMEOUT_SECS` (default 10s) to avoid indefinite hangs.
- **Enrollment quality** — enrollment now averages embeddings across captured frames (confidence-weighted) and re-normalizes the result.
- **Store hardening** — face DB blob parsing validates size/dimension and rejects NaN/Inf safely (no panics on corrupted blobs).
- **Status output** — `Status()` JSON includes additional config fields (paths, timeouts, frame counts, emitter/session flags).

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
