// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
//! Received rows → bounded contact windows with immutable certified prefixes.
use crate::{policy::RollingContext, search::SearchSeed, window_replay::Checkpoint, *};

#[derive(Clone, Debug, Default, Serialize)]
pub struct RollingReport {
    pub publications: usize,
    pub checkpoints: u64,
    pub received: usize,
    pub kept: usize,
    pub removed: usize,
    pub committed_heads: usize,
    pub committed_holds: usize,
    pub known_until: f64,
    pub certified_until: f64,
    pub peak_retained: usize,
    pub search_seconds: f64,
    pub recoveries: [usize; 4],
    pub failure: Option<String>,
    pub last: Option<PlanningReport>,
}
/// The candidate includes tentative right context; consumers may publish motion
/// only through certified_until. committed is for trace checking/recorders.
pub struct ContactPublication {
    pub plan: ContactPlan,
    pub committed: ContactPlan,
    pub certified_until: f64,
    pub known_until: f64,
    pub finished: bool,
}
pub struct RollingPlanner {
    window: f64,
    input: Input,
    raw_holds: Vec<Note>,
    last_raw_tick: Option<i64>,
    last_raw_time: f64,
    requested_finished: bool,
    received: bool,
    finished: bool,
    seed: SearchSeed,
    checkpoint: Checkpoint,
    job: Option<PlannerJob>,
    publication: Option<ContactPublication>,
    requested_until: f64,
    now: f64,
    pub report: RollingReport,
}
impl RollingPlanner {
    pub fn new(window: f64, timing: TimingMap) -> Result<Self, String> {
        Self::configured(window, timing, "dance-single", standard_rules())
    }
    pub fn configured(
        window: f64,
        timing: TimingMap,
        layout: &str,
        rules: Vec<(String, ddi_engine::rules::JudgeTable)>,
    ) -> Result<Self, String> {
        layout_lanes(layout)?;
        if rules.is_empty() {
            return Err("missing gameplay judge rules".into());
        }
        if !window.is_finite() || !(2. ..=3.).contains(&window) {
            return Err("lookahead must be 2–3 seconds".into());
        }
        let checkpoint = Checkpoint::configured(&timing, layout_lanes(layout)?, &rules);
        Ok(Self {
            window,
            input: Input {
                notes: Vec::new(),
                effective_notes: Vec::new(),
                original_notes: 0,
                timing,
                layout: layout.into(),
                rules,
            },
            raw_holds: Vec::new(),
            last_raw_tick: None,
            last_raw_time: f64::NEG_INFINITY,
            requested_finished: false,
            received: false,
            finished: false,
            seed: SearchSeed::configured(layout),
            checkpoint,
            job: None,
            publication: None,
            requested_until: f64::NEG_INFINITY,
            now: 0.,
            report: RollingReport::default(),
        })
    }
    pub fn window(&self) -> f64 {
        self.window
    }
    /// Complete rows, in source order. The watermark promises that no future
    /// packet will add a head at/before known_until. Hold ends are declared with
    /// their heads and may extend beyond the current known-note window.
    pub fn push(
        &mut self,
        now: f64,
        raw: &[Note],
        known_until: f64,
        finished: bool,
    ) -> Result<(), String> {
        let reject = if !now.is_finite()
            || !known_until.is_finite()
            || known_until < self.report.known_until
            || known_until > now.max(0.) + self.window + 1e-7
            || now < self.now - 1e-7
        {
            Some("invalid or oversized incoming-note watermark")
        } else if self.finished && !raw.is_empty() {
            Some("notes arrived after end of stream")
        } else if raw
            .windows(2)
            .any(|w| (w[0].tick, w[0].lane) >= (w[1].tick, w[1].lane))
            || raw
                .first()
                .is_some_and(|n| self.last_raw_tick.is_some_and(|t| n.tick.0 <= t))
        {
            Some("incoming rows are old, partial or unordered")
        } else if raw.iter().any(|n| {
            let t = self.input.timing.seconds_at(n.tick);
            !t.is_finite()
                || n.lane as usize >= layout_lanes(&self.input.layout).unwrap()
                || t > known_until
                || (self.received && t <= self.report.known_until)
                || n.kind.hold_end().is_some_and(|e| e <= n.tick)
        }) {
            Some("late note, invalid lane/hold or head beyond watermark")
        } else if self.input.notes.len() + raw.len() > 8192 {
            Some("incoming-note buffer limit")
        } else {
            None
        };
        if let Some(e) = reject {
            self.report.failure = Some(e.into());
            self.job = None;
            self.publication = None;
            return Err(e.into());
        }
        let mut source = self.raw_holds.clone();
        source.extend_from_slice(raw);
        let effective = prepare_notes(
            &source,
            &ddi_chart::Layout::builtin(&self.input.layout).unwrap(),
        );
        let new = effective
            .iter()
            .filter(|n| self.last_raw_tick.is_none_or(|t| n.tick.0 > t))
            .cloned()
            .collect::<Vec<_>>();
        self.input.original_notes += raw.len();
        self.report.received += raw.len();
        self.report.kept += new.len();
        self.report.removed += raw.len() - new.len();
        let panels = layout_panels(&self.input.layout)?;
        self.input.notes.extend(new.iter().map(|n| {
            InputNote {
                tick: n.tick.0,
                time: self.input.timing.seconds_at(n.tick),
                lane: n.lane,
                panel: panels[n.lane as usize],
                judgeable: self.input.timing.judgeable(n.tick),
                end: n
                    .kind
                    .hold_end()
                    .map(|e| (e.0, self.input.timing.seconds_at(e))),
                supported: matches!(
                    n.kind,
                    NoteKind::Tap
                        | NoteKind::HoldHead { .. }
                        | NoteKind::RollHead { .. }
                        | NoteKind::Mine
                        | NoteKind::Shock
                        | NoteKind::Lift
                ),
                mine: matches!(n.kind, NoteKind::Mine | NoteKind::Shock),
                shock: n.kind == NoteKind::Shock,
                lift: n.kind == NoteKind::Lift,
                roll: matches!(n.kind, NoteKind::RollHead { .. }),
                roll_refresh: false,
            }
        }));
        self.input.effective_notes.extend(new);
        self.last_raw_time = raw
            .last()
            .map_or(self.last_raw_time, |n| self.input.timing.seconds_at(n.tick));
        self.last_raw_tick = raw.last().map(|n| n.tick.0).or(self.last_raw_tick);
        self.raw_holds = effective
            .into_iter()
            .filter(|n| {
                n.kind
                    .hold_end()
                    .is_some_and(|e| self.last_raw_tick.is_none_or(|t| e.0 >= t))
            })
            .collect();
        self.finished |= finished;
        self.received = true;
        self.now = now.max(0.);
        self.report.known_until = known_until;
        Ok(())
    }
    pub fn request(&mut self, now: f64) {
        if self.report.failure.is_some() || self.job.is_some() || !self.received {
            return;
        }
        if !now.is_finite() || now.max(0.) < self.now - 1e-7 {
            return;
        }
        self.now = now.max(0.);
        let known = self
            .report
            .known_until
            .min((self.now * 4.).floor() / 4. + self.window);
        let final_fraction =
            self.finished && self.report.known_until < self.now + self.window - 1e-7;
        if known <= 0.
            || known <= self.requested_until
            || (known < self.requested_until + 0.25 - 1e-7 && !final_fraction)
        {
            return;
        }
        self.requested_until = known;
        self.requested_finished = self.finished && self.last_raw_time <= known;
        let mut input = self.input.clone();
        let mut notes = Vec::new();
        let mut effective = Vec::new();
        for (n, raw) in input.notes.iter().zip(&input.effective_notes) {
            if n.time <= known {
                notes.push(*n);
                effective.push(raw.clone());
            }
        }
        input.notes = notes;
        input.effective_notes = effective;
        self.job = Some(PlannerJob::rolling(
            input,
            RollingContext {
                seed: self.seed.clone(),
                checkpoint: self.checkpoint.clone(),
                known,
                finished: self.requested_finished,
            },
        ));
    }
    pub fn stage(&self) -> &'static str {
        self.job.as_ref().map_or("idle", |j| j.stage())
    }
    pub fn charge_search(&mut self, seconds: f64) {
        if let Some(j) = self.job.as_mut() {
            j.charge_search(seconds);
        }
    }
    /// One cooperative checkpoint. True means work ran; publications are taken
    /// separately, and contain no partial/unvalidated candidate.
    pub fn step(&mut self) -> bool {
        let Some(job) = self.job.as_mut() else {
            return false;
        };
        self.report.checkpoints += 1;
        if !job.is_done() {
            job.step();
            return true;
        }
        let result = self.job.take().unwrap().finish_rolling();
        self.report.search_seconds += result.report.search_seconds;
        if let (Some(plan), Some(mut seed), Some(checkpoint)) =
            (result.plan, result.seed, result.checkpoint)
        {
            for (i, used) in [
                result.report.allowance_seconds.is_some(),
                result.report.restore_window_seconds.is_some(),
                result.report.handoff_window_seconds.is_some(),
                result.report.adaptive_timing == Some(true),
            ]
            .into_iter()
            .enumerate()
            {
                self.report.recoveries[i] += usize::from(used);
            }
            let committed = Search::committed_plan(&seed, self.requested_until);
            self.report.committed_heads += committed
                .events
                .iter()
                .filter(|e| self.seed.tick().is_none_or(|t| e.tick > t))
                .count();
            let new_ticks = committed
                .events
                .iter()
                .filter(|e| self.seed.tick().is_none_or(|t| e.tick > t))
                .map(|e| (e.time, e.foot))
                .collect::<Vec<_>>();
            self.report.committed_holds += committed
                .holds
                .iter()
                .filter(|h| new_ticks.contains(&(h.start, h.foot)))
                .count();
            self.report.publications += 1;
            self.report.certified_until = seed.release_floor;
            self.publication = Some(ContactPublication {
                plan,
                committed,
                certified_until: seed.release_floor,
                known_until: self.requested_until,
                finished: self.requested_finished,
            });
            seed.prune((self.now - 1.).max(0.));
            self.seed = seed;
            self.checkpoint = checkpoint;
            let before = self.now - 1.;
            let mut notes = Vec::new();
            let mut effective = Vec::new();
            for (n, raw) in self.input.notes.iter().zip(&self.input.effective_notes) {
                if n.time >= before || n.end.is_some_and(|(_, e)| e >= before) {
                    notes.push(*n);
                    effective.push(raw.clone());
                }
            }
            self.input.notes = notes;
            self.input.effective_notes = effective;
            self.report.peak_retained = self
                .report
                .peak_retained
                .max(self.input.notes.len() + self.seed.retained() + self.checkpoint.retained());
        } else {
            self.report.failure = Some(format!(
                "{:?}: {}",
                result.report.status,
                result
                    .report
                    .reason
                    .as_deref()
                    .or(result.report.strict_reason.as_deref())
                    .unwrap_or("no accepted boundary continuation")
            ));
        }
        self.report.last = Some(result.report);
        true
    }
    pub fn take_publication(&mut self) -> Option<ContactPublication> {
        self.publication.take()
    }
}
