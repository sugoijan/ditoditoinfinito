//! What an upload token may do, as the server said it, for the session
//! (`docs/plans/shared-songs.md`): the share dialog offers
//! Forever only where the server's listing (`GET <base>/s`) named the
//! token an administrator's. The shells ask when the dialog opens
//! ([`ask`], `SelectCall::ShareAccess`) and tell what they heard whenever
//! they read the listing ([`heard`]; Options' Shares dialog reads it
//! too). Nothing known (no answer yet, a server that names no role,
//! offline): the expiries every token may choose, never Forever.
//!
//! Tokens are kept by a hash, never as text; one thread's memory (each
//! shell's UI thread), gone with the session.

use std::cell::RefCell;

use ddi_library::share::{ShareListing, UPLOAD_EXPIRIES};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Known {
    /// Asked, no answer yet.
    Asking,
    /// What the server said the token may choose.
    Expiries(&'static [&'static str]),
}

thread_local! {
    static KNOWN: RefCell<Vec<(u64, Known)>> = const { RefCell::new(Vec::new()) };
}

/// The token's key (FNV-1a 64 of its trimmed text).
fn key(token: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in token.trim().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn get(token: &str) -> Option<Known> {
    let k = key(token);
    KNOWN.with(|m| m.borrow().iter().find(|(t, _)| *t == k).map(|(_, v)| *v))
}

fn set(token: &str, value: Option<Known>) {
    let k = key(token);
    KNOWN.with(|m| {
        let mut m = m.borrow_mut();
        m.retain(|(t, _)| *t != k);
        if let Some(v) = value {
            m.push((k, v));
        }
    });
}

/// Whether the shell should ask the server what `token` may do now:
/// nothing known of it and no question out. Marks it asked.
pub fn ask(token: &str) -> bool {
    if token.trim().is_empty() || get(token).is_some() {
        return false;
    }
    set(token, Some(Known::Asking));
    true
}

/// The server's listing for `token` read: what it may choose, kept for
/// the session. A listing that names no role the game knows leaves it
/// not known (and not asked again this session).
pub fn heard(token: &str, listing: &ShareListing) {
    if token.trim().is_empty() {
        return;
    }
    set(
        token,
        Some(Known::Expiries(listing.expiries().unwrap_or(&UPLOAD_EXPIRIES))),
    );
}

/// The question failed (offline, the token refused, the server's answer
/// unread): nothing known, asked again the next time the dialog opens.
pub fn failed(token: &str) {
    if get(token) == Some(Known::Asking) {
        set(token, None);
    }
}

/// The expiry words the share dialog offers for `token`: what the server
/// said it may choose, else those every token may.
pub fn expiries(token: &str) -> &'static [&'static str] {
    match get(token) {
        Some(Known::Expiries(e)) => e,
        _ => &UPLOAD_EXPIRIES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(role: &str) -> ShareListing {
        ShareListing {
            role: role.into(),
            shares: Vec::new(),
        }
    }

    #[test]
    fn forever_is_offered_only_once_the_server_named_the_token_an_admin_s() {
        // Not known: every token's, asked once while the question is out.
        assert_eq!(expiries("tok"), UPLOAD_EXPIRIES);
        assert!(!ask(""));
        assert!(ask("tok"));
        assert!(!ask("tok"));
        assert!(!ask(" tok "));
        assert_eq!(expiries("tok"), UPLOAD_EXPIRIES);
        // Failed: asked again.
        failed("tok");
        assert!(ask("tok"));
        heard("tok", &listing("admin"));
        assert!(expiries("tok").contains(&"never"));
        assert!(!ask("tok"));
        // A late failure does not forget the answer.
        failed("tok");
        assert!(expiries("tok").contains(&"never"));
        // Another token is its own.
        assert!(!expiries("other").contains(&"never"));
        heard("other", &listing("upload"));
        assert_eq!(expiries("other"), UPLOAD_EXPIRIES);
        assert!(!ask("other"));
        // A server that names no role: not known, not asked again.
        assert!(ask("old"));
        heard("old", &listing(""));
        assert_eq!(expiries("old"), UPLOAD_EXPIRIES);
        assert!(!ask("old"));
    }
}
