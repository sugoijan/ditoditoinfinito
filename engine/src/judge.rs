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
    /// `docs/plans/pump.md`). `None`: it weighs nothing.
    pub fn scored_as(self, ticked: bool) -> Option<JudgeEventKind> {
        match self {
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
    /// Each lane's button, as presses and releases left it.
    down: Vec<bool>,
}

impl Judge {
    /// Precomputes seconds and judgeability for `notes` (sorted by tick).
    pub fn new(notes: &[Note], timing: &TimingMap, table: JudgeTable) -> Judge {
        let notes: Vec<JudgedNote> = notes
            .iter()
            .map(|n| {
                let seconds = timing.seconds_at(n.tick);
                let end_seconds = match n.kind {
                    NoteKind::HoldHead { end } | NoteKind::RollHead { end } => {
                        timing.seconds_at(end).max(seconds)
                    }
                    _ => seconds,
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
        let (checkpoints, heads) = if table.hold.is_ticked() {
            let mut heads: Vec<usize> = (0..notes.len())
                .filter(|&i| {
                    notes[i].judgeable
                        && matches!(
                            notes[i].note.kind,
                            NoteKind::HoldHead { .. } | NoteKind::RollHead { .. }
                        )
                })
                .collect();
            heads.sort_by(|&a, &b| notes[a].seconds.total_cmp(&notes[b].seconds));
            (checkpoint_rows(&notes, timing), heads)
        } else {
            (Vec::new(), Vec::new())
        };
        let mut judge = Judge {
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

    pub fn table(&self) -> &JudgeTable {
        &self.table
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
            .filter(|n| {
                n.judgeable
                    && matches!(
                        n.note.kind,
                        NoteKind::HoldHead { .. } | NoteKind::RollHead { .. }
                    )
            })
            .count() as u32
    }

    /// Judgeable mines and shocks.
    pub fn mine_count(&self) -> u32 {
        self.notes
            .iter()
            .filter(|n| n.judgeable && n.is_mine())
            .count() as u32
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

    /// The ticked heads whose time passed by `song_time` (StepMania's
    /// `CrossedRows`): a head whose panel is down is stepped on, a `W1`;
    /// otherwise its hold starts anyway, with [`INITIAL_HOLD_LIFE`].
    fn cross_heads(&mut self, song_time: f64, out: &mut Vec<JudgeEvent>) {
        while let Some(&idx) = self.heads.get(self.next_head) {
            let n = &self.notes[idx];
            if n.seconds > song_time {
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
            if row.seconds > song_time {
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

    /// When a hold whose life ran out was dropped.
    fn drop_time(&self, idx: usize, song_time: f64) -> f64 {
        let n = &self.notes[idx];
        let from = n.decay_from.unwrap_or(song_time);
        let at = match (self.table.hold, n.is_roll()) {
            (HoldRules::ReleaseBudget { budget }, false) => from + (budget - n.released).max(0.0),
            _ => from + self.hold_window(idx),
        };
        at.min(song_time)
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
        let body = matches!(
            self.notes[idx].note.kind,
            NoteKind::HoldHead { .. } | NoteKind::RollHead { .. }
        );
        if body && self.ticked() {
            out.push(self.event(
                idx,
                JudgeEventKind::Tap(Judgement::Miss, at - self.notes[idx].seconds),
                at,
            ));
            // Its life runs from its head ([`Judge::cross_heads`]).
            let life = self.hold_life(idx, at);
            self.notes[idx].state = NoteState::HoldActive { life };
        } else if is_hold && start.accept.is_some() {
            if start.judged {
                out.push(self.event(
                    idx,
                    JudgeEventKind::Tap(Judgement::Miss, at - self.notes[idx].seconds),
                    at,
                ));
            }
            self.notes[idx].state = NoteState::LetGo;
            out.push(self.event(idx, JudgeEventKind::LetGo, at));
        } else {
            self.notes[idx].state = NoteState::Missed;
            out.push(self.event(
                idx,
                JudgeEventKind::Tap(Judgement::Miss, at - self.notes[idx].seconds),
                at,
            ));
        }
    }

    /// Resolves an active hold at `song_time`: `LetGo` when its life ran
    /// out, `Held` once the tail passed, otherwise just refreshes `life`.
    /// A ticked hold is only resolved at its tail: kept when it missed no
    /// checkpoint and hit one (without checkpoints: when its head was
    /// stepped on), a ticked roll when its life lasted.
    fn settle_hold(&mut self, idx: usize, song_time: f64, out: &mut Vec<JudgeEvent>) {
        let life = self.hold_life(idx, song_time);
        if self.ticked() {
            let n = &self.notes[idx];
            if song_time < n.end_seconds {
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
        if life <= 0.0 {
            let at = self.drop_time(idx, song_time);
            self.notes[idx].state = NoteState::LetGo;
            out.push(self.event(idx, JudgeEventKind::LetGo, at));
        } else if song_time >= self.notes[idx].end_seconds {
            self.notes[idx].state = NoteState::Held;
            out.push(self.event(idx, JudgeEventKind::Held, self.notes[idx].end_seconds));
        } else {
            self.notes[idx].state = NoteState::HoldActive { life };
        }
    }

    /// Misses the tap-like notes of `lane` that passed unjudged by
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
                (song_time >= at && at - n.seconds <= miss_after).then_some(at)
            });
            if let Some(at) = superseded {
                missed.push((idx, at));
            } else if song_time - n.seconds > miss_after {
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
        self.cross_heads(song_time, &mut out);
        self.checkpoints_until(song_time, &mut out);
        self.down[lane_idx] = true;
        self.expire(lane_idx, song_time, &mut out);
        let queue: Vec<usize> = self.lanes[lane_idx][self.cursor[lane_idx]..].to_vec();

        // Active holds get their life back; active rolls restart their decay.
        for &idx in &queue {
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

        let max_early = self.table.max_early().max(self.table.mine_window_seconds());
        let mut tap: Option<(usize, f64, Judgement)> = None;
        let mut mine: Option<(usize, f64)> = None;
        for &idx in &queue {
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
            self.notes[idx].state = NoteState::MineHit;
            out.push(self.event(idx, JudgeEventKind::HitMine, song_time));
        } else if let Some((idx, delta, _)) = tap.filter(|&(idx, ..)| {
            self.ticked()
                && matches!(
                    self.notes[idx].note.kind,
                    NoteKind::HoldHead { .. } | NoteKind::RollHead { .. }
                )
        }) {
            self.step_on_ticked_head(idx, delta, song_time, &mut out);
        } else if let Some((idx, delta, judgement)) = tap {
            let is_hold = matches!(
                self.notes[idx].note.kind,
                NoteKind::HoldHead { .. } | NoteKind::RollHead { .. }
            );
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
                EmptyPress::ExcessiveEarly { lo, hi, .. } => queue.iter().any(|&idx| {
                    let n = &self.notes[idx];
                    let early = n.seconds - song_time;
                    n.state == NoteState::Pending
                        && n.is_press_judged()
                        && early >= lo
                        && early <= hi
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
        // Checkpoints and misses found on the way are earlier than the
        // press: combo and gauge see events in the order they happened.
        out.sort_by(|a, b| a.song_time.total_cmp(&b.song_time));
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
        self.cross_heads(song_time, &mut out);
        self.checkpoints_until(song_time, &mut out);
        self.down[lane_idx] = false;
        let queue: Vec<usize> = self.lanes[lane_idx][self.cursor[lane_idx]..].to_vec();
        let max_early = self.table.max_early();
        let mut lift: Option<(usize, f64, Judgement)> = None;
        for &idx in &queue {
            let n = &self.notes[idx];
            match n.state {
                NoteState::HoldActive { .. } => {
                    if !n.is_roll() && n.decay_from.is_none() {
                        self.notes[idx].decay_from = Some(song_time);
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
        out.sort_by(|a, b| a.song_time.total_cmp(&b.song_time));
        out
    }

    /// Advances time to `song_time`: misses, hold life, mines crossed while
    /// held, mines avoided. `held[lane]` is the current button state.
    pub fn update(&mut self, song_time: f64, held: &[bool]) -> Vec<JudgeEvent> {
        let mut out = Vec::new();
        let prev = self.last_time.unwrap_or(song_time);
        self.last_time = Some(song_time);
        let max_ahead = self.table.max_early().max(self.table.mine_window_seconds());
        for (lane, down) in self.down.iter_mut().enumerate() {
            *down = held.get(lane).copied().unwrap_or(false);
        }
        self.cross_heads(song_time, &mut out);
        self.checkpoints_until(song_time, &mut out);
        for lane in 0..self.lanes.len() {
            self.expire(lane, song_time, &mut out);
            let is_held = held.get(lane).copied().unwrap_or(false);
            let queue: Vec<usize> = self.lanes[lane][self.cursor[lane]..].to_vec();
            for &idx in &queue {
                let n = &self.notes[idx];
                let seconds = n.seconds;
                if seconds - song_time > max_ahead {
                    break;
                }
                match n.state {
                    NoteState::Pending if n.is_mine() => {
                        if is_held && prev < seconds && seconds <= song_time {
                            self.notes[idx].state = NoteState::MineHit;
                            out.push(self.event(idx, JudgeEventKind::HitMine, seconds));
                        } else if song_time > seconds + self.table.mine_window_seconds() {
                            self.notes[idx].state = NoteState::MineAvoided;
                        }
                    }
                    NoteState::HoldActive { .. } => {
                        if !n.is_roll() {
                            if is_held {
                                self.regrab(idx, prev);
                            } else if n.decay_from.is_none() {
                                self.notes[idx].decay_from = Some(prev);
                            }
                        }
                        self.settle_hold(idx, song_time, &mut out);
                    }
                    _ => {}
                }
            }
            self.advance_cursor(lane);
        }
        // Lanes are scanned one after another; hand events over in the order
        // they happened so combo and gauge see them chronologically.
        out.sort_by(|a, b| a.song_time.total_cmp(&b.song_time));
        out
    }
}

/// The checkpoint rows of a chart's holds and rolls: rows on the tick
/// grid of the tick count in force there (`row % (48 / ticks) == 0`, as
/// StepMania's `Player::UpdateHoldNotes` checks; no grid where it is 0),
/// after a hold's head and up to its tail, where notes are judged.
fn checkpoint_rows(notes: &[JudgedNote], timing: &TimingMap) -> Vec<CheckpointRow> {
    let mut by_tick: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    for (i, n) in notes.iter().enumerate() {
        let end = match n.note.kind {
            NoteKind::HoldHead { end } | NoteKind::RollHead { end } if n.judgeable => end,
            _ => continue,
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
        .filter(|n| {
            n.judgeable
                && matches!(
                    n.note.kind,
                    NoteKind::Tap | NoteKind::HoldHead { .. } | NoteKind::RollHead { .. }
                )
        })
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
        let ev = j.update(1.0 + 0.18 + 0.001, &[false; 4]);
        let misses: Vec<_> = ev
            .iter()
            .filter(|e| matches!(e.kind, JudgeEventKind::Tap(Judgement::Miss, _)))
            .collect();
        assert_eq!(misses.len(), 2);
        assert_eq!(misses[0].note_index, Some(0));
        assert_eq!(misses[1].note_index, Some(1));
        assert!(j.press(0, 5.01).is_empty()); // a press does not judge a lift
        let ev = j.release(0, 5.01);
        assert!(matches!(ev[0].kind, JudgeEventKind::Tap(Judgement::W1, _)));
    }

    #[test]
    fn hold_let_go_and_held() {
        let mut j = judge();
        j.update(1.9, &[false; 4]);
        let ev = j.press(2, 2.0);
        assert!(matches!(ev[0].kind, JudgeEventKind::Tap(Judgement::W1, _)));
        j.release(2, 2.5);
        let ev = j.update(2.7, &[false; 4]);
        assert!(ev.is_empty());
        assert!(
            matches!(j.notes()[2].state, NoteState::HoldActive { life } if (life - 0.2).abs() < 1e-5)
        );
        let ev = j.update(2.8, &[false; 4]);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].kind, JudgeEventKind::LetGo);
        assert!((ev[0].song_time - 2.75).abs() < 1e-9);

        let mut j = judge();
        j.update(1.9, &[false; 4]);
        j.press(2, 2.0);
        j.release(2, 3.0);
        j.press(2, 3.2);
        let ev = j.update(4.0, &[false, false, true, false]);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].kind, JudgeEventKind::Held);
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
        ev.extend(j.update(1.5, &[true]));
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
        ev.extend(j.update(0.55, &[false]));
        // Never dropped on the way: pressed again, it goes on.
        assert!(matches!(j.notes()[0].state, NoteState::HoldActive { .. }));
        ev.extend(j.press(0, 0.6));
        ev.extend(j.update(1.2, &[true]));
        assert_eq!(
            ticks(&ev),
            [(0.25, 1, 0), (0.5, 0, 1), (0.75, 1, 0), (1.0, 1, 0)]
        );
        assert_eq!(outcomes(&ev), [JudgeEventKind::LetGo]);
        // A short release (0.1 s) is forgiven.
        let mut j = ticked(&[hold(0.0, 0, 1.0)], TimingMap::constant(60.0, 0.0));
        let mut ev = j.press(0, 0.0);
        ev.extend(j.release(0, 0.4));
        ev.extend(j.update(0.5, &[false]));
        ev.extend(j.press(0, 0.52));
        ev.extend(j.update(1.2, &[true]));
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
            ev.extend(j.update(t, &[true]));
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
        let ev = j.update(2.5, &[false]);
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
        let mut ev = j.update(head + 0.1, &[false]);
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
        ev.extend(j.update(1.0, &[true]));
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
        ev.extend(j.update(0.79, &[true]));
        ev.extend(j.release(0, 0.8));
        ev.extend(j.update(1.5, &[false]));
        assert_eq!(ticks(&ev).last(), Some(&(1.0, 0, 1)));
        assert_eq!(outcomes(&ev), [JudgeEventKind::Held]);
    }

    #[test]
    fn events_a_press_finds_on_the_way_come_in_time_order() {
        // A hold kept in lane 0; a tap in lane 2 left unjudged. A late press
        // in lane 2 finds the hold's checkpoints and the tap's miss (at
        // 0.48), which falls between them.
        let notes = [
            hold(0.0, 0, 1.0),
            Note::new(Tick::from_beat_f64(0.3), 2, NoteKind::Tap),
        ];
        let mut j = ticked(&notes, TimingMap::constant(60.0, 0.0));
        j.press(0, 0.0);
        let ev = j.press(2, 1.0);
        assert_eq!(ev.len(), 5);
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
        ev.extend(j.update(3.5, &[false, true]));
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
        ev.extend(j.update(1.5, &[true, false, false]));
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
        ev.extend(j.update(1.5, &[true, false]));
        assert_eq!(ticks(&ev), [(0.5, 1, 0), (1.0, 1, 0)]);
        // Without checkpoints: kept when its head was stepped on.
        ev.extend(j.press(1, 2.0));
        ev.extend(j.release(1, 2.1));
        ev.extend(j.update(4.0, &[false, false]));
        assert_eq!(outcomes(&ev), [JudgeEventKind::Held, JudgeEventKind::Held]);
        let ev = j.update(5.0, &[false, false]);
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
            ev.extend(j.update(t, &[false]));
            ev.extend(j.press(0, t));
        }
        ev.extend(j.update(2.5, &[false]));
        assert_eq!(ticks(&ev).len(), 8);
        assert_eq!(outcomes(&ev), [JudgeEventKind::Held]);
        // Not tapped past the roll window (0.5 s): its life ran out, but
        // it is only judged at its tail.
        let mut j = ticked(&[roll], TimingMap::constant(60.0, 0.0));
        let mut ev = j.press(0, 0.0);
        ev.extend(j.update(1.0, &[false]));
        assert!(outcomes(&ev).is_empty());
        ev.extend(j.press(0, 1.0));
        ev.extend(j.update(2.5, &[false]));
        assert_eq!(outcomes(&ev), [JudgeEventKind::LetGo]);
    }

    #[test]
    fn mines_on_press_and_while_held() {
        let mut j = judge();
        j.update(2.9, &[false; 4]);
        let ev = j.press(3, 3.05);
        assert_eq!(ev[0].kind, JudgeEventKind::HitMine);

        let mut j = judge();
        j.update(2.9, &[false; 4]);
        let ev = j.update(3.1, &[false, false, false, true]);
        assert_eq!(ev[0].kind, JudgeEventKind::HitMine);

        let mut j = judge();
        j.update(2.9, &[false; 4]);
        let ev = j.update(3.2, &[false; 4]);
        assert!(ev.iter().all(|e| e.kind != JudgeEventKind::HitMine));
        assert_eq!(j.notes()[3].state, NoteState::MineAvoided);
    }

    #[test]
    fn empty_press_policies() {
        let mut table = presets::sm5().judge;
        table.empty_press = EmptyPress::Boo;
        let mut j = Judge::new(&notes(), &TimingMap::constant(60.0, 0.0), table.clone());
        let ev = j.press(2, 0.5);
        assert_eq!(ev[0].kind, JudgeEventKind::Boo);
        table.empty_press = EmptyPress::ExcessiveEarly {
            lo: 0.2,
            hi: 0.4,
            factor: 0.25,
        };
        let mut j = Judge::new(&notes(), &TimingMap::constant(60.0, 0.0), table);
        assert!(j.press(1, 0.5).is_empty());
        let ev = j.press(1, 0.7);
        assert_eq!(ev[0].kind, JudgeEventKind::Boo);
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
        assert!(j.update(miss_after, &[false; 4]).is_empty());
        assert_eq!(j.notes()[0].state, NoteState::Pending);
        let ev = j.press(0, miss_after);
        assert!(matches!(ev[0].kind, JudgeEventKind::Tap(Judgement::W5, _)));

        let mut j = judge();
        let ev = j.update(miss_after + 1e-9, &[false; 4]);
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
        j.update(2.4, &[true]);
        assert!(
            matches!(j.notes()[0].state, NoteState::HoldActive { life } if (life - 0.2).abs() < 1e-5)
        );
        let ev = j.update(2.6, &[true]);
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
        let ev = j.update(4.0, &[false]);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].kind, JudgeEventKind::Held);
    }

    #[test]
    fn missed_hold_head_is_never_held() {
        let mut j = judge(); // hold at beat 2 → 4 in lane 2
        j.update(1.9, &[false; 4]);
        let ev = j.update(2.3, &[false; 4]);
        assert!(ev.iter().any(|e| e.note_index == Some(2)
            && matches!(e.kind, JudgeEventKind::Tap(Judgement::Miss, _))));
        assert_eq!(j.notes()[2].state, NoteState::Missed);
        // Pressing and holding through the tail changes nothing.
        j.press(2, 2.35);
        let ev = j.update(4.5, &[false, false, true, false]);
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
        let ev = j.update(10.0, &[false, false, false, true]);
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
        let ev = j.update(10.0, &[false; 3]);
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
        assert!(j.update(3.0 + 1.5 * F, &[false]).is_empty());
        let ev = j.update(3.1, &[false]);
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
        let ev = j.update(4.0, &[true]);
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
        let ev = j.update(2.5, &[false]);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].kind, JudgeEventKind::LetGo);
        assert_eq!(j.judged_note_count(), 0);
        let mut j = Judge::new(&one_hold(), &timing, danoni_table(true));
        let ev = j.update(2.5, &[false]);
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
