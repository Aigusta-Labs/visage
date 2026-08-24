//! Contract: every hardening directive we *claim* must exist in **both**
//! packaging definitions.
//!
//! This closes a class that has bitten this project twice.
//!
//! **Issue #78.** `docs/threat-model.md` justified `MemoryDenyWriteExecute=false`
//! — its largest documented hardening gap — by stating the daemon "has no
//! network access, no inbound connections". No unit directive enforced that, so
//! the compensating control for that exception did not exist. It was reported by
//! an external contributor, not caught by us.
//!
//! **`TimeoutStopSec`.** Added for issue #26 to cut a ~90s post-hibernate hang
//! to ~10s. It was present in the Debian unit and absent from the NixOS module,
//! so on every NixOS host the fix was simply not applied — the running service
//! reported systemd's 90s default, exactly the hang it was meant to eliminate.
//! This test was written before that was fixed, and failed on it.
//!
//! Neither defect is reachable by a unit test. Both are contracts *between
//! artifacts*: a security document and a unit file; two packaging formats that
//! must agree and have no shared source.
//!
//! # Why the list is hardcoded here rather than parsed from the doc
//!
//! Parsing `threat-model.md` at test time would make the doc the single source
//! and remove all possible drift — but it couples the build to prose, so a
//! reworded table breaks CI. That is the fragility class this repo has spent
//! real time removing. Instead the contract is stated here, in code, citing the
//! doc. Adding a directive to the doc without adding it here is a review moment,
//! not a silent divergence.

use std::path::{Path, PathBuf};

/// Hardening directives that must be present in **both** packaging definitions.
///
/// Source of truth for *why* each is claimed: `docs/threat-model.md`, the
/// "systemd Sandbox" table. Keep the two in step — if you add a row there, add
/// it here.
const REQUIRED_DIRECTIVES: &[&str] = &[
    "CapabilityBoundingSet",
    "DeviceAllow",
    "MemoryDenyWriteExecute",
    "NoNewPrivileges",
    // Enforces the "no network access" claim that threat-model.md relies on to
    // justify MemoryDenyWriteExecute=false. Issue #78.
    "PrivateNetwork",
    "PrivateTmp",
    "ProtectHome",
    "ProtectSystem",
    "ReadWritePaths",
    "SystemCallArchitectures",
    // Bounds a stuck v4l2 capture on stop/restart. Issue #26; without it a
    // post-hibernate restart hangs for systemd's 90s default.
    "TimeoutStopSec",
];

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is <repo>/crates/visaged
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/visaged should be two levels below the repo root")
        .to_path_buf()
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Lines with the leading `#` comment stripped, so a directive that appears only
/// inside a comment does not count as present.
fn uncommented(body: &str) -> Vec<&str> {
    body.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .collect()
}

/// `Directive=value` — systemd unit syntax.
fn unit_declares(body: &str, directive: &str) -> bool {
    let prefix = format!("{directive}=");
    uncommented(body).iter().any(|l| l.starts_with(&prefix))
}

/// `Directive = value;` — Nix attribute syntax.
fn nix_declares(body: &str, directive: &str) -> bool {
    let prefix = format!("{directive} =");
    uncommented(body).iter().any(|l| l.starts_with(&prefix))
}

/// The `systemd.services.visaged` block only.
///
/// Scoped deliberately: the module also defines `systemd.services.visage-resume`,
/// and a directive present only there must not count as protecting the daemon.
fn visaged_service_block(module: &str) -> String {
    let start = module
        .find("systemd.services.visaged = {")
        .expect("module.nix should define systemd.services.visaged");
    let rest = &module[start..];
    // The next service definition ends this block.
    let end = rest[1..]
        .find("systemd.services.")
        .map(|i| i + 1)
        .unwrap_or(rest.len());
    rest[..end].to_string()
}

#[test]
fn every_required_directive_is_declared_in_both_packaging_definitions() {
    let unit = read("packaging/systemd/visaged.service");
    let module = read("packaging/nix/module.nix");
    let block = visaged_service_block(&module);

    let mut missing = Vec::new();
    for directive in REQUIRED_DIRECTIVES {
        if !unit_declares(&unit, directive) {
            missing.push(format!("packaging/systemd/visaged.service: {directive}"));
        }
        if !nix_declares(&block, directive) {
            missing.push(format!(
                "packaging/nix/module.nix (systemd.services.visaged): {directive}"
            ));
        }
    }

    assert!(
        missing.is_empty(),
        "hardening directives claimed in docs/threat-model.md are missing from a packaging \
         definition. A directive present in one format and absent from the other means the \
         protection silently does not apply on that platform — this is issue #78's class.\n  {}",
        missing.join("\n  ")
    );
}

/// A control for the test above.
///
/// If the extraction or matching breaks, `every_required_directive_...` would
/// pass vacuously on an empty search space. This proves the instrument can see
/// a directive at all, and that scoping to the visaged block actually excludes
/// the sibling service.
#[test]
fn the_matchers_can_see_a_directive_and_the_scoping_excludes_the_sibling_service() {
    let unit = read("packaging/systemd/visaged.service");
    let module = read("packaging/nix/module.nix");
    let block = visaged_service_block(&module);

    // Positive control: ExecStart is unmissable in both.
    assert!(
        unit_declares(&unit, "ExecStart"),
        "positive control failed — the unit matcher cannot see ExecStart, so an empty \
         result from it means nothing"
    );
    assert!(
        nix_declares(&block, "ExecStart"),
        "positive control failed — the nix matcher cannot see ExecStart"
    );

    // Negative control: a directive that exists in neither file must read absent.
    assert!(
        !unit_declares(&unit, "ThisDirectiveDoesNotExist"),
        "negative control failed — the unit matcher reports a fabricated directive as present"
    );

    // Scoping control: the block must stop before the sibling service.
    assert!(
        !block.contains("systemd.services.visage-resume"),
        "scoping failed — the extracted visaged block ran on into visage-resume, so a \
         directive present only on the sibling would be miscounted as protecting the daemon"
    );
}
