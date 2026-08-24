//! Contract: the paths the packaging declares are real, and agree with each other.
//!
//! `cargo deb` reads an asset list out of `crates/visaged/Cargo.toml`. Nothing
//! checks that the artifacts it names are ones the workspace actually builds, or
//! that the files it copies exist, or that the systemd unit's `ExecStart` points
//! where the package installs the binary. Rename a `[[bin]]` target and the build
//! stays green; the `.deb` fails at package time, or worse, ships a unit whose
//! `ExecStart` points at nothing.
//!
//! The install locations are also duplicated across three packaging formats with
//! no shared definition, which is the same drift shape as the PAM control string
//! (see `pam_control_contract.rs`) and the systemd directives (see
//! `systemd_hardening_contract.rs`).
//!
//! Parsed as text rather than via a TOML crate deliberately: `visaged` has no
//! dev-dependencies, and these tests are meant to add none.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
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

/// `["source", "dest", "mode"]` rows from `[package.metadata.deb] assets`.
fn deb_assets() -> Vec<(String, String)> {
    let manifest = read("crates/visaged/Cargo.toml");
    let start = manifest
        .find("assets = [")
        .expect("crates/visaged/Cargo.toml should declare [package.metadata.deb] assets");
    let rest = &manifest[start..];
    let end = rest.find("\n]").expect("assets list should be terminated");

    rest[..end]
        .lines()
        .filter_map(|l| {
            let t = l.trim();
            if !t.starts_with('[') {
                return None;
            }
            let fields: Vec<_> = t
                .trim_start_matches('[')
                .split(',')
                .map(|f| f.trim().trim_matches(|c| c == '"' || c == ']' || c == ' '))
                .collect();
            match fields.as_slice() {
                [src, dest, ..] => Some((src.to_string(), dest.to_string())),
                _ => None,
            }
        })
        .collect()
}

/// Filenames the workspace actually produces in `target/release/`.
fn built_artifact_names() -> Vec<String> {
    let crates = [
        "crates/visaged/Cargo.toml",
        "crates/visage-cli/Cargo.toml",
        "crates/pam-visage/Cargo.toml",
        "crates/visage-core/Cargo.toml",
        "crates/visage-hw/Cargo.toml",
        "crates/visage-models/Cargo.toml",
    ];
    let mut names = Vec::new();
    for c in crates {
        let body = read(c);
        let mut section = String::new();
        let mut is_cdylib = false;
        // A second pass is not needed: crate-type appears inside [lib].
        for line in body.lines() {
            let t = line.trim();
            if t.starts_with('[') && t.ends_with(']') {
                section = t.to_string();
                continue;
            }
            if section == "[lib]" && t.contains("crate-type") && t.contains("cdylib") {
                is_cdylib = true;
            }
        }
        section.clear();
        for line in body.lines() {
            let t = line.trim();
            if t.starts_with('[') && t.ends_with(']') {
                section = t.to_string();
                continue;
            }
            let Some(rest) = t.strip_prefix("name = ") else {
                continue;
            };
            let name = rest.trim().trim_matches('"');
            match section.as_str() {
                "[[bin]]" => names.push(name.to_string()),
                "[lib]" if is_cdylib => names.push(format!("lib{name}.so")),
                _ => {}
            }
        }
    }
    names
}

#[test]
fn every_built_artifact_the_deb_ships_is_one_the_workspace_produces() {
    let assets = deb_assets();
    assert!(
        !assets.is_empty(),
        "parsed no deb assets — the extractor is broken, so a pass here would mean nothing"
    );

    let built = built_artifact_names();
    assert!(
        built.contains(&"visaged".to_string()),
        "positive control failed — did not find the `visaged` binary target, so the artifact \
         list is not trustworthy. found: {built:?}"
    );

    let mut missing = Vec::new();
    for (src, _) in &assets {
        let Some(file) = src.strip_prefix("target/release/") else {
            continue;
        };
        if !built.contains(&file.to_string()) {
            missing.push(format!(
                "{src}: the deb ships this, but no crate declares a target producing `{file}`. \
                 Renaming a [[bin]] leaves the build green and breaks packaging."
            ));
        }
    }
    assert!(
        missing.is_empty(),
        "deb asset / build target mismatch:\n  {}",
        missing.join("\n  ")
    );
}

#[test]
fn every_source_file_the_deb_ships_exists() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut missing = Vec::new();
    for (src, _) in deb_assets() {
        if src.starts_with("target/") {
            continue; // build output, not in the tree
        }
        // Asset sources are relative to the manifest directory.
        if !manifest_dir.join(&src).exists() {
            missing.push(format!(
                "{src}: declared in the deb asset list but not present"
            ));
        }
    }
    assert!(
        missing.is_empty(),
        "the deb references files that do not exist — `cargo deb` fails at package time, \
         which is after CI has gone green:\n  {}",
        missing.join("\n  ")
    );
}

#[test]
fn the_systemd_unit_execstart_points_where_the_deb_installs_the_daemon() {
    let unit = read("packaging/systemd/visaged.service");
    let exec = unit
        .lines()
        .find_map(|l| l.trim().strip_prefix("ExecStart="))
        .expect("the unit should declare ExecStart")
        .split_whitespace()
        .next()
        .expect("ExecStart should name a binary")
        .to_string();

    let dest = deb_assets()
        .into_iter()
        .find(|(src, _)| src == "target/release/visaged")
        .map(|(_, dest)| dest)
        .expect("the deb should ship the visaged binary");

    // dest is a directory like "usr/bin/"
    let installed = format!("/{}{}", dest.trim_end_matches('/'), "/visaged");
    assert_eq!(
        exec, installed,
        "the packaged unit's ExecStart ({exec}) does not match where the deb installs the \
         daemon ({installed}). The package would install a unit that cannot start."
    );
}

#[test]
fn the_pam_module_install_path_agrees_across_packaging_formats() {
    const SUFFIX: &str = "lib/security/pam_visage.so";

    let deb_dest = deb_assets()
        .into_iter()
        .find(|(src, _)| src.ends_with("libpam_visage.so"))
        .map(|(_, dest)| dest)
        .expect("the deb should ship the PAM module");

    let aur = read("packaging/aur/PKGBUILD");
    let nix = read("packaging/nix/default.nix");

    let mut problems = Vec::new();
    if !deb_dest.ends_with(SUFFIX) {
        problems.push(format!(
            "deb installs to `{deb_dest}`, expected to end with `{SUFFIX}`"
        ));
    }
    if !aur.contains(SUFFIX) {
        problems.push(format!(
            "packaging/aur/PKGBUILD does not install to `{SUFFIX}`"
        ));
    }
    if !nix.contains(SUFFIX) {
        problems.push(format!(
            "packaging/nix/default.nix does not install to `{SUFFIX}`"
        ));
    }

    assert!(
        problems.is_empty(),
        "the PAM module install path differs between packaging formats. A PAM stack that \
         references a path one format does not use fails closed to a password, silently.\n  {}",
        problems.join("\n  ")
    );
}
