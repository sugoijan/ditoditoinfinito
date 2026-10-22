//! A skin and its fallbacks, and how StepMania finds an element's file
//! (`NoteSkinManager::LoadNoteSkinDataRecursive`, `GetPath`,
//! `GetPathFromDirAndFile`, `src/NoteSkinManager.cpp:115-544`; directory
//! listings, `src/RageFileManager.cpp`, `src/RageUtil_FileDB.cpp`; 5_1-new).

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;

use crate::builtin::{BuiltIn, Virtual};
use crate::hints::Hints;
use crate::ini::Ini;

/// One skin folder (imported or bundled).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Folder {
    /// The game folder it was found in (`dance`), or `common`.
    pub game: String,
    pub name: String,
    /// Every file, relative to the skin folder, `/`-separated.
    pub files: Vec<String>,
    /// The text of `metrics.ini`, of every `.redir` and of every `.lua`, by
    /// path as in `files`.
    pub texts: Vec<(String, String)>,
}

impl Folder {
    /// The text of `path` (ignoring case), if it was kept.
    pub fn text(&self, path: &str) -> Option<&str> {
        let want = path.to_lowercase();
        self.texts
            .iter()
            .find(|(p, _)| p.to_lowercase() == want)
            .map(|(_, t)| t.as_str())
    }

    /// Whether the folder has `path` (ignoring case).
    pub fn has_file(&self, path: &str) -> bool {
        let want = path.to_lowercase();
        self.files.iter().any(|f| f.to_lowercase() == want)
    }
}

/// A folder of the chain: imported, or a stand-in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    Folder(usize),
    BuiltIn(BuiltIn),
}

/// A file found for an element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRef {
    pub dir: Dir,
    /// Relative to its folder.
    pub path: String,
    /// What a stand-in's file is (`None` for imported files).
    pub virt: Option<Virtual>,
    /// A folder rather than a file.
    pub is_dir: bool,
}

impl FileRef {
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// The extension, as `GetExtension` returns it (case kept).
    pub fn extension(&self) -> &str {
        extension(&self.path)
    }
}

/// `path`'s file name's extension (case kept).
fn extension(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rfind('.').map_or("", |i| &name[i + 1..])
}

/// Whether `path` is an image a skin's actors may use, by its extension
/// (SVG: our extension, `crate::svg`).
pub fn is_image(path: &str) -> bool {
    ["png", "jpg", "jpeg", "gif", "bmp", "svg"]
        .iter()
        .any(|e| extension(path).eq_ignore_ascii_case(e))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// A skin of the chain is in no folder (`"references skin … that is not
    /// present"`): StepMania drops the skin.
    Missing { skin: String, missing: String },
    /// Twenty steps without an end.
    Circular(String),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Missing { skin, missing } => {
                write!(f, "{skin} falls back to {missing}, which is not there")
            }
            LoadError::Circular(skin) => write!(f, "{skin}'s fallbacks go round in a circle"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathError {
    /// In none of the folders, as itself or as `Fallback …`.
    NotFound(String),
    /// A `.redir` names a file that is in none of the folders.
    BadRedirect { redirect: String, target: String },
    /// A hundred redirects without an end.
    RedirectLoop(String),
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PathError::NotFound(name) => write!(f, "{name} is in none of the skin's folders"),
            PathError::BadRedirect { redirect, target } => {
                write!(f, "{redirect} points to {target}, which is not there")
            }
            PathError::RedirectLoop(name) => write!(f, "the redirects for {name} never end"),
        }
    }
}

/// An entry of a folder listing.
#[derive(Clone, Debug)]
struct Entry {
    /// The name, lowercased (`MakeLower`, which folds UTF-8 letters too).
    lower: String,
    path: String,
    virt: Option<Virtual>,
    is_dir: bool,
}

/// A folder's entries by (lowercased) subfolder, each list in lowercase
/// order: what `GetDirListing` sees.
#[derive(Clone, Debug, Default)]
struct Listing {
    dirs: HashMap<String, Vec<Entry>>,
}

impl Listing {
    fn of_folder(folder: &Folder) -> Listing {
        let mut l = Listing::default();
        for file in &folder.files {
            let Some(path) = normalize(file) else {
                continue;
            };
            let mut parent = String::new();
            let parts: Vec<&str> = path.split('/').collect();
            for (i, part) in parts.iter().enumerate() {
                let is_dir = i + 1 < parts.len();
                let entry_path = if parent.is_empty() {
                    part.to_string()
                } else {
                    format!("{parent}/{part}")
                };
                l.add(
                    &parent.to_lowercase(),
                    part,
                    entry_path.clone(),
                    None,
                    is_dir,
                );
                parent = entry_path;
            }
        }
        l.finish();
        l
    }

    fn of_builtin(b: BuiltIn) -> Listing {
        let mut l = Listing::default();
        for (file, virt) in b.files() {
            l.add("", file, file.to_string(), Some(*virt), false);
        }
        l.finish();
        l
    }

    fn add(&mut self, parent: &str, name: &str, path: String, virt: Option<Virtual>, is_dir: bool) {
        // `RageFileManager::GetDirListing` drops macOS resource forks.
        if name.starts_with("._") {
            return;
        }
        self.dirs
            .entry(parent.to_string())
            .or_default()
            .push(Entry {
                lower: name.to_lowercase(),
                path,
                virt,
                is_dir,
            });
    }

    fn finish(&mut self) {
        for entries in self.dirs.values_mut() {
            entries.sort_by(|a, b| a.lower.cmp(&b.lower).then(a.is_dir.cmp(&b.is_dir)));
            // A folder listed once, however many files it holds.
            entries.dedup_by(|a, b| a.lower == b.lower);
        }
    }

    /// `GetDirListing(dir + mask)`: the entries of `mask`'s folder whose
    /// names match its last part, with up to two `*`
    /// (`GetFilesSimpleMatch`: begins with, contains, ends with); the
    /// first `limit` of them (callers need one, or to know there are more).
    fn matching(&self, mask: &str, limit: usize) -> Vec<&Entry> {
        let (sub, pattern) = match mask.rfind('/') {
            Some(i) => (&mask[..i], &mask[i + 1..]),
            None => ("", mask),
        };
        let Some(entries) = self.dirs.get(&sub.to_lowercase()) else {
            return Vec::new();
        };
        let pattern = pattern.to_lowercase();
        let mut stars = pattern.match_indices('*').map(|(i, _)| i);
        let (begin, middle, end) = match (stars.next(), stars.next()) {
            (None, _) => (pattern.as_str(), None, None),
            (Some(a), None) => (&pattern[..a], None, Some(&pattern[a + 1..])),
            (Some(a), Some(b)) => (
                &pattern[..a],
                Some(&pattern[a + 1..b]),
                Some(&pattern[b + 1..]),
            ),
        };
        // Entries starting with `begin` are a run of the sorted list.
        let first = entries.partition_point(|e| e.lower.as_str() < begin);
        entries[first..]
            .iter()
            .take_while(|e| e.lower.starts_with(begin))
            .filter(|e| match end {
                None => e.lower == begin,
                Some(end) => {
                    let rest = &e.lower[begin.len()..];
                    rest.len() >= end.len()
                        && rest.ends_with(end)
                        && middle.is_none_or(|m| rest[..rest.len() - end.len()].contains(m))
                }
            })
            .take(limit)
            .collect()
    }
}

/// `s` after `prefix`, comparing character by character ignoring case
/// (lowercasing can change a character's length in bytes, so the rest is
/// found in `s` itself).
fn strip_prefix_ignoring_case<'s>(s: &'s str, prefix: &str) -> Option<&'s str> {
    let mut rest = s.char_indices();
    for p in prefix.chars() {
        let (_, c) = rest.next()?;
        if !c.to_lowercase().eq(p.to_lowercase()) {
            return None;
        }
    }
    let at = rest.next().map_or(s.len(), |(i, _)| i);
    s.get(at..)
}

/// `NormalizePath`: `\` to `/`, `.` and empty parts dropped, `dir/..`
/// collapsed; `None` for a path that leaves the folder.
fn normalize(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    Some(parts.join("/"))
}

/// A loaded skin: its folders in search order and its merged metrics.
#[derive(Debug)]
pub struct Skin<'a> {
    pub name: String,
    pub game: String,
    /// Borrowed, or owned by a skin that has to live on its own (the Lua
    /// host's, [`Skin::into_owned`]).
    folders: Cow<'a, [Folder]>,
    /// Search order: the skin, then each fallback.
    pub chain: Vec<Dir>,
    pub metrics: Ini,
    /// In StepMania 3.9's and OpenITG's format (`crate::legacy`): its
    /// fallbacks end in their stand-ins, and its elements are looked up
    /// by button, without dance/default's redirects.
    pub legacy: bool,
    /// Where its folders are as Lua names them: `/NoteSkins/`, or
    /// `/Songs/` for a song's folder ([`Skin::song`]).
    root: &'static str,
    listings: HashMap<Dir, Listing>,
    cache: RefCell<HashMap<(String, String), Result<FileRef, PathError>>>,
    /// Each imported folder's kept texts by lowercased path, built when
    /// first needed.
    text_index: RefCell<HashMap<usize, HashMap<String, usize>>>,
    warnings: RefCell<Vec<String>>,
    /// The actor files of StepMania 3.9's format read so far, by folder
    /// and path, and the work building them may still do
    /// (`crate::legacy`).
    pub(crate) legacy_nodes: RefCell<LegacyNodes>,
    pub(crate) legacy_budget: Cell<crate::legacy::Budget>,
}

/// [`Skin::legacy_nodes`].
pub(crate) type LegacyNodes =
    HashMap<(Dir, String), Result<std::sync::Arc<crate::legacy::Node>, String>>;

/// Where skins' and songs' files are, as StepMania's paths name them.
pub const SKINS_ROOT: &str = "/NoteSkins/";
pub const SONGS_ROOT: &str = "/Songs/";

/// `GAME_BASE_NOTESKIN_NAME`, `GAME_COMMON_NOTESKIN_NAME`.
pub const BASE: &str = "default";
pub const COMMON: &str = "common";

/// Warnings kept, at most.
const MAX_WARNINGS: usize = 100;

impl<'a> Skin<'a> {
    /// Loads skin `name` of `game` from `folders`, following its fallbacks.
    /// Each name is looked for in `game`'s skins, then in `common`'s; the
    /// game's `default` and `common`'s `common` stand in when they are not
    /// imported (StepMania always has them).
    pub fn load(folders: &'a [Folder], game: &str, name: &str) -> Result<Skin<'a>, LoadError> {
        let legacy = crate::legacy::is_legacy_chain(folders, game, name);
        let mut skin = Skin::new(name, game, Cow::Borrowed(folders), SKINS_ROOT);
        skin.legacy = legacy;
        let (mut loaded_base, mut loaded_common) = (false, false);
        let mut current = name.to_string();
        let mut depth = 0;
        loop {
            depth += 1;
            if depth >= 20 {
                return Err(LoadError::Circular(name.to_string()));
            }
            let dir = find(folders, game, &current, legacy).ok_or_else(|| LoadError::Missing {
                skin: name.to_string(),
                missing: current.clone(),
            })?;
            let ini = match dir {
                Dir::Folder(i) => Ini::parse(folders[i].text("metrics.ini").unwrap_or("")),
                Dir::BuiltIn(b) => b.ini(),
            };
            loaded_base |= current.eq_ignore_ascii_case(BASE);
            loaded_common |= current.eq_ignore_ascii_case(COMMON);
            let fallback = match ini.get("Global", "FallbackNoteSkin") {
                Some(f) => f.to_string(),
                None if !loaded_base => BASE.to_string(),
                None if !loaded_common => COMMON.to_string(),
                None => String::new(),
            };
            skin.metrics.merge_under(&ini);
            skin.chain.push(dir);
            skin.listings.entry(dir).or_insert_with(|| match dir {
                Dir::Folder(i) => Listing::of_folder(&folders[i]),
                Dir::BuiltIn(b) => Listing::of_builtin(b),
            });
            if fallback.is_empty() {
                break;
            }
            current = fallback;
        }
        Ok(skin)
    }

    /// A song's folder as the Lua host's files, for its background
    /// scripts (`docs/plans/scripted-backgrounds.md`): `folder` alone (its
    /// `game` the song's pack, its `name` the song's folder), at
    /// StepMania's own path, `/Songs/<pack>/<song>/`, without fallbacks
    /// or metrics.
    pub fn song(folder: Folder) -> Skin<'static> {
        let listing = Listing::of_folder(&folder);
        let dir = Dir::Folder(0);
        let (name, game) = (folder.name.clone(), folder.game.clone());
        let mut skin = Skin::new(&name, &game, Cow::Owned(vec![folder]), SONGS_ROOT);
        skin.chain.push(dir);
        skin.listings.insert(dir, listing);
        skin
    }

    /// Skin `name` of `game` over `folders` at `root`, with nothing loaded
    /// yet.
    fn new(name: &str, game: &str, folders: Cow<'a, [Folder]>, root: &'static str) -> Skin<'a> {
        Skin {
            name: name.to_string(),
            game: game.to_string(),
            folders,
            chain: Vec::new(),
            metrics: Ini::default(),
            legacy: false,
            root,
            listings: HashMap::new(),
            cache: RefCell::new(HashMap::new()),
            text_index: RefCell::new(HashMap::new()),
            warnings: RefCell::new(Vec::new()),
            legacy_nodes: RefCell::new(HashMap::new()),
            legacy_budget: Cell::new(crate::legacy::SKIN_BUDGET),
        }
    }

    /// Whether it is a song's folder ([`Skin::song`]).
    pub fn is_song(&self) -> bool {
        self.root == SONGS_ROOT
    }

    /// The same skin owning a copy of its folders.
    pub fn into_owned(self) -> Skin<'static> {
        Skin {
            name: self.name,
            game: self.game,
            folders: Cow::Owned(self.folders.into_owned()),
            chain: self.chain,
            metrics: self.metrics,
            legacy: self.legacy,
            root: self.root,
            listings: self.listings,
            cache: self.cache,
            text_index: self.text_index,
            warnings: self.warnings,
            legacy_nodes: self.legacy_nodes,
            legacy_budget: self.legacy_budget,
        }
    }

    /// The folder `dir` stands for.
    pub fn folder(&self, dir: Dir) -> Option<&Folder> {
        match dir {
            Dir::Folder(i) => self.folders.get(i),
            Dir::BuiltIn(_) => None,
        }
    }

    /// Whether the skin or one it falls back on has Lua of its own
    /// (StepMania 3.9's format has none: it is built natively).
    pub fn has_lua(&self) -> bool {
        !self.legacy
            && self.chain.iter().any(|&d| {
                self.folder(d).is_some_and(|f| {
                    f.files
                        .iter()
                        .any(|p| p.to_ascii_lowercase().ends_with(".lua"))
                })
            })
    }

    /// The name of `dir`'s skin.
    pub fn dir_name(&self, dir: Dir) -> &str {
        match dir {
            Dir::Folder(i) => &self.folders[i].name,
            Dir::BuiltIn(b) => b.name(),
        }
    }

    /// Whether `dir` has a `NoteSkin.lua`.
    pub fn has_script(&self, dir: Dir) -> bool {
        match dir {
            Dir::Folder(i) => self.folders[i].has_file("NoteSkin.lua"),
            // StepMania 3.9's and OpenITG's skins have none.
            Dir::BuiltIn(b) => matches!(
                b,
                BuiltIn::DanceDefault | BuiltIn::PumpDefault | BuiltIn::Common
            ),
        }
    }

    /// The `NoteSkin.lua` files in the order StepMania runs them: from the
    /// last fallback back to the skin, each given the previous one's table.
    pub fn scripts(&self) -> Vec<Dir> {
        self.chain
            .iter()
            .rev()
            .copied()
            .filter(|&d| self.has_script(d))
            .collect()
    }

    /// What StepMania would warn about while looking (several matches).
    pub fn warnings(&self) -> Vec<String> {
        self.warnings.borrow().clone()
    }

    fn warn(&self, w: String) {
        let mut ws = self.warnings.borrow_mut();
        if ws.len() < MAX_WARNINGS && !ws.contains(&w) {
            ws.push(w);
        }
    }

    /// `NOTESKIN:GetMetric(button, name)`: `[<button>]`, then
    /// `[NoteDisplay]`.
    pub fn metric(&self, button: &str, name: &str) -> Option<&str> {
        self.metrics
            .get(button, name)
            .or_else(|| self.metrics.get("NoteDisplay", name))
    }

    /// The hints of `file`'s name, read from its whole path as StepMania
    /// does (`/NoteSkins/<game>/<skin>/<file>`).
    pub fn hints(&self, file: &FileRef) -> Hints {
        let game = match file.dir {
            Dir::BuiltIn(b) => b.game(),
            Dir::Folder(i) => &self.folders[i].game,
        };
        Hints::parse(&format!(
            "{}{game}/{}/{}",
            self.root,
            self.dir_name(file.dir),
            file.path
        ))
    }

    /// `NOTESKIN:GetPath(button, element)`: `"<button> <element>"` in every
    /// folder, then `"Fallback <element>"`, then each `.redir` hop. Cached
    /// (`g_PathCache`).
    pub fn get_path(&self, button: &str, element: &str) -> Result<FileRef, PathError> {
        let key = (button.to_string(), element.to_string());
        if let Some(r) = self.cache.borrow().get(&key) {
            return r.clone();
        }
        let r = self.find_path(button, element);
        // Bounded: a script can ask for any number of names.
        let mut cache = self.cache.borrow_mut();
        if cache.len() < 4096 && button.len() + element.len() <= 256 {
            cache.insert(key, r.clone());
        }
        r
    }

    fn find_path(&self, button: &str, element: &str) -> Result<FileRef, PathError> {
        let wanted = if button.is_empty() {
            element.to_string()
        } else {
            format!("{button} {element}")
        };
        let mut found = self.search(&wanted);
        if found.is_none() && !button.is_empty() {
            found = self.search(&format!("Fallback {element}"));
        }
        let mut file = found.ok_or_else(|| PathError::NotFound(wanted.clone()))?;
        let mut level = 0;
        // `GetExtension(sPath) == "redir"`: case-sensitive.
        while file.extension() == "redir" {
            level += 1;
            if level >= 100 {
                return Err(PathError::RedirectLoop(wanted));
            }
            let target = self.redirect_target(&file);
            file = self.search(&target).ok_or_else(|| PathError::BadRedirect {
                redirect: file.path.clone(),
                target: target.clone(),
            })?;
        }
        Ok(file)
    }

    /// A `.redir`'s first line without its line ending
    /// (`GetFileContents(…, bOneLine)`, `StripCrnl`).
    fn redirect_target(&self, file: &FileRef) -> String {
        if let Some(Virtual::Redir(t)) = file.virt {
            return t.to_string();
        }
        let text = self.folder_text(file.dir, &file.path).unwrap_or("");
        let line = text.split('\n').next().unwrap_or("");
        line.trim_end_matches(['\r', '\n']).to_string()
    }

    /// The first folder with a match for `name` (`GetPathFromDirAndFile`
    /// over the search order).
    fn search(&self, name: &str) -> Option<FileRef> {
        let mask = normalize(&format!("{name}*"))?;
        for dir in &self.chain {
            let listing = &self.listings[dir];
            let matches = listing.matching(&mask, 9);
            let Some(first) = matches.first() else {
                continue;
            };
            if matches.len() > 1 {
                let names: Vec<&str> = matches.iter().take(8).map(|m| m.path.as_str()).collect();
                self.warn(format!(
                    "{} files in {} match {name}: {}{}",
                    if matches.len() > 8 {
                        "9 or more".to_string()
                    } else {
                        matches.len().to_string()
                    },
                    self.dir_name(*dir),
                    names.join(", "),
                    if matches.len() > 8 { ", …" } else { "" }
                ));
            }
            return Some(FileRef {
                dir: *dir,
                path: first.path.clone(),
                virt: first.virt,
                is_dir: first.is_dir,
            });
        }
        None
    }

    /// The folder of `dir` as StepMania names it to Lua:
    /// `/NoteSkins/<game>/<skin>/` (a song's: `/Songs/<pack>/<song>/`).
    pub fn dir_path(&self, dir: Dir) -> String {
        let game = match dir {
            Dir::BuiltIn(b) => b.game(),
            Dir::Folder(i) => &self.folders[i].game,
        };
        format!("{}{game}/{}/", self.root, self.dir_name(dir))
    }

    /// `file` as StepMania's paths name it (`GetPath`'s result).
    pub fn file_path(&self, file: &FileRef) -> String {
        format!("{}{}", self.dir_path(file.dir), file.path)
    }

    /// The folder of the chain and the path inside it that `path` names
    /// (`/NoteSkins/<game>/<skin>/<file>`, ignoring case); `None` outside
    /// the chain's folders or out of them by `..`.
    pub fn locate(&self, path: &str) -> Option<(Dir, String)> {
        let path = format!("/{}", normalize(path)?);
        self.chain.iter().find_map(|&d| {
            strip_prefix_ignoring_case(&path, &self.dir_path(d)).map(|rest| (d, rest.to_string()))
        })
    }

    /// `ActorUtil::ResolvePath` on `path` (`src/ActorUtil.cpp:44-110`): the
    /// file or folder itself, else the one file or folder whose name begins
    /// so (several: the first, with a warning), a folder holding
    /// `default.lua` as that file, and `.redir` files followed
    /// (`DerefRedir`: relative to the redirect, a hundred at most).
    pub fn resolve_path(&self, path: &str) -> Result<FileRef, String> {
        let (dir, rel) = self
            .locate(path)
            .ok_or_else(|| format!("{path}: not in the skin's folders"))?;
        let listing = &self.listings[&dir];
        let entry = |e: &Entry| FileRef {
            dir,
            path: e.path.clone(),
            virt: e.virt,
            is_dir: e.is_dir,
        };
        let mut file = match listing.matching(&rel, 1).first() {
            Some(e) => entry(e),
            None => {
                let matches = listing.matching(&format!("{rel}*"), 9);
                let first = matches
                    .first()
                    .ok_or_else(|| format!("references a file \"{path}\" which doesn't exist"))?;
                if matches.len() > 1 {
                    self.warn(format!(
                        "references a file \"{path}\" which has multiple matches: {}",
                        matches
                            .iter()
                            .take(8)
                            .map(|m| m.path.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                entry(first)
            }
        };
        if file.is_dir {
            if let Some(lua) = self.dir_has(dir, &format!("{}/default.lua", file.path)) {
                return Ok(FileRef {
                    path: lua,
                    is_dir: false,
                    virt: None,
                    dir,
                });
            }
            return Ok(file);
        }
        for _ in 0..100 {
            if file.extension() != "redir" {
                return Ok(file);
            }
            let target = self.redirect_target(&file);
            if target.is_empty() {
                return Err(format!("the redirect {} is empty", file.path));
            }
            let base = file.path.rfind('/').map_or("", |i| &file.path[..=i]);
            let mask = normalize(&format!("{base}{target}*"))
                .ok_or_else(|| format!("the redirect {} leaves the folder", file.path))?;
            let matches = listing.matching(&mask, 2);
            file = match matches.as_slice() {
                [one] => entry(one),
                [] => {
                    return Err(format!(
                        "the redirect {} references a file \"{target}\" which doesn't exist",
                        file.path
                    ));
                }
                _ => {
                    return Err(format!(
                        "the redirect {} references a file \"{target}\" with multiple matches",
                        file.path
                    ));
                }
            };
        }
        Err(format!("circular redirect {}", file.path))
    }

    /// The text of a Lua file of the chain: a stand-in's (StepMania's own),
    /// or one kept with an imported folder.
    pub fn lua_text(&self, file: &FileRef) -> Option<&str> {
        match file.dir {
            Dir::BuiltIn(b) => b.lua_source(&file.path),
            Dir::Folder(_) => self.folder_text(file.dir, &file.path),
        }
    }

    /// The kept text of `file` of an imported folder (actor files, models,
    /// redirects).
    pub fn file_text(&self, file: &FileRef) -> Option<&str> {
        self.folder_text(file.dir, &file.path)
    }

    /// A kept text of an imported folder (ignoring case), through an index.
    fn folder_text(&self, dir: Dir, path: &str) -> Option<&str> {
        let Dir::Folder(i) = dir else { return None };
        let folder = self.folders.get(i)?;
        let at = {
            let mut index = self.text_index.borrow_mut();
            let texts = index.entry(i).or_insert_with(|| {
                folder
                    .texts
                    .iter()
                    .enumerate()
                    .rev()
                    .map(|(n, (p, _))| (p.to_lowercase(), n))
                    .collect()
            });
            *texts.get(&path.to_lowercase())?
        };
        folder.texts.get(at).map(|(_, t)| t.as_str())
    }

    /// The files directly in folder `folder` of `dir` (relative to it;
    /// empty for its top), in the order `GetDirListing` sorts them
    /// (lowercased).
    pub(crate) fn folder_files(&self, dir: Dir, folder: &str) -> Vec<FileRef> {
        let mask = if folder.is_empty() {
            "*".to_string()
        } else {
            match normalize(&format!("{folder}/*")) {
                Some(m) => m,
                None => return Vec::new(),
            }
        };
        let Some(listing) = self.listings.get(&dir) else {
            return Vec::new();
        };
        listing
            .matching(&mask, usize::MAX)
            .into_iter()
            .filter(|e| !e.is_dir)
            .map(|e| FileRef {
                dir,
                path: e.path.clone(),
                virt: e.virt,
                is_dir: false,
            })
            .collect()
    }

    /// Whether `dir` holds `path` (a file, ignoring case).
    pub(crate) fn dir_has(&self, dir: Dir, path: &str) -> Option<String> {
        let mask = normalize(path)?;
        self.listings
            .get(&dir)?
            .matching(&mask, 4)
            .into_iter()
            .find(|e| !e.is_dir)
            .map(|e| e.path.clone())
    }
}

/// `NoteSkins/<game>/<name>/`, else `NoteSkins/common/<name>/`. StepMania
/// always has `<game>/default` and `common/common`; the stand-ins take
/// their place when they are not imported.
fn find(folders: &[Folder], game: &str, name: &str, legacy: bool) -> Option<Dir> {
    // StepMania 3.9's and OpenITG's fallbacks are never StepMania 5's: an
    // imported `default`, `flat` or `common` with a `NoteSkin.lua` is not
    // one of theirs.
    let standin = legacy
        && [BASE, "flat", COMMON]
            .iter()
            .any(|n| n.eq_ignore_ascii_case(name));
    let by = |g: &str| {
        folders.iter().position(|f| {
            f.game.eq_ignore_ascii_case(g)
                && f.name.eq_ignore_ascii_case(name)
                && !(standin && f.has_file("NoteSkin.lua"))
        })
    };
    if let Some(i) = by(game) {
        return Some(Dir::Folder(i));
    }
    // StepMania 3.9's and OpenITG's own (`LoadNoteSkinData`: the game's
    // `default` and `common/default` under every skin).
    if legacy && game.eq_ignore_ascii_case("dance") {
        if name.eq_ignore_ascii_case(BASE) {
            return Some(Dir::BuiltIn(BuiltIn::LegacyDefault));
        }
        if name.eq_ignore_ascii_case("flat") {
            return Some(Dir::BuiltIn(BuiltIn::LegacyFlat));
        }
        if name.eq_ignore_ascii_case(COMMON) {
            return Some(Dir::BuiltIn(BuiltIn::LegacyCommon));
        }
    }
    if name.eq_ignore_ascii_case(BASE) && game.eq_ignore_ascii_case("dance") {
        return Some(Dir::BuiltIn(BuiltIn::DanceDefault));
    }
    if name.eq_ignore_ascii_case(BASE) && game.eq_ignore_ascii_case("pump") {
        return Some(Dir::BuiltIn(BuiltIn::PumpDefault));
    }
    if let Some(i) = by(COMMON) {
        return Some(Dir::Folder(i));
    }
    name.eq_ignore_ascii_case(COMMON)
        .then_some(Dir::BuiltIn(BuiltIn::Common))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::builtin::NativeLua;

    /// A folder from `(path, text)` pairs; text is kept for the files an
    /// import keeps it for (`crate::import::kept_text`).
    pub(crate) fn folder(game: &str, name: &str, files: &[(&str, &str)]) -> Folder {
        Folder {
            game: game.into(),
            name: name.into(),
            files: files.iter().map(|(p, _)| p.to_string()).collect(),
            texts: files
                .iter()
                .filter(|(p, _)| crate::import::kept_text(p))
                .map(|(p, t)| (p.to_string(), t.to_string()))
                .collect(),
        }
    }

    #[test]
    fn the_chain_ends_in_default_and_common() {
        let folders = [folder(
            "dance",
            "Mine",
            &[("metrics.ini", "[NoteDisplay]\nA=1\n")],
        )];
        let skin = Skin::load(&folders, "dance", "mine").unwrap();
        assert_eq!(
            skin.chain,
            [
                Dir::Folder(0),
                Dir::BuiltIn(BuiltIn::DanceDefault),
                Dir::BuiltIn(BuiltIn::Common)
            ]
        );
        assert_eq!(skin.metric("Down", "A"), Some("1"));
        // Merged under: dance/default's value, then common's.
        assert_eq!(skin.metric("Down", "FlipHeadAndTailWhenReverse"), Some("1"));
        assert_eq!(skin.metric("Down", "TopHoldAnchorWhenReverse"), Some("0"));
    }

    #[test]
    fn fallbacks_are_named_and_searched_in_the_game_then_common() {
        let folders = [
            folder(
                "dance",
                "child",
                &[(
                    "metrics.ini",
                    "[Global]\nFallbackNoteSkin=Parent\n[NoteDisplay]\nA=child\n[Down]\nB=down\n",
                )],
            ),
            folder(
                "common",
                "parent",
                &[(
                    "metrics.ini",
                    "[Global]\nFallbackNoteSkin=common\n[NoteDisplay]\nA=parent\nB=parent\nC=parent\n",
                )],
            ),
        ];
        let skin = Skin::load(&folders, "dance", "child").unwrap();
        // common names no fallback, so StepMania still adds `default` after
        // it (not loaded yet), and `common` again after that.
        assert_eq!(
            skin.chain,
            [
                Dir::Folder(0),
                Dir::Folder(1),
                Dir::BuiltIn(BuiltIn::Common),
                Dir::BuiltIn(BuiltIn::DanceDefault),
                Dir::BuiltIn(BuiltIn::Common)
            ]
        );
        assert_eq!(skin.metric("Down", "A"), Some("child"));
        assert_eq!(skin.metric("Down", "B"), Some("down"));
        assert_eq!(skin.metric("Left", "B"), Some("parent"));
        assert_eq!(skin.metric("Left", "C"), Some("parent"));
        assert_eq!(skin.metric("Left", "Nope"), None);
    }

    #[test]
    fn broken_chains_fail_the_skin() {
        let missing = [folder(
            "dance",
            "a",
            &[("metrics.ini", "[Global]\nFallbackNoteSkin=nowhere\n")],
        )];
        assert_eq!(
            Skin::load(&missing, "dance", "a").unwrap_err(),
            LoadError::Missing {
                skin: "a".into(),
                missing: "nowhere".into()
            }
        );
        let circle = [
            folder(
                "dance",
                "a",
                &[("metrics.ini", "[Global]\nFallbackNoteSkin=b\n")],
            ),
            folder(
                "dance",
                "b",
                &[("metrics.ini", "[Global]\nFallbackNoteSkin=a\n")],
            ),
        ];
        assert_eq!(
            Skin::load(&circle, "dance", "a").unwrap_err(),
            LoadError::Circular("a".into())
        );
        // An empty fallback ends the chain there.
        let alone = [folder(
            "dance",
            "a",
            &[("metrics.ini", "[Global]\nFallbackNoteSkin=\n")],
        )];
        assert_eq!(
            Skin::load(&alone, "dance", "a").unwrap().chain,
            [Dir::Folder(0)]
        );
        // A skin that is not there.
        assert!(Skin::load(&alone, "dance", "zzz").is_err());
        // pump's `default` has a stand-in, another game's none.
        assert_eq!(
            Skin::load(&[], "pump", "default").unwrap().chain,
            [
                Dir::BuiltIn(BuiltIn::PumpDefault),
                Dir::BuiltIn(BuiltIn::Common)
            ]
        );
        assert!(Skin::load(&[], "kb7", "default").is_err());
        // A pump skin never falls back on a dance skin: only on what its
        // metrics name, pump's `default` and `common`.
        let mixed = [
            folder("dance", "default", &[("metrics.ini", "")]),
            folder("dance", "p", &[("metrics.ini", "")]),
            folder("pump", "p", &[("metrics.ini", "")]),
        ];
        assert_eq!(
            Skin::load(&mixed, "pump", "p").unwrap().chain,
            [
                Dir::Folder(2),
                Dir::BuiltIn(BuiltIn::PumpDefault),
                Dir::BuiltIn(BuiltIn::Common)
            ]
        );
    }

    #[test]
    fn get_path_matches_prefixes_ignoring_case() {
        let folders = [folder(
            "dance",
            "s",
            &[
                ("metrics.ini", ""),
                ("Down Tap Note 4x2 (doubleres).png", ""),
                ("down hold body ACTIVE.png", ""),
                ("Down Receptor.lua", ""),
                ("Down Receptor go.png", ""),
                ("Fallback Tap Mine.png", ""),
                ("Down Hold Explosion/default.xml", ""),
                ("sub/Down Thing.png", ""),
            ],
        )];
        let skin = Skin::load(&folders, "dance", "s").unwrap();
        let p = skin.get_path("Down", "Tap Note").unwrap();
        assert_eq!(p.path, "Down Tap Note 4x2 (doubleres).png");
        assert_eq!(skin.hints(&p).frames, Some((4, 2)));
        assert_eq!(
            skin.get_path("Down", "Hold Body Active").unwrap().path,
            "down hold body ACTIVE.png"
        );
        // Two matches: the first in lowercase order, with a warning.
        assert_eq!(
            skin.get_path("Down", "Receptor").unwrap().path,
            "Down Receptor go.png"
        );
        assert_eq!(skin.warnings().len(), 1);
        // `Fallback …` after every folder failed for the button.
        assert_eq!(
            skin.get_path("Left", "Tap Mine").unwrap().path,
            "Fallback Tap Mine.png"
        );
        // A folder is an entry.
        let d = skin.get_path("Down", "Hold Explosion").unwrap();
        assert!(d.is_dir);
        assert_eq!(d.path, "Down Hold Explosion");
        // A path into a subfolder.
        assert_eq!(
            skin.get_path("", "sub/down th").unwrap().path,
            "sub/Down Thing.png"
        );
        // dance/default has its own lift; only common has a fake.
        let lift = skin.get_path("Down", "Tap Lift").unwrap();
        assert_eq!(
            (lift.dir, lift.virt),
            (
                Dir::BuiltIn(BuiltIn::DanceDefault),
                Some(Virtual::Lua(NativeLua::DanceDefaultLook))
            )
        );
        let fake = skin.get_path("Down", "Tap Fake").unwrap();
        assert_eq!(fake.path, "Fallback Tap Fake 4x1.png");
        assert_eq!(
            (fake.dir, fake.virt),
            (Dir::BuiltIn(BuiltIn::Common), Some(Virtual::Art))
        );
        assert_eq!(
            skin.get_path("Down", "Nothing").unwrap_err(),
            PathError::NotFound("Down Nothing".into())
        );
        // With no button there is no `Fallback` retry.
        assert!(skin.get_path("", "Tap Fake").is_err());
    }

    #[test]
    fn redirects_are_followed_through_every_folder() {
        let folders = [folder(
            "dance",
            "s",
            &[
                (
                    "Down Tap Explosion Dim.redir",
                    "Down Tap Explosion Bright\r\nignored",
                ),
                ("Down Tap Explosion Bright.redir", "_glow"),
                ("Down Loop A.redir", "Down Loop B"),
                ("Down Loop B.redir", "Down Loop A"),
                ("Down Bad.redir", "Nowhere"),
                ("Down Upper.REDIR", "x"),
            ],
        )];
        let skin = Skin::load(&folders, "dance", "s").unwrap();
        // Into dance/default's `_glow`.
        let glow = skin.get_path("Down", "Tap Explosion Dim").unwrap();
        assert_eq!(glow.path, "_glow (doubleres).png");
        assert_eq!(glow.dir, Dir::BuiltIn(BuiltIn::DanceDefault));
        // common's own redirects: a hold head is common's tap note.
        let plain = Skin::load(&[], "dance", "default").unwrap();
        let head = plain.get_path("Left", "Hold Head Active").unwrap();
        assert_eq!(head.path, "Fallback Tap Note 4x1.png");
        assert_eq!(
            skin.get_path("Down", "Loop A").unwrap_err(),
            PathError::RedirectLoop("Down Loop A".into())
        );
        assert_eq!(
            skin.get_path("Down", "Bad").unwrap_err(),
            PathError::BadRedirect {
                redirect: "Down Bad.redir".into(),
                target: "Nowhere".into()
            }
        );
        // Only `.redir` in lowercase redirects.
        assert_eq!(
            skin.get_path("Down", "Upper").unwrap().path,
            "Down Upper.REDIR"
        );
    }

    #[test]
    fn located_paths_ignore_case_whatever_their_length_in_bytes() {
        // KELVIN SIGN lowercases to a one-byte `k`; `İ` to two characters.
        let folders = [
            folder("dance", "k日", &[("x.png", "")]),
            folder("dance", "İ", &[("日.png", "")]),
        ];
        let skin = Skin::load(&folders, "dance", "k日").unwrap();
        let kelvin = "/NoteS\u{212A}ins/dance/k日/x.png";
        assert_eq!(skin.locate(kelvin), Some((Dir::Folder(0), "x.png".into())));
        assert!(skin.resolve_path(kelvin).is_ok());
        let skin = Skin::load(&folders, "dance", "İ").unwrap();
        let dotted = "/NoteSkins/dance/İ/日.png";
        assert_eq!(skin.locate(dotted), Some((Dir::Folder(1), "日.png".into())));
        assert!(skin.resolve_path(dotted).is_ok());
    }

    #[test]
    fn paths_are_normalised_and_masks_have_stars() {
        let folders = [folder(
            "dance",
            "s",
            &[
                ("Down Tap Note.png", ""),
                ("sub/Down X.png", ""),
                ("Down A.redir", "./Down Tap Note"),
                ("Down B.redir", "sub\\Down X"),
                ("Down C.redir", "sub/../Down Tap Note"),
                ("Down D.redir", "../outside"),
                ("Down E.redir", "Down*Note"),
                ("._Down F.png", ""),
                ("Down Hold Explosion/default.lua", ""),
                ("Down Hold Explosion/x.png", ""),
            ],
        )];
        let skin = Skin::load(&folders, "dance", "s").unwrap();
        for e in ["A", "C", "E"] {
            assert_eq!(
                skin.get_path("Down", e).unwrap().path,
                "Down Tap Note.png",
                "{e}"
            );
        }
        assert_eq!(skin.get_path("Down", "B").unwrap().path, "sub/Down X.png");
        assert!(matches!(
            skin.get_path("Down", "D"),
            Err(PathError::BadRedirect { .. })
        ));
        // Resource forks are not listed.
        assert!(skin.get_path("", "._Down").is_err());
        // A folder is one entry however many files it holds.
        let d = skin.get_path("Down", "Hold Explosion").unwrap();
        assert!(d.is_dir);
        assert!(skin.warnings().is_empty());
        assert_eq!(
            skin.dir_has(Dir::Folder(0), "down hold explosion/DEFAULT.lua")
                .as_deref(),
            Some("Down Hold Explosion/default.lua")
        );
    }

    #[test]
    fn the_stand_in_default_beats_an_imported_common_default() {
        let folders = [
            folder("common", "default", &[("Down Tap Note.png", "")]),
            folder(
                "common",
                "x",
                &[("metrics.ini", "[Global]\nFallbackNoteSkin=default\n")],
            ),
        ];
        let skin = Skin::load(&folders, "dance", "x").unwrap();
        assert_eq!(skin.chain[1], Dir::BuiltIn(BuiltIn::DanceDefault));
        // An imported dance/default is used.
        let folders = [folder("dance", "Default", &[("metrics.ini", "")])];
        let skin = Skin::load(&folders, "dance", "default").unwrap();
        assert_eq!(skin.chain[0], Dir::Folder(0));
        // Scripts run from the last fallback back; this `default` has none.
        assert_eq!(skin.chain, [Dir::Folder(0), Dir::BuiltIn(BuiltIn::Common)]);
        assert_eq!(skin.scripts(), [Dir::BuiltIn(BuiltIn::Common)]);
        // Hints come from the whole path.
        let folders = [folder(
            "dance",
            "Big 4x4 skin",
            &[("Down Tap Note.png", "")],
        )];
        let skin = Skin::load(&folders, "dance", "Big 4x4 skin").unwrap();
        let f = skin.get_path("Down", "Tap Note").unwrap();
        assert_eq!(skin.hints(&f).frames, Some((4, 4)));
    }
}
