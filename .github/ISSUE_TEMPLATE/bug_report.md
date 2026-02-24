---
name: Bug Report
about: Report a bug in Visage (daemon, CLI, PAM module, or packaging)
title: "[Bug] "
labels: bug
assignees: ''
---

## Environment

- **Visage version:** <!-- output of `visage --version` or git commit hash -->
- **OS:** <!-- e.g. Ubuntu 24.04.4 LTS -->
- **Kernel:** <!-- output of `uname -r` -->
- **Camera:** <!-- output of `visage discover`, or N/A if not camera-related -->
- **Install method:** <!-- .deb package / built from source / other -->

## Description

<!-- A clear description of what the bug is. -->

## Steps to reproduce

1. 
2. 
3. 

## Expected behavior

<!-- What should happen. -->

## Actual behavior

<!-- What actually happens. Include error messages verbatim. -->

## Logs

<!-- If relevant, include daemon logs:
     journalctl -u visaged -n 50 --no-pager
     
     For PAM issues:
     sudo grep pam_visage /var/log/auth.log | tail -20
-->

```
<!-- paste logs here -->
```

## Additional context

<!-- Screenshots, config overrides, anything else that helps. -->
