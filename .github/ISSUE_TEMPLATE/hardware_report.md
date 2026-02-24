---
name: Hardware Report
about: Report test results for your laptop's IR camera (Adopt-a-Laptop program)
title: "[Hardware Report] "
labels: hardware
assignees: ''
---

## Device info

- **Laptop:** <!-- brand and model, e.g. "Lenovo ThinkPad T14s Gen 2" -->
- **Camera node:** <!-- /dev/video? from `visage discover` -->
- **Camera driver:** <!-- uvcvideo / intel_ipu6 / other -->
- **VID:PID:** <!-- from `visage discover` output -->
- **IR emitter:** <!-- activates (bright frames) / dark frames / not present / unknown -->

## Environment

- **OS:** <!-- e.g. Ubuntu 24.04, Fedora 41, Arch, NixOS -->
- **Kernel:** <!-- output of `uname -r` -->
- **Visage version:** <!-- output of `visage --version` or git commit -->

## Test results

- **Enroll time:** <!-- ~Xs -->
- **Verify time:** <!-- ~Xs -->
- **Match success rate:** <!-- X/3 attempts -->

## Discovery output

```
<!-- paste full output of `visage discover` here -->
```

## Daemon status

```json
<!-- paste output of `visage status` here -->
```

## Notes

<!-- Anything unusual: dark frames, slow capture, emitter not activating, 
     pixel format issues, error messages, etc. 
     
     If you have the IR emitter control bytes (from linux-enable-ir-emitter configure
     or UVC descriptor analysis), include them here — that's everything we need
     to add a quirk entry for your camera. -->
