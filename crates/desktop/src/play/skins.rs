//! A play's noteskin readied natively (`docs/plans/desktop.md`, section
//! 3): the skin chosen in the settings (the bundled ones, those in the
//! song folders' `NoteSkins/`, `crate::library::noteskins`), built for the
//! chart's layout through its Lua when it has some (`ddi-noteskin`'s
//! loader, `lunita`) or natively, its images decoded (PNG and the rest
//! through `image`, SVG through `ddi_noteskin::raster`, several at once).
//! The web's `noteskins.rs` does the same with the browser's decoders and
//! its SVG worker, which draws as this does; the two passes (which images the
//! actors use, then the actors with the images' sizes) are its.

use std::collections::HashMap;
use std::path::Path;

use ddi_chart::Layout;
use ddi_noteskin::build::{Images, MAX_IMAGE_SIDE, MAX_PIXELS, ProbeSizes};
use ddi_noteskin::bundled::Index;
use ddi_noteskin::import::{MAX_TEXT, MAX_TEXTS};
use ddi_noteskin::lua::Env;
use ddi_noteskin::report::{ImageCost, ImageReport, Outcome};
use ddi_noteskin::{Folder, Skin};
use ddi_render::noteskin::{NoteskinPlay, SameDocuments};

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
    /// What its images cost (`docs/plans/noteskin-svg.md`, step 3).
    pub report: ImageReport,
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
    decode_bytes(&bytes, is_svg(path))
}

fn is_svg(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
}

/// [`decode_image`] of a file's bytes.
fn decode_bytes(bytes: &[u8], svg: bool) -> Result<(Rgba, (u32, u32)), String> {
    if svg {
        return decode_svg(bytes);
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

/// The scale skins' SVGs are drawn at ([`ddi_noteskin::build::svg_scale`]
/// for the window's screen, [`set_screen_height`]), as `f64` bits; four
/// times their size until a window says (tests, tools).
static SVG_SCALE_BITS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(ddi_noteskin::build::SVG_SCALE.to_bits());

/// The window's screen is `height` device pixels tall: skins loaded from
/// now on draw their SVGs for it (one already loaded keeps its own).
pub fn set_screen_height(height: u32) {
    SVG_SCALE_BITS.store(
        ddi_noteskin::build::svg_scale(height).to_bits(),
        std::sync::atomic::Ordering::Relaxed,
    );
}

fn svg_scale() -> f64 {
    f64::from_bits(SVG_SCALE_BITS.load(std::sync::atomic::Ordering::Relaxed))
}

/// An SVG checked (`ddi_noteskin::svg`) and rasterised at the scale the
/// screen needs ([`set_screen_height`]), as the web's SVG worker draws it
/// (`ddi_noteskin::raster`).
pub fn decode_svg(bytes: &[u8]) -> Result<(Rgba, (u32, u32)), String> {
    let d = ddi_noteskin::raster::decode(bytes, svg_scale())?;
    Ok((
        Rgba {
            size: [d.size.0, d.size.1],
            data: d.pixels,
        },
        d.own,
    ))
}

/// The pixels decoding the image at `path` keeps ([`decode_image`]'s
/// size), read from its header or, for an SVG, its root; `None` when it
/// cannot be read (decoding it would fail too).
fn image_pixels(path: &Path) -> Option<u64> {
    let (w, h) = if is_svg(path) {
        let bytes = std::fs::read(path).ok()?;
        if bytes.len() > ddi_noteskin::svg::MAX_SVG {
            return None;
        }
        ddi_noteskin::svg::check_svg(std::str::from_utf8(&bytes).ok()?)
            .ok()?
            .raster_size(svg_scale())
    } else {
        let size = image::ImageReader::open(path)
            .ok()?
            .with_guessed_format()
            .ok()?
            .into_dimensions()
            .ok()?;
        ddi_noteskin::svg::raster_size(size, 1.0)
    };
    Some(u64::from(w) * u64::from(h))
}

/// Where an image to decode comes from: a file of the skin, or a
/// component's part made from one (`ddi_noteskin::component::part_svg`).
enum ImageSource {
    File(std::path::PathBuf),
    Svg(String),
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
    /// Parts' documents drawn, each once.
    documents: SameDocuments,
    pixels: u64,
    report: ImageReport,
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
        documents: SameDocuments::default(),
        pixels: 0,
        report: ImageReport::default(),
    };
    r.decode();
    Some(r)
}

impl Read {
    /// The images the last pass asked for, decoded (the skin's pixels
    /// capped): admitted in turn while the skin's pixels are under
    /// [`MAX_PIXELS`] (each one's from its header), then decoded on
    /// several threads.
    fn decode(&mut self) {
        // Component files: their components read, every part drawn as an
        // image of its own (`ddi_noteskin::component`).
        let mut generated: Vec<(usize, String, String)> = Vec::new();
        for (i, path) in self.sizes.take_asked_components() {
            let read = self
                .dirs
                .get(i)
                .and_then(|dir| ddi_library::path::RelPath::new(&path).to_path(dir))
                .ok_or_else(|| "its path leaves its folder".to_string())
                .and_then(|file| std::fs::read_to_string(file).map_err(|e| e.to_string()));
            let found = read.and_then(|text| {
                let read = ddi_noteskin::component::part_documents(&path, &text)?;
                generated.extend(read.documents.into_iter().map(|(p, doc)| (i, p, doc)));
                Ok(read.components)
            });
            if let Err(e) = &found {
                eprintln!(
                    "noteskin {}: {path}: {e}",
                    self.names.get(i).map_or("", |n| n)
                );
            }
            self.sizes.components.insert((i, path), found);
        }
        let mut admitted = Vec::new();
        // Parts whose documents another's is: `(part, that one's)`.
        let mut copies = Vec::new();
        let asked = self.sizes.take_asked();
        let generated = generated.into_iter().map(|(i, p, doc)| (i, p, Some(doc)));
        for (i, path, doc) in asked
            .into_iter()
            .map(|(i, p)| (i, p, None))
            .chain(generated)
        {
            if self.decoded.contains_key(&(i, path.clone())) || self.documents.is_copy(i, &path) {
                continue;
            }
            let (Some(dir), Some(name)) = (self.dirs.get(i), self.names.get(i)) else {
                continue;
            };
            let mut cost = ImageCost::left_out(name, &path);
            // A part of a document drawn already costs nothing more.
            if let Some(first) = doc
                .as_ref()
                .and_then(|d| self.documents.first_of(i, &path, d))
            {
                cost.svg = true;
                self.report.images.push(cost);
                copies.push((i, path, first, self.report.images.len() - 1));
                continue;
            }
            if self.pixels >= MAX_PIXELS {
                eprintln!("noteskin {name}: {path} left out: its images are too large together");
                self.report.images.push(cost);
                continue;
            }
            // A gradient's ramp a material samples: drawn from its file.
            let doc = doc.or_else(|| {
                let (file, _) = ddi_noteskin::component::split_ramp_path(&path)?;
                let text = ddi_library::path::RelPath::new(file)
                    .to_path(dir)
                    .and_then(|f| std::fs::read_to_string(f).ok());
                match text.map(|t| ddi_noteskin::component::derived_svg(&path, &t)) {
                    Some(Some(Ok(doc))) => Some(doc),
                    other => {
                        let why = match other {
                            Some(Some(Err(e))) => e,
                            _ => "its file could not be read".into(),
                        };
                        eprintln!("noteskin {name}: {path}: {why}");
                        None
                    }
                }
            });
            let source = match doc {
                Some(doc) => {
                    cost.svg = true;
                    if let Ok(c) = ddi_noteskin::svg::check_svg(&doc) {
                        let (w, h) = c.raster_size(svg_scale());
                        self.pixels += u64::from(w) * u64::from(h);
                    }
                    ImageSource::Svg(doc)
                }
                None => {
                    let Some(file) = ddi_library::path::RelPath::new(&path).to_path(dir) else {
                        eprintln!("noteskin {name}: {path}: path leaves its folder");
                        cost.outcome = Outcome::Failed("its path leaves its folder".into());
                        self.report.images.push(cost);
                        continue;
                    };
                    if let Some(pixels) = image_pixels(&file) {
                        self.pixels += pixels;
                    }
                    ImageSource::File(file)
                }
            };
            self.report.images.push(cost);
            admitted.push((i, path, source, self.report.images.len() - 1));
        }
        let next = std::sync::atomic::AtomicUsize::new(0);
        let threads = std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .clamp(1, 8)
            .min(admitted.len());
        let mut decoded: Vec<_> = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..threads)
                .map(|_| {
                    scope.spawn(|| {
                        let mut out = Vec::new();
                        loop {
                            let n = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let Some((_, _, source, _)) = admitted.get(n) else {
                                return out;
                            };
                            let (bytes, svg) = match source {
                                ImageSource::File(file) => (std::fs::read(file), is_svg(file)),
                                ImageSource::Svg(doc) => (Ok(doc.as_bytes().to_vec()), true),
                            };
                            let hash = bytes
                                .as_ref()
                                .map(|b| ddi_noteskin::report::hash(b))
                                .unwrap_or_default();
                            // A decoder that panics on a broken file
                            // fails that file only.
                            let result = bytes.map_err(|e| e.to_string()).and_then(|b| {
                                std::panic::catch_unwind(|| decode_bytes(&b, svg))
                                    .unwrap_or_else(|_| Err("its decoder failed".into()))
                            });
                            out.push((n, hash, result));
                        }
                    })
                })
                .collect();
            workers
                .into_iter()
                .flat_map(|w| w.join().unwrap_or_default())
                .collect()
        });
        decoded.sort_by_key(|(n, _, _)| *n);
        for (n, hash, result) in decoded {
            let (i, path, _, at) = &admitted[n];
            let cost = &mut self.report.images[*at];
            cost.hash = hash;
            match result {
                Ok((rgba, size)) => {
                    cost.own = size;
                    cost.decoded = (rgba.size[0], rgba.size[1]);
                    cost.outcome = Outcome::Kept;
                    self.sizes.sizes.insert((*i, path.clone()), size);
                    self.decoded.insert((*i, path.clone()), rgba);
                }
                Err(e) => {
                    eprintln!("noteskin {}: {path}: {e}", self.names[*i]);
                    cost.outcome = Outcome::Failed(e);
                }
            }
        }
        // A part of a document drawn already: as that one came out, its
        // image that one's (`NoteskinPlay::take_images`).
        for (i, path, first, at) in copies {
            let name = self.names.get(i).map_or("", String::as_str);
            if self.report.copy_outcome(at, name, &first)
                && let Some(size) = self.sizes.sizes.get(&(i, first)).copied()
            {
                self.sizes.sizes.insert((i, path), size);
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
    let mut play = play?;
    let (mut images, same) = play.take_images(&mut r.decoded, &r.documents.same);
    share_images(&mut play, &mut images, &r.names, &mut r.report, same);
    for n in &play.notes {
        eprintln!("noteskin {}: {n}", r.name);
    }
    eprintln!("noteskin {}: {}", r.name, r.report.summary());
    Some(Prepared {
        load_error: r.lua_error,
        key: r.key,
        strike: r.strike,
        play,
        images,
        report: r.report,
    })
}

/// Lets each image whose pixels an earlier one has go, its texture given
/// that one's (`NoteskinPlay::share_textures`), and says so in `report`
/// for those and for `same`, the textures of one document
/// (`NoteskinPlay::take_images`).
fn share_images(
    play: &mut NoteskinPlay,
    images: &mut [Option<Rgba>],
    names: &[String],
    report: &mut ImageReport,
    same: Vec<(usize, usize)>,
) {
    let pixels = play.share_textures(
        |i| {
            let image = images.get(i)?.as_ref()?;
            Some(ddi_noteskin::report::pixels_hash(image.size, &image.data))
        },
        |a, b| match (&images[a], &images[b]) {
            (Some(a), Some(b)) => a.size == b.size && a.data == b.data,
            _ => false,
        },
    );
    let keys = play.texture_keys();
    let path = |i: usize| match keys.get(i) {
        Some(ddi_noteskin::build::TextureKey::Skin {
            dir: ddi_noteskin::Dir::Folder(f),
            path,
        }) => Some((names.get(*f)?.as_str(), path.as_str())),
        _ => None,
    };
    for (i, with) in same.into_iter().chain(pixels) {
        images[i] = None;
        if let (Some((skin, path)), Some((_, with))) = (path(i), path(with)) {
            report.share(skin, path, with);
        }
    }
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
