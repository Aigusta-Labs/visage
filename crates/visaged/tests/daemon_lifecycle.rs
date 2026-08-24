//! End-to-end lifecycle: launch the real `visaged` and drive it over D-Bus.
//!
//! **Ignored by default. This needs a camera.**
//!
//! ```text
//! cargo test -p visaged --test daemon_lifecycle -- --ignored --nocapture
//! ```
//!
//! `spawn_engine` opens the camera and loads the ONNX models synchronously
//! before the daemon serves D-Bus (`crates/visaged/src/engine.rs`), so there is
//! no headless path — the daemon cannot start without hardware. Rather than
//! depend on a virtual V4L2 device in CI, which would be another
//! externally-maintained thing that can silently break the build, this runs on a
//! real machine with a real IR camera. That is also higher fidelity.
//!
//! Everything it needs is steerable by environment (`crates/visaged/src/config.rs`):
//!
//! | var | default | meaning |
//! |---|---|---|
//! | `VISAGE_TEST_CAMERA` | `/dev/video2` | capture device |
//! | `VISAGE_TEST_MODEL_DIR` | `/var/lib/visage/models` | ONNX models |
//!
//! The daemon runs on a **private session bus** this test spawns, with a
//! temporary database, so it never touches the system bus, the real store, or a
//! running production `visaged`.
//!
//! # What this covers that no unit test can
//!
//! Startup ordering, the model-integrity gate, D-Bus name registration, method
//! dispatch across the wire, store open, and — the one that matters most —
//! that a client call **fails promptly rather than hanging** when the daemon is
//! gone. `pam-visage`'s entire contract is that it never blocks a login; a hang
//! there is an unbootable machine, and nothing in the unit tests exercises it.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

const BUS_NAME: &str = "org.freedesktop.Visage1";
const OBJECT_PATH: &str = "/org/freedesktop/Visage1";
const INTERFACE: &str = "org.freedesktop.Visage1";

fn camera() -> String {
    std::env::var("VISAGE_TEST_CAMERA").unwrap_or_else(|_| "/dev/video2".to_string())
}

fn model_dir() -> String {
    std::env::var("VISAGE_TEST_MODEL_DIR").unwrap_or_else(|_| "/var/lib/visage/models".to_string())
}

/// A private session bus plus a `visaged` attached to it.
///
/// Both children are killed on drop, including on panic, so a failing assertion
/// cannot leave a stray daemon holding a camera.
struct Fixture {
    bus: Child,
    daemon: Child,
    address: String,
    db_dir: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
        let _ = self.bus.kill();
        let _ = self.bus.wait();
        let _ = std::fs::remove_dir_all(&self.db_dir);
    }
}

impl Fixture {
    fn start() -> Self {
        let mut bus = Command::new("dbus-daemon")
            .args(["--session", "--print-address", "--nofork"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect(
                "cannot spawn `dbus-daemon` — this test needs it to create a private session \
                 bus. It is deliberately not skipped: you ran it with --ignored on purpose.",
            );

        let address = {
            let stdout = bus.stdout.take().expect("dbus-daemon stdout");
            let mut line = String::new();
            BufReader::new(stdout)
                .read_line(&mut line)
                .expect("dbus-daemon should print its address");
            line.trim().to_string()
        };
        assert!(
            !address.is_empty(),
            "dbus-daemon printed an empty bus address"
        );

        let db_dir = std::env::temp_dir().join(format!("visage-it-{}", std::process::id()));
        std::fs::create_dir_all(&db_dir).expect("create temp db dir");

        let daemon = Command::new(env!("CARGO_BIN_EXE_visaged"))
            .env("DBUS_SESSION_BUS_ADDRESS", &address)
            .env("VISAGE_SESSION_BUS", "1")
            .env("VISAGE_DB_PATH", db_dir.join("faces.db"))
            .env("VISAGE_CAMERA_DEVICE", camera())
            .env("VISAGE_MODEL_DIR", model_dir())
            .env("RUST_LOG", "visaged=info")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn visaged");

        Fixture {
            bus,
            daemon,
            address,
            db_dir,
        }
    }

    /// Connect once the daemon has claimed the bus name, or fail with why.
    async fn connect(&self) -> zbus::Connection {
        let conn = zbus::connection::Builder::address(self.address.as_str())
            .expect("valid bus address")
            .build()
            .await
            .expect("connect to the private session bus");

        let dbus = zbus::fdo::DBusProxy::new(&conn).await.expect("DBusProxy");
        for _ in 0..100 {
            let names = dbus.list_names().await.expect("list bus names");
            if names.iter().any(|n| n.as_str() == BUS_NAME) {
                return conn;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!(
            "`{BUS_NAME}` never appeared on the bus within 10s. The daemon opens the camera \
             ({}) and verifies models in {} before serving D-Bus, so a failure here is most \
             likely one of those — check the stderr above.",
            camera(),
            model_dir()
        );
    }

    async fn call(
        &self,
        conn: &zbus::Connection,
        method: &str,
        body: &(impl serde::Serialize + zbus::zvariant::DynamicType),
    ) -> zbus::Result<zbus::Message> {
        conn.call_method(Some(BUS_NAME), OBJECT_PATH, Some(INTERFACE), method, body)
            .await
    }
}

#[tokio::test]
#[ignore = "needs a camera; run with --ignored on a machine that has one"]
async fn the_daemon_claims_its_bus_name_and_answers_status() {
    let fx = Fixture::start();
    let conn = fx.connect().await;

    let reply = fx
        .call(&conn, "Status", &())
        .await
        .expect("Status should succeed");
    let status: String = reply.body().deserialize().expect("Status returns a string");

    assert!(
        !status.trim().is_empty(),
        "Status returned an empty string; the daemon is on the bus but not reporting state"
    );
    eprintln!("visaged status: {status}");
}

#[tokio::test]
#[ignore = "needs a camera; run with --ignored on a machine that has one"]
async fn verify_for_an_unenrolled_user_returns_false_rather_than_erroring() {
    let fx = Fixture::start();
    let conn = fx.connect().await;

    // A user that cannot have an enrolment.
    let unknown = format!("visage-it-nobody-{}", std::process::id());
    let reply = fx.call(&conn, "Verify", &(unknown.as_str(),)).await.expect(
        "Verify on an unenrolled user must return a value, not a D-Bus error. pam-visage \
             treats an error the same as a non-match, so this distinction is invisible at the \
             PAM layer and must be asserted here.",
    );
    let matched: bool = reply.body().deserialize().expect("Verify returns a bool");

    assert!(!matched, "an unenrolled user must not match");
}

#[tokio::test]
#[ignore = "needs a camera; run with --ignored on a machine that has one"]
async fn a_client_call_fails_promptly_once_the_daemon_is_gone() {
    let mut fx = Fixture::start();
    let conn = fx.connect().await;

    // Prove the connection works before we take the daemon away, so a failure
    // below cannot be blamed on a bad connection.
    fx.call(&conn, "Status", &())
        .await
        .expect("control: Status should succeed while the daemon is alive");

    fx.daemon.kill().expect("kill visaged");
    fx.daemon.wait().expect("reap visaged");

    // This is pam-visage's core safety property. Its contract is PAM_IGNORE —
    // fall through to a password — and a hang here is a machine nobody can log
    // into. The bound is generous; what matters is that it terminates at all.
    let call = fx.call(&conn, "Status", &());
    let result = tokio::time::timeout(Duration::from_secs(10), call).await;

    match result {
        Err(_) => panic!(
            "a D-Bus call hung for 10s after the daemon exited. pam_visage would block the \
             authentication stack here rather than falling through to a password."
        ),
        Ok(Ok(_)) => panic!("the call succeeded after the daemon was killed — impossible"),
        Ok(Err(e)) => eprintln!("failed cleanly, as required: {e}"),
    }
}
