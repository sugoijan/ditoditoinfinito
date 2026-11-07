//! Announcers (`docs/plans/announcers.md`, step 1): the lines StepMania
//! 5.1 and Etterna speak, 3.9's that packs still carry, our own, and the
//! announcers found in an import's files. The layout is StepMania's,
//! `Announcers/<Name>/<line folder>/`, the sounds directly in the line
//! folder, names matched without regard to case
//! (`docs/research/announcers.md`). The announcer studio's drafts are
//! [`draft`], tidied as [`tidy`] says, with [`examples`] of each line
//! (`docs/plans/announcer-studio.md`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::pack::{extension, file_name, is_ignored, parent};
use crate::path::{RelPath, RelPathBuf};
use crate::report::{AnnouncerSummary, Finding};

/// The sound files read in a line folder: StepMania's
/// (`GameSoundManager::DoPlayOnceFromDir`), and Opus and FLAC, which
/// both shells decode as they do songs' music (StepMania passes them
/// over; the studio keeps them as recorded, `draft`).
pub const AUDIO_EXTENSIONS: [&str; 6] = ["mp3", "wav", "ogg", "oga", "opus", "flac"];
/// An announcer's sound files kept, at most; the rest are left out.
pub const MAX_FILES: usize = 2000;
/// An announcer's sound files kept, in bytes, at most.
pub const MAX_BYTES: u64 = 128 << 20;
/// The announcers kept from one import, at most.
pub const MAX_ANNOUNCERS: usize = 50;
/// The known lines a folder holds to be an announcer (but under an
/// `Announcers` folder, where one is enough).
const MIN_LINES: usize = 3;

/// Seconds between the periodic comments (`SecondsBetweenComments`;
/// `ddi_session::announcer` speaks them, the studio's notes measure takes
/// against them).
pub const COMMENT_GAP: f64 = 10.0;
/// Seconds a combo line waits after the last gated line (5.1's 2 s).
pub const COMBO_GAP: f64 = 2.0;
/// `evaluation cheer` this long after the results' line
/// (`CheerDelaySeconds`).
pub const CHEER_DELAY: f64 = 2.5;
/// `here we go` ends this long before the first note.
pub const GO_BEFORE: f64 = 0.5;

/// A results grade as the lines name it: StepMania's seven tiers and a
/// failed play (`GradeToOldString`), and Etterna's `aaaaa` and `f`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Grade {
    Aaaaa,
    Aaaa,
    Aaa,
    Aa,
    A,
    B,
    C,
    D,
    /// StepMania's failed play.
    E,
    /// Etterna's failed play.
    F,
}

impl Grade {
    pub const ALL: [Grade; 10] = [
        Grade::Aaaaa,
        Grade::Aaaa,
        Grade::Aaa,
        Grade::Aa,
        Grade::A,
        Grade::B,
        Grade::C,
        Grade::D,
        Grade::E,
        Grade::F,
    ];

    fn name(self) -> &'static str {
        match self {
            Grade::Aaaaa => "aaaaa",
            Grade::Aaaa => "aaaa",
            Grade::Aaa => "aaa",
            Grade::Aa => "aa",
            Grade::A => "a",
            Grade::B => "b",
            Grade::C => "c",
            Grade::D => "d",
            Grade::E => "e",
            Grade::F => "f",
        }
    }
}

/// Song select's comment on the song chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Comment {
    /// Played this session.
    Repeat,
    /// Never played.
    New,
    /// Its meter is high.
    Hard,
    General,
}

impl Comment {
    pub const ALL: [Comment; 4] = [
        Comment::Repeat,
        Comment::New,
        Comment::Hard,
        Comment::General,
    ];

    fn name(self) -> &'static str {
        match self {
            Comment::Repeat => "repeat",
            Comment::New => "new",
            Comment::Hard => "hard",
            Comment::General => "general",
        }
    }
}

/// A line the game speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Line {
    /// `gameplay ready`, as a play begins.
    Ready,
    /// `gameplay here we go normal`, before the first note.
    HereWeGo,
    /// `gameplay restart` (ours), a quick restart's in place of `Ready`.
    Restart,
    /// `gameplay comment hot`: the periodic comment at full life.
    CommentHot,
    /// `gameplay comment good`: the periodic comment otherwise.
    CommentGood,
    /// `gameplay comment danger`: the periodic comment in danger.
    CommentDanger,
    /// `gameplay comment oni`: the periodic comment on a battery gauge.
    CommentOni,
    /// `gameplay <N> combo`, N a hundred: StepMania's to 1000, ours past.
    Combo(u32),
    /// `gameplay combo overflow`: each hundred past 1000.
    ComboOverflow,
    /// `gameplay combo stopped`: a combo above 50 broken.
    ComboStopped,
    /// `gameplay full combo` (ours): the last note keeps a full combo.
    FullCombo,
    /// `gameplay full combo w1|w2|w3` (ours): of that kind.
    FullComboW(u8),
    /// `gameplay last life` (ours): one life left on a battery gauge.
    LastLife,
    /// `gameplay life lost` (ours): a battery gauge's life lost.
    LifeLost,
    /// `gameplay cleared`: the last note judged.
    Cleared,
    /// `gameplay failed`.
    Failed,
    /// `evaluation quit` (ours): a quit's results.
    Quit,
    /// `evaluation first clear` (ours): a chart's first clear.
    FirstClear,
    /// `evaluation new record`.
    NewRecord,
    /// `evaluation full combo w1…w4`.
    EvaluationFullCombo(u8),
    /// `evaluation <grade>`.
    Grade(Grade),
    /// `evaluation cheer`, after the results' line for a good grade.
    Cheer,
    /// `select music intro`.
    SelectMusicIntro,
    /// `select music comment <comment>`.
    SelectMusic(Comment),
}

/// Where a line is spoken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spoken {
    Play,
    Results,
    Menu,
}

/// Who named a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    StepMania,
    Etterna,
    /// Ours (`docs/plans/announcers.md`, step 6).
    Ours,
}

impl Line {
    /// Every line, combos to 1000 (past it, any hundred is one too).
    pub fn all() -> Vec<Line> {
        let mut all = vec![
            Line::Ready,
            Line::HereWeGo,
            Line::Restart,
            Line::CommentHot,
            Line::CommentGood,
            Line::CommentDanger,
            Line::CommentOni,
        ];
        all.extend((1..=10).map(|n| Line::Combo(n * 100)));
        all.extend([
            Line::ComboOverflow,
            Line::ComboStopped,
            Line::FullCombo,
            Line::FullComboW(1),
            Line::FullComboW(2),
            Line::FullComboW(3),
            Line::LastLife,
            Line::LifeLost,
            Line::Cleared,
            Line::Failed,
            Line::Quit,
            Line::FirstClear,
            Line::NewRecord,
        ]);
        all.extend((1..=4).map(Line::EvaluationFullCombo));
        all.extend(Grade::ALL.map(Line::Grade));
        all.extend([Line::Cheer, Line::SelectMusicIntro]);
        all.extend(Comment::ALL.map(Line::SelectMusic));
        all
    }

    /// Its folder's name, in lower case.
    pub fn folder(self) -> String {
        let fixed = match self {
            Line::Ready => "gameplay ready",
            Line::HereWeGo => "gameplay here we go normal",
            Line::Restart => "gameplay restart",
            Line::CommentHot => "gameplay comment hot",
            Line::CommentGood => "gameplay comment good",
            Line::CommentDanger => "gameplay comment danger",
            Line::CommentOni => "gameplay comment oni",
            Line::Combo(n) => return format!("gameplay {n} combo"),
            Line::ComboOverflow => "gameplay combo overflow",
            Line::ComboStopped => "gameplay combo stopped",
            Line::FullCombo => "gameplay full combo",
            Line::FullComboW(k) => return format!("gameplay full combo w{k}"),
            Line::LastLife => "gameplay last life",
            Line::LifeLost => "gameplay life lost",
            Line::Cleared => "gameplay cleared",
            Line::Failed => "gameplay failed",
            Line::Quit => "evaluation quit",
            Line::FirstClear => "evaluation first clear",
            Line::NewRecord => "evaluation new record",
            Line::EvaluationFullCombo(k) => return format!("evaluation full combo w{k}"),
            Line::Grade(g) => return format!("evaluation {}", g.name()),
            Line::Cheer => "evaluation cheer",
            Line::SelectMusicIntro => "select music intro",
            Line::SelectMusic(c) => return format!("select music comment {}", c.name()),
        };
        fixed.to_string()
    }

    /// The line a folder name (any case) is, StepMania's alias
    /// `gameplay combo <N>` included.
    pub fn parse(name: &str) -> Option<Line> {
        let name = name.to_ascii_lowercase();
        let combo = name
            .strip_prefix("gameplay ")
            .and_then(|r| r.strip_suffix(" combo"))
            .or_else(|| name.strip_prefix("gameplay combo "))
            .filter(|n| n.bytes().all(|b| b.is_ascii_digit()) && !n.starts_with('0'));
        if let Some(n) = combo {
            let n = n.parse::<u32>().ok()?;
            return (n >= 100 && n % 100 == 0).then_some(Line::Combo(n));
        }
        Line::all().into_iter().find(|l| l.folder() == name)
    }

    pub fn spoken(self) -> Spoken {
        match self {
            Line::Quit
            | Line::FirstClear
            | Line::NewRecord
            | Line::EvaluationFullCombo(_)
            | Line::Grade(_)
            | Line::Cheer => Spoken::Results,
            Line::SelectMusicIntro | Line::SelectMusic(_) => Spoken::Menu,
            _ => Spoken::Play,
        }
    }

    pub fn origin(self) -> Origin {
        match self {
            Line::Restart
            | Line::FullCombo
            | Line::FullComboW(_)
            | Line::LastLife
            | Line::LifeLost
            | Line::Quit
            | Line::FirstClear => Origin::Ours,
            Line::Combo(n) if n > 1000 => Origin::Ours,
            Line::Grade(Grade::Aaaaa | Grade::F) => Origin::Etterna,
            _ => Origin::StepMania,
        }
    }

    /// The line, then the lines spoken in its place where an announcer
    /// lacks it, in order.
    pub fn chain(self) -> Vec<Line> {
        let mut chain = vec![self];
        match self {
            Line::Combo(n) if n > 1000 => chain.push(Line::ComboOverflow),
            Line::Restart => chain.push(Line::Ready),
            Line::FullComboW(_) => chain.push(Line::FullCombo),
            Line::Grade(Grade::Aaaaa) => chain.push(Line::Grade(Grade::Aaaa)),
            Line::Grade(Grade::E) => chain.push(Line::Grade(Grade::F)),
            Line::Grade(Grade::F) => chain.push(Line::Grade(Grade::E)),
            _ => {}
        }
        chain
    }
}

/// A line is kept by its folder's name ([`Line::folder`]; the studio's
/// drafts).
impl Serialize for Line {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.folder())
    }
}

impl<'de> Deserialize<'de> for Line {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Line, D::Error> {
        let name = String::deserialize(d)?;
        Line::parse(&name)
            .ok_or_else(|| serde::de::Error::custom(format!("no line is named {name:?}")))
    }
}

/// Names StepMania and its forks speak (or 3.9 did) that we do not: a
/// line folder's name, in lower case, to the name it is kept under
/// (StepMania 5.1's alias table maps the screens' names onto 3.9's,
/// `AnnouncerManager.cpp`).
fn unused(name: &str) -> Option<String> {
    const FIXED: &[&str] = &[
        "gameplay intro",
        "gameplay here we go extra",
        "gameplay here we go final",
        "gameplay extra",
        "gameplay oni failed",
        "gameplay oni failed halfway",
        "gameplay battle trick level1",
        "gameplay battle trick level2",
        "gameplay battle trick level3",
        "gameplay battle damage level1",
        "gameplay battle damage level2",
        "gameplay battle damage level3",
        "evaluation n",
        "evaluation win",
        "evaluation lose",
        "select course comment general",
        "hurry up",
        "player options intro",
        "select group intro",
        "name entry",
        "music scroll",
        "title menu game name",
        "select difficulty challenge",
        "select difficulty intro",
        "select style intro",
        "title menu attract",
        "caution",
        "game over",
        "credits",
        "select group comment all music",
        "select group comment general",
        "select player intro",
        "select course intro",
        "select character intro",
    ];
    const DIFFICULTIES: [&str; 7] = [
        "beginner", "easy", "medium", "hard", "oni", "nonstop", "endless",
    ];
    const STYLES: [&str; 4] = ["single", "double", "solo", "versus"];
    const STAGES: [&str; 14] = [
        "1", "2", "3", "4", "5", "6", "final", "extra1", "extra2", "oni", "nonstop", "endless",
        "event", "demo",
    ];
    let name = name.to_ascii_lowercase();
    let aliased = if let Some(rest) = name.strip_prefix("screenselectdifficulty ") {
        format!("select difficulty {rest}")
    } else if let Some(rest) = name.strip_prefix("screenselectstyle ") {
        format!("select style {rest}")
    } else {
        name
    };
    let known = FIXED.contains(&aliased.as_str())
        || aliased
            .strip_prefix("select difficulty comment ")
            .is_some_and(|d| DIFFICULTIES.contains(&d))
        || aliased
            .strip_prefix("select style comment ")
            .is_some_and(|s| STYLES.contains(&s))
        || aliased
            .strip_prefix("stage ")
            .is_some_and(|s| STAGES.contains(&s))
        || aliased
            .strip_prefix("evaluation final ")
            .is_some_and(|g| ["aaaa", "aaa", "aa", "a", "b", "c", "d", "e", "n"].contains(&g));
    known.then_some(aliased)
}

/// A theme's own screen lines (`<Screen> intro`, `<Screen> IdleComment`,
/// `<Screen> comment <choice>`): known once a folder is an announcer, but
/// too loose to find one by (a theme's `BGAnimations` has such names).
fn screen_line(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with("screen")
        && (name.ends_with(" intro")
            || name.ends_with(" idlecomment")
            || name.contains(" comment "))
}

/// What a line folder is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Line(Line),
    /// Known, not spoken (yet): kept under this name.
    Unused(String),
    /// Not known: kept under its name in lower case (a later line may
    /// take it).
    Unknown(String),
}

/// What the folder named `name` is.
pub fn classify(name: &str) -> Kind {
    if let Some(line) = Line::parse(name) {
        Kind::Line(line)
    } else if let Some(kept) = unused(name) {
        Kind::Unused(kept)
    } else if screen_line(name) {
        Kind::Unused(name.to_ascii_lowercase())
    } else {
        Kind::Unknown(name.to_ascii_lowercase())
    }
}

/// An announcer found among an import's files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Announcer {
    /// Its folder (`""`: the import's root).
    pub dir: RelPathBuf,
    pub name: String,
    /// The lines it speaks, each with its sound files.
    pub lines: BTreeMap<Line, Vec<RelPathBuf>>,
    /// Known lines not spoken, by the name they are kept under.
    pub unused: BTreeMap<String, Vec<RelPathBuf>>,
    /// Folders of sounds no engine names, by their name in lower case.
    pub unknown: BTreeMap<String, Vec<RelPathBuf>>,
    pub report: Report,
}

/// What finding an announcer passed over.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// The sound files kept, and their bytes.
    pub files: usize,
    pub bytes: u64,
    /// The bytes of the lines' sounds (those [`Announcer::kept`] keeps).
    pub line_bytes: u64,
    /// Files that are no line's sound: not audio, beside the line
    /// folders, or in folders inside them.
    pub ignored: usize,
    /// Line folders without a sound (as named on disk).
    pub empty: Vec<String>,
    /// Announcers inside its line folders (copies; StepMania never reads
    /// them).
    pub nested: Vec<RelPathBuf>,
    /// Sound files over [`MAX_FILES`] or [`MAX_BYTES`], left out.
    pub left_out: usize,
}

impl Announcer {
    /// Its sound files for `line`, from the first of [`Line::chain`] it
    /// has.
    pub fn files(&self, line: Line) -> Option<(Line, &[RelPathBuf])> {
        line.chain()
            .into_iter()
            .find_map(|l| self.lines.get(&l).map(|f| (l, f.as_slice())))
    }
}

/// An announcer's key: its name in lower case (StepMania matches names
/// without regard to case), the `announcer` setting's value in both
/// shells.
pub fn key(name: &str) -> String {
    name.to_lowercase()
}

/// What a bundled announcer's key starts with: ours, apart from the
/// player's (one imported under the same name is another announcer).
pub const BUNDLED_PREFIX: &str = "bundled:";

/// A bundled announcer's key: [`BUNDLED_PREFIX`], then [`key`].
pub fn bundled_key(name: &str) -> String {
    format!("{BUNDLED_PREFIX}{}", key(name))
}

/// Whether `key` is a bundled announcer's (never removed).
pub fn is_bundled(key: &str) -> bool {
    key.starts_with(BUNDLED_PREFIX)
}

/// What the shells keep of an announcer: the lines it speaks, by their
/// folder ([`Line::folder`]), each with its sounds relative to the
/// announcer's folder. The web stores it with those sounds (and nothing
/// of the names no line uses); desktop reads them where they are.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kept {
    pub name: String,
    /// A folder no line has (a later version's) is passed over.
    pub lines: BTreeMap<String, Vec<RelPathBuf>>,
    /// The sounds, and their bytes.
    pub files: usize,
    pub bytes: u64,
    /// One of ours, which comes with the game (`assets/announcers/`):
    /// its key is [`bundled_key`].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bundled: bool,
}

impl Kept {
    pub fn key(&self) -> String {
        if self.bundled {
            bundled_key(&self.name)
        } else {
            key(&self.name)
        }
    }

    /// Its sounds for `line`, from the first of [`Line::chain`] it has.
    pub fn files(&self, line: Line) -> Option<(Line, &[RelPathBuf])> {
        line.chain()
            .into_iter()
            .find_map(|l| self.lines.get(&l.folder()).map(|f| (l, f.as_slice())))
    }

    /// The sound Options speaks when it is chosen: one of `select music
    /// intro`'s (the `pick`th, wrapping), else of the first line it has.
    pub fn sample(&self, pick: usize) -> Option<&RelPathBuf> {
        let files = self
            .files(Line::SelectMusicIntro)
            .map(|(_, f)| f)
            .or_else(|| {
                self.lines
                    .values()
                    .find(|f| !f.is_empty())
                    .map(Vec::as_slice)
            })?;
        files.get(pick % files.len().max(1))
    }
}

/// The bundled announcers (`announcers/index.json` on the web, which
/// `xtask gen-announcers` writes from `assets/announcers/`; desktop reads
/// the folder itself).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    pub announcers: Vec<Bundled>,
}

/// A bundled announcer in [`Index`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bundled {
    /// Its folder under `assets/announcers/` (its name).
    pub path: String,
    /// Its lines' sounds, relative to that folder ([`Kept::bundled`]).
    pub kept: Kept,
    /// Each sound's content hash, by its path in [`Kept::lines`]: its
    /// URL's `?v=`, so a new build's sound never comes from an old one's
    /// cache.
    pub hashes: BTreeMap<String, String>,
    /// What each sound says, by the same paths (its `PROVENANCE.toml`'s
    /// `[said]`): the announcer studio's examples.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub said: BTreeMap<String, String>,
}

/// The first `n` of `items` (the report names a few and counts the
/// rest).
fn first(items: &[String], n: usize) -> Vec<String> {
    items[..items.len().min(n)].to_vec()
}

/// What an import says of an announcer (`ddi_ui::screens::import_report`
/// words it).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Said {
    /// What it speaks: its lines, where, its sounds and their size.
    pub summary: AnnouncerSummary,
    /// What it lost: sounds past the caps.
    pub warnings: Vec<Finding>,
    /// What makes no difference: files passed over, empty folders,
    /// nested copies, names no line uses.
    pub details: Vec<Finding>,
}

impl Announcer {
    /// What the shells keep of it ([`Kept`]): its lines' sounds, the
    /// paths relative to its folder.
    pub fn kept(&self) -> Kept {
        let mut kept = Kept {
            name: self.name.clone(),
            bytes: self.report.line_bytes,
            ..Kept::default()
        };
        for (line, files) in &self.lines {
            let files: Vec<RelPathBuf> = files
                .iter()
                .map(|f| f.strip_prefix(&self.dir).unwrap_or(f).to_rel_path_buf())
                .collect();
            kept.files += files.len();
            kept.lines.insert(line.folder(), files);
        }
        kept
    }

    /// What an import says of it.
    pub fn said(&self) -> Said {
        let by = |s: Spoken| self.lines.keys().filter(|l| l.spoken() == s).count();
        let summary = AnnouncerSummary {
            lines: self.lines.len(),
            in_play: by(Spoken::Play),
            on_results: by(Spoken::Results),
            on_select: by(Spoken::Menu),
            sounds: self.lines.values().map(Vec::len).sum(),
            bytes: self.report.line_bytes,
        };
        let r = &self.report;
        let mut warnings = Vec::new();
        if r.left_out > 0 {
            warnings.push(Finding::AnnouncerSoundsLeftOut {
                count: r.left_out,
                max_files: MAX_FILES,
                max_megabytes: MAX_BYTES >> 20,
            });
        }
        let mut details = Vec::new();
        if !self.unused.is_empty() {
            let names: Vec<String> = self.unused.keys().cloned().collect();
            details.push(Finding::AnnouncerOldFolders {
                count: names.len(),
                names: first(&names, 6),
            });
        }
        if !self.unknown.is_empty() {
            let names: Vec<String> = self.unknown.keys().cloned().collect();
            details.push(Finding::AnnouncerUnknownFolders {
                count: names.len(),
                names: first(&names, 6),
            });
        }
        if !r.empty.is_empty() {
            details.push(Finding::AnnouncerEmptyFolders {
                count: r.empty.len(),
                names: first(&r.empty, 6),
            });
        }
        if !r.nested.is_empty() {
            let copies: Vec<String> = r.nested.iter().map(|n| n.to_string()).collect();
            details.push(Finding::AnnouncerNestedCopies {
                count: copies.len(),
                names: first(&copies, 3),
            });
        }
        if r.ignored > 0 {
            details.push(Finding::AnnouncerOtherFiles { count: r.ignored });
        }
        Said {
            summary,
            warnings,
            details,
        }
    }
}

/// The announcers among an import's files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Found {
    pub announcers: Vec<Announcer>,
    /// Announcers past [`MAX_ANNOUNCERS`], left out.
    pub left_out: usize,
}

fn is_audio(path: &str) -> bool {
    extension(path).is_some_and(|e| AUDIO_EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// The announcers' folders among `paths` (files, `/`-separated), each with
/// the announcers nested in it. A folder is one when at least three of its
/// folders are lines StepMania or we know (by name or alias, not a theme's
/// screen line) holding a sound, or one under a folder named
/// `Announcers`; never one inside another announcer, under `Themes` or
/// named `Empty` (StepMania's template, which it hides).
fn detect<'a>(paths: impl Iterator<Item = &'a str>) -> Vec<(String, Vec<RelPathBuf>)> {
    let mut audio_dirs: std::collections::BTreeSet<&str> = Default::default();
    for p in paths {
        if !is_ignored(p) && is_audio(p) {
            audio_dirs.insert(parent(p));
        }
    }
    let mut lines: BTreeMap<&str, usize> = BTreeMap::new();
    for dir in &audio_dirs {
        if dir.is_empty() {
            continue;
        }
        let name = file_name(dir);
        if Line::parse(name).is_some() || unused(name).is_some() {
            *lines.entry(parent(dir)).or_default() += 1;
        }
    }
    let mut found: Vec<(String, Vec<RelPathBuf>)> = Vec::new();
    for (dir, count) in lines {
        let components: Vec<String> = dir.split('/').map(str::to_ascii_lowercase).collect();
        let under_announcers =
            components.len() >= 2 && components[components.len() - 2] == "announcers";
        if !(count >= MIN_LINES || under_announcers)
            || components.iter().any(|c| c == "themes")
            || file_name(dir).eq_ignore_ascii_case("empty")
        {
            continue;
        }
        if let Some(outer) = found
            .iter_mut()
            .find(|(f, _)| RelPath::new(dir).starts_with(RelPath::new(f)))
        {
            outer.1.push(dir.into());
            continue;
        }
        found.push((dir.to_string(), Vec::new()));
    }
    found
}

/// The announcers' folders among `paths` (files, `/`-separated): see
/// [`find_announcers`].
pub fn announcer_dirs<S: AsRef<str>>(paths: &[S]) -> Vec<RelPathBuf> {
    detect(paths.iter().map(AsRef::as_ref))
        .into_iter()
        .map(|(d, _)| d.into())
        .collect()
}

/// The announcers among an import's files (`(path, bytes)`, `/`-separated),
/// in their folders' order. An announcer at the import's root (its line
/// folders there, as some zips have them) is named `root`. Its sounds are
/// the audio files directly in its line folders; the lines it speaks keep
/// theirs first, then the unused lines, then the unknown, up to
/// [`MAX_FILES`] and [`MAX_BYTES`]. Two folders for one line (an alias and
/// its name, or two spellings) give the line their sounds together.
pub fn find_announcers<S: AsRef<str>>(files: &[(S, u64)], root: &str) -> Found {
    let dirs = detect(files.iter().map(|(p, _)| p.as_ref()));
    let mut found = Found::default();
    for (dir, nested) in dirs {
        if found.announcers.len() == MAX_ANNOUNCERS {
            found.left_out += 1;
            continue;
        }
        let name = if dir.is_empty() {
            root.to_string()
        } else {
            file_name(&dir).to_string()
        };
        let mut a = Announcer {
            dir: dir.as_str().into(),
            name,
            report: Report {
                nested,
                ..Report::default()
            },
            ..Announcer::default()
        };
        // Line folder (as on disk) → its sounds and their bytes.
        let mut folders: BTreeMap<&str, Vec<(&str, u64)>> = BTreeMap::new();
        // Line folders holding something other than a sound.
        let mut silent: std::collections::BTreeSet<&str> = Default::default();
        for (path, bytes) in files {
            let path = path.as_ref();
            if is_ignored(path) {
                continue;
            }
            let Some(rel) = RelPath::new(path).strip_prefix(RelPath::new(&dir)) else {
                continue;
            };
            match rel.as_str().split_once('/') {
                Some((folder, file)) if !file.contains('/') && is_audio(file) => {
                    folders.entry(folder).or_default().push((path, *bytes));
                }
                Some((folder, _)) => {
                    a.report.ignored += 1;
                    silent.insert(folder);
                }
                None => a.report.ignored += 1,
            }
        }
        a.report.empty = silent
            .into_iter()
            .filter(|f| !folders.contains_key(f))
            .map(str::to_string)
            .collect();
        // Each folder's kind, the lines first.
        let mut kinds: Vec<(Kind, Vec<(&str, u64)>)> = folders
            .into_iter()
            .map(|(folder, sounds)| (classify(folder), sounds))
            .collect();
        let rank = |k: &Kind| match k {
            Kind::Line(_) => 0,
            Kind::Unused(_) => 1,
            Kind::Unknown(_) => 2,
        };
        kinds.sort_by_key(|(k, _)| rank(k));
        for (kind, mut sounds) in kinds {
            sounds.sort();
            let mut kept = Vec::new();
            for (path, bytes) in sounds {
                if a.report.files == MAX_FILES || a.report.bytes + bytes > MAX_BYTES {
                    a.report.left_out += 1;
                    continue;
                }
                a.report.files += 1;
                a.report.bytes += bytes;
                if matches!(kind, Kind::Line(_)) {
                    a.report.line_bytes += bytes;
                }
                kept.push(RelPathBuf::from(path));
            }
            if kept.is_empty() {
                continue;
            }
            let list = match kind {
                Kind::Line(line) => a.lines.entry(line).or_default(),
                Kind::Unused(name) => a.unused.entry(name).or_default(),
                Kind::Unknown(name) => a.unknown.entry(name).or_default(),
            };
            list.extend(kept);
            list.sort();
        }
        found.announcers.push(a);
    }
    found
}

pub mod draft;
pub mod examples;
pub mod tidy;

#[cfg(test)]
mod tests;
