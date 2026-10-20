//! Windows: the parent starts this process with a restricted primary token
//! (deny-only groups, a null restricting SID, low integrity), inside a job
//! that allows no child process, and lends its main thread the parent's
//! ordinary token for startup; `enter` gives that token back
//! (`RevertToSelf`), after which every object access is checked against
//! the restricted token. Unverified on real Windows: the self-test decides.

use windows::Win32::Security::RevertToSelf;

pub fn enter() -> Result<&'static str, String> {
    // SAFETY: plain call; failing means the thread had no impersonation
    // token, which is only wrong if the parent did not restrict us (the
    // self-test tells).
    unsafe { RevertToSelf() }.map_err(|e| format!("RevertToSelf: {e}"))?;
    Ok("windows-restricted")
}
