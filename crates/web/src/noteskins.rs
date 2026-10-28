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

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use ddi_chart::Layout;
use ddi_noteskin::build::{MAX_IMAGE_SIDE, MAX_PIXELS, ProbeSizes, TextureKey};
use ddi_noteskin::lua::Env;
use ddi_noteskin::report::{ImageCost, ImageReport, Outcome};
use ddi_noteskin::{Dir, Folder, Skin};
use ddi_render::noteskin::{NoteskinPlay, SameDocuments};
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
    /// The texts `ddi_noteskin::import::kept_text` keeps (`.ini`, `.redir`,
    /// `.lua`, `.txt`; older records get the ones added since from the
    /// stored files, `add_missing_texts`).
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
    /// The bundled skin this one is a color scheme of (never stored).
    #[serde(skip)]
    pub(crate) variant_of: Option<String>,
}

impl StoredSkin {
    /// The key, `<game>/<name>` (names match ignoring case, as
    /// StepMania's do), after [`BUNDLED`] for a bundled skin.
    pub(crate) fn key(&self) -> String {
        ddi_noteskin::bundled::key(&self.game, &self.name, self.bundled.is_some())
    }

    /// See [`strike_key`](fn@strike_key).
    fn strike_key(&self) -> String {
        ddi_noteskin::bundled::strike_key(
            &self.game,
            &self.name,
            self.bundled.is_some(),
            self.variant_of.as_deref(),
        )
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

use ddi_noteskin::bundled::INFINITO;
pub(crate) use ddi_noteskin::bundled::{BUILT_IN, KEY_PREFIX as BUNDLED, skin_key};

/// Key of a skin's file in the `files` store.
pub(crate) fn file_key(skin: &str, path: &str) -> String {
    format!("noteskin/{skin}/{path}")
}

/// Images and scripts a skin keeps, at most; larger skins are cut and
/// reported (its `metrics.ini` and redirects are always kept).
pub(crate) const MAX_FILES: usize = 2000;
/// Skins one import keeps, at most.
pub(crate) const MAX_SKINS: usize = 200;
pub(crate) use ddi_noteskin::import::{MAX_TEXT, MAX_TEXTS, kept_text};

/// Whether a skin's file is worth storing: images, Lua and models (for the
/// steps that read them), the texts.
pub(crate) fn kept_file(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [
        ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".svg", ".lua", ".wgsl", ".txt", ".ini", ".redir",
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
    // Revalidated each session: the files' hashes in it name them.
    let index = match crate::songs::fetch_text_fresh("noteskins/index.json").await {
        Ok(text) => serde_json::from_str::<ddi_noteskin::bundled::Index>(&text)
            .map_err(|e| format!("noteskins/index.json: {e}")),
        Err(e) => Err(e),
    };
    let index = index?;
    BUNDLED_HASHES.with_borrow_mut(|h| {
        for b in &index.skins {
            for (f, hash) in b.folder.files.iter().zip(&b.hashes) {
                h.insert((b.path.clone(), f.clone()), hash.clone());
            }
        }
    });
    BUNDLED_SIZES.with_borrow_mut(|h| {
        for b in &index.skins {
            for (f, size) in b.folder.files.iter().zip(&b.sizes) {
                h.insert((b.path.clone(), f.clone()), *size);
            }
        }
    });
    let skins: Vec<StoredSkin> = index
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

/// The key a Lua stop of skin `key` counts against: a color variant's
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
#[derive(Clone)]
pub(crate) struct Choices {
    /// `(key, name)`.
    pub(crate) skins: Vec<(String, String)>,
    /// Whether every skin could be read (else a chosen one missing from
    /// the list may still be there).
    pub(crate) complete: bool,
    /// Why the default skin's (Dito's) Lua is off for the session, if it is.
    pub(crate) default_lua_off: Option<String>,
    /// The parameters each skin declares, by key (`ddi_noteskin::params`).
    pub(crate) parameters: std::collections::BTreeMap<String, Vec<ddi_noteskin::params::Parameter>>,
}

thread_local! {
    /// The last [`choices`] read ([`choices_cached`]).
    static CHOICES: RefCell<Option<Rc<Choices>>> = const { RefCell::new(None) };
}

/// The skins one can pick as last read ([`choices`]): the prompt's
/// options from their first frame (the boot's preload reads them).
pub(crate) fn choices_cached() -> Option<Rc<Choices>> {
    CHOICES.with_borrow(Clone::clone)
}

/// The dance skins one can pick; an error when none could be read.
pub(crate) async fn choices() -> Result<Choices, String> {
    let read = read_choices().await?;
    CHOICES.with_borrow_mut(|c| *c = Some(Rc::new(read.clone())));
    Ok(read)
}

async fn read_choices() -> Result<Choices, String> {
    let imported = match Db::open().await {
        Ok(db) => all(&db).await,
        Err(e) => Err(e),
    };
    let bundled = bundled().await;
    let complete = imported.is_ok() && bundled.is_ok();
    let (stored, bundled_from) = match (imported, bundled) {
        (Err(e), Err(_)) => return Err(e),
        (i, b) => {
            let mut skins = i.unwrap_or_default();
            let bundled_from = skins.len();
            skins.extend(b.unwrap_or_default());
            (skins, bundled_from)
        }
    };
    let skins = pickable(&stored);
    // What each declares (Dito's are built in).
    let keys: Vec<String> = skins.iter().map(|(k, _)| k.clone()).collect();
    let folders: Vec<Folder> = stored.iter().map(StoredSkin::folder).collect();
    let parameters = ddi_noteskin::params::catalog(&folders, bundled_from, &keys);
    Ok(Choices {
        parameters,
        skins,
        complete,
        default_lua_off: LUA_OFF.with(|off| off.borrow().get(INFINITO).cloned()),
    })
}

/// The skins of `stored` one can pick, `(key, name)`. Dito and its colors
/// are the default, chosen as such. Pump skins are listed too, for the
/// pump layouts (`Settings::pump_noteskin`).
fn pickable(stored: &[StoredSkin]) -> Vec<(String, String)> {
    ddi_noteskin::bundled::pickable(stored.iter().map(|s| ddi_noteskin::bundled::Candidate {
        game: &s.game,
        name: &s.name,
        bundled: s.bundled.is_some(),
        variant: s.variant_of.is_some(),
        lua_off: s.lua_broken.is_some(),
    }))
}

/// What each element of a skin is, in a line for the import report: how
/// many of a column's elements are the skin's own and why the rest are
/// the built-in skin's, as its Lua builds them when it has Lua, else as
/// they resolve without it. A dance skin is probed on dance single's
/// Down, a pump skin on Pump single's UpLeft; `common` is said to be the
/// shared files.
pub(crate) async fn summary(folders: &[Folder], game: &str, name: &str) -> String {
    use ddi_noteskin::Button;
    if game == "common" {
        return "StepMania's shared files (common), which the other skins fall back on".to_string();
    }
    let skin = match Skin::load(folders, game, name) {
        Ok(s) => s,
        Err(e) => return e.to_string(),
    };
    let pump = game == "pump";
    let (layout, button) = if pump {
        (Layout::pump_single(), Button::UpLeft)
    } else {
        (Layout::dance_single(), Button::Down)
    };
    let line = if skin.legacy {
        legacy_summary(&skin, button)
    } else if !skin.has_lua() && !pump {
        native_summary(&skin, button)
    } else {
        lua_summary(folders, &skin, game, name, &layout, button).await
    };
    if pump {
        format!("a pump skin, on Pump single: {line}")
    } else {
        line
    }
}

/// [`summary`]'s line from the skin's Lua (a pump skin's always runs:
/// pump/default's Lua makes its elements).
async fn lua_summary(
    folders: &[Folder],
    skin: &Skin<'_>,
    game: &str,
    name: &str,
    layout: &Layout,
    button: ddi_noteskin::Button,
) -> String {
    let probe = ProbeSizes::probe();
    let env = Env {
        game: game.to_string(),
        style: ddi_noteskin::lua::style_for(game, layout).into(),
        ..Env::default()
    };
    match NoteskinPlay::load_lua(
        folders,
        game,
        name,
        env,
        hash_seed(),
        layout,
        &probe,
        clock(),
        Some(&mut pause),
    )
    .await
    {
        Ok(Some(play)) => {
            let total = ddi_noteskin::load::ENGINE_ELEMENTS.len();
            let own = play.own_elements(1);
            let mut line = format!("its Lua ran: {own} of {total} elements are its own");
            if own < total {
                // Why the first few are not (text, an unsupported actor).
                let prefix = format!("{}: ", button.name());
                let why: Vec<&str> = play
                    .notes
                    .iter()
                    .filter_map(|n| n.strip_prefix(prefix.as_str()))
                    .take(3)
                    .collect();
                line.push_str(&format!("; {} drawn built in", total - own));
                if !why.is_empty() {
                    line.push_str(&format!(" ({})", why.join("; ")));
                }
            }
            line
        }
        Ok(None) => "a layout's lanes it cannot draw".to_string(),
        Err(e) => format!(
            "its Lua stopped ({e}); it is drawn without it: {}",
            native_summary(skin, button)
        ),
    }
}

/// [`summary`]'s line for a skin drawn without its Lua.
/// A skin in StepMania 3.9's format, built natively: how many of the
/// elements the engine asks for are its own, and why the first few are
/// not.
fn legacy_summary(skin: &Skin<'_>, button: ddi_noteskin::Button) -> String {
    use ddi_noteskin::build::{Built, Textures, build};
    use ddi_noteskin::load::ENGINE_ELEMENTS;
    let probe = ProbeSizes::probe();
    let mut textures = Textures::default();
    // The format has no lifts: they are drawn built in.
    let elements: Vec<&str> = ENGINE_ELEMENTS
        .iter()
        .copied()
        .filter(|e| *e != "Tap Lift")
        .collect();
    let (mut own, mut why) = (0, Vec::new());
    for e in &elements {
        let el = build(skin, button, e, &mut textures, &probe);
        match el.built {
            Built::Actor(_)
            | Built::Blank
            | Built::Component(_)
            | Built::LongNote(_)
            | Built::Lift(_) => own += 1,
            Built::BuiltIn => why.extend(el.notes.into_iter().take(1)),
        }
    }
    let mut line = format!(
        "made for StepMania 3.9 or OpenITG: {own} of {} elements are its own",
        elements.len()
    );
    if own < elements.len() {
        line.push_str(&format!("; {} drawn built in", elements.len() - own));
        if !why.is_empty() {
            why.truncate(3);
            line.push_str(&format!(" ({})", why.join("; ")));
        }
    }
    line
}

fn native_summary(skin: &Skin<'_>, button: ddi_noteskin::Button) -> String {
    use ddi_noteskin::load::ENGINE_ELEMENTS;
    use ddi_noteskin::{Resolved, Source};
    let (mut own, mut lua, mut other, mut builtin) = (0, 0, 0, 0);
    for e in ENGINE_ELEMENTS {
        match skin.resolve(button, e) {
            Resolved::File {
                source: Source::Image(_) | Source::Model(_),
                ..
            }
            | Resolved::Blank => own += 1,
            Resolved::File {
                source: Source::Lua(_),
                ..
            } => lua += 1,
            Resolved::File {
                source: Source::BuiltInArt(_) | Source::Native(..),
                ..
            }
            | Resolved::Missing(_) => builtin += 1,
            _ => other += 1,
        }
    }
    let mut parts = vec![format!(
        "{own} of {} elements are its own images and models",
        ENGINE_ELEMENTS.len()
    )];
    if lua > 0 {
        parts.push(format!("{lua} are Lua (drawn built in)"));
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

/// Whether a load is still wanted ([`prepare_while`]).
pub(crate) type Wanted = Rc<dyn Fn() -> bool>;

/// A load's clock for its Lua: the host's, until the load is no longer
/// wanted while it runs (not `done`); from then on each reading is an
/// hour on, so the Lua's deadline ends it at its next check, whatever
/// it was doing, rather than running to its end for nothing.
fn load_clock(wanted: Wanted, done: Rc<Cell<bool>>) -> Box<dyn FnMut() -> f64> {
    let mut host = clock();
    let mut ahead = 0.0;
    Box::new(move || {
        if !done.get() && !wanted() {
            ahead += 3600.0;
        }
        host() + ahead
    })
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

/// Fills in the texts of a skin stored before they were kept with its
/// record (`.lua` since skins' Lua runs, models and `.ini` textures since
/// models draw), from its stored files, and saves the record; whether
/// any were added.
pub(crate) async fn add_missing_texts(db: &Db, skin: &mut StoredSkin) -> bool {
    let missing: Vec<String> = skin
        .stored
        .iter()
        .filter(|p| kept_text(p))
        .filter(|p| !skin.texts.iter().any(|(t, _)| t == *p))
        .cloned()
        .collect();
    if missing.is_empty() {
        return false;
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
        return false;
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
    true
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
    // An imported skin's record is stored under its key ([`store`]).
    let Ok(Some(record)) = db.get(idb::NOTESKINS, key).await else {
        return;
    };
    let Some(mut skin) = record
        .as_string()
        .and_then(|json| serde_json::from_str::<StoredSkin>(&json).ok())
    else {
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
/// yet, up to [`MAX_PIXELS`] in all (admitted in turn, an SVG's pixels
/// known from its root before it is drawn); then `sizes` answers with
/// theirs. The SVG images are drawn by the SVG workers, several at once
/// (`web::svg`), as desktop draws them.
#[allow(clippy::too_many_arguments)]
async fn decode(
    db: Option<&Db>,
    stored: &[StoredSkin],
    sizes: &mut ProbeSizes,
    bitmaps: &mut HashMap<(usize, String), ImageBitmap>,
    hashes: &mut HashMap<(usize, String), u64>,
    documents: &mut SameDocuments,
    pixels: &mut u64,
    report: &mut ImageReport,
    wanted: &Wanted,
    scale: f64,
) {
    let began = js_sys::Date::now();
    // Component files: their components read, every part drawn as an SVG
    // of its own with the others below (`ddi_noteskin::component`).
    let mut generated: Vec<(usize, String, String)> = Vec::new();
    for (i, path) in sizes.take_asked_components() {
        let Some(skin) = stored.get(i) else {
            continue;
        };
        let text = match skin_blob(db, skin, &path).await {
            Some(blob) => svg_text(&blob).await.map(|(text, _)| text),
            None => Err("not stored".to_string()),
        };
        let found = text.and_then(|text| {
            let read = ddi_noteskin::component::part_documents(&path, &text)?;
            generated.extend(read.documents.into_iter().map(|(p, doc)| (i, p, doc)));
            Ok(read.components)
        });
        if let Err(e) = &found {
            web_sys::console::warn_1(&format!("noteskin {}: {path}: {e}", skin.name).into());
        }
        sizes.components.insert((i, path), found);
    }
    let asked = sizes.take_asked();
    // The bundled skins' files downloaded together, decoded in turn.
    for (i, path) in &asked {
        if let Some(dir) = stored.get(*i).and_then(|s| s.bundled.as_deref()) {
            // A gradient's ramp is drawn from its file.
            let file =
                ddi_noteskin::component::split_ramp_path(path).map_or(path.as_str(), |r| r.0);
            let _ = file_promise(dir, file);
        }
    }
    let mut svgs = Vec::new();
    for (i, path) in asked {
        // A load no longer wanted decodes no more.
        if !wanted() {
            break;
        }
        if bitmaps.contains_key(&(i, path.clone())) {
            continue;
        }
        let Some(skin) = stored.get(i) else {
            continue;
        };
        let svg = path.to_ascii_lowercase().ends_with(".svg");
        report.images.push(ImageCost {
            hash: skin
                .bundled
                .as_deref()
                .and_then(|dir| bundled_hash(dir, &path))
                .unwrap_or_default(),
            ..ImageCost::left_out(&skin.name, &path)
        });
        let at_report = report.images.len() - 1;
        let cost = &mut report.images[at_report];
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
        // A gradient's ramp a material samples: drawn from its file with
        // the components' parts below.
        if let Some((file, _)) = ddi_noteskin::component::split_ramp_path(&path) {
            let text = match skin_blob(db, skin, file).await {
                Some(blob) => svg_text(&blob).await.map(|(text, _)| text),
                None => Err("not stored".to_string()),
            };
            match text.and_then(|t| {
                ddi_noteskin::component::derived_svg(&path, &t).unwrap_or(Err("no ramp".into()))
            }) {
                Ok(doc) => {
                    report.images.pop();
                    generated.push((i, path, doc));
                }
                Err(e) => cost.outcome = Outcome::Failed(e),
            }
            continue;
        }
        let Some(blob) = skin_blob(db, skin, &path).await else {
            web_sys::console::warn_1(
                &format!("noteskin {}: {path} is not stored", skin.name).into(),
            );
            cost.outcome = Outcome::Failed("not stored".into());
            continue;
        };
        if svg {
            // Checked and its pixels counted now, drawn by the SVG
            // workers with the others (collected below).
            match svg_text(&blob).await {
                Ok((text, checked)) => {
                    if cost.hash.is_empty() {
                        cost.hash = ddi_noteskin::report::hash(text.as_bytes());
                    }
                    let size = checked.size;
                    cost.own = size;
                    let at = checked.raster_size(scale);
                    *pixels += u64::from(at.0) * u64::from(at.1);
                    let drawing =
                        (!browser_svg()).then(|| crate::web::svg::rasterise(text.clone(), at));
                    svgs.push((i, path, text, size, at, drawing, at_report));
                }
                Err(e) => {
                    web_sys::console::warn_1(
                        &format!("noteskin {}: {path}: {e}", skin.name).into(),
                    );
                    cost.outcome = Outcome::Failed(e);
                }
            }
            continue;
        }
        if cost.hash.is_empty()
            && let Ok(buffer) = wasm_bindgen_futures::JsFuture::from(blob.array_buffer()).await
        {
            cost.hash = ddi_noteskin::report::hash(&js_sys::Uint8Array::new(&buffer).to_vec());
        }
        match crate::web::image::decode_sized(&blob, MAX_IMAGE_SIDE).await {
            Ok((bitmap, size)) => {
                *pixels += u64::from(bitmap.width()) * u64::from(bitmap.height());
                cost.own = size;
                cost.decoded = (bitmap.width(), bitmap.height());
                cost.outcome = Outcome::Kept;
                sizes.sizes.insert((i, path.clone()), size);
                bitmaps.insert((i, path), bitmap);
            }
            Err(e) => {
                web_sys::console::warn_1(&format!("noteskin {}: {path}: {e}", skin.name).into());
                cost.outcome = Outcome::Failed(e);
            }
        }
    }
    // The components' parts, checked and counted as SVG files are; a part
    // of a document drawn already not drawn again.
    let mut copies = Vec::new();
    for (i, path, text) in generated {
        let Some(skin) = stored.get(i) else {
            continue;
        };
        if bitmaps.contains_key(&(i, path.clone())) || documents.is_copy(i, &path) {
            continue;
        }
        let first = documents.first_of(i, &path, &text);
        report.images.push(ImageCost {
            hash: ddi_noteskin::report::hash(text.as_bytes()),
            svg: true,
            ..ImageCost::left_out(&skin.name, &path)
        });
        let at_report = report.images.len() - 1;
        if let Some(first) = first {
            copies.push((i, path, first, at_report));
            continue;
        }
        if *pixels >= MAX_PIXELS {
            continue;
        }
        match ddi_noteskin::svg::check_svg(&text) {
            Ok(checked) => {
                report.images[at_report].own = checked.size;
                let at = checked.raster_size(scale);
                *pixels += u64::from(at.0) * u64::from(at.1);
                let drawing =
                    (!browser_svg()).then(|| crate::web::svg::rasterise(text.clone(), at));
                svgs.push((i, path, text, checked.size, at, drawing, at_report));
            }
            Err(e) => report.images[at_report].outcome = Outcome::Failed(e),
        }
    }
    let count = svgs.len();
    let mut by_browser = 0;
    let mut svgs = svgs.into_iter();
    while let Some((i, path, text, size, at, drawing, at_report)) = svgs.next() {
        if !wanted() {
            // Given up: the drawings already asked for are let go as they
            // come, their bitmaps closed.
            for drawing in std::iter::once(drawing).chain(svgs.map(|s| s.5)).flatten() {
                wasm_bindgen_futures::spawn_local(async move {
                    if let Ok(v) = wasm_bindgen_futures::JsFuture::from(drawing).await
                        && let Some((bitmap, _)) = crate::web::svg::drawn(&v)
                    {
                        bitmap.close();
                    }
                });
            }
            break;
        }
        // `Err((why, refused))`: refused by the worker (the image's fault).
        let drawn: Result<_, (String, bool)> = match drawing {
            Some(p) => wasm_bindgen_futures::JsFuture::from(p)
                .await
                .map_err(|e| match crate::web::svg::refusal(&e) {
                    Some(why) => (why, true),
                    None => (e.as_string().unwrap_or_else(|| format!("{e:?}")), false),
                })
                .and_then(|v| crate::web::svg::drawn(&v).ok_or_else(|| ("no bitmap".into(), false)))
                .map(|(b, hash)| {
                    if let Some(hash) = hash {
                        hashes.insert((i, path.clone()), hash);
                    }
                    (b, Outcome::Kept)
                }),
            None => Err(("drawn by the browser (__DDI_SVG_BROWSER)".into(), false)),
        };
        // Where no SVG worker draws (its files missing), the browser does;
        // an image a worker refused is refused, as desktop refuses it.
        let drawn = match drawn {
            Ok(b) => Ok(b),
            Err((why, true)) => Err(why),
            Err((e, false)) => {
                if by_browser == 0 && !browser_svg() {
                    web_sys::console::warn_1(
                        &format!("noteskin {path}: {e}; its SVG images drawn by the browser")
                            .into(),
                    );
                }
                by_browser += 1;
                crate::web::image::rasterise_svg(&text, at)
                    .await
                    .map(|b| (b, Outcome::ByBrowser))
            }
        };
        let cost = &mut report.images[at_report];
        match drawn {
            Ok((bitmap, outcome)) => {
                cost.decoded = (bitmap.width(), bitmap.height());
                cost.outcome = outcome;
                sizes.sizes.insert((i, path.clone()), size);
                bitmaps.insert((i, path), bitmap);
            }
            Err(e) => {
                let name = stored.get(i).map_or("", |s| s.name.as_str());
                web_sys::console::warn_1(&format!("noteskin {name}: {path}: {e}").into());
                cost.outcome = Outcome::Failed(e);
            }
        }
    }
    // A part of a document drawn already: as that one came out.
    for (i, path, first, at) in copies {
        let name = stored.get(i).map_or("", |s| s.name.as_str());
        if report.copy_outcome(at, name, &first)
            && let Some(size) = sizes.sizes.get(&(i, first)).copied()
        {
            sizes.sizes.insert((i, path), size);
        }
    }
    if count > 0 {
        svg_load(count, by_browser, js_sys::Date::now() - began);
    }
    sizes.probe = false;
}

/// `window.__DDI_SKIN_IMAGES`: the last skin loaded's image report
/// (`{skin, summary, hash, pixels, budget, images}`), and its summary in the
/// console.
fn publish_report(name: &str, report: &ImageReport) {
    web_sys::console::info_1(&format!("noteskin {name}: {}", report.summary()).into());
    let Some(w) = web_sys::window() else {
        return;
    };
    let json = serde_json::json!({
        "skin": name,
        "summary": report.summary(),
        "hash": report.hash(),
        "pixels": report.pixels(),
        "budget": report.budget(),
        "images": report.images,
    });
    if let Ok(value) = js_sys::JSON::parse(&json.to_string()) {
        let _ = js_sys::Reflect::set(&w, &JsValue::from_str("__DDI_SKIN_IMAGES"), &value);
    }
}

/// Tests set `window.__DDI_SVG_BROWSER` to draw skins' SVGs with the
/// browser, as before the SVG worker, to compare the two.
fn browser_svg() -> bool {
    web_sys::window()
        .and_then(|w| js_sys::Reflect::get(&w, &JsValue::from_str("__DDI_SVG_BROWSER")).ok())
        .is_some_and(|v| v.is_truthy())
}

/// `window.__DDI_SVG_LOADS`: each skin load's SVG images, how many the
/// browser drew, and the time its images took to read and decode, ms.
fn svg_load(images: usize, by_browser: usize, ms: f64) {
    let Some(w) = web_sys::window() else {
        return;
    };
    let key = JsValue::from_str("__DDI_SVG_LOADS");
    let loads = js_sys::Reflect::get(&w, &key)
        .ok()
        .and_then(|v| v.dyn_into::<js_sys::Array>().ok())
        .unwrap_or_else(|| {
            let a = js_sys::Array::new();
            let _ = js_sys::Reflect::set(&w, &key, &a);
            a
        });
    let o = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&o, &"images".into(), &JsValue::from(images as u32));
    let _ = js_sys::Reflect::set(&o, &"byBrowser".into(), &JsValue::from(by_browser as u32));
    let _ = js_sys::Reflect::set(&o, &"ms".into(), &JsValue::from(ms));
    loads.push(&o);
}

/// The scale skins' SVGs are drawn at for this screen
/// ([`ddi_noteskin::build::svg_scale`]; its height in device pixels, the
/// most a field can take), or for `arrows` a page draws at its own size
/// (device pixels), whichever is larger.
fn svg_scale(arrows: Option<f64>) -> f64 {
    let height = web_sys::window()
        .and_then(|w| {
            let height = w.screen().ok()?.height().ok()?;
            Some(f64::from(height) * w.device_pixel_ratio())
        })
        .unwrap_or(2160.0);
    let screen = ddi_noteskin::build::svg_scale(height.round() as u32);
    arrows.map_or(screen, |px| {
        screen.max(ddi_noteskin::build::svg_scale_for_arrow(px))
    })
}

/// A skin's file: a bundled skin's fetched, an imported one's from the
/// library.
async fn skin_blob(db: Option<&Db>, skin: &StoredSkin, path: &str) -> Option<Blob> {
    match (&skin.bundled, db) {
        (Some(dir), _) => fetch_blob(dir, path).await,
        (None, Some(db)) => db
            .get(idb::FILES, &file_key(&skin.key(), path))
            .await
            .ok()
            .flatten()
            .and_then(|v| v.dyn_into::<Blob>().ok()),
        (None, None) => None,
    }
}

/// An SVG image's text, checked.
async fn svg_text(blob: &Blob) -> Result<(String, ddi_noteskin::svg::Checked), String> {
    if blob.size() > ddi_noteskin::svg::MAX_SVG as f64 {
        return Err("the SVG is too large".into());
    }
    let text = wasm_bindgen_futures::JsFuture::from(blob.text())
        .await
        .map_err(|_| "the SVG could not be read")?
        .as_string()
        .ok_or("the SVG could not be read")?;
    let checked = ddi_noteskin::svg::check_svg(&text)?;
    Ok((text, checked))
}

/// A bundled skin's file `path`, from its folder `dir` under `noteskins/`.
/// A bundled skin's file, fetched once a session: plays and the preview
/// share one download of it (a second play never waits for the network
/// again, nor loses a file to it), tried again after a failed one.
async fn fetch_blob(dir: &str, path: &str) -> Option<Blob> {
    wasm_bindgen_futures::JsFuture::from(file_promise(dir, path))
        .await
        .ok()?
        .dyn_into::<Blob>()
        .ok()
}

thread_local! {
    /// The bundled skins' files fetched this session, in bytes (the
    /// index's sizes; a byte for a file it does not size): begun, and
    /// done. How far a skin's download is (the boot's bar,
    /// `crate::preload`).
    static FETCHES: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
}

/// The bundled skins' files fetched this session, in bytes: begun, and
/// done.
pub(crate) fn fetches() -> (u64, u64) {
    FETCHES.with(Cell::get)
}

/// The promise of a bundled skin's file's `Blob` (its download started
/// now, unless it already was): `null` when it could not be fetched, and
/// then forgotten, for the next load to try again. Fetched under its
/// content hash (`?v=`), so a new build's file never comes from an old
/// one's cache.
fn file_promise(dir: &str, path: &str) -> js_sys::Promise {
    let encoded: Vec<String> = format!("{dir}/{path}")
        .split('/')
        .map(|p| String::from(js_sys::encode_uri_component(p)))
        .collect();
    let mut url = format!("noteskins/{}", encoded.join("/"));
    if let Some(hash) = bundled_hash(dir, path) {
        url.push_str(&format!("?v={hash}"));
    }
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
    let size = bundled_size(dir, path).unwrap_or(1).max(1);
    FILES.with_borrow_mut(|files| {
        files
            .entry(url.clone())
            .or_insert_with(|| {
                FETCHES.with(|f| f.set((f.get().0 + size, f.get().1)));
                wasm_bindgen_futures::future_to_promise(async move {
                    let blob = fetch_again(&url)
                        .await
                        .and_then(|bytes| crate::web::files::bytes_blob(&bytes, mime));
                    FETCHES.with(|f| f.set((f.get().0, f.get().1 + size)));
                    Ok(match blob {
                        Ok(blob) => blob.into(),
                        Err(e) => {
                            FILES.with_borrow_mut(|files| files.remove(&url));
                            web_sys::console::warn_1(&e.into());
                            JsValue::NULL
                        }
                    })
                })
            })
            .clone()
    })
}

/// [`crate::songs::fetch_bytes`], tried three times (a slow network may
/// drop a download partway).
async fn fetch_again(url: &str) -> Result<Vec<u8>, String> {
    let mut tries = 0;
    loop {
        match crate::songs::fetch_bytes(url).await {
            Ok(bytes) => return Ok(bytes),
            Err(e) if tries >= 2 => return Err(e),
            Err(_) => {
                tries += 1;
                gloo::timers::future::TimeoutFuture::new(500 * tries).await;
            }
        }
    }
}

thread_local! {
    /// The bundled skins' files fetched this session (or being fetched),
    /// by URL: a promise of each one's `Blob`.
    static FILES: RefCell<HashMap<String, js_sys::Promise>> = RefCell::new(HashMap::new());
    /// The bundled skins' files' content hashes, by `(folder, path)`, from
    /// the index.
    static BUNDLED_HASHES: RefCell<HashMap<(String, String), String>> =
        RefCell::new(HashMap::new());
    /// The bundled skins' files' sizes in bytes, by `(folder, path)`,
    /// from the index.
    static BUNDLED_SIZES: RefCell<HashMap<(String, String), u64>> =
        RefCell::new(HashMap::new());
}

/// A bundled skin's file's content hash, from the index.
fn bundled_hash(dir: &str, path: &str) -> Option<String> {
    BUNDLED_HASHES.with_borrow(|h| h.get(&(dir.to_string(), path.to_string())).cloned())
}

/// A bundled skin's file's size in bytes, from the index.
fn bundled_size(dir: &str, path: &str) -> Option<u64> {
    BUNDLED_SIZES.with_borrow(|h| h.get(&(dir.to_string(), path.to_string())).copied())
}

/// Starts the downloads of the files of the bundled skin `key` (its
/// folder's, from the index) a load for `layout` is about to read, at
/// once ([`prefetch_folder`]). The bytes begun.
pub(crate) fn prefetch(key: &str, layout: &Layout) -> u64 {
    let skins = BUNDLED_SKINS
        .with(|b| b.borrow().clone())
        .unwrap_or_default();
    match skins
        .iter()
        .find(|s| s.key() == key)
        .and_then(|s| s.bundled.as_deref())
    {
        Some(dir) => prefetch_folder(dir, Some(layout)),
        None => 0,
    }
}

/// Starts the downloads of a bundled folder's files (`dir` under
/// `noteskins/`, its files from the index), at once and in a useful
/// order: its texts (metrics, Lua, shaders) first, since a load reads
/// them before any image, then its images, but for `layout` those named
/// for other layouts' buttons alone (`upleft` for a dance-single load),
/// which [`warm`] fetches later. A load reads its files in turn, and a
/// slow link pays for every file ahead of the one it waits for
/// (`crate::preload`, the boot's skin). The bytes begun.
fn prefetch_folder(dir: &str, layout: Option<&Layout>) -> u64 {
    let files: Vec<String> = BUNDLED_SIZES.with_borrow(|sizes| {
        sizes
            .keys()
            .filter(|(d, _)| d == dir)
            .map(|(_, f)| f.clone())
            .collect()
    });
    let lanes: HashSet<String> = layout
        .map(|l| {
            l.lanes
                .iter()
                .map(|l| l.name.to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_default();
    let others: HashSet<String> = match layout {
        Some(_) => Layout::builtins()
            .iter()
            .flat_map(|l| l.lanes.iter().map(|l| l.name.to_ascii_lowercase()))
            .chain(["onigiri".to_string(), "center".to_string()])
            .filter(|n| n.len() > 1 && !n.bytes().all(|b| b.is_ascii_digit()) && !lanes.contains(n))
            .collect(),
        None => HashSet::new(),
    };
    let mut files: Vec<&String> = files
        .iter()
        .filter(|f| {
            !f.to_ascii_lowercase()
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|word| others.contains(word))
        })
        .collect();
    files.sort_by_key(|f| !ddi_noteskin::import::kept_text(f));
    let mut bytes = 0;
    for path in files {
        let _ = file_promise(dir, path);
        bytes += bundled_size(dir, path).unwrap_or(1);
    }
    bytes
}

/// Fetches the rest of the chosen skin's files in the background (every
/// file of each bundled folder in its chain, texts first,
/// [`fetch_blob`]: those a chart's mines, rolls or other layouts use),
/// so that a play need not wait for the network: once the boot shell
/// has faded and the first screen's entrance is done (the preload
/// readied the preview's skin; a load here parsed the skin's components
/// again, 150 ms on the main thread as the menu came in), and
/// [`WARM_AFTER_MS`] later. Nothing is parsed or drawn.
pub(crate) fn warm() {
    wasm_bindgen_futures::spawn_local(async {
        for _ in 0..60 {
            if crate::boot::revealed() {
                break;
            }
            gloo::timers::future::TimeoutFuture::new(250).await;
        }
        gloo::timers::future::TimeoutFuture::new(WARM_AFTER_MS).await;
        let settings = crate::settings::Settings::load();
        let options = ddi_shell::field_preview_options(&settings);
        let want = ddi_shell::field_preview_skin(&settings, &options);
        let Some(key) = want.key.clone() else {
            return;
        };
        if !key.starts_with(BUNDLED) {
            return;
        }
        let layout = want.layout();
        for dir in chain_folders(&key, &want.env(&layout)).await {
            prefetch_folder(&dir, None);
        }
    });
}

/// The bundled folders the skin `key` (its parameters' head) draws
/// from, in its chain's order: the folder's path under `noteskins/`.
async fn chain_folders(key: &str, env: &Env) -> Vec<String> {
    let stored = bundled_or_none().await;
    let Some(chosen) = stored.iter().find(|s| s.key() == key) else {
        return Vec::new();
    };
    let folders: Vec<Folder> = stored.iter().map(StoredSkin::folder).collect();
    let Ok(skin) = ddi_noteskin::params::head(&folders, &chosen.game, &chosen.name, &env.params)
    else {
        return Vec::new();
    };
    skin.chain
        .iter()
        .filter_map(|d| match d {
            Dir::Folder(i) => stored.get(*i)?.bundled.clone(),
            Dir::BuiltIn(_) => None,
        })
        .collect()
}

/// How long after the shell's fade [`warm`] begins, in ms: after the
/// first screen's entrance.
const WARM_AFTER_MS: u32 = 1500;

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

/// Readies the first of `wants` one can pick ([`ddi_shell::first_there`]:
/// a chosen skin no longer there is drawn as Options shows it) for
/// `layout`, with its `Env` for its Lua: `None` for the built-in arrows,
/// or when it does not load or the layout's lanes are not its buttons.
pub(crate) async fn prepare_first(
    wants: ddi_shell::SkinWants,
    layout: &Layout,
) -> Option<Prepared> {
    let db = Db::open().await.ok();
    let stored = catalogue(db.as_ref()).await;
    let there = pickable(&stored);
    let (key, env) = ddi_shell::first_there(wants, &there)?;
    let with = LoadWith {
        among: Some((db, stored)),
        ..LoadWith::default()
    };
    load(&key, layout, env, Rc::new(|| true), with).await
}

/// The skins a load reads among: storage's, when it opens, and the
/// bundled ones ([`catalogue`]).
type Among = (Option<Db>, Vec<StoredSkin>);

/// What a load reads among and draws for besides its skin
/// ([`prepare_until`]).
#[derive(Default)]
struct LoadWith {
    /// The skins it reads among, when they were read already.
    among: Option<Among>,
    /// Arrows a page draws at its own size, device pixels: the skin's
    /// SVGs drawn sharp for those too ([`svg_scale`]).
    arrows: Option<f64>,
}

/// Readies skin `key` for `layout`, as [`prepare_first`], given up
/// (`None`) as soon as it is no longer `wanted` (a preview stepping
/// through skins): between its stages, between images, and inside the
/// skin's Lua, which its clock ends ([`load_clock`]). A load given up never counts against the skin's
/// Lua.
/// `arrows`, for a page drawing its arrows at its own size (device pixels,
/// `#/skin-preview`), has the skin's SVGs drawn sharp for those too.
pub(crate) async fn prepare_while(
    key: &str,
    layout: &Layout,
    env: Env,
    wanted: Wanted,
    arrows: Option<f64>,
) -> Option<Prepared> {
    let with = LoadWith {
        arrows,
        ..LoadWith::default()
    };
    load(key, layout, env, wanted, with).await
}

/// [`prepare_while`], as `with` says.
async fn load(
    key: &str,
    layout: &Layout,
    env: Env,
    wanted: Wanted,
    with: LoadWith,
) -> Option<Prepared> {
    let done = Rc::new(Cell::new(false));
    let prepared = prepare_until(key, layout, env, &wanted, &done, with).await;
    // The skin's Lua keeps the clock for the play: the host's from now on.
    done.set(true);
    let given_up = !wanted();
    LOADS.with(|l| {
        let (done, gave_up) = l.get();
        l.set(if given_up {
            (done, gave_up + 1)
        } else {
            (done + 1, gave_up)
        });
    });
    match prepared {
        Some(p) if given_up => {
            p.close();
            None
        }
        p => p,
    }
}

thread_local! {
    /// Loads that ran to their end, and loads given up (`__DDI_DEBUG`).
    static LOADS: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
}

/// Loads that ran to their end, and loads given up, so far.
pub(crate) fn loads() -> (u64, u64) {
    LOADS.with(Cell::get)
}

/// [`load`].
async fn prepare_until(
    key: &str,
    layout: &Layout,
    env: Env,
    wanted: &Wanted,
    done: &Rc<Cell<bool>>,
    with: LoadWith,
) -> Option<Prepared> {
    let LoadWith { among, arrows } = with;
    let scale = svg_scale(arrows);
    let clock = || load_clock(wanted.clone(), done.clone());
    // A bundled skin plays without storage.
    let (db, mut stored) = match among {
        Some(among) => among,
        None => {
            let db = Db::open().await.ok();
            let stored = catalogue(db.as_ref()).await;
            (db, stored)
        }
    };
    if !wanted() {
        return None;
    }
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
    let mut folders: Vec<Folder> = stored.iter().map(StoredSkin::folder).collect();
    // The skin its parameters pick (a color scheme's), loaded in its place
    // (`ddi_noteskin::params`).
    let (head, chain): (String, Vec<usize>) = {
        let skin = match ddi_noteskin::params::head(
            &folders,
            &stored[chosen].game,
            &stored[chosen].name,
            &env.params,
        ) {
            Ok(s) => s,
            Err(e) => {
                web_sys::console::warn_1(&format!("noteskin {key}: {e}").into());
                return None;
            }
        };
        let chain = skin
            .chain
            .iter()
            .filter_map(|d| match d {
                Dir::Folder(i) => Some(*i),
                Dir::BuiltIn(_) => None,
            })
            .collect();
        (skin.name, chain)
    };
    for i in chain {
        if let (Some(s), Some(db)) = (stored.get_mut(i), &db)
            && s.bundled.is_none()
            && add_missing_texts(db, s).await
        {
            folders[i] = s.folder();
        }
    }
    if !wanted() {
        return None;
    }
    let stored = stored;
    let folders = folders;
    let chosen = &stored[chosen];
    let skin = match Skin::load(&folders, &chosen.game, &head) {
        Ok(s) => s,
        Err(e) => {
            web_sys::console::warn_1(&format!("noteskin {head}: {e}").into());
            return None;
        }
    };
    if let Some(why) = &chosen.lua_broken {
        web_sys::console::info_1(
            &format!("noteskin {}: its Lua is off ({why})", chosen.name).into(),
        );
    }
    // Skins with no Lua of their own (nor their fallbacks) play natively:
    // the same as StepMania's stand-ins' Lua, without its costs. A pump
    // skin's elements are pump/default's Lua's (the right side's taken
    // from the left's): it always runs.
    let with_lua = chosen.lua_broken.is_none() && (skin.has_lua() || chosen.game == "pump");
    // The skin's Lua is told its own game, and the style it plays.
    let mut env = env;
    env.game = chosen.game.clone();
    env.style = ddi_noteskin::lua::style_for(&chosen.game, layout).into();
    let seed = hash_seed();
    let mut lua_error = None;
    // The SVG worker compiled while the first pass runs, for a skin with
    // SVG images.
    if folders.iter().any(|f| {
        f.files
            .iter()
            .any(|p| p.to_ascii_lowercase().ends_with(".svg"))
    }) {
        crate::web::svg::warm();
    }
    // First pass: which images the actors use.
    let mut sizes = ProbeSizes::probe();
    if with_lua {
        match NoteskinPlay::load_lua(
            &folders,
            &chosen.game,
            &head,
            env.clone(),
            seed,
            layout,
            &sizes,
            clock(),
            Some(&mut pause),
        )
        .await
        {
            Ok(Some(_)) => {}
            Ok(None) => return None,
            Err(_) if !wanted() => return None,
            Err(e) => lua_error = Some(e),
        }
    }
    if !with_lua || lua_error.is_some() {
        NoteskinPlay::new(&skin, layout, &sizes, true)?;
    }
    let mut bitmaps: HashMap<(usize, String), ImageBitmap> = HashMap::new();
    let mut hashes: HashMap<(usize, String), u64> = HashMap::new();
    let mut documents = SameDocuments::default();
    let mut pixels: u64 = 0;
    let mut report = ImageReport::default();
    // Every image decoded so far is closed when the load is given up.
    let give_up = |bitmaps: &HashMap<(usize, String), ImageBitmap>| {
        for b in bitmaps.values() {
            b.close();
        }
        None
    };
    decode(
        db.as_ref(),
        &stored,
        &mut sizes,
        &mut bitmaps,
        &mut hashes,
        &mut documents,
        &mut pixels,
        &mut report,
        wanted,
        scale,
    )
    .await;
    if !wanted() {
        return give_up(&bitmaps);
    }
    let mut play = None;
    if with_lua && lua_error.is_none() {
        match NoteskinPlay::load_lua(
            &folders,
            &chosen.game,
            &head,
            env,
            seed,
            layout,
            &sizes,
            clock(),
            Some(&mut pause),
        )
        .await
        {
            Ok(p) => play = p,
            Err(_) if !wanted() => return give_up(&bitmaps),
            Err(e) => {
                lua_error = Some(e);
                // The native build may want images the Lua did not.
                sizes.probe = true;
                let _ = NoteskinPlay::new(&skin, layout, &sizes, true);
                decode(
                    db.as_ref(),
                    &stored,
                    &mut sizes,
                    &mut bitmaps,
                    &mut hashes,
                    &mut documents,
                    &mut pixels,
                    &mut report,
                    wanted,
                    scale,
                )
                .await;
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
    let Some(mut play) = play else {
        for b in bitmaps.values() {
            b.close();
        }
        return None;
    };
    let (mut images, same) = play.take_images(&mut bitmaps, &documents.same);
    // An image of the same pixels as an earlier one (the SVG workers'
    // hashes, by the part each image is) is closed, its texture given
    // that one's.
    let keys: Vec<_> = play
        .texture_keys()
        .iter()
        .map(|k| match k {
            TextureKey::Skin {
                dir: Dir::Folder(f),
                path,
            } => Some((*f, path.clone())),
            _ => None,
        })
        .collect();
    let hash = |i: usize| {
        let key = keys[i].as_ref()?;
        images[i].as_ref()?;
        match documents.same.get(key) {
            Some(first) => hashes.get(&(key.0, first.clone())).copied(),
            None => hashes.get(key).copied(),
        }
    };
    let pixels = play.share_textures(hash, |_, _| true);
    for (i, with) in same.into_iter().chain(pixels) {
        // Closed unless another texture keeps the same bitmap (one of the
        // same file mipmapped otherwise, given a copy of the handle).
        if let Some(b) = images[i].take()
            && !images
                .iter()
                .flatten()
                .any(|other| js_sys::Object::is(other, &b))
        {
            b.close();
        }
        if let (Some((f, path)), Some((_, with))) = (&keys[i], &keys[with])
            && let Some(skin) = stored.get(*f)
        {
            report.share(&skin.name, path, with);
        }
    }
    // What no texture took.
    for b in bitmaps.values() {
        b.close();
    }
    for n in &play.notes {
        web_sys::console::info_1(&format!("noteskin {}: {n}", chosen.name).into());
    }
    publish_report(&chosen.name, &report);
    Some(Prepared {
        key: key.to_string(),
        play,
        images,
    })
}
