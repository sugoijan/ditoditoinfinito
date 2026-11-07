//! What went wrong outside the game (the system, the browser, the
//! network), as the screens say it (`docs/plans/localization.md`, step 4).
//!
//! Errors from outside reach the screens as the system wrote them ("No
//! such file or directory (os error 2)", a `QuotaExceededError`): their
//! words are the system's, in its language or in English. [`error_text`]
//! reads what kind of failure it was from the names and codes systems
//! give (a DOMException's name, an OS error's code, an HTTP status) and
//! says that in the player's language, the system's words after it as
//! data; a failure of no kind it knows is the system's words alone. Our
//! own reports (what an import found, why a song did not load) are never
//! passed through here: they are values the screens word themselves.

use ddi_l10n::UiText;
use ddi_l10n::msg::errors as m;

/// A kind of failure outside the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// A file, a folder, a record or a page is not there.
    NotFound,
    /// The system or the browser refused access.
    Denied,
    /// Out of storage (a disk, the browser's quota).
    Full,
    /// The network failed or the server could not be reached.
    Network,
    /// It took too long.
    Timeout,
    /// The data is damaged or not what it should be.
    Unreadable,
    /// This browser or system cannot do it.
    Unsupported,
}

/// The kind of failure `raw` (an error as the system wrote it) says,
/// from the names and codes it carries; `None` when it says none known.
pub fn kind_of(raw: &str) -> Option<FailureKind> {
    use FailureKind::*;
    let has = |needles: &[&str]| needles.iter().any(|n| raw.contains(n));
    // Most specific first: a quota error is also a DOM error, a timeout
    // also a network one.
    if has(&[
        "QuotaExceeded",
        "os error 28)",
        "os error 112)",
        "No space left",
        "disk is full",
    ]) {
        Some(Full)
    } else if has(&["TimeoutError", "timed out", "os error 60)", "os error 110)"]) {
        Some(Timeout)
    } else if has(&[
        "NotAllowedError",
        "SecurityError",
        "os error 13)",
        "os error 1)",
        "os error 5)",
        "Permission denied",
        "Access is denied",
        "HTTP 401",
        "HTTP 403",
        "status 401",
        "status 403",
    ]) {
        Some(Denied)
    } else if has(&[
        "NotFoundError",
        "os error 2)",
        "os error 3)",
        "No such file",
        "cannot find the",
        "HTTP 404",
        "HTTP 410",
        "status 404",
        "status 410",
    ]) {
        Some(NotFound)
    } else if has(&[
        "NetworkError",
        "Failed to fetch",
        "Connection refused",
        "connection refused",
        "os error 61)",
        "os error 111)",
        "dns error",
        "Network is unreachable",
    ]) {
        Some(Network)
    } else if has(&["NotSupportedError", "not supported"]) {
        Some(Unsupported)
    } else if has(&[
        "NotReadableError",
        "DataError",
        "EncodingError",
        "invalid data",
        "InvalidData",
        "stream did not contain valid UTF-8",
        "corrupt",
    ]) {
        Some(Unreadable)
    } else {
        None
    }
}

/// `raw`, an error as the system wrote it, as the screens show it: its
/// kind in the player's language and the system's words after it, else
/// the system's words alone (data).
pub fn error_text(raw: &str) -> UiText {
    let raw = raw.trim();
    match kind_of(raw) {
        Some(kind) => failure(kind, raw),
        None => UiText::data(raw),
    }
}

/// A failure of `kind`, with what the system said (`detail`, data).
pub fn failure(kind: FailureKind, detail: &str) -> UiText {
    match kind {
        FailureKind::NotFound => m::not_found(detail),
        FailureKind::Denied => m::denied(detail),
        FailureKind::Full => m::full(detail),
        FailureKind::Network => m::network(detail),
        FailureKind::Timeout => m::timeout(detail),
        FailureKind::Unreadable => m::unreadable(detail),
        FailureKind::Unsupported => m::unsupported(detail),
    }
}

/// An I/O error (desktop's files, the share client's connection) as the
/// screens show it: its kind read from the error's own kind first.
pub fn io_error_text(e: &std::io::Error) -> UiText {
    use std::io::ErrorKind as K;
    let detail = e.to_string();
    let kind = match e.kind() {
        K::NotFound => Some(FailureKind::NotFound),
        K::PermissionDenied => Some(FailureKind::Denied),
        K::StorageFull | K::QuotaExceeded => Some(FailureKind::Full),
        K::TimedOut => Some(FailureKind::Timeout),
        K::ConnectionRefused
        | K::ConnectionReset
        | K::ConnectionAborted
        | K::NotConnected
        | K::HostUnreachable
        | K::NetworkUnreachable
        | K::NetworkDown => Some(FailureKind::Network),
        K::InvalidData | K::UnexpectedEof => Some(FailureKind::Unreadable),
        K::Unsupported => Some(FailureKind::Unsupported),
        _ => kind_of(&detail),
    };
    match kind {
        Some(kind) => failure(kind, &detail),
        None => UiText::data(detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_are_read_from_their_names_and_codes() {
        use FailureKind::*;
        for (raw, kind) in [
            (
                "writing the song library: JsValue(QuotaExceededError: …)",
                Some(Full),
            ),
            ("No such file or directory (os error 2)", Some(NotFound)),
            ("Permission denied (os error 13)", Some(Denied)),
            (
                "JsValue(NotAllowedError: The request is not allowed)",
                Some(Denied),
            ),
            ("TypeError: Failed to fetch", Some(Network)),
            ("connection timed out", Some(Timeout)),
            ("HTTP 404 Not Found", Some(NotFound)),
            ("JsValue(NotReadableError: …)", Some(Unreadable)),
            ("something else", None),
        ] {
            assert_eq!(kind_of(raw), kind, "{raw}");
        }
    }

    #[test]
    fn a_failure_says_its_kind_and_the_system_s_words() {
        ddi_l10n::set_language("en");
        assert_eq!(
            &*error_text("No such file or directory (os error 2)"),
            "not found (No such file or directory (os error 2))"
        );
        assert_eq!(&*error_text("odd"), "odd");
        let e = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope");
        assert_eq!(&*io_error_text(&e), "access was refused (nope)");
        ddi_l10n::set_language("ja");
        assert!(error_text("QuotaExceededError").starts_with("保存容量"));
        ddi_l10n::set_language("en");
    }
}
