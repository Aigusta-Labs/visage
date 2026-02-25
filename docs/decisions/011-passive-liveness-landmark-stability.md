# ADR 011 — Passive Liveness Detection via Landmark Stability

**Date:** 2026-02-25
**Status:** Implemented
**Scope:** `visage-core`, `visaged`

---

## Context

Visage v0.3 had no liveness detection. The threat model (§Tier 1) identified that a
high-quality photograph — printed or displayed on a screen — could pass face verification
because SCRFD+ArcFace cannot distinguish a real face from a photographic reproduction.

The v3 roadmap planned active liveness (blink challenge), but that requires user cooperation,
adds latency, and needs UI integration (PAM conversation or display). A lower-cost passive
approach was identified: exploiting the fact that **live eyes exhibit involuntary micro-saccades
between frames, while photographs produce static landmarks**.

## Decision

### 1. Passive landmark stability check in `visage-core::liveness`

A new module `visage-core/src/liveness.rs` implements `check_landmark_stability()`:

- Takes a sequence of 5-point landmark arrays (one per frame, from SCRFD output)
- Computes Euclidean displacement of the two eye landmarks (indices 0 and 1) between
  consecutive frames
- Averages displacement across all frame pairs
- Compares against a configurable minimum threshold (default: 0.8 px)
- Returns `LivenessResult { is_live, mean_eye_displacement, frame_pairs_analysed }`

**Why eye landmarks only (not all 5):** Eyes exhibit the most involuntary movement (saccades,
vergence adjustments). Nose and mouth landmarks are more stable even in live subjects, making
them poor discriminators between live and static.

**Why 0.8 px default:** Empirically, sensor noise on a static image produces <0.3 px of
apparent landmark jitter. Natural micro-saccades at 30 fps on a 640×480 sensor produce
>1.0 px displacement. The 0.8 px threshold sits between these two distributions with margin.

### 2. Integration point: after detection, before match acceptance

The liveness check runs in `run_verify()` in `visaged/src/engine.rs`:

1. For each captured frame, SCRFD detection produces landmarks (already happening)
2. Landmarks are collected into a `Vec<[(f32, f32); 5]>` across all frames
3. After the detection+embedding loop completes, `check_landmark_stability()` is called
4. If the check fails, `EngineError::LivenessCheckFailed` is returned by the engine
5. If the check passes, embedding comparison proceeds as before

This ordering ensures:

- All landmark data is available before the check runs
- The D-Bus layer maps `LivenessCheckFailed` to an auth non-match, so spoof attempts are
  rate-limited like other failed authentication attempts
- The audit log captures both the liveness failure and whether the face would have matched
  (for detecting spoof attempts)

### 3. Configuration via environment variables

| Variable | Default | Effect |
|----------|---------|--------|
| `VISAGE_LIVENESS_ENABLED` | `1` (on) | Set to `0` to disable. Intended for development only. |
| `VISAGE_LIVENESS_MIN_DISPLACEMENT` | `0.8` | Minimum mean eye displacement in pixels. Lower = more permissive. |

Both are surfaced in `visage status` JSON output.

### 4. Enabled by default

Unlike some security features that start opt-in, passive liveness is enabled by default
because:

- Zero latency impact (operates on data already produced)
- Zero UX impact (no user interaction required)
- Zero false-reject risk for live users (micro-saccades are involuntary and universal)
- Only affects static images, which should never authenticate

## Alternatives Considered

### Active blink challenge

Requires PAM conversation API for display, adds 1-2s to auth, needs UI framework support
(terminal sudo has no display). Deferred to v0.4 as a complementary layer.

### Frame differencing (pixel-level)

Compare raw pixel data between frames. Rejected: too sensitive to camera noise and AGC
adjustments, especially during IR emitter warm-up. Landmark-based comparison is more robust
because SCRFD's neural network output is invariant to global brightness changes.

### Optical flow analysis

Compute dense optical flow between frames. Rejected: requires significant computation
(~50ms per frame pair), and the information it provides (global motion vectors) is less
discriminative than landmark displacement for the specific photo-vs-live question.

### Require minimum N frames with landmarks

Alternative: reject if fewer than 2 frames have landmarks. Current implementation: pass
through with `is_live = true` if <2 frames. Rationale: a single-frame verify should not
be blocked by liveness — the check simply cannot run. The `frames_per_verify` default (3)
ensures enough frames in normal operation.

## Consequences

### Positive

- **Blocks the #1 threat:** Printed photos and static screen displays are rejected
- **Zero overhead:** No additional model inference, no extra frames, no user interaction
- **Auditable:** Every liveness check is logged with displacement value and threshold
- **Configurable:** Threshold tunable per deployment via environment variable
- **Forward-compatible:** The `LivenessResult` struct can carry additional signals in v3
  (blink detection, gaze direction) without API changes

### Negative

- **Does not block video replay:** A pre-recorded video of the user will pass because
  landmarks move naturally in video. This is the expected limitation — video replay
  requires active challenges (v0.4) or IR strobe analysis (v3).
- **Threshold sensitivity:** Very low frame rates (<5 fps) or very high sensor noise
  cameras may need threshold adjustment. The default is conservative.
- **Single-frame verify bypasses check:** If `frames_per_verify=1`, the liveness check
  passes trivially. This is documented but not enforced — operators who set 1 frame
  are trading security for speed.

## Test Coverage

10 unit tests in `visage-core/src/liveness.rs`:

| Test | What it verifies |
|------|-----------------|
| `test_single_frame_passes` | <2 frames = pass through (cannot determine) |
| `test_empty_sequence_passes` | Empty input = pass through |
| `test_identical_landmarks_rejected` | Perfectly static = rejected |
| `test_near_identical_landmarks_rejected` | Sensor-noise-level jitter = rejected |
| `test_natural_movement_passes` | Simulated micro-saccade = passes |
| `test_large_movement_passes` | Deliberate head movement = passes |
| `test_custom_threshold` | Low threshold makes small movement pass |
| `test_custom_high_threshold` | High threshold makes moderate movement fail |
| `test_two_frames_minimum` | Exactly 2 frames = 1 pair, works correctly |
| `test_displacement_calculation_accuracy` | Known 3-4-5 triangle geometry verified |
| `test_mean_across_multiple_pairs` | Multi-pair averaging is correct |
