//! A song read for a play, off the event loop's thread: its simfile
//! parsed, its music decoded and resampled to the output's rate, its
//! files found where the library has them (a bundled song's folder, a
//! song folder, a zip read in place).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use ddi_chart::Song;
use ddi_library::manifest::ManifestEntry;
use ddi_library::path::{RelPath, RelPathBuf};
use ddi_library::scan::Reader;
use ddi_platform::decode_all::Range;

use crate::audio::Sound;
use crate::library::LibSong;
use crate::library::files::{DirReader, ZipReader};

/// Where a song's files are.
#[derive(Clone)]
pub enum Files {
    /// A folder: the song's own (a bundled song's, a song folder's).
    Dir { root: PathBuf, dir: RelPathBuf },
    /// A zip read in place.
    Zip {
        zip: PathBuf,
        reader: Arc<ZipReader>,
        dir: RelPathBuf,
    },
}

impl Files {
    /// A song of the library, or a bundled one.
    pub fn of(id: &str) -> Option<(Files, ManifestEntry)> {
        if let Some(s) = crate::library::song(id) {
            return Some((Files::lib(&s).ok()?, s.entry));
        }
        let entry = crate::library::bundled().into_iter().find(|e| e.id == id)?;
        let root = crate::images::assets().join("songs").join(id);
        Some((
            Files::Dir {
                root,
                dir: RelPathBuf::new(),
            },
            entry,
        ))
    }

    fn lib(s: &LibSong) -> Result<Files, String> {
        Ok(if s.zip {
            Files::Zip {
                zip: s.root.clone(),
                reader: Arc::new(ZipReader::open(&s.root, &crate::library::legacy_text)?),
                dir: s.dir.clone(),
            }
        } else {
            Files::Dir {
                root: s.root.clone(),
                dir: s.dir.clone(),
            }
        })
    }

    /// One of the song's files (relative to its folder) relative to the
    /// root.
    fn rooted(&self, file: &str) -> RelPathBuf {
        let dir = match self {
            Files::Dir { dir, .. } | Files::Zip { dir, .. } => dir,
        };
        let path = dir.join(file);
        path.normalize().unwrap_or(path)
    }

    /// The song's folder, relative to the root.
    pub fn dir(&self) -> &ddi_library::path::RelPath {
        match self {
            Files::Dir { dir, .. } | Files::Zip { dir, .. } => dir,
        }
    }

    /// A file of the song, whole.
    pub fn read(&self, file: impl AsRef<str>) -> Result<Vec<u8>, String> {
        let file = file.as_ref();
        match self {
            Files::Dir { root, .. } => DirReader { root: root.clone() }.read(&self.rooted(file)),
            Files::Zip { reader, .. } => reader.read(&self.rooted(file)),
        }
    }

    /// Movie `name` of the song: a folder's file, a zip's stored entry
    /// read in place, a compressed one extracted once into the cache
    /// folder (`movies/`, kept to [`MOVIE_CACHE`] bytes).
    pub fn movie(&self, name: impl AsRef<str>) -> Result<MovieAt, String> {
        let name = name.as_ref();
        self.movie_unless(name, &std::sync::atomic::AtomicBool::new(false))
    }

    /// [`Files::movie`], an extraction given up once `cancel` is set (the
    /// song's prompt went).
    pub fn movie_unless(
        &self,
        name: impl AsRef<str>,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<MovieAt, String> {
        let name = name.as_ref();
        match self {
            Files::Dir { root, .. } => Ok((
                self.rooted(name)
                    .to_path(root)
                    .ok_or("path leaves its root")?,
                None,
            )),
            Files::Zip { zip, reader, .. } => {
                let entry = self.rooted(name);
                if let Some(range) = reader.stored_range(&entry)? {
                    return Ok((zip.clone(), Some(range)));
                }
                let (crc, size) = reader
                    .content_id(&entry)
                    .ok_or_else(|| format!("{name}: not in the zip"))?;
                let dir = crate::paths::cache_dir()
                    .ok_or("no cache folder")?
                    .join("movies");
                let ext = entry
                    .as_str()
                    .rsplit_once('.')
                    .map_or(String::new(), |(_, e)| {
                        format!(".{}", e.to_ascii_lowercase())
                    });
                let key = format!("{}\0{entry}\0{crc:08x}\0{size}", zip.display());
                let path = dir.join(format!(
                    "{:016x}{ext}",
                    ddi_library::pack::fnv1a64(key.as_bytes())
                ));
                if std::fs::metadata(&path).is_ok_and(|m| m.len() == size) {
                    // Used again: the newest for the cache's trimming.
                    let _ = std::fs::File::options()
                        .append(true)
                        .open(&path)
                        .and_then(|f| f.set_modified(std::time::SystemTime::now()));
                    return Ok((path, None));
                }
                if size > MOVIE_CACHE {
                    return Err(format!(
                        "{name}: too large to keep extracted ({} MB)",
                        size >> 20
                    ));
                }
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                trim_movies(&dir, MOVIE_CACHE.saturating_sub(size));
                // A part of its own: two loads of the song may extract it
                // at once.
                static PARTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                let n = PARTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let part = path.with_extension(format!("{}-{n}.part", std::process::id()));
                let written = std::fs::File::create(&part)
                    .map_err(|e| e.to_string())
                    .and_then(|f| {
                        let mut out = std::io::BufWriter::new(Cancellable { out: f, cancel });
                        reader.extract_to(&entry, &mut out)?;
                        std::io::Write::flush(&mut out).map_err(|e| e.to_string())
                    });
                if let Err(e) = written {
                    let _ = std::fs::remove_file(&part);
                    return Err(format!("{name}: {e}"));
                }
                if let Err(e) = std::fs::rename(&part, &path) {
                    let _ = std::fs::remove_file(&part);
                    // Another load got there first.
                    if !std::fs::metadata(&path).is_ok_and(|m| m.len() == size) {
                        return Err(format!("{name}: {e}"));
                    }
                }
                Ok((path, None))
            }
        }
    }

    /// A file by its path relative to the root (a work's files are named
    /// so), whole.
    pub fn read_at_root(&self, path: impl AsRef<str>) -> Result<Vec<u8>, String> {
        let path = path.as_ref();
        let path = RelPath::new(path);
        match self {
            Files::Dir { root, .. } => DirReader { root: root.clone() }.read(path),
            Files::Zip { reader, .. } => reader.read(path),
        }
    }

    /// A file by its path relative to the root, as a texture source.
    pub fn source_at_root(&self, path: impl AsRef<str>) -> String {
        let path = path.as_ref();
        let path = RelPath::new(path);
        let path = path.normalize().unwrap_or_else(|| path.to_owned());
        match self {
            Files::Dir { root, .. } => path
                .to_path(root)
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            Files::Zip { zip, .. } => format!("zip:{}!{path}", zip.display()),
        }
    }

    /// A file of the song as a texture source (`crate::images::load`).
    pub fn source(&self, file: impl AsRef<str>) -> String {
        let file = file.as_ref();
        match self {
            Files::Dir { root, .. } => self
                .rooted(file)
                .to_path(root)
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            Files::Zip { zip, .. } => format!("zip:{}!{}", zip.display(), self.rooted(file)),
        }
    }
}

/// Where a movie's bytes are: the file, and the range of it
/// (`(offset, length)`; `None`: all of it).
pub type MovieAt = (PathBuf, Option<(u64, u64)>);

/// The most the extracted movies keep of the cache folder.
pub const MOVIE_CACHE: u64 = 2 << 30;

/// The oldest extracted movies in `dir` removed until they hold at most
/// `keep` bytes.
/// A writer that fails once `cancel` is set.
struct Cancellable<'a, W> {
    out: W,
    cancel: &'a std::sync::atomic::AtomicBool,
}

impl<W: std::io::Write> std::io::Write for Cancellable<'_, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(std::io::Error::other("given up"));
        }
        self.out.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
    }
}

/// Parts of extractions left behind (a run that stopped mid-way), an
/// hour old or more: gone (the start of a run).
pub fn clean_stale_parts() {
    let Some(dir) = crate::paths::cache_dir().map(|d| d.join("movies")) else {
        return;
    };
    let Ok(read) = std::fs::read_dir(&dir) else {
        return;
    };
    for e in read.flatten() {
        let p = e.path();
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() >= 3600);
        if old && p.extension().is_some_and(|x| x == "part") {
            let _ = std::fs::remove_file(p);
        }
    }
}

fn trim_movies(dir: &std::path::Path, keep: u64) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = read
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            let modified = m.modified().ok()?;
            // Another extraction's part is not the cache's.
            let part = e.path().extension().is_some_and(|x| x == "part");
            (m.is_file() && !part).then(|| (modified, m.len(), e.path()))
        })
        .collect();
    files.sort();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    for (_, len, path) in files {
        if total <= keep {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= len;
        }
    }
}

/// The song's movies that play, where each is (or why it cannot be had).
pub fn movies(files: &Files, entry: &ManifestEntry) -> Vec<(String, Result<MovieAt, String>)> {
    movies_unless(files, entry, &std::sync::atomic::AtomicBool::new(false))
}

/// [`movies`], extractions given up once `cancel` is set.
pub fn movies_unless(
    files: &Files,
    entry: &ManifestEntry,
    cancel: &std::sync::atomic::AtomicBool,
) -> Vec<(String, Result<MovieAt, String>)> {
    entry
        .bg_videos
        .iter()
        .filter(|v| v.plays())
        .map(|v| (v.name.to_string(), files.movie_unless(&v.name, cancel)))
        .collect()
}

/// A song read for a play.
pub struct Loaded {
    pub entry: ManifestEntry,
    pub song: Song,
    pub files: Files,
    pub music: Sound,
    /// Its movies, when asked for ([`movies`]).
    pub movies: Vec<(String, Result<MovieAt, String>)>,
    /// A work's back and mask images: `(name as its data writes it,
    /// texture source)`.
    pub layers: Vec<(String, String)>,
}

/// Reads song `id` and decodes its music at `rate` Hz, its movies found.
pub fn load(id: &str, rate: u32) -> Result<Loaded, String> {
    load_with(
        id,
        None,
        rate,
        true,
        &std::sync::atomic::AtomicBool::new(false),
    )
}

/// [`load`]; with `chart`, the music that chart plays (`.ssc` per-chart
/// `#MUSIC`, `ManifestEntry::music_for`; else the song's); with
/// `with_movies`, its movies found (a zip's compressed ones extracted).
pub fn load_with(
    id: &str,
    chart: Option<usize>,
    rate: u32,
    with_movies: bool,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Loaded, String> {
    let (files, entry) = Files::of(id).ok_or_else(|| format!("no song {id}"))?;
    if let Some(w) = crate::library::song(id).and_then(|s| s.work) {
        return load_work(files, entry, &w, rate);
    }
    let bytes = files.read(&entry.chart)?;
    let text = ddi_library::pack::decode_text_with(&bytes, crate::library::legacy_text);
    let ext = entry
        .chart
        .as_str()
        .rsplit_once('.')
        .map_or("sm", |(_, e)| e)
        .to_ascii_lowercase();
    let mut song = ddi_library::scan::parse_simfile(&text, &ext).map_err(|e| e.to_string())?;
    song.resolve_change_names(&entry.change_names());
    add_lyrics(&files, &entry, &mut song);
    let music_file = chart.map_or(entry.music.as_rel_path(), |c| entry.music_for(c));
    let music = files.read(music_file)?;
    let music_ext = music_file
        .as_str()
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase());
    // A Dancing☆Onigiri work's MP3 where Chrome's decode has it (its
    // notes are timed against that), a simfile's where StepMania's does.
    let range = Range {
        target: ddi_session::music_target(&song),
        ..Range::default()
    };
    let music = crate::audio::decoder::sound(music, music_ext.as_deref(), range, rate)?;
    let movies = if with_movies {
        movies_unless(&files, &entry, cancel)
    } else {
        Vec::new()
    };
    Ok(Loaded {
        entry,
        song,
        files,
        music,
        movies,
        layers: Vec::new(),
    })
}

/// Song `id`'s chart alone, no music (a replay's chart looked for).
pub fn read_chart(id: &str) -> Result<Song, String> {
    let (files, entry) = Files::of(id).ok_or_else(|| format!("no song {id}"))?;
    if let Some(w) = crate::library::song(id).and_then(|s| s.work) {
        return read_work(&files, &w);
    }
    let bytes = files.read(&entry.chart)?;
    let text = ddi_library::pack::decode_text_with(&bytes, crate::library::legacy_text);
    let ext = entry
        .chart
        .as_str()
        .rsplit_once('.')
        .map_or("sm", |(_, e)| e)
        .to_ascii_lowercase();
    ddi_library::scan::parse_simfile(&text, &ext).map_err(|e| e.to_string())
}

/// The song's `.lrc` lyrics (`ManifestEntry::lyrics`), read and kept on
/// it (`ddi_chart::formats::lrc`); a play shows them without when the
/// file cannot be read.
fn add_lyrics(files: &Files, entry: &ManifestEntry, song: &mut Song) {
    let Some(file) = &entry.lyrics else {
        return;
    };
    match files.read(file) {
        Ok(bytes) => ddi_chart::formats::lrc::add_lyrics(
            song,
            &ddi_library::pack::decode_text_with(&bytes, crate::library::legacy_text),
        ),
        Err(e) => eprintln!("lyrics {file}: {e}"),
    }
}

/// A work's song read again from where the library found it: its page
/// and external dos, or its dump's effective dos.
pub fn read_work(files: &Files, w: &crate::library::WorkRef) -> Result<Song, String> {
    struct AtRoot<'a>(&'a Files);
    impl Reader for AtRoot<'_> {
        fn read(&self, path: &RelPath) -> Result<Vec<u8>, String> {
            self.0.read_at_root(path)
        }
    }
    let reader = AtRoot(files);
    let work = if w.dump {
        let (effective, _) = w.dos.as_ref().ok_or("the dump has no effective dos")?;
        let text = ddi_library::pack::decode_text(&reader.read(effective)?);
        ddi_library::works::Work {
            dos: ddi_library::danoni::dump_dos(&text)?,
            music: ddi_library::works::WorkMusic::Dump(Default::default()),
        }
    } else {
        let paths: Vec<RelPathBuf> = w.dos.iter().map(|(p, _)| p.clone()).collect();
        crate::library::read_page(&reader, &paths, w.source.as_str())?
            .ok_or("the page no longer holds a work")?
            .0
    };
    ddi_library::works::song_of(&work, w.music_no)
}

/// A Dancing☆Onigiri work's song, read again from where the library
/// found it (the web reads its stored work): its page and external dos,
/// or its dump's effective dos; its music (a script's decoded), lined up
/// with Chrome's decode; its layers' images.
fn load_work(
    files: Files,
    entry: ManifestEntry,
    w: &crate::library::WorkRef,
    rate: u32,
) -> Result<Loaded, String> {
    struct AtRoot<'a>(&'a Files);
    impl Reader for AtRoot<'_> {
        fn read(&self, path: &RelPath) -> Result<Vec<u8>, String> {
            self.0.read_at_root(path)
        }
    }
    let song = read_work(&files, w)?;
    let reader = AtRoot(&files);
    let mut music = reader.read(&w.music)?;
    if w.encoded {
        let js = ddi_library::pack::decode_text(&music);
        music = ddi_library::danoni::decode_music(&js).ok_or("its music script holds no audio")?;
    }
    let (ext, _) = ddi_library::works::sniff_audio(&music);
    let range = Range {
        target: ddi_session::music_target(&song),
        ..Range::default()
    };
    let music = crate::audio::decoder::sound(music, Some(ext), range, rate)?;
    let layers = w
        .layers
        .iter()
        .map(|(name, path)| (name.clone(), files.source_at_root(path)))
        .collect();
    Ok(Loaded {
        entry,
        song,
        files,
        music,
        movies: Vec::new(),
        layers,
    })
}

/// A load on its own thread.
pub struct Loading {
    pub id: String,
    rx: Receiver<Result<Loaded, String>>,
    /// Set when the load is let go (its prompt went): a movie being
    /// extracted is given up.
    cancel: Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for Loading {
    fn drop(&mut self) {
        self.cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Loading {
    /// Starts reading song `id` (chart `chart`'s music at `rate` Hz,
    /// movies when `with_movies`); `wake` wakes the event loop when it is done.
    pub fn start(
        id: &str,
        chart: Option<usize>,
        rate: u32,
        with_movies: bool,
        wake: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Loading {
        let (tx, rx) = mpsc::channel();
        let job = id.to_string();
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancelled = cancel.clone();
        let _ = std::thread::Builder::new()
            .name("ddi-play-load".into())
            .spawn(move || {
                let r = std::panic::catch_unwind(|| {
                    load_with(&job, chart, rate, with_movies, &cancelled)
                })
                .unwrap_or_else(|_| Err("the song could not be read".into()));
                let _ = tx.send(r);
                if let Some(w) = wake {
                    w();
                }
            });
        Loading {
            id: id.to_string(),
            rx,
            cancel,
        }
    }

    /// The song, once read.
    pub fn poll(&self) -> Option<Result<Loaded, String>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err("the song could not be read".into())),
        }
    }

    /// Waits for it (tests).
    pub fn wait(self) -> Result<Loaded, String> {
        self.rx
            .recv()
            .unwrap_or_else(|_| Err("the song could not be read".into()))
    }
}

/// Calibration test `mode` as a song: its generated chart and click
/// track (at `rate` Hz), no files.
pub fn calibration(mode: ddi_session::calibration::CalMode, rate: u32) -> Loaded {
    let song = ddi_session::calibration::song(mode);
    let data = ddi_session::calibration::click_samples(rate as f32, &song);
    let music = Arc::new(crate::audio::mixer::Pcm {
        rate,
        channels: 1,
        data,
    });
    let entry = ManifestEntry {
        variants_version: 0,
        id: format!("calibration:{}", mode.id()),
        title: mode.title().into(),
        artist: String::new(),
        chart: RelPathBuf::new(),
        music: RelPathBuf::new(),
        banner: None,
        background: None,
        jacket: None,
        cd_title: None,
        lyrics: None,
        preview: None,
        bg_images: Vec::new(),
        bg_videos: Vec::new(),
        bg_shared: Vec::new(),
        bg_scripted: Vec::new(),
        bg_scripts: Vec::new(),
        layer_images: Vec::new(),
        credit: String::new(),
        bpm: format!("{}", ddi_session::calibration::BPM),
        preview_start: 0.0,
        preview_length: 0.0,
        length: 0.0,
        charts: Vec::new(),
        layouts: Vec::new(),
        title_translit: String::new(),
        artist_translit: String::new(),
    };
    Loaded {
        entry,
        song,
        files: Files::Dir {
            root: PathBuf::new(),
            dir: RelPathBuf::new(),
        },
        music,
        movies: Vec::new(),
        layers: Vec::new(),
    }
}
