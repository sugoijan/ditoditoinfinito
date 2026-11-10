//! Songs shared by link (`docs/plans/shared-songs.md`): what the game and
//! its private server (`../ddi-server`) agree on, in one place. The ids,
//! the limits, the expiry words, the JSON of a share being made, read and
//! listed, the paths a share's files are at, the routes under the API's
//! base, and the claims of an upload grant a room gives its host
//! (`docs/plans/multiplayer-prototype-progress.md`, M5d). Serde only: the
//! server depends on this crate by path and must not pull the game's
//! library; `ddi_library::share` re-exports it for the game.
//!
//! The JSON here is a wire contract: the server's answers are written with
//! [`Share`], [`ShareSummary`] and [`ShareListing`], the game reads them with
//! the more forgiving [`ShareDoc`] (generic over the record type each shell
//! stores songs as); a test keeps the two in step.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The server's address where a build names no other: the Worker's
/// route beside the app's page.
pub const DEFAULT_BASE: &str = "https://sugoijan.dev/ditoditoinfinito/api";

/// A share id as links carry it: 1 to 32 characters of `[0-9A-Za-z]`
/// (the server makes them [`ID_LEN`] long; a link is read leniently).
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// The length of the ids the server makes.
pub const ID_LEN: usize = 8;
pub const ID_ALPHABET: &[u8; 62] =
    b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// An id as the server makes them, and the only ones its routes take:
/// [`ID_LEN`] characters of `[0-9A-Za-z]`.
pub fn valid_minted_id(id: &str) -> bool {
    id.len() == ID_LEN && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// An id from random bytes (one a character; a slight bias toward the
/// first few characters of the alphabet, which no one guesses by).
pub fn id_from_bytes(bytes: &[u8; ID_LEN]) -> String {
    bytes
        .iter()
        .map(|b| ID_ALPHABET[(*b as usize) % ID_ALPHABET.len()] as char)
        .collect()
}

/// The id of song `n` of share `share`, as the library and the history
/// name it.
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

/// The limits a share is refused over (`413`).
pub const MAX_SHARE_BYTES: f64 = 2.0 * 1024.0 * 1024.0 * 1024.0;
pub const MAX_FILE_BYTES: f64 = 512.0 * 1024.0 * 1024.0;
pub const MAX_SONGS: usize = 200;
/// The pack's art at most.
pub const MAX_ART_BYTES: u64 = 16 * 1024 * 1024;
/// A share never finished is deleted after this long (seconds).
pub const UNFINISHED_LIFE: i64 = 86_400;

/// The words for how long a share lasts.
pub const EXPIRIES: [&str; 5] = ["1h", "1d", "1w", "1m", "never"];

/// How long an expiry word gives, in seconds; `Some(None)`: never;
/// `None`: not one of [`EXPIRIES`].
pub fn expiry_seconds(word: &str) -> Option<Option<i64>> {
    Some(match word {
        "1h" => Some(3600),
        "1d" => Some(86_400),
        "1w" => Some(7 * 86_400),
        "1m" => Some(30 * 86_400),
        "never" => None,
        _ => return None,
    })
}

/// What `POST <base>/s` is sent: the share to make, its files by path
/// and size (their bytes follow, one `PUT` each). The records are the
/// web's stored song records as JSON (`R`: a shell's own record type;
/// the server keeps them as given).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NewShare<R = Value> {
    #[serde(default)]
    pub pack: String,
    /// One of [`EXPIRIES`]. A share made under an upload grant lasts as
    /// its room does, whatever this says.
    #[serde(default)]
    pub expires: String,
    pub songs: Vec<ShareSongDoc<R>>,
    /// Whether the pack's art comes.
    #[serde(default)]
    pub pack_art: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShareSongDoc<R = Value> {
    pub record: R,
    #[serde(default)]
    pub files: Vec<ShareFileDoc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShareFileDoc {
    pub path: String,
    #[serde(default)]
    pub bytes: f64,
    /// Relative to the base the share was read at, or absolute; empty
    /// (and left out) when the share is being made.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
}

impl<R> NewShare<R> {
    /// Why the share is refused (`413`), if it is.
    pub fn check(&self) -> Result<(), &'static str> {
        if self.songs.is_empty() {
            return Err("a share needs a song");
        }
        if self.songs.len() > MAX_SONGS {
            return Err("too many songs");
        }
        if expiry_seconds(&self.expires).is_none() {
            return Err("unknown expiry");
        }
        let mut total = 0.0;
        for song in &self.songs {
            for f in &song.files {
                if !valid_path(&f.path) {
                    return Err("a file's path is not allowed");
                }
                if f.bytes < 0.0 || f.bytes > MAX_FILE_BYTES {
                    return Err("a file is too large");
                }
                total += f.bytes;
            }
            let mut paths: Vec<&str> = song.files.iter().map(|f| f.path.as_str()).collect();
            paths.sort_unstable();
            if paths.windows(2).any(|w| w[0] == w[1]) {
                return Err("a file is listed twice");
            }
        }
        if total > MAX_SHARE_BYTES {
            return Err("the share is too large");
        }
        Ok(())
    }

    /// The file `path` of song `n`, if listed.
    pub fn file(&self, n: usize, path: &str) -> Option<&ShareFileDoc> {
        self.songs.get(n)?.files.iter().find(|f| f.path == path)
    }

    /// The bytes of its files, as listed.
    pub fn bytes(&self) -> f64 {
        self.songs
            .iter()
            .flat_map(|s| s.files.iter())
            .map(|f| f.bytes)
            .sum()
    }
}

/// The server's answer to `POST <base>/s`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Created {
    pub id: String,
}

/// A share as the server answers `GET <base>/s/<id>` (what it writes;
/// the game reads it as a [`ShareDoc`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Share {
    pub pack: String,
    /// Unix seconds; `None`: never.
    pub expires_at: Option<i64>,
    pub created_at: i64,
    pub songs: Vec<ShareSongDoc<Value>>,
    /// The art's URL, if any.
    pub pack_art: Option<String>,
}

/// A share as the game reads `GET <base>/s/<id>`: the records as a shell
/// stores them (`R`: the web reads them whole, desktop takes the entry
/// and the pack), anything missing defaulted.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ShareDoc<R = Value> {
    #[serde(default)]
    pub pack: String,
    /// Unix seconds; `None`: never.
    #[serde(default)]
    pub expires_at: Option<f64>,
    #[serde(default)]
    pub created_at: f64,
    pub songs: Vec<ShareSongDoc<R>>,
    /// The art's URL, if any.
    #[serde(default)]
    pub pack_art: Value,
    /// The share's id, as loaded (not in the server's answer).
    #[serde(skip)]
    pub id: String,
}

/// A share as the server lists it (`GET <base>/s`, with a token).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ShareSummary {
    pub id: String,
    #[serde(default)]
    pub pack: String,
    #[serde(default)]
    pub songs: usize,
    #[serde(default)]
    pub bytes: f64,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub expires_at: Option<i64>,
    #[serde(default)]
    pub finished: bool,
    /// Made with the token that asked.
    #[serde(default)]
    pub mine: bool,
}

/// The listing: the asking token's role (`admin` sees and deletes every
/// share, `upload` its own) and the shares it may see.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShareListing {
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub shares: Vec<ShareSummary>,
}

/// A file's path inside a song's folder: relative, no `..`, no empty
/// parts, nothing a key could not hold.
pub fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 512
        && !path.starts_with('/')
        && !path.ends_with('/')
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != ".." && !p.contains('\\'))
        && !path.chars().any(|c| c.is_control())
}

/// `part` percent-encoded but for RFC 3986's unreserved characters.
pub fn percent_encode(part: &str) -> String {
    let mut out = String::new();
    for b in part.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// A path's parts percent-encoded for a URL, the slashes kept.
pub fn encode_path(path: &str) -> String {
    path.split('/')
        .map(percent_encode)
        .collect::<Vec<_>>()
        .join("/")
}

/// A percent-encoded path decoded (UTF-8), `None` when malformed.
pub fn decode_path(encoded: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(encoded.len());
    let mut it = encoded.bytes();
    while let Some(b) = it.next() {
        if b == b'%' {
            let h = it.next()?;
            let l = it.next()?;
            let hex = |c: u8| (c as char).to_digit(16);
            bytes.push((hex(h)? * 16 + hex(l)?) as u8);
        } else {
            bytes.push(b);
        }
    }
    String::from_utf8(bytes).ok()
}

/// Where file `path` of song `n` of share `id` is, relative to the API's
/// base: `s/<id>/f/<n>/<encoded path>` (the URL a share lists).
pub fn file_path(id: &str, n: usize, path: &str) -> String {
    format!("s/{id}/f/{n}/{}", encode_path(path))
}

/// Where a share's pack art is, relative to the API's base.
pub fn art_path(id: &str) -> String {
    format!("s/{id}/art")
}

/// The URL of file `stored` of song `n` of share `id` at `base`, as the
/// server has it (what a listing leaves out is asked for there).
pub fn file_url(base: &str, id: &str, n: usize, stored: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        file_path(id, n, stored)
    )
}

/// A listed URL as fetched: an absolute one as it is, a relative one
/// under `base`.
pub fn resolve_url(base: &str, url: &str) -> String {
    if url.contains("://") {
        url.to_string()
    } else {
        format!(
            "{}/{}",
            base.trim_end_matches('/'),
            url.trim_start_matches('/')
        )
    }
}

/// A request's route under the API's base.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    /// `/s`.
    Shares,
    /// `/s/<id>`.
    Share(String),
    /// `/s/<id>/finish`.
    Finish(String),
    /// `/s/<id>/art`.
    Art(String),
    /// `/s/<id>/f/<n>/<path>` (the path decoded).
    File {
        id: String,
        n: usize,
        path: String,
    },
    NotFound,
}

impl Route {
    /// `path` is the request's path with the base path removed.
    pub fn parse(path: &str) -> Route {
        let path = path.trim_start_matches('/');
        let Some(rest) = path.strip_prefix("s") else {
            return Route::NotFound;
        };
        if rest.is_empty() || rest == "/" {
            return Route::Shares;
        }
        let rest = rest.trim_start_matches('/');
        let (id, tail) = rest.split_once('/').unwrap_or((rest, ""));
        if !valid_minted_id(id) {
            return Route::NotFound;
        }
        let id = id.to_string();
        match tail {
            "" => Route::Share(id),
            "finish" => Route::Finish(id),
            "art" => Route::Art(id),
            t => {
                let Some(t) = t.strip_prefix("f/") else {
                    return Route::NotFound;
                };
                let Some((n, p)) = t.split_once('/') else {
                    return Route::NotFound;
                };
                let (Ok(n), Some(path)) = (n.parse(), decode_path(p)) else {
                    return Route::NotFound;
                };
                if !valid_path(&path) {
                    return Route::NotFound;
                }
                Route::File { id, n, path }
            }
        }
    }
}

/// The path with the base path (`/ditoditoinfinito/api`) removed; `None`
/// when it is not under it.
pub fn strip_base<'a>(path: &'a str, base: &str) -> Option<&'a str> {
    let base = base.trim_end_matches('/');
    if base.is_empty() {
        return Some(path);
    }
    let rest = path.strip_prefix(base)?;
    if rest.is_empty() || rest.starts_with('/') {
        Some(rest)
    } else {
        None
    }
}

/// An upload grant's claims (M5d): an online room's authority lets one
/// member (its host, a co-host or an admin) upload one stage's song, for a
/// while. The server signs them and checks every `/s` request made with
/// `Authorization: Grant <grant>` against them; the share made lasts as
/// the room does and belongs to it, never to a token.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UploadGrant {
    pub room: String,
    /// The member's player id (`twitch:<id>`).
    pub player: String,
    pub stage: u64,
    /// What the stage was chosen to play ([`content_key`]): the shares a
    /// room keeps are found again by it.
    pub content_key: String,
    pub max_bytes: f64,
    pub max_songs: usize,
    pub expires_at_ms: i64,
}

/// How long a grant lasts.
pub const GRANT_TTL_MS: i64 = 30 * 60 * 1_000;
/// The songs a grant uploads: the stage's one.
pub const GRANT_MAX_SONGS: usize = 1;

impl UploadGrant {
    /// Whether `share` fits what the grant allows.
    pub fn allows<R>(&self, share: &NewShare<R>) -> Result<(), &'static str> {
        if share.songs.len() > self.max_songs {
            return Err("more songs than the grant allows");
        }
        if share.bytes() > self.max_bytes {
            return Err("larger than the grant allows");
        }
        Ok(())
    }
}

/// The owner a share made under a grant keeps: the room and the member,
/// never a token's mark.
pub fn room_owner(room: &str, player: &str) -> String {
    format!("room:{room}:{player}")
}

/// The content key of a stage's chosen chart: its digest as the room
/// names it (`ddi_room::ChartKey`). PROVISIONAL: the chart alone, so a
/// second upload of the same chart in a room reuses the first's share
/// (its music and pictures included).
pub fn content_key(chart_digest: &str) -> String {
    chart_digest.to_string()
}

/// The grant of an `Authorization: Grant <grant>` header.
pub fn grant_of(header: &str) -> Option<&str> {
    let (scheme, token) = header.trim().split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("grant")
        .then(|| token.trim())
        .filter(|t| !t.is_empty())
}

/// What `POST <base>/rooms/<room>/grant {stage}` answers a member who may
/// upload the stage's song: a grant to upload it with, or the share the
/// room already has of the same content (the stage then is available at
/// once, nothing to upload).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GrantAnswer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share: Option<String>,
    #[serde(default)]
    pub expires_at_ms: i64,
    #[serde(default)]
    pub max_bytes: f64,
}

/// What a grant's upload is refused over, by the server's code
/// (`{"error": code}`), so a client can tell renewing from giving up.
pub mod refusals {
    /// The grant expired: ask the room for another and go on.
    pub const GRANT_EXPIRED: &str = "grant_expired";
    /// Not a grant the server signed.
    pub const GRANT_INVALID: &str = "grant_invalid";
    /// The grant is another room's, stage's or member's.
    pub const GRANT_SCOPE: &str = "grant_scope";
    /// More than the grant allows (`413`).
    pub const GRANT_LIMITS: &str = "grant_limits";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_share() -> NewShare {
        NewShare {
            pack: "P".into(),
            expires: "1w".into(),
            songs: vec![ShareSongDoc {
                record: serde_json::json!({"entry": {"id": "u-1"}}),
                files: vec![
                    ShareFileDoc {
                        path: "chart.ssc".into(),
                        bytes: 10.0,
                        url: String::new(),
                    },
                    ShareFileDoc {
                        path: "bg/a b.png".into(),
                        bytes: 20.0,
                        url: String::new(),
                    },
                ],
            }],
            pack_art: true,
        }
    }

    #[test]
    fn ids_are_short_and_plain() {
        assert!(valid_id("abc12345") && valid_id("A") && valid_id(&"z".repeat(32)));
        assert!(!valid_id("") && !valid_id("ab-c") && !valid_id(&"z".repeat(33)));
        assert!(valid_minted_id("abc12345"));
        assert!(!valid_minted_id("abc1234") && !valid_minted_id("abc1234-"));
        let id = id_from_bytes(&[0, 61, 62, 255, 10, 35, 36, 61]);
        assert_eq!(id, "0z07AZaz");
        assert!(valid_minted_id(&id));
        assert_eq!(song_id("abc12345", 3), "shared:abc12345:3");
        assert_eq!(parse_song_id("shared:abc12345:3"), Some(("abc12345", 3)));
        assert_eq!(parse_song_id("shared:abc12345:x"), None);
        assert_eq!(parse_song_id("shared:ab c:1"), None);
        assert_eq!(parse_song_id("u-abc"), None);
        assert!(is_shared_id("shared:q:0") && !is_shared_id("beatucada"));
    }

    #[test]
    fn a_new_share_is_checked() {
        assert_eq!(new_share().check(), Ok(()));
        assert_eq!(new_share().bytes(), 30.0);
        let mut s = new_share();
        s.songs.clear();
        assert!(s.check().is_err());
        let mut s = new_share();
        s.expires = "2w".into();
        assert!(s.check().is_err());
        let mut s = new_share();
        s.songs[0].files[0].path = "../x".into();
        assert!(s.check().is_err());
        let mut s = new_share();
        s.songs[0].files[1].path = "chart.ssc".into();
        assert!(s.check().is_err());
        let mut s = new_share();
        s.songs[0].files[0].bytes = MAX_FILE_BYTES + 1.0;
        assert!(s.check().is_err());
        let mut s = new_share();
        s.songs = (0..MAX_SONGS + 1).map(|_| s.songs[0].clone()).collect();
        assert!(s.check().is_err());
        assert_eq!(expiry_seconds("1w"), Some(Some(7 * 86_400)));
        assert_eq!(expiry_seconds("never"), Some(None));
        assert_eq!(expiry_seconds("2w"), None);
        assert!(EXPIRIES.iter().all(|w| expiry_seconds(w).is_some()));
    }

    #[test]
    fn the_wire_forms_stay_as_they_were() {
        // What is sent leaves out the URLs a share being made has none of.
        let new = NewShare {
            pack: "P".into(),
            expires: "1w".into(),
            songs: vec![ShareSongDoc {
                record: serde_json::json!({"pack": "P"}),
                files: vec![ShareFileDoc {
                    path: "a.ssc".into(),
                    bytes: 3.0,
                    url: String::new(),
                }],
            }],
            pack_art: false,
        };
        assert_eq!(
            serde_json::to_string(&new).unwrap(),
            r#"{"pack":"P","expires":"1w","songs":[{"record":{"pack":"P"},"files":[{"path":"a.ssc","bytes":3.0}]}],"pack_art":false}"#
        );
        // The server reads what a client leaves out as defaults.
        let lean: NewShare = serde_json::from_str(r#"{"songs":[{"record":{}}]}"#).unwrap();
        assert_eq!((lean.pack.as_str(), lean.expires.as_str()), ("", ""));
        assert!(!lean.pack_art && lean.songs[0].files.is_empty());
        // The server's answer, as written and as the game reads it.
        let share = Share {
            pack: "P".into(),
            expires_at: Some(1_900_000_000),
            created_at: 1_700_000_000,
            songs: vec![ShareSongDoc {
                record: serde_json::json!({"entry": {"id": "u-1"}}),
                files: vec![ShareFileDoc {
                    path: "bg/a b.png".into(),
                    bytes: 20.0,
                    url: file_path("abc12345", 0, "bg/a b.png"),
                }],
            }],
            pack_art: Some(art_path("abc12345")),
        };
        let text = serde_json::to_string(&share).unwrap();
        assert_eq!(
            text,
            r#"{"pack":"P","expires_at":1900000000,"created_at":1700000000,"songs":[{"record":{"entry":{"id":"u-1"}},"files":[{"path":"bg/a b.png","bytes":20.0,"url":"s/abc12345/f/0/bg/a%20b.png"}]}],"pack_art":"s/abc12345/art"}"#
        );
        let read: ShareDoc = serde_json::from_str(&text).unwrap();
        assert_eq!(read.expires_at, Some(1_900_000_000.0));
        assert_eq!(read.songs[0].files[0].url, "s/abc12345/f/0/bg/a%20b.png");
        assert_eq!(read.pack_art, serde_json::json!("s/abc12345/art"));
        let listing = ShareListing {
            role: "upload".into(),
            shares: vec![ShareSummary {
                id: "abc12345".into(),
                pack: "P".into(),
                songs: 1,
                bytes: 30.0,
                created_at: 1,
                expires_at: None,
                finished: true,
                mine: true,
            }],
        };
        assert_eq!(
            serde_json::to_string(&listing).unwrap(),
            r#"{"role":"upload","shares":[{"id":"abc12345","pack":"P","songs":1,"bytes":30.0,"created_at":1,"expires_at":null,"finished":true,"mine":true}]}"#
        );
    }

    #[test]
    fn urls_and_routes() {
        assert_eq!(
            resolve_url("https://w/app/api", "s/x/f/0/a.ssc"),
            "https://w/app/api/s/x/f/0/a.ssc"
        );
        assert_eq!(resolve_url("https://w/api", "https://o/x"), "https://o/x");
        assert_eq!(
            file_url("https://w/app/api/", "abc12345", 1, "bg/a b.png"),
            "https://w/app/api/s/abc12345/f/1/bg/a%20b.png"
        );
        assert_eq!(decode_path("bg/a%20b.png").as_deref(), Some("bg/a b.png"));
        assert_eq!(decode_path("%zz"), None);
        assert_eq!(Route::parse("/s"), Route::Shares);
        assert_eq!(Route::parse("/s/"), Route::Shares);
        assert_eq!(Route::parse("/s/abc12345"), Route::Share("abc12345".into()));
        assert_eq!(
            Route::parse("/s/abc12345/finish"),
            Route::Finish("abc12345".into())
        );
        assert_eq!(
            Route::parse("/s/abc12345/art"),
            Route::Art("abc12345".into())
        );
        assert_eq!(
            Route::parse("/s/abc12345/f/2/bg/a%20b.png"),
            Route::File {
                id: "abc12345".into(),
                n: 2,
                path: "bg/a b.png".into()
            }
        );
        assert_eq!(Route::parse("/s/abc12345/f/2/../x"), Route::NotFound);
        assert_eq!(Route::parse("/s/abc1234/"), Route::NotFound);
        assert_eq!(Route::parse("/x"), Route::NotFound);
        let base = "/ditoditoinfinito/api";
        assert_eq!(strip_base("/ditoditoinfinito/api/s/x", base), Some("/s/x"));
        assert_eq!(strip_base("/ditoditoinfinito/api", base), Some(""));
        assert_eq!(strip_base("/ditoditoinfinito/apis/s", base), None);
        assert_eq!(strip_base("/s", ""), Some("/s"));
    }

    #[test]
    fn grants_name_their_room_and_bound_the_upload() {
        let g = UploadGrant {
            room: "Room000001".into(),
            player: "twitch:1".into(),
            stage: 3,
            content_key: content_key("00ff"),
            max_bytes: 25.0,
            max_songs: GRANT_MAX_SONGS,
            expires_at_ms: 10,
        };
        assert_eq!(g.allows(&new_share()), Err("larger than the grant allows"));
        let small = UploadGrant {
            max_bytes: MAX_SHARE_BYTES,
            ..g.clone()
        };
        assert_eq!(small.allows(&new_share()), Ok(()));
        let mut two = new_share();
        two.songs.push(two.songs[0].clone());
        assert!(small.allows(&two).is_err());
        assert_eq!(
            room_owner("Room000001", "twitch:1"),
            "room:Room000001:twitch:1"
        );
        assert_eq!(grant_of("Grant abc.def"), Some("abc.def"));
        assert_eq!(grant_of("grant  abc "), Some("abc"));
        assert_eq!(grant_of("Bearer abc"), None);
        assert_eq!(grant_of("Grant "), None);
        let answer: GrantAnswer = serde_json::from_str(r#"{"share":"abc12345"}"#).unwrap();
        assert_eq!(answer.share.as_deref(), Some("abc12345"));
        assert!(answer.grant.is_none());
        assert_eq!(
            serde_json::to_string(&GrantAnswer {
                grant: Some("g".into()),
                share: None,
                expires_at_ms: 1,
                max_bytes: 2.0
            })
            .unwrap(),
            r#"{"grant":"g","expires_at_ms":1,"max_bytes":2.0}"#
        );
    }
}
