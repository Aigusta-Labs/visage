//! Contract: the D-Bus interface the daemon serves is the one its clients call.
//!
//! `org.freedesktop.Visage1` is declared in **three places that compile
//! independently of each other**:
//!
//! | role | file |
//! |---|---|
//! | server | `crates/visaged/src/dbus_interface.rs` — `#[interface(...)]` |
//! | CLI client | `crates/visage-cli/src/main.rs` — `#[zbus::proxy(...)]`, 5 methods |
//! | PAM client | `crates/pam-visage/src/lib.rs` — `#[zbus::proxy(...)]`, 1 method |
//!
//! plus the bus name and object path in `crates/visaged/src/main.rs`, and the
//! system-bus policy in `packaging/dbus/org.freedesktop.Visage1.conf`.
//!
//! There is no shared constant. Rename a method on the server and every one of
//! those still compiles; the break appears only when a real call is made — which
//! for `pam-visage` means at an authentication prompt, where its contract is to
//! return `PAM_IGNORE` and fall through to a password. A rename would therefore
//! present as "face auth stopped working" with a green build, which is the same
//! shape as the `success=end` bug.
//!
//! This test compares the declarations as source. It cannot catch a semantic
//! change behind an unchanged signature — but it catches the rename, the removed
//! method, and the drifted interface name, which are the realistic failures.

use std::path::{Path, PathBuf};

const SERVER: &str = "crates/visaged/src/dbus_interface.rs";
const DAEMON_MAIN: &str = "crates/visaged/src/main.rs";
const CLI_CLIENT: &str = "crates/visage-cli/src/main.rs";
const PAM_CLIENT: &str = "crates/pam-visage/src/lib.rs";
const DBUS_POLICY: &str = "packaging/dbus/org.freedesktop.Visage1.conf";

const INTERFACE: &str = "org.freedesktop.Visage1";
const OBJECT_PATH: &str = "/org/freedesktop/Visage1";

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

/// The `(...)` argument list of `async fn <name>`, paren-balanced so multi-line
/// signatures work.
fn arg_list_of(body: &str, name: &str) -> Option<String> {
    let needle = format!("async fn {name}(");
    let start = body.find(&needle)? + needle.len();
    let mut depth = 1usize;
    for (i, ch) in body[start..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(body[start..start + i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Arguments that actually cross the wire.
///
/// `&self` is not an argument, and zbus injects `#[zbus(header)]` /
/// `#[zbus(connection)]` parameters locally — they are not part of the D-Bus
/// signature, so a client legitimately does not declare them.
fn wire_args(arg_list: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for ch in arg_list.chars() {
        match ch {
            '(' | '<' | '[' => {
                depth += 1;
                current.push(ch);
            }
            ')' | '>' | ']' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                args.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    args.push(current);

    args.into_iter()
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .filter(|a| a != "&self" && a != "self")
        .filter(|a| !a.contains("#[zbus("))
        .collect()
}

/// Method names declared inside a `#[zbus::proxy]` trait.
fn proxy_methods(body: &str) -> Vec<String> {
    let start = match body.find("#[zbus::proxy(") {
        Some(i) => i,
        None => return Vec::new(),
    };
    let rest = &body[start..];
    let brace = match rest.find('{') {
        Some(i) => i,
        None => return Vec::new(),
    };
    let end = rest[brace..]
        .find("\n}")
        .map(|i| brace + i)
        .unwrap_or(rest.len());
    rest[brace..end]
        .lines()
        .filter_map(|l| {
            let t = l.trim();
            let t = t.strip_prefix("async fn ")?;
            let name = t.split('(').next()?;
            Some(name.to_string())
        })
        .collect()
}

#[test]
fn the_interface_name_and_object_path_agree_everywhere() {
    let mut missing = Vec::new();

    // The bus name / interface must appear in every site, policy included.
    for file in [SERVER, DAEMON_MAIN, CLI_CLIENT, PAM_CLIENT, DBUS_POLICY] {
        if !read(file).contains(INTERFACE) {
            missing.push(format!("{file}: does not mention `{INTERFACE}`"));
        }
    }
    // The object path is not meaningful in the policy file, which grants by
    // bus name and interface only.
    for file in [SERVER, DAEMON_MAIN, CLI_CLIENT, PAM_CLIENT] {
        let body = read(file);
        if !body.contains(OBJECT_PATH) {
            missing.push(format!("{file}: does not mention `{OBJECT_PATH}`"));
        }
    }

    assert!(
        missing.is_empty(),
        "the D-Bus identity has drifted between the server, its clients and the bus policy. \
         These compile independently, so a mismatch here is invisible until a real call is \
         made.\n  {}",
        missing.join("\n  ")
    );
}

#[test]
fn every_method_a_client_calls_exists_on_the_server_with_the_same_wire_arity() {
    let server = read(SERVER);

    let clients: &[(&str, Vec<String>)] = &[
        (CLI_CLIENT, proxy_methods(&read(CLI_CLIENT))),
        (PAM_CLIENT, proxy_methods(&read(PAM_CLIENT))),
    ];

    // Vacuity guard: if the proxy parser found nothing, every check below would
    // pass over an empty set.
    for (file, methods) in clients {
        assert!(
            !methods.is_empty(),
            "no proxy methods parsed from {file} — the extractor is broken, so a pass here \
             would mean nothing"
        );
    }

    let mut problems = Vec::new();
    for (file, methods) in clients {
        for m in methods {
            match arg_list_of(&server, m) {
                None => problems.push(format!(
                    "{file} calls `{m}`, which the server does not declare. This compiles on \
                     both sides and fails only at runtime."
                )),
                Some(server_args) => {
                    let client_args = arg_list_of(&read(file), m)
                        .map(|a| wire_args(&a))
                        .unwrap_or_default();
                    let server_wire = wire_args(&server_args);
                    if server_wire.len() != client_args.len() {
                        problems.push(format!(
                            "{file} calls `{m}` with {} wire argument(s); the server declares \
                             {}. server: {server_wire:?} client: {client_args:?}",
                            client_args.len(),
                            server_wire.len()
                        ));
                    }
                }
            }
        }
    }

    assert!(
        problems.is_empty(),
        "client/server D-Bus contract mismatch:\n  {}",
        problems.join("\n  ")
    );
}

/// Control for the parsers above.
#[test]
fn the_parsers_can_see_real_declarations() {
    let server = read(SERVER);

    // Positive: the server really declares `verify`, and it takes one wire arg.
    let verify = arg_list_of(&server, "verify").expect(
        "positive control failed — cannot find `async fn verify` in the server, so an empty \
         result from this parser proves nothing",
    );
    let wire = wire_args(&verify);
    assert_eq!(
        wire.len(),
        1,
        "expected `verify` to take exactly one wire argument (the username); zbus-injected \
         header/connection params must be filtered out. got: {wire:?}"
    );

    // Negative: a method that does not exist must not be found.
    assert!(
        arg_list_of(&server, "definitely_not_a_method").is_none(),
        "negative control failed — the parser reports a fabricated method as present"
    );

    // Both clients must yield methods.
    assert!(
        proxy_methods(&read(CLI_CLIENT)).contains(&"verify".to_string()),
        "positive control failed — the proxy parser cannot see `verify` in the CLI client"
    );
    assert!(
        proxy_methods(&read(PAM_CLIENT)).contains(&"verify".to_string()),
        "positive control failed — the proxy parser cannot see `verify` in the PAM client"
    );
}
