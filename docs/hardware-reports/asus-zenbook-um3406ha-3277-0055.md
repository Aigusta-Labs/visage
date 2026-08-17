# Hardware Report: ASUS Zenbook 14 UM3406HA (Shinetech `3277:0055`)

**Date tested:** 2026-08-17
**Result:** IR-capable and PAM-functional on ambient infrared. **No emitter quirk** for this
module, and passive liveness does not discriminate a live face from a screen spoof here.
**Validation level:** Full end-to-end — enrollment, D-Bus verify, PAM `sudo` path, and a first
spoof attempt. Read-only on the USB control surface; no UVC control writes were sent.

## Summary

This is the same laptop model Visage names as reference hardware in
[`hardware-compatibility.md`](../hardware-compatibility.md) — ASUS Zenbook 14 UM3406HA — but
carrying a **different camera module** than the one the shipped quirk covers.

`contrib/hw/04f2-b6d9.toml` is titled *"ASUS Zenbook 14 UM3406HA IR Camera"* and keyed to
Chicony `04F2:B6D9`. This unit carries Shinetech `3277:0055`. ASUS ships this chassis with at
least two camera modules, so **the model name does not predict emitter support** —
`visage discover` does, and it reports `no quirk` on all four nodes here.

Three consequences, in order of severity:

1. **A hand-held phone screen matched the enrolled identity at 0.9013 similarity**, and its
   landmark displacement was *higher* than two genuine live attempts. On this module the
   landmark-stability metric does not separate live from spoof. See
   [Liveness measurements](#liveness-measurements).
2. **Passive liveness rejects live users ~18% of the time** on this sensor.
3. **The IR emitter fires anyway — and the daemon says otherwise.** Despite having no quirk,
   the emitter strobes lit/unlit every frame by firmware default. The log line
   `no IR emitter quirk for device; proceeding without illumination` is therefore misleading:
   the quirk is absent, the illumination is not. See
   [the emitter section](#-the-ir-emitter-is-active-on-this-module-with-no-quirk).

**The strobe is an unused anti-spoof primitive.** `threat-model.md` lists *"IR strobe pattern
detection (odd/even frame analysis)"* as roadmap; on this hardware the signal is already being
produced and nothing consumes it. A live face reflects the emitter and alternates strongly; a
self-emissive display does not. That is a physical discriminator the landmark-stability metric
provably lacks here.

Prior art on this exact machine: under Ubuntu 24.04 it ran Howdy 2.6.1 with
`linux-enable-ir-emitter` 7.0.0-beta2 and reached **10/10 consecutive `sudo` authentications**,
also observing the strobe (reported there as ~35% dark frames).

## Host

```text
Hostname: the-first
Vendor:   ASUSTeK COMPUTER INC.
Product:  ASUS Zenbook 14 UM3406HA_UM3406HA
Board:    UM3406HA
BIOS:     UM3406HA.303
OS:       Esver OS 1.0 (Aegis)
Kernel:   Linux 7.1.4 x86_64
CPU:      AMD Ryzen 7 8840HS w/ Radeon 780M Graphics
Visage:   0.3.6
```

## Camera discovery

```text
Bus 003 Device 002: ID 3277:0055 Shinetech USB2.0 FHD UVC WebCam

$ visage discover
/dev/video0  driver=uvcvideo  VID=0x3277 PID=0x0055  no quirk (VID=0x3277 PID=0x0055)
/dev/video1  driver=uvcvideo  VID=0x3277 PID=0x0055  no quirk (VID=0x3277 PID=0x0055)
/dev/video2  driver=uvcvideo  VID=0x3277 PID=0x0055  no quirk (VID=0x3277 PID=0x0055)
/dev/video3  driver=uvcvideo  VID=0x3277 PID=0x0055  no quirk (VID=0x3277 PID=0x0055)
```

Four nodes, two streams. `ID_V4L_PRODUCT` disambiguates them:

| Nodes | `ID_V4L_PRODUCT` suffix | Stream |
|---|---|---|
| `/dev/video0`, `/dev/video1` | `USB2.0 F` | RGB |
| `/dev/video2`, `/dev/video3` | `USB2.0 I` | **Infrared** |

Configure `camera = "/dev/video2"`. Note the spoof result below before assuming the IR node
by itself defeats a displayed photograph — it did not defeat an emissive phone screen.

## Capture quality

```text
$ visage test
Opening /dev/video2...
  Format: GREY 640x360
Capturing 10 frames...
  Captured: 10 good, 9 dark skipped
Average brightness: 59.5
```

### ⚠️ The IR emitter IS active on this module, with no quirk

This is the most consequential finding in this report, and it contradicts the daemon's own
log line. A 20-frame capture shows **clean per-frame strobe alternation**:

```text
$ visage test -n 20
[0] 82.7   [1] 32.0   [2] 81.5   [3] 32.7   [4] 80.7   [5] 33.7
[6] 81.6   [7] 34.7   [8] 83.1   [9] 36.0  [10] 85.2  [11] 36.5  …
```

Lit frames ~81–85, unlit ~32–39, alternating every frame. Visual comparison of an adjacent
pair confirms the mechanism: **the face and room interior go black in the unlit frame, while
the outdoor scene through a window is unchanged** — sunlight is IR-rich, so distant content
stays lit while the emitter's own illumination reaches only nearby subjects.

The strobe was present in the **first** capture of the session, before any external tool was
fetched or run, so it is firmware-default (or persisted) — not something we induced. The 9
"dark skipped" frames in that cold 10-frame run were the **unlit half of the strobe**, filtered
out of the printed list; they were never an exposure artifact.

**Therefore `no IR emitter quirk for device; proceeding without illumination` is misleading.**
The quirk is genuinely absent — `visage discover` is correct about that — but illumination is
*present*. Absence of a quirk entry does not imply absence of illumination on hardware whose
firmware enables face-auth mode by default. The message conflates the two.

Auto-exposure additionally ramps across a cold capture (52.8 → 68.0), which is a separate and
much smaller effect.

The sensor is **640×360**, not the 640×480 that `DEFAULT_MIN_EYE_DISPLACEMENT` was calibrated
against.

Visual inspection of a captured frame confirms a clearly resolved, well-exposed face on
ambient IR alone in a normally-lit room.

## Enrollment and authentication

Enrolled via `sudo visage onboard` — 7 models across `normal`, `left`, `right`, `glasses`,
quality **0.7404–0.8685**.

PAM is wired at order 900 in `sudo` and `login`:

```text
auth [success=done default=ignore]  pam_visage.so   # visage (order 900)
auth sufficient                     pam_unix.so     # fallback
auth required                       pam_deny.so
```

The `sudo` path was exercised end to end, including fallback:

```text
pam_visage: no match for user 'cc'
op=PAM:authentication grantors=pam_unix acct="cc" res=success
```

**Identity matching never failed** across 12 attempts. Similarity ranged **0.4609–0.9847**
against a 0.40 threshold.

**Live pass rate: 9/11 = 82%**, below the ≥9-in-10 bar in [`STATUS.md`](../STATUS.md). Every
failure was the liveness gate vetoing a face it had already recognised — never an identity
miss.

## Liveness measurements

Threshold in force: `liveness_min_displacement = 0.80` px, `verify_n = 3` frames.

The daemon logs displacement only on rejection, so passing attempts are known to be ≥0.80 but
their margin is unmeasured — raising `logLevel` to `debug` exposes it via the
`"liveness check"` trace in `engine.rs`.

| Subject | Similarity | Displacement | Outcome |
|---|---|---|---|
| Live — `onboard` auto-verify | 0.9847 | **0.263** | rejected |
| Live — via PAM `sudo` | 0.8793 | **0.670** | rejected |
| Live ×9 | 0.4609–0.8759 | ≥0.80 | passed |
| **Phone screen, hand-held** | **0.9013** | **0.681** | rejected by floor |
| Unattributed¹ | 0.7951 | 0.306 | rejected |

¹ Provenance not established — either a third sub-floor live sample or an earlier spoof
attempt. Recorded rather than assigned.

**The spoof's displacement exceeds the live minimum.** No threshold on this metric admits the
0.263 live sample while rejecting the 0.681 spoof. Lowering the floor to fix the 82% pass rate
would admit the spoof. The gate currently blocks it only because 0.80 is high enough to reject
most things, live faces included.

**Hypothesis (untested):** the metric cannot distinguish ocular micro-saccades from tremor in
the hand holding the spoof. `liveness.rs` documents *"a printed photo produces <0.3 px (sensor
noise only)"* — which describes a **mounted, static** photo. The settling test: mount the
phone on a stand and re-measure. If displacement drops below 0.3, tremor is confirmed as the
confound.

**Sample size is n=1 on the spoof side.** This is recorded as a measurement, not yet as a
threat-model revision.

## Recommended configuration

```nix
services.visage = {
  enable = true;
  camera = "/dev/video2";   # IR node — NOT video0/1
};
```

Leave `liveness.minDisplacement` at its 0.80 default. It is a weak barrier on this module, but
until an emitter quirk or a stronger liveness signal lands it is the only one, and lowering it
measurably widens what a screen spoof can do.

## Open work for this module

- [ ] Derive emitter control bytes for `3277:0055` via `linux-enable-ir-emitter configure` and
      contribute `contrib/hw/3277-0055.toml`. Known-achievable on this hardware — see the
      prior-art note in the Summary.
- [ ] Re-run the spoof matrix **with the emitter firing**. Skin and an LCD panel reflect
      infrared very differently, and active illumination may supply the physical
      discrimination the IR node alone did not.
- [ ] Test printed paper separately from an emissive screen.
- [ ] Measure displacement on *passing* attempts (`logLevel = "debug"`) to establish the live
      distribution rather than only its sub-threshold tail.
- [ ] Consider whether the emitter's strobe (~35% dark frames) interacts with `verify_n = 3`,
      since retained-frame spacing drives the displacement metric.
