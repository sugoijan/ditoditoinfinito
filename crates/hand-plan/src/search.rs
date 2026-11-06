// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
#[path = "lifts.rs"]
pub(crate) mod lifts;
#[path = "mines.rs"]
pub(crate) mod mines;
#[path = "rolls.rs"]
pub(crate) mod rolls;
use crate::geometry::{DISTANCE, SHAPES};
use crate::history::History;
use crate::*;
use std::{collections::HashSet, rc::Rc};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub lead: f64,
    pub tail: f64,
    pub beam: usize,
    pub first: Option<i64>,
    pub last: Option<i64>,
    pub hold_tail_grace: f64,
    pub hold_restore_window: f64,
    pub hold_handoff_window: f64,
    pub adaptive_timing: bool,
    pub restore_early_window: f64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            lead: 3.,
            tail: 1.5,
            beam: 256,
            first: None,
            last: None,
            hold_tail_grace: 0.,
            hold_restore_window: 0.,
            hold_handoff_window: 0.,
            adaptive_timing: false,
            restore_early_window: 0.,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Boundary {
    pub tick: i64,
    pub candidates: usize,
    pub unique: usize,
    pub kept: usize,
}
#[derive(Clone, Debug, Serialize)]
pub struct SearchReport {
    pub skipped_roll_tail_refreshes: usize,
    pub beam: usize,
    pub cost: f64,
    pub boundaries: Vec<Boundary>,
    pub finalists: usize,
}
#[derive(Clone)]
struct State {
    panels: [u8; 2],
    last: [f64; 2],
    holds: [f64; 2],
    tracks: [History<Swing>; 2],
    events: History<Event>,
    hold_events: History<Hold>,
    aux: History<Auxiliary>,
    gaps: History<Gap>,
    timing: History<TimingAdjustment>,
    cost: f64,
    heading: f64,
    rank: usize,
    row_feet: [Foot; 2],
    tick: Option<i64>,
    processed: f64,
    boundary: Option<Rc<State>>,
}
impl State {
    fn initial() -> Self {
        Self {
            panels: [5, 5],
            last: [-0.055; 2],
            holds: [0.; 2],
            tracks: Default::default(),
            events: Default::default(),
            hold_events: Default::default(),
            aux: Default::default(),
            gaps: Default::default(),
            timing: Default::default(),
            cost: 0.,
            heading: 0.,
            rank: 0,
            row_feet: [Foot::L; 2],
            tick: None,
            processed: f64::NEG_INFINITY,
            boundary: None,
        }
    }
    fn record(
        &mut self,
        foot: usize,
        previous: f64,
        release: f64,
        land: f64,
        limits: (f64, f64),
        o: Options,
    ) {
        if o.adaptive_timing && limits != (0.055, 0.060) {
            self.timing.push(TimingAdjustment {
                foot: Foot::at(foot),
                previous,
                release,
                land,
                minimum_planted: limits.0,
                minimum_swing: limits.1,
            });
        }
    }
}
/// Frozen row boundary; only recent flights and active original holds survive
/// pruning. Pending returns remain part of the committed state.
#[derive(Clone)]
pub(crate) struct SearchSeed {
    state: State,
    pub release_floor: f64,
    mines: Vec<ddi_hand_pose::Mine>,
    layout: String,
}
impl SearchSeed {
    pub fn initial() -> Self {
        Self {
            state: State::initial(),
            release_floor: 0.,
            mines: Vec::new(),
            layout: default_layout(),
        }
    }
    pub fn tick(&self) -> Option<i64> {
        self.state.tick
    }
    pub fn gaps(&self) -> usize {
        self.state.gaps.backwards().count()
    }
    pub fn timing(&self) -> usize {
        self.state.timing.backwards().count()
    }
    pub fn prune(&mut self, before: f64) {
        fn keep<T: Copy>(h: &mut History<T>, predicate: impl Fn(&T) -> bool) {
            let values = h.values();
            *h = History::default();
            for value in values.into_iter().filter(predicate) {
                h.push(value);
            }
        }
        for track in &mut self.state.tracks {
            let values = track.values();
            let start = values
                .partition_point(|s| s.land < before)
                .saturating_sub(1);
            *track = History::default();
            for &s in &values[start..] {
                track.push(s);
            }
        }
        keep(&mut self.state.events, |e| e.time >= before);
        keep(&mut self.state.hold_events, |h| h.end >= before);
        keep(&mut self.state.aux, |a| a.time >= before);
        keep(&mut self.state.gaps, |g| g.restore >= before);
        keep(&mut self.state.timing, |t| t.land >= before);
        self.mines.retain(|m| m.time + 0.09 >= before);
        self.state.boundary = None;
        self.state.rank = 0;
        self.state.cost = 0.;
    }
    pub fn retained(&self) -> usize {
        self.state
            .tracks
            .iter()
            .map(|h| h.backwards().count())
            .sum::<usize>()
            + self.state.events.backwards().count()
            + self.state.hold_events.backwards().count()
            + self.state.aux.backwards().count()
            + self.state.gaps.backwards().count()
            + self.state.timing.backwards().count()
            + self.mines.len()
    }
}

fn index(p: u8) -> usize {
    match p {
        2 => 0,
        4 => 1,
        5 => 2,
        6 => 3,
        8 => 4,
        7 => 5,
        9 => 6,
        1 => 7,
        3 => 8,
        _ => unreachable!("validated panel"),
    }
}
fn panel(n: &InputNote) -> u8 {
    n.panel
}
fn shape(p: [u8; 2]) -> (f64, f64) {
    SHAPES[index(p[0])][index(p[1])]
}
fn distance(p: u8, old: u8, d: usize) -> f64 {
    DISTANCE[d][index(p)][index(old)]
}
fn square(v: f64) -> f64 {
    v * v
}
fn limits(dt: f64, adaptive: bool) -> (f64, f64) {
    if !adaptive || dt >= 0.115 {
        (0.055, 0.060)
    } else {
        let f = ((dt - 0.045) / 0.070).clamp(0., 1.);
        (0.020 + 0.035 * f, 0.025 + 0.035 * f)
    }
}
fn lift(dist: f64, repeat: bool, dt: f64, adaptive: bool) -> f64 {
    let h = if repeat {
        0.035
    } else {
        0.045_f64.min(0.028 + dist / 1800.)
    };
    if adaptive {
        h * square(dt / 0.14).clamp(0.2, 1.)
    } else {
        h
    }
}
fn overlap(track: &History<Swing>, release: f64, land: f64) -> f64 {
    track
        .suffix(|s| s.land > release)
        .iter()
        .fold(0., |sum, s| {
            sum + (land.min(s.land) - release.max(s.release)).max(0.)
        })
}
fn held(s: &State, d: usize, land: f64) -> Option<Hold> {
    s.hold_events
        .backwards()
        .find(|h| h.panel == s.panels[d] && h.end == s.holds[d] && h.end > land + 0.005)
        .copied()
}
fn hold_event(n: &InputNote, d: usize, land: f64, origin: f64) -> Option<Hold> {
    n.end.map(|(tick, end)| Hold {
        panel: panel(n),
        end_time: end,
        end_tick: tick,
        start: land,
        end: end - origin,
        foot: Foot::at(d),
        roll: n.roll,
    })
}
fn unique(mut v: Vec<f64>) -> Vec<f64> {
    v.sort_by(f64::total_cmp);
    v.dedup();
    v
}
#[derive(Clone, Copy)]
struct Restore {
    penalty: f64,
    release: f64,
    outward: f64,
    restored: f64,
    back: f64,
    end_tick: i64,
}
fn restore_order(r: &Restore) -> [f64; 5] {
    [r.penalty, r.release, r.outward, r.restored, r.back]
}
fn cmp_floats(a: &[f64], b: &[f64]) -> std::cmp::Ordering {
    a.iter()
        .zip(b)
        .find_map(|(a, b)| {
            let c = a.partial_cmp(b).unwrap();
            (c != std::cmp::Ordering::Equal).then_some(c)
        })
        .unwrap_or(std::cmp::Ordering::Equal)
}

/// One checkpoint expands one beam parent (at most four candidates) or sorts
/// one bounded row. No partial plan escapes; yielding preserves candidate order.
pub struct Search {
    skipped_roll_tail_refreshes: usize,
    rows: Vec<Vec<InputNote>>,
    mines: Vec<InputNote>,
    origin: f64,
    options: Options,
    future: Vec<[f64; 9]>,
    states: Vec<State>,
    candidates: Vec<State>,
    row: usize,
    parent: usize,
    counts: Vec<Boundary>,
    result: Option<Result<(ContactPlan, SearchReport), String>>,
    layout: String,
    initial: Initial,
    seed: Option<SearchSeed>,
    commit_time: f64,
    duration: Option<f64>,
}
impl Search {
    pub(crate) fn failure_tick(&self) -> Option<i64> {
        self.result.as_ref()?.as_ref().err()?;
        self.rows.get(self.row)?.first().map(|n| n.tick)
    }
    pub fn new(input: &Input, o: Options) -> Result<Self, String> {
        Self::new_with_rolls(input, o, None, None)
    }
    fn new_with_rolls(
        input: &Input,
        o: Options,
        seed: Option<&SearchSeed>,
        known: Option<f64>,
    ) -> Result<Self, String> {
        let lanes = layout_lanes(&input.layout)?;
        if !o.lead.is_finite()
            || o.lead < 0.
            || !o.tail.is_finite()
            || o.tail < 0.
            || o.beam == 0
            || o.beam > 4096
            || [
                o.hold_tail_grace,
                o.hold_restore_window,
                o.hold_handoff_window,
            ]
            .iter()
            .any(|v| !v.is_finite() || !(0. ..=0.22).contains(v))
            || !o.restore_early_window.is_finite()
            || !(0. ..=0.25).contains(&o.restore_early_window)
        {
            return Err("invalid planner options".into());
        }
        let first = o.first.unwrap_or(i64::MIN);
        let last = o.last.unwrap_or(i64::MAX);
        let mut notes = input
            .notes
            .iter()
            .copied()
            .filter(|n| {
                n.tick >= first
                    && n.tick <= last
                    && n.judgeable
                    && seed.is_none_or(|s| s.tick().is_none_or(|t| n.tick > t))
            })
            .collect::<Vec<_>>();
        let rolling = input.notes.iter().any(|n| n.roll && n.judgeable);
        let mut skipped_roll_tail_refreshes = 0;
        if rolling {
            let (refreshes, skipped) = rolls::refreshes(
                input,
                known.unwrap_or(f64::INFINITY),
                seed.map_or(f64::NEG_INFINITY, |s| s.state.processed),
                o.adaptive_timing,
            )?;
            skipped_roll_tail_refreshes = skipped;
            notes.extend(refreshes.into_iter().filter(|n| {
                n.tick >= first && n.tick <= last && seed.is_none_or(|s| n.time > s.state.processed)
            }));
            notes.sort_by(|a, b| {
                a.time
                    .total_cmp(&b.time)
                    .then(a.roll_refresh.cmp(&b.roll_refresh))
                    .then(a.tick.cmp(&b.tick))
                    .then(a.lane.cmp(&b.lane))
            });
        }
        if notes.is_empty() {
            if seed.is_some() {
                return Ok(Self::with_rows(Vec::new(), 0., o));
            }
            return Err("No judgeable notes".into());
        }
        if notes.len() > 10000 {
            return Err("plan note limit".into());
        }
        if notes.iter().any(|n| !n.supported) {
            return Err("This planner supports taps, holds, rolls, mines, shocks and lifts only; unsupported notes are never discarded silently".into());
        }
        if notes.iter().any(|n| {
            n.lane as usize >= lanes
                || layout_panels(&input.layout).unwrap().get(n.lane as usize) != Some(&n.panel)
                || !n.time.is_finite()
                || n.end.is_some_and(|(_, e)| !e.is_finite())
        }) {
            return Err("invalid chart timing or lane".into());
        }
        if input.notes.iter().any(|n| {
            n.tick < first
                && n.end.is_some_and(|(_, end)| {
                    end != 0. && end >= notes.iter().map(|n| n.time).fold(f64::INFINITY, f64::min)
                })
        }) {
            return Err("Passage starts inside a hold".into());
        }
        let mut rows: Vec<Vec<InputNote>> = Vec::new();
        for n in notes {
            if rows.last().is_none_or(|r| {
                if rolling {
                    r[0].time != n.time
                } else {
                    r[0].tick != n.tick
                }
            }) {
                rows.push(Vec::new());
            }
            rows.last_mut().unwrap().push(n);
            if rows.last().unwrap().iter().filter(|n| !n.mine).count() > 2 {
                return Err("More than two simultaneous contacts".into());
            }
        }
        let origin = if seed.is_some() {
            0.
        } else {
            rows[0][0].time - o.lead
        };
        let mines = rows.iter().flatten().filter(|n| n.mine).copied().collect();
        let mut search = Self::with_rows(rows, origin, o);
        search.layout = input.layout.clone();
        search.skipped_roll_tail_refreshes = skipped_roll_tail_refreshes;
        search.mines = mines;
        Ok(search)
    }
    fn legal(&self, heading: f64, span: f64) -> bool {
        // These layouts include the 50.8 mm opposite-corner stance and
        // reversed escape stances. Actual reach and rotation remain
        // subject to the unchanged bounded pose solver and contact checks.
        if crate::wide_stances(&self.layout) {
            (8. ..=51.).contains(&span)
        } else {
            (8. ..=48.).contains(&span) && heading.abs() <= 76.
        }
    }
    fn facing_cost(&self, heading: f64) -> f64 {
        // Keep broad turns available, but prefer the reference-facing family
        // over unnecessarily crossing the fingers through a reversed stance.
        if crate::wide_stances(&self.layout) {
            square(heading / 45.) * 4.
        } else {
            0.
        }
    }
    fn with_rows(rows: Vec<Vec<InputNote>>, origin: f64, o: Options) -> Self {
        let mut next = [f64::INFINITY; 9];
        let mut future = vec![next; rows.len()];
        if o.restore_early_window > 0. {
            for i in (0..rows.len()).rev() {
                future[i] = next;
                for n in &rows[i] {
                    if !n.mine && !n.roll_refresh {
                        next[index(panel(n))] = n.time - origin;
                    }
                }
            }
        }
        Self {
            skipped_roll_tail_refreshes: 0,
            rows,
            mines: Vec::new(),
            layout: default_layout(),
            origin,
            options: o,
            future,
            states: vec![State::initial()],
            candidates: Vec::new(),
            row: 0,
            parent: 0,
            counts: Vec::new(),
            result: None,
            initial: Initial { left: 5, right: 5 },
            seed: None,
            commit_time: f64::NEG_INFINITY,
            duration: None,
        }
    }
    pub(crate) fn seeded(
        input: &Input,
        mut o: Options,
        seed: &SearchSeed,
        commit_time: f64,
        known: f64,
    ) -> Result<Self, String> {
        let mut future = input.clone();
        future
            .notes
            .retain(|n| seed.tick().is_none_or(|t| n.tick > t));
        let first = future.notes.iter().find(|n| n.judgeable);
        o.lead = first.map_or(0., |n| n.time);
        o.first = None;
        o.last = None;
        let mut s = if input.notes.iter().any(|n| n.roll && n.judgeable) {
            Self::new_with_rolls(input, o, Some(seed), Some(known))?
        } else if first.is_some() {
            Self::new(&future, o)?
        } else {
            Self::with_rows(Vec::new(), 0., o)
        };
        let mut state = seed.state.clone();
        state.cost = 0.;
        state.rank = 0;
        state.boundary = None;
        s.mines = input
            .notes
            .iter()
            .filter(|n| n.judgeable && n.mine)
            .copied()
            .collect();
        s.initial = Initial {
            left: state.tracks[0]
                .values()
                .first()
                .map_or(state.panels[0], |s| s.from),
            right: state.tracks[1]
                .values()
                .first()
                .map_or(state.panels[1], |s| s.from),
        };
        s.layout = input.layout.clone();
        s.states = vec![state];
        s.seed = Some(seed.clone());
        s.commit_time = commit_time;
        s.duration = Some(known);
        if s.rows.is_empty() {
            s.result = Some(Ok(s.build()));
        }
        Ok(s)
    }
    fn release_floor(&self) -> f64 {
        self.seed
            .as_ref()
            .map_or(f64::NEG_INFINITY, |s| s.release_floor.next_up())
    }
    fn capture(&self, mut s: State, tick: Option<i64>, land: f64) -> State {
        s.tick = tick.or(s.tick);
        s.processed = land + self.origin;
        if land <= self.commit_time {
            let mut boundary = s.clone();
            boundary.boundary = None;
            s.boundary = Some(Rc::new(boundary));
        }
        s
    }
    pub(crate) fn boundary(&self, known: f64, finished: bool) -> Option<SearchSeed> {
        self.result.as_ref()?.as_ref().ok()?;
        let best = &self.states[0];
        let mut seed = self.seed.clone()?;
        if let Some(boundary) = &best.boundary {
            seed.state = (**boundary).clone();
        }
        let mut until = if finished { known } else { known - 0.45 };
        // EOF can announce every head while a declared roll still extends past
        // this watermark. Leave room for the next refresh's flight.
        if best
            .hold_events
            .backwards()
            .any(|h| h.roll && h.end > known)
        {
            until = until.min(known - 0.180);
        }
        for d in 0..2 {
            let committed = seed.state.tracks[d].backwards().count();
            until = best.tracks[d]
                .values()
                .iter()
                .skip(committed)
                .map(|s| s.release - 1e-8)
                .fold(until, f64::min);
        }
        // Leave the full existing 450 ms flight choice available for every
        // mutable head, even if this candidate chose a shorter lift. Otherwise
        // a later window could force a newly preferred assignment to rush.
        until = best
            .events
            .backwards()
            .filter(|e| seed.tick().is_none_or(|t| e.tick > t))
            .map(|e| e.time - 0.45 - 1e-8)
            .fold(until, f64::min);
        seed.release_floor = until.max(seed.release_floor);
        seed.layout = self.layout.clone();
        seed.mines = self
            .mines
            .iter()
            .map(|m| ddi_hand_pose::Mine {
                time: m.time - self.origin,
                panel: panel(m),
            })
            .collect();
        Some(seed)
    }
    pub(crate) fn committed_plan(seed: &SearchSeed, known: f64) -> ContactPlan {
        let mut s = Self::with_rows(Vec::new(), 0., Options::default());
        s.initial = Initial {
            left: seed.state.tracks[0]
                .values()
                .first()
                .map_or(seed.state.panels[0], |s| s.from),
            right: seed.state.tracks[1]
                .values()
                .first()
                .map_or(seed.state.panels[1], |s| s.from),
        };
        s.layout = seed.layout.clone();
        s.states = vec![seed.state.clone()];
        s.duration = Some(known);
        s.seed = Some(seed.clone());
        let mut p = s.build().0;
        p.mines = seed.mines.clone();
        p
    }
    fn allowed(&self, p: u8, t: f64) -> bool {
        self.options.restore_early_window == 0.
            || self.future[self.row][index(p)] - t > self.options.restore_early_window + 1e-9
    }
    fn restore(
        &self,
        old: &State,
        d: usize,
        n: &InputNote,
        land: f64,
        next: f64,
        ideal: f64,
    ) -> Option<Restore> {
        let o = self.options;
        let foot = Foot::at(d);
        let source = old.panels[d];
        // Ownership is carried state, independent of permission to introduce
        // a new handoff in this retry tier. The original head keeps its foot.
        let h = held(old, d, land).or_else(|| {
            old.hold_events
                .backwards()
                .find(|h| h.foot == foot)
                .copied()
        })?;
        if h.panel != source || h.end <= land || (n.end.is_some() && panel(n) != source) {
            return None;
        }
        let (plant, swing) = self.departure_limits(old, d, land);
        let (back_plant, back_swing) =
            limits((next - 0.01).min(h.end - 0.005) - land, o.adaptive_timing);
        let mut options = Vec::new();
        for outward in unique(vec![swing, 0.06, 0.08, 0.10, 0.14]) {
            let release = land - outward;
            if release < self.release_floor()
                || release < old.last[d] + plant - 1e-9
                || overlap(&old.tracks[1 - d], release, land) > 0.145
            {
                continue;
            }
            for back in if panel(n) == source {
                vec![0.]
            } else {
                unique(vec![back_swing, 0.06, 0.08, 0.10])
            } {
                let restored = if back == 0. {
                    land
                } else {
                    land + back_plant + back
                };
                let gap = restored - release;
                if gap > o.hold_restore_window + 1e-9
                    || restored > next - 0.01
                    || restored >= h.end - 0.005
                    || (back != 0. && !self.allowed(source, restored))
                {
                    continue;
                }
                let mut penalty = square((outward - ideal) / 0.10) * 0.35
                    + 2.
                    + gap / o.hold_restore_window * 0.5;
                if back != 0. {
                    penalty += square((back - ideal) / 0.10) * 0.35;
                }
                options.push(Restore {
                    penalty,
                    release,
                    outward,
                    restored,
                    back,
                    end_tick: h.end_tick,
                });
            }
        }
        options.into_iter().min_by(|a, b| {
            cmp_floats(&restore_order(a), &restore_order(b)).then(a.end_tick.cmp(&b.end_tick))
        })
    }
    fn handoff(&self, old: &State, n: &InputNote, land: f64) -> Vec<State> {
        let o = self.options;
        let mut out = Vec::new();
        for d in 0..2 {
            let other = 1 - d;
            let Some(h) = held(old, d, land) else {
                continue;
            };
            if panel(n) == h.panel || !self.allowed(h.panel, land) {
                continue;
            }
            let ls = limits(
                old.last
                    .iter()
                    .map(|t| land - t)
                    .fold(f64::INFINITY, f64::min),
                o.adaptive_timing,
            );
            let earliest = (old.last[d] + ls.0)
                .max(old.last[other] + ls.0)
                .max(old.holds[other] + if old.holds[other] > 0. { 0.01 } else { 0. });
            let earliest = earliest.max(self.release_floor());
            let duration = 0.14_f64.min(o.hold_handoff_window).min(land - earliest);
            if duration < ls.1 - 1e-9 {
                continue;
            }
            let release = land - duration;
            let mut panels = old.panels;
            panels[d] = panel(n);
            panels[other] = h.panel;
            let (heading, span) = shape(panels);
            if !self.legal(heading, span) {
                continue;
            }
            let mut s = old.clone();
            let mut travel = 0.;
            for (side, &p) in panels.iter().enumerate() {
                let dist = distance(p, old.panels[side], side);
                travel += square(dist / 18.) * 0.65;
                s.tracks[side].push(Swing {
                    release,
                    land,
                    from: old.panels[side],
                    to: p,
                    lift_m: lift(dist, false, duration, o.adaptive_timing),
                });
                s.record(side, old.last[side], release, land, ls, o);
            }
            s.events.push(Event {
                lift: false,
                time: land,
                music_time: n.time,
                tick: n.tick,
                panel: panel(n),
                foot: Foot::at(d),
            });
            s.row_feet = [Foot::at(d), Foot::L];
            s.aux.push(Auxiliary {
                time: land,
                foot: Foot::at(other),
                panel: h.panel,
                kind: RestoreKind::HoldRestore,
                end_tick: h.end_tick,
            });
            s.gaps.push(Gap {
                release,
                restore: land,
                foot: Foot::at(d),
                restore_foot: Some(Foot::at(other)),
                panel: h.panel,
                end_tick: h.end_tick,
            });
            s.panels = panels;
            s.last = [land; 2];
            s.holds[other] = h.end;
            s.holds[d] = n.end.map_or(0., |(_, end)| end - self.origin);
            if let Some(h) = hold_event(n, d, land, self.origin) {
                s.hold_events.push(h);
            }
            s.heading = heading;
            s.cost += travel
                + 2.5
                + square(duration / o.hold_handoff_window) * 0.5
                + square((duration - 0.14) / 0.10) * 0.35
                + square((span - 32.).max(0.)) * 0.006
                + square((heading - old.heading) / 45.) * 0.25
                + self.facing_cost(heading);
            out.push(s);
        }
        out
    }
    fn wide(panels: [u8; 2]) -> bool {
        let [a, b] =
            panels.map(|p| ddi_hand_pose::panel_position(p, 24.).expect("validated panel"));
        (a[0] - b[0]).hypot(a[1] - b[1]) > 60.
    }
    fn support_step(
        &self,
        old: &State,
        head: &InputNote,
        d: usize,
        target: u8,
        land: f64,
    ) -> Option<State> {
        let other = 1 - d;
        if old.holds[other] > land || old.last[other] >= land || old.panels[other] == target {
            return None;
        }
        let ls = self.departure_limits(old, other, land);
        let dist = distance(target, old.panels[other], other);
        let duration = 0.45;
        let release = (land - duration)
            .max(old.last[other] + ls.0)
            .max(self.release_floor())
            .max(old.holds[other]);
        if land - release < ls.1 || !self.allowed(target, land) {
            return None;
        }
        let mut s = old.clone();
        s.tracks[other].push(Swing {
            release,
            land,
            from: old.panels[other],
            to: target,
            lift_m: self.clearance_lift(
                old.panels[other],
                target,
                release,
                land,
                lift(dist, false, land - release, self.options.adaptive_timing),
            )?,
        });
        s.aux.push(Auxiliary {
            time: land,
            foot: Foot::at(other),
            panel: target,
            kind: RestoreKind::SupportStep,
            end_tick: head.tick,
        });
        s.record(other, old.last[other], release, land, ls, self.options);
        s.panels[other] = target;
        s.last[other] = land;
        s.cost += 0.5 + square(dist / 18.) * 0.65;
        Some(s)
    }
    // A shared roll or rapid repeat needs the companion off its button. With every tile scored,
    // choose a temporary rest only after checking known hazards and nearby heads.
    fn companion_rest(
        &self,
        old: &State,
        d: usize,
        source: u8,
        release: f64,
        head: f64,
    ) -> Option<(u8, f64, f64)> {
        let mut candidates = Vec::new();
        for panel in 1..=9 {
            if panel == source {
                continue;
            }
            let mut land = head + 0.060;
            for mine in self.mines.iter().filter(|m| self::panel(m) == panel) {
                let time = mine.time - self.origin;
                if time + 0.095 >= land && time - 0.095 <= land + 0.055 {
                    land = land.max(time + 0.170);
                }
            }
            if land > head + 0.4
                || self.rows.iter().skip(self.row + 1).flatten().any(|n| {
                    !n.mine
                        && !n.roll_refresh
                        && self::panel(n) == panel
                        && (n.time - self.origin - land).abs() <= 0.25
                })
            {
                continue;
            }
            let mut panels = old.panels;
            panels[d] = panel;
            let (heading, span) = shape(panels);
            if !self.legal(heading, span) {
                continue;
            }
            let height = self.clearance_lift(source, panel, release, land, 0.035);
            if let Some(height) = height {
                let cost =
                    square(distance(panel, source, d) / 18.) * 0.65 + self.facing_cost(heading);
                candidates.push((cost, panel, land, height));
            }
        }
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        candidates.first().map(|&(_, p, t, h)| (p, t, h))
    }
    fn ordinary(
        &self,
        old: &State,
        group: &[InputNote],
        assignment: [usize; 2],
        land: f64,
        next: f64,
        direct_lifts: bool,
    ) -> Option<State> {
        let o = self.options;
        let mut s = old.clone();
        s.row_feet = [Foot::L; 2];
        let mut cost = old.cost;
        let mut landing = old.panels;
        let mut returns = Vec::new();
        let lift_jump = crate::wide_stances(&self.layout)
            && group.len() == 2
            && group.iter().all(|n| n.lift)
            && Self::wide([panel(&group[0]), panel(&group[1])])
            && group
                .iter()
                .enumerate()
                .any(|(i, n)| old.panels[assignment[i]] != panel(n));
        let support_jump = crate::wide_stances(&self.layout)
            && old.aux.backwards().any(|a| {
                a.kind == RestoreKind::SupportStep
                    && a.time == land
                    && group.iter().any(|n| n.tick == a.end_tick)
            });
        for (i, n) in group.iter().enumerate() {
            let d = assignment[i];
            let other = 1 - d;
            let target = panel(n);
            if n.roll_refresh {
                let prior = s.cost;
                self.roll_contact(&mut s, n, d, land)?;
                s.row_feet[i] = Foot::at(d);
                cost += s.cost - prior;
                continue;
            }
            if n.lift {
                let prior = s.cost;
                self.lift_contact(&mut s, n, d, land, next, direct_lifts, lift_jump)?;
                s.row_feet[i] = Foot::at(d);
                landing[d] = s.panels[d];
                cost += s.cost - prior;
                continue;
            }
            let roll_repeat = old.panels[d] == target
                && old
                    .hold_events
                    .backwards()
                    .any(|h| h.roll && h.panel == target && h.end == old.holds[d]);
            let held_at_hit = old.holds[d] - o.hold_tail_grace > land - 1e-8 && !roll_repeat;
            if held_at_hit && o.hold_restore_window == 0. {
                return None;
            }
            if old.panels[other] == target
                && !assignment[..group.len()].contains(&other)
                && !(self.layout == "danoni-9B"
                    && old.tracks[other]
                        .backwards()
                        .any(|s| s.release <= land && s.land > land))
            {
                if !crate::scored_center(&self.layout) {
                    return None;
                }
                // A shared scored tile needs both fingers lifted for a new
                // aggregate press. The companion rejoins after the exact hit.
                let release = land - 0.100;
                if release < self.release_floor()
                    || release < old.last[other] + 0.055
                    || old.holds[other] > release
                {
                    return None;
                }
                let clear = if self.layout == "danoni-7" && n.end.is_some() && !n.roll {
                    // Keep an already-shared freeze supported by either finger,
                    // as on five keys. A roll still needs aggregate releases.
                    target
                } else if let Some(stance) = crate::escape_stances(&self.layout).first() {
                    if crate::wide_stances(&self.layout) {
                        stance[other]
                    } else if n.roll {
                        if other == 0 { 4 } else { 6 }
                    } else {
                        target
                    }
                } else if n.roll {
                    if other == 0 { 4 } else { 6 }
                } else {
                    target
                };
                let (clear, rejoin, height) = if self.layout == "danoni-9B"
                    && (n.roll
                        || (n.end.is_none()
                            && self.rows.iter().skip(self.row + 1).flatten().any(|head| {
                                !head.mine
                                    && !head.lift
                                    && !head.roll_refresh
                                    && panel(head) == target
                                    && head.time - n.time < 0.215
                            }))) {
                    self.companion_rest(old, other, target, release, land)?
                } else {
                    (clear, land + 0.060, 0.035)
                };
                s.tracks[other].push(Swing {
                    release,
                    land: rejoin,
                    from: target,
                    to: clear,
                    lift_m: height,
                });
                s.aux.push(Auxiliary {
                    time: rejoin,
                    foot: Foot::at(other),
                    panel: clear,
                    kind: RestoreKind::CompanionHop,
                    end_tick: n.tick,
                });
                s.panels[other] = clear;
                landing[other] = clear;
                s.last[other] = rejoin;
                cost += 0.5;
            }
            if old.last[d] == land
                && old.panels[d] == target
                && old.aux.backwards().any(|a| {
                    a.foot.index() == d
                        && a.time == land
                        && matches!(a.kind, RestoreKind::MineAvoid | RestoreKind::LiftClear)
                })
            {
                // A hazard or scored-lift flight can already end on this head.
                s.events.push(Event {
                    lift: false,
                    time: land,
                    music_time: n.time,
                    tick: n.tick,
                    panel: target,
                    foot: Foot::at(d),
                });
                s.row_feet[i] = Foot::at(d);
                if let Some(h) = hold_event(n, d, land, self.origin) {
                    s.holds[d] = h.end;
                    s.hold_events.push(h);
                }
                continue;
            }
            let dist = distance(target, old.panels[d], d);
            let repeat = old.panels[d] == target;
            let ls = self.departure_limits(old, d, land);
            let earliest = (old.last[d] + ls.0).max(if roll_repeat {
                f64::NEG_INFINITY
            } else {
                old.holds[d] + if old.holds[d] > 0. { 0.01 } else { 0. } - o.hold_tail_grace
            });
            let earliest = earliest.max(self.release_floor());
            let available = land - earliest;
            let ideal = if repeat {
                0.14
            } else {
                0.45_f64.min(0.20 + dist / 140.)
            };
            let restore = if held_at_hit || available < ls.1 - 1e-9 {
                if o.hold_restore_window == 0. {
                    return None;
                }
                Some(self.restore(old, d, n, land, next, ideal)?)
            } else {
                None
            };
            let durations = unique(
                [ideal, 0.14, 0.20, 0.28, 0.36, 0.45]
                    .into_iter()
                    .map(|dt| available.min(dt))
                    .filter(|dt| *dt >= ls.1 - if o.adaptive_timing { 1e-9 } else { 0. })
                    .collect(),
            );
            let mut options = Vec::new();
            for duration in durations {
                let release = land - duration;
                let air = overlap(&old.tracks[other], release, land);
                if air > if support_jump { 0.45 } else { 0.145 } {
                    continue;
                }
                let mut penalty =
                    square((duration - ideal) / 0.10) * 0.35 + square(air / 0.14) * 0.25;
                if o.hold_tail_grace != 0. {
                    let early = (old.holds[d] - release).max(0.);
                    penalty += square(early / 0.10) * 2.;
                }
                if repeat && duration < 0.14 {
                    penalty += square((0.14 - duration) / 0.04) * 2.;
                }
                if let Some(height) = self.clearance_lift(
                    old.panels[d],
                    target,
                    release,
                    land,
                    lift(dist, repeat, duration, o.adaptive_timing),
                ) {
                    options.push([penalty, release, duration, height]);
                }
            }
            let [penalty, release, _duration, height] = if let Some(r) = restore {
                let height = self.clearance_lift(
                    old.panels[d],
                    target,
                    r.release,
                    land,
                    lift(dist, repeat, r.outward, o.adaptive_timing),
                )?;
                [r.penalty, r.release, r.outward, height]
            } else {
                options.into_iter().min_by(|a, b| cmp_floats(a, b))?
            };
            s.tracks[d].push(Swing {
                release,
                land,
                from: old.panels[d],
                to: target,
                lift_m: height,
            });
            s.record(d, old.last[d], release, land, ls, o);
            s.events.push(Event {
                lift: false,
                time: land,
                music_time: n.time,
                tick: n.tick,
                panel: target,
                foot: Foot::at(d),
            });
            s.row_feet[i] = Foot::at(d);
            s.panels[d] = target;
            landing[d] = target;
            s.last[d] = land;
            if let Some(r) = restore {
                s.gaps.push(Gap {
                    foot: Foot::at(d),
                    panel: old.panels[d],
                    release,
                    restore: r.restored,
                    end_tick: r.end_tick,
                    restore_foot: None,
                });
                if r.back != 0. {
                    s.tracks[d].push(Swing {
                        release: if o.adaptive_timing {
                            r.restored - r.back
                        } else {
                            land + 0.055
                        },
                        land: r.restored,
                        from: target,
                        to: old.panels[d],
                        lift_m: lift(dist, repeat, r.back, o.adaptive_timing),
                    });
                    let back_ls = limits(
                        (next - 0.01).min(old.holds[d] - 0.005) - land,
                        o.adaptive_timing,
                    );
                    s.record(d, land, r.restored - r.back, r.restored, back_ls, o);
                    s.aux.push(Auxiliary {
                        time: r.restored,
                        foot: Foot::at(d),
                        panel: old.panels[d],
                        kind: RestoreKind::HoldRestore,
                        end_tick: r.end_tick,
                    });
                    s.panels[d] = old.panels[d];
                    s.last[d] = r.restored;
                    returns.push((r.restored, d, old.panels[d]));
                }
            }
            if let Some(h) = hold_event(n, d, land, self.origin) {
                s.holds[d] = h.end;
                s.hold_events.push(h);
            }
            cost += square(dist / 18.) * 0.65 + penalty;
        }
        if crate::wide_stances(&self.layout)
            && group.len() == 1
            && !group[0].lift
            && !group[0].roll_refresh
            && (Self::wide(old.panels) || Self::wide(landing))
        {
            cost += 20.;
        }
        let (mut heading, span) = shape(landing);
        if !self.legal(heading, span) {
            return None;
        }
        cost += square((span - 32.).max(0.)) * 0.006
            + square((heading - old.heading) / 45.) * 0.25
            + self.facing_cost(heading);
        returns.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        let mut i = 0;
        while i < returns.len() {
            let time = returns[i].0;
            while i < returns.len() && returns[i].0 == time {
                landing[returns[i].1] = returns[i].2;
                i += 1;
            }
            let (h, span) = shape(landing);
            if !self.legal(h, span) {
                return None;
            }
            cost += square((h - heading) / 45.) * 0.25 + self.facing_cost(h);
            heading = h;
        }
        s.cost = cost;
        s.heading = heading;
        Some(s)
    }
    /// Advance one checkpoint; true means finished, not merely that work ran.
    pub fn step(&mut self) -> bool {
        if self.result.is_some() {
            return true;
        }
        let group = &self.rows[self.row];
        let land = group[0].time - self.origin;
        let source_tick = group
            .iter()
            .filter(|n| !n.roll_refresh)
            .map(|n| n.tick)
            .max();
        let next = self
            .rows
            .get(self.row + 1)
            .map_or(f64::INFINITY, |g| g[0].time - self.origin);
        if self.parent < self.states.len() {
            let heads: std::borrow::Cow<'_, [InputNote]> = if self.mines.is_empty() {
                std::borrow::Cow::Borrowed(group)
            } else {
                std::borrow::Cow::Owned(group.iter().copied().filter(|n| !n.mine).collect())
            };
            let hazards = &group[..];
            let old = &self.states[self.parent];
            if self.options.hold_handoff_window > 0.
                && heads.len() == 1
                && !heads[0].lift
                && !heads[0].roll_refresh
            {
                let extra = self.handoff(old, &heads[0], land);
                let extra = extra
                    .into_iter()
                    .flat_map(|s| self.hazard_candidates(s, hazards, land))
                    .filter(|s| self.mine_safe(s, land))
                    .map(|s| self.capture(s, source_tick, land))
                    .collect::<Vec<_>>();
                self.candidates.extend(extra);
            }
            for assignment in [[0, 1], [1, 0]] {
                for direct_lifts in [false, true] {
                    if direct_lifts && !heads.iter().any(|n| n.lift) {
                        break;
                    }
                    if crate::wide_stances(&self.layout)
                        && heads.len() == 1
                        && !heads[0].lift
                        && !heads[0].roll_refresh
                    {
                        let mut landing = old.panels;
                        landing[assignment[0]] = panel(&heads[0]);
                        if Self::wide(old.panels) || Self::wide(landing) {
                            for &target in crate::support_tiles(&self.layout) {
                                if let Some(s) = self
                                    .support_step(old, &heads[0], assignment[0], target, land)
                                    .and_then(|s| {
                                        self.ordinary(
                                            &s,
                                            &heads,
                                            assignment,
                                            land,
                                            next,
                                            direct_lifts,
                                        )
                                    })
                                {
                                    for s in self.hazard_candidates(s, hazards, land) {
                                        if self.mine_safe(&s, land) {
                                            let s = self.capture(s, source_tick, land);
                                            self.candidates.push(s);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if let Some(s) =
                        self.ordinary(old, &heads, assignment, land, next, direct_lifts)
                    {
                        for s in self.hazard_candidates(s, hazards, land) {
                            if self.mine_safe(&s, land) {
                                let s = self.capture(s, source_tick, land);
                                self.candidates.push(s);
                            }
                        }
                    }
                }
                if heads.is_empty() {
                    break;
                }
            }
            self.parent += 1;
            return false;
        }
        // rank is the lexical order of the entire prior foot sequence. Append
        // this row without copying that sequence into every candidate.
        self.candidates.sort_by(|a, b| {
            a.cost
                .partial_cmp(&b.cost)
                .unwrap()
                .then(a.rank.cmp(&b.rank))
                .then(a.row_feet.cmp(&b.row_feet))
        });
        let count = self.candidates.len();
        let mut seen = HashSet::new();
        let mut states = Vec::new();
        for s in self.candidates.drain(..) {
            let bits = |v: f64| if v == 0. { 0 } else { v.to_bits() };
            let mut key = s.panels.map(u64::from).to_vec();
            key.extend(s.last.map(bits));
            key.extend(s.holds.map(bits));
            // Exactly the reference's recent-flight boundary: older history
            // is shared for output, but does not distinguish equivalent states.
            for d in 0..2 {
                for swing in s.tracks[d].suffix(|s| s.land >= land - 0.45) {
                    key.extend([d as u64, bits(swing.release), bits(swing.land)]);
                }
            }
            if seen.insert(key) {
                states.push(s);
            }
        }
        let unique = states.len();
        states.truncate(self.options.beam);
        let mut orders = states
            .iter()
            .map(|s| (s.rank, s.row_feet))
            .collect::<Vec<_>>();
        orders.sort();
        orders.dedup();
        for s in &mut states {
            s.rank = orders.binary_search(&(s.rank, s.row_feet)).unwrap();
        }
        if states.is_empty() {
            self.result = Some(Err(format!(
                "No supported contact plan at tick {}",
                group[0].tick
            )));
            return true;
        }
        self.counts.push(Boundary {
            tick: group[0].tick,
            candidates: count,
            unique,
            kept: states.len(),
        });
        self.states = states;
        self.row += 1;
        self.parent = 0;
        if self.row == self.rows.len() {
            self.result = Some(Ok(self.build()));
            return true;
        }
        false
    }
    fn build(&self) -> (ContactPlan, SearchReport) {
        let s = &self.states[0];
        let o = self.options;
        let holds = s.hold_events.values();
        let events = s.events.values();
        let duration = events
            .iter()
            .map(|e| e.time)
            .fold(f64::NEG_INFINITY, f64::max)
            .max(holds.iter().map(|h| h.end).fold(0., f64::max))
            + o.tail;
        let duration = if self.mines.is_empty() {
            duration
        } else {
            duration.max(self.mines.last().unwrap().time - self.origin + o.tail)
        };
        let duration = self.duration.map_or(duration, |known| {
            s.tracks
                .iter()
                .flat_map(|h| h.backwards().map(|s| s.land))
                .fold(known, f64::max)
        });
        let extras = s.aux.backwards().next().is_some()
            || holds.iter().any(|h| h.roll)
            || events.iter().any(|e| e.lift)
            || !self.mines.is_empty()
            || self.seed.is_some()
            || o.hold_restore_window > 0.
            || o.hold_handoff_window > 0.;
        let plan = ContactPlan {
            layout: self.layout.clone(),
            version: 1,
            scope: if self.mines.is_empty() {"Automatic bounded-beam planning from imported chart taps/holds. No manual assignments, lift timestamps or accepted motion inputs."} else {"Automatic bounded-beam planning of taps/holds with explicit mine-clearance contacts."}.into(),
            initial: self.initial.clone(),
            placement: Placement {
                tile_pitch_mm: 24., panel_anchor_mm: 15.,
                center_offset_mm: 9., other_offset_mm: 5.5,
            },
            tracks: Tracks { left: s.tracks[0].values(), right: s.tracks[1].values() },
            events,
            source_music_origin_seconds: self.origin,
            duration,
            holds,
            mines: self.mines.iter().map(|m|ddi_hand_pose::Mine{time:m.time-self.origin,panel:panel(m)}).collect(),
            roll_pulses: rolls::pulses(&s.hold_events.values(), &[s.tracks[0].values(),s.tracks[1].values()]),
            aux_contacts: extras.then(|| s.aux.values()),
            hold_gaps: extras.then(|| s.gaps.values()),
            timing_adjustments: (self.seed.is_some() || o.adaptive_timing).then(|| s.timing.values()),
            restore_early_window_seconds: (o.restore_early_window > 0.).then_some(o.restore_early_window),
        };
        let report = SearchReport {
            skipped_roll_tail_refreshes: self.skipped_roll_tail_refreshes,
            beam: o.beam,
            cost: s.cost,
            boundaries: self.counts.clone(),
            finalists: self.states.len(),
        };
        (plan, report)
    }

    pub fn finish(self) -> Option<Result<(ContactPlan, SearchReport), String>> {
        self.result
    }
    pub fn row(&self) -> usize {
        self.row
    }
}
