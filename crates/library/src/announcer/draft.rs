//! The announcer studio's draft (`docs/plans/announcer-studio.md`): the
//! recordings an author brings, each put on the line it is, and the pack
//! they make. The shells keep each recording's bytes under its take's id
//! (the web in IndexedDB, desktop in its data folder) and this beside
//! them; the screen shows it.
//!
//! A take is written into the pack as it came where the game reads its
//! format and it is not tidied, and as Opus otherwise, or a WAV where
//! Opus will not do ([`Write`]): the shell decodes it, tidies it
//! ([`super::tidy`]) and encodes it (the browser's encoder or
//! `ddi_platform::opus`) or writes it ([`crate::wav`]).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::tidy::Measure;
use super::{
    CHEER_DELAY, COMBO_GAP, COMMENT_GAP, GO_BEFORE, Grade, Kept, Line, MAX_BYTES, MAX_FILES,
    OUR_COMBOS, find_announcers,
};
use crate::path::RelPath;

/// Files a pick or drop takes as recordings by their extension, where
/// their first bytes say nothing (M4A, AAC, AIFF, WebM…): a song's
/// music's and these.
pub const MORE_RECORDINGS: [&str; 11] = [
    "m4a", "aac", "aif", "aiff", "caf", "webm", "weba", "mp4", "3gp", "amr", "wma",
];

/// Whether `path`'s extension is a recording's.
pub fn is_recording(path: &str) -> bool {
    crate::pack::is_audio(path) || crate::pack::has_extension(path, &MORE_RECORDINGS)
}

/// Whether the file at `path`, starting with `head` (its first [`HEAD`]
/// bytes), is taken as a recording: not a hidden file, and a format the
/// game reads or a recording's extension.
pub fn takes(path: &str, head: &[u8]) -> bool {
    let name = RelPath::new(path).file_name();
    !name.starts_with('.') && (Format::sniff(head) != Format::Other || is_recording(path))
}

/// The README of the pack among `paths`: the one nearest its top
/// (`README.txt`, in any case).
pub fn readme_path<'a>(paths: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    paths
        .into_iter()
        .filter(|p| {
            RelPath::new(p)
                .file_name()
                .eq_ignore_ascii_case("readme.txt")
        })
        .min_by_key(|p| p.matches('/').count())
}

/// A pack made before among `listed` (paths and sizes): the one
/// announcer in them, and the path of the README that may say who made
/// it. `None` for files that are not a single pack.
pub fn made_before(listed: &[(String, u64)]) -> Option<(String, Option<String>)> {
    let found = find_announcers(listed, "");
    let [announcer] = found.announcers.as_slice() else {
        return None;
    };
    let readme = readme_path(listed.iter().map(|(p, _)| p.as_str())).map(str::to_string);
    Some((announcer.name.clone(), readme))
}

/// What a pack made before says of itself ([`Draft::adopt`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Adopted {
    pub name: String,
    pub by: Option<String>,
    pub license: Option<String>,
}

impl Adopted {
    /// The pack `name`, with what `readme`'s text credits.
    pub fn new(name: String, readme: Option<&str>) -> Adopted {
        let (by, license) = readme.map(readme_credits).unwrap_or_default();
        Adopted { name, by, license }
    }
}

/// A finished pack's files as an installed announcer.
#[derive(Clone, Debug, PartialEq)]
pub struct InstallPlan {
    pub kept: Kept,
    /// The announcer's folder's name among the files.
    pub folder: String,
    /// What each of its files' paths starts with.
    prefix: String,
}

impl InstallPlan {
    /// `path` in the announcer's own folder.
    pub fn inside<'a>(&self, path: &'a str) -> &'a str {
        path.strip_prefix(&self.prefix).unwrap_or(path)
    }
}

/// How the files `listed` (paths and sizes) install as an announcer.
pub fn install_plan(listed: &[(String, u64)]) -> Result<InstallPlan, &'static str> {
    let announcer = find_announcers(listed, "root")
        .announcers
        .into_iter()
        .next()
        .ok_or("it has no lines yet")?;
    Ok(InstallPlan {
        kept: announcer.kept(),
        folder: announcer.dir.file_name().to_string(),
        prefix: format!("{}/", announcer.dir),
    })
}

/// What a recording is, by its first bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Format {
    Mp3,
    Wav,
    /// Ogg Vorbis.
    Vorbis,
    /// Ogg Opus.
    Opus,
    Flac,
    /// Anything else (M4A, AAC, AIFF, WebM…): written anew ([`Write`]).
    Other,
}

/// The first bytes [`Format::sniff`] wants.
pub const HEAD: usize = 64;

impl Format {
    /// What a file starting with `head` (its first [`HEAD`] bytes, or
    /// all of a shorter one) is.
    pub fn sniff(head: &[u8]) -> Format {
        let has = |needle: &[u8]| head.windows(needle.len()).any(|w| w == needle);
        if head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WAVE" {
            Format::Wav
        } else if head.starts_with(b"fLaC") {
            Format::Flac
        } else if head.starts_with(b"OggS") {
            if has(b"OpusHead") {
                Format::Opus
            } else if has(b"\x01vorbis") {
                Format::Vorbis
            } else {
                Format::Other
            }
        } else if head.starts_with(b"ID3")
            // An MPEG audio frame's sync, layer III (ADTS AAC says layer 0).
            || (head.len() >= 2 && head[0] == 0xFF && head[1] & 0xE0 == 0xE0 && (head[1] >> 1) & 3 == 1)
        {
            Format::Mp3
        } else {
            Format::Other
        }
    }

    /// The extension a file of it is written with.
    pub fn extension(self) -> Option<&'static str> {
        match self {
            Format::Mp3 => Some("mp3"),
            Format::Wav => Some("wav"),
            Format::Vorbis => Some("ogg"),
            Format::Opus => Some("opus"),
            Format::Flac => Some("flac"),
            Format::Other => None,
        }
    }

    /// Whether encoding it again would cost it something.
    fn lossy(self) -> bool {
        matches!(self, Format::Mp3 | Format::Vorbis | Format::Opus)
    }

    /// Whether a pack keeps it as it is: the game reads it, and with
    /// `stepmania` (Advanced → Also for StepMania) StepMania does too.
    pub fn kept(self, stepmania: bool) -> bool {
        match self {
            Format::Mp3 | Format::Wav | Format::Vorbis => true,
            Format::Opus | Format::Flac => !stepmania,
            Format::Other => false,
        }
    }
}

/// What decoding a take found.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Sound {
    /// Not decoded yet.
    #[default]
    Unread,
    Read(Measure),
    /// The shell could not decode it: why.
    Unreadable(String),
}

/// A recording in the draft.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Take {
    /// Its key in the shell's store.
    pub id: u64,
    /// The name it came with: a file's, or its path in a folder or zip
    /// that was added.
    pub source: String,
    pub bytes: u64,
    pub format: Format,
    /// `None`: not sorted yet.
    pub line: Option<Line>,
    /// Tidied or not, where it differs from the draft's choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tidy: Option<bool>,
    #[serde(default)]
    pub sound: Sound,
}

impl Take {
    /// Its file's name, without the folders it came in.
    pub fn file_name(&self) -> &str {
        RelPath::new(&self.source).file_name()
    }

    pub fn measure(&self) -> Option<&Measure> {
        match &self.sound {
            Sound::Read(m) => Some(m),
            _ => None,
        }
    }
}

/// An announcer being made.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Draft {
    pub name: String,
    pub author: String,
    pub license: String,
    /// Takes are tidied unless they say otherwise (decision 5).
    pub tidy: bool,
    /// Advanced → Also for StepMania (decision 6).
    #[serde(default)]
    pub stepmania: bool,
    pub takes: Vec<Take>,
    /// The combo lines listed however empty, in order: the author's to
    /// add to and take from (`gameplay 64 combo`, a pack counting so);
    /// [`OUR_COMBOS`] to begin with.
    #[serde(default = "our_combos")]
    pub combos: Vec<u32>,
    /// The id the next take gets.
    next: u64,
}

fn our_combos() -> Vec<u32> {
    OUR_COMBOS.to_vec()
}

impl Default for Draft {
    fn default() -> Draft {
        Draft {
            name: String::new(),
            author: String::new(),
            license: String::new(),
            tidy: true,
            stepmania: false,
            takes: Vec::new(),
            combos: our_combos(),
            next: 1,
        }
    }
}

/// The lines the studio lists however empty: every line the game speaks
/// but the combos (the draft's own, [`Draft::combos`]), Etterna's
/// `evaluation aaaaa` (no rules give it) and `evaluation f` (`evaluation
/// e` is that moment's line), in the game's order.
pub fn listed() -> Vec<Line> {
    Line::all()
        .into_iter()
        .filter(|l| !matches!(l, Line::Combo(_) | Line::Grade(Grade::Aaaaa | Grade::F)))
        .collect()
}

/// Something about a take or a line the author may want to know; never
/// in the way of an export.
#[derive(Clone, Debug, PartialEq)]
pub enum Note {
    /// The shell could not decode it: why.
    Unreadable(String),
    /// It reached full scale when recorded.
    Clipped,
    /// It runs past what follows it in the game: `seconds` long where
    /// `limit` is what its moment allows.
    TooLong { seconds: f64, limit: f64 },
}

/// A take's line runs this long at most before what follows it: a combo
/// line before the next gated line may come, a comment before the next
/// comment, a results line before the cheer.
pub fn limit(line: Line) -> Option<f64> {
    match line {
        Line::Combo(_) | Line::ComboOverflow | Line::ComboStopped | Line::LifeLost => {
            Some(COMBO_GAP)
        }
        Line::CommentHot | Line::CommentGood | Line::CommentDanger | Line::CommentOni => {
            Some(COMMENT_GAP)
        }
        Line::FirstClear
        | Line::NewRecord
        | Line::EvaluationFullCombo(_)
        | Line::Grade(Grade::Aaaaa | Grade::Aaaa | Grade::Aaa | Grade::Aa) => Some(CHEER_DELAY),
        _ => None,
    }
}

/// A play speaks `ready` and `here we go` both only where its first note
/// comes at least their length and [`GO_BEFORE`] into the song; past
/// this, most charts have no room for both, and `ready` is left out.
pub const OPENING_ROOM: f64 = 4.0;

/// A pack's file in the export.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Its path in the zip.
    pub path: String,
    pub take: u64,
    pub write: Write,
}

/// How a take goes into the pack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Write {
    /// Its bytes as they came.
    AsIs,
    /// Decoded and encoded as Opus, tidied or not.
    Opus { tidy: bool },
    /// Decoded and written as a 16-bit WAV, tidied or not: for StepMania,
    /// which reads no Opus, and a take of more channels than Opus here.
    Wav { tidy: bool },
}

/// What an export writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    /// The zip's name.
    pub file_name: String,
    pub entries: Vec<Entry>,
    /// `README.txt` at the zip's root, beside `Announcers/`.
    pub readme: String,
}

/// The name a file or folder of the pack takes from `name`: no
/// separators or characters systems refuse, no dots or spaces at the
/// ends, at most 80 characters.
fn file_safe(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    let joined: String = words.join(" ").chars().take(80).collect();
    joined
        .trim_matches(|c: char| c == '.' || c.is_whitespace())
        .to_string()
}

/// The line a file is, from its path: a line folder it is in (the
/// innermost; a pack added whole), else its own name ([`by_name`]), else
/// the name of the folder it is in. Recording apps' names (`New
/// Recording 12.m4a`) are no line.
pub fn match_line(path: &str) -> Option<Line> {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    let (file, dirs) = parts.split_last()?;
    dirs.iter()
        .rev()
        .find_map(|d| Line::parse(d))
        .or_else(|| by_name(RelPath::new(file).file_stem()))
        .or_else(|| dirs.last().and_then(|d| by_name(d)))
}

/// The line `name` says, as a line's folder is named, with `_`, `-` and
/// `.` as spaces, case ignored and a take's number at its end left out
/// (`Gameplay_Ready_2`, `ready take 3`), or as the end of one line's
/// folder alone, four letters or more (`here we go normal`, `100 combo`,
/// `cheer`).
pub fn by_name(name: &str) -> Option<Line> {
    let lowered = name.to_lowercase();
    let spaced: String = lowered
        .chars()
        .map(|c| if matches!(c, '_' | '-' | '.') { ' ' } else { c })
        .collect();
    let mut words: Vec<&str> = spaced.split_whitespace().collect();
    let mut tries = vec![words.join(" ")];
    // A take's number: `2`, `(2)`, `#2`, `take 2`, `take2`, `v2`.
    let numbered = |w: &str| {
        let w = w.trim_matches(|c| c == '(' || c == ')' || c == '#');
        let digits = w
            .strip_prefix("take")
            .or_else(|| w.strip_prefix('v'))
            .unwrap_or(w);
        !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
    };
    if words.len() > 1 && words.last().is_some_and(|w| numbered(w)) {
        words.pop();
        if words.len() > 1 && words.last() == Some(&"take") {
            words.pop();
        }
        tries.push(words.join(" "));
    }
    for name in &tries {
        if let Some(line) = Line::parse(name) {
            return Some(line);
        }
        if let Some(line @ Line::Combo(_)) = Line::parse(&format!("gameplay {name}")) {
            return Some(line);
        }
        if name.chars().count() >= 4 {
            let end = format!(" {name}");
            let mut ends = Line::all()
                .into_iter()
                .filter(|l| l.folder().ends_with(&end));
            if let (Some(line), None) = (ends.next(), ends.next()) {
                return Some(line);
            }
        }
    }
    None
}

impl Draft {
    /// A recording added: `source` its name (or its path in a folder or
    /// zip added), `bytes` its size and `head` its first bytes. It goes
    /// on the line its name says, else waits unsorted. Its id.
    pub fn add(&mut self, source: &str, bytes: u64, head: &[u8]) -> u64 {
        let id = self.next;
        self.next += 1;
        let line = match_line(source);
        if let Some(Line::Combo(n)) = line {
            self.add_combo(n);
        }
        self.takes.push(Take {
            id,
            source: source.to_string(),
            bytes,
            format: Format::sniff(head),
            line,
            tidy: None,
            sound: Sound::Unread,
        });
        id
    }

    /// Combo line `n` listed (in its place among them); whether it was
    /// not already.
    pub fn add_combo(&mut self, n: u32) -> bool {
        if n == 0 {
            return false;
        }
        match self.combos.binary_search(&n) {
            Ok(_) => false,
            Err(at) => {
                self.combos.insert(at, n);
                true
            }
        }
    }

    /// Combo line `n` no longer listed; not while a take is on it.
    pub fn remove_combo(&mut self, n: u32) -> bool {
        if self.takes_of(Line::Combo(n)).next().is_some() {
            return false;
        }
        let before = self.combos.len();
        self.combos.retain(|&c| c != n);
        self.combos.len() < before
    }

    pub fn take(&self, id: u64) -> Option<&Take> {
        self.takes.iter().find(|t| t.id == id)
    }

    pub fn take_mut(&mut self, id: u64) -> Option<&mut Take> {
        self.takes.iter_mut().find(|t| t.id == id)
    }

    /// Takes it off the draft.
    pub fn remove(&mut self, id: u64) -> Option<Take> {
        let i = self.takes.iter().position(|t| t.id == id)?;
        Some(self.takes.remove(i))
    }

    /// What a pack made before says of itself, where the draft says
    /// nothing yet.
    pub fn adopt(&mut self, pack: Adopted) {
        let empty = |field: &str| field.trim().is_empty();
        if empty(&self.name) {
            self.name = pack.name;
        }
        if let Some(by) = pack.by.filter(|_| empty(&self.author)) {
            self.author = by;
        }
        if let Some(license) = pack.license.filter(|_| empty(&self.license)) {
            self.license = license;
        }
    }

    /// The takes on `line`, in the order they came.
    pub fn takes_of(&self, line: Line) -> impl Iterator<Item = &Take> {
        self.takes.iter().filter(move |t| t.line == Some(line))
    }

    /// The takes not sorted yet.
    pub fn unsorted(&self) -> impl Iterator<Item = &Take> {
        self.takes.iter().filter(|t| t.line.is_none())
    }

    /// Every line the studio shows: [`listed`], the draft's combos, and
    /// any other a take is on (Etterna's two), in the game's order.
    pub fn lines(&self) -> Vec<Line> {
        let mut lines: BTreeSet<Line> = listed().into_iter().collect();
        lines.extend(self.combos.iter().map(|&n| Line::Combo(n)));
        lines.extend(self.takes.iter().filter_map(|t| t.line));
        lines.into_iter().collect()
    }

    /// Whether `take` is tidied.
    pub fn tidies(&self, take: &Take) -> bool {
        take.tidy.unwrap_or(self.tidy)
    }

    /// How `take` goes into the pack. One that is compressed already and
    /// was tidied before is not encoded again, so that opening a pack and
    /// exporting it costs its sounds nothing.
    pub fn write_of(&self, take: &Take) -> Write {
        let tidy = self.tidies(take);
        let settled = take.format.lossy() && take.measure().is_some_and(|m| m.already_tidy());
        if (!tidy || settled) && take.format.kept(self.stepmania) {
            Write::AsIs
        } else if self.stepmania || take.measure().is_some_and(|m| m.channels > 2) {
            Write::Wav { tidy }
        } else {
            Write::Opus { tidy }
        }
    }

    /// Its length as it goes into the pack, once decoded.
    pub fn seconds(&self, take: &Take) -> Option<f64> {
        let m = take.measure()?;
        Some(if self.tidies(take) {
            m.tidied_seconds()
        } else {
            m.seconds
        })
    }

    /// What the author may want to know of `take`.
    pub fn notes(&self, take: &Take) -> Vec<Note> {
        let mut notes = Vec::new();
        match &take.sound {
            Sound::Unreadable(why) => notes.push(Note::Unreadable(why.clone())),
            Sound::Read(m) if m.clipped => notes.push(Note::Clipped),
            _ => {}
        }
        if let (Some(limit), Some(seconds)) = (take.line.and_then(limit), self.seconds(take))
            && seconds > limit
        {
            notes.push(Note::TooLong { seconds, limit });
        }
        notes
    }

    /// `ready` (or `restart`) and `here we go` together, the longest of
    /// each, with [`GO_BEFORE`]: how far into the song a chart's first
    /// note must come for a play to say both. `None` without both.
    pub fn opening(&self) -> Option<f64> {
        let longest = |line: Line| {
            self.takes_of(line)
                .filter_map(|t| self.seconds(t))
                .reduce(f64::max)
        };
        let ready = [longest(Line::Ready), longest(Line::Restart)]
            .into_iter()
            .flatten()
            .reduce(f64::max)?;
        Some(ready + longest(Line::HereWeGo)? + GO_BEFORE)
    }

    /// The files an export writes, and their bytes as near as is known
    /// before writing them (a sound written anew: from its length, once
    /// decoded).
    pub fn size(&self) -> (usize, u64) {
        let mut files = 0;
        let mut bytes = 0;
        for t in self.takes.iter().filter(|t| exported(t)) {
            files += 1;
            bytes += match (self.write_of(t), t.measure()) {
                (Write::AsIs, _) => t.bytes,
                (Write::Opus { .. }, Some(m)) => {
                    let s = self.seconds(t).unwrap_or(m.seconds);
                    ddi_platform::ogg_opus::len(s, usize::from(m.channels))
                }
                (Write::Wav { .. }, Some(m)) => {
                    let s = self.seconds(t).unwrap_or(m.seconds);
                    crate::wav::len((s * f64::from(m.rate)).round() as u64, m.channels)
                }
                (Write::Opus { .. } | Write::Wav { .. }, None) => t.bytes,
            };
        }
        (files, bytes)
    }

    /// Whether the pack goes past what an import keeps ([`MAX_FILES`],
    /// [`MAX_BYTES`]).
    pub fn too_big(&self) -> bool {
        let (files, bytes) = self.size();
        files > MAX_FILES || bytes > MAX_BYTES
    }

    /// The pack's name as its folder has it ("Announcer" when unnamed).
    pub fn folder_name(&self) -> String {
        Some(file_safe(&self.name))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "Announcer".into())
    }

    /// What an export writes: each sorted take that was read, in
    /// `Announcers/<Name>/<line folder>/`, named after the file it came
    /// as (a number added where two would share a name), and a README
    /// with the name, author and license.
    pub fn export(&self) -> Export {
        let folder = self.folder_name();
        let mut by_line: BTreeMap<Line, Vec<&Take>> = BTreeMap::new();
        for t in self.takes.iter().filter(|t| exported(t)) {
            by_line
                .entry(t.line.expect("exported"))
                .or_default()
                .push(t);
        }
        let mut entries = Vec::new();
        for (line, takes) in by_line {
            let mut used: BTreeSet<String> = BTreeSet::new();
            for t in takes {
                let write = self.write_of(t);
                let ext = match write {
                    Write::AsIs => t.format.extension().unwrap_or("wav"),
                    Write::Opus { .. } => "opus",
                    Write::Wav { .. } => "wav",
                };
                let base = Some(file_safe(RelPath::new(t.file_name()).file_stem()))
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "take".into());
                let mut file = format!("{base}.{ext}");
                let mut n = 2;
                while !used.insert(file.to_lowercase()) {
                    file = format!("{base} {n}.{ext}");
                    n += 1;
                }
                entries.push(Entry {
                    path: format!("Announcers/{folder}/{}/{file}", line.folder()),
                    take: t.id,
                    write,
                });
            }
        }
        let mut readme = format!("{folder}\n");
        if !self.author.trim().is_empty() {
            readme += &format!("By {}\n", self.author.trim());
        }
        if !self.license.trim().is_empty() {
            readme += &format!("License: {}\n", self.license.trim());
        }
        readme += "\nAn announcer for Dito Dito Infinito, made with its announcer studio.\n";
        Export {
            file_name: format!("{folder}.zip"),
            entries,
            readme,
        }
    }
}

/// The author and the license an export's README names (`By …`,
/// `License: …` under the name), for its pack imported again.
pub fn readme_credits(readme: &str) -> (Option<String>, Option<String>) {
    let (mut by, mut license) = (None, None);
    for l in readme.lines().skip(1).take_while(|l| !l.trim().is_empty()) {
        if let Some(a) = l.strip_prefix("By ") {
            by = Some(a.trim().to_string());
        } else if let Some(a) = l.strip_prefix("License: ") {
            license = Some(a.trim().to_string());
        }
    }
    (by, license)
}

/// Whether an export writes `take`: sorted, and not refused by its
/// decoder.
fn exported(take: &Take) -> bool {
    take.line.is_some() && !matches!(take.sound, Sound::Unreadable(_))
}

#[cfg(test)]
mod tests;
