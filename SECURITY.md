# Security Policy

## Scope

Visage is a PAM authentication module. Vulnerabilities in the authentication path
have direct security impact — they can bypass login, leak biometric data, or
escalate privileges.

The following components are security-critical:

| Component | Risk | Examples |
|-----------|------|----------|
| `pam-visage` | **Critical** — PAM auth path | Auth bypass, PAM_SUCCESS without match, FFI unsoundness |
| `visaged` | **Critical** — daemon with root privileges | Privilege escalation, D-Bus method access control bypass, arbitrary code execution |
| `visage-core` | **High** — inference pipeline | Embedding comparison bypass, constant-time violation leaking similarity scores |
| `visage-models` | **High** — model integrity gate | Checksum bypass allowing tampered model loading |
| `visage-hw` | **Medium** — camera and emitter control | Denial of service via camera resource exhaustion |
| `visage-cli` | **Low** — user-facing CLI | Command injection via crafted input (unlikely — no shell calls) |
| Packaging (`debian/`, systemd units) | **Medium** — system integration | File permission errors, insecure tmpfiles, service misconfiguration |

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.3.x | ✅ Current release |
| < 0.3 | ❌ No backports |

Visage is pre-1.0 software. Security fixes are applied to `main` and shipped in
the next release. There is no long-term support branch.

## Reporting a Vulnerability

**Do not open a public issue for security vulnerabilities.**

Use **GitHub Private Vulnerability Reporting**:

1. Go to [github.com/sovren-software/visage/security/advisories](https://github.com/sovren-software/visage/security/advisories)
2. Click **"Report a vulnerability"**
3. Fill in the details (affected component, reproduction steps, impact assessment)
4. Submit — this creates a private advisory visible only to maintainers

If you cannot use GitHub's reporting feature, email **security@sovren.software**
with the subject line `[Visage Security]` and a description of the issue.

## What to include

- **Affected component** (crate name or file path)
- **Visage version** (output of `visage --version` or git commit hash)
- **Description** of the vulnerability and its security impact
- **Reproduction steps** (minimal, ideally a test case or command sequence)
- **Suggested fix** (optional but appreciated)

## Response timeline

| Stage | Target |
|-------|--------|
| Acknowledgment | Within 48 hours |
| Initial assessment | Within 7 days |
| Fix or mitigation | Within 30 days for critical/high severity |
| Public disclosure | After fix is released, coordinated with reporter |

We follow coordinated disclosure. We will not publicly disclose a vulnerability
before a fix is available unless 90 days have elapsed without resolution.

## Recognition

Security researchers who report valid vulnerabilities will be credited in the
release notes and CHANGELOG (unless they prefer anonymity). Visage does not
currently offer a bug bounty program.

## Security design overview

For details on Visage's security architecture, threat model, and known limitations:

- [Threat Model](docs/threat-model.md)
- [ADR 009 — Model Integrity Verification](docs/decisions/009-onnx-model-integrity-verification.md)
- [STATUS.md — Known Limitations](docs/STATUS.md#known-limitations-at-v03)
