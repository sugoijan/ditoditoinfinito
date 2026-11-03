//! Announcers on disk (`docs/plans/announcers.md`, step 2), read in place
//! as skins are and found as the web's import finds them
//! (`ddi_library::announcer::find_announcers`): under the `Announcers/`
//! folder of each StepMania root among the song folders (zips and
//! `.smzip`s in it too), a song folder that is itself an announcer or a
//! folder named `Announcers`, an announcer among a song folder's packs,
//! and a listed zip holding one. Keys are the web's (the name in lower
//! case), so the `announcer` setting means the same announcer in both
//! shells; the first folder holding a key wins, as for skins.

use std::path::{Path, PathBuf};

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
    path.is_file()
        && path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| ddi_library::zip::archive_stem(n).is_some())
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

/// Every announcer of the song folders, the first holding a key winning.
pub fn all(folders: &[PathBuf]) -> Vec<DiskAnnouncer> {
    let mut out: Vec<DiskAnnouncer> = Vec::new();
    for f in folders {
        for a in in_folder(f) {
            if !out.iter().any(|o| o.key() == a.key()) {
                out.push(a);
            }
        }
    }
    out
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
        // The same root twice, and the shelf listed itself: once each.
        let twice = all(&[root.clone(), root.join("Announcers"), root.clone()]);
        assert_eq!(twice.len(), 4);
        let _ = std::fs::remove_dir_all(&root);
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
