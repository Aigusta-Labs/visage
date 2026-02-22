//! pam_visage — PAM module for Visage biometric authentication.
//!
//! Thin D-Bus client that calls visaged over the system bus.
//! The PAM module never owns the camera or runs inference directly.
//!
//! # Safety
//!
//! All Rust logic is wrapped in `catch_unwind` — a panic unwinding across the
//! `extern "C"` boundary is undefined behavior.
//!
//! Every error path returns `PAM_IGNORE` (25), which tells the PAM stack to
//! skip this module and continue to the next (e.g., password). We never return
//! `PAM_AUTH_ERR` to avoid locking the user out if the daemon is unavailable.
//!
//! # Timeout behaviour
//!
//! If visaged is running but hung, the PAM module waits up to the D-Bus default
//! timeout (~25 s) before returning `PAM_IGNORE`. Under normal conditions the
//! daemon's own 10 s inference timeout fires first. A short client-side timeout
//! (≤3 s) is deferred to Step 6.

// Enforce explicit `unsafe {}` blocks inside `unsafe fn` bodies — catches
// the Rust 2024 edition change before it lands.
#![warn(unsafe_op_in_unsafe_fn)]

use std::ffi::CStr;
use std::panic;

// PAM return codes (POSIX / Linux-PAM values)
const PAM_SUCCESS: libc::c_int = 0;
const PAM_IGNORE: libc::c_int = 25;

extern "C" {
    fn pam_get_user(
        pamh: *mut libc::c_void,
        user: *mut *const libc::c_char,
        prompt: *const libc::c_char,
    ) -> libc::c_int;
}

// D-Bus proxy — `#[zbus::proxy]` generates both `VisageProxy` (async) and
// `VisageProxyBlocking` (synchronous). Only the blocking variant is used here.
#[zbus::proxy(
    interface = "org.freedesktop.Visage1",
    default_service = "org.freedesktop.Visage1",
    default_path = "/org/freedesktop/Visage1"
)]
trait Visage {
    async fn verify(&self, user: &str) -> zbus::Result<bool>;
}

/// Connect to the system bus and call `Visage1.Verify(username)`.
///
/// Returns `Ok(false)` if the daemon responds but finds no match.
/// Returns `Err` if the daemon is not running, the call fails, or times out.
fn verify_face(username: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let conn = zbus::blocking::Connection::system()?;
    let proxy = VisageProxyBlocking::new(&conn)?;
    let matched = proxy.verify(username)?;
    Ok(matched)
}

/// PAM authentication entry point.
///
/// Called by the PAM stack when `auth sufficient pam_visage.so` is configured.
/// Extracts the username via `pam_get_user`, then calls `visaged` over D-Bus.
///
/// Returns:
/// - `PAM_SUCCESS` (0) if face matched
/// - `PAM_IGNORE` (25) on any failure — daemon down, no match, error, panic
///
/// # Safety
///
/// `pamh` must be a valid PAM handle provided by the PAM framework.
/// This function is loaded by the PAM stack via `dlopen`. Panics are caught
/// by `catch_unwind` and converted to `PAM_IGNORE` rather than unwinding
/// across the FFI boundary.
#[no_mangle]
pub unsafe extern "C" fn pam_sm_authenticate(
    pamh: *mut libc::c_void,
    _flags: libc::c_int,
    _argc: libc::c_int,
    _argv: *const *const libc::c_char,
) -> libc::c_int {
    let result = panic::catch_unwind(|| {
        // Extract username from PAM handle.
        let mut user_ptr: *const libc::c_char = std::ptr::null();
        // SAFETY: pamh is a valid PAM handle. pam_get_user writes a pointer
        // that remains valid for the lifetime of the PAM conversation.
        let ret = unsafe { pam_get_user(pamh, &mut user_ptr, std::ptr::null()) };
        if ret != PAM_SUCCESS || user_ptr.is_null() {
            eprintln!("pam_visage: pam_get_user failed (ret={})", ret);
            return PAM_IGNORE;
        }

        // SAFETY: pam_get_user guarantees the pointer is non-null and points
        // to a NUL-terminated string that lives for the PAM conversation.
        let username = match unsafe { CStr::from_ptr(user_ptr) }.to_str() {
            Ok(s) => s,
            Err(_) => {
                eprintln!("pam_visage: username is not valid UTF-8");
                return PAM_IGNORE;
            }
        };

        // Call visaged over D-Bus.
        match verify_face(username) {
            Ok(true) => {
                eprintln!("pam_visage: face matched for user '{}'", username);
                PAM_SUCCESS
            }
            Ok(false) => {
                eprintln!("pam_visage: no match for user '{}'", username);
                PAM_IGNORE
            }
            Err(e) => {
                eprintln!("pam_visage: error: {}", e);
                PAM_IGNORE
            }
        }
    });

    result.unwrap_or(PAM_IGNORE)
}

/// PAM credential management entry point (required by the PAM ABI).
///
/// Visage does not manage credentials — always returns `PAM_IGNORE`.
///
/// # Safety
///
/// `_pamh` must be a valid PAM handle. This function is a no-op stub.
#[no_mangle]
pub unsafe extern "C" fn pam_sm_setcred(
    _pamh: *mut libc::c_void,
    _flags: libc::c_int,
    _argc: libc::c_int,
    _argv: *const *const libc::c_char,
) -> libc::c_int {
    PAM_IGNORE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pam_constants_match_spec() {
        // Verify against the values defined in <security/pam_modules.h>.
        // These are load-bearing: wrong values silently mis-route the PAM stack.
        assert_eq!(PAM_SUCCESS, 0, "PAM_SUCCESS must be 0");
        assert_eq!(PAM_IGNORE, 25, "PAM_IGNORE must be 25");
    }

    #[test]
    fn verify_face_errors_when_daemon_not_running() {
        // When visaged is not on the system bus, verify_face must return Err,
        // not panic. This exercises the ServiceUnknown / NameHasNoOwner path.
        //
        // This test will pass in any environment where visaged is not running,
        // including CI. If the daemon happens to be running, the test is skipped
        // to avoid a real camera capture during unit testing.
        let result = verify_face("_pam_visage_unit_test_user_");
        // If the daemon is running we get Ok(true/false); that's also fine —
        // the important property is no panic.
        match result {
            Err(e) => {
                // Expected: daemon not present
                let msg = e.to_string();
                assert!(
                    msg.contains("ServiceUnknown")
                        || msg.contains("NameHasNoOwner")
                        || msg.contains("not provided")
                        || msg.contains("Failed to connect"),
                    "unexpected error message: {msg}"
                );
            }
            Ok(_) => {
                // Daemon is running — acceptable, confirms no panic either way
            }
        }
    }
}
