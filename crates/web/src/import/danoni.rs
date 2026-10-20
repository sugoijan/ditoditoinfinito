//! Importing Dancing☆Onigiri works: pages (with their external dos and
//! music) and dumps. The finding and resolving is `ddi_library::danoni`;
//! this reads the files, decodes text in the charset the work declares and
//! stores one song per music file.

use std::collections::{HashMap, HashSet};

use ddi_library::danoni::{self as work, StoredWork, WorkSource};
use ddi_library::pack;
use ddi_library::works::{self, Work, WorkSong};
use wasm_bindgen::JsCast;
use web_sys::Blob;

use super::{
    ImportReport, Source, link_or_copy, mime_type, read_blob, read_bytes, source, write_song,
};
use crate::songs::ImportedSong;
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

/// Imports every work among `paths` into the library: each read here,
/// its songs found by the library's core (`ddi_library::works`, as
/// desktop's library finds them), stored.
pub(super) async fn import_works(
    db: &Db,
    sources: &HashMap<String, Source>,
    paths: &[String],
    fallback_pack: &str,
    report: &mut ImportReport,
    progress: &crate::component::Callback<String>,
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
        let w = match result {
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
        let plan = works::plan(&ws, &label, &w, paths, fallback_pack, &mut seen);
        report.warnings.extend(plan.warnings);
        report.skipped.extend(plan.skipped);
        out.music.extend(plan.music);
        if plan.songs.is_empty() {
            continue;
        }
        // The work's fields are stored once, with its first stored song.
        let shared = match works::shared(&w).stored() {
            Ok((key, json)) => (free_work_key(db, key, &json).await, json),
            Err(e) => {
                report.skipped.push((label, e));
                continue;
            }
        };
        let mut shared_stored = false;
        for s in &plan.songs {
            let stored = StoredWork::referencing(&shared.0, s.music_no);
            let with_work = (!shared_stored).then_some(&shared);
            match store_song(db, sources, s, &stored, with_work, folder, link).await {
                Ok(title) => {
                    shared_stored = true;
                    report.imported.push(title);
                    out.ids.push(s.id.clone());
                }
                Err(e) => report
                    .skipped
                    .push((format!("{} ({})", s.label, s.song.title), e)),
            }
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

/// A page's dos (inline, then external), or `None` for a page without one.
async fn read_page(
    sources: &HashMap<String, Source>,
    paths: &[String],
    page: &str,
) -> Result<Option<Work>, String> {
    let bytes = read_bytes(source(sources, page)?).await?;
    let charset = work::declared_charset(&bytes);
    let html = decode_with(&bytes, charset.as_deref());
    let Some(read) = works::read_page(page, &html, charset.as_deref(), paths)? else {
        return Ok(None);
    };
    let external = match &read.external {
        Some((path, charset)) => {
            let bytes = read_bytes(source(sources, path)?).await?;
            Some(decode_with(&bytes, charset.as_deref()))
        }
        None => None,
    };
    read.finish(external.as_deref()).map(Some)
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
    works::dump_work(plan, &eff).map(Some)
}

/// Stores one song of a work: the work file and its music.
async fn store_song(
    db: &Db,
    sources: &HashMap<String, Source>,
    s: &WorkSong,
    stored: &StoredWork,
    shared: Option<&(String, String)>,
    folder: Option<&str>,
    link: bool,
) -> Result<String, String> {
    let (id, pack, dir, music, encoded, layers) = (
        s.id.as_str(),
        s.pack.as_str(),
        s.label.as_str(),
        s.music.as_str(),
        s.encoded,
        s.layers.as_slice(),
    );
    let link = link && folder.is_some();
    // Linked (a kept folder): the music stays there, unless it is a script
    // that has to be decoded.
    let mut links = std::collections::BTreeMap::new();
    let music_link = source(sources, music)?
        .link(music)
        .filter(|_| link && !encoded);
    let (music_blob, music_ext) = if let Some(music_link) = music_link {
        let ext = match mime_type(music) {
            "" => works::sniff_audio(&read_head_bytes(sources, music).await?)
                .0
                .to_string(),
            _ => super::extension(music).unwrap_or_else(|| "mp3".into()),
        };
        links.insert(format!("music.{ext}"), music_link);
        (None, ext)
    } else if encoded {
        let js = pack::decode_text(&read_bytes(source(sources, music)?).await?);
        let bytes = work::decode_music(&js).ok_or("its music script holds no audio")?;
        let (ext, mime) = works::sniff_audio(&bytes);
        (Some(bytes_blob(&bytes, mime)?), ext.to_string())
    } else {
        // A file without an audio extension (a dump's blob) is named
        // by what it holds.
        let (ext, mime) = match mime_type(music) {
            "" => {
                let (ext, mime) = works::sniff_audio(&read_head_bytes(sources, music).await?);
                (ext.to_string(), mime)
            }
            m => (super::extension(music).unwrap_or_else(|| "mp3".into()), m),
        };
        (Some(read_blob(source(sources, music)?, mime).await?), ext)
    };
    let chart_name = format!("chart.{}", work::STORED_EXT);
    let music_name = format!("music.{music_ext}");
    let title = s.song.title.clone();
    let entry = works::entry(s, &chart_name, &music_name);
    let json = serde_json::to_string(stored).map_err(|e| e.to_string())?;
    let mut files: Vec<(String, Blob)> = vec![(
        chart_name,
        bytes_blob(json.as_bytes(), "application/json;charset=utf-8")?,
    )];
    if let Some(blob) = music_blob {
        files.push((music_name, blob));
    }
    for (name, path) in layers {
        let src = source(sources, path)?;
        // A broken image is not worth losing the song over.
        link_or_copy(
            src,
            path,
            format!("layer/{name}"),
            link,
            &mut links,
            &mut files,
        )
        .await;
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
        still: None,
    };
    let record = serde_json::to_string(&record).map_err(|e| e.to_string())?;
    let shared = match shared {
        Some((key, json)) => Some(Write::Put {
            store: idb::FILES,
            key: key.clone(),
            value: bytes_blob(json.as_bytes(), "application/json;charset=utf-8")?.into(),
        }),
        None => None,
    };
    write_song(db, id, files, shared, record).await?;
    Ok(title)
}

async fn read_head_bytes(sources: &HashMap<String, Source>, path: &str) -> Result<Vec<u8>, String> {
    super::read_head(source(sources, path)?, 16).await
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
