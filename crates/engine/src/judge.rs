//! Per-note judging state machine.
//!
//! [`Judge`] precomputes every note's judged second, keeps one ordered
//! queue per lane and turns presses, releases and the passage of time into
//! [`JudgeEvent`]s. It knows nothing about combo, score or life: those are
//! layered on in [`crate::player`].
//!
//! Press resolution follows StepMania's ordered iteration: the **earliest
//! unjudged tap-like note in the lane whose window contains the press** is
//! taken (not the closest one). When both a tap and a mine are candidates,
//! the one closer in time wins, as SM's `GetClosestNote` would.

use std::collections::{BTreeMap, HashSet};

use ddi_chart::{Note, NoteKind, TICKS_PER_BEAT, Tick, TimingMap};
use serde::{Deserialize, Serialize};

use crate::rules::{EmptyPress, HoldRules, JudgeTable, Judgement};

/// Lifecycle of one note.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum NoteState {
    /// Not yet judged.
    Pending,
    /// Tap (or hold/roll head, or lift) hit; `delta` = press − note, seconds.
    Hit {
        judgement: Judgement,
        delta: f64,
    },
    /// Passed the widest window without a press.
    Missed,
    /// Hold/roll head hit, body in progress; `life` in `0..=1`.
    HoldActive {
        life: f32,
    },
    /// Hold/roll kept to its tail.
    Held,
    /// Hold/roll dropped.
    LetGo,
    MineHit,
    MineAvoided,
    /// Never judged (fake, dummy, autokeysound, inside a warp or fake segment).
    Faked,
}

impl NoteState {
    /// Whether the note can still produce events.
    pub fn is_terminal(self) -> bool {
        !matches!(self, NoteState::Pending | NoteState::HoldActive { .. })
    }
}

/// A chart note with its judging data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JudgedNote {
    pub note: Note,
    /// Song second at which the note is judged.
    pub seconds: f64,
    /// Song second of the tail (holds/rolls), else `seconds`.
    pub end_seconds: f64,
    /// `false` for fakes, dummies, autokeysounds and notes inside warps or
    /// fake segments; such notes start and stay `Faked`.
    pub judgeable: bool,
    pub state: NoteState,
    /// Index into [`Judge::rows`] for judgeable tap-like notes.
    pub row: Option<usize>,
    /// Hold/roll body: song second since which life has been decaying.
    /// `None` while a hold is pressed. Rolls always decay.
    decay_from: Option<f64>,
    /// Seconds a hold has been released in all before `decay_from`
    /// ([`HoldRules::ReleaseBudget`]).
    released: f64,
    /// Ticked holds ([`HoldRules::Ticked`]): checkpoints kept and missed,
    /// and whether the head was stepped on.
    checkpoints_hit: u32,
    checkpoints_missed: u32,
    head_hit: bool,
}

impl JudgedNote {
    fn is_tap_like(&self) -> bool {
        matches!(
            self.note.kind,
            NoteKind::Tap | NoteKind::HoldHead { .. } | NoteKind::RollHead { .. } | NoteKind::Lift
        )
    }

    fn is_press_judged(&self) -> bool {
        matches!(
            self.note.kind,
            NoteKind::Tap | NoteKind::HoldHead { .. } | NoteKind::RollHead { .. }
        )
    }

    fn is_mine(&self) -> bool {
        matches!(self.note.kind, NoteKind::Mine | NoteKind::Shock)
    }

    fn is_roll(&self) -> bool {
        matches!(self.note.kind, NoteKind::RollHead { .. })
    }

    fn is_long(&self) -> bool {
        self.note.kind.is_long()
    }
}

/// Simultaneous judgeable tap-like notes.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    pub tick: Tick,
    /// Number of judgeable tap-like notes on the row.
    pub size: u32,
}

/// What happened to a note (or an empty press).
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum JudgeEventKind {
    /// Tap-like note judged; `delta` = press − note in seconds (positive =
    /// late). For `Miss` the delta is the late bound at which the note was
    /// declared missed.
    Tap(Judgement, f64),
    /// Hold/roll kept to its tail (O.K.).
    Held,
    /// Hold/roll dropped (N.G.).
    LetGo,
    HitMine,
    /// A shock ([`JudgeTable::shock_rows`]) passed with no lane stepped on
    /// (O.K.): one event a row, on its first lane.
    AvoidShock,
    /// A shock stepped on in some lane (N.G.): one event a row, on the
    /// lane that stepped.
    HitShock,
    /// Empty press, per [`EmptyPress`].
    Boo,
    /// A checkpoint row of ticked holds ([`HoldRules::Ticked`]): `held` of
    /// the holds crossing it were kept and `missed` were not; a hit when
    /// none was missed. `lanes` has a bit per hold's lane (lanes 0–63).
    Checkpoint {
        held: u16,
        missed: u16,
        lanes: u64,
    },
}

impl JudgeEventKind {
    /// What the event weighs as in a score or a gauge: a checkpoint as a
    /// `W1` when hit and a `Miss` when not, and with ticked holds a hold's
    /// own outcome as nothing (StepMania's pump weights,
    /// `docs/plans/pump.md`); a shock as a hold kept when avoided and
    /// dropped when stepped on (DDR's O.K. and N.G.). `None`: it weighs
    /// nothing.
    pub fn scored_as(self, ticked: bool) -> Option<JudgeEventKind> {
        match self {
            JudgeEventKind::AvoidShock => Some(JudgeEventKind::Held),
            JudgeEventKind::HitShock => Some(JudgeEventKind::LetGo),
            JudgeEventKind::Checkpoint { missed, .. } => Some(if missed == 0 {
                JudgeEventKind::Tap(Judgement::W1, 0.0)
            } else {
                JudgeEventKind::Tap(Judgement::Miss, 0.0)
            }),
            JudgeEventKind::Held | JudgeEventKind::LetGo if ticked => None,
            other => Some(other),
        }
    }
}

/// One judging outcome.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JudgeEvent {
    /// Index into the chart's notes; `None` for `Boo`.
    pub note_index: Option<usize>,
    pub lane: u8,
    /// The note's tick (`Tick::ZERO` for `Boo`).
    pub tick: Tick,
    pub kind: JudgeEventKind,
    /// Song second the event happened at.
    pub song_time: f64,
}

/// StepMania's `InitialHoldLife` for pump: a ticked hold or roll whose
/// panel is up as its head passes starts with this much life.
const INITIAL_HOLD_LIFE: f64 = 0.05;

/// A row of hold checkpoints ([`HoldRules::Ticked`]).
#[derive(Clone, Debug, PartialEq)]
struct CheckpointRow {
    tick: Tick,
    seconds: f64,
    /// Holds and rolls crossing the row.
    holds: Vec<usize>,
    /// Judged on its own: no step (tap or hold head) on the row.
    scored: bool,
}

/// The per-note state machine for one chart.
#[derive(Clone, Debug)]
pub struct Judge {
    notes: Vec<JudgedNote>,
    rows: Vec<Row>,
    /// Note indices per lane, in time order.
    lanes: Vec<Vec<usize>>,
    /// Per lane, position of the first non-terminal note in `lanes`.
    cursor: Vec<usize>,
    table: JudgeTable,
    last_time: Option<f64>,
    /// Checkpoint rows in time order, with ticked holds.
    checkpoints: Vec<CheckpointRow>,
    /// The first checkpoint row not judged yet.
    next_checkpoint: usize,
    /// Ticked holds' and rolls' heads in time order, and the first whose
    /// time has not passed yet.
    heads: Vec<usize>,
    next_head: usize,
    /// Each lane's button, as presses and releases left it: what it was
    /// all the time since [`Judge::last_time`].
    down: Vec<bool>,
    /// Shock rows ([`JudgeTable::shock_rows`]): each row's notes, one a
    /// lane.
    shocks: Vec<Vec<usize>>,
    /// The second of the chart's first judged press ([`EmptyPress::BooFromFirstNote`]):
    /// that of the chart as given, a play started part-way too.
    first_note: Option<f64>,
}

impl Judge {
    /// Precomputes seconds and judgeability for `notes` (sorted by tick).
    pub fn new(notes: &[Note], timing: &TimingMap, table: JudgeTable) -> Judge {
        let notes: Vec<JudgedNote> = notes
            .iter()
            .map(|n| {
                let seconds = timing.seconds_at(n.tick);
                let end_seconds = match n.kind.hold_end() {
                    Some(end) => timing.seconds_at(end).max(seconds),
                    None => seconds,
                };
                let judgeable = !matches!(
                    n.kind,
                    NoteKind::Fake | NoteKind::Dummy | NoteKind::AutoKeysound
                ) && timing.judgeable(n.tick);
                JudgedNote {
                    note: n.clone(),
                    seconds,
                    end_seconds,
                    judgeable,
                    state: if judgeable {
                        NoteState::Pending
                    } else {
                        NoteState::Faked
                    },
                    row: None,
                    decay_from: None,
                    released: 0.0,
                    checkpoints_hit: 0,
                    checkpoints_missed: 0,
                    head_hit: false,
                }
            })
            .collect();
        let lane_count = notes
            .iter()
            .map(|n| usize::from(n.note.lane) + 1)
            .max()
            .unwrap_or(0);
        let mut lanes = vec![Vec::new(); lane_count];
        for (i, n) in notes.iter().enumerate() {
            if n.judgeable {
                lanes[usize::from(n.note.lane)].push(i);
            }
        }
        for lane in &mut lanes {
            lane.sort_by(|&a, &b| {
                notes[a]
                    .seconds
                    .total_cmp(&notes[b].seconds)
                    .then(notes[a].note.tick.cmp(&notes[b].note.tick))
            });
        }
        // Shocks: the notes of each row together.
        let mut shocks: Vec<Vec<usize>> = Vec::new();
        let mut shock_ticks: Vec<Tick> = Vec::new();
        for (i, n) in notes.iter().enumerate() {
            if n.judgeable && n.note.kind == NoteKind::Shock {
                match shock_ticks.iter().position(|&t| t == n.note.tick) {
                    Some(r) => shocks[r].push(i),
                    None => {
                        shock_ticks.push(n.note.tick);
                        shocks.push(vec![i]);
                    }
                }
            }
        }
        let (checkpoints, heads) = if table.hold.is_ticked() {
            let mut heads: Vec<usize> = (0..notes.len())
                .filter(|&i| notes[i].judgeable && notes[i].is_long())
                .collect();
            heads.sort_by(|&a, &b| notes[a].seconds.total_cmp(&notes[b].seconds));
            (checkpoint_rows(&notes, timing), heads)
        } else {
            (Vec::new(), Vec::new())
        };
        let first_note = notes
            .iter()
            .filter(|n| n.judgeable && n.is_press_judged())
            .map(|n| n.seconds)
            .reduce(f64::min);
        let mut judge = Judge {
            first_note,
            notes,
            rows: Vec::new(),
            cursor: vec![0; lane_count],
            lanes,
            table,
            last_time: None,
            checkpoints,
            next_checkpoint: 0,
            heads,
            next_head: 0,
            down: vec![false; lane_count],
            shocks,
        };
        judge.rebuild_rows();
        judge
    }

    /// Whether a note gets a tap judgement: tap-like, and for a hold head
    /// only when the ruleset judges hold starts.
    fn is_step(&self, n: &JudgedNote) -> bool {
        n.judgeable
            && n.is_tap_like()
            && (self.table.hold_start.judged || !matches!(n.note.kind, NoteKind::HoldHead { .. }))
    }

    fn rebuild_rows(&mut self) {
        self.rows.clear();
        let mut order: Vec<usize> = (0..self.notes.len())
            .filter(|&i| self.is_step(&self.notes[i]))
            .collect();
        order.sort_by_key(|&i| self.notes[i].note.tick);
        for i in order {
            let tick = self.notes[i].note.tick;
            match self.rows.last_mut() {
                Some(row) if row.tick == tick => row.size += 1,
                _ => self.rows.push(Row { tick, size: 1 }),
            }
            self.notes[i].row = Some(self.rows.len() - 1);
        }
    }

    /// Marks every note that ends before `song_time` as `Faked`, for plays
    /// that start part-way through a song. Call before the first update.
    pub fn skip_before(&mut self, song_time: f64) {
        for n in &mut self.notes {
            if n.judgeable && n.end_seconds < song_time {
                n.judgeable = false;
                n.state = NoteState::Faked;
                n.row = None;
            }
        }
        for lane in 0..self.lanes.len() {
            self.lanes[lane].retain(|&i| self.notes[i].judgeable);
        }
        self.rebuild_rows();
        let notes = &self.notes;
        self.heads.retain(|&i| notes[i].judgeable);
        self.checkpoints.retain_mut(|row| {
            row.holds.retain(|&i| notes[i].judgeable);
            row.seconds >= song_time && !row.holds.is_empty()
        });
    }

    /// Judges presses in at least `lanes` lanes, those without notes too
    /// (where every stray press counts, a Boo in a lane the chart leaves
    /// empty).
    pub fn with_lanes(mut self, lanes: usize) -> Judge {
        if lanes > self.lanes.len() {
            self.lanes.resize(lanes, Vec::new());
            self.cursor.resize(lanes, 0);
            self.down.resize(lanes, false);
        }
        self
    }

    pub fn table(&self) -> &JudgeTable {
        &self.table
    }

    /// The song second judged up to (the last press, release or update),
    /// `None` before any.
    pub fn last_time(&self) -> Option<f64> {
        self.last_time
    }

    /// All notes, in chart order.
    pub fn notes(&self) -> &[JudgedNote] {
        &self.notes
    }

    pub fn note(&self, index: usize) -> Option<&JudgedNote> {
        self.notes.get(index)
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// Lanes the chart uses (highest lane + 1).
    pub fn lane_count(&self) -> usize {
        self.lanes.len()
    }

    /// Judgeable tap-like notes (taps, hold/roll heads, lifts), without
    /// hold heads when hold starts are not judged.
    pub fn judged_note_count(&self) -> u32 {
        self.notes.iter().filter(|n| self.is_step(n)).count() as u32
    }

    /// Judgeable holds and rolls.
    pub fn hold_count(&self) -> u32 {
        self.notes
            .iter()
            .filter(|n| n.judgeable && n.is_long())
            .count() as u32
    }

    /// Judgeable mines.
    pub fn mine_count(&self) -> u32 {
        self.notes
            .iter()
            .filter(|n| n.judgeable && n.note.kind == NoteKind::Mine)
            .count() as u32
    }

    /// Shock rows ([`JudgeTable::shock_rows`]).
    pub fn shock_count(&self) -> u32 {
        self.shocks.len() as u32
    }

    /// The shock row stepped on in note `idx`'s lane at `song_time`: every
    /// lane's shock gone, one N.G.
    fn hit_shock(&mut self, idx: usize, song_time: f64, out: &mut Vec<JudgeEvent>) {
        if let Some(row) = self.shocks.iter().find(|r| r.contains(&idx)) {
            for &i in row {
                self.notes[i].state = NoteState::MineHit;
            }
        }
        out.push(self.event(idx, JudgeEventKind::HitShock, song_time));
    }

    /// Note `idx`, a shock, passed unstepped: once every lane's has, one
    /// O.K. on the row's first lane.
    fn avoid_shock(&mut self, idx: usize, out: &mut Vec<JudgeEvent>) {
        let Some(row) = self.shocks.iter().find(|r| r.contains(&idx)) else {
            return;
        };
        if row
            .iter()
            .all(|&i| self.notes[i].state == NoteState::MineAvoided)
        {
            let first = *row
                .iter()
                .min_by_key(|&&i| self.notes[i].note.lane)
                .expect("a shock row has notes");
            let at = self.notes[first].seconds + self.table.mine_window_seconds();
            out.push(self.event(first, JudgeEventKind::AvoidShock, at));
        }
    }

    pub fn row_count(&self) -> u32 {
        self.rows.len() as u32
    }

    /// Checkpoint rows judged on their own (ticked holds), the ones a
    /// score counts.
    pub fn checkpoint_count(&self) -> u32 {
        self.checkpoints.iter().filter(|r| r.scored).count() as u32
    }

    /// Whether every note has reached a terminal state.
    pub fn all_resolved(&self) -> bool {
        self.notes.iter().all(|n| n.state.is_terminal())
    }

    /// Song second at which the last note ends, `None` for an empty chart.
    pub fn last_end_seconds(&self) -> Option<f64> {
        self.notes
            .iter()
            .map(|n| n.end_seconds)
            .fold(None, |acc: Option<f64>, s| {
                Some(acc.map_or(s, |a| a.max(s)))
            })
    }

    fn event(&self, idx: usize, kind: JudgeEventKind, song_time: f64) -> JudgeEvent {
        let n = &self.notes[idx];
        JudgeEvent {
            note_index: Some(idx),
            lane: n.note.lane,
            tick: n.note.tick,
            kind,
            song_time,
        }
    }

    /// How far ahead of the time judged a note or a mine can be reached.
    fn lookahead(&self) -> f64 {
        self.table.max_early().max(self.table.mine_window_seconds())
    }

    /// The mine (or shock) `idx` is stepped on at `at`.
    fn step_on_mine(&mut self, idx: usize, at: f64, out: &mut Vec<JudgeEvent>) {
        if self.notes[idx].note.kind == NoteKind::Shock {
            self.hit_shock(idx, at, out);
        } else {
            self.notes[idx].state = NoteState::MineHit;
            out.push(self.event(idx, JudgeEventKind::HitMine, at));
        }
    }

    fn hold_window(&self, idx: usize) -> f64 {
        match (self.table.hold, self.notes[idx].is_roll()) {
            (_, true) => self.table.roll_window_seconds(),
            (HoldRules::Ticked { checkpoint_window }, false) => {
                self.table.scaled(checkpoint_window)
            }
            _ => self.table.hold_window_seconds(),
        }
    }

    fn ticked(&self) -> bool {
        self.table.hold.is_ticked()
    }

    /// Hold life at `song_time` given the decay start (`None` = full, or
    /// what the release budget has left).
    fn hold_life(&self, idx: usize, song_time: f64) -> f32 {
        let n = &self.notes[idx];
        let until = song_time.min(n.end_seconds);
        let open = n.decay_from.map_or(0.0, |from| (until - from).max(0.0));
        match (self.table.hold, n.is_roll()) {
            (HoldRules::ReleaseBudget { budget }, false) => {
                let used = n.released + open;
                if used > budget {
                    0.0
                } else {
                    (1.0 - used / budget.max(f64::EPSILON)).max(f64::MIN_POSITIVE) as f32
                }
            }
            _ => match n.decay_from {
                None => 1.0,
                Some(_) => (1.0 - open / self.hold_window(idx)).max(0.0) as f32,
            },
        }
    }

    /// The ticked heads whose time passed before `song_time` (StepMania's
    /// `CrossedRows`): a head whose panel is down is stepped on, a `W1`;
    /// otherwise its hold starts anyway, with [`INITIAL_HOLD_LIFE`].
    fn cross_heads(&mut self, song_time: f64, out: &mut Vec<JudgeEvent>) {
        while let Some(&idx) = self.heads.get(self.next_head) {
            let n = &self.notes[idx];
            if n.seconds >= song_time {
                break;
            }
            self.next_head += 1;
            if n.state != NoteState::Pending {
                continue;
            }
            let at = n.seconds;
            if self
                .down
                .get(usize::from(n.note.lane))
                .copied()
                .unwrap_or(false)
            {
                self.step_on_ticked_head(idx, 0.0, at, out);
            } else {
                let window = self.hold_window(idx);
                self.notes[idx].decay_from = Some(at - (1.0 - INITIAL_HOLD_LIFE) * window);
            }
        }
    }

    /// A ticked hold's head stepped on at `at`: always a `W1` (StepMania's
    /// `Player::Step` without `RequireStepOnHoldHeads`), the hold full.
    fn step_on_ticked_head(&mut self, idx: usize, delta: f64, at: f64, out: &mut Vec<JudgeEvent>) {
        out.push(self.event(idx, JudgeEventKind::Tap(Judgement::W1, delta), at));
        let n = &mut self.notes[idx];
        n.head_hit = true;
        n.state = NoteState::HoldActive { life: 1.0 };
        n.decay_from = n.is_roll().then_some(at);
    }

    /// Judges the checkpoint rows up to `song_time` (ticked holds): a hold
    /// is kept at a row when its life is above zero there (a hold whose
    /// head is still unjudged too: StepMania starts it at its head either
    /// way). A row is one event, when it has no step of its own. The row
    /// of a hold's tail does not count for the hold's outcome: StepMania
    /// settles the hold (`UpdateHoldNotes`) before it crosses that row.
    fn checkpoints_until(&mut self, song_time: f64, out: &mut Vec<JudgeEvent>) {
        while let Some(row) = self.checkpoints.get(self.next_checkpoint) {
            if row.seconds >= song_time {
                break;
            }
            let row = row.clone();
            self.next_checkpoint += 1;
            let (mut held, mut missed, mut lanes) = (0u16, 0u16, 0u64);
            for &idx in &row.holds {
                let alive = match self.notes[idx].state {
                    NoteState::Pending | NoteState::HoldActive { .. } => {
                        self.hold_life(idx, row.seconds) > 0.0
                    }
                    _ => continue,
                };
                let n = &mut self.notes[idx];
                let tail = n.note.end_tick() == row.tick;
                if alive {
                    n.checkpoints_hit += u32::from(!tail);
                    held = held.saturating_add(1);
                } else {
                    n.checkpoints_missed += u32::from(!tail);
                    missed = missed.saturating_add(1);
                }
                if n.note.lane < 64 {
                    lanes |= 1 << n.note.lane;
                }
            }
            if row.scored && held + missed > 0 {
                let first = row.holds[0];
                out.push(JudgeEvent {
                    note_index: Some(first),
                    lane: self.notes[first].note.lane,
                    tick: row.tick,
                    kind: JudgeEventKind::Checkpoint {
                        held,
                        missed,
                        lanes,
                    },
                    song_time: row.seconds,
                });
            }
        }
    }

    /// When an active hold's life runs out, as it decays now (`None`: it
    /// is not decaying). A release budget is spent at that instant and
    /// exceeded just after: the hold is dropped then as well.
    fn drop_time(&self, idx: usize) -> Option<f64> {
        let n = &self.notes[idx];
        let from = n.decay_from?;
        Some(match (self.table.hold, n.is_roll()) {
            (HoldRules::ReleaseBudget { budget }, false) => from + (budget - n.released).max(0.0),
            _ => from + self.hold_window(idx),
        })
    }

    /// A released hold is pressed again at `song_time`: under a release
    /// budget the time it was released for is spent.
    fn regrab(&mut self, idx: usize, song_time: f64) {
        let n = &mut self.notes[idx];
        if let Some(from) = n.decay_from.take()
            && matches!(self.table.hold, HoldRules::ReleaseBudget { .. })
        {
            n.released += (song_time - from).max(0.0);
        }
    }

    /// A tap-like note passed unjudged, at `at`: a miss, and for a hold
    /// whose start is only accepted (danoniplus) a dropped hold too. A
    /// ticked hold goes on (its head missed): held while its lane is down.
    fn miss_note(&mut self, idx: usize, at: f64, out: &mut Vec<JudgeEvent>) {
        let is_hold = matches!(self.notes[idx].note.kind, NoteKind::HoldHead { .. });
        let start = self.table.hold_start;
        let body = self.notes[idx].is_long();
        let miss = self.event(
            idx,
            JudgeEventKind::Tap(Judgement::Miss, at - self.notes[idx].seconds),
            at,
        );
        if body && self.ticked() {
            out.push(miss);
            // Its life runs from its head ([`Judge::cross_heads`]).
            let life = self.hold_life(idx, at);
            self.notes[idx].state = NoteState::HoldActive { life };
        } else if is_hold && start.accept.is_some() {
            if start.judged {
                out.push(miss);
            }
            self.notes[idx].state = NoteState::LetGo;
            out.push(self.event(idx, JudgeEventKind::LetGo, at));
        } else {
            self.notes[idx].state = NoteState::Missed;
            out.push(miss);
        }
    }

    /// Resolves an active hold with judging up to `song_time`: `LetGo`
    /// once the instant its life ran out is passed, `Held` once its tail
    /// is, otherwise just refreshes `life`. A ticked hold is only resolved
    /// past its tail: kept when it missed no checkpoint and hit one
    /// (without checkpoints: when its head was stepped on), a ticked roll
    /// when its life lasted.
    fn settle_hold(&mut self, idx: usize, song_time: f64, out: &mut Vec<JudgeEvent>) {
        let life = self.hold_life(idx, song_time);
        if self.ticked() {
            let n = &self.notes[idx];
            if song_time <= n.end_seconds {
                self.notes[idx].state = NoteState::HoldActive { life };
                return;
            }
            let kept = if n.is_roll() {
                life > 0.0
            } else if n.checkpoints_hit + n.checkpoints_missed == 0 {
                n.head_hit
            } else {
                n.checkpoints_missed == 0
            };
            let end = n.end_seconds;
            let (state, kind) = if kept {
                (NoteState::Held, JudgeEventKind::Held)
            } else {
                (NoteState::LetGo, JudgeEventKind::LetGo)
            };
            self.notes[idx].state = state;
            out.push(self.event(idx, kind, end));
            return;
        }
        let end = self.notes[idx].end_seconds;
        match self.drop_time(idx) {
            // Its life runs out by its tail (at its tail too: StepMania
            // checks the life before the tail).
            Some(at) if at <= end => {
                if at < song_time {
                    self.notes[idx].state = NoteState::LetGo;
                    out.push(self.event(idx, JudgeEventKind::LetGo, at));
                } else {
                    self.notes[idx].state = NoteState::HoldActive { life };
                }
            }
            _ if end < song_time => {
                self.notes[idx].state = NoteState::Held;
                out.push(self.event(idx, JudgeEventKind::Held, end));
            }
            _ => self.notes[idx].state = NoteState::HoldActive { life },
        }
    }

    /// Misses the tap-like notes of `lane` that passed unjudged before
    /// `song_time`: past every window, or given way to the next note.
    fn expire(&mut self, lane: usize, song_time: f64, out: &mut Vec<JudgeEvent>) {
        let miss_after = self.table.miss_after();
        let queue = &self.lanes[lane][self.cursor[lane]..];
        let mut missed = Vec::new();
        for (qi, &idx) in queue.iter().enumerate() {
            let n = &self.notes[idx];
            if n.seconds > song_time {
                break;
            }
            if n.state != NoteState::Pending || !n.is_tap_like() {
                continue;
            }
            // danoniplus: an unjudged late note gives way to the next one
            // (a hold's start to any note, a tap only to a tap).
            let superseded = self.table.supersede.and_then(|s| {
                let hold = matches!(n.note.kind, NoteKind::HoldHead { .. });
                let next = queue[qi + 1..].iter().find(|&&j| {
                    let m = &self.notes[j];
                    m.is_tap_like() && (hold || !matches!(m.note.kind, NoteKind::HoldHead { .. }))
                })?;
                let (within, late) = if hold {
                    (s.hold_next_within, s.hold_prev_late)
                } else {
                    (s.next_within, s.prev_late)
                };
                let at = (self.notes[*next].seconds - within).max(n.seconds + late);
                (song_time > at && at - n.seconds <= miss_after).then_some(at)
            });
            if let Some(at) = superseded {
                missed.push((idx, at));
            } else if song_time > n.seconds + miss_after {
                // The same sum as the event's time, so the instant and the
                // test agree to the last bit.
                missed.push((idx, n.seconds + miss_after));
            }
        }
        for (idx, at) in missed {
            self.miss_note(idx, at, out);
        }
        self.advance_cursor(lane);
    }

    fn advance_cursor(&mut self, lane: usize) {
        while let Some(&idx) = self.lanes[lane].get(self.cursor[lane]) {
            if self.notes[idx].state.is_terminal() {
                self.cursor[lane] += 1;
            } else {
                break;
            }
        }
    }

    /// A button went down in `lane` at `song_time` (already judge-offset).
    pub fn press(&mut self, lane: u8, song_time: f64) -> Vec<JudgeEvent> {
        let mut out = Vec::new();
        let lane_idx = usize::from(lane);
        if lane_idx >= self.lanes.len() {
            return out;
        }
        if !song_time.is_finite() {
            return out;
        }
        // Everything before the press, the lane still as it was.
        self.advance(song_time, &mut out);
        self.down[lane_idx] = true;
        let max_early = self.lookahead();
        let (from, to) = (self.cursor[lane_idx], self.lanes[lane_idx].len());

        // Active holds get their life back; active rolls restart their decay.
        // (A hold is active from a press no earlier than its window.)
        for qi in from..to {
            let idx = self.lanes[lane_idx][qi];
            if self.notes[idx].seconds - song_time > max_early {
                break;
            }
            if let NoteState::HoldActive { .. } = self.notes[idx].state {
                let life = self.hold_life(idx, song_time);
                if self.ticked() {
                    // Never dropped before its end: full again.
                    let n = &mut self.notes[idx];
                    n.decay_from = n.is_roll().then_some(song_time);
                    n.state = NoteState::HoldActive { life: 1.0 };
                } else if life <= 0.0 {
                    self.settle_hold(idx, song_time, &mut out);
                } else if self.notes[idx].is_roll() {
                    self.notes[idx].decay_from = Some(song_time);
                    self.notes[idx].state = NoteState::HoldActive { life: 1.0 };
                } else {
                    self.regrab(idx, song_time);
                    let life = self.hold_life(idx, song_time);
                    self.notes[idx].state = NoteState::HoldActive { life };
                }
            }
        }

        let mut tap: Option<(usize, f64, Judgement)> = None;
        let mut mine: Option<(usize, f64)> = None;
        for qi in from..to {
            let idx = self.lanes[lane_idx][qi];
            let n = &self.notes[idx];
            if n.state != NoteState::Pending {
                continue;
            }
            let delta = song_time - n.seconds;
            if delta < -max_early {
                break;
            }
            if n.is_press_judged() && tap.is_none() {
                if let Some(j) = self.table.judge(delta) {
                    tap = Some((idx, delta, j));
                }
            } else if n.is_mine()
                && mine.is_none()
                && delta.abs() <= self.table.mine_window_seconds()
            {
                mine = Some((idx, delta));
            }
        }
        let hit_mine = match (tap, mine) {
            (Some((_, td, _)), Some((_, md))) => md.abs() < td.abs(),
            (None, Some(_)) => true,
            _ => false,
        };
        if hit_mine {
            let (idx, _) = mine.expect("mine candidate");
            self.step_on_mine(idx, song_time, &mut out);
        } else if let Some((idx, delta, _)) =
            tap.filter(|&(idx, ..)| self.ticked() && self.notes[idx].is_long())
        {
            self.step_on_ticked_head(idx, delta, song_time, &mut out);
        } else if let Some((idx, delta, judgement)) = tap {
            let is_hold = self.notes[idx].is_long();
            // Hold start rules apply to holds; rolls are StepMania's own.
            let freeze = matches!(self.notes[idx].note.kind, NoteKind::HoldHead { .. });
            let start = self.table.hold_start;
            // danoniplus: a hold starts only within its acceptance window;
            // a judged press outside it drops the hold.
            let accepted = !freeze
                || start.accept.is_none_or(|w| {
                    let bound = if delta < 0.0 { w.early } else { w.late };
                    delta.abs() <= self.table.scaled(bound)
                });
            if !freeze || start.judged {
                out.push(self.event(idx, JudgeEventKind::Tap(judgement, delta), song_time));
            }
            self.notes[idx].head_hit = true;
            self.notes[idx].state = if !is_hold {
                NoteState::Hit { judgement, delta }
            } else if accepted {
                NoteState::HoldActive { life: 1.0 }
            } else {
                out.push(self.event(idx, JudgeEventKind::LetGo, song_time));
                NoteState::LetGo
            };
            self.notes[idx].decay_from = if self.notes[idx].is_roll() {
                Some(song_time)
            } else {
                None
            };
        } else {
            let boo = match self.table.empty_press {
                EmptyPress::Ignore => false,
                EmptyPress::Boo => true,
                // FFR: nothing before the first note. R^3 times a Boo by
                // the game's frame, which a press inside three frames of a
                // note of its lane can move past that note (`booConflict`);
                // a press so close always hits the note here (the press's
                // own time is judged, not a frame's), so that never arises.
                EmptyPress::BooFromFirstNote => self.first_note.is_some_and(|t| song_time >= t),
                EmptyPress::ExcessiveEarly { lo, hi, .. } => self.lanes[lane_idx][from..]
                    .iter()
                    .map(|&idx| &self.notes[idx])
                    .take_while(|n| n.seconds - song_time <= hi)
                    .any(|n| {
                        let early = n.seconds - song_time;
                        n.state == NoteState::Pending && n.is_press_judged() && early >= lo
                    }),
            };
            if boo {
                out.push(JudgeEvent {
                    note_index: None,
                    lane,
                    tick: Tick::ZERO,
                    kind: JudgeEventKind::Boo,
                    song_time,
                });
            }
        }
        self.advance_cursor(lane_idx);
        // What was judged on the way (in order, all earlier) and then the
        // press's own.
        out
    }

    /// A button went up in `lane` at `song_time`: judges lifts and starts
    /// the life decay of active holds.
    pub fn release(&mut self, lane: u8, song_time: f64) -> Vec<JudgeEvent> {
        let mut out = Vec::new();
        let lane_idx = usize::from(lane);
        if lane_idx >= self.lanes.len() {
            return out;
        }
        if !song_time.is_finite() {
            return out;
        }
        // Everything before the release, the lane still down.
        self.advance(song_time, &mut out);
        self.down[lane_idx] = false;
        let max_early = self.table.max_early();
        let mut lift: Option<(usize, f64, Judgement)> = None;
        for qi in self.cursor[lane_idx]..self.lanes[lane_idx].len() {
            let idx = self.lanes[lane_idx][qi];
            let n = &self.notes[idx];
            match n.state {
                NoteState::HoldActive { .. } => {
                    // A hold hit early and let go before its head decays
                    // from its head: StepMania judges a hold's body from
                    // its head row on (`UpdateHoldNotes`).
                    if !n.is_roll() && n.decay_from.is_none() {
                        self.notes[idx].decay_from = Some(song_time.max(n.seconds));
                    }
                }
                NoteState::Pending => {
                    let delta = song_time - n.seconds;
                    if delta < -max_early {
                        break;
                    }
                    if n.note.kind == NoteKind::Lift
                        && lift.is_none()
                        && let Some(j) = self.table.judge(delta)
                    {
                        lift = Some((idx, delta, j));
                    }
                }
                _ => {}
            }
        }
        if let Some((idx, delta, judgement)) = lift {
            self.notes[idx].state = NoteState::Hit { judgement, delta };
            out.push(self.event(idx, JudgeEventKind::Tap(judgement, delta), song_time));
        }
        self.advance_cursor(lane_idx);
        out
    }

    /// Advances time to `song_time`: misses, hold life, mines crossed while
    /// held, mines avoided. The lanes are as the presses and releases left
    /// them (each lane's state is known from its edges: a judged lane under
    /// `judge_all_in` is pressed and released for every lane by the
    /// player).
    pub fn update(&mut self, song_time: f64) -> Vec<JudgeEvent> {
        let mut out = Vec::new();
        self.advance(song_time, &mut out);
        out
    }

    /// Judges everything that happens before `song_time` with no press or
    /// release on the way: since the last time judged, every lane stayed
    /// as it is ([`Judge::down`]). Each rule is measured in song time, at
    /// the instant it happens (a mine's crossing, a window's end, a hold's
    /// life running out), so it makes no difference how often this is
    /// called: StepMania judges these once a frame (`Player::Update`,
    /// `UpdateHoldNotes`, `CrossedRows`), and this is what it would judge
    /// at an infinitely fast frame rate.
    ///
    /// One convention for an instant: what happens at `t` is judged once
    /// judging is past `t`, so a press or release at `t` comes first (as
    /// StepMania handles a frame's input before its update), and every
    /// call judges a span `[last, song_time)`. Each call's events are in
    /// order of time, lane and note, so however the time is cut into
    /// calls, the events come in one order. Time never goes back: an
    /// earlier or non-finite `song_time` judges nothing.
    fn advance(&mut self, song_time: f64, out: &mut Vec<JudgeEvent>) {
        let prev = self.last_time.unwrap_or(f64::NEG_INFINITY);
        if !song_time.is_finite() || song_time < prev {
            return;
        }
        self.last_time = Some(song_time);
        let first = out.len();
        let max_ahead = self.lookahead();
        self.cross_heads(song_time, out);
        self.checkpoints_until(song_time, out);
        for lane in 0..self.lanes.len() {
            self.expire(lane, song_time, out);
            let down = self.down[lane];
            for qi in self.cursor[lane]..self.lanes[lane].len() {
                let idx = self.lanes[lane][qi];
                let n = &self.notes[idx];
                let seconds = n.seconds;
                if seconds - song_time > max_ahead {
                    break;
                }
                match n.state {
                    // A mine whose row is crossed while its lane is down
                    // (`Player::CrossedRows` with the button held): the
                    // lane was down the whole span.
                    NoteState::Pending if n.is_mine() => {
                        if down && prev <= seconds && seconds < song_time {
                            self.step_on_mine(idx, seconds, out);
                        } else if song_time > seconds + self.table.mine_window_seconds() {
                            self.notes[idx].state = NoteState::MineAvoided;
                            if self.notes[idx].note.kind == NoteKind::Shock {
                                self.avoid_shock(idx, out);
                            }
                        }
                    }
                    NoteState::HoldActive { .. } => {
                        // Up since `prev` with no decay running (a hold
                        // whose head was not pressed here): it has decayed
                        // since then.
                        if !n.is_roll() && !down && n.decay_from.is_none() {
                            self.notes[idx].decay_from = Some(prev.max(seconds));
                        }
                        self.settle_hold(idx, song_time, out);
                    }
                    _ => {}
                }
            }
            self.advance_cursor(lane);
        }
        out[first..].sort_by(|a, b| {
            a.song_time
                .total_cmp(&b.song_time)
                .then(a.lane.cmp(&b.lane))
                .then(
                    a.note_index
                        .unwrap_or(usize::MAX)
                        .cmp(&b.note_index.unwrap_or(usize::MAX)),
                )
        });
    }
}

/// The checkpoint rows of a chart's holds and rolls: rows on the tick
/// grid of the tick count in force there (`row % (48 / ticks) == 0`, as
/// StepMania's `Player::UpdateHoldNotes` checks; no grid where it is 0),
/// after a hold's head and up to its tail, where notes are judged.
fn checkpoint_rows(notes: &[JudgedNote], timing: &TimingMap) -> Vec<CheckpointRow> {
    let mut by_tick: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    for (i, n) in notes.iter().enumerate() {
        let Some(end) = n.note.kind.hold_end().filter(|_| n.judgeable) else {
            continue;
        };
        for t in n.note.tick.0 + 1..=end.0 {
            let tick = Tick(t);
            let per_beat = i64::from(timing.ticks_per_beat_at(tick)).min(TICKS_PER_BEAT);
            if per_beat > 0
                && t.rem_euclid(TICKS_PER_BEAT / per_beat) == 0
                && timing.judgeable(tick)
            {
                by_tick.entry(t).or_default().push(i);
            }
        }
    }
    let steps: HashSet<i64> = notes
        .iter()
        .filter(|n| n.judgeable && n.is_press_judged())
        .map(|n| n.note.tick.0)
        .collect();
    by_tick
        .into_iter()
        .map(|(t, holds)| CheckpointRow {
            tick: Tick(t),
            seconds: timing.seconds_at(Tick(t)),
            holds,
            scored: !steps.contains(&t),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::presets;
    use ddi_chart::Tick;

    fn notes() -> Vec<Note> {
        vec![
            Note::new(Tick::from_beats(0), 0, NoteKind::Tap),
            Note::new(Tick::from_beats(1), 1, NoteKind::Tap),
            Note::new(
                Tick::from_beats(2),
                2,
                NoteKind::HoldHead {
                    end: Tick::from_beats(4),
                },
            ),
            Note::new(Tick::from_beats(3), 3, NoteKind::Mine),
            Note::new(Tick::from_beats(5), 0, NoteKind::Lift),
            Note::new(Tick::from_beats(6), 1, NoteKind::Fake),
        ]
    }

    fn judge() -> Judge {
        // 60 BPM: beat n at n seconds.
        Judge::new(
            &notes(),
            &TimingMap::constant(60.0, 0.0),
            presets::sm5().judge,
        )
    }

    #[test]
    fn counts_and_rows() {
        let j = judge();
        assert_eq!(j.judged_note_count(), 4); // tap, tap, hold head, lift
        assert_eq!(j.hold_count(), 1);
        assert_eq!(j.mine_count(), 1);
        assert_eq!(j.row_count(), 4);
        assert_eq!(j.notes()[5].state, NoteState::Faked);
        assert_eq!(j.lane_count(), 4);
    }

    #[test]
    fn earliest_note_in_window_is_taken() {
        let two = vec![
            Note::new(Tick::from_beat_f64(0.0), 0, NoteKind::Tap),
            Note::new(Tick::from_beat_f64(0.1), 0, NoteKind::Tap),
        ];
        let mut j = Judge::new(&two, &TimingMap::constant(60.0, 0.0), presets::sm5().judge);
        // 0.08 s: closer to the second note (at 0.104) but the first is pending.
        let ev = j.press(0, 0.08);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].note_index, Some(0));
        let ev = j.press(0, 0.11);
        assert_eq!(ev[0].note_index, Some(1));
    }

    #[test]
    fn miss_after_window_and_lift_on_release() {
        let mut j = judge();
        let ev = j.update(1.0 + 0.18 + 0.001);
        let misses: Vec<_> = ev
            .iter()
            .filter(|e| matches!(e.kind, JudgeEventKind::Tap(Judgement::Miss, _)))
            .collect();
        assert_eq!(misses.len(), 2);
        assert_eq!(misses[0].note_index, Some(0));
        assert_eq!(misses[1].note_index, Some(1));
        // A press does not judge a lift (the hold of lane 2, never
        // pressed, is missed on the way).
        assert!(j.press(0, 5.01).iter().all(|e| e.lane != 0));
        let ev = j.release(0, 5.01);
        assert!(matches!(ev[0].kind, JudgeEventKind::Tap(Judgement::W1, _)));
    }

    #[test]
    fn hold_let_go_and_held() {
        let mut j = judge();
        j.update(1.9);
        let ev = j.press(2, 2.0);
        assert!(matches!(ev[0].kind, JudgeEventKind::Tap(Judgement::W1, _)));
        j.release(2, 2.5);
        let ev = j.update(2.7);
        assert!(ev.is_empty());
        assert!(
            matches!(j.notes()[2].state, NoteState::HoldActive { life } if (life - 0.2).abs() < 1e-5)
        );
        let ev = j.update(2.8);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].kind, JudgeEventKind::LetGo);
        assert!((ev[0].song_time - 2.75).abs() < 1e-9);

        let mut j = judge();
        j.update(1.9);
        j.press(2, 2.0);
        j.release(2, 3.0);
        j.press(2, 3.2);
        // Judged once past its tail (4.0).
        assert!(j.update(4.0).is_empty());
        let ev = j.update(4.01);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].kind, JudgeEventKind::Held);
        assert_eq!(ev[0].song_time, 4.0);
    }

    /// 60 BPM (beat n at n seconds) under StepMania 5's windows with
    /// ticked holds: 4 checkpoints a beat, 0.1664 s to come back.
    fn ticked(notes: &[Note], timing: TimingMap) -> Judge {
        let mut table = presets::sm5().judge;
        table.hold = HoldRules::Ticked {
            checkpoint_window: HoldRules::CHECKPOINT_WINDOW,
        };
        Judge::new(notes, &timing, table)
    }

    fn hold(beat: f64, lane: u8, end: f64) -> Note {
        Note::new(
            Tick::from_beat_f64(beat),
            lane,
            NoteKind::HoldHead {
                end: Tick::from_beat_f64(end),
            },
        )
    }

    /// `(seconds, held, missed)` of each checkpoint event, and the holds'
    /// outcomes.
    fn ticks(ev: &[JudgeEvent]) -> Vec<(f64, u16, u16)> {
        ev.iter()
            .filter_map(|e| match e.kind {
                JudgeEventKind::Checkpoint { held, missed, .. } => {
                    Some(((e.song_time * 100.0).round() / 100.0, held, missed))
                }
                _ => None,
            })
            .collect()
    }

    fn outcomes(ev: &[JudgeEvent]) -> Vec<JudgeEventKind> {
        ev.iter()
            .filter(|e| matches!(e.kind, JudgeEventKind::Held | JudgeEventKind::LetGo))
            .map(|e| e.kind)
            .collect()
    }

    #[test]
    fn a_held_ticked_hold_hits_every_checkpoint_after_its_head() {
        let mut j = ticked(&[hold(0.0, 0, 1.0)], TimingMap::constant(60.0, 0.0));
        assert_eq!(j.checkpoint_count(), 4);
        let mut ev = j.press(0, 0.0);
        ev.extend(j.update(1.5));
        // Not the head's row; up to and including the tail's.
        assert_eq!(
            ticks(&ev),
            [(0.25, 1, 0), (0.5, 1, 0), (0.75, 1, 0), (1.0, 1, 0)]
        );
        assert_eq!(outcomes(&ev), [JudgeEventKind::Held]);
        // The tail's checkpoint before the hold's outcome.
        assert!(matches!(ev.last().unwrap().kind, JudgeEventKind::Held));
    }

    #[test]
    fn a_released_ticked_hold_misses_checkpoints_and_comes_back() {
        let mut j = ticked(&[hold(0.0, 0, 1.0)], TimingMap::constant(60.0, 0.0));
        let mut ev = j.press(0, 0.0);
        ev.extend(j.release(0, 0.3));
        // 0.2 s released at 0.5: over the 0.1664 s window.
        ev.extend(j.update(0.55));
        // Never dropped on the way: pressed again, it goes on.
        assert!(matches!(j.notes()[0].state, NoteState::HoldActive { .. }));
        ev.extend(j.press(0, 0.6));
        ev.extend(j.update(1.2));
        assert_eq!(
            ticks(&ev),
            [(0.25, 1, 0), (0.5, 0, 1), (0.75, 1, 0), (1.0, 1, 0)]
        );
        assert_eq!(outcomes(&ev), [JudgeEventKind::LetGo]);
        // A short release (0.1 s) is forgiven.
        let mut j = ticked(&[hold(0.0, 0, 1.0)], TimingMap::constant(60.0, 0.0));
        let mut ev = j.press(0, 0.0);
        ev.extend(j.release(0, 0.4));
        ev.extend(j.update(0.5));
        ev.extend(j.press(0, 0.52));
        ev.extend(j.update(1.2));
        assert!(ticks(&ev).iter().all(|t| t.2 == 0));
        assert_eq!(outcomes(&ev), [JudgeEventKind::Held]);
    }

    #[test]
    fn a_ticked_hold_starts_without_its_head() {
        // Pressed half a second early (no window) and held through: the
        // head is stepped on as it passes, a W1 (StepMania's
        // `CrossedRows`).
        let mut j = ticked(&[hold(1.0, 0, 2.0)], TimingMap::constant(60.0, 0.0));
        let mut ev = j.press(0, 0.5);
        assert!(ev.is_empty());
        for t in [1.0, 1.1, 1.2, 1.3, 1.6, 2.1] {
            ev.extend(j.update(t));
        }
        let heads: Vec<_> = ev
            .iter()
            .filter_map(|e| match e.kind {
                JudgeEventKind::Tap(j, _) => Some((j, e.song_time)),
                _ => None,
            })
            .collect();
        assert_eq!(heads, [(Judgement::W1, 1.0)]);
        assert_eq!(ticks(&ev).len(), 4);
        assert!(ticks(&ev).iter().all(|t| t.2 == 0));
        assert_eq!(outcomes(&ev), [JudgeEventKind::Held]);
        // Never pressed at all: a miss, its checkpoints missed.
        let mut j = ticked(&[hold(1.0, 0, 2.0)], TimingMap::constant(60.0, 0.0));
        let ev = j.update(2.5);
        assert!(
            ev.iter()
                .any(|e| matches!(e.kind, JudgeEventKind::Tap(Judgement::Miss, _)))
        );
        assert_eq!(
            ticks(&ev),
            [(1.25, 0, 1), (1.5, 0, 1), (1.75, 0, 1), (2.0, 0, 1)]
        );
        assert_eq!(outcomes(&ev), [JudgeEventKind::LetGo]);
    }

    #[test]
    fn a_ticked_head_stepped_on_late_is_a_w1_with_little_life_before() {
        // 180 BPM: a beat is 1/3 s, a checkpoint every 1/12 s after the
        // head at 1/3 s.
        let mut j = ticked(&[hold(1.0, 0, 2.0)], TimingMap::constant(180.0, 0.0));
        let head = 1.0 / 3.0;
        let mut ev = j.update(head + 0.1);
        // Stepped on 120 ms late: still a W1, with its real offset.
        ev.extend(j.press(0, head + 0.12));
        let tap = ev
            .iter()
            .find_map(|e| match e.kind {
                JudgeEventKind::Tap(j, d) => Some((j, d)),
                _ => None,
            })
            .unwrap();
        assert_eq!(tap.0, Judgement::W1);
        assert!((tap.1 - 0.12).abs() < 1e-9);
        // Up as the head passed, the hold had `InitialHoldLife` (0.05 of
        // the window, 8 ms): the checkpoint 83 ms in was missed.
        ev.extend(j.update(1.0));
        let t = ticks(&ev);
        assert_eq!((t[0].1, t[0].2), (0, 1));
        assert!(t[1..].iter().all(|t| t.2 == 0));
        // Early within the windows: a W1 as well.
        let mut j = ticked(&[hold(1.0, 0, 2.0)], TimingMap::constant(60.0, 0.0));
        let ev = j.press(0, 0.9);
        assert!(matches!(ev[0].kind, JudgeEventKind::Tap(Judgement::W1, _)));
    }

    #[test]
    fn a_ticked_holds_tail_row_does_not_count_for_its_outcome() {
        // Let go 0.2 s before its tail: the tail's checkpoint is missed
        // (and scored), the hold is still kept (StepMania settles it before
        // crossing that row).
        let mut j = ticked(&[hold(0.0, 0, 1.0)], TimingMap::constant(60.0, 0.0));
        let mut ev = j.press(0, 0.0);
        ev.extend(j.update(0.79));
        ev.extend(j.release(0, 0.8));
        ev.extend(j.update(1.5));
        assert_eq!(ticks(&ev).last(), Some(&(1.0, 0, 1)));
        assert_eq!(outcomes(&ev), [JudgeEventKind::Held]);
    }

    #[test]
    fn events_a_press_finds_on_the_way_come_in_time_order() {
        // A hold kept in lane 0; a tap in lane 2 left unjudged. A late press
        // in lane 2 finds the hold's checkpoints and outcome and the tap's
        // miss (at 0.48), which falls between them.
        let notes = [
            hold(0.0, 0, 1.0),
            Note::new(Tick::from_beat_f64(0.3), 2, NoteKind::Tap),
        ];
        let mut j = ticked(&notes, TimingMap::constant(60.0, 0.0));
        j.press(0, 0.0);
        let ev = j.press(2, 1.05);
        assert_eq!(ev.len(), 6);
        assert_eq!(ev[5].kind, JudgeEventKind::Held);
        assert!(ev.is_sorted_by(|a, b| a.song_time <= b.song_time));
        assert!(matches!(
            ev[1].kind,
            JudgeEventKind::Tap(Judgement::Miss, _)
        ));
    }

    #[test]
    fn starting_part_way_keeps_the_rows_still_to_come() {
        let notes = [hold(0.0, 0, 1.0), hold(2.0, 1, 3.0)];
        let mut j = ticked(&notes, TimingMap::constant(60.0, 0.0));
        assert_eq!(j.checkpoint_count(), 8);
        j.skip_before(1.5);
        assert_eq!(j.checkpoint_count(), 4);
        let mut ev = j.press(1, 2.0);
        ev.extend(j.update(3.5));
        assert_eq!(ticks(&ev).len(), 4);
        assert_eq!(outcomes(&ev), [JudgeEventKind::Held]);
    }

    #[test]
    fn checkpoint_rows_are_judged_once_and_not_where_a_step_is() {
        let notes = [
            hold(0.0, 0, 1.0),
            hold(0.0, 1, 1.0),
            Note::new(Tick::from_beat_f64(0.5), 2, NoteKind::Tap),
        ];
        let mut j = ticked(&notes, TimingMap::constant(60.0, 0.0));
        // The row at 0.5 has a tap: three rows judged.
        assert_eq!(j.checkpoint_count(), 3);
        let mut ev = j.press(0, 0.0);
        ev.extend(j.press(1, 0.0));
        ev.extend(j.press(2, 0.5));
        ev.extend(j.release(1, 0.55));
        ev.extend(j.update(1.5));
        // Two holds a row; lane 1 let go at 0.55 misses 0.75 and 1.0.
        assert_eq!(ticks(&ev), [(0.25, 2, 0), (0.75, 1, 1), (1.0, 1, 1)]);
        let lanes: Vec<u64> = ev
            .iter()
            .filter_map(|e| match e.kind {
                JudgeEventKind::Checkpoint { lanes, .. } => Some(lanes),
                _ => None,
            })
            .collect();
        assert_eq!(lanes, [0b11, 0b11, 0b11]);
        // Lane 0 kept the unjudged row at 0.5 too.
        assert_eq!(outcomes(&ev), [JudgeEventKind::Held, JudgeEventKind::LetGo]);
    }

    #[test]
    fn tick_counts_set_the_grid_and_zero_has_none() {
        let mut timing = TimingMap::constant(60.0, 0.0);
        timing.tick_counts = vec![
            ddi_chart::TickCount {
                tick: Tick::ZERO,
                ticks_per_beat: 2,
            },
            ddi_chart::TickCount {
                tick: Tick::from_beats(2),
                ticks_per_beat: 0,
            },
        ];
        let notes = [hold(0.0, 0, 1.0), hold(2.0, 1, 3.0), hold(4.5, 0, 4.6)];
        let mut j = ticked(&notes, timing);
        assert_eq!(j.checkpoint_count(), 2);
        let mut ev = j.press(0, 0.0);
        ev.extend(j.update(1.5));
        ev.extend(j.release(0, 1.5));
        assert_eq!(ticks(&ev), [(0.5, 1, 0), (1.0, 1, 0)]);
        // Without checkpoints: kept when its head was stepped on.
        ev.extend(j.press(1, 2.0));
        ev.extend(j.release(1, 2.1));
        ev.extend(j.update(4.0));
        assert_eq!(outcomes(&ev), [JudgeEventKind::Held, JudgeEventKind::Held]);
        let ev = j.update(5.0);
        assert_eq!(outcomes(&ev), [JudgeEventKind::LetGo]);
    }

    #[test]
    fn a_ticked_roll_lasts_while_tapped() {
        let roll = Note::new(
            Tick::ZERO,
            0,
            NoteKind::RollHead {
                end: Tick::from_beats(2),
            },
        );
        let mut j = ticked(std::slice::from_ref(&roll), TimingMap::constant(60.0, 0.0));
        let mut ev = j.press(0, 0.0);
        for t in [0.4, 0.8, 1.2, 1.6] {
            ev.extend(j.update(t));
            ev.extend(j.press(0, t));
        }
        ev.extend(j.update(2.5));
        assert_eq!(ticks(&ev).len(), 8);
        assert_eq!(outcomes(&ev), [JudgeEventKind::Held]);
        // Not tapped past the roll window (0.5 s): its life ran out, but
        // it is only judged at its tail.
        let mut j = ticked(&[roll], TimingMap::constant(60.0, 0.0));
        let mut ev = j.press(0, 0.0);
        ev.extend(j.update(1.0));
        assert!(outcomes(&ev).is_empty());
        ev.extend(j.press(0, 1.0));
        ev.extend(j.update(2.5));
        assert_eq!(outcomes(&ev), [JudgeEventKind::LetGo]);
    }

    #[test]
    fn mines_on_press_and_while_held() {
        let mut j = judge();
        j.update(2.9);
        let ev = j.press(3, 3.05);
        assert_eq!(ev[0].kind, JudgeEventKind::HitMine);

        // Down since before the mine's window, across its row.
        let mut j = judge();
        j.update(2.9);
        assert!(j.press(3, 2.9).is_empty());
        let ev = j.update(3.1);
        assert_eq!(ev[0].kind, JudgeEventKind::HitMine);
        assert_eq!(ev[0].song_time, 3.0);

        let mut j = judge();
        j.update(2.9);
        let ev = j.update(3.2);
        assert!(ev.iter().all(|e| e.kind != JudgeEventKind::HitMine));
        assert_eq!(j.notes()[3].state, NoteState::MineAvoided);
    }

    #[test]
    fn a_press_after_a_mine_passed_does_not_step_on_it() {
        // A mine just before a tap (a tick, 21 ms at 60 BPM), as charts
        // put them: pressed for the tap, the frame that sees the mine
        // finds the lane down, but it went down after the mine had passed.
        let notes = [
            Note::new(Tick::from_beats(1), 0, NoteKind::Mine),
            Note::new(Tick(TICKS_PER_BEAT + 1), 0, NoteKind::Tap),
        ];
        let tap = notes[1].tick.beat();
        let table = presets::itg().judge;
        let mut j = Judge::new(&notes, &TimingMap::constant(60.0, 0.0), table.clone());
        let mut ev = j.update(0.99);
        ev.extend(j.press(0, tap));
        ev.extend(j.update(1.03));
        assert!(ev.iter().all(|e| e.kind != JudgeEventKind::HitMine));
        assert!(matches!(ev[0].kind, JudgeEventKind::Tap(Judgement::W1, _)));
        // Down before it: stepped on.
        let mut j = Judge::new(&notes, &TimingMap::constant(60.0, 0.0), table.clone());
        let mut ev = j.update(0.9);
        ev.extend(j.press(0, 0.95));
        ev.extend(j.update(1.03));
        assert!(ev.iter().any(|e| e.kind == JudgeEventKind::HitMine));
        // Down since 0.5 and a second key on the lane for the tap: still
        // down when the mine passed.
        let mut j = Judge::new(&notes, &TimingMap::constant(60.0, 0.0), table.clone());
        let mut ev = j.press(0, 0.5);
        ev.extend(j.update(0.99));
        ev.extend(j.press(0, tap));
        ev.extend(j.update(1.03));
        assert!(ev.iter().any(|e| e.kind == JudgeEventKind::HitMine));
        // Down over the mine, up and down again in the same frame: the
        // lane was down when it passed.
        let mut j = Judge::new(&notes, &TimingMap::constant(60.0, 0.0), table);
        let mut ev = j.press(0, 0.5);
        ev.extend(j.update(0.99));
        ev.extend(j.release(0, 1.005));
        ev.extend(j.press(0, 1.019));
        ev.extend(j.update(1.03));
        assert!(ev.iter().any(|e| e.kind == JudgeEventKind::HitMine));
    }

    #[test]
    fn empty_press_policies() {
        let mut table = presets::sm5().judge;
        table.empty_press = EmptyPress::Boo;
        let mut j = Judge::new(&notes(), &TimingMap::constant(60.0, 0.0), table.clone());
        // The tap of lane 0 missed on the way, then the press's boo.
        let ev = j.press(2, 0.5);
        assert_eq!(ev[1].kind, JudgeEventKind::Boo);
        table.empty_press = EmptyPress::ExcessiveEarly {
            lo: 0.2,
            hi: 0.4,
            factor: 0.25,
        };
        let mut j = Judge::new(&notes(), &TimingMap::constant(60.0, 0.0), table);
        let boo = |ev: &[JudgeEvent]| ev.iter().any(|e| e.kind == JudgeEventKind::Boo);
        assert!(!boo(&j.press(1, 0.5)));
        assert!(boo(&j.press(1, 0.7)));
    }

    #[test]
    fn jump_needs_two_presses_and_a_stray_press_is_ignored() {
        let jump = vec![
            Note::new(Tick::from_beats(1), 0, NoteKind::Tap),
            Note::new(Tick::from_beats(1), 3, NoteKind::Tap),
        ];
        let mut j = Judge::new(&jump, &TimingMap::constant(60.0, 0.0), presets::sm5().judge);
        assert_eq!(j.row_count(), 1);
        assert_eq!(j.rows()[0].size, 2);
        let ev = j.press(0, 1.0);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].note_index, Some(0));
        // Pressing the same lane again hits nothing (SM/DDR ignore it).
        assert!(j.press(0, 1.01).is_empty());
        assert_eq!(j.notes()[1].state, NoteState::Pending);
        let ev = j.press(3, 1.02);
        assert_eq!(ev[0].note_index, Some(1));
        assert!(j.all_resolved());
    }

    #[test]
    fn miss_happens_strictly_after_the_late_bound() {
        let miss_after = presets::sm5().judge.miss_after(); // 0.180
        let mut j = judge();
        // Exactly on the bound: still pending, and a press there is a W5.
        assert!(j.update(miss_after).is_empty());
        assert_eq!(j.notes()[0].state, NoteState::Pending);
        let ev = j.press(0, miss_after);
        assert!(matches!(ev[0].kind, JudgeEventKind::Tap(Judgement::W5, _)));

        let mut j = judge();
        let ev = j.update(miss_after + 1e-9);
        assert_eq!(ev.len(), 1);
        assert!(matches!(
            ev[0].kind,
            JudgeEventKind::Tap(Judgement::Miss, _)
        ));
        assert!((ev[0].song_time - miss_after).abs() < 1e-9);
    }

    #[test]
    fn roll_decays_while_held_and_resets_on_each_press() {
        let roll = vec![Note::new(
            Tick::from_beats(2),
            0,
            NoteKind::RollHead {
                end: Tick::from_beats(4),
            },
        )];
        let table = presets::sm5().judge; // roll window 0.5 s
        let mut j = Judge::new(&roll, &TimingMap::constant(60.0, 0.0), table.clone());
        j.press(0, 2.0);
        // Holding the button does not keep a roll alive.
        j.update(2.4);
        assert!(
            matches!(j.notes()[0].state, NoteState::HoldActive { life } if (life - 0.2).abs() < 1e-5)
        );
        let ev = j.update(2.6);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].kind, JudgeEventKind::LetGo);
        assert!((ev[0].song_time - 2.5).abs() < 1e-9);

        // Re-tapping within the window resets life; the tail then passes → Held.
        let mut j = Judge::new(&roll, &TimingMap::constant(60.0, 0.0), table);
        j.press(0, 2.0);
        for t in [2.4, 2.8, 3.2, 3.6] {
            j.release(0, t - 0.05);
            assert!(j.press(0, t).is_empty());
            assert!(matches!(j.notes()[0].state, NoteState::HoldActive { life } if life == 1.0));
        }
        let ev = j.update(4.01);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].kind, JudgeEventKind::Held);
    }

    #[test]
    fn missed_hold_head_is_never_held() {
        let mut j = judge(); // hold at beat 2 → 4 in lane 2
        j.update(1.9);
        let ev = j.update(2.3);
        assert!(ev.iter().any(|e| e.note_index == Some(2)
            && matches!(e.kind, JudgeEventKind::Tap(Judgement::Miss, _))));
        assert_eq!(j.notes()[2].state, NoteState::Missed);
        // Pressing and holding through the tail changes nothing.
        j.press(2, 2.35);
        let ev = j.update(4.5);
        assert!(ev.iter().all(|e| e.note_index != Some(2)));
        assert_eq!(j.notes()[2].state, NoteState::Missed);
    }

    #[test]
    fn warped_and_fake_segment_notes_are_neither_judged_nor_counted() {
        use ddi_chart::{FakeSegment, WarpSegment};
        let mut t = TimingMap::constant(60.0, 0.0);
        t.warps.push(WarpSegment {
            tick: Tick::from_beats(1),
            length: Tick::from_beats(1),
        });
        t.fakes.push(FakeSegment {
            tick: Tick::from_beats(3),
            length: Tick::from_beats(1),
        });
        t.tidy();
        let notes = vec![
            Note::new(Tick::from_beats(0), 0, NoteKind::Tap),
            Note::new(Tick::from_beats(1), 1, NoteKind::Tap), // warped
            Note::new(Tick::from_beats(2), 2, NoteKind::Tap),
            Note::new(Tick::from_beats(3), 3, NoteKind::Mine), // fake segment
            Note::new(Tick::from_beats(4), 0, NoteKind::Tap),
        ];
        let mut j = Judge::new(&notes, &t, presets::sm5().judge);
        assert_eq!(j.judged_note_count(), 3);
        assert_eq!(j.row_count(), 3);
        assert_eq!(j.mine_count(), 0);
        assert_eq!(j.notes()[1].state, NoteState::Faked);
        assert_eq!(j.notes()[3].state, NoteState::Faked);
        let ev = j.update(10.0);
        assert_eq!(ev.len(), 3);
        assert!(
            ev.iter()
                .all(|e| e.note_index != Some(1) && e.note_index != Some(3))
        );
        assert!(j.all_resolved());
    }

    #[test]
    fn update_reports_events_in_time_order_across_lanes() {
        let notes = vec![
            Note::new(Tick::from_beats(2), 0, NoteKind::Tap),
            Note::new(Tick::from_beats(1), 1, NoteKind::Tap),
            Note::new(Tick::from_beats(3), 2, NoteKind::Tap),
        ];
        let mut j = Judge::new(
            &notes,
            &TimingMap::constant(60.0, 0.0),
            presets::sm5().judge,
        );
        let ev = j.update(10.0);
        let order: Vec<_> = ev.iter().map(|e| e.note_index).collect();
        assert_eq!(order, vec![Some(1), Some(0), Some(2)]);
    }

    fn danoni_table(judged: bool) -> JudgeTable {
        let mut t = presets::danoni().judge;
        t.hold_start.judged = judged;
        t
    }

    const F: f64 = crate::rules::danoni::FRAME;

    fn one_hold() -> Vec<Note> {
        vec![Note::new(
            Tick::from_beats(2),
            0,
            NoteKind::HoldHead {
                end: Tick::from_beats(4),
            },
        )]
    }

    #[test]
    fn release_budget_counts_released_time_in_all() {
        let timing = TimingMap::constant(60.0, 0.0);
        let mut j = Judge::new(&one_hold(), &timing, danoni_table(false));
        // An unjudged start: no tap event, the hold is active.
        assert!(j.press(0, 2.0).is_empty());
        assert!(matches!(j.notes()[0].state, NoteState::HoldActive { .. }));
        // Three frames released, pressed again: two frames left.
        j.release(0, 2.5);
        assert!(j.press(0, 2.5 + 3.0 * F).is_empty());
        assert!(
            matches!(j.notes()[0].state, NoteState::HoldActive { life } if (life - 0.4).abs() < 1e-4)
        );
        // Pressing again never refills it: the next release drops it after
        // two more frames.
        j.release(0, 3.0);
        assert!(j.update(3.0 + 1.5 * F).is_empty());
        let ev = j.update(3.1);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].kind, JudgeEventKind::LetGo);
        assert!((ev[0].song_time - (3.0 + 2.0 * F)).abs() < 1e-9);

        // Short releases within the budget keep it to the end.
        let mut j = Judge::new(&one_hold(), &timing, danoni_table(false));
        j.press(0, 2.0);
        j.release(0, 2.5);
        j.press(0, 2.5 + 2.0 * F);
        j.release(0, 3.5);
        j.press(0, 3.5 + 2.0 * F);
        let ev = j.update(4.01);
        assert_eq!(ev[0].kind, JudgeEventKind::Held);
    }

    #[test]
    fn hold_starts_only_within_the_acceptance_window() {
        let timing = TimingMap::constant(60.0, 0.0);
        // 4.5 frames late is still accepted (bound: 5 frames late).
        let mut j = Judge::new(&one_hold(), &timing, danoni_table(false));
        assert!(j.press(0, 2.0 + 4.5 * F).is_empty());
        assert!(matches!(j.notes()[0].state, NoteState::HoldActive { .. }));
        // 6 frames late is a dropped hold at once; judged starts also get
        // their tap judgement.
        let mut j = Judge::new(&one_hold(), &timing, danoni_table(false));
        let ev = j.press(0, 2.0 + 6.0 * F);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].kind, JudgeEventKind::LetGo);
        let mut j = Judge::new(&one_hold(), &timing, danoni_table(true));
        let ev = j.press(0, 2.0 + 6.0 * F);
        assert!(matches!(ev[0].kind, JudgeEventKind::Tap(Judgement::W3, _)));
        assert_eq!(ev[1].kind, JudgeEventKind::LetGo);
        // A start never pressed: N.G. (and a miss when starts are judged).
        let mut j = Judge::new(&one_hold(), &timing, danoni_table(false));
        let ev = j.update(2.5);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].kind, JudgeEventKind::LetGo);
        assert_eq!(j.judged_note_count(), 0);
        let mut j = Judge::new(&one_hold(), &timing, danoni_table(true));
        let ev = j.update(2.5);
        assert_eq!(ev.len(), 2);
        assert!(matches!(
            ev[0].kind,
            JudgeEventKind::Tap(Judgement::Miss, _)
        ));
        assert_eq!(j.judged_note_count(), 1);
    }

    #[test]
    fn a_late_note_gives_way_to_the_next() {
        // Six frames apart in one lane.
        let notes = vec![
            Note::new(Tick::from_beat_f64(1.0), 0, NoteKind::Tap),
            Note::new(Tick::from_beat_f64(1.1), 0, NoteKind::Tap),
        ];
        let timing = TimingMap::constant(60.0, 0.0);
        let first = timing.seconds_at(notes[0].tick);
        let second = timing.seconds_at(notes[1].tick);
        // The first is superseded once the second is 4 frames away and the
        // first at least 3 frames late; a press then hits the second.
        let mut j = Judge::new(&notes, &timing, danoni_table(false));
        let ev = j.press(0, first + 3.5 * F);
        assert_eq!(ev.len(), 2);
        assert!(matches!(
            ev[0].kind,
            JudgeEventKind::Tap(Judgement::Miss, _)
        ));
        assert!((ev[0].song_time - (second - 4.0 * F).max(first + 3.0 * F)).abs() < 1e-9);
        assert_eq!(ev[1].note_index, Some(1));
        // Within Ii of the first, the first is still taken.
        let mut j = Judge::new(&notes, &timing, danoni_table(false));
        let ev = j.press(0, first + 2.5 * F);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].note_index, Some(0));
        // A hold never takes over from an earlier tap.
        let tap_then_hold = vec![
            Note::new(Tick::from_beat_f64(1.0), 0, NoteKind::Tap),
            Note::new(
                Tick::from_beat_f64(1.1),
                0,
                NoteKind::HoldHead {
                    end: Tick::from_beats(3),
                },
            ),
        ];
        let mut j = Judge::new(&tap_then_hold, &timing, danoni_table(false));
        let ev = j.press(0, first + 3.5 * F);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].note_index, Some(0));
        assert!(matches!(ev[0].kind, JudgeEventKind::Tap(Judgement::W2, _)));
        // StepMania keeps the first until its window ends.
        let mut j = Judge::new(&notes, &timing, presets::sm5().judge);
        let ev = j.press(0, first + 3.5 * F);
        assert_eq!(ev[0].note_index, Some(0));
    }

    #[test]
    fn skip_before_fakes_old_notes() {
        let mut j = judge();
        j.skip_before(2.5);
        assert_eq!(j.notes()[0].state, NoteState::Faked);
        assert_eq!(j.notes()[2].state, NoteState::Pending); // hold ends at 4
        assert_eq!(j.judged_note_count(), 2);
        assert_eq!(j.row_count(), 2);
    }
}
