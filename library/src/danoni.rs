//! Dancing☆Onigiri works in an import.
//!
//! A work is found two ways:
//! - a page (`.html`/`.htm`) whose dos sits inline and/or in an external
//!   file the page names, with music under the site's `music` folder, as
//!   published (a saved copy of a work's site or a downloaded package);
//! - a dump of the `ddi-danoni-dump/1` format (`docs/research/danoni-corpus.md`),
//!   whose `manifest.json` points at the effective dos fields and the
//!   decoded music of each captured chart.
//!
//! Each music file of a work becomes one song. What gets stored is the
//! work's merged dos fields and the song's music number
//! ([`StoredWork`], file extension [`STORED_EXT`]), so loading re-runs the
//! importer and an improved importer improves stored songs too.
//!
//! Paths follow danoniplus (`getFullMusicUrl`, `getFilePath`,
//! `js/lib/dosConverter.js` 620–636, `js/danoni_main.js` 1265–1277): the
//! music folder (`musicFolder`, default `music`) sits next to the engine's
//! `js` folder, one level above the pages in the usual site layout, and a
//! leading `(..)` means "relative to the page". Lookups are
//! case-insensitive, and a file found nowhere expected is looked for by
//! name anywhere in the import.

use ddi_chart::Song;
use ddi_chart::formats::danoni::{self, Dos};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::pack::{extension, file_name, is_ignored, join_relative, parent};

/// Extension of a stored work (`chart.danoni`).
pub const STORED_EXT: &str = "danoni";

/// The dump format this module reads.
pub const DUMP_SCHEMA: &str = "ddi-danoni-dump/1";

/// What an imported song stores in place of a simfile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoredWork {
    /// Music number of this song in the work.
    pub music: usize,
    /// The work's dos fields, merged.
    pub fields: Vec<(String, String)>,
}

impl StoredWork {
    pub fn new(dos: &Dos, music: usize) -> StoredWork {
        StoredWork {
            music,
            fields: dos
                .fields()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    pub fn dos(&self) -> Dos {
        let mut d = Dos::default();
        for (k, v) in &self.fields {
            d.set(k, v.clone());
        }
        d
    }
}

/// The song a stored work file describes.
pub fn load_stored(text: &str) -> Result<Song, String> {
    let work: StoredWork = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let import = danoni::import(&work.dos())?;
    import
        .music
        .iter()
        .position(|m| *m == work.music)
        .map(|k| import.songs[k].clone())
        .ok_or_else(|| format!("the work has no music number {}", work.music))
}

/// A place a work may be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkSource {
    /// A page that may hold or name a dos.
    Page(String),
    /// A dump's `manifest.json`.
    Dump(String),
}

/// Pages and dump manifests in an import, sorted by path. Pages without a
/// dos are told apart only once read ([`danoni::source::scan_html`]).
pub fn work_sources<S: AsRef<str>>(paths: &[S]) -> Vec<WorkSource> {
    let mut out: Vec<WorkSource> = paths
        .iter()
        .map(AsRef::as_ref)
        .filter(|p| !is_ignored(p))
        .filter_map(|p| {
            let ext = extension(p)?.to_ascii_lowercase();
            if ext == "html" || ext == "htm" {
                Some(WorkSource::Page(p.to_string()))
            } else if file_name(p).eq_ignore_ascii_case("manifest.json") {
                Some(WorkSource::Dump(p.to_string()))
            } else {
                None
            }
        })
        .collect();
    out.sort_by(|a, b| key(a).cmp(key(b)));
    out
}

fn key(s: &WorkSource) -> &str {
    match s {
        WorkSource::Page(p) | WorkSource::Dump(p) => p,
    }
}

/// The path in `paths` equal to `want`, ignoring case.
fn find<S: AsRef<str>>(paths: &[S], want: &str) -> Option<String> {
    paths
        .iter()
        .map(AsRef::as_ref)
        .find(|p| p.eq_ignore_ascii_case(want))
        .map(str::to_string)
}

/// The external dos file a page names, relative to the page (`?dos=`
/// queries are not available offline).
pub fn external_path<S: AsRef<str>>(page: &str, name: &str, paths: &[S]) -> Option<String> {
    let name = name.split('?').next().unwrap_or(name);
    join_relative(parent(page), name)
        .and_then(|p| find(paths, &p))
        .or_else(|| by_name(paths, name, None))
}

/// The music file of a page's song: where danoniplus would load it, else
/// a file of the same name anywhere in the import (preferring a `music`
/// folder).
pub fn music_path<S: AsRef<str>>(page: &str, dos: &Dos, url: &str, paths: &[S]) -> Option<String> {
    const CURRENT: &str = "(..)";
    let url = url.split('?').next().unwrap_or(url);
    let page_dir = parent(page);
    let folder = dos.get("musicFolder").unwrap_or("music");
    let mut tries: Vec<Option<String>> = Vec::new();
    if let Some(rest) = url.strip_prefix(CURRENT) {
        tries.push(join_relative(page_dir, rest));
    } else if let Some(rest) = folder.strip_prefix(CURRENT) {
        tries.push(join_relative(page_dir, &format!("{rest}/{url}")));
    } else {
        // `<engine root>/../<folder>/<url>`, the engine root being the
        // folder holding `js/` — the pages' parent in the usual layout.
        tries.push(join_relative(page_dir, &format!("../{folder}/{url}")));
        tries.push(join_relative(page_dir, &format!("{folder}/{url}")));
    }
    tries
        .into_iter()
        .flatten()
        .find_map(|p| find(paths, &p))
        .or_else(|| by_name(paths, url, Some(folder)))
}

fn by_name<S: AsRef<str>>(paths: &[S], url: &str, folder: Option<&str>) -> Option<String> {
    let name = file_name(url.trim_start_matches("(..)"));
    if name.is_empty() {
        return None;
    }
    let mut matches: Vec<&str> = paths
        .iter()
        .map(AsRef::as_ref)
        .filter(|p| !is_ignored(p) && file_name(p).eq_ignore_ascii_case(name))
        .collect();
    let in_folder = |p: &&str| {
        folder.is_some_and(|f| {
            let f = f.trim_start_matches("(..)").trim_matches('/');
            file_name(parent(p)).eq_ignore_ascii_case(file_name(f))
        })
    };
    matches.sort_by_key(|p| (!in_folder(p), p.len()));
    matches.first().map(|p| p.to_string())
}

/// Whether a music reference is a script holding the audio as base64
/// (`.js`/`.txt`, which danoniplus runs and decodes).
pub fn is_encoded_music(path: &str) -> bool {
    matches!(
        extension(path).map(str::to_ascii_lowercase).as_deref(),
        Some("js" | "txt")
    )
}

/// The audio bytes of an encoded music script.
pub fn decode_music(js: &str) -> Option<Vec<u8>> {
    base64(&danoni::source::encoded_music(js)?)
}

/// Standard base64, ignoring whitespace; `None` on anything else.
fn base64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            c if c.is_ascii_whitespace() => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// What to read from a dump: the effective dos fields of one captured chart
/// (they hold the whole work) and the decoded music of each captured chart.
#[derive(Clone, Debug, PartialEq)]
pub struct DumpPlan {
    /// Path of an `effective-dos.json`.
    pub effective: String,
    /// `(chartId, music path, encoded)` of the captured charts with music;
    /// `encoded` when the path is the captured music script itself (no
    /// decoded copy), to be decoded on import.
    pub music: Vec<(usize, String, bool)>,
    /// Page title of the dump, for the report.
    pub page: String,
}

/// Reads a dump manifest. `Ok(None)` when the JSON is not a dump.
pub fn dump_plan<S: AsRef<str>>(
    manifest_path: &str,
    json: &str,
    paths: &[S],
) -> Result<Option<DumpPlan>, String> {
    // Any other `manifest.json` (web apps ship them) is not a dump.
    let Ok(m) = serde_json::from_str::<Value>(json) else {
        return Ok(None);
    };
    if m["schema"].as_str() != Some(DUMP_SCHEMA) {
        return Ok(None);
    }
    let dir = parent(manifest_path);
    let at = |rel: &str| join_relative(dir, rel).and_then(|p| find(paths, &p));
    let charts = m["charts"].as_array().cloned().unwrap_or_default();
    let effective = charts
        .iter()
        .find_map(|c| c["paths"]["effectiveDos"].as_str().and_then(&at))
        .ok_or("the dump has no effective dos")?;
    let mut music = Vec::new();
    for a in m["assets"].as_array().into_iter().flatten() {
        if a["role"].as_str() != Some("music") {
            continue;
        }
        let decoded = a["decodedPath"].as_str().and_then(&at);
        let (path, encoded) = match decoded {
            Some(p) => (p, false),
            None => {
                let Some(p) = a["path"].as_str().and_then(&at) else {
                    continue;
                };
                let script = a["mimeType"]
                    .as_str()
                    .is_some_and(|m| m.contains("javascript") || m.starts_with("text/"));
                (p, script)
            }
        };
        for id in a["chartIds"].as_array().into_iter().flatten() {
            if let Some(id) = id.as_u64().and_then(|i| usize::try_from(i).ok()) {
                music.push((id, path.clone(), encoded));
            }
        }
    }
    let page = m["page"]["finalUrl"]
        .as_str()
        .or(m["page"]["requestedUrl"].as_str())
        .unwrap_or(manifest_path)
        .to_string();
    Ok(Some(DumpPlan {
        effective,
        music,
        page,
    }))
}

/// The dos fields of an `effective-dos.json` (string fields only, as the
/// dump stores them).
pub fn dump_dos(effective_json: &str) -> Result<Dos, String> {
    let v: Value = serde_json::from_str(effective_json).map_err(|e| e.to_string())?;
    let fields = v["fields"].as_object().ok_or("no fields")?;
    let mut d = Dos::default();
    for (k, v) in fields {
        if let Some(v) = v.as_str() {
            d.set(k, v.to_string());
        }
    }
    Ok(d)
}

/// The charset a page declares (`<meta charset>` or `http-equiv`), read
/// from its bytes before decoding.
pub fn declared_charset(bytes: &[u8]) -> Option<String> {
    let head = &bytes[..bytes.len().min(4096)];
    let text: String = head
        .iter()
        .map(|&b| b as char)
        .collect::<String>()
        .to_ascii_lowercase();
    let at = text.find("charset=")? + "charset=".len();
    let rest = text[at..].trim_start_matches(['"', '\'']);
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(rest.len());
    (end > 0).then(|| rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_chart::formats::danoni::DosFlags;

    #[test]
    fn finds_pages_and_dumps() {
        let paths = [
            "site/danoni/work.html",
            "site/danoni/dos/score.txt",
            "site/music/Song.MP3",
            "site/js/danoni_main.js",
            "dump/manifest.json",
            "site/._work.html",
        ];
        assert_eq!(
            work_sources(&paths),
            [
                WorkSource::Dump("dump/manifest.json".into()),
                WorkSource::Page("site/danoni/work.html".into())
            ]
        );
        let dos = Dos::parse("|musicUrl=song.mp3|", DosFlags::default());
        assert_eq!(
            music_path("site/danoni/work.html", &dos, "song.mp3", &paths).as_deref(),
            Some("site/music/Song.MP3")
        );
        assert_eq!(
            external_path("site/danoni/work.html", "dos/score.txt", &paths).as_deref(),
            Some("site/danoni/dos/score.txt")
        );
        // `(..)`: relative to the page; a missing folder falls back to the name.
        let flat = ["work/work.html", "work/sound/a.ogg"];
        let dos = Dos::parse("|musicFolder=(..)sound|", DosFlags::default());
        assert_eq!(
            music_path("work/work.html", &dos, "a.ogg", &flat).as_deref(),
            Some("work/sound/a.ogg")
        );
        assert_eq!(
            music_path("work/work.html", &Dos::default(), "a.ogg", &flat).as_deref(),
            Some("work/sound/a.ogg")
        );
        assert!(is_encoded_music("music/song.js"));
        assert_eq!(
            music_path(
                "site/danoni/work.html",
                &Dos::default(),
                "song.mp3?v=2",
                &paths
            )
            .as_deref(),
            Some("site/music/Song.MP3")
        );
    }

    #[test]
    fn stored_works_round_trip() {
        let dos = Dos::parse(
            "|musicTitle=A,x$B,y|musicUrl=a.ogg$b.ogg|musicNo=0$1|difData=5,E$5,H|\
             |left_data=300|left2_data=400|",
            DosFlags::default(),
        );
        let stored = StoredWork::new(&dos, 1);
        let text = serde_json::to_string(&stored).unwrap();
        let song = load_stored(&text).unwrap();
        assert_eq!(song.title, "B");
        assert_eq!(song.charts[0].notes[0].tick.0, 400);
    }

    #[test]
    fn dumps_and_encodings() {
        let manifest = r#"{"schema":"ddi-danoni-dump/1","page":{"finalUrl":"https://x/w.html"},
            "charts":[{"chartId":0,"paths":{"effectiveDos":"charts/0000/effective-dos.json"}}],
            "assets":[{"role":"music","path":"blobs/sha256/ab","decodedPath":"media/ab.mp3","chartIds":[0]}]}"#;
        let paths = [
            "d/manifest.json",
            "d/charts/0000/effective-dos.json",
            "d/media/ab.mp3",
        ];
        let plan = dump_plan("d/manifest.json", manifest, &paths)
            .unwrap()
            .unwrap();
        assert_eq!(plan.effective, "d/charts/0000/effective-dos.json");
        assert_eq!(plan.music, [(0, "d/media/ab.mp3".to_string(), false)]);
        assert_eq!(dump_plan("x/manifest.json", "{}", &paths).unwrap(), None);
        assert_eq!(
            dump_plan("x/manifest.json", "{not json", &paths).unwrap(),
            None
        );
        let script = manifest.replace(
            r#","decodedPath":"media/ab.mp3""#,
            r#","mimeType":"application/javascript""#,
        );
        let paths2 = [
            "d/manifest.json",
            "d/charts/0000/effective-dos.json",
            "d/blobs/sha256/ab",
        ];
        let plan = dump_plan("d/manifest.json", &script, &paths2)
            .unwrap()
            .unwrap();
        assert_eq!(plan.music, [(0, "d/blobs/sha256/ab".to_string(), true)]);
        let dos = dump_dos(r#"{"fields":{"a":"1","n":{"x":1}}}"#).unwrap();
        assert_eq!(dos.get("a"), Some("1"));
        assert_eq!(dos.get("n"), None);
        assert_eq!(
            decode_music("function musicInit(){g_musicdata = \"SUQzBA==\";}"),
            Some(b"ID3\x04".to_vec())
        );
        assert_eq!(
            declared_charset(b"<html><head><meta charset=\"Shift_JIS\">").as_deref(),
            Some("shift_jis")
        );
        assert_eq!(
            declared_charset(
                b"<meta http-equiv=\"Content-Type\" content=\"text/html; charset=EUC-JP\">"
            )
            .as_deref(),
            Some("euc-jp")
        );
    }
}
