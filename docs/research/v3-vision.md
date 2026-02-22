# Visage v3 Vision — Forward-Looking Architecture

**Date:** 2026-02-21
**Status:** Design vision — informs v2 architectural decisions
**Purpose:** Define what v3 represents and identify v2 decisions that must not block it.

---

## Why This Document Exists

Visage v2 is a clean-sheet redesign of Howdy. If v2 only fixes Howdy's known defects,
it is an incremental improvement — not the generational leap that justifies a new project.
The value of v2 is measured not just by what it delivers, but by what it *enables*.

This document defines the v3 horizon: the capabilities that v3 should deliver, and the
v2 architectural decisions required to keep that path open. Every v2 design choice is
evaluated against one question: **does this decision make v3 cheaper, more expensive,
or impossible?**

---

## Version Semantics

| Version | Identity | Scope |
|---------|----------|-------|
| Howdy | v1 — proof of concept | Face auth works on Linux, barely |
| Visage v2 | Correct foundation | Reliable, secure, fast face auth via persistent daemon |
| Visage v3 | Intelligent biometric platform | Self-calibrating, hardware-adaptive, multi-modal, AI-assisted lifecycle |

v2 must be usable and complete without any v3 capability. v3 is not a prerequisite
for v2's success — it is a *consequence* of v2's design quality.

---

## Five Dimensions of v3

### 1. Near-Perfect Hardware Reliability

**v1 (Howdy):** Each camera requires manually discovered UVC control bytes. Two of three
camera backends have bugs. No auto-detection. Success depends on luck and community wikis.

**v2 (current):** Single V4L2 implementation. Hardware quirks database (`contrib/hw/*.toml`).
`visage discover` probes UVC extension units. Format auto-detection (GREY, YUYV). But
still one camera at a time, manual quirk discovery.

**v3 target:** Near-zero-config camera support on any IR-equipped Linux laptop.

**How:**

- **UVC descriptor-guided probing** — Instead of brute-forcing all 20 × 50 = 1000
  unit/selector combinations (slow, can brick hardware), read the UVC descriptor
  tables first. They advertise which extension units exist and their supported
  control selectors. Probe only advertised combinations. This reduces the search
  space from 1000 to typically 5-15 safe attempts.

- **Camera capability fingerprinting** — At first boot or `visage setup`, build a
  profile: supported pixel formats, resolution range, frame rate stability, native
  IR response curve, dark frame rate with/without emitter, sensor noise floor. Store
  as JSON alongside the quirk entry. This profile enables:
  - Automatic CLAHE parameter tuning per camera model
  - Dark frame threshold calibration per sensor
  - Frame rate adaptation for power-constrained environments

- **Community quirk repository with opt-in telemetry** — Users who successfully
  configure a camera can submit their quirk entry via `visage contribute`. Submission
  is privacy-safe: VID:PID + control bytes + negotiated format. No images, no
  embeddings, no user data. A central repository aggregates submissions.
  The daemon periodically fetches updated quirks (opt-in).

- **Graceful degradation to visible-light mode** — When no IR camera or emitter
  is detected, operate in visible-light mode with explicit security downgrade warnings.
  Visible-light face auth is strictly inferior (spoofable with photos), but better than
  nothing for laptop users without IR hardware. Liveness detection becomes mandatory
  in this mode.

**v2 decisions that enable this:**
- Quirks database format must be machine-writable (TOML, not hardcoded Rust enums)
- `visage discover` must output structured data that can be directly saved as a quirk
- Camera capability metadata should be part of the `DeviceInfo` struct (add fields
  as needed, don't block on it now)

**v2 anti-pattern to avoid:**
- Do NOT create a `CameraBackend` trait with V4L2 as one implementation. V4L2 is the
  only backend. Adding the trait now is premature abstraction. When (if) a second backend
  arrives, the refactor from concrete to trait is straightforward and mechanical. Build
  the trait when there are two implementations, not before.

---

### 2. Environmental Adaptability

**v1 (Howdy):** Static global threshold (certainty = 3.5). Ships too tight for most
hardware. No feedback on why authentication fails. Same parameters regardless of
lighting, distance, or face angle.

**v2 (current):** Better models (SCRFD + ArcFace), but still static threshold per
deployment. Manual calibration via `visage calibrate`.

**v3 target:** Adaptive recognition that improves with use and adjusts to environmental
variation.

**How:**

- **Per-user adaptive thresholds** — After enrollment, compute the intra-class variance
  of that user's embeddings. Users with highly distinctive features (low variance) can
  tolerate a tighter threshold. Users with common features (higher variance) need
  looser thresholds. The threshold becomes a function of the enrolled embedding
  distribution, not a global constant.

  Formula: `threshold = mean_self_distance + k * std_self_distance` where `k` is a
  configurable security parameter (default 2.5, adjustable per deployment).

- **Multi-enrollment with quality scoring** — Enroll 3-5 poses: straight, slight left,
  slight right, with glasses, without glasses. Score each enrollment on quality metrics:
  - Face size in frame (too small → low confidence at recognition time)
  - Landmark alignment confidence (SCRFD outputs per-landmark scores)
  - Image sharpness (Laplacian variance)
  - Illumination uniformity (histogram entropy)
  Reject enrollments below quality threshold. Weight matches by enrollment quality.

- **Environmental context signals** — Each authentication frame carries implicit
  environmental data computable from existing pipeline outputs:
  - Ambient light level → frame histogram mean
  - Face distance → detection bounding box area relative to frame
  - Face angle → landmark geometry (inter-eye distance ratio, nose-to-chin vector)
  Use these to select the closest-matching enrollment or apply a small threshold
  adjustment.

- **Failed-match learning loop (opt-in)** — When face auth fails and the user
  authenticates via password within 10 seconds, optionally store the failed frame's
  embedding (encrypted, ephemeral, auto-deleted after 24h). If the same face fails
  3+ times and always follows with successful password auth, prompt the user to
  re-enroll at that distance/angle/lighting.

  This creates a closed learning loop: failure → password fallback → re-enrollment
  prompt → improved coverage → fewer failures.

- **Temporal enrollment drift (cautious)** — Faces change over months (aging, facial
  hair, weight). v3 could slowly update enrolled embeddings using successful
  authentications, with strict guards:
  - Maximum drift rate: embedding can shift at most 0.01 cosine distance per week
  - Drift only from high-confidence matches (>0.8 similarity)
  - Maintain original enrollment as immutable anchor — never overwrite it
  - User can disable drift entirely

  This is the most controversial v3 feature. The risk is adversarial drift: an attacker
  who can authenticate at slightly-below-threshold similarity could gradually shift the
  enrolled embedding toward their own face over weeks. The drift rate limit and immutable
  anchor mitigate this, but the feature should be opt-in and off-by-default.

**v2 decisions that enable this:**

1. **Rich match results** — `Verify` returns a struct, not a bool:
   ```rust
   pub struct MatchResult {
       pub matched: bool,
       pub similarity: f64,
       pub model_id: String,
       pub model_label: String,
       pub face_quality: f32,
       pub face_bbox: [f32; 4],    // enables distance estimation
       pub latency_ms: u64,
   }
   ```
   Cost: one struct. Enables: adaptive thresholds, diagnostics, analytics.

2. **SQLite schema with quality metadata** — Add columns to the existing schema:
   ```sql
   ALTER TABLE models ADD COLUMN quality_score REAL DEFAULT 0.0;
   ALTER TABLE models ADD COLUMN pose_label TEXT DEFAULT 'unknown';
   ```
   Cost: two columns. Enables: multi-enrollment, weighted matching.

3. **Frame carries environmental metadata** — Add fields to `Frame`:
   ```rust
   pub struct Frame {
       // ... existing fields ...
       pub histogram_mean: f32,     // ambient light proxy
       pub histogram_entropy: f32,  // illumination uniformity
   }
   ```
   Cost: two fields + 10 lines of computation. Enables: environmental context.

4. **Embedding comparison behind a trait:**
   ```rust
   pub trait Matcher: Send + Sync {
       fn compare(&self, probe: &[f32], gallery: &[f32]) -> f64;
   }
   ```
   With `CosineMatcher` as the v2 default. v3 swaps in `AdaptiveMatcher` without
   touching the daemon. Cost: one trait + one impl. Enables: pluggable matching.

**v2 anti-pattern to avoid:**
- Do NOT implement adaptive thresholds in v2. Collect the data that enables them
  (rich match results, quality scores), defer the algorithm. Premature adaptive
  logic is harder to debug than a well-understood static threshold.

---

### 3. AI-Augmented System Intelligence

**v1 (Howdy):** AI used only for the core recognition task (dlib detect + embed + compare).
No intelligence in setup, diagnostics, or lifecycle management.

**v2 (current):** Better AI for recognition (SCRFD + ArcFace). But setup, diagnostics,
and calibration are still manually coded heuristics.

**v3 target:** AI used to improve *the system itself* — not just the faces it recognizes.

**How:**

#### 3a. Hardware Compatibility Classifier

A small neural network (not an LLM) trained on UVC descriptor patterns to predict
which extension unit / selector / control byte combinations are likely to activate the
IR emitter for an unknown camera.

Training data: the community quirks repository. Each entry maps UVC descriptor features
(unit count, selector count, max control size, device class, vendor patterns) to the
working control bytes. A few hundred entries would be sufficient for a useful classifier.

This is a bounded, well-defined ML task: input is a fixed-size feature vector from UVC
descriptors, output is a ranked list of (unit, selector, control_bytes) candidates to
try. The model ships with Visage as an ONNX file (~100KB) and runs locally.

**v2 enabler:** `visage discover` must output structured UVC descriptor data in a
machine-readable format, not just "try this control and see what happens."

#### 3b. Enrollment Quality Model

Evaluate enrolled face images for quality using the same SCRFD landmarks plus
learned quality metrics. Guide the user in real-time during enrollment:

```
"Face detected — good alignment"
"Move slightly closer — face is too small for reliable matching"
"Turn slightly left — need a side-angle enrollment"
"Remove glasses for one enrollment — improves matching consistency"
```

v2 can implement the heuristic version (bbox size, landmark confidence, sharpness).
v3 replaces heuristics with a dedicated quality model (ONNX, ~500KB) that predicts
match-time recognition accuracy from the enrollment frame.

**v2 enabler:** Frame quality metrics (sharpness, landmark confidence, bbox area)
must be computed and available — even if v2 doesn't act on them beyond basic rejection.

#### 3c. Authentication Anomaly Detection

Track authentication patterns over time. Not liveness detection (that's per-auth,
real-time). This is behavioral pattern analysis across days/weeks:

- Sudden change in average face distance (user changed chair/monitor?)
- Authentication attempts at unusual times
- Repeated failures from a specific angle (hardware degradation? emitter dying?)
- Match confidence trending downward over weeks (face drift? enrollment stale?)

This does NOT make auth decisions. It produces advisory alerts:
```
"IR emitter may be degrading — dark frame rate has increased from 10% to 60% over 30 days"
"Average match confidence has dropped 15% in 2 weeks — consider re-enrollment"
```

**v2 enabler:** Structured event log per authentication attempt. Every `Verify` call
should log: timestamp, user, dark_frames_skipped, total_frames_captured, best_similarity,
matched (bool), face_quality, latency_ms. This log is the training data for anomaly
detection.

#### 3d. Self-Calibration

After N successful authentications (default: 50), automatically adjust:
- Per-user threshold (based on observed similarity distribution)
- CLAHE parameters (based on observed histogram distributions)
- Dark frame threshold (based on observed dark frame rates)

The adjustment is bounded: no parameter can drift more than 20% from its initial value.
Resets to defaults on re-enrollment. User can disable self-calibration entirely.

**v2 enabler:** The structured event log from 3c provides the data. The daemon config
must support per-user parameter overrides (not just global). TOML naturally supports this:
```toml
[threshold]
default = 0.5

[threshold.overrides]
ccross = 0.47  # auto-calibrated 2026-03-15
```

#### 3e. What AI Should NOT Do

**AI must never make authentication decisions.** The auth path must be:
- Deterministic (same input → same output)
- Fast (<200ms inference, no LLM round-trip)
- Auditable (every step logged with numeric values, not "the model said yes")
- Offline-capable (no network dependency)

AI in v3 is for *system optimization and user assistance* — never for the
pass/fail decision itself.

---

### 4. LLM-Powered Lifecycle Management

**v1 (Howdy):** No guided setup. User reads wiki, edits INI files, runs
`howdy test` and hopes.

**v2 (current):** Better CLI (`visage test`, `visage calibrate`), but still
traditional command-line workflows with static help text.

**v3 target:** An optional AI-powered assistant for setup, diagnostics, and
troubleshooting — strictly outside the authentication path.

**How:**

#### 4a. Setup Wizard

An LLM-powered `visage setup` that walks users through first-time configuration:

```
$ visage setup
Visage setup assistant (using local model via Ollama)

I found 2 cameras:
  /dev/video0 — USB2.0 FHD UVC WebCam (RGB, 640x480)
  /dev/video2 — USB2.0 FHD UVC WebCam (IR, GREY 640x360)

The IR camera at /dev/video2 is the best choice for secure face authentication.
IR cameras work in all lighting conditions and resist photo spoofing.

I'll now probe for the IR emitter control bytes. This is safe — I only test
controls that your camera's UVC descriptors advertise...

Found working IR emitter configuration:
  Unit: 14, Selector: 6, Control: [1, 3, 3, 0, 0, 0, 0, 0, 0]

Let's do a test capture...
[saves 10 frames to /tmp/visage-test/]
8 of 10 frames captured successfully (2 dark — normal during emitter warm-up).
Average brightness: 127.4 — excellent IR illumination.

Ready to enroll your face. Look straight at the camera...
```

The LLM does not make decisions. It narrates the deterministic pipeline in
natural language, provides context on what each step means, and explains
failures in human terms instead of error codes.

**Architecture:** `visage setup` is a separate binary or subcommand that depends
on an LLM backend (Ollama, cloud API). The core `visage` binary and `visaged`
daemon have zero LLM dependencies. The setup wizard is optional — `visage enroll`
works without it.

#### 4b. Diagnostic Agent

When authentication reliability degrades, `visage diagnose` analyzes the
structured event log and explains what changed:

```
$ visage diagnose
Analyzing last 7 days of authentication events...

Issue found: Dark frame rate increased from 12% to 78% starting 2026-03-10.
This typically indicates the IR emitter is no longer activating properly.

Possible causes:
1. Kernel update changed V4L2 driver behavior (you updated to 6.18 on 2026-03-10)
2. IR emitter hardware failure
3. UVC control bytes need re-discovery after driver change

Recommended action: Run `visage discover` to re-probe the IR emitter.
If that doesn't find working controls, try rolling back to kernel 6.17.
```

This is log analysis + pattern matching + natural language explanation. The
diagnostic logic can be rule-based initially; the LLM adds natural language
articulation and handles novel failure patterns.

#### 4c. Quirk Contribution Agent

When a user discovers working IR emitter control bytes for a new camera:

```
$ visage contribute
I'll prepare a hardware quirk contribution for the Visage community database.

Your camera: VID=0x13D3, PID=0x56D0
Working controls: Unit 14, Selector 6, Bytes [1, 3, 3, 0, 0, 0, 0, 0, 0]
Pixel format: GREY 640x360

I've created contrib/hw/13d3-56d0.toml with your camera's configuration.
This file contains only the hardware identifiers and control bytes — no
images or personal data.

Shall I open a pull request to the Visage quirks repository?
[y/N]
```

#### 4d. Boundaries

| Layer | LLM Permitted? | Why |
|-------|---------------|-----|
| Authentication path | Never | Determinism, speed, auditability |
| Enrollment | Guidance text only | "Move left" is computable from landmarks |
| Setup wizard | Yes (optional) | User-facing, latency-tolerant |
| Diagnostics | Yes (optional) | Log analysis, latency-tolerant |
| Quirk contribution | Yes (optional) | PR formatting, latency-tolerant |
| System configuration | Never | Config must be deterministic and reproducible |
| Threshold adjustment | Never | Must be algorithmic with auditable formula |

**v2 enabler:** The setup, diagnostic, and contribution workflows must exist as
deterministic CLI commands first. The LLM layer wraps them — it does not replace
them. `visage enroll`, `visage discover`, and `visage test` are the substrate.
The LLM adds narration and explanation.

**v2 anti-pattern to avoid:**
- Do NOT add any LLM dependency to any v2 crate. Not even as an optional feature.
  The LLM integration lives in a separate `visage-assistant` crate (or external tool)
  that calls the core CLI commands.

---

### 5. Multi-Modal Biometric Platform

**v1 (Howdy):** Face only.

**v2 (current):** Face only. Correct decision — do one thing well.

**v3 target:** Face + voice authentication under a unified API.

**Why face + voice (and not more):**

| Modality | Hardware | Linux Ecosystem | v3 Fit |
|----------|----------|-----------------|--------|
| Face (IR) | Every Windows Hello laptop | Howdy (broken), nothing else | Already implemented |
| Voice | Every laptop microphone | Nothing | Underserved, same hardware class |
| Fingerprint | Dedicated sensor | fprintd (mature, well-maintained) | Competing with fprintd = waste |
| Iris | No consumer hardware | Nothing | No hardware to target |
| Behavioral | Keyboard/mouse patterns | Experimental | Too noisy for auth |

Face + voice share the same hardware class (camera + microphone — both standard on
every laptop). They are independent biometric channels: spoofing both simultaneously
is substantially harder than spoofing either one. Multi-modal fusion (require both
channels or accept either with higher confidence thresholds) is the enterprise direction
for biometric security.

**How:**

#### 5a. Voice Authentication Pipeline

```
Audio capture (ALSA/PipeWire, 16kHz mono)
    │
    ▼
Voice Activity Detection (Silero VAD, ONNX, ~2ms)
    │ outputs: speech segments
    ▼
Speaker embedding extraction (ECAPA-TDNN or TitaNet, ONNX, ~15ms)
    │ outputs: 192-D or 512-D normalized vector
    ▼
Cosine similarity against enrolled voiceprints
    │
    ▼
Threshold check → Accept / Reject
```

The pipeline mirrors the face pipeline structurally: capture → detect → embed →
compare. This is not a coincidence — it is the core architectural insight that
enables multi-modal support without separate codepaths.

#### 5b. Unified Pipeline Abstraction

```rust
pub trait BiometricPipeline: Send + Sync {
    type CaptureOutput;
    type Detection;
    type Embedding;

    fn capture(&self) -> Result<Self::CaptureOutput, PipelineError>;
    fn detect(&self, input: &Self::CaptureOutput) -> Result<Vec<Self::Detection>, PipelineError>;
    fn embed(&self, detection: &Self::Detection) -> Result<Self::Embedding, PipelineError>;
    fn compare(&self, probe: &Self::Embedding, gallery: &Self::Embedding) -> f64;
}
```

`FacePipeline` and `VoicePipeline` both implement this trait. The daemon orchestrates
them generically: capture from the appropriate sensor, run through detect → embed →
compare, return the result.

#### 5c. Multi-Modal Fusion

When both face and voice are enrolled, three authentication modes:

| Mode | Behavior | Security Level |
|------|----------|---------------|
| `any` | Either modality succeeds → auth passes | Convenience (default) |
| `all` | Both must succeed → auth passes | High security |
| `adaptive` | Use face alone if high-confidence (>0.8); require both if marginal | Balanced |

The fusion logic is in the daemon, not the PAM module. The PAM module still calls
`Verify(user)` and gets pass/fail. The daemon decides which modalities to invoke
based on configuration and available hardware.

#### 5d. D-Bus API Evolution

v2 ships `org.freedesktop.Visage1` — face only. v3 adds `org.freedesktop.Visage2`:

```
Verify(user: s, modality: s)  → (matched: b, confidence: d, ...)
  modality: "face" | "voice" | "multi" | "auto" (default)

Enroll(user: s, label: s, modality: s) → (model_id: s)
```

The `Visage1` bus name remains backward-compatible — it maps to `Verify(user, "face")`.

#### 5e. SQLite Schema Evolution

```sql
ALTER TABLE models ADD COLUMN modality TEXT DEFAULT 'face';
ALTER TABLE models ADD COLUMN embedding_dim INTEGER DEFAULT 512;
```

Voice embeddings have different dimensionality (192-D for ECAPA-TDNN vs 512-D for
ArcFace). The schema must not assume a fixed embedding size.

**v2 decisions that enable this:**

1. **Embedding stored as BLOB, not fixed-size array** — The existing SQLite schema
   already stores embeddings as BLOB. Good. Do not change this to a fixed-width column.

2. **Pipeline stages as separable functions** — v2's `camera.rs` → `frame.rs` →
   `core::detect()` → `core::embed()` → `core::compare()` flow should remain
   function-call-based (not monolithic). v3 wraps each stage in the trait.

3. **D-Bus method signatures use named parameters, not positional** — Makes adding
   optional parameters backward-compatible.

**v2 anti-patterns to avoid:**
- Do NOT add `modality` to the v2 D-Bus API. It leaks unimplemented capability into
  the public contract. `Visage2` bus name handles this cleanly.
- Do NOT implement the `BiometricPipeline` trait in v2. The concrete face pipeline
  functions are sufficient. Add the trait when voice arrives.
- Do NOT add audio capture to `visage-hw`. Audio is a different hardware domain with
  different abstractions (ALSA/PipeWire vs V4L2). It belongs in a future `visage-audio`
  crate.

---

## Summary: v2 Design Requirements from v3 Vision

### Must Do in v2 (cheap now, expensive to retrofit)

| Requirement | Cost | Enables |
|-------------|------|---------|
| `MatchResult` struct (not bool) from Verify | 1 struct definition | Adaptive thresholds, diagnostics, analytics |
| Structured event log per auth attempt | 1 tracing macro per pipeline stage | Anomaly detection, self-calibration, LLM diagnostics |
| Frame quality metadata (histogram mean, entropy) | 2 fields + 10 lines | Enrollment quality scoring, environmental adaptation |
| `Matcher` trait with `CosineMatcher` default | 1 trait + 1 impl | Pluggable adaptive matching |
| SQLite: `quality_score` + `pose_label` columns | 2 columns | Multi-enrollment, weighted matching |
| Embeddings stored as variable-length BLOB | Already done | Multi-modal embeddings |
| `visage discover` outputs structured data | JSON output format | Hardware compatibility classifier training data |
| Quirks database as TOML files (not hardcoded) | Already done | Community contribution pipeline |

### Must NOT Do in v2 (premature, blocks nothing)

| Anti-Pattern | Why Not |
|-------------|---------|
| `CameraBackend` trait | No second backend exists. Refactor is mechanical when one arrives. |
| `BiometricPipeline` trait | No second modality exists. Same reasoning. |
| `modality` parameter in D-Bus API | Leaks unimplemented capability. `Visage2` bus name handles it. |
| Adaptive thresholds | Collect data first. Static threshold is debuggable. |
| LLM dependency in any core crate | Keep core deterministic. LLM lives in separate `visage-assistant`. |
| Temporal enrollment drift | Controversial. Collect match-quality data for a year, then evaluate. |
| Audio capture in visage-hw | Different hardware domain. Future `visage-audio` crate. |

### Design Principle

**Build the data plane for v3. Build the control plane for v2.**

v2 collects rich, structured data at every pipeline stage. v3's intelligence features
consume that data. The expensive part of v3 is not the AI models — it's having the
right data to train and evaluate them. v2's job is to instrument the pipeline thoroughly
and store the telemetry, so v3 can focus on the algorithms.

---

## Open Questions (for v3 design phase)

1. **Voice enrollment UX:** Face enrollment is "look at camera." Voice enrollment
   requires speaking a phrase. What phrase? How long? Can it be the user's name?
   Passphrase? A standardized pangram? This has security implications (replay attacks
   with recorded phrases).

2. **Multi-modal fusion confidence calibration:** Face and voice similarity scores
   are not on the same scale. How do you combine them? Weighted average? Learned
   fusion model? This is an active research question.

3. **Privacy of telemetry data:** The structured event log contains authentication
   timing, match confidence, and environmental metadata. Is this a privacy concern?
   Should the log be encrypted at rest? Auto-deleted after N days? User-configurable
   retention?

4. **Community quirks repository governance:** Who reviews submitted quirk entries?
   Can a malicious submission brick cameras? What validation is required before a
   quirk is accepted?

5. **Drift detection vs drift acceptance:** Should the system detect that a user's
   face has drifted from enrollment (alert) or accept the drift (adapt)? These are
   opposite responses to the same signal. The right answer may depend on deployment
   context (personal laptop vs shared workstation).

---

## References

- Microsoft Windows Hello Enhanced Sign-in Security — https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/windows-hello-enhanced-sign-in-security
- ECAPA-TDNN: Emphasized Channel Attention, Propagation and Aggregation in TDNN (2020) — https://arxiv.org/abs/2005.07143
- NVIDIA TitaNet: Neural Model for Speaker Representation (2022) — https://arxiv.org/abs/2110.04410
- Silero VAD — https://github.com/snakers4/silero-vad
- ISO/IEC 24745 Biometric Information Protection — https://www.iso.org/standard/75302.html
- NIST Speaker Recognition Evaluation — https://www.nist.gov/itl/iad/mig/speaker-recognition
