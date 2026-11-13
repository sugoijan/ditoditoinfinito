//! An opponent's miniature field as the play shows it
//! (`docs/plans/multiplayer-ui.md`, "Faithful miniature fields";
//! `docs/plans/multiplayer-prototype-progress.md`, M6): the notes they
//! play scrolling to their receptors on this play's song clock, and what
//! a stream of their judged moments says of them, drawn a presentation
//! delay after each moment.
//!
//! The view is fed by moments ([`Moment`]: when, which note, the
//! judgment, the life and combo after it and whether it put them out), never by
//! the player behind them, so that a remote player's stream can feed the
//! same view later. A local CPU's moments come from its own judged play
//! as it is advanced ([`crate::battle::Battle`]), with no delay
//! ([`LOCAL_DELAY`]); a network's come late, and its delay hides that:
//! the whole field is drawn `delay` seconds behind the song (its notes
//! scrolling, its moments, its shocks), so that a moment shows when its
//! note reaches the receptors. A live stage's delay changes as its device
//! measures the network (`ddi_room::live::delay`, [`FieldFeed::set_delay`]).
//!
//! The notes are those of the chart's tap-only copy the CPUs play
//! (`ddi_engine::cpu::project`: holds and rolls as taps, mines, lifts
//! and fakes left out), never the chart itself ([`TapTrack`]). A note
//! leaves the field when a moment says it was hit; a missed one scrolls
//! on past the receptors, as a play shows it. Nothing is inferred from
//! a note reaching the receptors.
//!
//! A battle's shocks put into their play (M8b) come with the attack's
//! commitment ([`FieldFeed::add_hazards`]) and are drawn as the skin's
//! mines (`SpriteKind::Mine`, numbered from
//! `ddi_engine::battle::HAZARD_INDEX_BASE`); a moment saying one was
//! stepped on ([`MomentKind::ShockHit`]) takes it off the field and
//! flashes its receptor ([`ReceptorState::mine`]); one avoided scrolls on
//! past the receptors. Where the stage's rules lock a lane a shock is
//! stepped on in (DDI's, `JudgeTable::lock`), the same moment says the
//! lane is locked for the lock's time ([`FieldFeed::locks`]), which the
//! miniature shows.

use ddi_chart::{Quantization, Song, Tick, TimingMap};
use ddi_engine::battle::{HAZARD_INDEX_BASE, Hazard};
use ddi_engine::frame::{
    Frame, HoldJudgements, HoldKind, NoteSprite, ReceptorState, SpriteKind,
};
use ddi_engine::frame::JudgementFlash;
use ddi_engine::rules::{Judgement, ScoreView};
use ddi_engine::scroll::{ScrollOptions, ScrollState};

/// A local opponent's presentation delay: none (its moments are judged
/// on this device, in step with the song).
pub const LOCAL_DELAY: f64 = 0.0;

/// How long a receptor shows a judgment after it (seconds): the hit
/// effect's time in a skin that draws its own, the flash's otherwise.
pub const FLASH_SECONDS: f32 = 0.4;

/// How long a field shown large shows a judgment's word after it,
/// seconds (a play's own: `ddi_engine::player`'s flash).
pub const WORD_SECONDS: f32 = 1.0;

/// What a moment judged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MomentKind {
    /// A note of the copy, and its judgment.
    Tap(Judgement),
    /// A battle shock stepped on (M8b).
    ShockHit,
    /// A hold's (or roll's) end, its head's note: held to it, or let go
    /// (a person's stream, `crate::battle::live`).
    HoldEnd(bool),
    /// A mine of the chart stepped on (a person's stream).
    MineHit,
    /// A stray press that counted (DDI's Boo): the large field says
    /// "Boo", nothing else shows it (as a play's own field).
    Boo,
    /// Nothing a note shows (their going out): their numbers after it
    /// alone.
    Mark,
    /// Their life ran out and they stepped down to this rung of the
    /// stage's ladder (a match's "At zero life: Step down",
    /// `ddi_engine::ladder`): from the moment the notes of the chart they
    /// were on are gone, from the boundary (the moment's tick) the
    /// rung's are theirs ([`FieldFeed::step_down`]); the life after it is
    /// what was given back.
    StepDown(u8),
}

/// One judged moment of a player's run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Moment {
    /// When it was judged: judged song seconds.
    pub at: f64,
    /// Which note of the copy (or which shock) it judged: its tick and
    /// its lane.
    pub tick: Tick,
    pub lane: u8,
    pub kind: MomentKind,
    /// The player's life after it, `0..=1`, their combo after it, and
    /// whether it put them out of the stage.
    pub life: f32,
    pub combo: u32,
    pub out: bool,
    /// What their field shown large says of it (a watched stage's
    /// focus, [`FieldFeed::frame_large`]): how early or late the press
    /// was (press − note, seconds: the judgment's early or late word; 0
    /// for a miss and a shock), whether a top hit was inside the
    /// ruleset's inner window (`JudgeTable::is_w0`; none where it has
    /// none), and their score after it as the room shows it (a CPU's on
    /// the chart's own scale).
    pub offset: f32,
    pub w0: Option<bool>,
    pub score: ScoreView,
}

/// What a note of the track is drawn as.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Shape {
    Tap,
    Lift,
    Mine,
    /// A hold or a roll to its end (song seconds, displayed beat).
    Hold {
        roll: bool,
        end_seconds: f64,
        end_displayed: f64,
    },
}

#[derive(Clone, Copy, Debug)]
struct TrackNote {
    seconds: f64,
    displayed: f64,
    tick: Tick,
    lane: u8,
    quantization: Quantization,
    shape: Shape,
}

/// The chart's tap-only copy, as the miniatures draw it: its notes in
/// time order and its timing. One for all the opponents of a stage.
#[derive(Clone, Debug)]
pub struct TapTrack {
    timing: TimingMap,
    notes: Vec<TrackNote>,
    lanes: u8,
}

impl TapTrack {
    /// The copy of chart `chart` of `song` that the CPUs play; refused
    /// where they cannot play it.
    pub fn new(song: &Song, chart: usize) -> Result<TapTrack, String> {
        let copy = ddi_engine::cpu::project(song, chart)?;
        Ok(Self::of(&copy, chart, |_| Some(Shape::Tap)))
    }

    /// Chart `chart` of `song` itself, as a person plays it (an online
    /// room's stage played live, `crate::battle::live`): its taps, holds
    /// and rolls to their ends, lifts and mines, as a play's own field
    /// draws them; fakes left out.
    pub fn full(song: &Song, chart: usize) -> Result<TapTrack, String> {
        let Some(c) = song.charts.get(chart) else {
            return Err(format!("no chart {chart}"));
        };
        let timing = c.timing(song);
        Ok(Self::of(song, chart, |k| match *k {
            ddi_chart::NoteKind::Tap => Some(Shape::Tap),
            ddi_chart::NoteKind::Lift => Some(Shape::Lift),
            ddi_chart::NoteKind::Mine => Some(Shape::Mine),
            ddi_chart::NoteKind::HoldHead { end } | ddi_chart::NoteKind::RollHead { end } => {
                Some(Shape::Hold {
                    roll: matches!(k, ddi_chart::NoteKind::RollHead { .. }),
                    end_seconds: timing.seconds_at(end),
                    end_displayed: timing.displayed_beat(end.beat()),
                })
            }
            _ => None,
        }))
    }

    fn of(
        song: &Song,
        chart: usize,
        shape: impl Fn(&ddi_chart::NoteKind) -> Option<Shape>,
    ) -> TapTrack {
        let c = &song.charts[chart];
        let timing = c.timing(song).clone();
        let lanes = song.layout_of(c).map_or(4, |l| l.lane_count()) as u8;
        let mut notes: Vec<TrackNote> = c
            .notes
            .iter()
            .filter(|n| timing.judgeable(n.tick))
            .filter_map(|n| {
                Some(TrackNote {
                    seconds: timing.seconds_at(n.tick),
                    displayed: timing.displayed_beat(n.tick.beat()),
                    tick: n.tick,
                    lane: n.lane,
                    quantization: n.tick.quantization(),
                    shape: shape(&n.kind)?,
                })
            })
            .collect();
        notes.sort_by(|a, b| a.seconds.total_cmp(&b.seconds).then(a.lane.cmp(&b.lane)));
        TapTrack {
            timing,
            notes,
            lanes,
        }
    }

    pub fn lanes(&self) -> u8 {
        self.lanes
    }

    /// The notes, in time order.
    pub fn len(&self) -> usize {
        self.notes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    /// The song second of `tick` on the chart's timing.
    pub fn seconds_at(&self, tick: Tick) -> f64 {
        self.timing.seconds_at(tick)
    }

    /// The note a moment judged.
    fn find(&self, tick: Tick, lane: u8) -> Option<usize> {
        let seconds = self.timing.seconds_at(tick);
        let from = self.notes.partition_point(|n| n.seconds < seconds - 1e-6);
        self.notes[from..]
            .iter()
            .take_while(|n| n.seconds <= seconds + 1e-6)
            .position(|n| n.tick == tick && n.lane == lane)
            .map(|i| from + i)
    }
}

/// What a field shows of its player at a moment: their life, their
/// combo and whether they are out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shown {
    pub life: f32,
    pub combo: u32,
    pub out: bool,
}

/// A battle shock in a player's field ([`FieldFeed::add_hazards`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FeedShock {
    pub tick: Tick,
    pub lane: u8,
    pub seconds: f64,
    displayed: f64,
    /// Drawn from this song second of the field's own time on (its
    /// attack's commitment).
    pub shown_from: f64,
    /// When it leaves the field: stepped on (its moment), field time.
    pub gone: f64,
}

/// One player's stream of moments and what it says of their field.
#[derive(Clone, Debug)]
pub struct FieldFeed {
    moments: Vec<Moment>,
    /// For each note of the track: when it leaves the field (its hit's
    /// moment, the field's own time), if it was hit; a hold's: when its
    /// head was hit (it is held from then).
    gone: Vec<f64>,
    /// For each hold of the track: when it ended and whether it was held
    /// to its end (else let go: dropped).
    hold_end: Vec<Option<(f64, bool)>>,
    /// The battle's shocks put into their play, in time order.
    shocks: Vec<FeedShock>,
    delay: f64,
    /// Life before any moment.
    life: f32,
    /// How long a lane a shock was stepped on in stays locked, seconds,
    /// where the stage's rules lock it.
    lock: Option<f64>,
    /// The earliest moment that put them out, kept as it comes so that
    /// what every frame asks of a field not drawn (its summary: life,
    /// combo, out, score, [`FieldFeed::shown`]) costs the same however
    /// long the stream (a large field's, `docs/plans/multiplayer-prototype-progress.md`,
    /// "Rooms of 1+72 players").
    out_at: f64,
    /// Their steps down to easier charts, in order: what the field draws
    /// from each one on ([`FieldFeed::step_down`]).
    steps: Vec<FeedStep>,
}

/// A step down in a field: the chart they play from its boundary, and
/// what of its notes the moments said.
#[derive(Clone, Debug)]
struct FeedStep {
    at: f64,
    boundary: f64,
    rung: u8,
    track: TapTrack,
    gone: Vec<f64>,
    hold_end: Vec<Option<(f64, bool)>>,
}

/// Each note index a step's chart draws with is past this many times its
/// place (the stage's chart's are its own).
const STEP_INDEX: usize = 1 << 24;

impl FieldFeed {
    /// A field of `track` starting at `life`, drawn `delay` seconds
    /// behind the song.
    pub fn new(track: &TapTrack, life: f32, delay: f64) -> FieldFeed {
        FieldFeed {
            moments: Vec::new(),
            gone: vec![f64::INFINITY; track.len()],
            hold_end: vec![None; track.len()],
            shocks: Vec::new(),
            delay: delay.max(0.0),
            life,
            lock: None,
            out_at: f64::INFINITY,
            steps: Vec::new(),
        }
    }

    /// They stepped down at song second `at` to rung `rung` (from 1), whose
    /// chart, `track`, they play from `boundary` on (a match's "At zero
    /// life: Step down"): the field draws the chart they were on to `at`,
    /// then the rung's from the boundary, the breather between empty. A
    /// step already known is not taken again.
    pub fn step_down(&mut self, track: TapTrack, at: f64, boundary: f64, rung: u8) {
        if self.steps.iter().any(|s| s.rung == rung) {
            return;
        }
        let n = track.len();
        let i = self.steps.partition_point(|s| s.at <= at);
        self.steps.insert(
            i,
            FeedStep {
                at,
                boundary,
                rung,
                track,
                gone: vec![f64::INFINITY; n],
                hold_end: vec![None; n],
            },
        );
    }

    /// The rung they play as shown at song second `t`: 0 the stage's
    /// chart, else the last step down shown.
    pub fn rung_at(&self, t: f64) -> u8 {
        let v = self.view_time(t);
        self.steps
            .iter()
            .rev()
            .find(|s| s.at <= v)
            .map_or(0, |s| s.rung)
    }

    /// The rungs they stepped down to, with when (judged song seconds).
    pub fn steps(&self) -> impl Iterator<Item = (f64, f64, u8)> + '_ {
        self.steps.iter().map(|s| (s.at, s.boundary, s.rung))
    }

    /// The chart a moment judged at `at` is of: its step's (the last
    /// before it), none for the stage's chart.
    fn step_of(&self, at: f64) -> Option<usize> {
        let k = self.steps.partition_point(|s| s.at < at);
        k.checked_sub(1)
    }

    /// The stage's rules lock a lane a shock is stepped on in for
    /// `seconds` (DDI's, `JudgeTable::lock`); `None`, they lock nothing.
    pub fn set_lock(&mut self, seconds: Option<f64>) {
        self.lock = seconds.filter(|s| *s > 0.0);
    }

    /// Each of `lanes` lanes' lock as shown at song second `t`: how far
    /// into it, `0..1`, where a shock stepped on there
    /// ([`MomentKind::ShockHit`]) locks it still; none where the rules
    /// lock nothing. (A locked lane steps on nothing, so a lane's lock is
    /// its last such moment's.)
    pub fn locks(&self, lanes: u8, t: f64) -> Vec<Option<f32>> {
        let mut out = vec![None; usize::from(lanes)];
        let Some(lock) = self.lock else {
            return out;
        };
        let v = self.view_time(t);
        for m in self.shown_moments(t).iter().rev() {
            let since = v - m.at;
            if since >= lock {
                break;
            }
            if m.kind == MomentKind::ShockHit
                && let Some(l) = out.get_mut(usize::from(m.lane))
                && l.is_none()
            {
                *l = Some((since / lock).clamp(0.0, 1.0) as f32);
            }
        }
        out
    }

    pub fn delay(&self) -> f64 {
        self.delay
    }

    /// Draw it `delay` seconds behind the song from now (a live stage's
    /// view delay, as its device measures the network).
    pub fn set_delay(&mut self, delay: f64) {
        self.delay = delay.max(0.0);
    }

    /// The field's own time at song second `t`: what it shows then.
    pub fn view_time(&self, t: f64) -> f64 {
        t - self.delay
    }

    /// The moments received, in the order they happened.
    pub fn moments(&self) -> &[Moment] {
        &self.moments
    }

    /// The battle's shocks put into their play, in time order.
    pub fn shocks(&self) -> &[FeedShock] {
        &self.shocks
    }

    /// An attack's shocks `cells` put into their play at song second `at`
    /// (its commitment): drawn from then on the field's own time (the
    /// delay after it on the song's), on `track`'s timing, as they scroll
    /// in.
    pub fn add_hazards(&mut self, track: &TapTrack, cells: &[Hazard], at: f64) {
        for c in cells {
            let seconds = track.timing.seconds_at(c.tick);
            let shock = FeedShock {
                tick: c.tick,
                lane: c.lane,
                seconds,
                displayed: track.timing.displayed_beat(c.tick.beat()),
                shown_from: at,
                gone: f64::INFINITY,
            };
            let i = self.shocks.partition_point(|s| s.seconds <= seconds);
            self.shocks.insert(i, shock);
        }
    }

    /// Shocks taken back off the field (an attack its recipient refused:
    /// it reached their play too late, `crate::battle::live`).
    pub fn remove_hazards(&mut self, cells: &[Hazard]) {
        self.shocks
            .retain(|s| !cells.iter().any(|c| c.tick == s.tick && c.lane == s.lane));
    }

    /// Takes in a moment of the stream (they come in the order they
    /// happened; one out of order is put in its place).
    pub fn push(&mut self, track: &TapTrack, m: Moment) {
        let step = self.step_of(m.at);
        let (track, gone, hold_end) = match step {
            Some(k) => {
                let s = &mut self.steps[k];
                (&s.track, &mut s.gone, &mut s.hold_end)
            }
            None => (track, &mut self.gone, &mut self.hold_end),
        };
        match m.kind {
            MomentKind::Tap(j) if !j.is_miss() => {
                if let Some(i) = track.find(m.tick, m.lane) {
                    gone[i] = gone[i].min(m.at);
                }
            }
            MomentKind::HoldEnd(ok) => {
                if let Some(i) = track.find(m.tick, m.lane)
                    && hold_end[i].is_none_or(|(at, _)| m.at < at)
                {
                    hold_end[i] = Some((m.at, ok));
                }
            }
            MomentKind::MineHit => {
                if let Some(i) = track.find(m.tick, m.lane) {
                    gone[i] = gone[i].min(m.at);
                }
            }
            MomentKind::Tap(_) | MomentKind::Boo | MomentKind::Mark | MomentKind::StepDown(_) => {}
            MomentKind::ShockHit => {
                if let Some(s) = self
                    .shocks
                    .iter_mut()
                    .find(|s| s.tick == m.tick && s.lane == m.lane)
                {
                    s.gone = s.gone.min(m.at);
                }
            }
        }
        if m.out {
            self.out_at = self.out_at.min(m.at);
        }
        let at = self.moments.partition_point(|x| x.at <= m.at);
        self.moments.insert(at, m);
    }

    /// The moments shown at song second `t`.
    fn shown_moments(&self, t: f64) -> &[Moment] {
        let v = self.view_time(t);
        let n = self.moments.partition_point(|m| m.at <= v);
        &self.moments[..n]
    }

    /// The score after the last moment shown at song second `t`.
    pub fn shown_score(&self, t: f64) -> Option<ScoreView> {
        self.shown_moments(t).last().map(|m| m.score)
    }

    /// Life and out as shown at song second `t`: the field's summary, a
    /// search and a comparison whatever the stream's length.
    pub fn shown(&self, t: f64) -> Shown {
        let last = self.shown_moments(t).last();
        Shown {
            life: last.map_or(self.life, |m| m.life),
            combo: last.map_or(0, |m| m.combo),
            out: self.out_at <= self.view_time(t),
        }
    }

    /// The field at song second `t` for a frame drawn with `scroll` (the
    /// local player's own speed and direction), `range` arrow heights
    /// of it visible past the receptors: the copy's notes not yet gone,
    /// faded in at the far end and out a little past the receptors (a
    /// miniature has no edge to hide them behind), each lane's last
    /// judgment shown within [`FLASH_SECONDS`]. Out of the stage, no
    /// notes, as a play's field goes blank. All of it the delay behind
    /// the song (its frame's time is the field's own).
    pub fn frame(&self, track: &TapTrack, scroll: ScrollOptions, t: f64, range: f32) -> Frame {
        let v = self.view_time(t);
        let state = ScrollState::at(scroll, &track.timing, v);
        let shown = self.shown_moments(t);
        let out = shown.iter().any(|m| m.out);
        let mut receptors = vec![
            ReceptorState {
                pressed: false,
                mine: None,
                flash: None,
                holding: None,
            };
            usize::from(track.lanes)
        ];
        for m in shown.iter().rev() {
            let age = (v - m.at) as f32;
            if age > FLASH_SECONDS {
                break;
            }
            let Some(r) = receptors.get_mut(usize::from(m.lane)) else {
                continue;
            };
            match m.kind {
                MomentKind::Tap(j) if r.flash.is_none() => r.flash = Some((j, age.max(0.0))),
                MomentKind::ShockHit | MomentKind::MineHit if r.mine.is_none() => {
                    r.mine = Some(age.max(0.0))
                }
                _ => {}
            }
        }
        let mut notes = Vec::new();
        if !out {
            // From a second back (a missed note still in sight) to the
            // first one past the far end.
            // From a second back (a missed note still in sight; a hold
            // still being held, however long) to the first one past the
            // far end.
            // The stage's chart to their first step down shown, each
            // step's chart from its boundary to the next step shown.
            let shown_steps: Vec<&FeedStep> = self.steps.iter().filter(|s| s.at <= v).collect();
            let first_cut = shown_steps.first().map_or(f64::INFINITY, |s| s.at);
            self.draw_notes(
                (track, &self.gone, &self.hold_end),
                (f64::NEG_INFINITY, first_cut, 0),
                (v, &state, range),
                &mut notes,
                &mut receptors,
            );
            for (k, step) in shown_steps.iter().enumerate() {
                let cut = shown_steps.get(k + 1).map_or(f64::INFINITY, |s| s.at);
                self.draw_notes(
                    (&step.track, &step.gone, &step.hold_end),
                    (step.boundary - 1e-6, cut, STEP_INDEX * usize::from(step.rung)),
                    (v, &state, range),
                    &mut notes,
                    &mut receptors,
                );
            }
            // The battle's shocks: from their commitment, until stepped
            // on; avoided ones scroll on past the receptors.
            for (i, s) in self.shocks.iter().enumerate() {
                if s.shown_from > v || s.gone <= v {
                    continue;
                }
                let y = state.y_displayed(s.displayed, s.seconds);
                if y > range || y < -0.6 {
                    continue;
                }
                let alpha = ((range - y) / 0.8).min((y + 0.6) / 0.6).clamp(0.0, 1.0);
                let beat = s.tick.beat();
                notes.push(NoteSprite {
                    note_index: HAZARD_INDEX_BASE + i,
                    lane: s.lane,
                    y,
                    kind: SpriteKind::Mine,
                    quantization: s.tick.quantization(),
                    beat_frac: (beat - beat.floor()) as f32,
                    color: None,
                    alpha,
                    glow: 0.0,
                    beat: beat as f32,
                    hold_life: 1.0,
                });
            }
        }
        let life = shown.last().map_or(self.life, |m| m.life);
        let combo = shown.last().map_or(0, |m| m.combo);
        Frame {
            song_time: v,
            beat: state.beat(),
            note_motion: state.note_motion(&track.timing),
            roll_window_seconds: 0.0,
            lanes: track.lanes,
            receptors,
            notes,
            judgement: None,
            holds: HoldJudgements::default(),
            combo,
            score: Default::default(),
            life,
            danger: false,
            failed: out,
            finished: false,
            visible_range: range,
            appearance: Default::default(),
            cover: Default::default(),
        }
    }

    /// One chart's notes of the field ([`FieldFeed::frame`]): those whose
    /// second is in `lo..hi`, numbered from `base`.
    #[allow(clippy::type_complexity)]
    fn draw_notes(
        &self,
        (track, gone, hold_end): (&TapTrack, &[f64], &[Option<(f64, bool)>]),
        (lo, hi, base): (f64, f64, usize),
        (v, state, range): (f64, &ScrollState, f32),
        notes: &mut Vec<NoteSprite>,
        receptors: &mut [ReceptorState],
    ) {
        let from = track.notes.partition_point(|n| n.seconds < v - 1.0);
        let holds = track.notes[..from].iter().enumerate().filter(|(i, n)| {
            matches!(n.shape, Shape::Hold { end_seconds, .. } if end_seconds >= v - 1.0)
                && hold_end[*i].is_none_or(|(at, ok)| !ok || at > v)
        });
        let rest = track.notes.iter().enumerate().skip(from);
        for (i, n) in holds.chain(rest) {
            if n.seconds < lo || n.seconds >= hi {
                if n.seconds >= hi {
                    break;
                }
                continue;
            }
            let mut y = state.y_displayed(n.displayed, n.seconds);
            if y > range && n.seconds > v {
                break;
            }
            let hit = gone[i] <= v;
            let (kind, bottom, life) = match n.shape {
                Shape::Tap | Shape::Lift | Shape::Mine if hit => continue,
                Shape::Tap => (SpriteKind::Tap, y, 1.0),
                Shape::Lift => (SpriteKind::Lift, y, 1.0),
                Shape::Mine => (SpriteKind::Mine, y, 1.0),
                Shape::Hold {
                    roll,
                    end_seconds,
                    end_displayed,
                } => {
                    let ended = hold_end[i].filter(|(at, _)| *at <= v);
                    if ended.is_some_and(|(_, ok)| ok) {
                        // Held to its end: gone.
                        continue;
                    }
                    let dropped = ended.is_some();
                    let active = hit && !dropped && v < end_seconds;
                    if active {
                        // Held: its head pinned to the receptor.
                        y = 0.0;
                    }
                    let tail_y = state.y_displayed(end_displayed, end_seconds);
                    // Its body drawn within the field (a miniature's
                    // panel ends there): no further than its far end.
                    let drawn_tail = tail_y.min(range - 0.3);
                    let head = if roll {
                        SpriteKind::RollHead {
                            tail_y: drawn_tail,
                            active,
                            dropped,
                        }
                    } else {
                        SpriteKind::HoldHead {
                            tail_y: drawn_tail,
                            active,
                            dropped,
                        }
                    };
                    (head, tail_y, if dropped { 0.0 } else { 1.0 })
                }
            };
            if y > range || bottom.max(y) < -0.6 {
                continue;
            }
            let alpha = ((range - y) / 0.8)
                .min((bottom.max(y) + 0.6) / 0.6)
                .clamp(0.0, 1.0);
            // A hold whose head has scrolled past the field's near end
            // (let go) while its body is still in sight: drawn from
            // there, never past the field's edge.
            if matches!(kind, SpriteKind::HoldHead { .. } | SpriteKind::RollHead { .. }) {
                y = y.max(-0.6);
            }
            if let SpriteKind::HoldHead { active: true, .. } = kind
                && let Some(r) = receptors.get_mut(usize::from(n.lane))
            {
                r.holding = Some(HoldKind::Hold);
            }
            if let SpriteKind::RollHead { active: true, .. } = kind
                && let Some(r) = receptors.get_mut(usize::from(n.lane))
            {
                r.holding = Some(HoldKind::Roll);
            }
            let beat = n.tick.beat();
            notes.push(NoteSprite {
                note_index: base + i,
                lane: n.lane,
                y,
                kind,
                quantization: n.quantization,
                beat_frac: (beat - beat.floor()) as f32,
                color: None,
                alpha,
                glow: 0.0,
                beat: beat as f32,
                hold_life: life,
            });
        }
    }

    /// [`FieldFeed::frame`] for a field shown large, as a play's own is
    /// (a watched stage's focus): with the judgment's word of the last
    /// note judged, its early or late and its combo, for
    /// [`WORD_SECONDS`], and the score after the last moment shown, all
    /// as the stream said them (nothing is judged here).
    pub fn frame_large(
        &self,
        track: &TapTrack,
        scroll: ScrollOptions,
        t: f64,
        range: f32,
    ) -> Frame {
        let mut frame = self.frame(track, scroll, t, range);
        let v = self.view_time(t);
        let shown = self.shown_moments(t);
        frame.score = shown.last().map(|m| m.score).unwrap_or_default();
        frame.judgement = shown
            .iter()
            .rev()
            .find_map(|m| match m.kind {
                MomentKind::Tap(judgement) => Some(JudgementFlash {
                    judgement,
                    delta_seconds: f64::from(m.offset),
                    age: (v - m.at).max(0.0) as f32,
                    combo_at: m.combo,
                    w0: m.w0,
                    boo: false,
                }),
                MomentKind::Boo => Some(JudgementFlash {
                    judgement: Judgement::Miss,
                    delta_seconds: 0.0,
                    age: (v - m.at).max(0.0) as f32,
                    combo_at: m.combo,
                    w0: None,
                    boo: true,
                }),
                MomentKind::ShockHit
                | MomentKind::HoldEnd(_)
                | MomentKind::MineHit
                | MomentKind::Mark
                | MomentKind::StepDown(_) => None,
            })
            .filter(|f| f.age < WORD_SECONDS);
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A field shown large says the judgment's word from the stream: the
    /// last note judged, how early or late, the inner window, the combo
    /// and the score after it, for the word's time; a miniature's frame
    /// says none of it.
    #[test]
    fn a_field_shown_large_says_the_judgment_the_stream_told() {
        let song = song();
        let track = TapTrack::new(&song, 1).unwrap();
        let mut feed = FieldFeed::new(&track, 1.0, LOCAL_DELAY);
        let scroll = ScrollOptions::default();
        let mut m = moment(&track, 0, Judgement::W1, -0.012);
        m.w0 = Some(true);
        m.combo = 7;
        m.score = ScoreView {
            percent: Some(0.5),
            ..ScoreView::default()
        };
        let at = m.at;
        feed.push(&track, m);
        let small = feed.frame(&track, scroll, at + 0.1, 8.5);
        assert_eq!(small.judgement, None);
        assert_eq!(small.score, ScoreView::default());
        let large = feed.frame_large(&track, scroll, at + 0.1, 8.5);
        let flash = large.judgement.expect("its word shows");
        assert_eq!(flash.judgement, Judgement::W1);
        assert!((flash.delta_seconds + 0.012).abs() < 1e-6, "early");
        assert_eq!((flash.w0, flash.combo_at, flash.boo), (Some(true), 7, false));
        assert!((flash.age - 0.1).abs() < 1e-3);
        assert_eq!(large.score.percent, Some(0.5));
        assert_eq!((large.combo, large.notes), (small.combo, small.notes));
        // Before the moment and after the word's time: no word.
        assert!(feed.frame_large(&track, scroll, at - 0.01, 8.5).judgement.is_none());
        let late = feed.frame_large(&track, scroll, at + f64::from(WORD_SECONDS) + 0.01, 8.5);
        assert!(late.judgement.is_none());
        assert_eq!(late.score.percent, Some(0.5), "the score stays");
    }

    /// A field not drawn is asked its summary alone (life, combo, out),
    /// which says what the whole stream says, at every moment, delayed or
    /// not, whatever order its moments came in.
    #[test]
    fn a_fields_summary_agrees_with_its_whole_stream() {
        let song = song();
        let track = TapTrack::new(&song, 1).unwrap();
        for delay in [LOCAL_DELAY, 0.75] {
            let mut feed = FieldFeed::new(&track, 1.0, delay);
            let n = track.len().min(200);
            // Out two thirds in; a few moments out of order.
            assert!(n > 12, "{n}");
            let mut order: Vec<usize> = (0..n).collect();
            order.swap(3, 4);
            order.swap(n - 4, n - 5);
            for i in order {
                let mut m = moment(&track, i, Judgement::W2, 0.01);
                m.combo = i as u32;
                m.life = 1.0 - i as f32 / n as f32;
                m.out = i >= n * 2 / 3;
                feed.push(&track, m);
            }
            for k in 0..400 {
                let t = k as f64 * 0.25;
                let shown = feed.shown(t);
                let whole: Vec<&Moment> = feed
                    .moments()
                    .iter()
                    .filter(|m| m.at + delay <= t)
                    .collect();
                assert_eq!(shown.out, whole.iter().any(|m| m.out), "{delay} {t}");
                assert_eq!(shown.combo, whole.last().map_or(0, |m| m.combo));
                assert_eq!(shown.life, whole.last().map_or(1.0, |m| m.life));
            }
        }
    }

    fn song() -> Song {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs/beatucada");
        let path = std::fs::read_dir(path)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e == "ssc"))
            .unwrap();
        ddi_library::scan::parse_simfile(&std::fs::read_to_string(path).unwrap(), "ssc").unwrap()
    }

    fn moment(track: &TapTrack, i: usize, j: Judgement, late: f64) -> Moment {
        let n = track.notes[i];
        Moment {
            at: n.seconds + late,
            tick: n.tick,
            lane: n.lane,
            kind: MomentKind::Tap(j),
            life: 0.5,
            combo: 3,
            out: false,
            offset: late as f32,
            w0: None,
            score: ScoreView::default(),
        }
    }

    #[test]
    fn the_copy_is_taps_only_and_every_note_is_found_again() {
        let song = song();
        let track = TapTrack::new(&song, 1).unwrap();
        assert!(!track.is_empty());
        assert_eq!(track.lanes(), 4);
        let taps = song.charts[1]
            .notes
            .iter()
            .filter(|n| {
                matches!(
                    n.kind,
                    ddi_chart::NoteKind::Tap
                        | ddi_chart::NoteKind::HoldHead { .. }
                        | ddi_chart::NoteKind::RollHead { .. }
                )
            })
            .count();
        assert_eq!(track.len(), taps);
        for (i, n) in track.notes.iter().enumerate() {
            assert_eq!(track.find(n.tick, n.lane), Some(i));
        }
        // A chart the CPUs cannot play has no copy.
        assert!(TapTrack::new(&song, 99).is_err());
    }

    #[test]
    fn a_hit_leaves_the_field_and_a_miss_scrolls_on() {
        let song = song();
        let track = TapTrack::new(&song, 1).unwrap();
        let mut feed = FieldFeed::new(&track, 1.0, LOCAL_DELAY);
        let scroll = ScrollOptions::default();
        let (a, b) = (0, 1);
        let t = track.notes[a].seconds;
        let before = feed.frame(&track, scroll, t, 6.0);
        assert!(before.notes.iter().any(|n| n.note_index == a));
        feed.push(&track, moment(&track, a, Judgement::W1, 0.0));
        feed.push(&track, moment(&track, b, Judgement::Miss, 0.0));
        let after = feed.frame(&track, scroll, track.notes[b].seconds + 0.01, 6.0);
        assert!(!after.notes.iter().any(|n| n.note_index == a), "hit: gone");
        let missed = after.notes.iter().find(|n| n.note_index == b);
        assert!(
            missed.is_some_and(|n| n.y <= 0.0),
            "missed: past the receptor"
        );
        // The judgments flash on their lanes, then stop.
        let lane = usize::from(track.notes[b].lane);
        assert!(after.receptors[lane].flash.is_some());
        let later = feed.frame(&track, scroll, track.notes[b].seconds + 1.0, 6.0);
        assert!(later.receptors.iter().all(|r| r.flash.is_none()));
        // Nothing beyond the visible range, none far behind.
        assert!(
            later
                .notes
                .iter()
                .all(|n| n.y <= 6.0 && n.y >= -0.6 && (0.0..=1.0).contains(&n.alpha))
        );
        assert_eq!(feed.shown(t + 0.5).life, 0.5);
    }

    #[test]
    fn a_delay_holds_each_moment_back() {
        let song = song();
        let track = TapTrack::new(&song, 1).unwrap();
        let mut feed = FieldFeed::new(&track, 1.0, 0.25);
        let scroll = ScrollOptions::default();
        let mut m = moment(&track, 0, Judgement::W2, 0.01);
        m.life = 0.0;
        m.out = true;
        feed.push(&track, m);
        let t = m.at;
        // At its time: not shown yet; the note still there, life as it was.
        let f = feed.frame(&track, scroll, t + 0.1, 6.0);
        assert!(f.notes.iter().any(|n| n.note_index == 0));
        assert!(f.receptors.iter().all(|r| r.flash.is_none()));
        assert_eq!(
            feed.shown(t + 0.1),
            Shown {
                life: 1.0,
                combo: 0,
                out: false
            }
        );
        // The delay later: shown, out, the field blank.
        let f = feed.frame(&track, scroll, t + 0.26, 6.0);
        assert!(f.failed && f.notes.is_empty());
        assert!(f.receptors.iter().any(|r| r.flash.is_some()));
        assert_eq!(
            feed.shown(t + 0.26),
            Shown {
                life: 0.0,
                combo: 3,
                out: true
            }
        );
        assert_eq!(f.combo, 3);
    }

    #[test]
    fn shocks_show_from_their_commitment_and_a_hit_takes_one_off() {
        let song = song();
        let track = TapTrack::new(&song, 1).unwrap();
        let mut feed = FieldFeed::new(&track, 1.0, LOCAL_DELAY);
        let scroll = ScrollOptions::default();
        let section =
            ddi_engine::battle::schedule(&song, 1, &ddi_engine::rules::presets::itg().judge, 3)
                .into_iter()
                .flatten()
                .nth(2)
                .unwrap();
        let first = track.timing.seconds_at(section.cells[0].tick);
        let committed = first - 3.0;
        feed.add_hazards(&track, &section.cells, committed);
        assert_eq!(feed.shocks().len(), 8);
        let mines = |f: &Frame| {
            f.notes
                .iter()
                .filter(|n| n.kind == SpriteKind::Mine)
                .map(|n| n.note_index - HAZARD_INDEX_BASE)
                .collect::<Vec<_>>()
        };
        // Not before the commitment, in view after it.
        assert!(mines(&feed.frame(&track, scroll, committed - 0.1, 12.0)).is_empty());
        assert!(mines(&feed.frame(&track, scroll, first - 0.5, 12.0)).contains(&0));
        // The first stepped on: gone, its receptor flashing; the second
        // avoided: on past the receptor.
        let c = section.cells[0];
        feed.push(
            &track,
            Moment {
                at: first + 0.01,
                tick: c.tick,
                lane: c.lane,
                kind: MomentKind::ShockHit,
                life: 0.4,
                combo: 0,
                out: false,
                offset: 0.0,
                w0: None,
                score: ScoreView::default(),
            },
        );
        let second = track.timing.seconds_at(section.cells[1].tick);
        let f = feed.frame(&track, scroll, first + 0.05, 12.0);
        assert!(!mines(&f).contains(&0));
        assert!(f.receptors[usize::from(c.lane)].mine.is_some());
        assert_eq!(feed.shown(first + 0.05).life, 0.4);
        // The lane's lock, where the rules lock it: for the lock's second
        // from the moment, that lane alone; none where they do not.
        let lanes = track.lanes();
        assert!(feed.locks(lanes, first + 0.05).iter().all(Option::is_none));
        feed.set_lock(Some(1.0));
        let locks = feed.locks(lanes, first + 0.26);
        for (lane, l) in locks.iter().enumerate() {
            if lane == usize::from(c.lane) {
                assert!(l.is_some_and(|k| (k - 0.25).abs() < 1e-3), "{locks:?}");
            } else {
                assert_eq!(*l, None);
            }
        }
        assert!(feed.locks(lanes, first).iter().all(Option::is_none), "not before");
        assert!(feed.locks(lanes, first + 1.02).iter().all(Option::is_none));
        let after = feed.frame(&track, scroll, second + 0.05, 12.0);
        let passed = after
            .notes
            .iter()
            .find(|n| n.note_index == HAZARD_INDEX_BASE + 1);
        assert!(passed.is_some_and(|n| n.y < 0.0), "avoided: scrolls past");
    }

    /// A person's field (the chart itself): a hold's head hit pins it to
    /// the receptor, held, until its end takes it off; one let go is drawn
    /// dropped and scrolls on; a mine stepped on leaves the field and
    /// flashes its receptor; a Boo is said on the large field alone.
    #[test]
    fn a_persons_holds_mines_and_boos_show_as_their_stream_says() {
        let song = song();
        let chart = (0..song.charts.len())
            .find(|&c| {
                TapTrack::full(&song, c)
                    .is_ok_and(|t| t.notes.iter().filter(|n| matches!(n.shape, Shape::Hold { .. })).count() >= 2)
            })
            .expect("a chart with holds");
        let track = TapTrack::full(&song, chart).unwrap();
        let mut feed = FieldFeed::new(&track, 1.0, LOCAL_DELAY);
        let scroll = ScrollOptions::default();
        let holds: Vec<usize> = (0..track.len())
            .filter(|&i| matches!(track.notes[i].shape, Shape::Hold { .. }))
            .collect();
        let (a, b) = (holds[0], holds[1]);
        let end = |i: usize| match track.notes[i].shape {
            Shape::Hold { end_seconds, .. } => end_seconds,
            _ => unreachable!(),
        };
        let kind_of = |f: &Frame, i: usize| f.notes.iter().find(|n| n.note_index == i).map(|n| (n.kind, n.y));
        for i in [a, b] {
            feed.push(&track, moment(&track, i, Judgement::W1, 0.0));
        }
        let mid = track.notes[a].seconds + (end(a) - track.notes[a].seconds) / 2.0;
        if mid > track.notes[a].seconds + 0.02 {
            let f = feed.frame(&track, scroll, mid, 12.0);
            let (k, y) = kind_of(&f, a).expect("held: drawn");
            assert!(matches!(k, SpriteKind::HoldHead { active: true, dropped: false, .. } | SpriteKind::RollHead { active: true, dropped: false, .. }), "{k:?}");
            assert_eq!(y, 0.0, "pinned to the receptor");
            assert!(f.receptors[usize::from(track.notes[a].lane)].holding.is_some());
        }
        let mut held = moment(&track, a, Judgement::W1, 0.0);
        held.kind = MomentKind::HoldEnd(true);
        held.at = end(a);
        feed.push(&track, held);
        let mut let_go = moment(&track, b, Judgement::W1, 0.0);
        let_go.kind = MomentKind::HoldEnd(false);
        let_go.at = track.notes[b].seconds + 0.01;
        feed.push(&track, let_go);
        let after = feed.frame(&track, scroll, end(a) + 0.01, 12.0);
        assert!(kind_of(&after, a).is_none(), "held to its end: gone");
        let t = track.notes[b].seconds + 0.02;
        if end(b) > t + 0.05 {
            let f = feed.frame(&track, scroll, t, 12.0);
            let (k, _) = kind_of(&f, b).expect("let go: still drawn");
            assert!(matches!(k, SpriteKind::HoldHead { dropped: true, active: false, .. } | SpriteKind::RollHead { dropped: true, active: false, .. }), "{k:?}");
        }
        // Every hold drawn within its field: no head past the near edge, no
        // tail past the far end (a miniature's panel ends there).
        for t in [track.notes[b].seconds + 1.5, track.notes[a].seconds - 3.0] {
            let f = feed.frame(&track, scroll, t, 4.0);
            for n in &f.notes {
                if let SpriteKind::HoldHead { tail_y, .. } | SpriteKind::RollHead { tail_y, .. } = n.kind {
                    assert!(n.y >= -0.6 && tail_y <= 4.0, "{t}: {n:?}");
                }
            }
        }
        // A Boo: the large field's word, nothing on a miniature.
        let mut boo = moment(&track, 0, Judgement::Miss, 0.0);
        boo.kind = MomentKind::Boo;
        boo.at = 0.5;
        feed.push(&track, boo);
        let large = feed.frame_large(&track, scroll, 0.6, 12.0);
        assert!(large.judgement.is_some_and(|j| j.boo));
        assert!(feed.frame(&track, scroll, 0.6, 12.0).judgement.is_none());
        // A mine of the chart, where it has one: drawn, and gone once
        // stepped on, its receptor flashing.
        if let Some(m) = (0..track.len()).find(|&i| track.notes[i].shape == Shape::Mine) {
            let at = track.notes[m].seconds;
            let before = feed.frame(&track, scroll, at - 0.2, 12.0);
            assert!(matches!(kind_of(&before, m), Some((SpriteKind::Mine, _))));
            let mut hit = moment(&track, m, Judgement::Miss, 0.0);
            hit.kind = MomentKind::MineHit;
            feed.push(&track, hit);
            let f = feed.frame(&track, scroll, at + 0.01, 12.0);
            assert!(kind_of(&f, m).is_none());
            assert!(f.receptors[usize::from(track.notes[m].lane)].mine.is_some());
        }
    }

    #[test]
    fn moments_out_of_order_take_their_place() {
        let song = song();
        let track = TapTrack::new(&song, 1).unwrap();
        let mut feed = FieldFeed::new(&track, 1.0, LOCAL_DELAY);
        let second = (2..track.len())
            .find(|&i| track.notes[i].seconds > track.notes[1].seconds)
            .unwrap();
        let mut late = moment(&track, second, Judgement::W3, 0.0);
        late.life = 0.3;
        let mut early = moment(&track, 1, Judgement::W1, 0.0);
        early.life = 0.9;
        feed.push(&track, late);
        feed.push(&track, early);
        assert!(feed.moments().windows(2).all(|w| w[0].at <= w[1].at));
        assert_eq!(feed.shown(late.at).life, 0.3);
        assert_eq!(feed.shown(early.at).life, 0.9);
    }
}
