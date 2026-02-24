## Type

<!-- Check one: -->
- [ ] Hardware quirk (`contrib/hw/*.toml`)
- [ ] Bug fix
- [ ] Distribution packaging
- [ ] Documentation
- [ ] Other: <!-- describe -->

## Description

<!-- What does this PR do? Link related issues with "Closes #123" or "Fixes #456". -->

## Testing

<!-- How did you verify this works?
     - For quirks: paste `visage discover` output showing the device
     - For bug fixes: describe the repro and how this fixes it
     - For packaging: which distro/version did you test on? -->

## Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] No new warnings introduced
- [ ] I have read [CONTRIBUTING.md](../CONTRIBUTING.md)

## Breaking changes

<!-- Does this change any public API, configuration, CLI behavior, or file format?
     If yes, describe what breaks and how users should migrate. If no, delete this section. -->
