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

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::manifest::ManifestEntry;

/// The expiries a share is made with: the server's word and the label.
pub const EXPIRIES: [(&str, &str); 5] = [
    ("1h", "1 hour"),
    ("1d", "1 day"),
    ("1w", "1 week"),
    ("1m", "1 month"),
    ("never", "Forever"),
];

/// A share as the server answers `GET <base>/s/<id>` (and, without the
/// URLs and the times, as `POST <base>/s` is sent). The records are the
/// web's stored song records as JSON: the web reads them whole, desktop
/// takes the entry and the pack ([`record_entry`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShareDoc {
    #[serde(default)]
    pub pack: String,
    /// Unix seconds; `None`: never.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<f64>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub created_at: f64,
    pub songs: Vec<ShareSongDoc>,
    /// On reading: the art's URL, if any; on making: whether one comes.
    #[serde(default)]
    pub pack_art: Value,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShareSongDoc {
    pub record: Value,
    #[serde(default)]
    pub files: Vec<ShareFileDoc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShareFileDoc {
    pub path: String,
    #[serde(default)]
    pub bytes: f64,
    /// Relative to the base the share was read at, or absolute; empty
    /// when the share is being made.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
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

/// A size in a few words ("12 MB").
pub fn size_text(bytes: f64) -> String {
    if bytes >= 1e9 {
        format!("{:.1} GB", bytes / 1e9)
    } else if bytes >= 1e6 {
        format!("{:.0} MB", bytes / 1e6)
    } else if bytes >= 1e3 {
        format!("{:.0} kB", bytes / 1e3)
    } else {
        format!("{bytes:.0} B")
    }
}

/// The entry and the pack of a song's record.
pub fn record_entry(record: &Value) -> Option<(ManifestEntry, String)> {
    let entry = serde_json::from_value(record.get("entry")?.clone()).ok()?;
    let pack = record
        .get("pack")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Some((entry, pack))
}

/// A song's record for a share, as the web stores a song (its
/// `ImportedSong`: the entry, the pack, when and how much).
pub fn record_of(entry: &ManifestEntry, pack: &str, now_ms: f64, bytes: f64) -> Value {
    serde_json::json!({
        "entry": entry,
        "pack": pack,
        "dir": "",
        "imported": now_ms,
        "bytes": bytes,
    })
}

/// The files of a song as a share lists them, `(stored, named)`: the
/// path the web stores and shares a file under, and the path the entry
/// names it by (relative to the song's folder). Background images and
/// movies go under `bg/`, scripted backgrounds' files under `bga/`, a
/// work's layers under `layer/`, the rest as named. A desktop reading a
/// share writes each at its named path; one making a share reads each
/// there.
pub fn stored_files(entry: &ManifestEntry) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut push = |stored: String, named: String| {
        if !named.is_empty() && !out.iter().any(|(s, _)| *s == stored) {
            out.push((stored, named));
        }
    };
    let plain = |p: &str| (p.to_string(), p.to_string());
    let (s, n) = plain(entry.chart.as_str());
    push(s, n);
    let (s, n) = plain(entry.music.as_str());
    push(s, n);
    for c in &entry.charts {
        if let Some(m) = &c.music {
            let (s, n) = plain(m.as_str());
            push(s, n);
        }
    }
    for f in [
        &entry.banner,
        &entry.background,
        &entry.jacket,
        &entry.cd_title,
        &entry.lyrics,
        &entry.preview,
    ]
    .into_iter()
    .flatten()
    {
        let (s, n) = plain(f.as_str());
        push(s, n);
    }
    for f in &entry.bg_images {
        push(format!("bg/{f}"), f.to_string());
    }
    for v in entry.bg_videos.iter().filter(|v| v.stored) {
        push(format!("bg/{}", v.name), v.name.to_string());
    }
    if let Some(v) = entry.preview_video.as_ref().filter(|v| v.stored) {
        push(format!("bg/{}", v.name), v.name.to_string());
    }
    for f in &entry.bg_scripts {
        push(format!("bga/{f}"), f.to_string());
    }
    for f in &entry.layer_images {
        push(format!("layer/{f}"), f.to_string());
    }
    out
}

/// The files a play needs first, of [`stored_files`]: the chart, the
/// music, the pictures and the lyrics (everything but what the
/// background changes play, which a play fetches when it comes).
pub fn is_small(stored: &str) -> bool {
    !(stored.starts_with("bg/") || stored.starts_with("bga/") || stored.starts_with("layer/"))
}

/// The share id a `--shared` argument names: a link (`…#/s/<id>`), or
/// the id itself.
pub fn parse_link(arg: &str) -> Option<String> {
    let arg = arg.trim();
    let id = match arg.rsplit_once("#/s/") {
        Some((_, rest)) => rest.split(['?', '&', '/']).next().unwrap_or(""),
        None => arg,
    };
    valid_id(id).then(|| id.to_string())
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

/// The URL of file `stored` of song `n` of share `id` at `base`, as the
/// server has it (what a listing leaves out is asked for there).
pub fn file_url(base: &str, id: &str, n: usize, stored: &str) -> String {
    let encoded: Vec<String> = stored.split('/').map(encode_part).collect();
    format!(
        "{}/s/{id}/f/{n}/{}",
        base.trim_end_matches('/'),
        encoded.join("/")
    )
}

fn encode_part(part: &str) -> String {
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

/// The page a share is opened at: the API's base is `api` beside the
/// page, so the link is the base without it, then `#/s/<id>`.
pub fn link_of(base: &str, id: &str) -> String {
    let page = base
        .trim_end_matches('/')
        .strip_suffix("/api")
        .unwrap_or(base.trim_end_matches('/'));
    format!("{page}/#/s/{id}")
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

    #[test]
    fn a_shares_files_are_stored_as_the_web_keeps_them() {
        let entry: ManifestEntry = serde_json::from_value(serde_json::json!({
            "id": "u-1", "title": "T", "artist": "A", "chart": "song.ssc", "music": "song.ogg",
            "banner": "bn.png", "bg_images": ["BG/one.png"],
            "bg_videos": [{"name": "clip.avi", "format": {"container": "avi", "codec": "h264"}},
                          {"name": "gone.avi", "format": {"container": "avi", "codec": "h264"}, "stored": false}],
            "bg_scripts": ["Fx/default.lua"], "layer_images": ["back.png"]
        }))
        .unwrap();
        let files = stored_files(&entry);
        assert_eq!(
            files,
            vec![
                ("song.ssc".to_string(), "song.ssc".to_string()),
                ("song.ogg".to_string(), "song.ogg".to_string()),
                ("bn.png".to_string(), "bn.png".to_string()),
                ("bg/BG/one.png".to_string(), "BG/one.png".to_string()),
                ("bg/clip.avi".to_string(), "clip.avi".to_string()),
                (
                    "bga/Fx/default.lua".to_string(),
                    "Fx/default.lua".to_string()
                ),
                ("layer/back.png".to_string(), "back.png".to_string()),
            ]
        );
        assert!(is_small("song.ogg") && !is_small("bg/clip.avi"));
        let record = record_of(&entry, "P", 1.0, 2.0);
        let (back, pack) = record_entry(&record).unwrap();
        assert_eq!(back.id, "u-1");
        assert_eq!(pack, "P");
        let doc: ShareDoc = serde_json::from_str(
            r#"{"pack":"P","expires_at":null,"songs":[{"record":{"entry":{"id":"u-1","title":"T","artist":"A","chart":"a.ssc","music":"a.ogg"}},"files":[{"path":"a.ssc","bytes":3,"url":"s/x/f/0/a.ssc"}]}],"pack_art":null}"#,
        )
        .unwrap();
        assert_eq!(doc.songs[0].files[0].url, "s/x/f/0/a.ssc");
        assert_eq!(
            resolve_url("https://w/app/api", &doc.songs[0].files[0].url),
            "https://w/app/api/s/x/f/0/a.ssc"
        );
        assert_eq!(
            file_url("https://w/app/api/", "abc12345", 1, "bg/a b.png"),
            "https://w/app/api/s/abc12345/f/1/bg/a%20b.png"
        );
    }

    #[test]
    fn links_name_their_share() {
        assert_eq!(
            parse_link("https://sugoijan.dev/ditoditoinfinito/#/s/abc12345"),
            Some("abc12345".into())
        );
        assert_eq!(parse_link("abc12345"), Some("abc12345".into()));
        assert_eq!(
            parse_link("http://x/#/s/abc12345?x=1"),
            Some("abc12345".into())
        );
        assert_eq!(parse_link("not a link!"), None);
        assert_eq!(
            link_of("https://sugoijan.dev/ditoditoinfinito/api", "abc12345"),
            "https://sugoijan.dev/ditoditoinfinito/#/s/abc12345"
        );
        assert_eq!(
            link_of("http://127.0.0.1:8787/x", "abc12345"),
            "http://127.0.0.1:8787/x/#/s/abc12345"
        );
    }
}
