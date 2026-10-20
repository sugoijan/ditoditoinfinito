//! macOS: Seatbelt through `sandbox_init` (the API Chromium's helpers use;
//! Apple marks it deprecated but ships no replacement for a process
//! confining itself). The profile denies everything, then allows what a
//! decoder needs: signalling itself (abort), reading sysctls (CPU
//! features and counts); exiting, memory and threads Seatbelt does not
//! govern. Files, network, Mach services, other processes: denied.
//! The arena is mapped and the pipes open before this runs.

use std::ffi::{CStr, c_char, c_int};

unsafe extern "C" {
    fn sandbox_init(profile: *const c_char, flags: u64, errorbuf: *mut *mut c_char) -> c_int;
    fn sandbox_free_error(errorbuf: *mut c_char);
}

const PROFILE: &CStr = c"(version 1)
(deny default)
(allow signal (target self))
(allow sysctl-read)
";

pub fn enter() -> Result<&'static str, String> {
    let mut error: *mut c_char = std::ptr::null_mut();
    // SAFETY: a NUL-terminated profile; the error buffer is freed by the
    // library's own routine.
    let r = unsafe { sandbox_init(PROFILE.as_ptr(), 0, &mut error) };
    if r != 0 {
        let why = if error.is_null() {
            "unknown reason".to_string()
        } else {
            // SAFETY: a NUL-terminated string from sandbox_init.
            let s = unsafe { CStr::from_ptr(error) }
                .to_string_lossy()
                .into_owned();
            unsafe { sandbox_free_error(error) };
            s
        };
        return Err(format!("sandbox_init failed: {why}"));
    }
    Ok("seatbelt")
}
