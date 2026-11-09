// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use crate::replay::ReplayJob;
use crate::search::SearchSeed;
use crate::window_replay::{Checkpoint, WindowReplay};
use crate::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    StrictContactPass,
    JudgeRecovered,
    ContactFailure,
    Unsupported,
    ContactTimeout,
    ContactJudgeFailure,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct Attempt {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowance_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restore_window_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handoff_window_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adaptive_timing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub staggered_landings: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum_swing_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restore_early_window_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub judge: Option<ReplayReport>,
}
#[derive(Clone, Debug, Serialize)]
pub struct PlanningReport {
    pub status: Status,
    /// The last rejected search row, absent for timeouts and judge failures.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_tick: Option<i64>,
    pub attempts: Vec<Attempt>,
    pub search_seconds: f64,
    pub checkpoints: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adaptive_gate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowance_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restore_window_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handoff_window_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adaptive_timing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auxiliary_restores: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interior_gaps: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handoffs: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum_interior_gap_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum_handoff_gap_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing_adjustments: Option<usize>,
    pub search: Option<SearchReport>,
    pub judge: Option<ReplayReport>,
}
enum Work {
    Begin,
    Search(Box<Search>),
    BeginReplay,
    Replay(Box<ReplayJob>),
    WindowReplay(Box<WindowReplay>),
    Done,
}
/// Ordered recovery policy, with one shared active-search cap. Hosts inject
/// measured work time (native CPU for audit, monotonic active time in WASM).
/// No work time elapses merely because a job waits for the next frame.
#[derive(Clone)]
pub(crate) struct RollingContext {
    pub seed: SearchSeed,
    pub checkpoint: Checkpoint,
    pub known: f64,
    pub finished: bool,
}
pub(crate) struct RollingResult {
    pub plan: Option<ContactPlan>,
    pub report: PlanningReport,
    pub seed: Option<SearchSeed>,
    pub checkpoint: Option<Checkpoint>,
}
pub struct PlannerJob {
    input: Input,
    base: Options,
    tier: usize,
    has_holds: bool,
    rapid: bool,
    work: Work,
    candidate: Option<ContactPlan>,
    pub report: PlanningReport,
    pub limit_seconds: f64,
    context: Option<RollingContext>,
    next_seed: Option<SearchSeed>,
    next_checkpoint: Option<Checkpoint>,
}
impl PlannerJob {
    pub fn new(input: Input, base: Options) -> Self {
        let has_holds = input.notes.iter().any(|n| n.judgeable && n.end.is_some());
        let mut times = input
            .notes
            .iter()
            .filter(|n| n.judgeable)
            .map(|n| n.time)
            .collect::<Vec<_>>();
        times.sort_by(f64::total_cmp);
        times.dedup();
        // Roll maintenance adds physical contacts between source rows. Those
        // contacts can leave a rapid transfer even when the chart rows alone
        // are farther apart. Keep the same bounded final recovery tier.
        let mut rapid = times
            .windows(2)
            .any(|w| w[1] - w[0] > 0. && w[1] - w[0] < 0.115)
            || input.notes.iter().any(|n| n.judgeable && n.roll);
        if !rapid {
            let mut last = [f64::NEG_INFINITY; 18];
            for n in input.notes.iter().filter(|n| n.judgeable) {
                if n.mine {
                    if let Some(previous) = last.get(n.lane as usize) {
                        let gap = n.time - crate::search::mines::WINDOW - previous;
                        rapid |= gap > 0. && gap < 0.115;
                    }
                } else if !n.lift
                    && let Some(previous) = last.get_mut(n.lane as usize)
                {
                    *previous = n.time;
                }
            }
        }
        Self {
            input,
            base,
            context: None,
            next_seed: None,
            next_checkpoint: None,
            tier: 0,
            has_holds,
            rapid,
            work: Work::Begin,
            candidate: None,
            limit_seconds: 5.,
            report: PlanningReport {
                status: Status::ContactFailure,
                failure_tick: None,
                attempts: Vec::new(),
                search_seconds: 0.,
                checkpoints: 0,
                reason: None,
                strict_reason: None,
                gate: None,
                adaptive_gate: None,
                allowance_seconds: None,
                restore_window_seconds: None,
                handoff_window_seconds: None,
                adaptive_timing: None,
                auxiliary_restores: None,
                interior_gaps: None,
                handoffs: None,
                maximum_interior_gap_seconds: None,
                maximum_handoff_gap_seconds: None,
                timing_adjustments: None,
                search: None,
                judge: None,
            },
        }
    }
    pub(crate) fn rolling(input: Input, context: RollingContext) -> Self {
        let mut job = Self::new(input, Options::default());
        job.context = Some(context);
        job
    }
    pub fn is_done(&self) -> bool {
        matches!(self.work, Work::Done)
    }
    pub fn stage(&self) -> &'static str {
        match self.work {
            Work::Begin | Work::Search(_) => "search",
            Work::BeginReplay | Work::Replay(_) | Work::WindowReplay(_) => "judge",
            Work::Done => "done",
        }
    }
    pub fn row(&self) -> usize {
        if let Work::Search(s) = &self.work {
            s.row()
        } else {
            0
        }
    }
    pub fn input(&self) -> &Input {
        &self.input
    }
    /// Charge search/policy work only; actual judge replay has its own stage,
    /// as in the Python audit where the judge runs in a separate process.
    pub fn charge_search(&mut self, seconds: f64) {
        if !seconds.is_finite() || seconds < 0. {
            self.report.reason = Some("invalid work clock".into());
            self.timeout();
            return;
        }
        self.report.search_seconds += seconds;
        if self.report.search_seconds >= self.limit_seconds {
            self.timeout();
        }
    }
    fn timeout(&mut self) {
        self.report.failure_tick = None;
        self.work = Work::Done;
        self.candidate = None;
        self.report.status = Status::ContactTimeout;
        self.report
            .reason
            .get_or_insert("combined contact search exceeded active-work cap".into());
    }
    fn next(&mut self) {
        self.tier += 1;
        self.candidate = None;
        self.work = Work::Begin;
    }
    fn options(&mut self) -> Option<Options> {
        if self.tier == 1 {
            self.report.gate = Some(
                if self.has_holds {
                    "strict_failure_with_holds"
                } else {
                    "no_holds"
                }
                .into(),
            );
            if !self.has_holds {
                self.tier = 7;
            }
        }
        if self.tier == 4 {
            self.report.gate = Some("failed_tail_tiers_with_holds".into());
        }
        if self.tier == 6 {
            self.report.gate = Some("failed_same_finger_tiers_with_holds".into());
        }
        if matches!(self.tier, 7 | 8) {
            if !self.rapid {
                self.report.adaptive_gate = Some("no_rapid_rows".into());
                return None;
            }
            self.report.adaptive_gate = Some("prior_failure_with_rapid_rows".into());
        }
        if self.tier > 8 {
            return None;
        }
        let mut o = self.base;
        o.hold_tail_grace = 0.;
        o.hold_restore_window = 0.;
        o.hold_handoff_window = 0.;
        o.adaptive_timing = false;
        o.restore_early_window = 0.;
        let mut attempt = Attempt::default();
        match self.tier {
            0 => return Some(o),
            1..=3 => {
                let grace = [0.08, 0.15, 0.22][self.tier - 1];
                o.hold_tail_grace = grace;
                attempt.allowance_seconds = Some(grace);
            }
            4..=5 => {
                let window = [0.19, 0.22][self.tier - 4];
                o.hold_tail_grace = 0.22;
                o.hold_restore_window = window;
                attempt.restore_window_seconds = Some(window);
            }
            6 => {
                o.hold_tail_grace = 0.22;
                o.hold_restore_window = 0.22;
                o.hold_handoff_window = 0.22;
                attempt.handoff_window_seconds = Some(0.22);
            }
            7 | 8 => {
                o.hold_tail_grace = 0.22;
                o.hold_restore_window = 0.22;
                o.hold_handoff_window = 0.22;
                o.adaptive_timing = true;
                o.restore_early_window = 0.182;
                attempt.adaptive_timing = Some(true);
                attempt.staggered_landings = (self.tier == 8).then_some(true);
                attempt.minimum_swing_seconds = Some(0.025);
                attempt.restore_early_window_seconds = Some(0.182);
            }
            _ => unreachable!(),
        }
        self.report.attempts.push(attempt);
        Some(o)
    }
    /// Advance one search/replay checkpoint. Returns true once the job is done;
    /// the host must charge measured search work and enforce its frame budget.
    pub fn step(&mut self) -> bool {
        if self.is_done() {
            return true;
        }
        self.report.checkpoints += 1;
        match std::mem::replace(&mut self.work, Work::Done) {
            Work::Begin => {
                if let Some(o) = self.options() {
                    self.report.failure_tick = None;
                    let search = match &self.context {
                        Some(c) => Search::seeded(
                            &self.input,
                            o,
                            &c.seed,
                            if c.finished { c.known } else { c.known - 0.5 },
                            c.known,
                        ),
                        None => Search::new(&self.input, o),
                    };
                    match search {
                        Ok(mut s) => {
                            s.staggered_recovery = self.tier == 8;
                            self.work = Work::Search(Box::new(s));
                        }
                        Err(e) => self.reject(e, None),
                    }
                }
            }
            Work::Search(mut s) => {
                if !s.step() {
                    self.work = Work::Search(s);
                } else {
                    let next_seed = self
                        .context
                        .as_ref()
                        .and_then(|c| s.boundary(c.known, c.finished));
                    let failure_tick = s.failure_tick();
                    match s.finish().unwrap() {
                        Err(e) => self.reject(e, failure_tick),
                        Ok((p, report)) => {
                            let credited = match self.tier {
                                4 | 5 => p.hold_gaps.as_ref().is_some_and(|g| {
                                    g.len() > self.context.as_ref().map_or(0, |c| c.seed.gaps())
                                }),
                                6 => p
                                    .hold_gaps
                                    .iter()
                                    .flatten()
                                    .skip(self.context.as_ref().map_or(0, |c| c.seed.gaps()))
                                    .any(|g| g.restore_foot.is_some()),
                                8 => {
                                    report.staggered_flights
                                        > self
                                            .context
                                            .as_ref()
                                            .map_or(0, |c| c.seed.staggered_flights())
                                }
                                7 => {
                                    report.skipped_roll_tail_refreshes > 0
                                        || p.timing_adjustments.as_ref().is_some_and(|g| {
                                            g.len()
                                                > self
                                                    .context
                                                    .as_ref()
                                                    .map_or(0, |c| c.seed.timing())
                                        })
                                }
                                _ => true,
                            };
                            if !credited {
                                self.report.attempts.last_mut().unwrap().reason = Some(
                                    match self.tier {
                                        4 | 5 => {
                                            "No actual interior release; not credited to this tier"
                                        }
                                        6 => "No actual handoff; not credited to this tier",
                                        8 => "No new staggered flight; not credited to this tier",
                                        _ => "No timing compression or omitted roll-tail refresh; not credited",
                                    }
                                    .into(),
                                );
                                self.next();
                            } else {
                                self.next_seed = next_seed;
                                self.report.search = Some(report);
                                self.candidate = Some(p);
                                self.work = Work::BeginReplay;
                            }
                        }
                    }
                }
            }
            Work::BeginReplay if self.context.is_some() => {
                let c = self.context.as_ref().unwrap();
                let certified = self.next_seed.as_ref().unwrap().release_floor;
                match WindowReplay::new(
                    &self.input,
                    self.candidate.as_ref().unwrap(),
                    &c.checkpoint,
                    // EOF proves there are no unknown heads to steal a restore.
                    // Validate every fixed pending return before committing its
                    // row, even when its declared landing is just past K.
                    if c.finished {
                        self.candidate.as_ref().unwrap().duration
                    } else {
                        c.known
                    },
                    certified,
                ) {
                    Ok(r) => self.work = Work::WindowReplay(Box::new(r)),
                    Err(e) => self.judge_error(e),
                }
            }
            Work::WindowReplay(mut r) => {
                if !r.step() {
                    self.work = Work::WindowReplay(r);
                } else {
                    let (report, checkpoint) = r.finish();
                    if report.passed {
                        self.next_checkpoint = Some(checkpoint);
                    }
                    self.replay_finished(report);
                }
            }
            Work::BeginReplay => {
                match ReplayJob::new(&self.input, self.candidate.as_ref().unwrap()) {
                    Ok(r) => self.work = Work::Replay(Box::new(r)),
                    Err(e) => {
                        if self.tier == 0 {
                            self.report.reason = Some(e);
                            self.report.status = Status::ContactJudgeFailure;
                            self.candidate = None;
                        } else {
                            self.report.attempts.last_mut().unwrap().reason = Some(e);
                            self.next();
                        }
                    }
                }
            }
            Work::Replay(mut r) => {
                if !r.step() {
                    self.work = Work::Replay(r);
                } else {
                    self.replay_finished(r.finish());
                }
            }
            Work::Done => {}
        }
        self.is_done()
    }
    fn judge_error(&mut self, e: String) {
        if self.tier == 0 {
            self.report.reason = Some(e);
            self.report.status = Status::ContactJudgeFailure;
            self.candidate = None;
        } else {
            self.report.attempts.last_mut().unwrap().reason = Some(e);
            self.next();
        }
    }
    fn replay_finished(&mut self, report: ReplayReport) {
        if self.tier > 0 {
            self.report.attempts.last_mut().unwrap().judge = Some(report.clone());
        }
        if report.passed {
            self.report.judge = Some(report);
            self.accept();
        } else if self.tier == 0 {
            self.report.status = Status::ContactJudgeFailure;
            self.report.judge = Some(report);
            self.candidate = None;
        } else {
            self.next();
        }
    }
    pub(crate) fn finish_rolling(self) -> RollingResult {
        assert!(self.is_done());
        RollingResult {
            plan: self.candidate,
            report: self.report,
            seed: self.next_seed,
            checkpoint: self.next_checkpoint,
        }
    }
    fn reject(&mut self, e: String, tick: Option<i64>) {
        self.report.failure_tick = tick;
        if self.tier == 0 {
            if tick.is_none() {
                self.report.status = Status::Unsupported;
                self.report.reason = Some(e);
                return;
            }
            self.report.strict_reason = Some(e);
        } else {
            self.report.attempts.last_mut().unwrap().reason = Some(e);
        }
        self.next();
    }
    fn accept(&mut self) {
        self.report.status = if self.tier == 0 {
            Status::StrictContactPass
        } else {
            Status::JudgeRecovered
        };
        let p = self.candidate.as_ref().unwrap();
        match self.tier {
            1..=3 => self.report.allowance_seconds = Some([0.08, 0.15, 0.22][self.tier - 1]),
            4 | 5 => {
                self.report.restore_window_seconds = Some([0.19, 0.22][self.tier - 4]);
                self.report.auxiliary_restores = Some(
                    p.aux_contacts
                        .iter()
                        .flatten()
                        .filter(|a| a.kind == crate::RestoreKind::HoldRestore)
                        .count(),
                );
                self.report.interior_gaps = Some(p.hold_gaps.as_ref().unwrap().len());
                self.report.maximum_interior_gap_seconds = Some(
                    p.hold_gaps
                        .iter()
                        .flatten()
                        .map(|g| g.restore - g.release)
                        .fold(f64::NEG_INFINITY, f64::max),
                );
            }
            6 => {
                self.report.handoff_window_seconds = Some(0.22);
                let gaps = p
                    .hold_gaps
                    .iter()
                    .flatten()
                    .filter(|g| g.restore_foot.is_some())
                    .collect::<Vec<_>>();
                self.report.handoffs = Some(gaps.len());
                self.report.maximum_handoff_gap_seconds = Some(
                    gaps.iter()
                        .map(|g| g.restore - g.release)
                        .fold(f64::NEG_INFINITY, f64::max),
                );
                self.report.auxiliary_restores = Some(
                    p.aux_contacts
                        .iter()
                        .flatten()
                        .filter(|a| a.kind == crate::RestoreKind::HoldRestore)
                        .count(),
                );
            }
            7 | 8 => {
                self.report.adaptive_timing = Some(true);
                self.report.timing_adjustments = Some(p.timing_adjustments.as_ref().unwrap().len());
            }
            _ => {}
        }
    }
    /// Only a completed, judge-validated plan is visible. Failures/timeouts
    /// retain their report and expose no candidate.
    pub fn plan(&self) -> Option<&ContactPlan> {
        self.is_done().then_some(self.candidate.as_ref()).flatten()
    }
    pub fn finish(self) -> Option<(Option<ContactPlan>, PlanningReport)> {
        if self.is_done() {
            Some((self.candidate, self.report))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_chart::{Note, NoteKind, Tick};
    #[test]
    fn eof_validates_the_whole_pending_return_without_advancing_checkpoint() {
        let mut song = ddi_chart::formats::sm::parse_sm(
            "#BPMS:0=120;#NOTES:dance-single::Hard:1:0,0,0,0,0:\n1000;",
        )
        .unwrap();
        song.charts[0].notes = vec![Note::new(
            Tick(192),
            1,
            NoteKind::HoldHead { end: Tick(384) },
        )];
        let input = Input::from_song(&song, 0).unwrap();
        let context = RollingContext {
            seed: SearchSeed::initial(),
            checkpoint: Checkpoint::initial(&input.timing),
            known: 2.5,
            finished: false,
        };
        let mut first = PlannerJob::rolling(input, context);
        while !first.step() {}
        let first = first.finish_rolling();
        assert_eq!(first.plan.as_ref().unwrap().events[0].foot, Foot::L);
        // This later head was not known when the left finger took the hold.
        // It now has to visit Left and return, beyond the EOF watermark at 3 s.
        song.charts[0]
            .notes
            .push(Note::new(Tick(288), 0, NoteKind::Tap));
        let context = RollingContext {
            seed: first.seed.unwrap(),
            checkpoint: first.checkpoint.unwrap(),
            known: 3.,
            finished: true,
        };
        let mut job = PlannerJob::rolling(Input::from_song(&song, 0).unwrap(), context);
        while !job.step() {}
        let result = job.finish_rolling();
        assert_eq!(result.report.status, Status::JudgeRecovered);
        let plan = result.plan.unwrap();
        assert!(plan.tracks.left.iter().any(|s| s.land > 3.), "{plan:?}");
        for rule in result.report.judge.unwrap().rules.values() {
            assert!(rule.passed);
            assert_eq!(rule.hits, 2);
            assert_eq!(rule.edges, 4, "outward and return edges must all be judged");
        }
        assert_eq!(result.checkpoint.unwrap().time, 3.);
    }
}
