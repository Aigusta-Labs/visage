# Passive Liveness Rollout — Remaining Work

**Date:** 2026-02-25
**Scope:** Final completion checklist for passive liveness detection (landmark stability)

---

## 2026-08-17 — first hardware validation

§2 below ("Manual security validation on real hardware") was executed for the first time on
**ASUS Zenbook 14 UM3406HA / Shinetech `3277:0055`**, an IR module with **no emitter quirk**.
Full detail: [`hardware-reports/asus-zenbook-um3406ha-3277-0055.md`](hardware-reports/asus-zenbook-um3406ha-3277-0055.md).

**Both blocking exit criteria in §2 failed, and §3's calibration premise did not hold.**

| §2 matrix item | Status | Result |
|---|---|---|
| 1. Live user (expect pass) | ⚠️ Partial | **9/11 = 82%.** Below the ≥9-in-10 bar in `STATUS.md` |
| 2. Printed photo spoof | ⬜ Not run | Paper untested — only an emissive screen |
| 3. Static screen image spoof | ❌ **Contradicted** | Blocked, but **not by discrimination** — see below |
| 4. Video replay | ⬜ Not run | |
| 5. `VISAGE_LIVENESS_ENABLED=0` sanity | ⬜ Not run | |

Measurements at `liveness_min_displacement = 0.80`, `verify_n = 3`:

| Subject | Similarity | Displacement | Outcome |
|---|---|---|---|
| Live — `onboard` auto-verify | 0.9847 | **0.263** | rejected |
| Live — via PAM `sudo` | 0.8793 | **0.670** | rejected |
| Live ×9 | 0.4609–0.8759 | ≥0.80 | passed |
| **Phone screen, hand-held** | **0.9013** | **0.681** | rejected by floor |

### Finding 1 — the metric does not separate live from spoof on this module

The spoof's displacement (**0.681**) is *higher* than two genuine live attempts (0.263, 0.670).
**No threshold admits the live minimum while rejecting the spoof.** §3's exit criterion —
*"confirm static attacks remain below threshold"* — is unmet: the static attack measured above
most of the live tail. Calibrating the threshold *downward* to fix the 18% false-reject rate
would admit the spoof.

The gate blocked this spoof only because 0.80 rejects nearly everything on this sensor, live
users included. That is a high floor, not discrimination.

### Finding 2 — the identity stage matched a photograph at 0.9013

Independent of liveness: the enrolled model matched a phone-displayed photo at **0.9013**
against a 0.40 threshold — a stronger match than four genuine live attempts. With liveness
disabled (§2 item 5, or any deployment that turns it off), this hardware authenticates a
photograph. The IR node alone did **not** reject an emissive screen.

### Hypothesis for Finding 1 (untested)

`liveness.rs` states a printed photo yields *"<0.3 px (sensor noise only)"* — that describes a
**mounted, static** photo. A hand-held device shakes, and landmark displacement cannot tell
hand tremor from ocular micro-saccades. **Settling test:** mount the spoof on a stand and
re-measure; if displacement drops below 0.3, tremor is confirmed as the confound and the
threat model's "static photo" wording needs to become "*rigidly mounted* static photo."

### Calibration note

`DEFAULT_MIN_EYE_DISPLACEMENT = 0.8` is documented against **640×480 at 30 fps**. This sensor
is **640×360**. Per-sensor calibration has no supported path today beyond the raw env var.

### The strobe is already available — and it is the discriminator this metric lacks

A 20-frame capture on this module shows **clean lit/unlit alternation** (~84 vs ~36 per frame),
present from the first capture of the session with **no emitter quirk configured**. Comparing
an adjacent pair: the face and room interior go black in the unlit frame while an outdoor scene
through a window is unchanged — the emitter reaches nearby subjects only, and ambient-IR-lit
content is unaffected.

This matters directly for liveness. `threat-model.md` already lists *"IR strobe pattern
detection (odd/even frame analysis)"* as roadmap. On this hardware the signal **exists today**
and nothing consumes it:

| subject | behaviour under strobe |
|---|---|
| live face | strongly alternates — it *reflects* the emitter |
| phone screen / self-emissive display | should not alternate — it *emits* |
| distant ambient-IR-lit scene | does not alternate (measured) |

That is a physical discriminator a display cannot defeat, unlike landmark displacement, which
we measured as indistinguishable (spoof 0.681 vs live 0.263). **Untested on a spoof** — the
prediction for the screen row above must be measured before it is claimed.

⚠️ Retraction, recorded because it was briefly written into these docs: the cold run's dark
frames were first attributed to a strobe, then re-attributed to auto-exposure warm-up on the
strength of a *summary* line (`0 dark skipped`) that omitted per-frame values. The per-frame
sequence reverses that second reading — the dark frames were the unlit half of the strobe all
along, filtered out of the printed list. Frame spacing remains unestablished as a contributor
to the displacement figures.

### Not yet changed, and why

`threat-model.md` still marks *"Static photo (printed or displayed)"* as ✅ mitigated. That
claim is **not** revised on this evidence: n=1 on the spoof side, one sample of unestablished
provenance, and paper untested. Revising a public threat-model claim needs the full matrix
above. Recorded here as measurement, not verdict.

---

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

> **Partially executed 2026-08-17 — items 1 and 3 ran, and both exit criteria FAILED.**
> See [2026-08-17 — first hardware validation](#2026-08-17--first-hardware-validation).
> Still blocking: items 2, 4, 5, and a re-run with a working IR emitter.

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

> **Premise contradicted 2026-08-17.** The exit criterion *"confirm static attacks remain
> below threshold"* did not hold on `3277:0055`: a hand-held screen spoof measured **0.681**,
> above two live samples. On that module no threshold satisfies both criteria at once, so
> calibration alone cannot fix the 18% false-reject rate. See the
> [validation section](#2026-08-17--first-hardware-validation).

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
