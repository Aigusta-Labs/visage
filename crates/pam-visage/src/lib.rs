//! pam_visage — PAM module for Visage biometric authentication.
//!
//! This is a thin client that calls visaged over D-Bus.
//! The PAM module never owns the camera or runs inference directly.
//!
//! # PAM integration
//!
//! Install to /usr/lib/security/pam_visage.so
//! Add to PAM config: `auth sufficient pam_visage.so`
//!
//! STEP 3 design: pam_sm_authenticate(pamh, flags, argc, argv) + pam_sm_setcred(...)
//! D-Bus client calls org.freedesktop.Visage1.Verify() with timeout and PAM conversation feedback.
//! Returns PAM_IGNORE (25) on module unavailability for safe fallback.
//! See docs/architecture.md § PAM Integration.

/// Returns PAM_IGNORE (25) — tells PAM to skip this module and continue.
/// Full implementation deferred to Step 3 (daemon integration phase).
#[no_mangle]
pub extern "C" fn pam_sm_authenticate() -> i32 {
    25
}

/// Stub for credential management (Step 3).
#[no_mangle]
pub extern "C" fn pam_sm_setcred() -> i32 {
    25
}
