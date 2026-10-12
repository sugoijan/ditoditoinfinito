//! Noteskins: the imported ones (records in IndexedDB) and the bundled
//! ones (`noteskins/`, served with the game; `docs/plans/bundled-skins.md`),
//! and readying the chosen one for a play (its Lua run, its images
//! decoded, its actors built for the layout). The skin logic is
//! `ddi_noteskin`'s; drawing is `ddi_render::noteskin`'s.
//!
//! Imported skins come before bundled ones, so an import named like a
//! bundled skin stands in for it in fallback chains (as StepMania's own
//! folder would). A bundled skin's key starts with [`BUNDLED`].
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

/// A skin folder: imported, as stored, or bundled.
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
    /// A bundled skin's folder under `noteskins/` (never stored).
    #[serde(skip)]
    pub(crate) bundled: Option<String>,
    /// The bundled skin this one is a colour scheme of (never stored).
    #[serde(skip)]
    pub(crate) variant_of: Option<String>,
}

impl StoredSkin {
    /// The key, `<game>/<name>` (names match ignoring case, as
    /// StepMania's do), after [`BUNDLED`] for a bundled skin.
    pub(crate) fn key(&self) -> String {
        let key = skin_key(&self.game, &self.name);
        if self.bundled.is_some() {
            format!("{BUNDLED}{key}")
        } else {
            key
        }
    }

    /// See [`strike_key`](fn@strike_key).
    fn strike_key(&self) -> String {
        match (&self.bundled, &self.variant_of) {
            (Some(_), Some(base)) => format!("{BUNDLED}{}", skin_key(&self.game, base)),
            _ => self.key(),
        }
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

/// The start of a bundled skin's key.
pub(crate) const BUNDLED: &str = "bundled:";

/// The `noteskin` setting for the built-in arrows, drawn without a skin.
pub(crate) const BUILT_IN: &str = "builtin";

/// Dito's key (its colour variants' are `bundled:dance/dito <scheme>`).
const DITO: &str = "bundled:dance/dito";

/// The skin a play draws the dance layouts with, by key: the chosen one,
/// or Dito (the built-in arrows as a skin, when none is chosen) in its
/// variant for the note colours; none for the built-in arrows, and none
/// instead of Dito when the chart's own colours are drawn (a skin colours
/// notes by their place in the beat, not one by one).
pub(crate) fn play_skin(
    choice: Option<&str>,
    colors: crate::settings::NoteColors,
    chart_colors: bool,
) -> Option<String> {
    use crate::settings::NoteColors;
    match choice {
        Some(BUILT_IN) => None,
        Some(key) => Some(key.to_string()),
        None if chart_colors => None,
        None => Some(
            match colors {
                NoteColors::Quantized => DITO,
                NoteColors::Vivid => "bundled:dance/dito vivid",
                NoteColors::Flat => "bundled:dance/dito flat",
                NoteColors::Rainbow => "bundled:dance/dito rainbow",
                NoteColors::Single => "bundled:dance/dito single",
            }
            .to_string(),
        ),
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

pub(crate) use ddi_noteskin::import::kept_text;

/// Whether a skin's file is worth storing: images, Lua and models (for the
/// steps that read them), the texts.
pub(crate) fn kept_file(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [
        ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".svg", ".lua", ".txt", ".ini", ".redir",
    ]
    .iter()
    .any(|e| lower.ends_with(e))
}

/// The bundled skins, from `noteskins/index.json` (fetched once; a
/// failure is not kept, the next call tries again).
pub(crate) async fn bundled() -> Result<Vec<StoredSkin>, String> {
    if let Some(skins) = BUNDLED_SKINS.with(|b| b.borrow().clone()) {
        return Ok(with_session_stops(skins));
    }
    let index = match crate::songs::fetch_text("noteskins/index.json").await {
        Ok(text) => serde_json::from_str::<ddi_noteskin::bundled::Index>(&text)
            .map_err(|e| format!("noteskins/index.json: {e}")),
        Err(e) => Err(e),
    };
    let skins: Vec<StoredSkin> = index?
        .skins
        .into_iter()
        .map(|b| StoredSkin {
            game: b.folder.game,
            name: b.folder.name,
            files: b.folder.files,
            texts: b.folder.texts,
            origin: b.credit,
            stored: Vec::new(),
            lua_broken: None,
            bundled: Some(b.path),
            variant_of: b.variant_of,
        })
        .collect();
    BUNDLED_SKINS.with(|b| *b.borrow_mut() = Some(skins.clone()));
    Ok(with_session_stops(skins))
}

/// [`bundled`], or none (said in the console) when they cannot be read.
pub(crate) async fn bundled_or_none() -> Vec<StoredSkin> {
    bundled().await.unwrap_or_else(|e| {
        web_sys::console::warn_1(&e.into());
        Vec::new()
    })
}

thread_local! {
    /// The bundled skins, once read.
    static BUNDLED_SKINS: RefCell<Option<Vec<StoredSkin>>> = const { RefCell::new(None) };
}

/// Bundled skins whose Lua stopped twice in this session have it off.
fn with_session_stops(mut skins: Vec<StoredSkin>) -> Vec<StoredSkin> {
    LUA_OFF.with(|off| {
        let off = off.borrow();
        for s in &mut skins {
            s.lua_broken = off.get(&s.strike_key()).cloned();
        }
    });
    skins
}

/// The key a Lua stop of skin `key` counts against: a colour variant's
/// counts against the skin it is a variant of (they share their Lua).
fn strike_key(key: &str) -> String {
    BUNDLED_SKINS
        .with(|b| {
            b.borrow()
                .as_ref()?
                .iter()
                .find(|s| s.key() == key)
                .map(StoredSkin::strike_key)
        })
        .unwrap_or_else(|| key.to_string())
}

/// Every skin a chain may use: the imported ones (when storage opens),
/// then the bundled ones.
pub(crate) async fn catalogue(db: Option<&Db>) -> Vec<StoredSkin> {
    let mut skins = match db {
        Some(db) => all(db).await.unwrap_or_else(|e| {
            web_sys::console::warn_1(&e.into());
            Vec::new()
        }),
        None => Vec::new(),
    };
    skins.extend(bundled_or_none().await);
    skins
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
    if key.starts_with(BUNDLED) {
        return Err(format!("{key} comes with the game"));
    }
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

/// The dance skins one can pick: the bundled ones, then the imported ones.
pub(crate) struct Choices {
    /// `(key, name)`.
    pub(crate) skins: Vec<(String, String)>,
    /// Whether every skin could be read (else a chosen one missing from
    /// the list may still be there).
    pub(crate) complete: bool,
    /// Why the default skin's (Dito's) Lua is off for the session, if it is.
    pub(crate) default_lua_off: Option<String>,
}

/// The dance skins one can pick; an error when none could be read.
pub(crate) async fn choices() -> Result<Choices, String> {
    let imported = match Db::open().await {
        Ok(db) => all(&db).await,
        Err(e) => Err(e),
    };
    let bundled = bundled().await;
    let complete = imported.is_ok() && bundled.is_ok();
    let skins = match (imported, bundled) {
        (Err(e), Err(_)) => return Err(e),
        (i, b) => {
            let mut skins = i.unwrap_or_default();
            skins.extend(b.unwrap_or_default());
            skins
        }
    };
    let mut skins: Vec<(bool, String, String)> = skins
        .into_iter()
        // Dito and its colours are the default, chosen as such.
        .filter(|s| {
            s.game == "dance"
                && !s.name.eq_ignore_ascii_case("default")
                && s.variant_of.is_none()
                && !(s.bundled.is_some() && s.name == "Dito")
        })
        .map(|s| {
            let mut name = s.name.clone();
            if s.bundled.is_some() {
                name.push_str(" (with the game)");
            }
            if s.lua_broken.is_some() {
                name.push_str(" (its Lua is off)");
            }
            (s.bundled.is_none(), s.key(), name)
        })
        .collect();
    skins.sort_by_key(|(imported, _, n)| (*imported, n.to_lowercase()));
    Ok(Choices {
        skins: skins.into_iter().map(|(_, k, n)| (k, n)).collect(),
        complete,
        default_lua_off: LUA_OFF.with(|off| off.borrow().get(DITO).cloned()),
    })
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
    /// Bundled skins whose Lua is off for the session, and why.
    static LUA_OFF: RefCell<HashMap<String, String>> = RefCell::default();
}

/// Skin `key`'s Lua stopped (at a load or in a play) because of `why`:
/// the next play tries again; the second time its Lua is turned off and
/// the record says why (a bundled skin's for the session).
pub(crate) async fn lua_stopped(key: &str, why: &str) {
    web_sys::console::warn_1(&format!("noteskin {key}: {why}").into());
    let key = strike_key(key);
    let key = key.as_str();
    let again = STOPPED_ONCE.with(|s| !s.borrow_mut().insert(key.to_string()));
    if !again {
        return;
    }
    if key.starts_with(BUNDLED) {
        LUA_OFF.with(|off| off.borrow_mut().insert(key.to_string(), why.to_string()));
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
    db: Option<&Db>,
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
        let blob = match (&skin.bundled, db) {
            (Some(dir), _) => fetch_blob(dir, &path).await,
            (None, Some(db)) => db
                .get(idb::FILES, &file_key(&skin.key(), &path))
                .await
                .ok()
                .flatten()
                .and_then(|v| v.dyn_into::<Blob>().ok()),
            (None, None) => None,
        };
        let Some(blob) = blob else {
            web_sys::console::warn_1(
                &format!("noteskin {}: {path} is not stored", skin.name).into(),
            );
            continue;
        };
        let decoded = if path.to_ascii_lowercase().ends_with(".svg") {
            svg_bitmap(&blob).await
        } else {
            crate::web::image::decode_sized(&blob, MAX_IMAGE_SIDE).await
        };
        match decoded {
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

/// How much finer than its own size an SVG is drawn: a skin's 64-pixel
/// lane is drawn up to 160 device pixels wide (the arrow size's cap), so
/// four times keeps it sharp there.
const SVG_SCALE: f64 = 4.0;

/// An SVG image, checked and rasterised, with the size the skin sees (its
/// own, as for a bitmap).
async fn svg_bitmap(blob: &Blob) -> Result<(ImageBitmap, (u32, u32)), String> {
    if blob.size() > ddi_noteskin::svg::MAX_SVG as f64 {
        return Err("the SVG is too large".into());
    }
    let text = wasm_bindgen_futures::JsFuture::from(blob.text())
        .await
        .map_err(|_| "the SVG could not be read")?
        .as_string()
        .ok_or("the SVG could not be read")?;
    let size = ddi_noteskin::svg::check(&text)?;
    let bitmap = crate::web::image::rasterise_svg(&text, size, SVG_SCALE, MAX_IMAGE_SIDE).await?;
    Ok((bitmap, size))
}

/// A bundled skin's file `path`, from its folder `dir` under `noteskins/`.
async fn fetch_blob(dir: &str, path: &str) -> Option<Blob> {
    let encoded: Vec<String> = format!("{dir}/{path}")
        .split('/')
        .map(|p| String::from(js_sys::encode_uri_component(p)))
        .collect();
    let bytes = crate::songs::fetch_bytes(&format!("noteskins/{}", encoded.join("/")))
        .await
        .map_err(|e| web_sys::console::warn_1(&e.into()))
        .ok()?;
    let mime = match path
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        Some("svg") => "image/svg+xml",
        _ => "",
    };
    crate::web::files::bytes_blob(&bytes, mime).ok()
}

/// Whether a skin or one of the skins it falls back on has Lua of its own.
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
    // A bundled skin plays without storage.
    let db = Db::open().await.ok();
    let mut stored = catalogue(db.as_ref()).await;
    // Skins are found by name, and an imported one may share a bundled
    // one's: the chosen skin first; a bundled skin's fallbacks only among
    // the bundled ones (no import, not an imported `default` or `common`,
    // changes the default skin), an imported one's among the imported
    // ones, then the bundled ones.
    let chosen = stored.iter().position(|s| s.key() == key)?;
    let first = stored.remove(chosen);
    if first.bundled.is_some() {
        stored.retain(|s| s.bundled.is_some());
    }
    stored.insert(0, first);
    let chosen = 0;
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
        if let (Some(s), Some(db)) = (stored.get_mut(i), &db)
            && s.bundled.is_none()
        {
            add_lua_texts(db, s).await;
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
    decode(db.as_ref(), &stored, &mut sizes, &mut bitmaps, &mut pixels).await;
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
                decode(db.as_ref(), &stored, &mut sizes, &mut bitmaps, &mut pixels).await;
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
