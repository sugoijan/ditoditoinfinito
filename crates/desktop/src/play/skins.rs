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
use ddi_noteskin::build::{Images, MAX_IMAGE_SIDE, MAX_PIXELS, ProbeSizes, SVG_SCALE};
use ddi_noteskin::bundled::Index;
use ddi_noteskin::import::{MAX_TEXT, MAX_TEXTS};
use ddi_noteskin::lua::Env;
use ddi_noteskin::{Folder, Skin};
use ddi_render::noteskin::NoteskinPlay;

use super::view::Rgba;
use crate::library::noteskins::{DiskSkin, read_index};

/// A skin ready for a play.
pub struct Prepared {
    /// Authoring tools retain their last valid preview when Lua falls back after an error.
    pub load_error: Option<String>,
    pub key: String,
    /// What a Lua stop counts against ([`DiskSkin::strike_key`]).
    pub strike: String,
    pub play: NoteskinPlay,
    /// Its textures' images, by texture index (`None`: built-in art).
    pub images: Vec<Option<Rgba>>,
}

/// A skin's folder as the loader reads it: its files and bounded texts.
/// Authoring tools use the same listing for resolution and coverage reports.
pub fn folder(skin: &DiskSkin) -> Folder {
    let index = if skin.bundled { read_index() } else { None };
    folder_in(skin, index.as_ref())
}

/// The folders of `skins`, each as [`folder`] reads it, the bundled
/// skins' index read once.
pub fn folders(skins: &[DiskSkin]) -> Vec<Folder> {
    let index = if skins.iter().any(|s| s.bundled) {
        read_index()
    } else {
        None
    };
    skins.iter().map(|s| folder_in(s, index.as_ref())).collect()
}

/// [`folder`], with the bundled skins' `index` when it could be read.
fn folder_in(skin: &DiskSkin, index: Option<&Index>) -> Folder {
    // A bundled skin's are in the index.
    if skin.bundled
        && let Some(f) = index.and_then(|i| bundled_folder(i, skin))
    {
        return f;
    }
    let files: Vec<String> = crate::library::files::walk(&skin.dir)
        .listed
        .into_iter()
        .map(|l| l.path.into_string())
        .collect();
    let mut texts = Vec::new();
    let mut total = 0;
    for f in &files {
        if !ddi_noteskin::import::kept_text(f) {
            continue;
        }
        let Some(path) = ddi_library::path::RelPath::new(f).to_path(&skin.dir) else {
            continue;
        };
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
fn bundled_folder(index: &Index, skin: &DiskSkin) -> Option<Folder> {
    index
        .skins
        .iter()
        .find(|b| b.folder.game == skin.game && b.folder.name == skin.name)
        .map(|b| b.folder.clone())
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
        return decode_svg(&bytes);
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
pub fn decode_svg(bytes: &[u8]) -> Result<(Rgba, (u32, u32)), String> {
    if bytes.len() > ddi_noteskin::svg::MAX_SVG {
        return Err("the SVG is too large".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "the SVG is not text")?;
    let size = ddi_noteskin::svg::check(text)?;
    let longest = size.0.max(size.1) as f32;
    let scale = (SVG_SCALE as f32).min(MAX_IMAGE_SIDE as f32 / longest.max(1.0));
    let (w, h) = (
        ((size.0 as f32 * scale).round() as u32).max(1),
        ((size.1 as f32 * scale).round() as u32).max(1),
    );
    let data = rasterize_svg(text, w, h)?;
    Ok((Rgba { size: [w, h], data }, size))
}

/// SVG `text` drawn at `w`×`h` pixels (stretched to them): RGBA rows,
/// not premultiplied.
pub fn rasterize_svg(text: &str, w: u32, h: u32) -> Result<Vec<u8>, String> {
    let tree = resvg::usvg::Tree::from_str(text, &resvg::usvg::Options::default())
        .map_err(|e| e.to_string())?;
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
    Ok(data)
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

/// Builds skin `name` through its Lua for `layout` (the shells' way,
/// [`NoteskinPlay::load_lua`], with no pause: run to its end on this
/// thread); why it could not.
fn lua_play(
    folders: &[Folder],
    game: &str,
    name: &str,
    env: Env,
    seed: u64,
    layout: &Layout,
    images: &dyn Images,
) -> Result<Option<NoteskinPlay>, String> {
    crate::block_on(NoteskinPlay::load_lua(
        folders,
        game,
        name,
        env,
        seed,
        layout,
        images,
        clock(),
        None,
    ))
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
    record_failures: bool,
    sizes: ProbeSizes,
    decoded: HashMap<(usize, String), Rgba>,
    pixels: u64,
}

/// Reads skin `key` for `layout` (`env` for its Lua), among `skins`: its
/// files, and its images decoded; `None` when there is no such skin, it
/// does not load, or the layout's lanes are not its buttons. Any thread.
pub fn read(key: &str, layout: &Layout, env: Env, skins: &[DiskSkin]) -> Option<Read> {
    read_inner(key, layout, env, skins, true)
}

fn read_inner(
    key: &str,
    layout: &Layout,
    env: Env,
    skins: &[DiskSkin],
    record_failures: bool,
) -> Option<Read> {
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
    let folders = folders(&stored);
    let chosen = &stored[0];
    // The skin its parameters pick (a color scheme's), loaded in its place
    // (`ddi_noteskin::params`).
    let skin = match ddi_noteskin::params::head(&folders, &chosen.game, &chosen.name, &env.params) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("noteskin {key}: {e}");
            return None;
        }
    };
    let head = skin.name.clone();
    // Its Lua stopped twice this run: drawn without it.
    let strike = chosen.strike_key();
    let lua_off = record_failures
        .then(|| crate::library::noteskins::lua_off(&strike))
        .flatten();
    if let Some(why) = &lua_off {
        eprintln!("noteskin {}: its Lua is off ({why})", chosen.name);
    }
    let with_lua = lua_off.is_none() && (skin.has_lua() || chosen.game == "pump");
    let mut env = env;
    env.game = chosen.game.clone();
    env.style = ddi_noteskin::lua::style_for(&chosen.game, layout).into();
    let seed = hash_seed();
    let mut lua_error = None;
    let sizes = ProbeSizes::probe();
    // First pass: which images the actors use.
    if with_lua {
        match lua_play(
            &folders,
            &chosen.game,
            &head,
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
        name: head,
        dirs: stored.iter().map(|s| s.dir.clone()).collect(),
        names: stored.iter().map(|s| s.name.clone()).collect(),
        folders,
        env,
        seed,
        with_lua,
        lua_error,
        record_failures,
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
        for (i, path) in self.sizes.take_asked() {
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
            let Some(file) = ddi_library::path::RelPath::new(&path).to_path(dir) else {
                eprintln!("noteskin {name}: {path}: path leaves its folder");
                continue;
            };
            match decode_image(&file) {
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
    if let Some(e) = &r.lua_error
        && r.record_failures
    {
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
    let images = play.images_for(&r.decoded);
    for n in &play.notes {
        eprintln!("noteskin {}: {n}", r.name);
    }
    Some(Prepared {
        load_error: r.lua_error,
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

/// The authoring preview uses the same loader but retries corrected scripts without a play's
/// persistent strike count. `Prepared::load_error` reports a fallback instead of hiding it.
pub fn prepare_authoring(
    key: &str,
    layout: &Layout,
    env: Env,
    skins: &[DiskSkin],
) -> Option<Prepared> {
    build(read_inner(key, layout, env, skins, false)?, layout)
}

/// A skin read on a thread of its own; dropped, its answer is let go (a
/// pick passed over).
/// `DDI_SKIN_DELAY_MS`: a play's skin takes this much longer to read, a
/// testing aid for the start prompt's loading in the background
/// (`docs/plans/start-prompt.md`; the web's `__DDI_SKIN_DELAY_MS`).
fn test_delay() {
    if let Some(ms) = std::env::var("DDI_SKIN_DELAY_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        std::thread::sleep(std::time::Duration::from_millis(ms.min(60_000)));
    }
}

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

    /// Starts reading the first of `wants` one can pick for `layout`
    /// ([`ddi_shell::first_there`]: a chosen skin no longer there is
    /// drawn as Options shows it), as [`Reading::start`]; none for the
    /// built-in arrows.
    pub fn first(
        wants: ddi_shell::SkinWants,
        layout: &Layout,
        wake: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    ) -> Reading {
        Reading::spawn(crate::library::all_folders(), layout, wake, |skins| {
            test_delay();
            ddi_shell::first_there(wants, &crate::library::noteskins::choices(skins))
        })
    }

    /// [`Reading::start`] among the skins of `folders`.
    pub fn among(
        key: &str,
        layout: &Layout,
        env: Env,
        folders: Vec<std::path::PathBuf>,
        wake: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    ) -> Reading {
        let key = key.to_string();
        Reading::spawn(folders, layout, wake, |_| Some((key, env)))
    }

    /// Reads the skin `pick` picks among the skins of `folders` (with
    /// what its Lua is told), on a thread.
    fn spawn(
        folders: Vec<std::path::PathBuf>,
        layout: &Layout,
        wake: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
        pick: impl FnOnce(&[DiskSkin]) -> Option<(String, Env)> + Send + 'static,
    ) -> Reading {
        let (tx, rx) = std::sync::mpsc::channel();
        let layout = layout.clone();
        let _ = std::thread::Builder::new()
            .name("ddi-noteskin".into())
            .spawn(move || {
                let skins = crate::library::noteskins::all(&folders);
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let (key, env) = pick(&skins)?;
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
