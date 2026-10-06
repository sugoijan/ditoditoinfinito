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

use ddi_chart::{Note, NoteKind, Tick, TimingMap};
use serde::{Deserialize, Serialize};

use crate::rules::{EmptyPress, JudgeTable, Judgement};

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
        let mut judge = Judge {
            notes,
            rows: Vec::new(),
            cursor: vec![0; lane_count],
            lanes,
            table,
            last_time: None,
        };
        judge.rebuild_rows();
        judge
    }

    fn rebuild_rows(&mut self) {
        self.rows.clear();
        let mut order: Vec<usize> = (0..self.notes.len())
            .filter(|&i| self.notes[i].judgeable && self.notes[i].is_tap_like())
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

    /// Judgeable tap-like notes (taps, hold/roll heads, lifts).
    pub fn judged_note_count(&self) -> u32 {
        self.notes
            .iter()
            .filter(|n| n.judgeable && n.is_tap_like())
            .count() as u32
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
        if self.notes[idx].is_roll() {
            self.table.roll_window_seconds()
        } else {
            self.table.hold_window_seconds()
        }
    }

    /// Hold life at `song_time` given the decay start (`None` = full).
    fn hold_life(&self, idx: usize, song_time: f64) -> f32 {
        match self.notes[idx].decay_from {
            None => 1.0,
            Some(from) => {
                let until = song_time.min(self.notes[idx].end_seconds);
                (1.0 - (until - from) / self.hold_window(idx)) as f32
            }
        }
    }

    /// Resolves an active hold at `song_time`: `LetGo` when its life ran
    /// out, `Held` once the tail passed, otherwise just refreshes `life`.
    fn settle_hold(&mut self, idx: usize, song_time: f64, out: &mut Vec<JudgeEvent>) {
        let life = self.hold_life(idx, song_time);
        if life <= 0.0 {
            let at = self.notes[idx].decay_from.unwrap_or(song_time) + self.hold_window(idx);
            self.notes[idx].state = NoteState::LetGo;
            out.push(self.event(idx, JudgeEventKind::LetGo, at.min(song_time)));
        } else if song_time >= self.notes[idx].end_seconds {
            self.notes[idx].state = NoteState::Held;
            out.push(self.event(idx, JudgeEventKind::Held, self.notes[idx].end_seconds));
        } else {
            self.notes[idx].state = NoteState::HoldActive { life };
        }
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
        let Some(queue) = self.lanes.get(lane_idx) else {
            return out;
        };
        let queue: Vec<usize> = queue[self.cursor[lane_idx]..].to_vec();

        // Active holds get their life back; active rolls restart their decay.
        for &idx in &queue {
            if let NoteState::HoldActive { .. } = self.notes[idx].state {
                let life = self.hold_life(idx, song_time);
                if life <= 0.0 {
                    self.settle_hold(idx, song_time, &mut out);
                } else {
                    self.notes[idx].decay_from = if self.notes[idx].is_roll() {
                        Some(song_time)
                    } else {
                        None
                    };
                    self.notes[idx].state = NoteState::HoldActive { life: 1.0 };
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
        } else if let Some((idx, delta, judgement)) = tap {
            let is_hold = matches!(
                self.notes[idx].note.kind,
                NoteKind::HoldHead { .. } | NoteKind::RollHead { .. }
            );
            self.notes[idx].state = if is_hold {
                NoteState::HoldActive { life: 1.0 }
            } else {
                NoteState::Hit { judgement, delta }
            };
            self.notes[idx].decay_from = if self.notes[idx].is_roll() {
                Some(song_time)
            } else {
                None
            };
            out.push(self.event(idx, JudgeEventKind::Tap(judgement, delta), song_time));
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
        out
    }

    /// A button went up in `lane` at `song_time`: judges lifts and starts
    /// the life decay of active holds.
    pub fn release(&mut self, lane: u8, song_time: f64) -> Vec<JudgeEvent> {
        let mut out = Vec::new();
        let lane_idx = usize::from(lane);
        let Some(queue) = self.lanes.get(lane_idx) else {
            return out;
        };
        let queue: Vec<usize> = queue[self.cursor[lane_idx]..].to_vec();
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
        out
    }

    /// Advances time to `song_time`: misses, hold life, mines crossed while
    /// held, mines avoided. `held[lane]` is the current button state.
    pub fn update(&mut self, song_time: f64, held: &[bool]) -> Vec<JudgeEvent> {
        let mut out = Vec::new();
        let prev = self.last_time.unwrap_or(song_time);
        self.last_time = Some(song_time);
        let miss_after = self.table.miss_after();
        let max_ahead = self.table.max_early().max(self.table.mine_window_seconds());
        for lane in 0..self.lanes.len() {
            let is_held = held.get(lane).copied().unwrap_or(false);
            let queue: Vec<usize> = self.lanes[lane][self.cursor[lane]..].to_vec();
            for idx in queue {
                let n = &self.notes[idx];
                let seconds = n.seconds;
                if seconds - song_time > max_ahead {
                    break;
                }
                match n.state {
                    NoteState::Pending if n.is_tap_like() => {
                        if song_time - seconds > miss_after {
                            self.notes[idx].state = NoteState::Missed;
                            out.push(self.event(
                                idx,
                                JudgeEventKind::Tap(Judgement::Miss, miss_after),
                                seconds + miss_after,
                            ));
                        }
                    }
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
                                self.notes[idx].decay_from = None;
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
