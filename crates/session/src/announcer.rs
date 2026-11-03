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
use ddi_library::announcer::{Grade, Kept, Line, Spoken};
use ddi_library::path::RelPathBuf;
use ddi_platform::AudioBackend;

/// Seconds between the periodic comments (`SecondsBetweenComments`).
pub const COMMENT_GAP: f64 = 10.0;
/// Seconds a combo line waits after the last gated line (5.1's 2 s).
pub const COMBO_GAP: f64 = 2.0;
/// A combo above this broken says `combo stopped` (`ComboStoppedAt`).
pub const COMBO_STOPPED_AT: u32 = 50;
/// `evaluation cheer` this long after the results' line
/// (`CheerDelaySeconds`).
pub const CHEER_DELAY: f64 = 2.5;
/// `here we go` ends this long before the first note.
pub const GO_BEFORE: f64 = 0.5;
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
            seed: seed | 1,
        }
    }

    /// xorshift64*.
    fn next(&mut self) -> u64 {
        self.seed ^= self.seed >> 12;
        self.seed ^= self.seed << 25;
        self.seed ^= self.seed >> 27;
        self.seed.wrapping_mul(0x2545_F491_4F6C_DD1D)
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
                for i in (1..bag.len()).rev() {
                    let j = (self.next() % (i as u64 + 1)) as usize;
                    bag.swap(i, j);
                }
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
/// [`variants`] of each, the next of each line's bag.
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

/// [`variants`] of each of `kept`'s lines `which` keeps, from their bags.
fn take(
    kept: &Kept,
    bags: &mut Bags,
    which: impl Fn(Line) -> bool,
) -> Vec<(Line, Vec<RelPathBuf>)> {
    let key = kept.key();
    kept.lines
        .iter()
        .filter_map(|(folder, files)| {
            let line = Line::parse(folder)?;
            if !which(line) || files.is_empty() {
                return None;
            }
            let picks = bags.take(&key, line, files.len(), variants(line));
            Some((line, picks.into_iter().map(|i| files[i].clone()).collect()))
        })
        .collect()
}

/// The announcer's sounds as decoded, by line.
#[derive(Debug)]
pub struct Voice<B> {
    lines: BTreeMap<Line, Vec<B>>,
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
        self.lines.entry(line).or_default().push(buffer);
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

    fn expects(&self, line: Line) -> bool {
        line.chain().iter().any(|l| self.expected.contains(l))
    }

    /// The sound `line` (or the first of its chain decoded) speaks next.
    pub fn peek(&self, line: Line) -> Option<(Line, &B)> {
        let found = self.find(line)?;
        let sounds = &self.lines[&found];
        let i = self.cursor.get(&found).copied().unwrap_or(0) % sounds.len();
        Some((found, &sounds[i]))
    }

    /// [`Voice::peek`], the line's next sound then the one after.
    pub fn next(&mut self, line: Line) -> Option<(Line, &B)> {
        let found = self.find(line)?;
        let i = self.cursor.entry(found).or_insert(0);
        let at = *i;
        *i += 1;
        let sounds = &self.lines[&found];
        Some((found, &sounds[at % sounds.len()]))
    }
}

/// The voice as the shell decodes it and the session speaks it.
pub type SharedVoice<B> = Rc<RefCell<Voice<B>>>;

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
    pub fn of(best: Option<&ddi_ui::state::history::Best>) -> Prior {
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
    /// Gated lines that came too soon after another (5.1 drops them).
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
    fn at(&self, t: f64) -> f64 {
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
            opening: Line::Ready,
            opened: false,
            go_after: None,
            since: 0.0,
            combo: 0,
            lives: None,
            ended: false,
            report: Report::default(),
        }
    }

    /// A quick restart's play (decision 79): `gameplay restart` opens
    /// it, in place of ready.
    pub fn restarted(mut self) -> Announcer<B> {
        self.opening = Line::Restart;
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
        let Some(voice) = self.voice.clone() else {
            return false;
        };
        let mut voice = voice.borrow_mut();
        for &line in lines {
            let Some((found, sound)) = voice.next(line) else {
                continue;
            };
            if audio.play_voice(sound, at, self.volume).is_ok() {
                self.report.spoken.push((found.folder(), at));
                return true;
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
        false
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
        let Some(first) = self.first_note else {
            return;
        };
        let start = now + LEAD;
        let by = self.timing.at(first) - GO_BEFORE;
        let ready = self.opening;
        match (
            self.length(audio, ready),
            self.length(audio, Line::HereWeGo),
        ) {
            (Some(r), Some(g)) if start + r <= by - g => {
                self.say(audio, &[ready], start);
                self.say(audio, &[Line::HereWeGo], by - g);
            }
            (_, Some(g)) if by - g >= start => {
                self.say(audio, &[Line::HereWeGo], by - g);
            }
            // No room for here we go: nothing.
            (_, Some(_)) => {}
            (Some(r), None) if start + r <= by => {
                self.say(audio, &[ready], start);
                // Here we go may be decoded yet, and fit after it.
                self.go_after = Some(start + r);
            }
            (Some(_), None) => self.go_after = Some(start),
            (None, None) => {
                // Ready due and not there (reported); here we go may come.
                self.say(audio, &[ready], start);
                self.go_after = Some(start);
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
            // The combo crossing a hundred, one line a frame; a combo of
            // more than 50 broken.
            let (old, new) = (self.combo, m.combo);
            if lost {
            } else if new / 100 > old / 100 && new >= 100 {
                let n = new / 100 * 100;
                self.gated(audio, &[Line::Combo(n)], COMBO_GAP, m.now);
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
                let lines = if m.lives.is_some() {
                    vec![Line::CommentOni, life]
                } else {
                    vec![life]
                };
                self.gated(audio, &lines, COMMENT_GAP, m.now);
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
        if r.quit {
            self.say(audio, &[Line::Quit], at);
            return;
        }
        let grade = old_grade(grades, r.grade, r.failed);
        let mut lines = Vec::new();
        let first_play = self.prior.is_some_and(|p| !p.played);
        if let Some(p) = self.prior {
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
        lines.extend(grade.map(Line::Grade));
        if first_play {
            lines.push(Line::NewRecord);
        }
        if lines.is_empty() {
            return;
        }
        let spoke = self.say(audio, &lines, at);
        if spoke && matches!(grade, Some(Grade::Aaaa | Grade::Aaa | Grade::Aa)) {
            self.say(audio, &[Line::Cheer], at + CHEER_DELAY);
        }
    }
}

/// A play's grade as one of StepMania's old seven (and failed `e`), for
/// its `evaluation <grade>` line: a table whose grades are those letters
/// (with a `+` or `-`: DDR's) by letter; any other by its place among the
/// table's grades, rescaled onto seven as 5.1's `GradeToOldGrade` does
/// (`lround(i × 6 / (n − 1))`; ITG's seventeen, Dancing☆Onigiri's
/// eleven). `None` for a table with no grades to tell apart (FFR's).
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
    let letter = |name: &str| -> Option<Grade> {
        let at = ["AAAA", "AAA", "AA", "A", "B", "C", "D"]
            .iter()
            .position(|l| *l == name.trim_end_matches(['+', '-']))?;
        Some(OLD[at])
    };
    let mut names: Vec<&str> = Vec::new();
    for n in grades
        .all_w1
        .iter()
        .chain(grades.all_w2.iter())
        .map(String::as_str)
        .chain(grades.tiers.iter().map(|(n, _)| n.as_str()))
    {
        if !n.is_empty() && !names.contains(&n) {
            names.push(n);
        }
    }
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

#[cfg(test)]
mod tests;
