//! Announcers on disk (`docs/plans/announcers.md`, step 2), read in place
//! as skins are and found as the web's import finds them
//! (`ddi_library::announcer::find_announcers`): under the `Announcers/`
//! folder of each StepMania root among the song folders (zips and
//! `.smzip`s in it too), a song folder that is itself an announcer or a
//! folder named `Announcers`, an announcer among a song folder's packs,
//! and a listed zip holding one; ours, which come with the game, from
//! the assets' `announcers/` ([`bundled`]); those made in the announcer
//! studio, from the data folder's `announcers/` ([`made`]). Keys are the
//! web's (the name in lower case; ours `bundled:` and the name), so the
//! `announcer` setting means the same announcer in both shells; the first
//! folder holding a key wins, as for skins.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ddi_library::announcer::{self, Kept, Kind};
use ddi_library::path::{RelPath, RelPathBuf};
use ddi_library::scan::Reader;

use super::files::{self, DirReader, ZipReader};

/// Where an announcer's sounds are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Base {
    /// Its folder.
    Dir(PathBuf),
    /// A zip, and its folder in it.
    Zip { zip: PathBuf, dir: RelPathBuf },
}

/// An announcer found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiskAnnouncer {
    /// Its lines' sounds, relative to its folder.
    pub kept: Kept,
    pub base: Base,
}

impl DiskAnnouncer {
    /// The `announcer` setting's value for it, as the web's.
    pub fn key(&self) -> String {
        self.kept.key()
    }

    /// A reader of its sounds.
    pub fn reader(&self) -> Result<Box<dyn Reader>, String> {
        Ok(match &self.base {
            Base::Dir(dir) => Box::new(DirReader { root: dir.clone() }),
            Base::Zip { zip, .. } => Box::new(ZipReader::open(zip, &super::legacy_text)?),
        })
    }

    /// The name [`reader`](Self::reader) reads `sound` (a path of
    /// [`Kept::lines`]) under.
    pub fn name_in_reader(&self, sound: &RelPath) -> RelPathBuf {
        match &self.base {
            Base::Dir(_) => sound.to_rel_path_buf(),
            Base::Zip { dir, .. } => dir.join(sound),
        }
    }
}

/// Whether `dir` holds at least three folders named for lines (a cheap
/// look before walking it; the walk decides).
fn looks_like_one(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| matches!(announcer::classify(n), Kind::Line(_)))
        })
        .take(3)
        .count()
        == 3
}

/// The announcers in `dir` (walked; its files named under its own name),
/// its parent their base.
fn in_dir(dir: &Path) -> Vec<DiskAnnouncer> {
    let (Some(parent), Some(name)) = (dir.parent(), dir.file_name()) else {
        return Vec::new();
    };
    let name = name.to_string_lossy();
    let listed: Vec<(String, u64)> = files::walk(dir)
        .listed
        .into_iter()
        .map(|l| (format!("{name}/{}", l.path), l.size))
        .collect();
    announcer::find_announcers(&listed, &name)
        .announcers
        .into_iter()
        .filter_map(|a| {
            let kept = a.kept();
            if kept.lines.is_empty() {
                return None;
            }
            Some(DiskAnnouncer {
                base: Base::Dir(a.dir.to_path(parent)?),
                kept,
            })
        })
        .collect()
}

/// The announcers in the zip at `path` (line folders at its root take
/// its name).
fn in_zip(path: &Path) -> Vec<DiskAnnouncer> {
    let Some(stem) = path
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(ddi_library::zip::archive_stem)
    else {
        return Vec::new();
    };
    let Ok(zip) = ZipReader::open(path, &super::legacy_text) else {
        return Vec::new();
    };
    let listed: Vec<(&str, u64)> = zip
        .names
        .iter()
        .map(|n| (n.as_str(), zip.size(n).unwrap_or(0)))
        .collect();
    announcer::find_announcers(&listed, stem)
        .announcers
        .into_iter()
        .map(|a| DiskAnnouncer {
            kept: a.kept(),
            base: Base::Zip {
                zip: path.to_path_buf(),
                dir: a.dir,
            },
        })
        .filter(|a| !a.kept.lines.is_empty())
        .collect()
}

/// The folders and zips directly in `dir`, sorted.
fn children(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

fn is_archive(path: &Path) -> bool {
    path.is_file() && super::is_zip(path)
}

fn named_announcers(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.eq_ignore_ascii_case("Announcers"))
}

/// The announcers of one song folder (see the module's doc).
pub fn in_folder(folder: &Path) -> Vec<DiskAnnouncer> {
    if is_archive(folder) {
        return in_zip(folder);
    }
    let mut out = Vec::new();
    // A folder of announcers: its own, or a StepMania root's.
    let shelf = if named_announcers(folder) {
        Some(folder.to_path_buf())
    } else {
        super::child(folder, "Announcers")
    };
    if let Some(shelf) = shelf {
        out.extend(in_dir(&shelf));
        for zip in children(&shelf).iter().filter(|p| is_archive(p)) {
            out.extend(in_zip(zip));
        }
        return out;
    }
    if looks_like_one(folder) {
        return in_dir(folder);
    }
    // One among the packs.
    for dir in children(&super::songs_dir(folder)) {
        if dir.is_dir() && looks_like_one(&dir) {
            out.extend(in_dir(&dir));
        }
    }
    out
}

/// Ours, which come with the game: the announcers in the assets'
/// `announcers/` (`tools/announcer-design/kokoro.py pack --bundle`), each
/// [`Kept::bundled`].
pub fn bundled() -> Vec<DiskAnnouncer> {
    in_assets(&crate::images::assets())
}

fn in_assets(assets: &Path) -> Vec<DiskAnnouncer> {
    let shelf = assets.join("announcers");
    if !shelf.is_dir() {
        return Vec::new();
    }
    let mut found = in_dir(&shelf);
    for a in &mut found {
        a.kept.bundled = true;
    }
    found
}

/// What the announcer studio's Use in the game writes to: the data
/// folder's `announcers/`.
pub fn made_dir() -> Option<PathBuf> {
    Some(crate::paths::data_dir()?.join("announcers"))
}

/// Those made in the announcer studio ([`made_dir`]).
pub fn made() -> Vec<DiskAnnouncer> {
    let Some(shelf) = made_dir().filter(|d| d.is_dir()) else {
        return Vec::new();
    };
    in_dir(&shelf)
}

/// Removes the announcer made in the studio under `key`; whether one
/// was.
pub fn remove_made(key: &str) -> Result<bool, String> {
    let Some(a) = made().into_iter().find(|a| a.key() == key) else {
        return Ok(false);
    };
    let Base::Dir(dir) = &a.base else {
        return Ok(false);
    };
    std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    forget();
    Ok(true)
}

/// What ours say, from the bundled announcers' index (`target/announcers/
/// index.json`, as Trunk builds it, or beside the assets in a package);
/// none when it was not generated.
pub fn index() -> Vec<announcer::Bundled> {
    let assets = crate::images::assets();
    [
        assets.join("../target/announcers/index.json"),
        assets.join("announcers/index.json"),
    ]
    .iter()
    .find_map(|p| std::fs::read_to_string(p).ok())
    .and_then(|t| serde_json::from_str::<announcer::Index>(&t).ok())
    .map_or_else(Vec::new, |i| i.announcers)
}

/// Every announcer: ours, those made in the studio, then the song
/// folders', the first holding a key winning.
pub fn all(folders: &[PathBuf]) -> Vec<DiskAnnouncer> {
    let mut out: Vec<DiskAnnouncer> = bundled();
    for a in made() {
        if !out.iter().any(|o| o.key() == a.key()) {
            out.push(a);
        }
    }
    for f in folders {
        for a in in_folder(f) {
            if !out.iter().any(|o| o.key() == a.key()) {
                out.push(a);
            }
        }
    }
    out
}

/// The announcers found last ([`found`]), let go at each scan of the
/// library ([`forget`]).
static FOUND: Mutex<Found> = Mutex::new(Found {
    scan: 0,
    last: None,
});

struct Found {
    /// The library's scans so far: a list found during one is not kept.
    scan: u64,
    /// The folders looked in and what was found there.
    last: Option<(Vec<PathBuf>, Vec<DiskAnnouncer>)>,
}

/// [`all`], as found since the library last scanned: looked for again
/// only once it scans again (an announcer added to a song folder then
/// appears, as its songs do) or in other folders.
pub fn found(folders: &[PathBuf]) -> Vec<DiskAnnouncer> {
    let scan = {
        let Ok(f) = FOUND.lock() else {
            return all(folders);
        };
        if let Some((at, list)) = &f.last
            && at == folders
        {
            return list.clone();
        }
        f.scan
    };
    let list = all(folders);
    if let Ok(mut f) = FOUND.lock()
        && f.scan == scan
    {
        f.last = Some((folders.to_vec(), list.clone()));
    }
    list
}

/// The library scans again: the announcers are looked for again when
/// next asked for.
pub fn forget() {
    if let Ok(mut f) = FOUND.lock() {
        f.scan += 1;
        f.last = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::tests::{scratch, zip};

    fn write(root: &Path, files: &[&str]) {
        for p in files {
            let path = root.join(p);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, p.as_bytes()).unwrap();
        }
    }

    /// The first sound of `line`, read through the announcer's reader.
    fn first_sound(a: &DiskAnnouncer, line: announcer::Line) -> String {
        let (_, files) = a.kept.files(line).unwrap();
        let reader = a.reader().unwrap();
        let bytes = reader.read(&a.name_in_reader(&files[0])).unwrap();
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn a_stepmania_roots_announcers_are_found_in_folders_and_zips() {
        let root = scratch("announcers-root");
        write(
            &root,
            &[
                "Announcers/Techno/gameplay ready/r.ogg",
                "Announcers/Techno/gameplay cleared/c.mp3",
                "Announcers/Techno/readme.txt",
                "Announcers/Tiny/evaluation a/a.wav",
                "Announcers/Empty/gameplay ready/x.ogg",
                "Songs/Pack/Song/s.sm",
            ],
        );
        zip(
            &root.join("Announcers/Zipped.smzip"),
            &[
                ("Announcers/Zipped/gameplay ready/r.ogg", b"zipped ready"),
                ("Announcers/Zipped/gameplay failed/f.ogg", b"f"),
            ],
        );
        zip(
            &root.join("Announcers/Bare.zip"),
            &[
                ("gameplay ready/r.ogg", b"bare ready"),
                ("gameplay failed/f.ogg", b"f"),
                ("evaluation aa/aa.ogg", b"aa"),
            ],
        );
        let found = in_folder(&root);
        let keys: Vec<String> = found.iter().map(DiskAnnouncer::key).collect();
        assert_eq!(keys, ["techno", "tiny", "bare", "zipped"]);
        let techno = &found[0];
        assert_eq!(techno.base, Base::Dir(root.join("Announcers/Techno")));
        assert_eq!(
            first_sound(techno, announcer::Line::Ready),
            "Announcers/Techno/gameplay ready/r.ogg"
        );
        assert_eq!(first_sound(&found[2], announcer::Line::Ready), "bare ready");
        assert_eq!(
            first_sound(&found[3], announcer::Line::Restart),
            "zipped ready"
        );
        // The same root twice, and the shelf listed itself: once each,
        // after ours.
        let twice = all(&[root.clone(), root.join("Announcers"), root.clone()]);
        let ours = bundled().len();
        assert_eq!(twice.len(), ours + 4);
        assert!(twice[..ours].iter().all(|a| a.kept.bundled));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn ours_come_first_apart_from_the_players() {
        let assets = scratch("announcers-assets");
        let lines = [
            "gameplay ready/r.ogg",
            "gameplay cleared/c.ogg",
            "evaluation aa/aa.ogg",
        ];
        let at = |dir: &str| lines.map(|l| format!("{dir}/{l}"));
        write(
            &assets,
            &at("announcers/Ours").each_ref().map(String::as_str),
        );
        write(&assets, &["announcers/Ours/PROVENANCE.toml"]);
        let ours = in_assets(&assets);
        assert_eq!(ours.len(), 1);
        assert_eq!(ours[0].key(), "bundled:ours");
        assert_eq!(ours[0].base, Base::Dir(assets.join("announcers/Ours")));
        assert_eq!(
            first_sound(&ours[0], announcer::Line::Ready),
            "announcers/Ours/gameplay ready/r.ogg"
        );
        // None where the folder is not.
        assert!(in_assets(&assets.join("elsewhere")).is_empty());
        let _ = std::fs::remove_dir_all(&assets);
    }

    #[test]
    fn an_announcer_added_is_found_once_the_library_scans_again() {
        let root = scratch("announcers-found");
        let keys = |found: Vec<DiskAnnouncer>| -> Vec<String> {
            found
                .into_iter()
                .filter(|a| !a.kept.bundled)
                .map(|a| a.key())
                .collect()
        };
        write(&root, &["Announcers/One/gameplay ready/r.ogg"]);
        let folders = [root.clone()];
        assert_eq!(keys(found(&folders)), ["one"]);
        assert_eq!(keys(found(&folders)), ["one"]);
        write(&root, &["Announcers/Two/gameplay ready/r.ogg"]);
        forget();
        assert_eq!(keys(found(&folders)), ["one", "two"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The repository's own: every line ours, every sound decoded.
    #[test]
    fn the_bundled_announcers_speak_every_line_they_hold() {
        let found = bundled();
        let names: Vec<&str> = found.iter().map(|a| a.kept.name.as_str()).collect();
        assert_eq!(names, ["Mr. Clank", "ガキンさん"]);
        for a in &found {
            assert!(a.key().starts_with("bundled:"));
            assert_eq!(a.kept.lines.len(), 50, "{}", a.kept.name);
            let reader = a.reader().unwrap();
            for f in a.kept.lines.values().flatten() {
                assert!(reader.read(&a.name_in_reader(f)).is_ok(), "{f}");
            }
            let (_, files) = a.kept.files(announcer::Line::Ready).unwrap();
            for f in files {
                let bytes = reader.read(&a.name_in_reader(f)).unwrap();
                let pcm =
                    crate::audio::decoder::sound(bytes, f.extension(), Default::default(), 48_000)
                        .unwrap();
                // Longer than a tenth of a second, as said.
                let frames = pcm.data.len() / usize::from(pcm.channels);
                assert!(frames > 4800, "{}: {f}", a.kept.name);
            }
        }
    }

    #[test]
    fn a_listed_announcer_one_among_packs_and_a_listed_zip_are_found() {
        let base = scratch("announcers-listed");
        let lines = [
            "gameplay ready/r.ogg",
            "gameplay cleared/c.ogg",
            "evaluation aa/aa.ogg",
        ];
        let at = |dir: &str| lines.map(|l| format!("{dir}/{l}"));
        write(&base, &at("Voice").each_ref().map(String::as_str));
        write(&base, &at("Songs/Among").each_ref().map(String::as_str));
        write(&base, &["Songs/Pack/Song/s.sm", "Songs/Pack/Song/s.ogg"]);
        zip(
            &base.join("Loose.zip"),
            &[
                ("Loose/gameplay ready/r.ogg", b"loose"),
                ("Loose/gameplay cleared/c.ogg", b"c"),
                ("Loose/evaluation a/a.ogg", b"a"),
            ],
        );
        let names = |found: Vec<DiskAnnouncer>| -> Vec<String> {
            found.into_iter().map(|a| a.kept.name).collect()
        };
        assert_eq!(names(in_folder(&base.join("Voice"))), ["Voice"]);
        assert_eq!(names(in_folder(&base.join("Songs"))), ["Among"]);
        assert_eq!(names(in_folder(&base.join("Loose.zip"))), ["Loose"]);
        // A song folder with none.
        assert!(in_folder(&base.join("Songs/Pack")).is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }
}
