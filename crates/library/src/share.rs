//! Songs shared by link (`docs/plans/shared-songs.md`): what both shells
//! and the private Worker agree on without the web's record types. A
//! share is named by a short id; its songs are `shared:<id>:<n>` to the
//! library and the history.

/// A share id as the links carry it: 1 to 32 characters of `[0-9A-Za-z]`.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// The id of song `n` of share `share`.
pub fn song_id(share: &str, n: usize) -> String {
    format!("shared:{share}:{n}")
}

/// The share and the index a shared song's id names.
pub fn parse_song_id(id: &str) -> Option<(&str, usize)> {
    let rest = id.strip_prefix("shared:")?;
    let (share, n) = rest.rsplit_once(':')?;
    valid_id(share).then_some(())?;
    Some((share, n.parse().ok()?))
}

pub fn is_shared_id(id: &str) -> bool {
    parse_song_id(id).is_some()
}

/// How long a share has left at `now` (Unix seconds), in a few words;
/// `None` when it never expires.
pub fn expires_in(expires_at: Option<f64>, now: f64) -> Option<String> {
    let left = expires_at? - now;
    Some(if left <= 0.0 {
        "expired".into()
    } else if left < 3600.0 {
        format!("expires in {} min", (left / 60.0).ceil() as i64)
    } else if left < 86_400.0 {
        format!("expires in {} h", (left / 3600.0).ceil() as i64)
    } else {
        format!("expires in {} days", (left / 86_400.0).ceil() as i64)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_short_and_plain() {
        assert!(valid_id("abc12345") && valid_id("A") && valid_id(&"z".repeat(32)));
        assert!(!valid_id("") && !valid_id("ab-c") && !valid_id(&"z".repeat(33)));
        assert_eq!(song_id("abc12345", 3), "shared:abc12345:3");
        assert_eq!(parse_song_id("shared:abc12345:3"), Some(("abc12345", 3)));
        assert_eq!(parse_song_id("shared:abc12345:x"), None);
        assert_eq!(parse_song_id("shared:ab c:1"), None);
        assert_eq!(parse_song_id("u-abc"), None);
        assert!(is_shared_id("shared:q:0") && !is_shared_id("beatucada"));
    }

    #[test]
    fn the_time_left_reads_plainly() {
        let at = Some(1_900_000_000.0);
        assert_eq!(
            expires_in(at, 1_900_000_000.0 - 90.0).as_deref(),
            Some("expires in 2 min")
        );
        assert_eq!(
            expires_in(at, 1_900_000_000.0 - 7200.0).as_deref(),
            Some("expires in 2 h")
        );
        assert_eq!(
            expires_in(at, 1_900_000_000.0 - 3.0 * 86_400.0).as_deref(),
            Some("expires in 3 days")
        );
        assert_eq!(expires_in(at, 1_900_000_001.0).as_deref(), Some("expired"));
        assert_eq!(expires_in(None, 0.0), None);
    }
}
