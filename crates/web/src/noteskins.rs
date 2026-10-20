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
use std::collections::HashMap;

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use ddi_chart::Layout;
use ddi_noteskin::build::{MAX_IMAGE_SIDE, MAX_PIXELS, ProbeSizes, SVG_SCALE, TextureKey};
use ddi_noteskin::lua::Env;
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

use ddi_noteskin::bundled::DITO;
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
    // Dito and its colors are the default, chosen as such. Pump skins
    // are listed too, for the pump layouts (`Settings::pump_noteskin`).
    let skins =
        ddi_noteskin::bundled::pickable(skins.iter().map(|s| ddi_noteskin::bundled::Candidate {
            game: &s.game,
            name: &s.name,
            bundled: s.bundled.is_some(),
            variant: s.variant_of.is_some(),
            lua_off: s.lua_broken.is_some(),
        }));
    Ok(Choices {
        skins,
        complete,
        default_lua_off: LUA_OFF.with(|off| off.borrow().get(DITO).cloned()),
    })
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
            Built::Actor(_) | Built::Blank => own += 1,
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
/// yet, up to [`MAX_PIXELS`] in all; then `sizes` answers with theirs.
async fn decode(
    db: Option<&Db>,
    stored: &[StoredSkin],
    sizes: &mut ProbeSizes,
    bitmaps: &mut HashMap<(usize, String), ImageBitmap>,
    pixels: &mut u64,
    wanted: &Wanted,
) {
    for (i, path) in sizes.take_asked() {
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
    prepare_while(key, layout, env, Rc::new(|| true)).await
}

/// [`prepare`], given up (`None`) as soon as it is no longer `wanted`
/// (a preview stepping through skins): between its stages, between
/// images, and inside the skin's Lua, which its clock ends
/// ([`load_clock`]). A load given up never counts against the skin's
/// Lua.
pub(crate) async fn prepare_while(
    key: &str,
    layout: &Layout,
    env: Env,
    wanted: Wanted,
) -> Option<Prepared> {
    let done = Rc::new(Cell::new(false));
    let prepared = prepare_until(key, layout, env, &wanted, &done).await;
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

async fn prepare_until(
    key: &str,
    layout: &Layout,
    env: Env,
    wanted: &Wanted,
    done: &Rc<Cell<bool>>,
) -> Option<Prepared> {
    let clock = || load_clock(wanted.clone(), done.clone());
    // A bundled skin plays without storage.
    let db = Db::open().await.ok();
    let mut stored = catalogue(db.as_ref()).await;
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
    let chain: Vec<usize> = {
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
    // First pass: which images the actors use.
    let mut sizes = ProbeSizes::probe();
    if with_lua {
        match NoteskinPlay::load_lua(
            &folders,
            &chosen.game,
            &chosen.name,
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
    let mut pixels: u64 = 0;
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
        &mut pixels,
        wanted,
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
            &chosen.name,
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
                    &mut pixels,
                    wanted,
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
    let Some(play) = play else {
        for b in bitmaps.values() {
            b.close();
        }
        return None;
    };
    let images = play.images_for(&bitmaps);
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
