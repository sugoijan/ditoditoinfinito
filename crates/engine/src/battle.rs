//! Battle shocks in a play (`docs/plans/multiplayer-prototype-progress.md`,
//! M8a; the maintainer's decisions of 2026-11-11, D1 and D3): the mines an
//! attack puts into its recipient's play, judged apart from the chart
//! ([`HazardJudge`], `Player::add_hazards`: never inserted into it, so the
//! chart's notes keep their indices), and where a chart has room for them
//! ([`schedule`], [`PLACEMENT`]: margins about each step of a shock's
//! lane, `placement-v1`).
//!
//! Under DDI, the rooms' ruleset (`docs/plans/ddi-ruleset.md`, "Mines and
//! shocks": one object, two looks), **a shock is a mine**: stepped on, it
//! is judged, counted and scored exactly as a chart's mine (the tally, the
//! points, the life by the difficulty, the combo, the grade's floors, the
//! lamp) and locks its lane (`JudgeTable::lock`, `Judge::lock_lane`), and
//! a press in reach of a shock and of a note steps on the nearer, as a
//! chart's mine is chosen (`Player::input_at`, `Judge::press_beside`).
//!
//! Under a ruleset without the lock (the other presets, where a play is
//! given shocks by a test or a study: no room plays them any more), a
//! shock costs exactly the ruleset's miss to the gauge; it never changes
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

/// `injected-mine-v0` (PROVISIONAL, D1): the window a shock is judged in
/// (the ruleset's mine window; FFR's, which plays no mines, 70 ms) and,
/// under a ruleset without DDI's lane lock, what a hit costs: the
/// ruleset's miss to the gauge, nothing else. Under DDI a shock is a mine
/// (the module's note).
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

/// Where an attack's shocks may go (PROVISIONAL): a section from a
/// checkpoint (every four beats), `section_beats` long, `mines` shocks on
/// the eighth-note grid (the sixteenth where the eighths have too few safe
/// places), one a beat where it can, at most two, one a row (never a full
/// row), each clear of what the player must do in its lane
/// ([`Clearance`]); since `placement-v2`, most of them next to a note of
/// their lane ([`Near`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub id: &'static str,
    pub section_beats: u32,
    pub mines: usize,
    /// Beats between sections' possible starts (the attack checkpoints').
    pub every_beats: u32,
    pub clear: Clearance,
    /// `placement-v2`: most of a section's shocks aimed at places next
    /// to a note of their lane; `None`, anywhere the clearance allows.
    pub near: Option<Near>,
}

/// `placement-v2` (the maintainer, 2026-11-12: "the placement is a bit
/// too easy to ignore sometimes, maybe we should prioritize adding some
/// of them to lanes which already have a note nearby"): among the places
/// the [`Clearance`] allows, which are unchanged, a place is *next to a
/// note* of its lane when it is shortly after one (from the clearance's
/// `after_note` to `after` seconds past its step, a hold's or a roll's
/// end: a key still resting on the lane steps on it) or shortly before a
/// step (from the early W3 to `before_factor` times it ahead: an early
/// press steps on it). Of a section's shocks, `aim` go to such places
/// where the section has them, on the sixteenth-note grid, one a beat
/// then a second, spread over the lanes (the lane with the fewest so
/// far), never two by one note, never two in a row, and never two in a
/// lane closer than `lane_gap` seconds (the lane lock's: a second shock
/// inside the first's lock could not be stepped on). The rest are placed
/// as `placement-v1` places them, keeping `lane_gap` too; only where a
/// section then has no room for eight do the remaining ones ignore
/// `lane_gap` (as v1 always did), and where even that fails the section
/// is v1's own, so no section v1 has is lost.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Near {
    pub after: f64,
    pub before_factor: f64,
    pub aim: usize,
    pub lane_gap: f64,
}

/// How far a shock keeps from what the player must do in its lane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Clearance {
    /// `placement-v0`: both ways of every step by the shock's window plus
    /// the widest judged window plus `margin`; no hold or roll across its
    /// window; none ending `after_tail` before it; no lift `before_lift`
    /// after it.
    Windows {
        margin: f64,
        after_tail: f64,
        before_lift: f64,
    },
    /// `placement-v1` (`docs/plans/ddi-ruleset.md`, "Decided after the
    /// re-review"): margins about each step of the lane, so that a shock
    /// is always avoidable. None within `after_note` seconds after a step
    /// (the key that pressed it may still be down: a held key steps on a
    /// mine) nor within the ruleset's third tier's early window before
    /// one (W3: a press that early still hits the note); none from a
    /// hold's or a roll's head, that long before it, to `after_note` past
    /// its end; none from `lift_ready` seconds before a lift (its key is
    /// pressed ahead to be let go on it) to `after_note` past it.
    Margins { after_note: f64, lift_ready: f64 },
}

pub const PLACEMENT_V0: Placement = Placement {
    id: "placement-v0",
    section_beats: 8,
    mines: 8,
    every_beats: 4,
    clear: Clearance::Windows {
        margin: 0.050,
        after_tail: 0.150,
        before_lift: 0.5,
    },
    near: None,
};

/// `placement-v1` (PROVISIONAL; the values to tune): 0.25 s after a step,
/// W3 before it, a lift's key ready a second ahead (DDI's
/// `BOO_LIFT_READY`).
pub const PLACEMENT_V1: Placement = Placement {
    id: "placement-v1",
    section_beats: 8,
    mines: 8,
    every_beats: 4,
    clear: Clearance::Margins {
        after_note: 0.25,
        lift_ready: 1.0,
    },
    near: None,
};

/// `placement-v2` (PROVISIONAL; the values to tune): v1's margins exactly,
/// six of the eight shocks next to a note of their lane where the section
/// has such places (up to half a second after one, or up to twice W3
/// before a step), a second between two shocks of a lane ([`Near`]).
pub const PLACEMENT_V2: Placement = Placement {
    id: "placement-v2",
    section_beats: 8,
    mines: 8,
    every_beats: 4,
    clear: Clearance::Margins {
        after_note: 0.25,
        lift_ready: 1.0,
    },
    near: Some(Near {
        after: 0.5,
        before_factor: 2.0,
        aim: 6,
        lane_gap: 1.0,
    }),
};

/// The placement attacks use ([`schedule`]).
pub const PLACEMENT: Placement = PLACEMENT_V2;

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
/// `window`, by `p`'s [`Clearance`].
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
    let mut lane = notes.iter().filter(|n| n.lane == cell.lane);
    match p.clear {
        Clearance::Windows {
            margin,
            after_tail,
            before_lift,
        } => {
            let clear = window + table.max_early().max(table.miss_after()) + margin;
            lane.all(|n| {
                let at = timing.seconds_at(n.tick);
                match n.kind {
                    NoteKind::Tap => (at - m).abs() > clear,
                    NoteKind::Lift => {
                        (at - m).abs() > clear && !(at > m && at - m <= before_lift)
                    }
                    NoteKind::HoldHead { end } | NoteKind::RollHead { end } => {
                        let end = timing.seconds_at(end);
                        (at - m).abs() > clear
                            // No body across the shock's window.
                            && (end < m - window || at > m + window)
                            // No tail just before it.
                            && !(end <= m && m - end <= after_tail)
                    }
                    NoteKind::Mine | NoteKind::Shock => n.tick != cell.tick,
                    NoteKind::Fake | NoteKind::AutoKeysound | NoteKind::Dummy => true,
                }
            })
        }
        Clearance::Margins {
            after_note,
            lift_ready,
        } => {
            // A press this early still hits a step (the third tier; the
            // widest window where the ruleset has no third).
            let before = table
                .window(crate::rules::Judgement::W3)
                .map_or_else(|| table.max_early(), |w| w.early);
            // Not from `from` to `after_note` past `to`.
            let outside = |from: f64, to: f64| m < from || m > to + after_note;
            lane.all(|n| {
                let at = timing.seconds_at(n.tick);
                match n.kind {
                    NoteKind::Tap => outside(at - before, at),
                    NoteKind::Lift => outside(at - lift_ready.max(before), at),
                    NoteKind::HoldHead { end } | NoteKind::RollHead { end } => {
                        outside(at - before, timing.seconds_at(end).max(at))
                    }
                    NoteKind::Mine | NoteKind::Shock => n.tick != cell.tick,
                    NoteKind::Fake | NoteKind::AutoKeysound | NoteKind::Dummy => true,
                }
            })
        }
    }
}

/// The note of its lane a safe shock at `cell` is next to
/// ([`Near`]), by its place in `notes`: shortly after it, or shortly
/// before its step.
fn beside(
    cell: Hazard,
    notes: &[Note],
    timing: &TimingMap,
    table: &JudgeTable,
    p: &Placement,
) -> Option<usize> {
    let near = p.near?;
    let Clearance::Margins { after_note, .. } = p.clear else {
        return None;
    };
    let before = table
        .window(crate::rules::Judgement::W3)
        .map_or_else(|| table.max_early(), |w| w.early);
    let m = timing.seconds_at(cell.tick);
    notes.iter().position(|n| {
        if n.lane != cell.lane {
            return false;
        }
        let at = timing.seconds_at(n.tick);
        let (end, steps) = match n.kind {
            NoteKind::Tap => (at, true),
            // A lift's key is ready long before it: only after it.
            NoteKind::Lift => (at, false),
            NoteKind::HoldHead { end } | NoteKind::RollHead { end } => {
                (timing.seconds_at(end).max(at), true)
            }
            _ => return false,
        };
        (m >= end + after_note && m <= end + near.after)
            || (steps && m <= at - before && m >= at - near.before_factor * before)
    })
}

/// A section by `placement-v2` ([`Near`]): the shocks next to a note
/// first, then the rest as v1 places them; `None` where eight do not fit
/// so (the caller then places it as v1 does).
fn section_near(
    song: &Song,
    chart: usize,
    table: &JudgeTable,
    start: u32,
    seed: u64,
    p: &Placement,
    near: Near,
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
        })
        .min(64);
    let window = INJECTED_MINE_V0.window(table);
    let first = i64::from(start) * TICKS_PER_BEAT;
    let sixteenth = TICKS_PER_BEAT / 4;
    // Every safe place of the section on the sixteenths: its beat, its
    // second and the note it is next to.
    struct Place {
        cell: Hazard,
        beat: usize,
        at: f64,
        beside: Option<usize>,
    }
    let mut places = Vec::new();
    for b in 0..p.section_beats as usize {
        let beat = first + b as i64 * TICKS_PER_BEAT;
        for t in (beat..beat + TICKS_PER_BEAT).step_by(sixteenth as usize) {
            for lane in 0..lanes as u8 {
                let cell = Hazard {
                    tick: Tick(t),
                    lane,
                };
                if safe(cell, &c.notes, timing, table, window, p) {
                    places.push(Place {
                        cell,
                        beat: b,
                        at: timing.seconds_at(cell.tick),
                        beside: beside(cell, &c.notes, timing, table, p),
                    });
                }
            }
        }
    }
    // Free of the shocks chosen so far: another row, and (`gap`) a lane
    // gap from each of its lane's.
    let free = |pl: &Place, chosen: &[&Place], gap: bool| {
        chosen.iter().all(|o| {
            o.cell.tick != pl.cell.tick
                && (!gap || o.cell.lane != pl.cell.lane || (o.at - pl.at).abs() >= near.lane_gap)
        })
    };
    // Next to a note: one a beat, then a second, in the lane with the
    // fewest so far, one a note.
    let mut rng = mix(seed ^ mix(u64::from(start)) ^ 0x6e65_6172);
    let mut aimed: Vec<&Place> = Vec::new();
    let mut taken = vec![0usize; p.section_beats as usize];
    let mut per_lane = vec![0usize; lanes];
    for round in 0..2 {
        for (b, taken) in taken.iter_mut().enumerate() {
            if aimed.len() >= near.aim.min(p.mines) || *taken > round {
                continue;
            }
            let pool: Vec<&Place> = places
                .iter()
                .filter(|pl| {
                    pl.beat == b
                        && pl.beside.is_some()
                        && !aimed.iter().any(|o| o.beside == pl.beside)
                        && free(pl, &aimed, true)
                })
                .collect();
            let Some(least) = pool.iter().map(|pl| per_lane[usize::from(pl.cell.lane)]).min()
            else {
                continue;
            };
            let pool: Vec<&Place> = pool
                .into_iter()
                .filter(|pl| per_lane[usize::from(pl.cell.lane)] == least)
                .collect();
            rng = mix(rng);
            let pick = pool[(rng % pool.len() as u64) as usize];
            per_lane[usize::from(pick.cell.lane)] += 1;
            *taken += 1;
            aimed.push(pick);
        }
    }
    // The rest as v1: the eighths, then the sixteenths, one a beat then a
    // second; a lane gap kept, then (no other room) not.
    for (step, gap) in [
        (TICKS_PER_BEAT / 2, true),
        (sixteenth, true),
        (TICKS_PER_BEAT / 2, false),
        (sixteenth, false),
    ] {
        let mut rng = mix(seed ^ mix(u64::from(start)) ^ step as u64);
        let mut chosen = aimed.clone();
        let mut taken = taken.clone();
        for round in 0..2 {
            for (b, taken) in taken.iter_mut().enumerate() {
                if chosen.len() >= p.mines || *taken > round {
                    continue;
                }
                let pool: Vec<&Place> = places
                    .iter()
                    .filter(|pl| {
                        pl.beat == b && (pl.cell.tick.0 - first) % step == 0 && free(pl, &chosen, gap)
                    })
                    .collect();
                if pool.is_empty() {
                    continue;
                }
                rng = mix(rng);
                chosen.push(pool[(rng % pool.len() as u64) as usize]);
                *taken += 1;
            }
        }
        if chosen.len() >= p.mines {
            let mut cells: Vec<Hazard> = chosen.iter().map(|pl| pl.cell).collect();
            cells.sort();
            return Some(Section {
                start_beat: start,
                cells,
            });
        }
    }
    None
}

/// How many of `cells` (a section's, placed by `p` on chart `chart` of
/// `song` under `table`) are next to a note of their lane ([`Near`]): for
/// tests and the studies.
pub fn beside_count(
    song: &Song,
    chart: usize,
    table: &JudgeTable,
    cells: &[Hazard],
    p: &Placement,
) -> usize {
    let Some(c) = song.charts.get(chart) else {
        return 0;
    };
    let timing = c.timing(song);
    cells
        .iter()
        .filter(|h| beside(**h, &c.notes, timing, table, p).is_some())
        .count()
}

/// The section from beat `start` of chart `chart` of `song`, if eight
/// safe shocks fit (`p`), drawn from `seed`.
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
    // `placement-v2`: next to the lane's notes first; where that does not
    // fit, as v1 below.
    if let Some(near) = p.near
        && let Some(s) = section_near(song, chart, table, start, seed, p, near)
    {
        return Some(s);
    }
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
/// checkpoint (every [`PLACEMENT`]'s four beats from beat 0 to the
/// chart's last), `None` where eight safe shocks do not fit (never filled
/// with unsafe ones): pure in the chart, the table and `seed`, so everyone
/// gets the same.
pub fn schedule(song: &Song, chart: usize, table: &JudgeTable, seed: u64) -> Vec<Option<Section>> {
    schedule_with(song, chart, table, seed, &PLACEMENT)
}

/// [`schedule`] by placement `p` (an older version's, to compare).
pub fn schedule_with(
    song: &Song,
    chart: usize,
    table: &JudgeTable,
    seed: u64,
    p: &Placement,
) -> Vec<Option<Section>> {
    let p = *p;
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

/// The longest a horizon is looked for, seconds: past any lead a stage
/// allows (`ddi_room::attacks::LEAD_V0`'s cap).
pub const HORIZON_MAX: f64 = 12.0;

/// How far ahead a player reads (`docs/plans/multiplayer-prototype-progress.md`,
/// M8b, D5): the longest, over `from..=to` (judged song seconds, sampled
/// every half second), that a note is shown before it reaches the
/// receptors under `options` (the scroll's speed and action, Hidden and
/// Sudden, the frame's visible range) on `timing` (its tempo, stops,
/// scroll and speed segments), in seconds, to the next twentieth above;
/// [`HORIZON_MAX`] where a note is shown longer. Lane covers, which change
/// in play, are left out (a cover only shortens it).
pub fn reading_horizon(
    options: &crate::player::PlayOptions,
    timing: &TimingMap,
    from: f64,
    to: f64,
) -> f64 {
    // The farthest a note is shown, in arrow heights.
    let range = options.visible_range.max(0.0);
    let mut reach = range;
    while reach > 0.0 && options.appearance.visibility(reach).0 <= 0.0 {
        reach -= 0.05;
    }
    if reach <= 0.0 || !from.is_finite() || !to.is_finite() {
        return 0.0;
    }
    const STEP: f64 = 0.05;
    let mut longest: f64 = 0.0;
    let mut t = from;
    while t <= to && longest < HORIZON_MAX {
        let state = crate::scroll::ScrollState::at(options.scroll, timing, t);
        let mut ahead = STEP;
        while ahead < HORIZON_MAX {
            let s = t + ahead;
            let y = state.y_displayed(timing.displayed_beat(timing.beat_at(s)), s);
            if y >= reach {
                break;
            }
            ahead += STEP;
        }
        longest = longest.max(ahead.min(HORIZON_MAX));
        t += 0.5;
    }
    longest
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
    /// The first shock whose crossing is not judged yet, where the play
    /// judges crossings itself ([`HazardJudge::next_crossing`]).
    crossed: usize,
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
            crossed: 0,
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

    // Where a shock is a mine (DDI: the lane lock), the play judges the
    // shocks in step with the chart's judge, whose lanes they share: a
    // crossing at its own instant with the lane as the chart's judge has
    // it (a locked lane's key counts as released), a press against the
    // chart's nearest note. The four below are that play's; `press`,
    // `release` and `update` above are the rest's.

    /// The first shock, in time order, whose second is before `t` and
    /// whose crossing is not judged yet: its index and itself.
    pub fn next_crossing(&self, t: f64) -> Option<(usize, HazardMine)> {
        self.mines
            .get(self.crossed)
            .filter(|m| m.seconds < t)
            .map(|m| (self.crossed, *m))
    }

    /// The crossing of shock `index` ([`HazardJudge::next_crossing`]'s)
    /// judged: stepped on at its own second where its lane was `held`
    /// across it and it was not stepped on already.
    pub fn cross(&mut self, index: usize, held: bool) -> Option<HazardHit> {
        self.crossed = self.crossed.max(index + 1);
        let m = self.mines.get_mut(index)?;
        self.last = Some(self.last.map_or(m.seconds, |l| l.max(m.seconds)));
        (held && m.state == HazardState::Pending).then(|| {
            m.state = HazardState::Hit;
            HazardHit {
                index,
                at: m.seconds,
            }
        })
    }

    /// Judged to `t`: the shocks passed by their window unstepped are
    /// avoided.
    pub fn settle(&mut self, t: f64) {
        if !t.is_finite() || self.last.is_some_and(|l| t < l) {
            return;
        }
        self.last = Some(t);
        for m in &mut self.mines[self.first_open..] {
            if m.seconds - t > self.window {
                break;
            }
            if m.state == HazardState::Pending && t > m.seconds + self.window {
                m.state = HazardState::Avoided;
            }
        }
        while self
            .mines
            .get(self.first_open)
            .is_some_and(|m| m.state != HazardState::Pending)
        {
            self.first_open += 1;
        }
    }

    /// The pending shock of `lane` nearest to `t` within its window: its
    /// index and how far, seconds.
    pub fn nearest(&self, lane: u8, t: f64) -> Option<(usize, f64)> {
        (self.first_open..self.mines.len())
            .take_while(|&i| self.mines[i].seconds - t <= self.window)
            .filter(|&i| {
                let m = &self.mines[i];
                m.state == HazardState::Pending
                    && m.lane == lane
                    && (t - m.seconds).abs() <= self.window
            })
            .map(|i| (i, (t - self.mines[i].seconds).abs()))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// Shock `index` stepped on by a press at `t`.
    pub fn step_on(&mut self, index: usize, t: f64) -> Option<HazardHit> {
        let m = self.mines.get_mut(index)?;
        (m.state == HazardState::Pending).then(|| {
            m.state = HazardState::Hit;
            HazardHit { index, at: t }
        })
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
    fn a_reading_horizon_follows_the_speed_the_tempo_and_sudden() {
        let timing = TimingMap::constant(150.0, 0.0);
        let mut o = crate::player::PlayOptions::default();
        o.scroll.speed = crate::scroll::SpeedMod::XMod(1.0);
        // Twelve arrows at x1 and 150 BPM: 4.8 s.
        let h = reading_horizon(&o, &timing, 0.0, 10.0);
        assert!((h - 4.8).abs() <= 0.06, "{h}");
        o.scroll.speed = crate::scroll::SpeedMod::XMod(2.0);
        let h2 = reading_horizon(&o, &timing, 0.0, 10.0);
        assert!((h2 - 2.4).abs() <= 0.06, "{h2}");
        o.scroll.speed = crate::scroll::SpeedMod::CMod(300.0);
        let c = reading_horizon(&o, &timing, 0.0, 10.0);
        assert!((c - 2.4).abs() <= 0.06, "{c}");
        // Sudden shows them later.
        o.appearance = crate::appearance::Appearance::Sudden;
        assert!(reading_horizon(&o, &timing, 0.0, 10.0) < c);
        // Very slow: capped.
        o.appearance = Default::default();
        o.scroll.speed = crate::scroll::SpeedMod::XMod(0.25);
        assert_eq!(reading_horizon(&o, &timing, 0.0, 10.0), HORIZON_MAX);
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
