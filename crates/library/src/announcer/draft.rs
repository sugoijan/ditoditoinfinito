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

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use super::pool::Pool;
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

/// What the game's reader made of a pack the studio wrote, where it is
/// not what the draft says ([`Draft::check`]): a mistake of the studio's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unread(pub String);

impl std::fmt::Display for Unread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
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
    /// What it says, where it was recorded for a text line
    /// ([`Script`]): kept, and shown as its words in a scene.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words: Option<String>,
    /// Its language's tag (`None`: the draft's first, [`Draft::lang_of`]);
    /// drafts stored before languages read none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    /// Words of the author's own on it (`super::script::is_tag`): a tag
    /// players may switch off ([`Switch`]) leaves it out when off.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// The lines it is said for besides [`Take::line`] ("That's awesome!"
    /// for `gameplay 100 combo` and `evaluation cheer`): one sound, written
    /// once in the pack's `shared/` folder ([`Draft::export`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<Line>,
}

impl Take {
    /// Its file's name, without the folders it came in.
    pub fn file_name(&self) -> &str {
        RelPath::new(&self.source).file_name()
    }

    /// The lines it is said for: its own, then those it is said for too.
    pub fn lines(&self) -> impl Iterator<Item = Line> + '_ {
        self.line.into_iter().chain(self.also.iter().copied())
    }

    /// Whether it is said for `line`.
    pub fn says(&self, line: Line) -> bool {
        self.line == Some(line) || self.also.contains(&line)
    }

    /// Whether it is said for more lines than one (written in `shared/`).
    pub fn shared(&self) -> bool {
        self.line.is_some() && !self.also.is_empty()
    }

    pub fn measure(&self) -> Option<&Measure> {
        match &self.sound {
            Sound::Read(m) => Some(m),
            _ => None,
        }
    }
}

/// A tag players may switch off: a row under the announcer in Options
/// once the pack is used (`pack:tag` in its script).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Switch {
    pub tag: String,
    /// What Options calls it (the author's words).
    pub label: String,
    /// Its recordings are said until a player switches it off (`false`:
    /// not until switched on).
    pub default: bool,
}

impl Switch {
    /// Its label as a pack's script says it: on one line, at most what
    /// Options shows, its tag where it says nothing.
    pub fn label_written(&self) -> String {
        let label: String = one_line(&self.label)
            .chars()
            .take(ddi_noteskin::params::MAX_LABEL)
            .collect();
        if label.trim().is_empty() {
            self.tag.clone()
        } else {
            label
        }
    }
}

/// The folder of a pack's takes said for several lines.
pub const SHARED: &str = "shared";

/// Tags the studio suggests: plain words for how a recording sounds, any
/// pack may use (the author's own words are as good).
pub const SUGGESTED_TAGS: [&str; 4] = ["calm", "excited", "short", "long"];

/// A line a tag switched off would leave with no recording
/// ([`Draft::silenced`]): in which language, and what speaks instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Silenced {
    pub line: Line,
    /// Its language, where the draft has two or more.
    pub lang: Option<String>,
    /// The line said in its place (its fallback with a recording left),
    /// if any.
    pub instead: Option<Line>,
}

/// The processing selected for one original take. Also a cache key: moving
/// a take to/from an opening line changes its effect tail.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Processing {
    pub measure: Option<Measure>,
    pub stadium: bool,
    pub line: Option<Line>,
}

impl Processing {
    pub fn apply(&self, raw: &[Vec<f32>], rate: u32) -> (Vec<Vec<f32>>, u32) {
        let dry = self
            .measure
            .as_ref()
            .map_or_else(|| raw.to_vec(), |m| super::tidy::apply(raw, rate, m));
        if !self.stadium {
            return (dry, rate);
        }
        let body = dry.first().map_or(0, Vec::len) as f64 / f64::from(rate.max(1));
        let colored = super::stadium::apply(&dry, rate, self.line);
        (
            super::tidy::finished(&colored, super::stadium::RATE, body).0,
            super::stadium::RATE,
        )
    }
}

/// A bounded cache of processed copies. Originals remain in the shells'
/// decoded stores; settings changes never process an already colored copy.
pub struct Processed<S> {
    copies: Decoded<(Processing, S)>,
}

impl<S> Default for Processed<S> {
    fn default() -> Self {
        Self {
            copies: Decoded::default(),
        }
    }
}

impl<S: Clone> Processed<S> {
    pub fn get(&self, id: u64, key: Processing) -> Option<S> {
        self.copies
            .get(id)
            .filter(|(k, _)| *k == key)
            .map(|(_, s)| s.clone())
    }
    pub fn insert(&mut self, id: u64, key: Processing, sound: S, bytes: usize) {
        self.copies.insert(id, (key, sound), bytes);
    }
}

/// A text line: what the author means to record for a line, before
/// there is a recording of it ([`Draft::add_to`] gives it one).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Script {
    /// From the takes' ids: none is both.
    pub id: u64,
    pub line: Line,
    pub words: String,
    /// Its language, as a take's ([`Take::lang`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

/// An announcer being made.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Draft {
    pub name: String,
    pub author: String,
    pub license: String,
    /// Takes are tidied unless they say otherwise (decision 5).
    pub tidy: bool,
    /// Optional creative stadium processing, off for old and new drafts.
    #[serde(default)]
    pub stadium: bool,
    /// Advanced → Also for StepMania (decision 6).
    #[serde(default)]
    pub stepmania: bool,
    pub takes: Vec<Take>,
    /// The text lines not recorded yet.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<Script>,
    /// The combo lines listed however empty, in order: the author's to
    /// add to and take from (`gameplay 64 combo`, a pack counting so);
    /// [`OUR_COMBOS`] to begin with.
    #[serde(default = "our_combos")]
    pub combos: Vec<u32>,
    /// The languages the pack speaks, by tag, in the order offered (none:
    /// one, not named; drafts stored before languages read none). Each
    /// language's takes are its own; a draft of two or more is written
    /// with a folder a language and an `announcer.ini` ([`Draft::export`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<String>,
    /// Its languages are offered mixed too (`Mixed=1`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mixed: bool,
    /// The language the page shows and new recordings go in (none, or one
    /// not listed: the first).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working: Option<String>,
    /// The tags players may switch off, in the order made switchable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub switches: Vec<Switch>,
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
            stadium: false,
            stepmania: false,
            takes: Vec::new(),
            scripts: Vec::new(),
            combos: our_combos(),
            languages: Vec::new(),
            mixed: false,
            working: None,
            switches: Vec::new(),
            next: 1,
        }
    }
}

/// The languages the studio offers a pack, each by its tag and named in
/// itself (data, as the Language row in Options names a pack's languages).
pub const LANGUAGES: [(&str, &str); 7] = [
    ("en", "English"),
    ("ja", "日本語"),
    ("es", "Español"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("it", "Italiano"),
    ("pt-BR", "Português (Brasil)"),
];

/// Language `tag` named in itself ([`LANGUAGES`]), else its tag.
pub fn language_name(tag: &str) -> &str {
    LANGUAGES
        .iter()
        .find(|(t, _)| t.eq_ignore_ascii_case(tag))
        .map_or(tag, |(_, name)| name)
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
    /// The pack's text files, by their paths in the zip: its
    /// `announcer.ini` and `announcer.lua` where the plain layout cannot
    /// say what the draft does ([`Draft::declares`]; none for a plain
    /// draft, whose export is StepMania's layout alone).
    pub texts: Vec<(String, String)>,
}

impl Export {
    /// Every file of the pack in the zip but its README, each with its
    /// size: the sounds' as made (`sizes`, by their paths) and the texts'.
    pub fn listed(&self, sizes: &[(String, u64)]) -> Vec<(String, u64)> {
        sizes
            .iter()
            .cloned()
            .chain(self.texts.iter().map(|(p, t)| (p.clone(), t.len() as u64)))
            .collect()
    }

    /// The pack as the game's reader takes it, its sounds made with
    /// `sizes` (by their paths): one announcer, its `announcer.ini` and
    /// `announcer.lua` read as text ([`Kept::ini`], [`Kept::script`]).
    pub fn installed(&self, sizes: &[(String, u64)]) -> Result<InstallPlan, String> {
        let mut plan = install_plan(&self.listed(sizes)).map_err(str::to_string)?;
        plan.kept.ini = self.text(super::pool::DECLARATION).map(str::to_string);
        plan.kept.script = self.text(super::script::SCRIPT).map(str::to_string);
        Ok(plan)
    }

    /// The text at `path` among [`Export::texts`]: the pack's
    /// `announcer.ini` (`super::pool::DECLARATION`) or `announcer.lua`
    /// (`super::script::SCRIPT`), by its file's name.
    pub fn text(&self, name: &str) -> Option<&str> {
        self.texts
            .iter()
            .find(|(p, _)| RelPath::new(p).file_name() == name)
            .map(|(_, t)| t.as_str())
    }
}

/// The name a file or folder of the pack takes from `name`: no
/// separators or characters systems refuse, no dots or spaces at the
/// ends, no device name of Windows' ([`crate::path::portable_name`]), at
/// most 80 characters.
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
    crate::path::portable_name(&joined)
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
            let mut ends = super::folders()
                .iter()
                .filter(|(folder, _)| folder.ends_with(&end))
                .map(|&(_, line)| line);
            if let (Some(line), None) = (ends.next(), ends.next()) {
                return Some(line);
            }
        }
    }
    None
}

/// A recording's file at most: a line is seconds of speech, and this is
/// about three minutes of 24-bit stereo WAV at 48 kHz; a larger file is
/// a song, not a recording.
pub const TAKE_MAX_BYTES: u64 = 32 << 20;

/// A draft's recordings as they came, at most: four times what the pack
/// made from them may be ([`MAX_BYTES`]; uncompressed recordings become
/// Opus).
pub const DRAFT_MAX_BYTES: u64 = 4 * MAX_BYTES;

/// Bytes of decoded samples a studio keeps at most ([`Decoded`]): about
/// eleven minutes of mono sound at 48 kHz, far more than a scene's lines.
pub const DECODED_BYTES: usize = 128 << 20;

/// The recordings' sound a studio keeps decoded, by take: the ones
/// decoded last, up to `cap` bytes of samples in all (the one just put
/// in always kept), the oldest let go first. What is let go is decoded
/// again from its file when it is played, heard in a scene or exported,
/// so a draft of many recordings does not hold them all in memory.
pub struct Decoded<S> {
    sounds: HashMap<u64, S>,
    /// The takes kept, oldest first, each with its bytes.
    order: std::collections::VecDeque<(u64, usize)>,
    bytes: usize,
    cap: usize,
}

impl<S> Default for Decoded<S> {
    fn default() -> Self {
        Self::with_cap(DECODED_BYTES)
    }
}

impl<S> Decoded<S> {
    pub fn with_cap(cap: usize) -> Self {
        Decoded {
            sounds: HashMap::new(),
            order: Default::default(),
            bytes: 0,
            cap,
        }
    }

    /// Take `id`'s sound `sound`, of `bytes` bytes of samples, kept.
    pub fn insert(&mut self, id: u64, sound: S, bytes: usize) {
        self.remove(id);
        self.sounds.insert(id, sound);
        self.order.push_back((id, bytes));
        self.bytes += bytes;
        while self.bytes > self.cap && self.order.len() > 1 {
            if let Some((old, _)) = self.order.front().copied() {
                self.remove(old);
            }
        }
    }

    pub fn remove(&mut self, id: u64) -> Option<S> {
        if let Some(i) = self.order.iter().position(|(o, _)| *o == id) {
            let (_, bytes) = self.order.remove(i)?;
            self.bytes -= bytes;
        }
        self.sounds.remove(&id)
    }

    pub fn get(&self, id: u64) -> Option<&S> {
        self.sounds.get(&id)
    }

    pub fn contains(&self, id: u64) -> bool {
        self.sounds.contains_key(&id)
    }

    /// The sounds kept, by take.
    pub fn sounds(&self) -> &HashMap<u64, S> {
        &self.sounds
    }

    pub fn len(&self) -> usize {
        self.sounds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sounds.is_empty()
    }

    /// Bytes of samples kept.
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

/// The name a draft that could not be read is kept aside under, beside
/// where it was (`draft.unreadable.json`, then `draft.unreadable.2.json`
/// and so on): the first that `taken` does not say is there already, so
/// one kept aside before is never replaced.
pub fn aside_name(taken: impl Fn(&str) -> bool) -> String {
    let first = "draft.unreadable.json".to_string();
    if !taken(&first) {
        return first;
    }
    (2u32..)
        .map(|n| format!("draft.unreadable.{n}.json"))
        .find(|name| !taken(name))
        .unwrap_or(first)
}

impl Draft {
    /// The ids given from now on are past `highest`: the highest id a
    /// stored recording has, whatever draft it was of, so a new
    /// recording's file never takes the place of one that exists (a
    /// draft that could not be read and was begun anew, a draft read
    /// behind what was stored).
    pub fn start_past(&mut self, highest: u64) {
        self.next = self.next.max(highest.saturating_add(1));
    }

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
            words: None,
            lang: self.working_language().map(str::to_string),
            tags: Vec::new(),
            also: Vec::new(),
        });
        id
    }

    /// A recording added for text line `script` (as [`Draft::add`], on
    /// its line, with its words), which it takes the place of; one
    /// added as `add` does where that line is gone. Its id.
    pub fn add_to(&mut self, script: u64, source: &str, bytes: u64, head: &[u8]) -> u64 {
        let id = self.add(source, bytes, head);
        if let Some(i) = self.scripts.iter().position(|s| s.id == script) {
            let s = self.scripts.remove(i);
            if let Some(t) = self.take_mut(id) {
                t.line = Some(s.line);
                t.words = Some(s.words);
                t.lang = s.lang;
            }
        }
        id
    }

    /// A text line on `line`, in the language worked in, its words to be
    /// written. Its id.
    pub fn write(&mut self, line: Line) -> u64 {
        let lang = self.working_language().map(str::to_string);
        self.write_with(line, String::new(), lang)
    }

    /// [`Draft::write`] with its `words`, in `lang`.
    fn write_with(&mut self, line: Line, words: String, lang: Option<String>) -> u64 {
        let id = self.next;
        self.next += 1;
        self.scripts.push(Script {
            id,
            line,
            words,
            lang,
        });
        id
    }

    /// Text line `id` off the draft.
    pub fn unwrite(&mut self, id: u64) -> Option<Script> {
        let i = self.scripts.iter().position(|s| s.id == id)?;
        Some(self.scripts.remove(i))
    }

    /// The words of text line or take `id` ([`Take::words`]), where it
    /// has them to change.
    pub fn words_mut(&mut self, id: u64) -> Option<&mut String> {
        if let Some(s) = self.scripts.iter_mut().find(|s| s.id == id) {
            return Some(&mut s.words);
        }
        self.takes.iter_mut().find(|t| t.id == id)?.words.as_mut()
    }

    /// The text lines on `line` in the language worked in, in the order
    /// they were written.
    pub fn scripts_of(&self, line: Line) -> impl Iterator<Item = &Script> {
        self.scripts
            .iter()
            .filter(move |s| s.line == line && self.working(s.lang.as_deref()))
    }

    /// The language the page works in: [`Draft::working`] where it is one
    /// of the draft's, else the first (`None`: one, not named).
    pub fn working_language(&self) -> Option<&str> {
        self.working
            .as_deref()
            .filter(|w| self.languages.iter().any(|l| l == w))
            .or_else(|| self.languages.first().map(String::as_str))
    }

    /// Take `t`'s language: its own, else the draft's first.
    pub fn lang_of<'a>(&'a self, t: &'a Take) -> Option<&'a str> {
        t.lang
            .as_deref()
            .or_else(|| self.languages.first().map(String::as_str))
    }

    /// Whether something of language `lang` (`None`: the first) is in the
    /// language worked in.
    fn working(&self, lang: Option<&str>) -> bool {
        lang.or_else(|| self.languages.first().map(String::as_str)) == self.working_language()
    }

    /// Language `tag` listed (the first names the draft's, its takes
    /// being in it; another becomes the one worked in); whether it was not
    /// already.
    pub fn add_language(&mut self, tag: &str) -> bool {
        if !super::pool::is_language(tag) || self.languages.iter().any(|l| l == tag) {
            return false;
        }
        self.languages.push(tag.to_string());
        if self.languages.len() > 1 {
            self.working = Some(tag.to_string());
        }
        true
    }

    /// Whether a take or a text line is in language `tag`.
    pub fn speaks(&self, tag: &str) -> bool {
        self.takes.iter().any(|t| self.lang_of(t) == Some(tag))
            || self.scripts.iter().any(|s| {
                s.lang
                    .as_deref()
                    .or_else(|| self.languages.first().map(String::as_str))
                    == Some(tag)
            })
    }

    /// Language `tag` no longer listed; not while a take or a text line
    /// is in it.
    pub fn remove_language(&mut self, tag: &str) -> bool {
        if self.speaks(tag) {
            return false;
        }
        let before = self.languages.len();
        self.languages.retain(|l| l != tag);
        if self.working.as_deref() == Some(tag) {
            self.working = None;
        }
        if self.languages.len() < 2 {
            self.mixed = false;
        }
        self.languages.len() < before
    }

    /// Every tag on a take or made switchable, sorted.
    pub fn tags(&self) -> Vec<String> {
        let mut tags: BTreeSet<String> = self.takes.iter().flat_map(|t| t.tags.clone()).collect();
        tags.extend(self.switches.iter().map(|s| s.tag.clone()));
        tags.into_iter().collect()
    }

    /// The takes tag `tag` is on.
    pub fn tagged(&self, tag: &str) -> usize {
        self.takes
            .iter()
            .filter(|t| t.tags.iter().any(|g| g == tag))
            .count()
    }

    /// Tag `tag` put on take `id`, or taken off it where it was on;
    /// whether it is on now. Not a tag's name, or past
    /// `super::script::MAX_TAGS` on it: nothing.
    pub fn toggle_tag(&mut self, id: u64, tag: &str) -> Option<bool> {
        if !super::script::is_tag(tag) {
            return None;
        }
        let t = self.take_mut(id)?;
        if let Some(i) = t.tags.iter().position(|g| g == tag) {
            t.tags.remove(i);
            return Some(false);
        }
        if t.tags.len() >= super::script::MAX_TAGS {
            return None;
        }
        t.tags.push(tag.to_string());
        Some(true)
    }

    /// Whether `take` is said in the game before a player switches
    /// anything: none of its tags is a switch off at first.
    pub fn said_at_first(&self, take: &Take) -> bool {
        !take
            .tags
            .iter()
            .any(|g| self.switch_of(g).is_some_and(|s| !s.default))
    }

    /// Tag `tag`'s switch, where players may switch it off.
    pub fn switch_of(&self, tag: &str) -> Option<&Switch> {
        self.switches.iter().find(|s| s.tag == tag)
    }

    /// Tag `tag` made switchable (On at first, named by itself), or no
    /// longer; whether it is now.
    pub fn toggle_switch(&mut self, tag: &str) -> bool {
        if let Some(i) = self.switches.iter().position(|s| s.tag == tag) {
            self.switches.remove(i);
            return false;
        }
        if !super::script::is_tag(tag) || self.switches.len() >= super::script::MAX_TAGS {
            return false;
        }
        self.switches.push(Switch {
            tag: tag.to_string(),
            label: tag.to_string(),
            default: true,
        });
        true
    }

    /// Tag `tag`'s switch's first state turned over.
    pub fn toggle_default(&mut self, tag: &str) -> bool {
        match self.switches.iter_mut().find(|s| s.tag == tag) {
            Some(s) => {
                s.default = !s.default;
                true
            }
            None => false,
        }
    }

    /// Take `id` said for `line` too (a combo line listed if it was not);
    /// whether it was not already. Not for a take not on a line yet.
    pub fn share(&mut self, id: u64, line: Line) -> bool {
        let Some(t) = self.take_mut(id) else {
            return false;
        };
        if t.line.is_none() || t.says(line) {
            return false;
        }
        t.also.push(line);
        if let Line::Combo(n) = line {
            self.add_combo(n);
        }
        true
    }

    /// Take `id` no longer said for `line` besides its own; whether it was.
    pub fn unshare(&mut self, id: u64, line: Line) -> bool {
        let Some(t) = self.take_mut(id) else {
            return false;
        };
        let before = t.also.len();
        t.also.retain(|l| *l != line);
        t.also.len() < before
    }

    /// Take `id` put on `line` (`None`: unsorted): said for it, and for
    /// what it was said for besides as long as it is on a line.
    pub fn set_line(&mut self, id: u64, line: Option<Line>) {
        let Some(t) = self.take_mut(id) else {
            return;
        };
        t.line = line;
        match line {
            Some(l) => t.also.retain(|a| *a != l),
            None => t.also.clear(),
        }
    }

    /// The languages a take may be in, each (`None`: one, not named).
    fn each_language(&self) -> Vec<Option<&str>> {
        if self.languages.is_empty() {
            vec![None]
        } else {
            self.languages.iter().map(|l| Some(l.as_str())).collect()
        }
    }

    /// The takes said for `line` in language `lang` (`None`: the first).
    fn said_in<'a>(&'a self, line: Line, lang: Option<&'a str>) -> impl Iterator<Item = &'a Take> {
        let lang = lang.or_else(|| self.languages.first().map(String::as_str));
        self.takes
            .iter()
            .filter(move |t| exported(t) && t.says(line) && self.lang_of(t) == lang)
    }

    /// The lines tag `tag` switched off would leave with no recording, in
    /// each language, and what is said in their place then (as a pack
    /// lacking the line: its fallback with a recording left, or nothing).
    pub fn silenced(&self, tag: &str) -> Vec<Silenced> {
        let has = |t: &Take| t.tags.iter().any(|g| g == tag);
        let mut out = Vec::new();
        for lang in self.each_language() {
            let mut lines: Vec<Line> = self
                .takes
                .iter()
                .filter(|t| {
                    exported(t)
                        && self.lang_of(t) == lang.or(self.languages.first().map(String::as_str))
                })
                .flat_map(Take::lines)
                .collect();
            lines.sort();
            lines.dedup();
            for line in lines {
                let left = |l: Line| self.said_in(l, lang).any(|t| !has(t));
                if left(line) {
                    continue;
                }
                out.push(Silenced {
                    line,
                    lang: lang.filter(|_| self.languages.len() > 1).map(str::to_string),
                    instead: line.chain().into_iter().skip(1).find(|l| left(*l)),
                });
            }
        }
        out
    }

    /// The languages with a recording on `line`, in the draft's order.
    pub fn languages_of(&self, line: Line) -> Vec<&str> {
        self.languages
            .iter()
            .map(String::as_str)
            .filter(|l| {
                self.takes
                    .iter()
                    .any(|t| t.says(line) && self.lang_of(t) == Some(*l))
            })
            .collect()
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

    /// Combo line `n` no longer listed; not while a take or a text line
    /// is on it, in any language.
    pub fn remove_combo(&mut self, n: u32) -> bool {
        let line = Line::Combo(n);
        if self.takes.iter().any(|t| t.says(line))
            || self.scripts.iter().any(|s| s.line == line)
        {
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

    /// Takes it off the draft; one recorded for a text line leaves the
    /// line's words, a text line again (with an id of its own).
    pub fn remove(&mut self, id: u64) -> Option<Take> {
        let i = self.takes.iter().position(|t| t.id == id)?;
        let t = self.takes.remove(i);
        if let (Some(line), Some(words)) = (t.line, &t.words) {
            self.write_with(line, words.clone(), t.lang.clone());
        }
        Some(t)
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

    /// The takes on `line` in the language worked in, in the order they
    /// came.
    pub fn takes_of(&self, line: Line) -> impl Iterator<Item = &Take> {
        self.takes
            .iter()
            .filter(move |t| t.says(line) && self.working(t.lang.as_deref()))
    }

    /// The takes not sorted yet, in the language worked in.
    pub fn unsorted(&self) -> impl Iterator<Item = &Take> {
        self.takes
            .iter()
            .filter(|t| t.line.is_none() && self.working(t.lang.as_deref()))
    }

    /// Every line the studio shows: [`listed`], the draft's combos, and
    /// any other a take is on (Etterna's two), in the game's order.
    pub fn lines(&self) -> Vec<Line> {
        let mut lines: BTreeSet<Line> = listed().into_iter().collect();
        lines.extend(self.combos.iter().map(|&n| Line::Combo(n)));
        lines.extend(self.takes.iter().flat_map(Take::lines));
        lines.extend(self.scripts.iter().map(|s| s.line));
        lines.into_iter().collect()
    }

    pub fn processing(&self, take: &Take) -> Processing {
        Processing {
            measure: self.tidies(take).then(|| take.measure().copied()).flatten(),
            stadium: self.stadium,
            line: take.line,
        }
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
        if !self.stadium && (!tidy || settled) && take.format.kept(self.stepmania) {
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
        let body = if self.tidies(take) {
            m.tidied_seconds()
        } else {
            m.seconds
        };
        Some(if self.stadium {
            super::stadium::seconds(body, take.line)
        } else {
            body
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
        if let (Some(limit), Some(seconds)) = (
            take.lines().filter_map(limit).reduce(f64::min),
            self.seconds(take),
        ) && seconds > limit
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
            // A measured take's seconds as written ([`Draft::seconds`]).
            bytes += match (self.write_of(t), t.measure().zip(self.seconds(t))) {
                (Write::AsIs, _) | (Write::Opus { .. } | Write::Wav { .. }, None) => t.bytes,
                (Write::Opus { .. }, Some((m, s))) => {
                    ddi_platform::ogg_opus::len(s, usize::from(m.channels))
                }
                (Write::Wav { .. }, Some((m, s))) => {
                    crate::wav::len((s * f64::from(m.rate)).round() as u64, m.channels)
                }
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

    /// Whether one more recording of `bytes` (its file's size) is taken
    /// in: it is no larger than a recording is ([`TAKE_MAX_BYTES`]) and
    /// the draft's recordings stay within [`MAX_FILES`] files and
    /// [`DRAFT_MAX_BYTES`] as they came. One that is not is left out (a
    /// folder that is no pack, picked by mistake, stops there instead of
    /// filling the storage and the memory).
    pub fn has_room(&self, bytes: u64) -> bool {
        let total: u64 = self.takes.iter().map(|t| t.bytes).sum();
        bytes <= TAKE_MAX_BYTES
            && self.takes.len() < MAX_FILES
            && total.saturating_add(bytes) <= DRAFT_MAX_BYTES
    }

    /// The pack's name as its folder has it ("Announcer" when unnamed).
    pub fn folder_name(&self) -> String {
        Some(file_safe(&self.name))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "Announcer".into())
    }

    /// Whether the plain layout (StepMania's: line folders at the pack's
    /// top) cannot say what the draft does, so that its export declares
    /// itself in an `announcer.ini`: two languages or more, or what only
    /// a script says ([`Draft::scripted`]).
    pub fn declares(&self) -> bool {
        self.written_languages().len() > 1 || self.scripted()
    }

    /// Whether its export needs a script (`announcer.lua`): a take written
    /// carries tags, or is said for several lines.
    pub fn scripted(&self) -> bool {
        self.takes
            .iter()
            .any(|t| exported(t) && (!t.tags.is_empty() || t.shared()))
    }

    /// The languages its export offers: those with a take written, in
    /// the draft's order (a language added and not recorded yet is not
    /// offered).
    pub fn written_languages(&self) -> Vec<&str> {
        self.languages
            .iter()
            .map(String::as_str)
            .filter(|l| self.takes.iter().any(|t| exported(t) && self.lang_of(t) == Some(*l)))
            .collect()
    }

    /// The language `take` is written in, where the pack declares its
    /// languages (a declared pack's).
    fn written_lang<'a>(&'a self, take: &'a Take) -> Option<&'a str> {
        if self.declares() {
            self.lang_of(take)
        } else {
            None
        }
    }

    /// What an export writes: each sorted take that was read, in
    /// `Announcers/<Name>/<line folder>/` (a declared pack's under its
    /// language's folder, `Announcers/<Name>/<tag>/<line folder>/`), named
    /// after the file it came as (a number added where two would share a
    /// name), a README with the name, author and license, and a declared
    /// pack's `announcer.ini` ([`Draft::declaration`]).
    pub fn export(&self) -> Export {
        let folder = self.folder_name();
        // By its folder: a language's (where declared) and a line's, or
        // `shared/` (`None`) for a take said for several lines.
        let mut by_line: BTreeMap<(Option<&str>, Option<Line>), Vec<&Take>> = BTreeMap::new();
        for t in self.takes.iter().filter(|t| exported(t)) {
            let place = if t.shared() {
                (None, None)
            } else {
                (self.written_lang(t), t.line)
            };
            by_line.entry(place).or_default().push(t);
        }
        let mut entries = Vec::new();
        for ((lang, line), takes) in by_line {
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
                let path = match (lang, line) {
                    (_, None) => format!("Announcers/{folder}/{SHARED}/{file}"),
                    (Some(lang), Some(line)) => {
                        format!("Announcers/{folder}/{lang}/{}/{file}", line.folder())
                    }
                    (None, Some(line)) => format!("Announcers/{folder}/{}/{file}", line.folder()),
                };
                entries.push(Entry {
                    path,
                    take: t.id,
                    write,
                });
            }
        }
        let mut texts = Vec::new();
        if self.declares() {
            texts.push((
                format!("Announcers/{folder}/{}", super::pool::DECLARATION),
                self.declaration(),
            ));
        }
        if self.scripted() {
            let base = format!("Announcers/{folder}/");
            texts.push((
                format!("{base}{}", super::script::SCRIPT),
                self.script(&entries, &base),
            ));
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
            texts,
        }
    }

    /// What the game's reader must make of `export`'s pack: each take
    /// written (its file in the pack, the lines it is said for, its
    /// language, its tags, a weight of 1), the languages offered and
    /// Mixed, the tags players may switch off.
    fn expected(&self, export: &Export) -> Pool {
        let base = format!("Announcers/{}/", self.folder_name());
        let takes = export
            .entries
            .iter()
            .filter_map(|e| {
                let t = self.take(e.take)?;
                Some(super::pool::Take {
                    file: e.path.strip_prefix(&base).unwrap_or(&e.path).into(),
                    events: t.lines().collect(),
                    lang: self.written_lang(t).map(str::to_string),
                    tags: t.tags.clone(),
                    weight: 1.0,
                })
            })
            .collect();
        let languages: Vec<String> = if self.declares() {
            self.written_languages().into_iter().map(String::from).collect()
        } else {
            Vec::new()
        };
        let carried = |tag: &str| {
            self.takes
                .iter()
                .any(|t| exported(t) && t.tags.iter().any(|g| g == tag))
        };
        Pool {
            takes,
            mixed: self.mixed && languages.len() > 1,
            languages,
            tags: self
                .switches
                .iter()
                .filter(|s| carried(&s.tag))
                .map(|s| super::pool::Tag {
                    name: s.tag.clone(),
                    label: s.label_written(),
                    default: s.default,
                })
                .collect(),
        }
    }

    /// `export`'s pack (the draft's, made from it as it is), its sounds
    /// made with `sizes` (by their paths in the zip), read back through the
    /// game's own reader (the scan, the
    /// declaration, the script, the pool: what a player's game runs) and
    /// compared with the draft: every take with its lines, language, tags
    /// and weight, the languages and Mixed, the switches. The pack as an
    /// installed announcer, or what the reader made otherwise (a mistake
    /// of the studio's, never the author's: [`Unread`]).
    pub fn check(&self, export: &Export, sizes: &[(String, u64)]) -> Result<InstallPlan, Unread> {
        let plan = export.installed(sizes).map_err(Unread)?;
        let (pool, refused) = plan
            .kept
            .pool_with(&ddi_noteskin::params::Values::new(), super::pool::FALLBACK_LANGUAGE);
        if let Some(why) = refused {
            return Err(Unread(format!("its script was refused: {why}")));
        }
        // What the draft writes, whatever `export` holds.
        let want = self.expected(&self.export());
        let key = |t: &super::pool::Take| {
            let mut tags = t.tags.clone();
            tags.sort();
            (
                t.file.to_string(),
                t.events.clone(),
                t.lang.clone(),
                tags,
                t.weight.to_bits(),
            )
        };
        let mut got: Vec<_> = pool.takes.iter().map(key).collect();
        let mut wanted: Vec<_> = want.takes.iter().map(key).collect();
        got.sort();
        wanted.sort();
        // Past what an import keeps, the last sounds are left out (the
        // page says so beforehand, [`Draft::too_big`]).
        let cut = self.too_big();
        if let Some(w) = wanted.iter().find(|w| !got.contains(w))
            && !cut
        {
            return Err(Unread(format!("{} reads as {:?}", w.0, got.iter().find(|g| g.0 == w.0))));
        }
        if let Some(g) = got.iter().find(|g| !wanted.contains(g)) {
            return Err(Unread(format!("{} reads as {g:?}, not as written", g.0)));
        }
        if (pool.languages.as_slice(), pool.mixed) != (want.languages.as_slice(), want.mixed) {
            return Err(Unread(format!(
                "its languages read as {:?} (mixed: {}), not {:?} (mixed: {})",
                pool.languages, pool.mixed, want.languages, want.mixed
            )));
        }
        if pool.tags != want.tags {
            return Err(Unread(format!(
                "its switches read as {:?}, not {:?}",
                pool.tags, want.tags
            )));
        }
        Ok(plan)
    }

    /// A pack's `announcer.lua` (`docs/announcer-lines.md`, "Made in the
    /// studio"), in the documented API alone and the same for the same
    /// draft: every sound taken (its folder's line, in its language
    /// folder's language), those with more to say listed by their paths
    /// in the pack (`base` cut from `entries`' paths), then the tags
    /// players may switch off and the languages offered.
    fn script(&self, entries: &[Entry], base: &str) -> String {
        let mut listed: Vec<(String, String)> = Vec::new();
        for e in entries {
            let Some(t) = self.take(e.take) else {
                continue;
            };
            let mut fields = Vec::new();
            if t.shared() {
                let events: Vec<String> = t.lines().map(Line::folder).collect();
                fields.push(format!("events = {}", lua_list(&events)));
                if let Some(lang) = self.written_lang(t).filter(|_| !self.languages.is_empty()) {
                    fields.push(format!("lang = {}", lua_string(lang)));
                }
            }
            if !t.tags.is_empty() {
                fields.push(format!("tags = {}", lua_list(&t.tags)));
            }
            if !fields.is_empty() {
                let inside = e.path.strip_prefix(base).unwrap_or(&e.path);
                listed.push((inside.to_string(), fields.join(", ")));
            }
        }
        listed.sort();
        let mut lua = String::from(
            "-- announcer.lua, written by Dito Dito Infinito's announcer studio.\n\
             -- Run once as the pack is chosen, it says what each recording may be\n\
             -- said for (\"A script\" in the game's docs/announcer-lines.md): a\n\
             -- recording in a line's folder is said for that line, in its language\n\
             -- folder's language; those listed say more of themselves.\n\
             local pack = Announcer.pack()\n\n\
             local listed = {\n",
        );
        for (file, fields) in &listed {
            lua += &format!("  [{}] = {{ {fields} }},\n", lua_string(file));
        }
        lua += "}\n\nfor _, file in ipairs(Announcer.files()) do\n  pack:take(file, listed[file])\nend\n";
        let carried = |tag: &str| {
            self.takes
                .iter()
                .any(|t| exported(t) && t.tags.iter().any(|g| g == tag))
        };
        let switches: Vec<&Switch> = self.switches.iter().filter(|s| carried(&s.tag)).collect();
        if !switches.is_empty() {
            lua += "\n";
        }
        for s in switches {
            lua += &format!(
                "pack:tag({}, {{ label = {}, default = {} }})\n",
                lua_string(&s.tag),
                lua_string(&s.label_written()),
                s.default
            );
        }
        let languages: Vec<String> = self.written_languages().into_iter().map(String::from).collect();
        if !languages.is_empty() {
            let mixed = if self.mixed && languages.len() > 1 {
                ", { mixed = true }"
            } else {
                ""
            };
            lua += &format!("pack:languages({}{mixed})\n", lua_list(&languages));
        }
        lua += "return pack\n";
        lua
    }

    /// A declared pack's `announcer.ini` (`docs/announcer-lines.md`,
    /// "Made in the studio"): its name, and its languages, each named in
    /// itself, offered mixed where the draft says so.
    pub fn declaration(&self) -> String {
        let mut ini = String::from(
            "; announcer.ini, written by Dito Dito Infinito's announcer studio.\n\n[Announcer]\n",
        );
        let name = one_line(&self.name);
        if !name.is_empty() {
            ini += &format!("Name={name}\n");
        }
        let languages = self.written_languages();
        if !languages.is_empty() {
            let labels: Vec<&str> = languages.iter().map(|l| language_name(l)).collect();
            ini += "\n[Parameters]\nList=Language\n\n[Parameter Language]\nType=Language\n";
            ini += &format!("Languages={}\n", languages.join(","));
            ini += &format!("Labels={}\n", labels.join(","));
            if self.mixed && languages.len() > 1 {
                ini += "Mixed=1\n";
            }
        }
        ini
    }
}

/// `text` as a Lua string, quoted: `"` and `\\` escaped, control
/// characters as their decimal escapes.
fn lua_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_ascii_control() => out.push_str(&format!("\\{:03}", c as u32)),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `items` as a Lua list of strings: `{ "a", "b" }`.
fn lua_list(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|i| lua_string(i)).collect();
    format!("{{ {} }}", quoted.join(", "))
}

/// `text` on one line: no line breaks or other control characters, its
/// ends trimmed (a name in `announcer.ini`).
fn one_line(text: &str) -> String {
    let spaced: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    // A line ending in `\\` goes on into the next in `.ini` files.
    spaced.trim().trim_end_matches('\\').trim().to_string()
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
