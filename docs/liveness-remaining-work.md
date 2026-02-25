# Passive Liveness Rollout — Remaining Work

**Date:** 2026-02-25
**Scope:** Final completion checklist for passive liveness detection (landmark stability)

## What is already complete

- Core liveness module implemented with unit tests:
  - `crates/visage-core/src/liveness.rs`
  - exported via `crates/visage-core/src/lib.rs`
- Verify pipeline integration and config wiring:
  - `crates/visaged/src/engine.rs`
  - `crates/visaged/src/config.rs`
  - `crates/visaged/src/dbus_interface.rs`
- Packaging and docs updates:
  - `packaging/nix/module.nix`
  - `packaging/systemd/visaged.service`
  - `docs/threat-model.md`
  - `docs/architecture.md`
  - `docs/operations-guide.md`
  - `docs/decisions/011-passive-liveness-landmark-stability.md`

## Remaining work to mark this feature complete

### 1) Build/test validation (blocking)

This environment does not have `cargo` installed, so compilation/test validation has not
been executed here.

Run locally:

```bash
cargo check --workspace
cargo test -p visage-core
cargo test -p visaged
```

Exit criteria:
- All crates compile successfully
- No new test regressions
- `visage-core` liveness tests pass

### 2) Manual security validation on real hardware (blocking)

Run on a machine with the target IR camera setup.

#### Required test matrix

1. **Live user test (expected pass):**
   - Verify with enrolled user in normal lighting/IR
   - Expect successful match

2. **Printed photo spoof (expected fail):**
   - Present high-quality print of enrolled user
   - Expect non-match and rate limiter failure increment

3. **Static screen image spoof (expected fail):**
   - Present still image on phone/laptop screen
   - Expect non-match and rate limiter failure increment

4. **Video replay (known limitation):**
   - Present moving video of enrolled user
   - May pass passive liveness (documented limitation)

5. **Liveness disabled sanity check:**
   - Set `VISAGE_LIVENESS_ENABLED=0`
   - Confirm static photo test behavior changes accordingly

Exit criteria:
- Static photo/image attacks are blocked in default config
- Logs clearly record liveness failure path
- No unexpected false rejects for live users in typical conditions

### 3) Threshold calibration (recommended)

Default threshold is `VISAGE_LIVENESS_MIN_DISPLACEMENT=0.8`.

Perform quick calibration on target camera models:
- Collect displacement observations for live users across multiple sessions
- Confirm static attacks remain below threshold
- Adjust threshold only if needed and document final production value

Exit criteria:
- Threshold validated for target deployment hardware
- Chosen value documented in deployment notes

### 4) Optional hardening/tests before merge (recommended)

Not strictly required for initial merge, but desirable:
- Add `visaged` unit/integration test coverage for D-Bus mapping of
  `LivenessCheckFailed` to non-match behavior
- Add CI scenario asserting new env vars parse and appear in status output

## Out of scope for this completion

- Active liveness challenge (blink/head-turn)
- Defending video replay attacks
- New model-based anti-spoofing

Those remain roadmap items (v0.4+/v3).
