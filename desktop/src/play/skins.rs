//! A play's noteskin readied natively (`docs/plans/desktop.md`, section
//! 3): the skin chosen in the settings (the bundled ones, those in the
//! song folders' `NoteSkins/`, `crate::library::noteskins`), built for the
//! chart's layout through its Lua when it has some (`ddi-noteskin`'s
//! loader, `lunita`) or natively, its images decoded (PNG and the rest
//! through `image`, SVG through `resvg`). The web's `noteskins.rs` does
//! the same with the browser's decoders; the two passes (which images the
//! actors use, then the actors with the images' sizes) are its.

use std::collections::HashMap;
use std::path::Path;

use ddi_chart::Layout;
use ddi_noteskin::build::{Images, TextureKey};
use ddi_noteskin::lua::{Env, LoadBudget, LuaSkin};
use ddi_noteskin::{Dir, Folder, Skin};
use ddi_render::noteskin::NoteskinPlay;
use ddi_ui::state::settings::NoteColors;

use super::view::Rgba;
use crate::library::noteskins::DiskSkin;

/// The `noteskin` setting's value for the built-in arrows.
pub const BUILT_IN: &str = "builtin";
/// Dito's key (its colour variants' are `bundled:dance/dito <scheme>`).
const DITO: &str = "bundled:dance/dito";

/// The skin a play draws its layout with, by key (the web's
/// `noteskins::play_skin`): the chosen one, or Dito in its variant for
/// the note colours; none for the built-in arrows, and none instead of
/// Dito when the chart's own colours are drawn.
pub fn play_skin(choice: Option<&str>, colors: NoteColors, chart_colors: bool) -> Option<String> {
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

/// What a skin's Lua sees of the chart it is loaded for (the web's
/// `noteskins::lua_env`).
pub fn lua_env(
    song: &ddi_chart::Song,
    chart: &ddi_chart::Chart,
    layout: &Layout,
    reversed: bool,
) -> Env {
    use ddi_chart::Difficulty;
    let style = ddi_noteskin::lua::dance_style(layout);
    let difficulty = match chart.difficulty {
        Difficulty::Beginner => "Beginner",
        Difficulty::Easy => "Easy",
        Difficulty::Medium => "Medium",
        Difficulty::Hard => "Hard",
        Difficulty::Challenge => "Challenge",
        Difficulty::Edit => "Edit",
    };
    let starts = || chart.notes.iter().map(|n| n.tick.beat());
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
        reversed,
        ..Env::default()
    }
}

/// The largest image side kept, and the pixels a skin may hold (the web's).
const MAX_IMAGE_SIDE: u32 = 4096;
const MAX_PIXELS: u64 = 64 << 20;
/// How much finer than its own size an SVG is drawn (the web's).
const SVG_SCALE: f32 = 4.0;
/// The largest text read, and a skin's texts in all (the web's).
const MAX_TEXT: u64 = 1 << 20;
const MAX_TEXTS: u64 = 32 << 20;

/// A skin ready for a play.
pub struct Prepared {
    pub key: String,
    /// What a Lua stop counts against ([`DiskSkin::strike_key`]).
    pub strike: String,
    pub play: NoteskinPlay,
    /// Its textures' images, by texture index (`None`: built-in art).
    pub images: Vec<Option<Rgba>>,
}

/// A skin's folder as the loader reads it: its files and texts.
fn folder(skin: &DiskSkin) -> Folder {
    // A bundled skin's are in the index.
    if skin.bundled
        && let Some(f) = bundled_folder(skin)
    {
        return f;
    }
    let files: Vec<String> = crate::library::files::walk(&skin.dir)
        .listed
        .into_iter()
        .map(|l| l.path)
        .collect();
    let mut texts = Vec::new();
    let mut total = 0;
    for f in &files {
        if !ddi_noteskin::import::kept_text(f) {
            continue;
        }
        let path = skin.dir.join(f);
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if meta.len() > MAX_TEXT || total + meta.len() > MAX_TEXTS {
            continue;
        }
        if let Ok(bytes) = std::fs::read(&path) {
            total += meta.len();
            texts.push((f.clone(), String::from_utf8_lossy(&bytes).into_owned()));
        }
    }
    Folder {
        game: skin.game.clone(),
        name: skin.name.clone(),
        files,
        texts,
    }
}

/// A bundled skin's folder from the index.
fn bundled_folder(skin: &DiskSkin) -> Option<Folder> {
    let assets = crate::images::assets();
    let index = [
        assets.join("../target/noteskins/index.json"),
        assets.join("noteskins/index.json"),
    ]
    .iter()
    .find_map(|p| std::fs::read_to_string(p).ok())
    .and_then(|t| serde_json::from_str::<ddi_noteskin::bundled::Index>(&t).ok())?;
    index
        .skins
        .into_iter()
        .find(|b| b.folder.game == skin.game && b.folder.name == skin.name)
        .map(|b| b.folder)
}

/// Image sizes as decoded, by `(folder, path)`; the first pass answers
/// 128 for each and notes what was asked.
struct Sizes {
    sizes: HashMap<(usize, String), (u32, u32)>,
    asked: std::cell::RefCell<Vec<(usize, String)>>,
    probe: bool,
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
        MAX_IMAGE_SIDE
    }
}

/// Decodes an image of a skin: its RGBA and the size the skin sees (an
/// SVG's own, drawn at four times that).
pub fn decode_image(path: &Path) -> Result<(Rgba, (u32, u32)), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
    {
        return svg(&bytes);
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_SIDE * 4);
    limits.max_image_height = Some(MAX_IMAGE_SIDE * 4);
    limits.max_alloc = Some(512 << 20);
    reader.limits(limits);
    let img = reader.decode().map_err(|e| e.to_string())?;
    let size = (img.width(), img.height());
    let img = if size.0.max(size.1) > MAX_IMAGE_SIDE {
        let s = MAX_IMAGE_SIDE as f32 / size.0.max(size.1) as f32;
        img.resize(
            ((size.0 as f32 * s).round() as u32).max(1),
            ((size.1 as f32 * s).round() as u32).max(1),
            image::imageops::FilterType::Triangle,
        )
    } else {
        img
    };
    let rgba = img.to_rgba8();
    Ok((
        Rgba {
            size: [rgba.width(), rgba.height()],
            data: rgba.into_raw(),
        },
        size,
    ))
}

/// An SVG checked (`ddi_noteskin::svg`, the subset browsers and resvg
/// draw alike) and rasterised at [`SVG_SCALE`] times its size.
fn svg(bytes: &[u8]) -> Result<(Rgba, (u32, u32)), String> {
    if bytes.len() > ddi_noteskin::svg::MAX_SVG {
        return Err("the SVG is too large".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "the SVG is not text")?;
    let size = ddi_noteskin::svg::check(text)?;
    let tree = resvg::usvg::Tree::from_str(text, &resvg::usvg::Options::default())
        .map_err(|e| e.to_string())?;
    let longest = size.0.max(size.1) as f32;
    let scale = SVG_SCALE.min(MAX_IMAGE_SIDE as f32 / longest.max(1.0));
    let (w, h) = (
        ((size.0 as f32 * scale).round() as u32).max(1),
        ((size.1 as f32 * scale).round() as u32).max(1),
    );
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).ok_or("an empty SVG")?;
    let tree_size = tree.size();
    let transform = resvg::tiny_skia::Transform::from_scale(
        w as f32 / tree_size.width(),
        h as f32 / tree_size.height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    // tiny-skia's pixels are premultiplied; images are not.
    let mut data = pixmap.take();
    for px in data.as_chunks_mut::<4>().0 {
        let a = px[3];
        if a != 0 && a != 255 {
            for c in &mut px[..3] {
                *c = ((u32::from(*c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8;
            }
        }
    }
    Ok((Rgba { size: [w, h], data }, size))
}

/// A hash seed for a skin's VMs.
fn hash_seed() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u8(0);
    h.finish()
}

/// The host clock for the Lua's deadlines, seconds.
fn clock() -> Box<dyn FnMut() -> f64> {
    Box::new(|| crate::clock::now().0)
}

/// Builds skin `name` through its Lua for `layout`; why it could not.
fn lua_play(
    folders: &[Folder],
    game: &str,
    name: &str,
    env: Env,
    seed: u64,
    layout: &Layout,
    images: &dyn Images,
) -> Result<Option<NoteskinPlay>, String> {
    let loaded = Skin::load(folders, game, name).map_err(|e| e.to_string())?;
    if layout.lanes.iter().any(|l| loaded.lane_button(l).is_none()) {
        return Ok(None);
    }
    let settings = ddi_lua::Settings {
        hash_seed: seed,
        ..ddi_lua::Settings::default()
    };
    let lua = LuaSkin::new(
        loaded.into_owned(),
        env,
        settings,
        LoadBudget::default(),
        clock(),
    )
    .map_err(|e| e.to_string())?;
    crate::block_on(NoteskinPlay::with_lua(lua, layout, images, true, None))
        .map_err(|e| e.to_string())
}

/// Whether a skin or one it falls back on has Lua of its own.
fn has_lua(skin: &Skin<'_>) -> bool {
    !skin.legacy
        && skin.chain.iter().any(|&d| {
            skin.folder(d).is_some_and(|f| {
                f.files
                    .iter()
                    .any(|p| p.to_ascii_lowercase().ends_with(".lua"))
            })
        })
}

/// What readying a skin does off the UI thread ([`read`], `Send`): its
/// folders read, the images its actors use found (its Lua run once for
/// that), those decoded. [`build`] makes the skin from it on the UI
/// thread (a skin's Lua stays on the thread it was made on).
pub struct Read {
    key: String,
    strike: String,
    folders: Vec<Folder>,
    /// The folders on disk, in the same order.
    dirs: Vec<std::path::PathBuf>,
    names: Vec<String>,
    game: String,
    name: String,
    env: Env,
    seed: u64,
    with_lua: bool,
    lua_error: Option<String>,
    sizes: Sizes,
    decoded: HashMap<(usize, String), Rgba>,
    pixels: u64,
}

/// Reads skin `key` for `layout` (`env` for its Lua), among `skins`: its
/// files, and its images decoded; `None` when there is no such skin, it
/// does not load, or the layout's lanes are not its buttons. Any thread.
pub fn read(key: &str, layout: &Layout, env: Env, skins: &[DiskSkin]) -> Option<Read> {
    let mut stored: Vec<DiskSkin> = skins.to_vec();
    // The chosen skin first; a bundled one's fallbacks only among the
    // bundled ones, a player's among the player's, then the bundled
    // ones (the web's rule).
    let chosen = stored.iter().position(|s| s.key() == key)?;
    let first = stored.remove(chosen);
    if first.bundled {
        stored.retain(|s| s.bundled);
    }
    stored.insert(0, first);
    let folders: Vec<Folder> = stored.iter().map(folder).collect();
    let chosen = &stored[0];
    let skin = match Skin::load(&folders, &chosen.game, &chosen.name) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("noteskin {key}: {e}");
            return None;
        }
    };
    // Its Lua stopped twice this run: drawn without it.
    let strike = chosen.strike_key();
    let lua_off = crate::library::noteskins::lua_off(&strike);
    if let Some(why) = &lua_off {
        eprintln!("noteskin {}: its Lua is off ({why})", chosen.name);
    }
    let with_lua = lua_off.is_none() && (has_lua(&skin) || chosen.game == "pump");
    let mut env = env;
    env.game = chosen.game.clone();
    env.style = ddi_noteskin::lua::style_for(&chosen.game, layout).into();
    let seed = hash_seed();
    let mut lua_error = None;
    let sizes = Sizes {
        sizes: HashMap::new(),
        asked: Default::default(),
        probe: true,
    };
    // First pass: which images the actors use.
    if with_lua {
        match lua_play(
            &folders,
            &chosen.game,
            &chosen.name,
            env.clone(),
            seed,
            layout,
            &sizes,
        ) {
            Ok(Some(_)) => {}
            Ok(None) => return None,
            Err(e) => lua_error = Some(e),
        }
    }
    if !with_lua || lua_error.is_some() {
        NoteskinPlay::new(&skin, layout, &sizes, true)?;
    }
    drop(skin);
    let mut r = Read {
        key: key.to_string(),
        strike,
        game: chosen.game.clone(),
        name: chosen.name.clone(),
        dirs: stored.iter().map(|s| s.dir.clone()).collect(),
        names: stored.iter().map(|s| s.name.clone()).collect(),
        folders,
        env,
        seed,
        with_lua,
        lua_error,
        sizes,
        decoded: HashMap::new(),
        pixels: 0,
    };
    r.decode();
    Some(r)
}

impl Read {
    /// The images the last pass asked for, decoded (the skin's pixels
    /// capped).
    fn decode(&mut self) {
        let mut asked = self.sizes.asked.take();
        asked.sort();
        asked.dedup();
        for (i, path) in asked {
            if self.decoded.contains_key(&(i, path.clone())) {
                continue;
            }
            let (Some(dir), Some(name)) = (self.dirs.get(i), self.names.get(i)) else {
                continue;
            };
            if self.pixels >= MAX_PIXELS {
                eprintln!("noteskin {name}: {path} left out: its images are too large together");
                continue;
            }
            match decode_image(&dir.join(&path)) {
                Ok((rgba, size)) => {
                    self.pixels += u64::from(rgba.size[0]) * u64::from(rgba.size[1]);
                    self.sizes.sizes.insert((i, path.clone()), size);
                    self.decoded.insert((i, path), rgba);
                }
                Err(e) => eprintln!("noteskin {name}: {path}: {e}"),
            }
        }
        self.sizes.probe = false;
    }
}

/// The skin [`read`] read, made for `layout` on this thread (its Lua's
/// second pass, with the images' sizes).
pub fn build(mut r: Read, layout: &Layout) -> Option<Prepared> {
    let mut play = None;
    if r.with_lua && r.lua_error.is_none() {
        match lua_play(
            &r.folders,
            &r.game,
            &r.name,
            r.env.clone(),
            r.seed,
            layout,
            &r.sizes,
        ) {
            Ok(p) => play = p,
            Err(e) => {
                r.lua_error = Some(e);
                // Natively instead: its images found and decoded.
                r.sizes.probe = true;
                if let Ok(skin) = Skin::load(&r.folders, &r.game, &r.name) {
                    let _ = NoteskinPlay::new(&skin, layout, &r.sizes, true);
                }
                r.decode();
            }
        }
    }
    if let Some(e) = &r.lua_error {
        crate::library::noteskins::lua_stopped(
            &r.strike,
            &format!("its Lua stopped while loading ({e}); drawn without it"),
        );
    }
    if play.is_none() && (!r.with_lua || r.lua_error.is_some()) {
        let skin = Skin::load(&r.folders, &r.game, &r.name).ok()?;
        play = NoteskinPlay::new(&skin, layout, &r.sizes, true);
    }
    let play = play?;
    let images = play
        .texture_keys()
        .iter()
        .map(|k| match k {
            TextureKey::Skin {
                dir: Dir::Folder(i),
                path,
            } => r.decoded.get(&(*i, path.clone())).cloned(),
            _ => None,
        })
        .collect();
    for n in &play.notes {
        eprintln!("noteskin {}: {n}", r.name);
    }
    Some(Prepared {
        key: r.key,
        strike: r.strike,
        play,
        images,
    })
}

/// Readies skin `key` for `layout` (`env` for its Lua), among `skins`,
/// all on this thread ([`read`] then [`build`]): `None` when there is no
/// such skin, it does not load, or the layout's lanes are not its
/// buttons.
pub fn prepare(key: &str, layout: &Layout, env: Env, skins: &[DiskSkin]) -> Option<Prepared> {
    build(read(key, layout, env, skins)?, layout)
}

/// A skin read on a thread of its own; dropped, its answer is let go (a
/// pick passed over).
pub struct Reading {
    rx: std::sync::mpsc::Receiver<Option<Read>>,
}

impl Reading {
    /// Starts reading skin `key` for `layout` among the song folders'
    /// (taken here: the settings are this thread's) and the bundled
    /// skins; `wake` wakes the loop when it is done.
    pub fn start(
        key: &str,
        layout: &Layout,
        env: Env,
        wake: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    ) -> Reading {
        Reading::among(key, layout, env, crate::library::all_folders(), wake)
    }

    /// [`Reading::start`] among the skins of `folders`.
    pub fn among(
        key: &str,
        layout: &Layout,
        env: Env,
        folders: Vec<std::path::PathBuf>,
        wake: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    ) -> Reading {
        let (tx, rx) = std::sync::mpsc::channel();
        let (key, layout) = (key.to_string(), layout.clone());
        let _ = std::thread::Builder::new()
            .name("ddi-noteskin".into())
            .spawn(move || {
                let skins = crate::library::noteskins::all(&folders);
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    read(&key, &layout, env, &skins)
                }))
                .unwrap_or(None);
                if tx.send(r).is_ok()
                    && let Some(w) = wake
                {
                    w();
                }
            });
        Reading { rx }
    }

    /// The skin read, once it is (`Some(None)`: none to draw with).
    pub fn poll(&self) -> Option<Option<Read>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(None),
        }
    }
}
