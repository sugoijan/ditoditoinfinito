//! A song's scripted backgrounds and foregrounds on the web
//! (`docs/plans/scripted-backgrounds.md`): the files its import kept
//! (`bga/<path>`, [`ddi_library::manifest::ManifestEntry::bg_scripts`])
//! read from the library, their images decoded by the browser, their Lua
//! loaded (`ddi_session::scripted`), for the game loop to run and draw
//! through the shared view (`ddi_shell::scripted`).

use ddi_library::backgrounds;
use ddi_library::manifest::ManifestEntry;
use ddi_session::backgrounds::MAX_CHANGE_SIDE;
use ddi_session::scripted::{FileKind, Scripts, Sizes, SongFiles};
use ddi_ui::state::settings::BackgroundFrame;

use crate::songs::ImportedSong;
use crate::stage::Bitmap;

/// The scripts in a play: the view over them and the images they load.
pub(crate) type ScriptView = ddi_shell::scripted::ScriptView<Bitmap>;

/// A song's scripts loaded: the runtime, StepMania's screen they were
/// told, and their images by path relative to the song's folder.
pub(crate) struct Loaded {
    pub(crate) scripts: Scripts,
    pub(crate) screen: [f64; 2],
    pub(crate) images: Vec<(String, Bitmap)>,
    /// Their sounds decoded (Ogg Vorbis and MP3, as the music's fallback
    /// decodes them), by file relative to the song's folder.
    pub(crate) sounds: Vec<(String, ddi_platform::decode::DecodedAudio)>,
}

/// Reads `entry`'s scripts and loads them for `song` (as stored: the
/// play retimes the foregrounds by its song offset), told StepMania's
/// 4:3 screen under a 4:3 background `frame`, its 16:9 one otherwise.
/// `None` without scripts; an error when one fails to load (the song
/// background shows in their place).
pub(crate) async fn load(
    entry: &ManifestEntry,
    song: &ddi_chart::Song,
    record: Option<&ImportedSong>,
    frame: BackgroundFrame,
) -> Result<Option<Loaded>, String> {
    if entry.bg_scripts.is_empty() {
        return Ok(None);
    }
    let (pack, name) = match record {
        Some(r) => {
            let dir = ddi_library::path::RelPath::new(&r.dir);
            (r.pack.clone(), dir.file_name().to_string())
        }
        None => (String::new(), entry.id.clone()),
    };
    let mut files = SongFiles::new(pack, name, &entry.bg_scripts);
    let mut sizes = Sizes::default();
    let mut images = Vec::new();
    for f in &entry.bg_scripts {
        let kind = FileKind::of(f.as_str());
        if kind == FileKind::Other {
            continue;
        }
        let blob = match crate::songs::load_script_file(entry, f.as_str(), record).await {
            Ok(b) => b,
            Err(e) => {
                web_sys::console::warn_1(&format!("scripted backgrounds: {f}: {e}").into());
                continue;
            }
        };
        if kind == FileKind::Image {
            match crate::web::image::decode_sized(&blob, MAX_CHANGE_SIDE).await {
                Ok((bitmap, own)) => {
                    sizes.insert(f.as_str(), own);
                    images.push((f.to_string(), Bitmap(bitmap)));
                }
                Err(e) => web_sys::console::warn_1(&format!("{f}: {e}").into()),
            }
        } else {
            let text = wasm_bindgen_futures::JsFuture::from(blob.text())
                .await
                .ok()
                .and_then(|t| t.as_string())
                .unwrap_or_default();
            files.texts.push((f.to_string(), text));
        }
    }
    let folders = backgrounds::script_folders_of(&entry.bg_scripts);
    let none: &[&str] = &[];
    let layers = backgrounds::layers(song, none, none, &folders, none, false);
    let foregrounds = backgrounds::foregrounds(song, &folders);
    let env = ddi_session::scripted::env(song, ddi_session::scripted::frame_aspect(frame));
    let screen = env.screen;
    let clock = crate::noteskins::clock();
    // In slices, the page drawing between them (the start prompt).
    let mut pause = crate::noteskins::pause;
    let scripts = Scripts::load_sliced(
        files,
        env,
        &layers,
        &foregrounds,
        &sizes,
        clock,
        Some(&mut pause),
    )
    .await?;
    let Some(scripts) = scripts else {
        return Ok(None);
    };
    for note in scripts.notes() {
        web_sys::console::info_1(&format!("scripted backgrounds: {note}").into());
    }
    let mut sounds = Vec::new();
    for f in scripts.sound_files() {
        let decoded = match crate::songs::load_script_file(entry, &f, record).await {
            Ok(blob) => crate::web::files::blob_bytes(&blob)
                .await
                .and_then(|bytes| {
                    ddi_platform::decode::decode(&bytes).map_err(|e| format!("{e:?}"))
                }),
            Err(e) => Err(e),
        };
        match decoded {
            Ok(audio) => sounds.push((f, audio)),
            Err(e) => web_sys::console::warn_1(&format!("scripted backgrounds: {f}: {e}").into()),
        }
    }
    Ok(Some(Loaded {
        scripts,
        screen,
        images,
        sounds,
    }))
}
