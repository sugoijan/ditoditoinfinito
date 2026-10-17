//! Dancing☆Onigiri works in a folder or an import, for both shells
//! (`docs/plans/desktop.md`, step 7.5b): what the web's import did with a
//! page or a dump once read, moved here as [`crate::scan`] was for
//! simfiles, so the web's import and desktop's library find the same
//! songs under the same ids with the same entries.
//!
//! A work is read in steps, because the web reads its files
//! asynchronously and desktop as it goes: a page's text
//! ([`read_page`]), then the external dos it names ([`Page::finish`]); a
//! dump's manifest ([`crate::danoni::dump_plan`]) and its effective dos
//! ([`dump_work`]). [`plan`] then finds its songs: their ids, music and
//! back and mask images, and what the report says. [`entry`] is a song's
//! list entry with the files named as the shell keeps them.

use std::collections::HashMap;

use ddi_chart::Song;
use ddi_chart::formats::danoni::{self, Dos, DosFlags, source};

use crate::danoni::{self as work, DumpPlan, SharedWork, WorkSource};
use crate::manifest::{EntryMeta, ManifestEntry, summarize};
use crate::pack;

/// The back and mask images a song keeps, at most
/// (`docs/plans/overlays.md`; the largest work at hand shows 99).
pub const MAX_LAYER_IMAGES: usize = 128;

/// Where a work's music comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum WorkMusic {
    /// Resolved from the page's `musicUrl` (the page's path).
    Page(String),
    /// Captured per chart: `chartId` → (path, encoded music script).
    Dump(HashMap<usize, (String, bool)>),
}

/// A work read: its merged dos, and where its music is.
#[derive(Clone, Debug, PartialEq)]
pub struct Work {
    pub dos: Dos,
    pub music: WorkMusic,
}

/// A page whose dos is read as far as the page itself holds it.
#[derive(Clone, Debug)]
pub struct Page {
    page: String,
    dos: Dos,
    flags: DosFlags,
    /// The external dos file the page names: its path among the import's
    /// files, and the charset to read it in (the page's own when it sets
    /// none).
    pub external: Option<(String, Option<String>)>,
}

/// A page's text (in its declared charset, [`crate::danoni::declared_charset`])
/// read: `Ok(None)` when it holds no dos (not a work).
pub fn read_page<S: AsRef<str>>(
    page: &str,
    html: &str,
    charset: Option<&str>,
    paths: &[S],
) -> Result<Option<Page>, String> {
    let Some(found) = source::scan_html(html) else {
        return Ok(None);
    };
    let mut dos = Dos::default();
    if let Some(inline) = &found.inline {
        dos.merge(inline, found.flags);
    }
    if found.divided {
        return Err("splits its charts over several dos files, which is not supported yet".into());
    }
    let external = match &found.external {
        Some(name) => {
            let path = work::external_path(page, name, paths)
                .ok_or_else(|| format!("its dos file {name} is missing"))?;
            let charset = found
                .external_charset
                .clone()
                .or_else(|| charset.map(str::to_string));
            Some((path, charset))
        }
        None => None,
    };
    Ok(Some(Page {
        page: page.to_string(),
        dos,
        flags: found.flags,
        external,
    }))
}

impl Page {
    /// The work, with the external dos file's text (decoded in its
    /// charset) when the page names one.
    pub fn finish(self, external: Option<&str>) -> Result<Work, String> {
        let mut dos = self.dos;
        if let Some((path, _)) = &self.external {
            let text = external.ok_or_else(|| format!("{path} was not read"))?;
            let name = pack::file_name(path);
            let dos_text = source::external_dos_text(text).ok_or_else(|| {
                format!("{name} builds its dos with a script, which an import never runs")
            })?;
            dos.merge(&dos_text, self.flags);
        }
        Ok(Work {
            dos,
            music: WorkMusic::Page(self.page),
        })
    }
}

/// A dump's work from its plan and its `effective-dos.json`.
pub fn dump_work(plan: DumpPlan, effective_json: &str) -> Result<Work, String> {
    Ok(Work {
        dos: work::dump_dos(effective_json)?,
        music: WorkMusic::Dump(
            plan.music
                .into_iter()
                .map(|(id, path, encoded)| (id, (path, encoded)))
                .collect(),
        ),
    })
}

/// A song of a work, found.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkSong {
    /// `pack::song_id` of its pack and the work's name with its music
    /// number.
    pub id: String,
    pub pack: String,
    /// The work's page or manifest (the report's label, the song's
    /// "folder").
    pub label: String,
    /// Its music number in the work.
    pub music_no: usize,
    pub song: Song,
    /// Its music file, and whether it is a script holding it as base64.
    pub music: String,
    pub encoded: bool,
    /// Its back and mask images: `(name as the data writes it, path)`.
    pub layers: Vec<(String, String)>,
}

/// A work's songs, and what the report says of it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkPlan {
    pub songs: Vec<WorkSong>,
    /// `(label, line)` for the report.
    pub warnings: Vec<(String, String)>,
    pub skipped: Vec<(String, String)>,
    /// The music files its songs use (their folders are not "music
    /// without a simfile").
    pub music: Vec<String>,
}

/// The songs of work `w` found at `ws` (labelled `label`) among `paths`:
/// their ids (a pack's from the top folder of `label`, else
/// `fallback_pack`), music and layer images; `seen` holds the ids of the
/// import so far (a second song of the same id is skipped).
pub fn plan<S: AsRef<str>>(
    ws: &WorkSource,
    label: &str,
    w: &Work,
    paths: &[S],
    fallback_pack: &str,
    seen: &mut HashMap<String, String>,
) -> WorkPlan {
    let mut out = WorkPlan::default();
    let import = match danoni::import(&w.dos) {
        Ok(i) => i,
        Err(e) => {
            out.skipped.push((label.to_string(), e));
            return out;
        }
    };
    out.warnings.extend(
        import
            .warnings
            .iter()
            .map(|x| (label.to_string(), x.clone())),
    );
    let pack = pack_name(label, fallback_pack);
    // A song's id comes from its pack and a name like a song folder's:
    // the page's path inside the pack (or the dump's folder) with its
    // music number. `pack::song_id` keeps only that path's last part, so
    // `a/index.html` and `b/index.html` of one pack get the same ids
    // (the second is skipped as a duplicate); changing it changes every
    // work's id, which needs a migration of the bests and offsets kept
    // by id (`docs/plans/desktop.md`, 7.5b).
    let ident = match ws {
        WorkSource::Page(page) => inside_pack(page),
        WorkSource::Dump(manifest) => inside_pack(parent(manifest)),
    };
    let mut uncaptured: Vec<&str> = Vec::new();
    for (k, song) in import.songs.iter().enumerate() {
        let music_no = import.music[k];
        let song_label = format!("{label} ({})", song.title);
        let music = match &w.music {
            WorkMusic::Page(page) => song
                .music
                .as_deref()
                .and_then(|url| work::music_path(page, &w.dos, url, paths))
                .map(|p| {
                    let encoded = work::is_encoded_music(&p);
                    (p, encoded)
                })
                .ok_or_else(|| match &song.music {
                    Some(url) => format!("music {url} was not found"),
                    None => "the work names no music".to_string(),
                }),
            WorkMusic::Dump(by_chart) => {
                let found = import
                    .charts
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.is_some_and(|(s, _)| s == k))
                    .find_map(|(id, _)| by_chart.get(&id).cloned());
                let Some(found) = found else {
                    // Dumps capture some charts of a work: one line for
                    // all the songs left without music.
                    uncaptured.push(&song.title);
                    continue;
                };
                Ok(found)
            }
        };
        let (music, encoded) = match music {
            Ok(m) => m,
            Err(e) => {
                out.skipped.push((song_label, e));
                continue;
            }
        };
        out.music.push(music.clone());
        // Back and mask images: a page's, where danoniplus loads them; a
        // capture holds none.
        let layers = match ws {
            WorkSource::Page(page) => {
                let (mut found, mut missing) = work::layer_images(song, page, &w.dos, paths);
                if !missing.is_empty() {
                    let more = missing.len().saturating_sub(12);
                    missing.truncate(12);
                    let more = if more > 0 {
                        format!(" and {more} more")
                    } else {
                        String::new()
                    };
                    out.warnings.push((
                        song_label.clone(),
                        format!(
                            "back and mask images not found (or not drawn, like SVG): {}{more}",
                            missing.join(", ")
                        ),
                    ));
                }
                if found.len() > MAX_LAYER_IMAGES {
                    out.warnings.push((
                        song_label.clone(),
                        format!(
                            "{} back and mask images; only the first {MAX_LAYER_IMAGES} are kept",
                            found.len()
                        ),
                    ));
                    found.truncate(MAX_LAYER_IMAGES);
                }
                found
            }
            WorkSource::Dump(_) => {
                if song.effects.iter().any(|e| {
                    (e.kind == danoni::SPRITE_BACK || e.kind == danoni::SPRITE_MASK)
                        && e.fields.get(2).is_some_and(|s| work::is_layer_image(s))
                }) {
                    out.warnings.push((
                        song_label.clone(),
                        "a capture holds no back and mask images".into(),
                    ));
                }
                Vec::new()
            }
        };
        out.warnings.extend(
            ddi_engine::sprites::import_notes(song)
                .into_iter()
                .map(|n| (song_label.clone(), n)),
        );
        let id = pack::song_id(&pack, &format!("{ident}#{music_no}"));
        if let Some(first) = seen.get(&id) {
            out.skipped
                .push((song_label, format!("same pack and file name as {first}")));
            continue;
        }
        seen.insert(id.clone(), label.to_string());
        out.songs.push(WorkSong {
            id,
            pack: pack.clone(),
            label: label.to_string(),
            music_no,
            song: song.clone(),
            music,
            encoded,
            layers,
        });
    }
    if !uncaptured.is_empty() {
        out.skipped.push((
            label.to_string(),
            format!(
                "{} song(s) of the work have no captured music: {}",
                uncaptured.len(),
                uncaptured.join(", ")
            ),
        ));
    }
    out
}

/// What a work's songs share once stored (the web keeps it once).
pub fn shared(w: &Work) -> SharedWork {
    SharedWork::new(&w.dos)
}

/// Song `s`'s list entry, its chart and music named `chart` and `music`
/// (as the shell keeps them): the song list's facts, its charts' keys
/// (digest, excessive), no preview window (a work's starts with the
/// song), its layers' images by name.
pub fn entry(s: &WorkSong, chart: &str, music: &str) -> ManifestEntry {
    let song = &s.song;
    let mut entry = summarize(
        EntryMeta {
            id: s.id.clone(),
            title: song.title.clone(),
            artist: song.artist.clone(),
            chart: chart.into(),
            music: music.into(),
            banner: None,
            background: None,
            bg_images: Vec::new(),
            bg_videos: Vec::new(),
            bg_shared: Vec::new(),
            credit: song.credit.trim().to_string(),
        },
        song,
    );
    fill_chart_keys(&mut entry, song);
    entry.preview_start = 0.0;
    entry.preview_length = 0.0;
    entry.layer_images = s
        .layers
        .iter()
        .map(|(name, _)| name.clone().into())
        .collect();
    entry
}

/// The keys the song list finds a chart's bests by: its digest and, for a
/// work's chart, its own excessive setting.
pub fn fill_chart_keys(entry: &mut ManifestEntry, song: &Song) {
    for c in &mut entry.charts {
        if c.index < song.charts.len() {
            c.digest = Some(song.chart_digest(c.index));
            c.excessive = ddi_engine::rules::danoni::chart_excessive(&song.charts[c.index]);
        }
    }
}

/// The song of music number `music_no` of work `w` (as a stored work's).
pub fn song_of(w: &Work, music_no: usize) -> Result<Song, String> {
    let import = danoni::import(&w.dos)?;
    import
        .music
        .iter()
        .position(|m| *m == music_no)
        .map(|k| import.songs[k].clone())
        .ok_or_else(|| format!("the work has no music number {music_no}"))
}

/// A path without its first folder (the pack).
fn inside_pack(path: &str) -> &str {
    path.split_once('/').map_or(path, |(_, rest)| rest)
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

/// The pack of a work: the top folder of what was picked.
fn pack_name(path: &str, fallback: &str) -> String {
    match path.split_once('/') {
        Some((top, _)) if !top.is_empty() => top.to_string(),
        _ => fallback.to_string(),
    }
}

/// Extension and content type of audio bytes by their signature (an
/// encoded music script, a dump's blob, does not say what it holds).
pub fn sniff_audio(bytes: &[u8]) -> (&'static str, &'static str) {
    if bytes.starts_with(b"OggS") {
        ("ogg", "audio/ogg")
    } else if bytes.starts_with(b"RIFF") {
        ("wav", "audio/wav")
    } else if bytes.starts_with(b"fLaC") {
        ("flac", "audio/flac")
    } else if bytes.len() >= 8 && &bytes[4..8] == b"ftyp" {
        ("m4a", "audio/mp4")
    } else {
        ("mp3", "audio/mpeg")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<html><head><meta charset=\"utf-8\"></head><body>\
        <input type=\"hidden\" id=\"dos\" value='|musicTitle=Song,Me|difData=5,Easy,3.5\
        |musicUrl=song.ogg|left_data=300,400|back_data=200,0,../img/a.png\n210,1,../img/gone.png|'>\
        <input type=\"hidden\" id=\"externalDos\" value=\"dos/more.txt\"></body></html>";

    #[test]
    fn a_pages_songs_ids_music_and_layers() {
        let paths = [
            "Pack/danoni/work.html",
            "Pack/danoni/dos/more.txt",
            "Pack/music/song.ogg",
            "Pack/img/a.png",
        ];
        let page = read_page(paths[0], PAGE, Some("utf-8"), &paths)
            .unwrap()
            .unwrap();
        assert_eq!(
            page.external,
            Some(("Pack/danoni/dos/more.txt".into(), Some("utf-8".into())))
        );
        let ext = "function externalDosInit() { g_externalDos = `|down_data=350|`; }";
        let w = page.finish(Some(ext)).unwrap();
        let ws = WorkSource::Page(paths[0].into());
        let mut seen = HashMap::new();
        let p = plan(&ws, paths[0], &w, &paths, "Loose", &mut seen);
        assert_eq!(p.songs.len(), 1, "{:?}", p.skipped);
        let s = &p.songs[0];
        assert_eq!(s.pack, "Pack");
        assert_eq!(s.id, pack::song_id("Pack", "danoni/work.html#0"));
        assert_eq!(
            (s.music.as_str(), s.encoded),
            ("Pack/music/song.ogg", false)
        );
        assert_eq!(s.layers, [("../img/a.png".into(), "Pack/img/a.png".into())]);
        assert!(p.warnings.iter().any(|(_, w)| w.contains("gone.png")));
        // The external dos's notes are the song's too.
        assert_eq!(s.song.charts[0].notes.len(), 3);
        let e = entry(s, "chart.danoni", "music.ogg");
        assert_eq!(e.layer_images, ["../img/a.png"]);
        assert!(e.charts[0].digest.is_some() && e.charts[0].excessive.is_some());
        // The same work again in the import: skipped as a duplicate.
        let again = plan(&ws, paths[0], &w, &paths, "Loose", &mut seen);
        assert!(again.songs.is_empty() && !again.skipped.is_empty());
        assert_eq!(song_of(&w, 0).unwrap().title, s.song.title);
    }
}
