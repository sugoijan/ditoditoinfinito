//! The announcer in play and on the results (`docs/plans/announcers.md`,
//! step 4): when each line speaks, as StepMania 5.1's `ScreenGameplay`
//! and `ScreenEvaluation` have it (`docs/research/announcers.md`,
//! sections 2.2 and 2.3), over what the play already knows each frame.
//! The shells decode the sounds ([`wanted`]: the lines the play can
//! speak, a few of each from the line's shuffle, [`Bags`]) into a
//! [`Voice`] and hand it to the session; the [`Announcer`] plays them on
//! the voice's bus ([`AudioBackend::play_voice`]). Presentation only: it
//! never touches judging, the clock or a play's log, and a line not
//! decoded when it is due is left out (Start never waits for it).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use ddi_engine::player::Results;
use ddi_engine::rules::{FullCombo, GradeTable};
use ddi_engine::transform::SplitMix64;
use ddi_library::announcer::{Battle, Grade, Kept, Line, Spoken};
use ddi_library::path::RelPathBuf;
use ddi_platform::AudioBackend;

pub mod battle;
pub mod scene;

pub use ddi_library::announcer::{CHEER_DELAY, COMBO_GAP, COMMENT_GAP, GO_BEFORE};
/// A combo above this broken says `combo stopped` (`ComboStoppedAt`).
pub const COMBO_STOPPED_AT: u32 = 50;
/// A line starts this far ahead of now on the audio clock, so its start
/// is never in the past by the time it is scheduled.
const LEAD: f64 = 0.03;

/// The lines a play speaks: in play and on its results.
pub fn speaks(line: Line) -> bool {
    matches!(line.spoken(), Spoken::Play | Spoken::Results)
}

/// How many of a line's sounds a play decodes (decision 8: at most
/// four): the comments and the combo past 1000 speak again and again, a
/// broken combo now and then, the rest once a play.
pub fn variants(line: Line) -> usize {
    match line {
        Line::CommentHot
        | Line::CommentGood
        | Line::CommentDanger
        | Line::CommentOni
        | Line::ComboOverflow => 4,
        Line::ComboStopped => 2,
        _ => 1,
    }
}

/// The shuffle each line's sounds are taken in, for the whole run of the
/// game (StepMania 5.1's `g_DirSoundOrder`): none comes again until each
/// has, so a line of a hundred sounds is heard whole over the plays.
#[derive(Clone, Debug)]
pub struct Bags {
    bags: HashMap<(String, Line), Vec<usize>>,
    seed: u64,
}

impl Bags {
    pub fn new(seed: u64) -> Bags {
        Bags {
            bags: HashMap::new(),
            seed,
        }
    }

    /// The next `count` (at most `n`) of the `n` sounds of announcer
    /// `key`'s `line`, each once: taken from its bag, which is shuffled
    /// anew once empty (leaving out those already taken).
    pub fn take(&mut self, key: &str, line: Line, n: usize, count: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let want = count.min(n);
        let mut bag = self
            .bags
            .remove(&(key.to_string(), line))
            .unwrap_or_default();
        bag.retain(|&i| i < n);
        while out.len() < want {
            if bag.is_empty() {
                bag = (0..n).filter(|i| !out.contains(i)).collect();
                let mut rng = SplitMix64(self.seed);
                rng.shuffle(&mut bag);
                self.seed = rng.0;
            }
            match bag.pop() {
                Some(i) => out.push(i),
                None => break,
            }
        }
        self.bags.insert((key.to_string(), line), bag);
        out
    }
}

/// The sounds a play of a chart of `notes` notes decodes: those of the
/// lines it can speak ([`speaks`]; no combo it cannot reach), up to
/// [`variants`] of each, the next of each line's bag, its opening's
/// first.
pub fn wanted(kept: &Kept, notes: u32, bags: &mut Bags) -> Vec<(Line, Vec<RelPathBuf>)> {
    take(kept, bags, |line| {
        speaks(line)
            && match line {
                Line::Combo(n) => n <= notes,
                Line::ComboOverflow => notes > 1000,
                _ => true,
            }
    })
}

/// The sounds song select decodes (step 5): `select music intro` and
/// the chosen song's comments, one of each, the next of each line's bag.
pub fn menu(kept: &Kept, bags: &mut Bags) -> Vec<(Line, Vec<RelPathBuf>)> {
    take(kept, bags, |line| line.spoken() == Spoken::Menu)
}

/// [`variants`] of each of `kept`'s lines `which` keeps, from their bags,
/// those said first first: a play's opening, then here we go; song
/// select's intro.
fn take(
    kept: &Kept,
    bags: &mut Bags,
    which: impl Fn(Line) -> bool,
) -> Vec<(Line, Vec<RelPathBuf>)> {
    let key = kept.key();
    let mut lines: Vec<(Line, Vec<RelPathBuf>)> = kept
        .lines
        .iter()
        .filter_map(|(folder, files)| {
            let line = Line::parse(folder)?;
            if !which(line) || files.is_empty() {
                return None;
            }
            let picks = bags.take(&key, line, files.len(), variants(line));
            Some((line, picks.into_iter().map(|i| files[i].clone()).collect()))
        })
        .collect();
    lines.sort_by_key(|(line, _)| match line {
        Line::Ready | Line::Restart | Line::SelectMusicIntro => 0,
        Line::HereWeGo => 1,
        _ => 2,
    });
    lines
}

/// The announcer's sounds as decoded, by line.
#[derive(Debug)]
pub struct Voice<B> {
    /// Each line's sounds with their ranks, in the order spoken.
    lines: BTreeMap<Line, Vec<(usize, B)>>,
    /// The lines being decoded: one of these not there when it is due
    /// was not decoded in time ([`Report::missing`]); any other the pack
    /// lacks ([`Report::absent`]).
    expected: BTreeSet<Line>,
    /// The next of each line's sounds.
    cursor: BTreeMap<Line, usize>,
}

impl<B> Default for Voice<B> {
    fn default() -> Voice<B> {
        Voice {
            lines: BTreeMap::new(),
            expected: BTreeSet::new(),
            cursor: BTreeMap::new(),
        }
    }
}

impl<B> Voice<B> {
    /// The lines to be decoded ([`wanted`]'s).
    pub fn expect(&mut self, lines: impl IntoIterator<Item = Line>) {
        self.expected.extend(lines);
    }

    /// A sound of `line`, decoded.
    pub fn add(&mut self, line: Line, buffer: B) {
        let sounds = self.lines.entry(line).or_default();
        let rank = sounds.len();
        sounds.push((rank, buffer));
    }

    /// A sound of `line`, the `rank`th of those asked for, decoded in
    /// any order: it is spoken in its place among them, and what was
    /// spoken already stays behind the line's next sound.
    pub fn add_ranked(&mut self, line: Line, rank: usize, buffer: B) {
        let sounds = self.lines.entry(line).or_default();
        let at = sounds.partition_point(|(r, _)| *r < rank);
        sounds.insert(at, (rank, buffer));
        if let Some(next) = self.cursor.get_mut(&line)
            && at < *next
        {
            *next += 1;
        }
    }

    /// The sounds decoded so far.
    pub fn decoded(&self) -> usize {
        self.lines.values().map(Vec::len).sum()
    }

    /// The line of `line`'s chain that has sounds.
    fn find(&self, line: Line) -> Option<Line> {
        line.chain()
            .into_iter()
            .find(|l| self.lines.get(l).is_some_and(|b| !b.is_empty()))
    }

    /// The highest count past `old` and up to `new` it has a combo line
    /// for, decoded or being decoded.
    fn combo_within(&self, old: u32, new: u32) -> Option<u32> {
        self.lines
            .keys()
            .chain(&self.expected)
            .filter_map(|l| match *l {
                Line::Combo(n) if n > old && n <= new => Some(n),
                _ => None,
            })
            .max()
    }

    fn expects(&self, line: Line) -> bool {
        line.chain().iter().any(|l| self.expected.contains(l))
    }

    /// What it holds: the sounds decoded and the lines expected (a line
    /// it had nothing for may be there once either changes).
    fn held(&self) -> (usize, usize) {
        (self.decoded(), self.expected.len())
    }

    /// The sound `line` (or the first of its chain decoded) speaks next.
    pub fn peek(&self, line: Line) -> Option<(Line, &B)> {
        let found = self.find(line)?;
        let sounds = &self.lines[&found];
        let i = self.cursor.get(&found).copied().unwrap_or(0) % sounds.len();
        Some((found, &sounds[i].1))
    }

    /// [`Voice::peek`], the line's next sound then the one after.
    pub fn next(&mut self, line: Line) -> Option<(Line, &B)> {
        let found = self.find(line)?;
        let i = self.cursor.entry(found).or_insert(0);
        let at = *i;
        *i += 1;
        let sounds = &self.lines[&found];
        Some((found, &sounds[at % sounds.len()].1))
    }
}

/// The voice as the shell decodes it and the session speaks it.
pub type SharedVoice<B> = Rc<RefCell<Voice<B>>>;

/// How long song select's line asked for while its sound decodes may
/// wait to be spoken, seconds (later, its moment has passed).
pub const MENU_WAIT: f64 = 1.5;

/// Song select's lines of the announcer chosen (step 5): its sounds as
/// the shell decodes them, the first time a line is asked for, and the
/// line asked for before its sound came, spoken if it comes while due.
/// The times are seconds on any clock the shell keeps.
pub struct MenuVoice<B> {
    key: String,
    pub voice: SharedVoice<B>,
    /// A line asked for before its sound was decoded, and when.
    pending: Option<(Line, f64)>,
}

impl<B: Clone> MenuVoice<B> {
    /// Announcer `key`'s, its sounds decoded into `voice`.
    pub fn new(key: String, voice: SharedVoice<B>) -> MenuVoice<B> {
        MenuVoice {
            key,
            voice,
            pending: None,
        }
    }

    /// The announcer's key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// `line` asked for at `now`: the line found for it and its sound,
    /// to speak now; else it waits for its sound ([`MenuVoice::poll`]),
    /// in place of any line waiting.
    pub fn ask(&mut self, line: Line, now: f64) -> Option<(Line, B)> {
        let found = self.next(line);
        self.pending = found.is_none().then_some((line, now));
        found
    }

    /// The line waiting, at `now`: its sound to speak once decoded, while
    /// it is due ([`MENU_WAIT`]); dropped once it is not.
    pub fn poll(&mut self, now: f64) -> Option<(Line, B)> {
        let (line, at) = self.pending?;
        if now - at > MENU_WAIT {
            self.pending = None;
            return None;
        }
        let found = self.next(line);
        if found.is_some() {
            self.pending = None;
        }
        found
    }

    /// A line waits for its sound.
    pub fn waiting(&self) -> bool {
        self.pending.is_some()
    }

    fn next(&self, line: Line) -> Option<(Line, B)> {
        let mut voice = self.voice.borrow_mut();
        voice.next(line).map(|(found, b)| (found, b.clone()))
    }
}

/// What the history says of a chart (under the rules and the assist it
/// is played with) before a play: the results' first clear and new
/// record are told from it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Prior {
    /// A play of it counted (not autoplayed, not quit).
    pub played: bool,
    /// One of them cleared it.
    pub cleared: bool,
    /// The best score ([`ddi_ui::state::history::score_value`]).
    pub best: f64,
}

impl Prior {
    /// What `history` says of `song`'s chart `chart` played under
    /// `settings`, before the play to come.
    pub fn before(
        history: &ddi_ui::state::history::History,
        song: &ddi_chart::Song,
        chart: usize,
        settings: &ddi_ui::state::settings::Settings,
    ) -> Prior {
        Prior::of(history.best_of_play(song, chart, settings).as_ref())
    }

    fn of(best: Option<&ddi_ui::state::history::Best>) -> Prior {
        use ddi_ui::state::history::{Lamp, score_value};
        best.map_or(Prior::default(), |b| Prior {
            played: b.plays > 0,
            cleared: b.lamp >= Lamp::Clear,
            best: score_value(&b.score),
        })
    }
}

/// What the announcer did in a play, for the record and the tests
/// (`__DDI_DEBUG.announcer`, desktop's `--seconds`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    /// The lines started or scheduled, by folder, and the context time
    /// each starts (one scheduled ahead and cut by a quit is listed).
    pub spoken: Vec<(String, f64)>,
    /// Gated lines that came too soon after another (5.1 drops them), and
    /// a stage's battle lines passed over for one that mattered as much
    /// or more ([`battle::VoicePolicy::priority`]).
    pub dropped: u32,
    /// Lines due that were being decoded but were not yet.
    pub missing: Vec<String>,
    /// Lines due that the announcer has no sound for.
    pub absent: Vec<String>,
}

/// What the results' line is told from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ending<'a> {
    pub quit: bool,
    pub failed: bool,
    /// The ruleset's grade name (`""`: none).
    pub grade: &'a str,
    pub full_combo: FullCombo,
    /// [`ddi_ui::state::history::score_value`].
    pub score: f64,
}

impl<'a> Ending<'a> {
    pub fn of(r: &'a Results) -> Ending<'a> {
        Ending {
            quit: r.quit,
            failed: r.failed,
            grade: &r.grade,
            full_combo: r.full_combo,
            score: ddi_ui::state::history::score_value(&r.score),
        }
    }
}

/// How a play's song seconds fall on the audio clock: song second `t`
/// at `start + (t − offset) / rate`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timing {
    pub start: f64,
    pub offset: f64,
    pub rate: f64,
}

impl Timing {
    /// Where song second `t` falls on the audio clock.
    pub fn at(&self, t: f64) -> f64 {
        self.start + (t - self.offset) / self.rate.max(1e-6)
    }
}

/// The play as the announcer reads it, each frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Moment {
    /// The audio clock now.
    pub now: f64,
    /// Host seconds since the last frame.
    pub dt: f64,
    /// The song second judged to.
    pub song_time: f64,
    pub combo: u32,
    /// The gauge's life, `0..=1`.
    pub life: f32,
    /// The gauge is in danger and the play can fail.
    pub danger: bool,
    /// The lives left, where the gauge counts lives.
    pub lives: Option<u32>,
    pub failed: bool,
    /// The end is known: failed at once, the last note judged (or the
    /// song over, when a play fails at its end).
    pub ended: bool,
    /// The full combo kept, once the end is known.
    pub full_combo: FullCombo,
}

/// When each line speaks in a play and on its results.
pub struct Announcer<B> {
    voice: Option<SharedVoice<B>>,
    /// The voice's gain ([`ddi_ui::state::settings::Settings::voice_volume`]).
    volume: f32,
    /// `None`: nothing to tell first clears and records from (autoplay,
    /// a replay, a playtest).
    prior: Option<Prior>,
    /// Off in calibration (nothing may cue the player).
    on: bool,
    timing: Timing,
    /// The first note's song second.
    first_note: Option<f64>,
    /// The line a scene of the announcer studio's is for: what is not it
    /// is said quieter.
    example: Option<Line>,
    /// What opens the play: `gameplay ready`, or after a quick restart
    /// `gameplay restart` (ours, ready where the pack lacks it).
    opening: Line,
    /// Ready and here we go: decided at the first frame.
    opened: bool,
    /// Here we go still to place (its sound not decoded at the first
    /// frame), and where it may not start before (ready's end).
    go_after: Option<f64>,
    /// Seconds since the last gated line, counted from the play's start
    /// until its end (5.1's `m_fTimeSinceLastDancingComment`, counted
    /// from the dancing state, which begins as the music does); lines
    /// speak only from the first note.
    since: f64,
    combo: u32,
    /// The lives at the last frame, where the gauge counts them.
    lives: Option<u32>,
    /// `gameplay cleared` or `failed` said.
    ended: bool,
    /// The comment last due that had no sound (on a battery gauge, and
    /// the life's line), and what the voice held then: not tried again
    /// each frame until one of them changes.
    unsaid: Option<(bool, Line, (usize, usize))>,
    /// A multiplayer stage's voice ([`battle`]): its opening is the
    /// battle's, its routine lines left out, its battle lines one at a
    /// time.
    stage: Option<battle::StageVoice>,
    pub report: Report,
}

impl<B> Announcer<B> {
    /// `on`: it speaks at all; `first_note`: the chart's first judged
    /// note's song second, if any.
    pub fn new(
        on: bool,
        volume: f32,
        prior: Option<Prior>,
        timing: Timing,
        first_note: Option<f64>,
    ) -> Announcer<B> {
        Announcer {
            voice: None,
            volume,
            prior,
            on,
            timing,
            first_note,
            example: None,
            opening: Line::Ready,
            opened: false,
            go_after: None,
            since: 0.0,
            combo: 0,
            lives: None,
            ended: false,
            unsaid: None,
            stage: None,
            report: Report::default(),
        }
    }

    /// A multiplayer stage's (`crate::battle`): opened by `opening` in
    /// place of ready, spoken under `policy` ([`battle`]).
    pub fn in_stage(mut self, opening: Battle, policy: battle::VoicePolicy) -> Announcer<B> {
        self.opening = Line::Battle(opening);
        self.stage = Some(battle::StageVoice::new(policy));
        self
    }

    /// A stage's opening, once the play has begun: the line it was said
    /// with (`None`: nothing was, or nothing speaks). `None` before.
    pub fn opened_with(&self) -> Option<Option<Line>> {
        if !self.speaking() {
            return Some(None);
        }
        self.stage.as_ref().and_then(|s| s.opened)
    }

    /// A quick restart's play (decision 79): `gameplay restart` opens
    /// it, in place of ready.
    pub fn restarted(mut self) -> Announcer<B> {
        self.opening = Line::Restart;
        self
    }

    /// A staged play's (the announcer studio's scenes): `danced`, a note
    /// came before its start (no ready, no here we go: they open a
    /// play), its combo `combo` at the start and `since` seconds since
    /// the last gated line.
    pub fn staged(mut self, danced: bool, combo: u32, since: f64) -> Announcer<B> {
        self.opened |= danced;
        self.combo = combo;
        self.since = since.max(0.0);
        self
    }

    /// A scene of the announcer studio's for the line `example`: what is
    /// not that example is said quieter ([`scene::example_volume`]).
    pub fn for_example(mut self, example: Option<Line>) -> Announcer<B> {
        self.example = example;
        self
    }

    /// The end's line was said (or due).
    pub fn ended(&self) -> bool {
        self.ended
    }

    /// The sounds to speak with, decoded as they come.
    pub fn set_voice(&mut self, voice: SharedVoice<B>) {
        self.voice = Some(voice);
    }

    /// Whether it has a voice to speak with.
    pub fn speaking(&self) -> bool {
        self.on && self.voice.is_some()
    }

    /// The length of `line`'s next sound, if decoded.
    fn length<A: AudioBackend<Buffer = B>>(&self, audio: &A, line: Line) -> Option<f64> {
        let voice = self.voice.as_ref()?.borrow();
        voice.peek(line).map(|(_, sound)| audio.duration(sound))
    }

    /// Speaks the first of `lines` it has a sound for at `at`; whether
    /// one spoke. What it lacks is reported, the first line's name.
    fn say<A: AudioBackend<Buffer = B>>(&mut self, audio: &mut A, lines: &[Line], at: f64) -> bool {
        self.say_found(audio, lines, at).is_some()
    }

    /// [`Announcer::say`], the line found and spoken (of the first's
    /// chain, or a later one's).
    fn say_found<A: AudioBackend<Buffer = B>>(
        &mut self,
        audio: &mut A,
        lines: &[Line],
        at: f64,
    ) -> Option<Line> {
        let voice = self.voice.clone()?;
        let mut voice = voice.borrow_mut();
        for &line in lines {
            let Some((found, sound)) = voice.next(line) else {
                continue;
            };
            let volume = match self.example {
                Some(example) => scene::example_volume(example, found, self.volume),
                None => self.volume,
            };
            if audio.play_voice(sound, at, volume).is_ok() {
                self.report.spoken.push((found.folder(), at));
                return Some(found);
            }
        }
        if let Some(&first) = lines.first() {
            let name = first.folder();
            if lines.iter().any(|&l| voice.expects(l)) {
                if !self.report.missing.contains(&name) {
                    self.report.missing.push(name);
                }
            } else if !self.report.absent.contains(&name) {
                self.report.absent.push(name);
            }
        }
        None
    }

    /// The combo line due as the combo goes from `old` to `new`: the
    /// highest count reached that the voice has a line for (any count:
    /// `gameplay 64 combo`), else the hundred crossed (StepMania's to
    /// 1000; past it, `combo overflow` where the pack has no line of its
    /// own, as in 5.1).
    fn combo_due(&self, old: u32, new: u32) -> Option<Line> {
        if new <= old {
            return None;
        }
        let own = self
            .voice
            .as_ref()
            .and_then(|v| v.borrow().combo_within(old, new));
        own.or_else(|| (new / 100 > old / 100 && new >= 100).then_some(new / 100 * 100))
            .map(Line::Combo)
    }

    /// A gated line: spoken when `gap` seconds have passed since the last
    /// one (which it then is), else dropped.
    fn gated<A: AudioBackend<Buffer = B>>(
        &mut self,
        audio: &mut A,
        lines: &[Line],
        gap: f64,
        now: f64,
    ) {
        if self.since < gap {
            self.report.dropped += 1;
            return;
        }
        if self.say(audio, lines, now + LEAD) {
            self.since = 0.0;
        }
    }

    /// `gameplay ready` (or `restart`) as the play begins and `here we
    /// go` ending [`GO_BEFORE`] before the first note, where the lead-in
    /// has room: for both, else for here we go alone, else for neither
    /// (ready alone where the announcer has no here we go).
    fn open<A: AudioBackend<Buffer = B>>(&mut self, audio: &mut A, now: f64) {
        self.opened = true;
        // What the opening was said with, and until when (a stage's).
        let mut said = None;
        if let Some(first) = self.first_note {
            let start = now + LEAD;
            let by = self.timing.at(first) - GO_BEFORE;
            let ready = self.opening;
            match (
                self.length(audio, ready),
                self.length(audio, Line::HereWeGo),
            ) {
                (Some(r), Some(g)) if start + r <= by - g => {
                    said = self
                        .say_found(audio, &[ready], start)
                        .map(|l| (l, start + r));
                    self.say(audio, &[Line::HereWeGo], by - g);
                }
                (_, Some(g)) if by - g >= start => {
                    self.say(audio, &[Line::HereWeGo], by - g);
                }
                // No room for here we go: nothing.
                (_, Some(_)) => {}
                (Some(r), None) if start + r <= by => {
                    said = self
                        .say_found(audio, &[ready], start)
                        .map(|l| (l, start + r));
                    // Here we go may be decoded yet, and fit after it.
                    self.go_after = Some(start + r);
                }
                (Some(_), None) => self.go_after = Some(start),
                (None, None) => {
                    // Ready due and not there (reported); here we go may
                    // come.
                    self.say(audio, &[ready], start);
                    self.go_after = Some(start);
                }
            }
        }
        if let (Some(stage), Line::Battle(opening)) = (self.stage.as_mut(), self.opening) {
            stage.opened = Some(said.map(|(l, _)| l));
            if let Some((_, until)) = said {
                stage.busy = Some((until, stage.policy.priority(opening)));
            }
        }
    }

    /// One frame of the play.
    pub fn tick<A: AudioBackend<Buffer = B>>(&mut self, audio: &mut A, m: &Moment) {
        if !self.speaking() {
            return;
        }
        if !self.opened {
            self.open(audio, m.now);
        }
        // Here we go, decoded since: where it still fits after ready.
        if let (Some(after), Some(first)) = (self.go_after, self.first_note) {
            let by = self.timing.at(first) - GO_BEFORE;
            match self.length(audio, Line::HereWeGo) {
                Some(g) => {
                    let at = by - g;
                    if at >= after.max(m.now + LEAD) {
                        self.say(audio, &[Line::HereWeGo], at);
                    }
                    self.go_after = None;
                }
                None if m.now + LEAD >= by => self.go_after = None,
                None => {}
            }
        }
        // A stage leaves a play's routine lines out: its battle's moments
        // speak instead ([`Announcer::battle`]).
        if self.stage.is_some() {
            self.combo = m.combo;
            self.lives = m.lives;
            self.ended |= m.ended;
            return;
        }
        if !self.ended {
            self.since += m.dt.clamp(0.0, 0.25);
        }
        let dancing = !self.ended && self.first_note.is_some_and(|f| m.song_time >= f);
        if dancing {
            // A battery gauge's life lost (ours): the last one left said
            // whatever the gate (once a play, and the one that matters),
            // the others behind it; the life that fails the play is the
            // fail's line. It takes the frame from `combo stopped`.
            let mut lost = false;
            if let (Some(old), Some(new)) = (self.lives, m.lives)
                && new < old
                && new > 0
            {
                lost = true;
                if new == 1 {
                    if self.say(audio, &[Line::LastLife], m.now + LEAD) {
                        self.since = 0.0;
                    }
                } else {
                    self.gated(audio, &[Line::LifeLost], COMBO_GAP, m.now);
                }
            }
            // The combo reaching a count the pack has a line for, or
            // crossing a hundred, one line a frame; a combo of more than
            // 50 broken.
            let (old, new) = (self.combo, m.combo);
            if lost {
            } else if let Some(line) = self.combo_due(old, new) {
                self.gated(audio, &[line], COMBO_GAP, m.now);
            } else if old > COMBO_STOPPED_AT && new < COMBO_STOPPED_AT {
                self.gated(audio, &[Line::ComboStopped], COMBO_GAP, m.now);
            }
            if self.since >= COMMENT_GAP && !m.ended {
                let life = if m.life >= 1.0 {
                    Line::CommentHot
                } else if m.danger {
                    Line::CommentDanger
                } else {
                    Line::CommentGood
                };
                // On a battery gauge `oni` (5.1's in courses), else the
                // life's line.
                let both = [Line::CommentOni, life];
                let oni = m.lives.is_some();
                let lines = if oni { &both[..] } else { &both[1..] };
                let held = self.voice.as_ref().map(|v| v.borrow().held());
                let tried = held.map(|h| (oni, life, h));
                if tried != self.unsaid {
                    if self.say(audio, lines, m.now + LEAD) {
                        self.since = 0.0;
                        self.unsaid = None;
                    } else {
                        self.unsaid = tried;
                    }
                }
            }
        }
        self.combo = m.combo;
        self.lives = m.lives;
        if m.ended && !self.ended {
            self.ended = true;
            let at = m.now + LEAD;
            if m.failed {
                self.say(audio, &[Line::Failed], at);
                return;
            }
            // The full combo kept as the last note is judged (ours, the
            // moment DDR says it: its kind's line, else the plain one),
            // then cleared once it has been said.
            let full = match m.full_combo {
                FullCombo::MarvelousFC => Some(Line::FullComboW(1)),
                FullCombo::PerfectFC => Some(Line::FullComboW(2)),
                FullCombo::GreatFC => Some(Line::FullComboW(3)),
                FullCombo::FC => Some(Line::FullCombo),
                FullCombo::None => None,
            };
            let mut cleared = at;
            if let Some(line) = full {
                let len = self.length(audio, line);
                if self.say(audio, &[line], at) {
                    cleared += len.unwrap_or(0.0);
                }
            }
            self.say(audio, &[Line::Cleared], cleared);
        }
    }

    /// The results' line as they open, by 5.1's precedence with ours in
    /// it (a quit's own line alone; a first clear; a new record after an
    /// earlier play; the full combo; the grade; a first play's record
    /// only where nothing else speaks), and `evaluation cheer`
    /// [`CHEER_DELAY`] later for AA or better. `combo_w4`: the ruleset
    /// keeps the combo on the fourth judgment (a plain full combo is
    /// then W4's).
    pub fn results<A: AudioBackend<Buffer = B>>(
        &mut self,
        audio: &mut A,
        now: f64,
        r: &Ending,
        grades: &GradeTable,
        combo_w4: bool,
    ) {
        if !self.speaking() {
            return;
        }
        let at = now + LEAD;
        // A stage's results: a quit's own line; the rest is the battle's,
        // once the room has confirmed the stage ([`Announcer::stage_results`]).
        if self.stage.is_some() {
            if r.quit {
                self.say(audio, &[Line::Quit], at);
            }
            return;
        }
        let lines = results_lines(self.prior, r, grades, combo_w4);
        if lines.is_empty() {
            return;
        }
        let spoke = self.say(audio, &lines, at);
        if spoke && cheers(r, grades) {
            self.say(audio, &[Line::Cheer], at + CHEER_DELAY);
        }
    }
}

impl<B> Announcer<B> {
    /// A stage's battle `event` at `now` (audio clock): its line (or what
    /// speaks in its place) said at once where the policy lets it, else
    /// passed over, never delayed. The line said, if one was.
    pub fn battle<A: AudioBackend<Buffer = B>>(
        &mut self,
        audio: &mut A,
        event: Battle,
        now: f64,
    ) -> Option<Line> {
        if !self.speaking() {
            return None;
        }
        let at = now + LEAD;
        let stage = self.stage.as_ref()?;
        if !stage.may_speak(event, at) {
            self.report.dropped += 1;
            return None;
        }
        let priority = stage.policy.priority(event);
        let line = Line::Battle(event);
        let length = self.length(audio, line).unwrap_or(0.0);
        let said = self.say_found(audio, &[line], at)?;
        if let Some(stage) = self.stage.as_mut() {
            stage.busy = Some((at + length, priority));
        }
        Some(said)
    }

    /// A stage's end as its results will tell it (`crate::battle::Battle::
    /// end_event`), kept until the room has confirmed it.
    pub fn stage_end(&mut self, event: Option<Battle>) {
        if let Some(stage) = self.stage.as_mut() {
            stage.end = event;
        }
    }

    /// The stage's results' line once the room has confirmed it, at `now`
    /// (audio clock): on a match's last stage the match's `ending` (where
    /// the pack has it: one outcome, never two stacked), else the stage's
    /// end kept. The line said, if one was.
    pub fn stage_results<A: AudioBackend<Buffer = B>>(
        &mut self,
        audio: &mut A,
        now: f64,
        ending: Option<Battle>,
    ) -> Option<Line> {
        let end = self.stage.as_mut()?.end.take();
        if !self.speaking() {
            return None;
        }
        let lines: Vec<Line> = ending.into_iter().chain(end).map(Line::Battle).collect();
        if lines.is_empty() {
            return None;
        }
        self.say_found(audio, &lines, now + LEAD)
    }
}

/// The results' lines for `r`, in the order [`Announcer::results`] tries
/// them (the first it has a sound for speaks): a quit's own line alone;
/// a first clear; a new record after an earlier play; the full combo;
/// the grade; a first play's record. `prior`: what the history said
/// before the play (`None`: nothing to tell records from).
pub fn results_lines(
    prior: Option<Prior>,
    r: &Ending,
    grades: &GradeTable,
    combo_w4: bool,
) -> Vec<Line> {
    if r.quit {
        return vec![Line::Quit];
    }
    let mut lines = Vec::new();
    if let Some(p) = prior {
        if !r.failed && !p.cleared {
            lines.push(Line::FirstClear);
        }
        if p.played && r.score > p.best {
            lines.push(Line::NewRecord);
        }
    }
    if !r.failed {
        let w = match r.full_combo {
            FullCombo::MarvelousFC => Some(1),
            FullCombo::PerfectFC => Some(2),
            FullCombo::GreatFC => Some(3),
            FullCombo::FC if combo_w4 => Some(4),
            _ => None,
        };
        lines.extend(w.map(Line::EvaluationFullCombo));
    }
    lines.extend(old_grade(grades, r.grade, r.failed).map(Line::Grade));
    if prior.is_some_and(|p| !p.played) {
        lines.push(Line::NewRecord);
    }
    lines
}

/// Whether `evaluation cheer` follows the results' line of `r`: AA or
/// better (never after a quit).
pub fn cheers(r: &Ending, grades: &GradeTable) -> bool {
    !r.quit
        && matches!(
            old_grade(grades, r.grade, r.failed),
            Some(Grade::Aaaa | Grade::Aaa | Grade::Aa)
        )
}

/// A play's grade as one of StepMania's old seven (and failed `e`), for
/// its `evaluation <grade>` line: a table whose grades are those letters
/// (with a `+` or `-`: DDR's) by letter; any other by its place among the
/// table's grades, rescaled onto seven as 5.1's `GradeToOldGrade` does
/// (`lround(i × 6 / (n − 1))`; ITG's seventeen, Dancing☆Onigiri's
/// eleven). `None` for a table with no grades to tell apart (FFR's).
/// DDI's eight go down the seven from the top
/// (`docs/plans/ddi-ruleset.md`, "Normal play"): Absolute Dito (and
/// Absolute Bot) the best tier's lines, SSS the next, and so on down, its
/// C and D both the last.
pub fn old_grade(grades: &GradeTable, grade: &str, failed: bool) -> Option<Grade> {
    const OLD: [Grade; 7] = [
        Grade::Aaaa,
        Grade::Aaa,
        Grade::Aa,
        Grade::A,
        Grade::B,
        Grade::C,
        Grade::D,
    ];
    if failed {
        return Some(Grade::E);
    }
    if grades.rule == Some(ddi_engine::rules::GradeRule::Ddi) {
        use ddi_engine::rules::ddi;
        if grade == ddi::ABSOLUTE_DITO || grade == ddi::ABSOLUTE_BOT {
            return Some(Grade::Aaaa);
        }
        let i = ddi::GRADES.iter().position(|(name, _)| *name == grade)?;
        return OLD.get((i + 1).min(6)).copied();
    }
    // The letters in capitals, DDR's with a `+` or `-`.
    let letter = |name: &str| -> Option<Grade> {
        let name = name.trim_end_matches(['+', '-']);
        OLD.into_iter()
            .find(|g| g.name().to_ascii_uppercase() == name)
    };
    let names = grade_order(grades);
    if names.len() < 2 || !names.contains(&grade) {
        return None;
    }
    if names.iter().all(|n| letter(n).is_some()) {
        return letter(grade);
    }
    let i = names.iter().position(|n| *n == grade)?;
    let old = (i as f64 * 6.0 / (names.len() - 1) as f64).round() as usize;
    OLD.get(old.min(6)).copied()
}

/// `table`'s grade names from best to worst, each once.
pub(crate) fn grade_order(table: &GradeTable) -> Vec<&str> {
    let mut names: Vec<&str> = Vec::new();
    for n in table
        .all_w1
        .iter()
        .chain(table.all_w2.iter())
        .map(String::as_str)
        .chain(table.tiers.iter().map(|(n, _)| n.as_str()))
    {
        if !n.is_empty() && !names.contains(&n) {
            names.push(n);
        }
    }
    names
}

#[cfg(test)]
mod tests;
