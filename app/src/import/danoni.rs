//! Importing Dancing☆Onigiri works: pages (with their external dos and
//! music) and dumps. The finding and resolving is `ddi_library::danoni`;
//! this reads the files, decodes text in the charset the work declares and
//! stores one song per music file.

use std::collections::{HashMap, HashSet};

use ddi_chart::Song;
use ddi_chart::formats::danoni::{self, Dos, source};
use ddi_library::danoni::{self as work, SharedWork, StoredWork, WorkSource};
use ddi_library::manifest::{EntryMeta, summarize};
use ddi_library::pack;
use wasm_bindgen::JsCast;
use web_sys::Blob;

use super::{ImportReport, Source, mime_type, read_blob, read_bytes, source};
use crate::songs::{ImportedSong, file_key};
use crate::web::files::bytes_blob;
use crate::web::idb::{self, Db, Write};

/// What the works of an import left behind for the rest of the report.
#[derive(Default)]
pub(super) struct WorksOutcome {
    /// Music files the works used (their folders are not "music without a
    /// simfile").
    pub(super) music: HashSet<String>,
    /// Whether any page or dump was a work at all.
    pub(super) found: bool,
    /// Ids of the songs stored.
    pub(super) ids: Vec<String>,
}

/// Imports every work among `paths` into the library.
pub(super) async fn import_works(
    db: &Db,
    sources: &HashMap<String, Source>,
    paths: &[String],
    fallback_pack: &str,
    report: &mut ImportReport,
    progress: &yew::Callback<String>,
    origin: Option<&super::Origin>,
) -> WorksOutcome {
    let folder = origin.map(|o| o.folder.as_str());
    let link = origin.is_some_and(|o| o.link);
    let mut out = WorksOutcome::default();
    let mut seen: HashMap<String, String> = HashMap::new();
    for ws in work::work_sources(paths) {
        let (label, result) = match &ws {
            WorkSource::Page(page) => (page.clone(), read_page(sources, paths, page).await),
            WorkSource::Dump(manifest) => {
                (manifest.clone(), read_dump(sources, paths, manifest).await)
            }
        };
        let work = match result {
            Ok(Some(w)) => w,
            Ok(None) => continue,
            Err(e) => {
                out.found = true;
                report.skipped.push((label, e));
                continue;
            }
        };
        out.found = true;
        progress.emit(format!("importing {label}…"));
        let import = match danoni::import(&work.dos) {
            Ok(i) => i,
            Err(e) => {
                report.skipped.push((label, e));
                continue;
            }
        };
        report
            .warnings
            .extend(import.warnings.iter().map(|w| (label.clone(), w.clone())));
        let pack = pack_name(&label, fallback_pack);
        // A song's id comes from its pack and a name like a song folder's:
        // the page's file name, or the dump's folder.
        // The page's path inside the pack (or the dump's folder), so
        // `a/index.html` and `b/index.html` stay apart.
        let ident = match &ws {
            WorkSource::Page(page) => inside_pack(page),
            WorkSource::Dump(manifest) => inside_pack(parent(manifest)),
        };
        // The work's fields are stored once, with its first stored song.
        let shared = match SharedWork::new(&work.dos).stored() {
            Ok((key, json)) => (free_work_key(db, key, &json).await, json),
            Err(e) => {
                report.skipped.push((label, e));
                continue;
            }
        };
        let mut shared_stored = false;
        let mut uncaptured: Vec<&str> = Vec::new();
        for (k, song) in import.songs.iter().enumerate() {
            let music_no = import.music[k];
            let song_label = format!("{label} ({})", song.title);
            let music = match &work.music {
                Music::Page(page) => song
                    .music
                    .as_deref()
                    .and_then(|url| work::music_path(page, &work.dos, url, paths))
                    .map(|p| {
                        let encoded = work::is_encoded_music(&p);
                        (p, encoded)
                    })
                    .ok_or_else(|| match &song.music {
                        Some(url) => format!("music {url} was not found"),
                        None => "the work names no music".to_string(),
                    }),
                Music::Dump(by_chart) => {
                    let found = import
                        .charts
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| c.is_some_and(|(s, _)| s == k))
                        .find_map(|(id, _)| by_chart.get(&id).cloned());
                    let Some((found, encoded)) = found else {
                        // Dumps capture some charts of a work: one line for
                        // all the songs left without music.
                        uncaptured.push(&song.title);
                        continue;
                    };
                    Ok((found, encoded))
                }
            };
            let (music, encoded) = match music {
                Ok(m) => m,
                Err(e) => {
                    report.skipped.push((song_label, e));
                    continue;
                }
            };
            out.music.insert(music.clone());
            let id = pack::song_id(&pack, &format!("{ident}#{music_no}"));
            if let Some(first) = seen.get(&id) {
                report
                    .skipped
                    .push((song_label, format!("same pack and file name as {first}")));
                continue;
            }
            seen.insert(id.clone(), label.clone());
            let stored = StoredWork::referencing(&shared.0, music_no);
            let with_work = (!shared_stored).then_some(&shared);
            match store_song(
                db, sources, &id, &pack, &label, song, &stored, with_work, &music, encoded, folder,
                link,
            )
            .await
            {
                Ok(title) => {
                    shared_stored = true;
                    report.imported.push(title);
                    out.ids.push(id.clone());
                }
                Err(e) => report.skipped.push((song_label, e)),
            }
        }
        if !uncaptured.is_empty() {
            report.skipped.push((
                label.clone(),
                format!(
                    "{} song(s) of the work have no captured music: {}",
                    uncaptured.len(),
                    uncaptured.join(", ")
                ),
            ));
        }
    }
    out
}

/// `key`, or the first of `key-1`, `key-2`, … that is free or already
/// holds `json`: a work whose fields hash like another's must not
/// overwrite them.
async fn free_work_key(db: &Db, key: String, json: &str) -> String {
    for n in 0..16 {
        let candidate = if n == 0 {
            key.clone()
        } else {
            format!("{key}-{n}")
        };
        let stored = match db.get(idb::FILES, &candidate).await {
            Ok(Some(v)) => v.dyn_into::<Blob>().ok(),
            _ => return candidate,
        };
        let same = match stored {
            Some(blob) => crate::web::files::blob_bytes(&blob)
                .await
                .is_ok_and(|b| b == json.as_bytes()),
            None => false,
        };
        if same {
            return candidate;
        }
    }
    format!("{key}-{}", js_sys::Date::now() as u64)
}

/// Where a work's music comes from.
enum Music {
    /// Resolved from the page's `musicUrl`.
    Page(String),
    /// Captured per chart: `chartId` → (path, encoded music script).
    Dump(HashMap<usize, (String, bool)>),
}

struct Work {
    dos: Dos,
    music: Music,
}

/// A page's dos (inline, then external), or `None` for a page without one.
async fn read_page(
    sources: &HashMap<String, Source>,
    paths: &[String],
    page: &str,
) -> Result<Option<Work>, String> {
    let bytes = read_bytes(source(sources, page)?).await?;
    let charset = work::declared_charset(&bytes);
    let html = decode_with(&bytes, charset.as_deref());
    let Some(found) = source::scan_html(&html) else {
        return Ok(None);
    };
    let mut dos = Dos::default();
    if let Some(inline) = &found.inline {
        dos.merge(inline, found.flags);
    }
    if found.divided {
        return Err("splits its charts over several dos files, which is not supported yet".into());
    }
    if let Some(name) = &found.external {
        let path = work::external_path(page, name, paths)
            .ok_or_else(|| format!("its dos file {name} is missing"))?;
        let bytes = read_bytes(source(sources, &path)?).await?;
        let text = decode_with(
            &bytes,
            found.external_charset.as_deref().or(charset.as_deref()),
        );
        let dos_text = source::external_dos_text(&text).ok_or_else(|| {
            format!("{name} builds its dos with a script, which an import never runs")
        })?;
        dos.merge(&dos_text, found.flags);
    }
    Ok(Some(Work {
        dos,
        music: Music::Page(page.to_string()),
    }))
}

/// A dump's effective dos and captured music, or `None` for another
/// `manifest.json`.
async fn read_dump(
    sources: &HashMap<String, Source>,
    paths: &[String],
    manifest: &str,
) -> Result<Option<Work>, String> {
    let text = pack::decode_text(&read_bytes(source(sources, manifest)?).await?);
    let Some(plan) = work::dump_plan(manifest, &text, paths)? else {
        return Ok(None);
    };
    let eff = pack::decode_text(&read_bytes(source(sources, &plan.effective)?).await?);
    Ok(Some(Work {
        dos: work::dump_dos(&eff)?,
        music: Music::Dump(
            plan.music
                .into_iter()
                .map(|(id, path, encoded)| (id, (path, encoded)))
                .collect(),
        ),
    }))
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

/// Stores one song of a work: the work file and its music.
#[allow(clippy::too_many_arguments)]
async fn store_song(
    db: &Db,
    sources: &HashMap<String, Source>,
    id: &str,
    pack: &str,
    dir: &str,
    song: &Song,
    stored: &StoredWork,
    shared: Option<&(String, String)>,
    music: &str,
    encoded: bool,
    folder: Option<&str>,
    link: bool,
) -> Result<String, String> {
    let link = link && folder.is_some();
    // Linked (a kept folder): the music stays there, unless it is a script
    // that has to be decoded.
    let mut links = std::collections::BTreeMap::new();
    let music_link = source(sources, music)?
        .link(music)
        .filter(|_| link && !encoded);
    let (music_blob, music_ext) = if let Some(music_link) = music_link {
        let ext = match mime_type(music) {
            "" => sniff_audio(&read_head_bytes(sources, music).await?)
                .0
                .to_string(),
            _ => super::extension(music).unwrap_or_else(|| "mp3".into()),
        };
        links.insert(format!("music.{ext}"), music_link);
        (None, ext)
    } else if encoded {
        let js = pack::decode_text(&read_bytes(source(sources, music)?).await?);
        let bytes = work::decode_music(&js).ok_or("its music script holds no audio")?;
        let (ext, mime) = sniff_audio(&bytes);
        (Some(bytes_blob(&bytes, mime)?), ext.to_string())
    } else {
        // A file without an audio extension (a dump's blob) is named
        // by what it holds.
        let (ext, mime) = match mime_type(music) {
            "" => {
                let (ext, mime) = sniff_audio(&read_head_bytes(sources, music).await?);
                (ext.to_string(), mime)
            }
            m => (super::extension(music).unwrap_or_else(|| "mp3".into()), m),
        };
        (Some(read_blob(source(sources, music)?, mime).await?), ext)
    };
    let chart_name = format!("chart.{}", work::STORED_EXT);
    let music_name = format!("music.{music_ext}");
    let title = song.title.clone();
    let mut entry = summarize(
        EntryMeta {
            id: id.to_string(),
            title: title.clone(),
            artist: song.artist.clone(),
            chart: chart_name.clone(),
            music: music_name.clone(),
            banner: None,
            background: None,
            bg_images: Vec::new(),
            bg_shared: Vec::new(),
            credit: song.credit.trim().to_string(),
        },
        song,
    );
    entry.preview_start = 0.0;
    entry.preview_length = 0.0;
    let json = serde_json::to_string(stored).map_err(|e| e.to_string())?;
    let mut files: Vec<(String, Blob)> = vec![(
        chart_name,
        bytes_blob(json.as_bytes(), "application/json;charset=utf-8")?,
    )];
    if let Some(blob) = music_blob {
        files.push((music_name, blob));
    }
    let record = ImportedSong {
        entry,
        pack: pack.to_string(),
        dir: dir.to_string(),
        imported: js_sys::Date::now(),
        bytes: files.iter().map(|(_, b)| b.size()).sum(),
        work: stored.work.clone(),
        folder: folder.map(str::to_string),
        links,
    };
    let record = serde_json::to_string(&record).map_err(|e| e.to_string())?;
    let mut writes = vec![Write::DeletePrefix {
        store: idb::FILES,
        prefix: file_key(id, ""),
    }];
    writes.extend(files.into_iter().map(|(name, blob)| Write::Put {
        store: idb::FILES,
        key: file_key(id, &name),
        value: blob.into(),
    }));
    if let Some((key, json)) = shared {
        writes.push(Write::Put {
            store: idb::FILES,
            key: key.clone(),
            value: bytes_blob(json.as_bytes(), "application/json;charset=utf-8")?.into(),
        });
    }
    writes.push(Write::Put {
        store: idb::SONGS,
        key: id.to_string(),
        value: record.into(),
    });
    db.write(writes).await?;
    Ok(title)
}

async fn read_head_bytes(sources: &HashMap<String, Source>, path: &str) -> Result<Vec<u8>, String> {
    super::read_head(source(sources, path)?, 16).await
}

/// Extension and content type of audio bytes by their signature (an
/// encoded music script does not say what it holds).
fn sniff_audio(bytes: &[u8]) -> (&'static str, &'static str) {
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

/// Text in the charset a page declares, decoded by the browser (Shift_JIS
/// and the other legacy encodings Japanese pages use). Without a
/// declaration: UTF-8 when the bytes are valid UTF-8, else Shift_JIS.
fn decode_with(bytes: &[u8], label: Option<&str>) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let label = label.map(str::to_ascii_lowercase);
    match label.as_deref() {
        // As a browser would: a bad byte becomes U+FFFD, the rest stays.
        Some("utf-8" | "utf8") => String::from_utf8_lossy(bytes).into_owned(),
        None => match std::str::from_utf8(bytes) {
            Ok(s) => s.to_string(),
            Err(_) => browser_decode(bytes, "shift_jis"),
        },
        Some(l) => browser_decode(bytes, l),
    }
}

/// Decodes with the browser's `TextDecoder`; Windows-1252 if it refuses.
fn browser_decode(bytes: &[u8], label: &str) -> String {
    web_sys::TextDecoder::new_with_label(label)
        .and_then(|d| d.decode_with_buffer_source(&js_sys::Uint8Array::from(bytes)))
        .unwrap_or_else(|_| pack::decode_text(bytes))
}
