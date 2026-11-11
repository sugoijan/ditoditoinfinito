//! Battle shocks in a play (`docs/plans/multiplayer-prototype-progress.md`,
//! M8a; the maintainer's decisions of 2026-11-11, D1 and D3): the mines an
//! attack puts into its recipient's play, judged apart from the chart
//! ([`HazardJudge`], `Player::add_hazards`: never inserted into it, so the
//! chart's notes keep their indices), and where a chart has room for them
//! ([`schedule`], [`PLACEMENT_V0`]).
//!
//! A shock costs exactly the ruleset's miss to the gauge; it never changes
//! the score, the accuracy or the judgments' counts; it breaks the combo
//! only where the ruleset's own mines do (`ComboRules::mine_breaks`);
//! [`INJECTED_MINE_V0`]. PROVISIONAL: every number here is a working
//! value, named and versioned.

use ddi_chart::{Note, NoteKind, Song, TICKS_PER_BEAT, Tick, TimingMap};

use crate::rules::JudgeTable;

/// Note indices of battle shocks' events and sprites start here, past any
/// chart's notes: what tells them from the chart's.
pub const HAZARD_INDEX_BASE: usize = 1 << 30;

/// How a battle shock is judged: the ruleset's mine window, or this one
/// where the ruleset plays no mines (FFR: `fallback`, 70 ms, ITG's).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InjectedMine {
    pub id: &'static str,
    pub fallback: f64,
}

/// `injected-mine-v0` (PROVISIONAL, D1): a shock hit costs the ruleset's
/// miss to the gauge, nothing else; FFR's window 70 ms.
pub const INJECTED_MINE_V0: InjectedMine = InjectedMine {
    id: "injected-mine-v0",
    fallback: 0.070,
};

impl InjectedMine {
    /// The window a shock is judged in under `table`, seconds each side.
    pub fn window(&self, table: &JudgeTable) -> f64 {
        let w = table.mine_window_seconds();
        if w > 0.0 { w } else { self.fallback }
    }
}

/// Where an attack's shocks may go (PROVISIONAL, `placement-v0`): a
/// section from a checkpoint (every four beats), `section_beats` long,
/// `mines` shocks on the eighth-note grid (the sixteenth where the eighths
/// have too few safe places), one a beat where it can, at most two, one a
/// row (never a full row), each clear of what the player must do in its
/// lane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub id: &'static str,
    pub section_beats: u32,
    pub mines: usize,
    /// Beats between sections' possible starts (the attack checkpoints').
    pub every_beats: u32,
    /// Beyond the mine window and the widest judged window, seconds, no
    /// step in the lane.
    pub margin: f64,
    /// No hold or roll ends this long before a shock in its lane.
    pub after_tail: f64,
    /// No lift this long after a shock in its lane (its lane is held
    /// before it is released).
    pub before_lift: f64,
}

pub const PLACEMENT_V0: Placement = Placement {
    id: "placement-v0",
    section_beats: 8,
    mines: 8,
    every_beats: 4,
    margin: 0.050,
    after_tail: 0.150,
    before_lift: 0.5,
};

/// A battle shock: its row and lane (the lanes as played).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hazard {
    pub tick: Tick,
    pub lane: u8,
}

/// One section's shocks ([`schedule`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// Its first beat (a checkpoint's).
    pub start_beat: u32,
    /// In time order.
    pub cells: Vec<Hazard>,
}

/// SplitMix64.
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// Whether a shock at `cell` is safe in chart `notes` (the whole chart,
/// as written: one schedule for everyone) under `table`, its window
/// `window` ([`PLACEMENT_V0`]'s rules).
fn safe(
    cell: Hazard,
    notes: &[Note],
    timing: &TimingMap,
    table: &JudgeTable,
    window: f64,
    p: &Placement,
) -> bool {
    if !timing.judgeable(cell.tick) {
        return false;
    }
    let m = timing.seconds_at(cell.tick);
    let clear = window + table.max_early().max(table.miss_after()) + p.margin;
    notes.iter().filter(|n| n.lane == cell.lane).all(|n| {
        let at = timing.seconds_at(n.tick);
        match n.kind {
            NoteKind::Tap => (at - m).abs() > clear,
            NoteKind::Lift => (at - m).abs() > clear && !(at > m && at - m <= p.before_lift),
            NoteKind::HoldHead { end } | NoteKind::RollHead { end } => {
                let end = timing.seconds_at(end);
                (at - m).abs() > clear
                    // No body across the shock's window.
                    && (end < m - window || at > m + window)
                    // No tail just before it.
                    && !(end <= m && m - end <= p.after_tail)
            }
            NoteKind::Mine | NoteKind::Shock => n.tick != cell.tick,
            NoteKind::Fake | NoteKind::AutoKeysound | NoteKind::Dummy => true,
        }
    })
}

/// The section from beat `start` of chart `chart` of `song`, if eight
/// safe shocks fit ([`PLACEMENT_V0`]), drawn from `seed`.
fn section(
    song: &Song,
    chart: usize,
    table: &JudgeTable,
    start: u32,
    seed: u64,
    p: &Placement,
) -> Option<Section> {
    let c = song.charts.get(chart)?;
    let timing = c.timing(song);
    let lanes = song
        .layout_of(c)
        .map(|l| l.lane_count())
        .unwrap_or_else(|| {
            c.notes
                .iter()
                .map(|n| usize::from(n.lane) + 1)
                .max()
                .unwrap_or(0)
        });
    let window = INJECTED_MINE_V0.window(table);
    let first = i64::from(start) * TICKS_PER_BEAT;
    // The eighths, then the sixteenths.
    for step in [TICKS_PER_BEAT / 2, TICKS_PER_BEAT / 4] {
        let mut by_beat: Vec<Vec<Hazard>> = vec![Vec::new(); p.section_beats as usize];
        for (b, cells) in by_beat.iter_mut().enumerate() {
            let beat = first + b as i64 * TICKS_PER_BEAT;
            for t in (beat..beat + TICKS_PER_BEAT).step_by(step as usize) {
                for lane in 0..lanes.min(64) as u8 {
                    let cell = Hazard {
                        tick: Tick(t),
                        lane,
                    };
                    if safe(cell, &c.notes, timing, table, window, p) {
                        cells.push(cell);
                    }
                }
            }
        }
        let mut rng = mix(seed ^ mix(u64::from(start)) ^ step as u64);
        let mut draw = |n: usize| {
            rng = mix(rng);
            (rng % n as u64) as usize
        };
        let mut chosen: Vec<Hazard> = Vec::new();
        let mut taken = vec![0usize; by_beat.len()];
        // One a beat where a beat has room, then a second in the beats
        // with room left, until eight; never two in a row.
        for round in 0..2 {
            for (b, cells) in by_beat.iter().enumerate() {
                if chosen.len() >= p.mines || taken[b] > round {
                    continue;
                }
                let free: Vec<Hazard> = cells
                    .iter()
                    .copied()
                    .filter(|c| !chosen.iter().any(|o| o.tick == c.tick))
                    .collect();
                if free.is_empty() {
                    continue;
                }
                chosen.push(free[draw(free.len())]);
                taken[b] += 1;
            }
        }
        if chosen.len() >= p.mines {
            chosen.sort();
            return Some(Section {
                start_beat: start,
                cells: chosen,
            });
        }
    }
    None
}

/// Every section chart `chart` of `song` has under `table`, one per
/// checkpoint (every [`PLACEMENT_V0`]'s four beats from beat 0 to the
/// chart's last), `None` where eight safe shocks do not fit (never filled
/// with unsafe ones): pure in the chart, the table and `seed`, so everyone
/// gets the same.
pub fn schedule(song: &Song, chart: usize, table: &JudgeTable, seed: u64) -> Vec<Option<Section>> {
    let p = PLACEMENT_V0;
    let Some(c) = song.charts.get(chart) else {
        return Vec::new();
    };
    let last = c.notes.iter().map(|n| n.end_tick().0).max().unwrap_or(0);
    let last_beat = (last / TICKS_PER_BEAT).max(0) as u32;
    (0..=last_beat.saturating_sub(p.section_beats))
        .step_by(p.every_beats as usize)
        .map(|start| section(song, chart, table, start, seed, &p))
        .collect()
}

/// A shock in a play, judged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HazardMine {
    pub tick: Tick,
    pub lane: u8,
    pub seconds: f64,
    pub state: HazardState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HazardState {
    Pending,
    Hit,
    Avoided,
}

/// Why shocks could not be put into a play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HazardError {
    /// Not a lane of the play.
    Lane,
    /// Its window has begun already: a shock must arrive before it could
    /// be stepped on.
    Late,
    /// Before a shock already in (one attack at a time, in order).
    Order,
}

/// Battle shocks judged beside a play's chart: a press in a shock's
/// window steps on it (at the press), a lane held while one passes steps
/// on it (at its time), as the chart's mines are judged; one passed by its
/// window unstepped is avoided. Each instant judged exactly, so however
/// time is cut into calls the outcome is the same.
#[derive(Clone, Debug)]
pub struct HazardJudge {
    window: f64,
    mines: Vec<HazardMine>,
    down: Vec<bool>,
    last: Option<f64>,
    first_open: usize,
}

/// A shock stepped on: which ([`HazardJudge::mines`]'s index) and when.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HazardHit {
    pub index: usize,
    pub at: f64,
}

impl HazardJudge {
    pub fn new(window: f64, lanes: usize) -> HazardJudge {
        HazardJudge {
            window,
            mines: Vec::new(),
            down: vec![false; lanes],
            last: None,
            first_open: 0,
        }
    }

    pub fn window(&self) -> f64 {
        self.window
    }

    pub fn mines(&self) -> &[HazardMine] {
        &self.mines
    }

    /// Adds `mines` (`(cell, its second)`), judged from now on: each in a
    /// lane, after every shock already in, before its window while judging
    /// is at `judged` (the play's judged time).
    pub fn add(&mut self, mines: &[(Hazard, f64)], judged: Option<f64>) -> Result<(), HazardError> {
        let mut sorted: Vec<(Hazard, f64)> = mines.to_vec();
        sorted.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.lane.cmp(&b.0.lane)));
        let now = judged
            .into_iter()
            .chain(self.last)
            .fold(f64::NEG_INFINITY, f64::max);
        for (cell, at) in &sorted {
            if usize::from(cell.lane) >= self.down.len() {
                return Err(HazardError::Lane);
            }
            if at - self.window <= now {
                return Err(HazardError::Late);
            }
        }
        if let (Some(first), Some(last)) = (sorted.first(), self.mines.last())
            && first.1 <= last.seconds
        {
            return Err(HazardError::Order);
        }
        self.mines
            .extend(sorted.into_iter().map(|(c, at)| HazardMine {
                tick: c.tick,
                lane: c.lane,
                seconds: at,
                state: HazardState::Pending,
            }));
        Ok(())
    }

    /// Everything before `t`, every lane as it was.
    pub fn update(&mut self, t: f64) -> Vec<HazardHit> {
        let mut out = Vec::new();
        self.advance(t, &mut out);
        out
    }

    fn advance(&mut self, t: f64, out: &mut Vec<HazardHit>) {
        let prev = self.last.unwrap_or(f64::NEG_INFINITY);
        if !t.is_finite() || t < prev {
            return;
        }
        self.last = Some(t);
        for i in self.first_open..self.mines.len() {
            let m = self.mines[i];
            if m.seconds - t > self.window {
                break;
            }
            if m.state != HazardState::Pending {
                continue;
            }
            if self.down[usize::from(m.lane)] && prev <= m.seconds && m.seconds < t {
                self.mines[i].state = HazardState::Hit;
                out.push(HazardHit {
                    index: i,
                    at: m.seconds,
                });
            } else if t > m.seconds + self.window {
                self.mines[i].state = HazardState::Avoided;
            }
        }
        while self
            .mines
            .get(self.first_open)
            .is_some_and(|m| m.state != HazardState::Pending)
        {
            self.first_open += 1;
        }
        out.sort_by(|a, b| a.at.total_cmp(&b.at).then(a.index.cmp(&b.index)));
    }

    /// A press of `lane` at `t`: the nearest shock in its window there
    /// stepped on.
    pub fn press(&mut self, lane: u8, t: f64) -> Vec<HazardHit> {
        let mut out = Vec::new();
        let Some(l) = self.down.get(usize::from(lane)).map(|_| usize::from(lane)) else {
            return out;
        };
        self.advance(t, &mut out);
        self.down[l] = true;
        let nearest = (self.first_open..self.mines.len())
            .filter(|&i| {
                let m = &self.mines[i];
                m.state == HazardState::Pending
                    && usize::from(m.lane) == l
                    && (t - m.seconds).abs() <= self.window
            })
            .min_by(|&a, &b| {
                (t - self.mines[a].seconds)
                    .abs()
                    .total_cmp(&(t - self.mines[b].seconds).abs())
            });
        if let Some(i) = nearest {
            self.mines[i].state = HazardState::Hit;
            out.push(HazardHit { index: i, at: t });
        }
        out
    }

    /// A release of `lane` at `t`.
    pub fn release(&mut self, lane: u8, t: f64) -> Vec<HazardHit> {
        let mut out = Vec::new();
        self.advance(t, &mut out);
        if let Some(d) = self.down.get_mut(usize::from(lane)) {
            *d = false;
        }
        out
    }

    /// Stepped on and avoided so far.
    pub fn counts(&self) -> (u32, u32) {
        self.mines.iter().fold((0, 0), |(h, a), m| match m.state {
            HazardState::Hit => (h + 1, a),
            HazardState::Avoided => (h, a + 1),
            HazardState::Pending => (h, a),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(beat: i64, lane: u8) -> Hazard {
        Hazard {
            tick: Tick(beat * TICKS_PER_BEAT),
            lane,
        }
    }

    fn judge() -> HazardJudge {
        let mut j = HazardJudge::new(0.07, 4);
        j.add(
            &[(cell(2, 0), 1.0), (cell(4, 1), 2.0), (cell(6, 2), 3.0)],
            None,
        )
        .unwrap();
        j
    }

    #[test]
    fn a_press_in_the_window_steps_on_it_and_a_held_lane_crosses_it() {
        let mut j = judge();
        assert!(j.press(0, 0.9).is_empty(), "before its window");
        j.release(0, 0.92);
        assert_eq!(j.press(0, 1.05), vec![HazardHit { index: 0, at: 1.05 }]);
        // Held across the second.
        j.press(1, 1.5);
        assert_eq!(j.update(2.5), vec![HazardHit { index: 1, at: 2.0 }]);
        j.release(1, 2.6);
        // The third passes unstepped.
        assert!(j.update(4.0).is_empty());
        assert_eq!(j.counts(), (2, 1));
    }

    #[test]
    fn the_outcome_does_not_depend_on_how_time_is_cut() {
        let run = |step: f64| {
            let mut j = judge();
            let edges = [
                (1.5, 1, true),
                (2.3, 1, false),
                (2.95, 2, true),
                (3.2, 2, false),
            ];
            let mut hits = Vec::new();
            let mut t = 0.0;
            let mut e = edges.iter().peekable();
            while t < 5.0 {
                t += step;
                while let Some(&&(at, lane, down)) = e.peek() {
                    if at > t {
                        break;
                    }
                    e.next();
                    hits.extend(if down {
                        j.press(lane, at)
                    } else {
                        j.release(lane, at)
                    });
                }
                hits.extend(j.update(t));
            }
            (hits, j.counts())
        };
        assert_eq!(run(1.0 / 60.0), run(1.0 / 144.0));
        assert_eq!(run(1.0 / 60.0), run(0.9));
        assert_eq!(run(0.9).1, (2, 1));
    }

    #[test]
    fn shocks_arrive_before_their_window_and_in_order() {
        let mut j = HazardJudge::new(0.07, 4);
        assert_eq!(j.add(&[(cell(2, 9), 5.0)], None), Err(HazardError::Lane));
        assert_eq!(
            j.add(&[(cell(2, 0), 1.0)], Some(0.95)),
            Err(HazardError::Late)
        );
        j.add(&[(cell(2, 0), 2.0)], Some(0.95)).unwrap();
        assert_eq!(
            j.add(&[(cell(1, 0), 1.5)], Some(0.95)),
            Err(HazardError::Order)
        );
        assert_eq!(
            INJECTED_MINE_V0.window(&crate::rules::presets::ffr().judge),
            0.070
        );
        let itg = crate::rules::presets::itg().judge;
        assert_eq!(INJECTED_MINE_V0.window(&itg), itg.mine_window_seconds());
    }
}
