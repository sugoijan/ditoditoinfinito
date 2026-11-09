// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
//! Cooperative rolling output for a known contact plan. No clocks or I/O.
use crate::correction::correct_contact_pose_with;
use crate::kinematics::Kin;
use crate::{
    ContactCurve, ContactCurveError, ContactCurvePreparation, ContactCurvePublication,
    ContactCurveSample, Model, Parameters, Plan, Rolling,
};

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct RuntimeReport {
    pub publications: usize,
    pub discarded_publications: usize,
    pub checkpoints: u64,
    pub source_samples: usize,
    pub source_evaluations: usize,
    pub source_cache_hits: usize,
    pub source_cache_peak: usize,
    pub geometry_cache_hits: usize,
    pub geometry_evaluations: usize,
    pub geometry_cache_peak: usize,
    pub arrow_checks: usize,
    pub guide_late_jobs: usize,
    pub minimum_guide_lead_seconds: f64,
    pub peak_knots: usize,
    pub covered_until: f64,
    pub commit_start: f64,
    pub source_until: f64,
    pub known_until: f64,
    pub certified_until: f64,
    pub ready: bool,
    pub maximum_join_errors: [f64; 3],
    pub failure: Option<ContactCurveError>,
}
// Exact-value reuse, scoped to a single runtime/model. No approximate keys or
// temporal extrapolation: changed guides, targets, blend weights or stream poses
// miss. The three-second cache is independent of total chart length.
#[derive(Default)]
struct SourceCache {
    model: usize,
    bounds: [u64; 28],
    entries: std::collections::BTreeMap<u64, ([u64; 39], Parameters)>,
}
impl SourceCache {
    fn bind(&mut self, m: &Model) {
        let address = m as *const Model as usize;
        let bounds = std::array::from_fn(|i| if i < 14 { m.lo[i] } else { m.hi[i - 14] }.to_bits());
        if self.model != address || self.bounds != bounds {
            self.entries.clear();
            self.model = address;
            self.bounds = bounds;
        }
    }
    fn get(&mut self, m: &Model, t: f64, key: &[u64; 39]) -> Option<Parameters> {
        self.bind(m);
        self.entries
            .get(&t.to_bits())
            .filter(|(old, _)| old == key)
            .map(|(_, q)| *q)
    }
    fn insert(&mut self, t: f64, key: [u64; 39], pose: Parameters) {
        // Defensive hard bound even for a caller's unusually dense authored Plan.
        if self.entries.len() >= 4096 {
            self.entries.pop_first();
        }
        self.entries.insert(t.to_bits(), (key, pose));
    }
    fn retain(&mut self, start: f64, end: f64) {
        self.entries.retain(|t, _| {
            let t = f64::from_bits(*t);
            t >= start && t <= end
        });
    }
}
fn source_times(p: &Plan, start: f64, end: f64) -> Vec<f64> {
    let mut times = vec![start, end];
    times.extend(
        ((start * 120.).ceil() as usize..=(end * 120.).floor() as usize)
            .map(|i| i as f64 / 120.)
            // ceil/floor of t * 120 can round onto a grid index whose
            // division lies outside t. Never sample beyond finalized motion.
            .filter(|t| *t >= start && *t <= end),
    );
    times.extend(
        p.edges
            .iter()
            .copied()
            .chain(p.events.iter().map(|e| e.time))
            .filter(|t| *t > start && *t < end),
    );
    times.sort_by(f64::total_cmp);
    times.dedup();
    times
}

enum Work {
    Idle,
    Guide,
    Source {
        times: Vec<f64>,
        poses: Vec<Parameters>,
        at: usize,
    },
    Correct(Box<ContactCurvePreparation>),
    Publish(Box<ContactCurvePublication>),
    Verify {
        curve: ContactCurve,
        events: Vec<usize>,
        at: usize,
        probe: i32,
    },
}
/// A note lookahead of 2–3 seconds. The guide and corrected output remain bounded
/// to that window plus short history. Every step is a cooperative checkpoint;
/// contact refinement/evaluation is bounded to the existing 8+8 iterations.
pub struct DanceRuntime {
    rolling: Rolling,
    source_cache: Option<SourceCache>,
    inspection_cache: Option<crate::contact_curve::InspectionCache>,
    lookahead: f64,
    cycle: f64,
    snapshot: Plan,
    certified_until: f64,
    relaxed: Vec<[f64; 2]>,
    work: Work,
    output: Option<ContactCurve>,
    output_stream: Vec<[f64; 2]>,
    kin: Box<Kin>,
    pub report: RuntimeReport,
}
impl DanceRuntime {
    pub fn new(m: &Model, plan: &Plan, lookahead: f64) -> Result<Self, &'static str> {
        if !lookahead.is_finite() || !(2. ..=3.).contains(&lookahead) {
            return Err("lookahead must be 2–3 seconds");
        }
        let mut rolling = Rolling::new(m, plan.opening_seed_plan(lookahead + 0.5), lookahead);
        // A fit begun exactly at now + 250 ms is late as soon as it yields
        // into another frame. Match the corrected publication's 150 ms work
        // reserve without shortening the immutable quarter-second prefix.
        rolling.preparation_reserve = 0.15;
        Ok(Self {
            rolling,
            source_cache: Some(SourceCache::default()),
            inspection_cache: Some(Default::default()),
            lookahead,
            cycle: -1.,
            snapshot: plan.visible(lookahead),
            certified_until: plan.duration,
            relaxed: Vec::new(),
            work: Work::Idle,
            output: None,
            output_stream: Vec::new(),
            kin: Box::default(),
            report: RuntimeReport::default(),
        })
    }
    /// Reference switch for parity/profiling. A runtime keeps one calibration;
    /// changing a model requires rebuilding its guides as well as this cache.
    pub fn set_source_reuse(&mut self, enabled: bool) {
        assert_eq!(
            self.report.checkpoints, 0,
            "configure reuse before preparation starts"
        );
        self.source_cache = enabled.then(SourceCache::default);
        self.inspection_cache = enabled.then(Default::default);
    }
    pub fn lookahead(&self) -> f64 {
        self.lookahead
    }
    pub fn stage(&self) -> &'static str {
        match self.work {
            Work::Idle => "idle",
            Work::Guide => "guide",
            Work::Source { .. } => "source",
            Work::Correct(_) => "correct",
            Work::Publish(_) => "publish",
            Work::Verify { .. } => "arrows",
        }
    }
    /// Start at most one snapshot per quarter second. Requests never publish or
    /// perform numerical work. The caller supplies the same immutable plan.
    pub fn request(&mut self, p: &Plan, now: f64) {
        let known = now + self.lookahead;
        self.request_bounded(p, now, known, known.min(p.duration), p.duration <= known);
    }
    /// Read received candidate contacts as context, but publish prepared motion
    /// only through their immutable certified horizon. finished means this is
    /// the final motion window, not just a temporary end of received data.
    pub fn request_bounded(
        &mut self,
        p: &Plan,
        now: f64,
        known_until: f64,
        certified_until: f64,
        finished: bool,
    ) {
        if !now.is_finite() || self.report.failure.is_some() || !matches!(self.work, Work::Idle) {
            return;
        }
        if !p.same_geometry(&self.snapshot) {
            self.report.failure = Some(ContactCurveError {
                reason: "geometry_changed",
                time: now,
            });
            return;
        }
        if !known_until.is_finite()
            || !certified_until.is_finite()
            || certified_until < 0.
            || certified_until > known_until
        {
            self.report.failure = Some(ContactCurveError {
                reason: "invalid_contact_horizon",
                time: now,
            });
            return;
        }
        let now = now.max(0.).min(p.duration);
        if self.report.ready
            && (self.report.covered_until >= certified_until.min(p.duration)
                || now - self.cycle < 0.25)
        {
            return;
        }
        self.cycle = now;
        let received = (now + self.lookahead).min(known_until);
        let known = received.min(p.duration);
        self.snapshot = p.visible(known);
        self.report.known_until = known;
        self.certified_until = certified_until.min(p.duration);
        self.report.certified_until = self.certified_until;
        // `snapshot` is this request's visible plan: the same lookahead.
        self.rolling
            .request_visible(self.snapshot.clone(), p.duration, now, received, finished);
        self.work = Work::Guide;
    }
    pub fn step(&mut self, m: &Model, now: f64) -> bool {
        if self.report.failure.is_some() || !now.is_finite() {
            return false;
        }
        let work = std::mem::replace(&mut self.work, Work::Idle);
        if matches!(work, Work::Idle) {
            return false;
        }
        self.report.checkpoints += 1;
        let result = self.advance(m, now.max(0.), work);
        if let Err(e) = result {
            self.report.failure = Some(e);
            self.work = Work::Idle;
        }
        if let Some(cache) = &self.inspection_cache {
            self.report.geometry_cache_hits = cache.hits;
            self.report.geometry_evaluations = cache.misses;
            self.report.geometry_cache_peak = cache.peak;
        }
        true
    }
    fn advance(&mut self, m: &Model, now: f64, work: Work) -> Result<(), ContactCurveError> {
        match work {
            Work::Idle => {}
            Work::Guide => {
                if self.rolling.work_slice(m, now, 1) > 0 {
                    self.report.guide_late_jobs = self.rolling.report.late_jobs;
                    self.report.minimum_guide_lead_seconds =
                        self.rolling.report.minimum_planning_lead;
                    self.work = Work::Guide;
                } else {
                    self.relaxed = self.rolling.stream_intervals();
                    let start = (self.cycle - 0.5).max(0.);
                    let end = self.rolling.cap_stream_end(
                        (self.cycle + self.lookahead - 0.5)
                            .min(self.snapshot.duration)
                            .min(self.certified_until),
                    );
                    if end <= start {
                        return Err(ContactCurveError {
                            reason: "runtime_unprepared_window",
                            time: now,
                        });
                    }
                    if let Some(cache) = &mut self.source_cache {
                        cache.retain(start, end);
                    }
                    let times = source_times(&self.snapshot, start, end);
                    self.work = Work::Source {
                        poses: Vec::with_capacity(times.len()),
                        times,
                        at: 0,
                    };
                }
            }
            Work::Source {
                times,
                mut poses,
                mut at,
            } => {
                if at < times.len() {
                    let t = times[at];
                    let key = self
                        .source_cache
                        .as_ref()
                        .and_then(|_| self.rolling.source_key(&self.snapshot, t));
                    let reused = key
                        .as_ref()
                        .and_then(|key| self.source_cache.as_mut()?.get(m, t, key));
                    let q = if let Some(q) = reused {
                        self.report.source_cache_hits += 1;
                        q
                    } else {
                        self.report.source_evaluations += 1;
                        let (q, r) = self.rolling.evaluate(m, &self.snapshot, t);
                        if r.stream_missing {
                            return Err(ContactCurveError {
                                reason: "runtime_stream_starved",
                                time: t,
                            });
                        }
                        let q = if r.stream_weight == 0. {
                            let targets = self.snapshot.targets(t);
                            correct_contact_pose_with(m, targets, q, &mut self.kin).0
                        } else if r.stream_weight < 1. {
                            let targets = self.snapshot.targets(t);
                            let c = correct_contact_pose_with(m, targets, q, &mut self.kin).0;
                            std::array::from_fn(|i| q[i] + (1. - r.stream_weight) * (c[i] - q[i]))
                        } else {
                            q
                        };
                        if let (Some(cache), Some(key)) = (&mut self.source_cache, key) {
                            cache.insert(t, key, q);
                            self.report.source_cache_peak =
                                self.report.source_cache_peak.max(cache.entries.len());
                        }
                        q
                    };
                    poses.push(q);
                    at += 1;
                    self.report.source_samples += 1;
                    self.work = Work::Source { times, poses, at };
                } else {
                    self.report.source_until = *times.last().unwrap();
                    self.work = Work::Correct(Box::new(
                        ContactCurvePreparation::new(
                            m,
                            &self.snapshot,
                            &times,
                            &poses,
                            &self.relaxed,
                        )?
                        .with_cache(self.inspection_cache.take()),
                    ));
                }
            }
            Work::Correct(mut job) => {
                if !job.step(m, &self.snapshot) {
                    self.work = Work::Correct(job);
                } else {
                    self.inspection_cache = job.take_cache();
                    let next = job.finish().unwrap()?;
                    if let Some(old) = &self.output {
                        let old_times = old.knot_times().collect::<Vec<_>>();
                        let common = next
                            .knot_times()
                            .filter(|t| old_times.binary_search_by(|v| v.total_cmp(t)).is_ok())
                            .collect::<Vec<_>>();
                        // Leave checking time when possible, but do not waste
                        // a ready candidate because a stall consumed that reserve.
                        // The committed quarter second always remains protected.
                        let earliest = (now + 0.4)
                            .max(self.cycle + 0.5)
                            .min(old.time_range()[1] - 0.175)
                            .max(now + 0.25);
                        let start = common
                            .iter()
                            .copied()
                            .find(|t| *t >= earliest)
                            .unwrap_or(old.time_range()[1]);
                        let end =
                            common
                                .iter()
                                .copied()
                                .find(|t| *t >= start + 0.6 && *t <= old.time_range()[1] - 0.05)
                                .or_else(|| {
                                    common.iter().rev().copied().find(|t| {
                                        *t <= old.time_range()[1] - 0.05 && *t >= start + 0.1
                                    })
                                })
                                .unwrap_or(start);
                        if end - start < 0.1 {
                            return Err(ContactCurveError {
                                reason: "runtime_publication_deadline",
                                time: now,
                            });
                        }
                        self.report.commit_start = start;
                        self.work = Work::Publish(Box::new(
                            ContactCurvePublication::with_grid(
                                old,
                                &self.snapshot,
                                &next,
                                start,
                                end,
                                now,
                                &self.relaxed,
                                false,
                            )?
                            .with_cache(self.inspection_cache.take()),
                        ));
                    } else {
                        self.verify(next);
                    }
                }
            }
            Work::Publish(mut job) => {
                if !job.step(m, &self.snapshot) {
                    self.work = Work::Publish(job);
                } else {
                    self.inspection_cache = job.take_cache();
                    if self.discard_late(now) {
                        return Ok(());
                    }
                    self.verify(job.finish().unwrap()?);
                }
            }
            Work::Verify {
                curve,
                events,
                mut at,
                mut probe,
            } => {
                let e = &self.snapshot.events[events[at]];
                if let Some([before, after]) = self.snapshot.lift_interval(e) {
                    let d = e.foot.index();
                    let anchor = self.snapshot.targets(before).tips[d];
                    let a = m.evaluate_into(&curve.pose(before), &mut self.kin).tips[d];
                    let b = m.evaluate_into(&curve.pose(after), &mut self.kin).tips[d];
                    if a[2].abs() > 0.25
                        || (a[0] - anchor[0]).hypot(a[1] - anchor[1]) > 2.
                        || b[2] <= 0.25
                    {
                        return Err(ContactCurveError {
                            reason: "runtime_lift_release",
                            time: e.time,
                        });
                    }
                }
                let tip = m
                    .evaluate_into(&curve.pose(e.time + probe as f64 / 480.), &mut self.kin)
                    .tips[e.foot.index()];
                let [x, y] = self
                    .snapshot
                    .physical_position(e.panel)
                    .expect("validated panel");
                let touches = self.snapshot.tile_half_mm()
                    - (tip[0] + 16. - x).abs().max((tip[1] - y).abs())
                    >= 0.
                    && tip[2].abs() <= 1.;
                if touches {
                    self.report.arrow_checks += 1;
                    at += 1;
                    probe = -8;
                } else if probe == 8 {
                    return Err(ContactCurveError {
                        reason: "runtime_stream_arrow",
                        time: e.time,
                    });
                } else {
                    probe += 1;
                }
                if at == events.len() {
                    if self.output.is_some() && self.discard_late(now) {
                        return Ok(());
                    }
                    self.publish(curve);
                } else {
                    self.work = Work::Verify {
                        curve,
                        events,
                        at,
                        probe,
                    };
                }
            }
        }
        Ok(())
    }
    fn discard_late(&mut self, now: f64) -> bool {
        if self.report.commit_start >= now + 0.25 {
            return false;
        }
        // A stall must not retroactively rewrite the committed prefix. Keep
        // the valid output, discard only this candidate, and retry next frame.
        self.report.discarded_publications += 1;
        self.cycle = now - 0.25;
        self.work = Work::Idle;
        true
    }
    fn verify(&mut self, c: ContactCurve) {
        let [a, b] = c.time_range();
        let events = self
            .snapshot
            .events
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.time >= a
                    && e.time <= b
                    && self
                        .snapshot
                        .lift_interval(e)
                        .is_none_or(|[x, y]| x >= a && y <= b)
                    && (e.lift
                        || self
                            .relaxed
                            .iter()
                            .any(|[a, b]| e.time >= *a && e.time <= *b))
            })
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        if events.is_empty() {
            self.publish(c);
        } else {
            self.work = Work::Verify {
                curve: c,
                events,
                at: 0,
                probe: -8,
            };
        }
    }
    fn publish(&mut self, c: ContactCurve) {
        self.report.covered_until = c.time_range()[1];
        self.report.peak_knots = self.report.peak_knots.max(c.report.knots);
        // Joins before the commit are the last publication's, already counted.
        for (a, b) in self
            .report
            .maximum_join_errors
            .iter_mut()
            .zip(c.join_errors_from(self.report.commit_start))
        {
            *a = a.max(b);
        }
        self.report.ready = true;
        self.report.publications += 1;
        let [a, b] = c.time_range();
        self.output_stream = self
            .relaxed
            .iter()
            .copied()
            .filter(|v| v[1] > a && v[0] < b)
            .collect();
        self.output = Some(c);
    }
    /// Whether the published sample uses the accepted dense stream/blend guards.
    /// Pending guide sections cannot relabel the currently published output.
    pub fn stream_at(&self, t: f64) -> bool {
        self.output_stream.iter().any(|v| t > v[0] && t < v[1])
    }
    /// Drawing only evaluates the prepared polynomial. Explicitly refuse an
    /// uncovered time; ContactCurve's endpoint clamp must not hide starvation.
    pub fn sample(&self, t: f64) -> Option<ContactCurveSample> {
        self.covering(t).map(|c| c.sample(t))
    }
    /// [`Self::sample`]'s pose alone, without its derivatives.
    pub fn pose(&self, t: f64) -> Option<Parameters> {
        self.covering(t).map(|c| c.pose(t))
    }
    fn covering(&self, t: f64) -> Option<&ContactCurve> {
        let c = self.output.as_ref()?;
        let [a, b] = c.time_range();
        (t.is_finite() && t >= a && t <= b).then_some(c)
    }
}
