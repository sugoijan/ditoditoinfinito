//! Imported noteskins: their records in IndexedDB, and readying the chosen
//! one for a play (its Lua run, its images decoded, its actors built for
//! the layout). The skin logic is `ddi_noteskin`'s; drawing is
//! `ddi_render::noteskin`'s.
//!
//! A skin's Lua runs in slices with the page's turn between them. If it
//! stops (its budget, memory, a host limit), at a load or in a play, that
//! play draws the skin without it (natively, or the built-in skin for the
//! rest of a play); the next play tries again, and a second stop turns
//! its Lua off until the skin is imported again (`docs/plans/lua.md`).

use std::cell::RefCell;
use std::collections::HashMap;

use std::future::Future;
use std::pin::Pin;

use ddi_chart::Layout;
use ddi_noteskin::build::{Images, TextureKey};
use ddi_noteskin::lua::{Env, LoadBudget, LuaSkin};
use ddi_noteskin::{Dir, Folder, Skin};
use ddi_render::noteskin::NoteskinPlay;
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Blob, ImageBitmap};

use crate::web::idb::{self, Db, Write};

/// An imported skin folder, as stored.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct StoredSkin {
    pub(crate) game: String,
    pub(crate) name: String,
    /// Every file, relative to the skin folder.
    pub(crate) files: Vec<String>,
    /// `metrics.ini`, `.redir` and `.lua` texts (`.lua` since skins' Lua
    /// runs; older records get theirs from the stored files).
    pub(crate) texts: Vec<(String, String)>,
    /// Where it was imported from (the folder or zip path).
    #[serde(default)]
    pub(crate) origin: String,
    /// Files kept in storage (images, Lua, models), by path.
    #[serde(default)]
    pub(crate) stored: Vec<String>,
    /// Why the skin's Lua is off: it stopped in two plays. Importing the
    /// skin again clears it.
    #[serde(default)]
    pub(crate) lua_broken: Option<String>,
}

impl StoredSkin {
    /// The store key, `<game>/<name>` (names match ignoring case, as
    /// StepMania's do).
    pub(crate) fn key(&self) -> String {
        skin_key(&self.game, &self.name)
    }

    fn folder(&self) -> Folder {
        Folder {
            game: self.game.clone(),
            name: self.name.clone(),
            files: self.files.clone(),
            texts: self.texts.clone(),
        }
    }
}

pub(crate) fn skin_key(game: &str, name: &str) -> String {
    format!(
        "{}/{}",
        game.to_ascii_lowercase(),
        name.to_ascii_lowercase()
    )
}

/// Key of a skin's file in the `files` store.
pub(crate) fn file_key(skin: &str, path: &str) -> String {
    format!("noteskin/{skin}/{path}")
}

/// Images and scripts a skin keeps, at most; larger skins are cut and
/// reported (its `metrics.ini` and redirects are always kept).
pub(crate) const MAX_FILES: usize = 2000;
/// Skins one import keeps, at most.
pub(crate) const MAX_SKINS: usize = 200;
/// The largest `metrics.ini`, `.redir` or `.lua` read, in bytes.
pub(crate) const MAX_TEXT: u64 = 1 << 20;

/// Whether a skin's file is kept as text with its record.
pub(crate) fn kept_text(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower == "metrics.ini" || lower.ends_with(".redir") || lower.ends_with(".lua")
}

/// Whether a skin's file is worth storing: images, Lua and models (for the
/// steps that read them), the texts.
pub(crate) fn kept_file(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [
        ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".lua", ".txt", ".ini", ".redir",
    ]
    .iter()
    .any(|e| lower.ends_with(e))
}

/// Every imported skin.
pub(crate) async fn all(db: &Db) -> Result<Vec<StoredSkin>, String> {
    Ok(db
        .entries(idb::NOTESKINS)
        .await?
        .into_iter()
        .filter_map(|(_, v)| serde_json::from_str(&v.as_string()?).ok())
        .collect())
}

/// The writes that store `skin` (replacing one of the same name).
pub(crate) fn store(skin: &StoredSkin, files: Vec<(String, Blob)>) -> Result<Vec<Write>, String> {
    let key = skin.key();
    let mut writes = vec![Write::DeletePrefix {
        store: idb::FILES,
        prefix: file_key(&key, ""),
    }];
    for (path, blob) in files {
        writes.push(Write::Put {
            store: idb::FILES,
            key: file_key(&key, &path),
            value: blob.into(),
        });
    }
    let json = serde_json::to_string(skin).map_err(|e| e.to_string())?;
    writes.push(Write::Put {
        store: idb::NOTESKINS,
        key,
        value: JsValue::from_str(&json),
    });
    Ok(writes)
}

/// Removes an imported skin and its files.
pub(crate) async fn delete(key: &str) -> Result<(), String> {
    let db = Db::open().await?;
    db.write(vec![
        Write::Delete {
            store: idb::NOTESKINS,
            key: key.to_string(),
        },
        Write::DeletePrefix {
            store: idb::FILES,
            prefix: file_key(key, ""),
        },
    ])
    .await
}

/// The dance skins one can pick: `(key, name)`.
pub(crate) async fn choices() -> Result<Vec<(String, String)>, String> {
    let db = Db::open().await?;
    let mut skins: Vec<(String, String)> = all(&db)
        .await?
        .into_iter()
        .filter(|s| s.game == "dance" && !s.name.eq_ignore_ascii_case("default"))
        .map(|s| {
            let name = match &s.lua_broken {
                Some(_) => format!("{} (its Lua is off)", s.name),
                None => s.name.clone(),
            };
            (s.key(), name)
        })
        .collect();
    skins.sort_by_key(|(_, n)| n.to_lowercase());
    Ok(skins)
}

/// What each element of a skin is, in a line for the import report: how
/// many of a column's elements are the skin's own and why the rest are
/// the built-in skin's, as its Lua builds them when it has Lua, else as
/// they resolve without it.
pub(crate) async fn summary(folders: &[Folder], game: &str, name: &str) -> String {
    let skin = match Skin::load(folders, game, name) {
        Ok(s) => s,
        Err(e) => return e.to_string(),
    };
    if !has_lua(&skin) {
        return native_summary(&skin);
    }
    let probe = Sizes::probe();
    let layout = Layout::dance_single();
    match lua_play(
        folders,
        game,
        name,
        Env::default(),
        hash_seed(),
        &layout,
        &probe,
    )
    .await
    {
        Ok(Some(play)) => {
            let total = ddi_noteskin::load::ENGINE_ELEMENTS.len();
            let own = play.own_elements(1);
            let mut line = format!("its Lua ran: {own} of {total} elements are its own");
            if own < total {
                // Why the first few are not (a model, an unsupported actor).
                let why: Vec<&str> = play
                    .notes
                    .iter()
                    .filter(|n| n.starts_with("Down: "))
                    .map(|n| n.trim_start_matches("Down: "))
                    .take(3)
                    .collect();
                line.push_str(&format!("; {} drawn built in", total - own));
                if !why.is_empty() {
                    line.push_str(&format!(" ({})", why.join("; ")));
                }
            }
            line
        }
        Ok(None) => "its lanes are not dance buttons".to_string(),
        Err(e) => format!(
            "its Lua stopped ({e}); it is drawn without it: {}",
            native_summary(&skin)
        ),
    }
}

/// [`summary`]'s line for a skin drawn without its Lua.
fn native_summary(skin: &Skin<'_>) -> String {
    use ddi_noteskin::load::ENGINE_ELEMENTS;
    use ddi_noteskin::{Resolved, Source};
    let (mut own, mut lua, mut model, mut other, mut builtin) = (0, 0, 0, 0, 0);
    for e in ENGINE_ELEMENTS {
        match skin.resolve(ddi_noteskin::Button::Down, e) {
            Resolved::File {
                source: Source::Image(_),
                ..
            }
            | Resolved::Blank => own += 1,
            Resolved::File {
                source: Source::Lua(_),
                ..
            } => lua += 1,
            Resolved::File {
                source: Source::Model(_),
                ..
            } => model += 1,
            Resolved::File {
                source: Source::BuiltInArt(_) | Source::Native(..),
                ..
            }
            | Resolved::Missing(_) => builtin += 1,
            _ => other += 1,
        }
    }
    let mut parts = vec![format!(
        "{own} of {} elements are its own images",
        ENGINE_ELEMENTS.len()
    )];
    if lua > 0 {
        parts.push(format!("{lua} are Lua (drawn built in)"));
    }
    if model > 0 {
        parts.push(format!("{model} are 3D models (drawn built in)"));
    }
    if builtin > 0 {
        parts.push(format!(
            "{builtin} come from StepMania's own skins (drawn built in)"
        ));
    }
    if other > 0 {
        parts.push(format!("{other} cannot be drawn"));
    }
    parts.join("; ")
}

/// The page's turn between slices of a skin's Lua.
fn pause() -> Pin<Box<dyn Future<Output = ()>>> {
    Box::pin(gloo::timers::future::TimeoutFuture::new(0))
}

/// The host's clock for the Lua's deadlines, in seconds.
fn clock() -> Box<dyn FnMut() -> f64> {
    let performance = web_sys::window().and_then(|w| w.performance());
    Box::new(move || performance.as_ref().map_or(0.0, |p| p.now() / 1000.0))
}

/// Starts a skin's Lua: its own VM with hash seed `seed` (the two passes
/// of a play share one, so tables iterate alike), the prelude and the
/// `NoteSkin.lua` chain.
async fn lua_skin(skin: Skin<'static>, env: Env, seed: u64) -> Result<LuaSkin, ddi_lua::Failure> {
    let settings = ddi_lua::Settings {
        hash_seed: seed,
        ..ddi_lua::Settings::default()
    };
    LuaSkin::new_sliced(
        skin,
        env,
        settings,
        LoadBudget::default(),
        clock(),
        Some(&mut pause),
    )
    .await
}

/// What a skin's Lua sees of the chart it is loaded for.
pub(crate) fn lua_env(song: &ddi_chart::Song, chart: &ddi_chart::Chart, layout: &Layout) -> Env {
    use ddi_chart::Difficulty;
    let style = match layout.id.as_str() {
        "dance-double" => "double",
        "dance-solo" => "solo",
        _ => "single",
    };
    let difficulty = match chart.difficulty {
        Difficulty::Beginner => "Beginner",
        Difficulty::Easy => "Easy",
        Difficulty::Medium => "Medium",
        Difficulty::Hard => "Hard",
        Difficulty::Challenge => "Challenge",
        Difficulty::Edit => "Edit",
    };
    let starts = || chart.notes.iter().map(|n| n.tick.beat());
    // Holds and rolls last to their ends.
    let ends = chart.notes.iter().map(|n| match n.kind {
        ddi_chart::NoteKind::HoldHead { end } | ddi_chart::NoteKind::RollHead { end } => end.beat(),
        _ => n.tick.beat(),
    });
    Env {
        style: style.into(),
        columns: layout.lanes.len() as u32,
        difficulty: format!("Difficulty_{difficulty}"),
        timing: chart.timing(song).clone(),
        first_beat: starts().reduce(f64::min).unwrap_or(0.0),
        last_beat: ends.reduce(f64::max).unwrap_or(0.0),
        ..Env::default()
    }
}

/// A skin ready for a play: its actors and the decoded images its
/// textures stand for (`None` for the built-in art).
pub(crate) struct Prepared {
    /// The skin's store key.
    pub(crate) key: String,
    pub(crate) play: NoteskinPlay,
    pub(crate) images: Vec<Option<ImageBitmap>>,
}

impl Prepared {
    /// Frees the decoded images (a skin readied but never shown).
    pub(crate) fn close(self) {
        for b in self.images.into_iter().flatten() {
            b.close();
        }
    }
}

/// Image sizes as decoded (original pixels), by `(folder, path)`.
struct Sizes {
    sizes: HashMap<(usize, String), (u32, u32)>,
    max_size: u32,
    /// The images asked for (the first pass answers every size 128).
    asked: RefCell<Vec<(usize, String)>>,
    probe: bool,
}

impl Sizes {
    /// The first pass: every image 128 pixels square, the ones asked for
    /// noted.
    fn probe() -> Sizes {
        Sizes {
            sizes: HashMap::new(),
            max_size: MAX_IMAGE_SIDE,
            asked: RefCell::new(Vec::new()),
            probe: true,
        }
    }
}

impl Images for Sizes {
    fn size(&self, dir: Dir, path: &str) -> Option<(u32, u32)> {
        let Dir::Folder(i) = dir else {
            return None;
        };
        if self.probe {
            self.asked.borrow_mut().push((i, path.to_string()));
            return Some((128, 128));
        }
        self.sizes.get(&(i, path.to_string())).copied()
    }

    fn max_size(&self) -> u32 {
        self.max_size
    }
}

/// The largest image side kept (GPU limits and memory).
const MAX_IMAGE_SIDE: u32 = 4096;

/// Decoded pixels a skin may hold at once (each is kept as a bitmap and
/// again on the GPU): about 256 MB of each. Images past it are left out
/// and drawn by the built-in skin.
const MAX_PIXELS: u64 = 64 << 20;

/// Builds skin `name` through its Lua for `layout`; why it could not.
async fn lua_play(
    folders: &[Folder],
    game: &str,
    name: &str,
    env: Env,
    seed: u64,
    layout: &Layout,
    images: &dyn Images,
) -> Result<Option<NoteskinPlay>, String> {
    if layout
        .lanes
        .iter()
        .any(|l| ddi_noteskin::Button::from_lane(&l.name).is_none())
    {
        return Ok(None);
    }
    let loaded = Skin::load(folders, game, name).map_err(|e| e.to_string())?;
    let lua = lua_skin(loaded.into_owned(), env, seed)
        .await
        .map_err(|e| e.to_string())?;
    NoteskinPlay::with_lua(lua, layout, images, true, Some(&mut pause))
        .await
        .map_err(|e| e.to_string())
}

/// Fills in the `.lua` texts of a skin stored before they were kept with
/// its record, from its stored files, and saves the record.
pub(crate) async fn add_lua_texts(db: &Db, skin: &mut StoredSkin) {
    let missing: Vec<String> = skin
        .stored
        .iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".lua"))
        .filter(|p| !skin.texts.iter().any(|(t, _)| t == *p))
        .cloned()
        .collect();
    if missing.is_empty() {
        return;
    }
    let key = skin.key();
    let before = skin.texts.len();
    for path in missing {
        let blob = db
            .get(idb::FILES, &file_key(&key, &path))
            .await
            .ok()
            .flatten()
            .and_then(|v| v.dyn_into::<Blob>().ok());
        let Some(blob) = blob.filter(|b| b.size() <= MAX_TEXT as f64) else {
            continue;
        };
        if let Ok(text) = wasm_bindgen_futures::JsFuture::from(blob.text()).await
            && let Some(text) = text.as_string()
        {
            skin.texts.push((path, text));
        }
    }
    if skin.texts.len() == before {
        return;
    }
    if let Ok(json) = serde_json::to_string(skin) {
        let _ = db
            .write(vec![Write::Put {
                store: idb::NOTESKINS,
                key,
                value: JsValue::from_str(&json),
            }])
            .await;
    }
}

thread_local! {
    /// Skins whose Lua stopped once in this session.
    static STOPPED_ONCE: RefCell<std::collections::HashSet<String>> = RefCell::default();
}

/// Skin `key`'s Lua stopped (at a load or in a play) because of `why`:
/// the next play tries again; the second time its Lua is turned off and
/// the record says why.
pub(crate) async fn lua_stopped(key: &str, why: &str) {
    web_sys::console::warn_1(&format!("noteskin {key}: {why}").into());
    let again = STOPPED_ONCE.with(|s| !s.borrow_mut().insert(key.to_string()));
    if !again {
        return;
    }
    let Ok(db) = Db::open().await else {
        return;
    };
    let Ok(stored) = all(&db).await else {
        return;
    };
    let Some(mut skin) = stored.into_iter().find(|s| s.key() == key) else {
        return;
    };
    skin.lua_broken = Some(why.to_string());
    if let Ok(json) = serde_json::to_string(&skin) {
        let _ = db
            .write(vec![Write::Put {
                store: idb::NOTESKINS,
                key: key.to_string(),
                value: JsValue::from_str(&json),
            }])
            .await;
    }
}

/// Decodes the images `sizes` noted in its probe that are not decoded
/// yet, up to [`MAX_PIXELS`] in all; then `sizes` answers with theirs.
async fn decode(
    db: &Db,
    stored: &[StoredSkin],
    sizes: &mut Sizes,
    bitmaps: &mut HashMap<(usize, String), ImageBitmap>,
    pixels: &mut u64,
) {
    let mut asked = sizes.asked.take();
    asked.sort();
    asked.dedup();
    for (i, path) in asked {
        if bitmaps.contains_key(&(i, path.clone())) {
            continue;
        }
        let Some(skin) = stored.get(i) else {
            continue;
        };
        if *pixels >= MAX_PIXELS {
            web_sys::console::warn_1(
                &format!(
                    "noteskin {}: {path} left out: the skin's images are too large together",
                    skin.name
                )
                .into(),
            );
            continue;
        }
        let blob = db
            .get(idb::FILES, &file_key(&skin.key(), &path))
            .await
            .ok()
            .flatten()
            .and_then(|v| v.dyn_into::<Blob>().ok());
        let Some(blob) = blob else {
            web_sys::console::warn_1(
                &format!("noteskin {}: {path} is not stored", skin.name).into(),
            );
            continue;
        };
        match crate::web::image::decode_sized(&blob, MAX_IMAGE_SIDE).await {
            Ok((bitmap, size)) => {
                *pixels += u64::from(bitmap.width()) * u64::from(bitmap.height());
                sizes.sizes.insert((i, path.clone()), size);
                bitmaps.insert((i, path), bitmap);
            }
            Err(e) => {
                web_sys::console::warn_1(&format!("noteskin {}: {path}: {e}", skin.name).into())
            }
        }
    }
    sizes.probe = false;
}

/// Whether a skin or one of the imported skins it falls back on has Lua
/// of its own.
fn has_lua(skin: &Skin<'_>) -> bool {
    skin.chain.iter().any(|&d| {
        skin.folder(d).is_some_and(|f| {
            f.files
                .iter()
                .any(|p| p.to_ascii_lowercase().ends_with(".lua"))
        })
    })
}

/// A hash seed for a skin's VMs, from the platform's randomness.
fn hash_seed() -> u64 {
    let mut seed = [0u8; 8];
    if let Some(crypto) = web_sys::window().and_then(|w| w.crypto().ok()) {
        let _ = crypto.get_random_values_with_u8_array(&mut seed);
    }
    u64::from_le_bytes(seed)
}

/// Forgets that skin `key`'s Lua stopped in this session (it was imported
/// again).
pub(crate) fn forget_stops(key: &str) {
    STOPPED_ONCE.with(|s| s.borrow_mut().remove(key));
}

/// Readies skin `key` for `layout`, with `env` for its Lua: `None` when
/// there is no such skin, it does not load, or the layout's lanes are not
/// dance buttons.
pub(crate) async fn prepare(key: &str, layout: &Layout, env: Env) -> Option<Prepared> {
    let db = Db::open().await.ok()?;
    let mut stored = all(&db).await.ok()?;
    let chosen = stored.iter().position(|s| s.key() == key)?;
    // The `.lua` texts of the folders the skin falls back on.
    let chain: Vec<usize> = {
        let folders: Vec<Folder> = stored.iter().map(StoredSkin::folder).collect();
        let skin = match Skin::load(&folders, &stored[chosen].game, &stored[chosen].name) {
            Ok(s) => s,
            Err(e) => {
                web_sys::console::warn_1(&format!("noteskin {key}: {e}").into());
                return None;
            }
        };
        skin.chain
            .iter()
            .filter_map(|d| match d {
                Dir::Folder(i) => Some(*i),
                Dir::BuiltIn(_) => None,
            })
            .collect()
    };
    for i in chain {
        if let Some(s) = stored.get_mut(i) {
            add_lua_texts(&db, s).await;
        }
    }
    let stored = stored;
    let chosen = &stored[chosen];
    let folders: Vec<Folder> = stored.iter().map(StoredSkin::folder).collect();
    let skin = match Skin::load(&folders, &chosen.game, &chosen.name) {
        Ok(s) => s,
        Err(e) => {
            web_sys::console::warn_1(&format!("noteskin {}: {e}", chosen.name).into());
            return None;
        }
    };
    if let Some(why) = &chosen.lua_broken {
        web_sys::console::info_1(
            &format!("noteskin {}: its Lua is off ({why})", chosen.name).into(),
        );
    }
    // Skins with no Lua of their own (nor their fallbacks) play natively:
    // the same as StepMania's stand-ins' Lua, without its costs.
    let with_lua = chosen.lua_broken.is_none() && has_lua(&skin);
    let seed = hash_seed();
    let mut lua_error = None;
    // First pass: which images the actors use.
    let mut sizes = Sizes::probe();
    if with_lua {
        match lua_play(
            &folders,
            &chosen.game,
            &chosen.name,
            env.clone(),
            seed,
            layout,
            &sizes,
        )
        .await
        {
            Ok(Some(_)) => {}
            Ok(None) => return None,
            Err(e) => lua_error = Some(e),
        }
    }
    if !with_lua || lua_error.is_some() {
        NoteskinPlay::new(&skin, layout, &sizes, true)?;
    }
    let mut bitmaps: HashMap<(usize, String), ImageBitmap> = HashMap::new();
    let mut pixels: u64 = 0;
    decode(&db, &stored, &mut sizes, &mut bitmaps, &mut pixels).await;
    let mut play = None;
    if with_lua && lua_error.is_none() {
        match lua_play(
            &folders,
            &chosen.game,
            &chosen.name,
            env,
            seed,
            layout,
            &sizes,
        )
        .await
        {
            Ok(p) => play = p,
            Err(e) => {
                lua_error = Some(e);
                // The native build may want images the Lua did not.
                sizes.probe = true;
                let _ = NoteskinPlay::new(&skin, layout, &sizes, true);
                decode(&db, &stored, &mut sizes, &mut bitmaps, &mut pixels).await;
            }
        }
    }
    if let Some(e) = &lua_error {
        lua_stopped(
            key,
            &format!("its Lua stopped while loading ({e}); drawn without it"),
        )
        .await;
    }
    if play.is_none() && (!with_lua || lua_error.is_some()) {
        play = NoteskinPlay::new(&skin, layout, &sizes, true);
    }
    let Some(play) = play else {
        for b in bitmaps.values() {
            b.close();
        }
        return None;
    };
    let images = play
        .texture_keys()
        .iter()
        .map(|k| match k {
            TextureKey::Skin {
                dir: Dir::Folder(i),
                path,
            } => bitmaps.get(&(*i, path.clone())).cloned(),
            _ => None,
        })
        .collect();
    for (_, b) in bitmaps.iter().filter(|(k, _)| {
        !play.texture_keys().iter().any(|t| matches!(t, TextureKey::Skin { dir: Dir::Folder(i), path } if (*i, path) == (k.0, &k.1)))
    }) {
        b.close();
    }
    for n in &play.notes {
        web_sys::console::info_1(&format!("noteskin {}: {n}", chosen.name).into());
    }
    Some(Prepared {
        key: key.to_string(),
        play,
        images,
    })
}
