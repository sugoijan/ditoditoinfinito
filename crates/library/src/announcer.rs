//! Announcers (`docs/plans/announcers.md`, step 1): the lines StepMania
//! 5.1 and Etterna speak, 3.9's that packs still carry, our own, and the
//! announcers found in an import's files. The layout is StepMania's,
//! `Announcers/<Name>/<line folder>/`, the sounds directly in the line
//! folder, names matched without regard to case
//! (`docs/research/announcers.md`). The announcer studio's drafts are
//! [`draft`], tidied as [`tidy`] says, with [`examples`] of each line
//! (`docs/plans/announcer-studio.md`).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::pack::{file_name, is_audio, is_ignored, parent};
use crate::path::{RelPath, RelPathBuf};
use crate::report::{AnnouncerSummary, Finding};

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

    /// The grade's name in its line's folder (`evaluation aa`).
    pub fn name(self) -> &'static str {
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

/// A multiplayer battle's event, as its line names it (ours: the
/// `battle …` lines, `docs/announcer-lines.md`, "In battle"; the events of
/// `docs/plans/multiplayer.md`, "Announcer events implied by the
/// mechanism", that the prototype's stages produce,
/// `docs/plans/multiplayer-prototype-progress.md`, M4). Each says one
/// meaning to the local player; when it fires, its precedence and what
/// shows where a pack lacks it are `ddi_session::announcer::battle`'s.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Battle {
    /// The first stage of a match begins (a one-stage match's too): in
    /// place of `gameplay ready`.
    MatchBegin,
    /// A later stage begins, not the last.
    StageBegin,
    /// The match's last stage begins (announced by the match's length,
    /// never inferred from who is left): a counted match's last stage, or
    /// the stage an open-ended match's operator marked its last song (or
    /// the room's bound made it) (round AF, 2026-11-19: "Final stage for
    /// both, that's what I had meant").
    FinalStage,
    /// The local player's life enters danger while still in (once a
    /// stage).
    LifeCritical,
    /// An attack against the local player was committed: said at once,
    /// before its shocks can be seen.
    AttackIncoming,
    /// The local player's full attack power was spent on an attack of
    /// their own.
    AttackLaunched,
    /// The local player steps up to a harder chart (a match's "Charts:
    /// Adaptive"): said as the new chart's notes begin.
    StepUp,
    /// The local player steps down to an easier chart (their life ran
    /// out on a rung with one below): said as its notes begin.
    StepDown,
    /// The players still in become this many (four or two,
    /// [`SURVIVOR_MILESTONES`]), having been more.
    Survivors(u8),
    /// The local player is out of the stage (their life ran out).
    Eliminated,
    /// The local player alone won the stage, at its confirmation (before
    /// the song's end too).
    StageWon,
    /// The stage ended with the local player still in but not first.
    StageNotWon,
    /// The local player shares the stage's first place.
    StageDrawn,
    /// The stage's winner, having played on, reaches the song's end.
    SongFinished,
    /// The match's results: first alone, not first, sharing first.
    MatchWon,
    MatchNotWon,
    MatchDrawn,
}

/// The players-still-in counts [`Battle::Survivors`] speaks at.
pub const SURVIVOR_MILESTONES: [u8; 2] = [4, 2];

impl Battle {
    pub const ALL: [Battle; 18] = [
        Battle::MatchBegin,
        Battle::StageBegin,
        Battle::FinalStage,
        Battle::LifeCritical,
        Battle::AttackIncoming,
        Battle::AttackLaunched,
        Battle::StepUp,
        Battle::StepDown,
        Battle::Survivors(SURVIVOR_MILESTONES[0]),
        Battle::Survivors(SURVIVOR_MILESTONES[1]),
        Battle::Eliminated,
        Battle::StageWon,
        Battle::StageNotWon,
        Battle::StageDrawn,
        Battle::SongFinished,
        Battle::MatchWon,
        Battle::MatchNotWon,
        Battle::MatchDrawn,
    ];

    /// Its folder's name after `battle `.
    fn name(self) -> String {
        let fixed = match self {
            Battle::MatchBegin => "match begin",
            Battle::StageBegin => "stage begin",
            Battle::FinalStage => "final stage",
            Battle::LifeCritical => "life critical",
            Battle::AttackIncoming => "attack incoming",
            Battle::AttackLaunched => "attack launched",
            Battle::StepUp => "step up",
            Battle::StepDown => "step down",
            Battle::Survivors(n) => return format!("survivors {n}"),
            Battle::Eliminated => "eliminated",
            Battle::StageWon => "stage won",
            Battle::StageNotWon => "stage not won",
            Battle::StageDrawn => "stage drawn",
            Battle::SongFinished => "song finished",
            Battle::MatchWon => "match won",
            Battle::MatchNotWon => "match not won",
            Battle::MatchDrawn => "match drawn",
        };
        fixed.to_string()
    }

    /// One of the stage's openings, said in place of `gameplay ready`.
    pub fn opens(self) -> bool {
        matches!(
            self,
            Battle::MatchBegin | Battle::StageBegin | Battle::FinalStage
        )
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
    /// `gameplay <N> combo`: StepMania's each hundred to 1000; ours any
    /// other count (past 1000, or a pack counting by 64s).
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
    /// `battle <event>` (ours): a multiplayer battle's event.
    Battle(Battle),
}

/// Where a line is spoken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spoken {
    Play,
    Results,
    Menu,
    /// In multiplayer: a battle's stages, in play and on their results
    /// (never in a play of one's own).
    Battle,
}

/// Who named a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    StepMania,
    Etterna,
    /// Ours (`docs/plans/announcers.md`, step 6).
    Ours,
}

/// Every line of [`Line::all`] with its folder, in that order, made once
/// (an import reads a folder's name against them all).
pub(crate) fn folders() -> &'static [(String, Line)] {
    static FOLDERS: std::sync::LazyLock<Vec<(String, Line)>> = std::sync::LazyLock::new(|| {
        Line::all()
            .into_iter()
            .map(|line| (line.folder(), line))
            .collect()
    });
    &FOLDERS
}

impl Line {
    /// Every line, combos StepMania's hundreds to 1000 (any other count is
    /// one too: [`Line::Combo`]).
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
        all.extend(Battle::ALL.map(Line::Battle));
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
            Line::Battle(b) => return format!("battle {}", b.name()),
        };
        fixed.to_string()
    }

    /// The line a folder name (any case) is, StepMania's alias
    /// `gameplay combo <N>` included, N any count from 1.
    pub fn parse(name: &str) -> Option<Line> {
        let name = name.to_ascii_lowercase();
        let combo = name
            .strip_prefix("gameplay ")
            .and_then(|r| r.strip_suffix(" combo"))
            .or_else(|| name.strip_prefix("gameplay combo "))
            .filter(|n| n.bytes().all(|b| b.is_ascii_digit()) && !n.starts_with('0'));
        if let Some(n) = combo {
            let n = n.parse::<u32>().ok()?;
            return (n >= 1).then_some(Line::Combo(n));
        }
        folders()
            .iter()
            .find(|(folder, _)| *folder == name)
            .map(|&(_, line)| line)
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
            Line::Battle(_) => Spoken::Battle,
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
            | Line::FirstClear
            | Line::Battle(_) => Origin::Ours,
            Line::Combo(n) if !stepmanias_combo(n) => Origin::Ours,
            Line::Grade(Grade::Aaaaa | Grade::F) => Origin::Etterna,
            _ => Origin::StepMania,
        }
    }

    /// The line, then the lines spoken in its place where an announcer
    /// lacks it, in order. A battle's opening falls back on `battle stage
    /// begin`, then on `gameplay ready` (whose role it takes); its life
    /// critical on the danger comment, its elimination on `gameplay
    /// failed` and the winner's finished song on `gameplay cleared`, each
    /// of which says that moment in a play; the rest on nothing (an
    /// attack's two and a step's two among them: no play has that
    /// moment).
    pub fn chain(self) -> Vec<Line> {
        let mut chain = vec![self];
        match self {
            Line::Battle(Battle::MatchBegin | Battle::FinalStage) => {
                chain.extend([Line::Battle(Battle::StageBegin), Line::Ready])
            }
            Line::Battle(Battle::StageBegin) => chain.push(Line::Ready),
            Line::Battle(Battle::LifeCritical) => chain.push(Line::CommentDanger),
            Line::Battle(Battle::Eliminated) => chain.push(Line::Failed),
            Line::Battle(Battle::SongFinished) => chain.push(Line::Cleared),
            Line::Combo(n) if past_1000(n) => chain.push(Line::ComboOverflow),
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

impl Line {
    /// The combo lines (`battle-voice-v1` speaks them in a stage too,
    /// below every battle line): the combo reaching a count
    /// ([`Line::Combo`]), each hundred past 1000 where the pack has no
    /// count of its own ([`Line::ComboOverflow`]) and a combo above 50
    /// broken ([`Line::ComboStopped`]). Not the full combos, which are
    /// the play's end (a stage speaks those too, `battle-voice-v3`).
    pub fn is_combo(self) -> bool {
        matches!(self, Line::Combo(_) | Line::ComboOverflow | Line::ComboStopped)
    }
}

/// Whether a combo is one of StepMania's lines: a hundred to 1000.
pub fn stepmanias_combo(n: u32) -> bool {
    (100..=1000).contains(&n) && n.is_multiple_of(100)
}

/// Whether a combo is a hundred past 1000, which `gameplay combo
/// overflow` speaks for where the pack has no line of its own.
pub fn past_1000(n: u32) -> bool {
    n > 1000 && n.is_multiple_of(100)
}

/// The combo lines our packs have, which the studio lists to begin with
/// (`assets/announcers/`).
pub const OUR_COMBOS: [u32; 12] = [
    100, 200, 300, 400, 500, 600, 700, 800, 900, 1000, 1500, 2000,
];

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
    /// A declared pack's ([`Announcer::declaration`]) lines in each
    /// language, by its folder's tag ([`pool::language_tag`]): the line
    /// folders under `<dir>/<tag>/`.
    pub voices: BTreeMap<String, BTreeMap<Line, Vec<RelPathBuf>>>,
    /// Its `announcer.ini` ([`pool::DECLARATION`]), if it has one: then
    /// it is one announcer whatever its folders, its language folders
    /// its voices (the shells read the file's text into
    /// [`Kept::ini`]).
    pub declaration: Option<RelPathBuf>,
    /// Its `announcer.lua` ([`script::SCRIPT`]), if it has one: a
    /// declaration too, and the pack's sounds outside its line folders
    /// (its [`Announcer::unused`] and [`Announcer::unknown`] folders')
    /// are kept for it to name ([`Kept::others`]).
    pub script: Option<RelPathBuf>,
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

    /// Every line it has a sound of, in any language.
    fn every_line(&self) -> BTreeSet<Line> {
        self.lines
            .keys()
            .chain(self.voices.values().flat_map(|v| v.keys()))
            .copied()
            .collect()
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
    /// A declared pack's lines in each language, by tag, as [`Kept::lines`]
    /// has its own (paths relative to the announcer's folder, so under
    /// the language's folder): the voices its Language parameter chooses
    /// among ([`Kept::resolve`]). Records stored before them read none.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub voices: BTreeMap<String, BTreeMap<String, Vec<RelPathBuf>>>,
    /// Its `announcer.ini`'s text ([`pool::Declaration`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ini: Option<String>,
    /// What it was resolved with, among its shuffles (`""`: not
    /// resolved, or a pack without choices; [`Kept::resolve`]).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub choice: String,
    /// Its `announcer.lua`'s text ([`script`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    /// A scripted pack's sounds outside its line folders, which its
    /// script may name (`shared/awesome.ogg`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub others: Vec<RelPathBuf>,
}

impl Kept {
    pub fn key(&self) -> String {
        if self.bundled {
            bundled_key(&self.name)
        } else {
            key(&self.name)
        }
    }

    /// The key its lines' shuffles are kept under: its own, and what it
    /// was resolved with (English and Japanese take turns apart).
    pub fn bag_key(&self) -> String {
        if self.choice.is_empty() {
            self.key()
        } else {
            format!("{}#{}", self.key(), self.choice)
        }
    }

    /// Every sound of it, in its lines, its voices and those its script
    /// may name, each once.
    pub fn sounds(&self) -> BTreeSet<&RelPathBuf> {
        self.lines
            .values()
            .chain(self.voices.values().flat_map(|v| v.values()))
            .flatten()
            .chain(&self.others)
            .collect()
    }

    /// Every list of sounds, its lines', its voices' and its script's
    /// others, to change in place (an import keeps what it stored).
    pub fn sound_lists_mut(&mut self) -> impl Iterator<Item = &mut Vec<RelPathBuf>> {
        self.lines
            .values_mut()
            .chain(self.voices.values_mut().flat_map(|v| v.values_mut()))
            .chain(std::iter::once(&mut self.others))
    }

    /// Drops lines left without a sound, and voices without a line.
    pub fn drop_empty(&mut self) {
        self.lines.retain(|_, f| !f.is_empty());
        for v in self.voices.values_mut() {
            v.retain(|_, f| !f.is_empty());
        }
        self.voices.retain(|_, v| !v.is_empty());
    }

    /// Whether it has no sound of a line at all (a script naming sounds
    /// outside line folders has some).
    pub fn is_silent(&self) -> bool {
        self.lines.is_empty()
            && self.voices.is_empty()
            && (self.script.is_none() || self.others.is_empty())
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

/// The built-in announcer's folder under `assets/announcers/` (one
/// character, a voice a language: `en/`, `ja/`), and so its key
/// ([`BUILT_IN`]).
pub const BUILT_IN_FOLDER: &str = "Clank";
/// The built-in announcer's key, the `announcer` setting's default.
pub const BUILT_IN: &str = "bundled:clank";

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

impl Bundled {
    /// Its voice in language `lang` as a pack of its own: its folder that
    /// language's, its lines that voice's, its hashes and texts by their
    /// paths in it (the announcer studio's examples).
    pub fn voice(&self, lang: &str) -> Option<Bundled> {
        let lines = self.kept.voices.get(lang)?;
        let prefix = format!("{lang}/");
        let within = |p: &str| p.strip_prefix(&prefix).map(str::to_string);
        let rekey = |m: &BTreeMap<String, String>| -> BTreeMap<String, String> {
            m.iter()
                .filter_map(|(k, v)| Some((within(k)?, v.clone())))
                .collect()
        };
        let lines: BTreeMap<String, Vec<RelPathBuf>> = lines
            .iter()
            .map(|(folder, files)| {
                let files = files
                    .iter()
                    .filter_map(|f| within(f.as_str()).map(RelPathBuf::from))
                    .collect();
                (folder.clone(), files)
            })
            .collect();
        Some(Bundled {
            path: format!("{}/{lang}", self.path),
            kept: Kept {
                name: self.kept.display_name(lang),
                files: lines.values().map(Vec::len).sum(),
                lines,
                bundled: true,
                ..Kept::default()
            },
            hashes: rekey(&self.hashes),
            said: rekey(&self.said),
        })
    }
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
        let within = |files: &Vec<RelPathBuf>| -> Vec<RelPathBuf> {
            files
                .iter()
                .map(|f| f.strip_prefix(&self.dir).unwrap_or(f).to_rel_path_buf())
                .collect()
        };
        for (line, files) in &self.lines {
            let files = within(files);
            kept.files += files.len();
            kept.lines.insert(line.folder(), files);
        }
        for (lang, lines) in &self.voices {
            let voice = kept.voices.entry(lang.clone()).or_default();
            for (line, files) in lines {
                let files = within(files);
                kept.files += files.len();
                voice.insert(line.folder(), files);
            }
        }
        // What a script may name beside its line folders.
        if self.script.is_some() {
            for files in self.unused.values().chain(self.unknown.values()) {
                let files = within(files);
                kept.files += files.len();
                kept.others.extend(files);
            }
            kept.others.sort();
        }
        kept
    }

    /// What an import says of it.
    pub fn said(&self) -> Said {
        let every = self.every_line();
        let by = |s: Spoken| every.iter().filter(|l| l.spoken() == s).count();
        let summary = AnnouncerSummary {
            lines: every.len(),
            in_play: by(Spoken::Play),
            on_results: by(Spoken::Results),
            on_select: by(Spoken::Menu),
            sounds: self.lines.values().map(Vec::len).sum::<usize>()
                + self
                    .voices
                    .values()
                    .flat_map(|v| v.values())
                    .map(Vec::len)
                    .sum::<usize>(),
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

/// The announcers' folders among `paths` (files, `/`-separated), each with
/// the announcers nested in it. A folder is one when at least three of its
/// folders are lines StepMania or we know (by name or alias, not a theme's
/// screen line) holding a sound, or one under a folder named
/// `Announcers`; never one inside another announcer, under `Themes` or
/// named `Empty` (StepMania's template, which it hides).
fn detect<'a>(paths: impl Iterator<Item = &'a str>) -> Vec<(String, Vec<RelPathBuf>)> {
    let mut audio_dirs: BTreeSet<&str> = Default::default();
    // The folders holding an `announcer.ini`: one announcer each.
    let mut declared: BTreeSet<&str> = Default::default();
    // A line folder's sounds come in a song's music's formats: StepMania's
    // (`GameSoundManager::DoPlayOnceFromDir`), and Opus and FLAC, which both
    // shells decode as they do songs' music (StepMania passes them over;
    // the studio keeps them as recorded, `draft`).
    for p in paths {
        if is_ignored(p) {
            continue;
        }
        if is_audio(p) {
            audio_dirs.insert(parent(p));
        } else if file_name(p).eq_ignore_ascii_case(pool::DECLARATION)
            || file_name(p).eq_ignore_ascii_case(script::SCRIPT)
        {
            declared.insert(parent(p));
        }
    }
    let mut lines: BTreeMap<&str, usize> = BTreeMap::new();
    for dir in &audio_dirs {
        if dir.is_empty() {
            continue;
        }
        let name = file_name(dir);
        if Line::parse(name).is_some() || unused(name).is_some() {
            // A declared pack's language folder's lines are its own.
            let mut owner = parent(dir);
            if !owner.is_empty()
                && pool::is_language(file_name(owner))
                && declared.contains(parent(owner))
            {
                owner = parent(owner);
            }
            *lines.entry(owner).or_default() += 1;
        }
    }
    for dir in &declared {
        lines.entry(dir).or_default();
    }
    let mut found: Vec<(String, Vec<RelPathBuf>)> = Vec::new();
    for (dir, count) in lines {
        let components: Vec<String> = dir.split('/').map(str::to_ascii_lowercase).collect();
        let under_announcers =
            components.len() >= 2 && components[components.len() - 2] == "announcers";
        if !(count >= MIN_LINES || under_announcers || declared.contains(dir))
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

/// A folder's sounds as found: each path and its bytes.
type Sounds<'a> = Vec<(&'a str, u64)>;

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
        let declares = files.iter().any(|(p, _)| {
            let p = p.as_ref();
            !is_ignored(p)
                && (file_name(p).eq_ignore_ascii_case(pool::DECLARATION)
                    || file_name(p).eq_ignore_ascii_case(script::SCRIPT))
                && parent(p) == dir.as_str()
        });
        // Line folder (as on disk, with its language folder in a declared
        // pack) → its sounds and their bytes.
        let mut folders: BTreeMap<(Option<&str>, &str), Sounds> = BTreeMap::new();
        // Line folders holding something other than a sound.
        let mut silent: BTreeSet<(Option<&str>, &str)> = Default::default();
        for (path, bytes) in files {
            let path = path.as_ref();
            if is_ignored(path) {
                continue;
            }
            let Some(rel) = RelPath::new(path).strip_prefix(RelPath::new(&dir)) else {
                continue;
            };
            let parts: Vec<&str> = rel.as_str().split('/').collect();
            match parts.as_slice() {
                [folder, file] if is_audio(file) => {
                    folders.entry((None, folder)).or_default().push((path, *bytes));
                }
                [lang, folder, file] if declares && pool::is_language(lang) && is_audio(file) => {
                    folders
                        .entry((Some(lang), folder))
                        .or_default()
                        .push((path, *bytes));
                }
                [file] if declares && file.eq_ignore_ascii_case(pool::DECLARATION) => {
                    a.declaration = Some(RelPathBuf::from(path));
                }
                [file] if declares && file.eq_ignore_ascii_case(script::SCRIPT) => {
                    a.script = Some(RelPathBuf::from(path));
                }
                [lang, folder, _, ..] if declares && pool::is_language(lang) => {
                    a.report.ignored += 1;
                    silent.insert((Some(lang), folder));
                }
                [folder, _, ..] => {
                    a.report.ignored += 1;
                    silent.insert((None, folder));
                }
                _ => a.report.ignored += 1,
            }
        }
        a.report.empty = silent
            .into_iter()
            .filter(|f| !folders.contains_key(f))
            .map(|(lang, f)| match lang {
                Some(lang) => format!("{lang}/{f}"),
                None => f.to_string(),
            })
            .collect();
        // Each folder's kind, the lines first.
        let mut kinds: Vec<(Option<&str>, Kind, Sounds)> = folders
            .into_iter()
            .map(|((lang, folder), sounds)| (lang, classify(folder), sounds))
            .collect();
        let rank = |k: &Kind| match k {
            Kind::Line(_) => 0,
            Kind::Unused(_) => 1,
            Kind::Unknown(_) => 2,
        };
        kinds.sort_by_key(|(_, k, _)| rank(k));
        for (lang, kind, mut sounds) in kinds {
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
            let named = |name: String| match lang {
                Some(lang) => format!("{lang}/{name}"),
                None => name,
            };
            let list = match (kind, lang) {
                (Kind::Line(line), None) => a.lines.entry(line).or_default(),
                (Kind::Line(line), Some(lang)) => a
                    .voices
                    .entry(pool::language_tag(lang))
                    .or_default()
                    .entry(line)
                    .or_default(),
                (Kind::Unused(name), _) => a.unused.entry(named(name)).or_default(),
                (Kind::Unknown(name), _) => a.unknown.entry(named(name)).or_default(),
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
pub mod pool;
pub mod script;
pub mod tidy;
pub mod stadium;
pub mod tail;

#[cfg(test)]
mod tests;
