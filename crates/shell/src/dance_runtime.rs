// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
//! Song-clock adapter: received chart rows → rolling contacts → prepared poses.
use super::hand::ProceduralPose;
use ddi_hand_plan::RollingPlanner;
use ddi_hand_pose::{DanceRuntime, Model, Plan};
use std::{cell::RefCell, rc::Rc};

/// The bundled hand calibration (763 KB of JSON), read once a thread.
pub(crate) fn bundled_model() -> Rc<Model> {
    thread_local! {
        static MODEL: Rc<Model> = Rc::new(
            Model::load(include_bytes!("../../../assets/hand/model.json"))
                .expect("accepted calibration"),
        );
    }
    MODEL.with(Rc::clone)
}

/// The longest a frame prepares motion, seconds.
const MOST_PER_FRAME: f64 = 0.008;
/// The share of the time between frames preparing motion takes, as much
/// as a frame of 60 Hz spends in 4 ms.
const SHARE: f64 = 0.24;
/// With this much motion ready (seconds) it takes half its share, so the
/// clusters of work when a plan is published spread over more frames.
const AMPLE: f64 = 1.5;
/// With less than this ready it takes twice its share, to the most a frame
/// may: motion running out ends the hand for the play.
const SHORT: f64 = 0.75;

/// How long a frame may prepare motion, for a display refreshing every
/// `period` seconds and `ready` seconds of motion ready (none before the
/// first is). It follows the refresh and not the frame just shown: time
/// given to a late frame makes the next one late too.
fn frame_budget(period: f64, ready: Option<f64>) -> f64 {
    let share = match ready {
        Some(r) if r >= AMPLE => SHARE / 2.,
        Some(r) if r < SHORT => SHARE * 2.,
        _ => SHARE,
    };
    (share * period).min(MOST_PER_FRAME)
}

pub struct RuntimePreview {
    model: Rc<Model>,
    mat: ddi_character::appearance::Geometry,
    /// The last sample shown, for the debug state ([`Self::sample_debug`]).
    shown: Option<(f64, ddi_hand_pose::Parameters)>,
    origin: f64,
    duration: f64,
    first_contact: Option<f64>,
    source: Vec<ddi_chart::Note>,
    timing: ddi_chart::TimingMap,
    cursor: usize,
    planner: RollingPlanner,
    planning_ms: f64,
    planning_peak_ms: f64,
    plan: Option<Plan>,
    runtime: Option<DanceRuntime>,
    known: f64,
    certified: f64,
    window: f64,
    error: Option<String>,
    pub payload: Rc<RefCell<ProceduralPose>>,
    last_time: Option<f64>,
    /// The time between frames (seconds), a late frame moving it little.
    period: f64,
    presentation_revisions: usize,
    pub available: bool,
    pub work_ms: f64,
    pub peak_work_ms: f64,
    pub starved_frames: usize,
    pub resets: usize,
}
impl RuntimePreview {
    /// Bind the notes gameplay judges, after modifiers and song offsets,
    /// as two fingers can press them: at most two at once (`At once: up
    /// to two`) and No Stretch, on a copy of the hand's own. The play itself
    /// judges every note, the hand an autoplay beside it.
    pub fn for_player(player: &ddi_engine::player::Player, layout: &str) -> Result<Self, String> {
        if usize::from(player.lanes()) != ddi_hand_plan::layout_lanes(layout)? {
            return Err("dancing hand layout/lane count mismatch".into());
        }
        let table = &player.ruleset().judge;
        if table.hold_window_seconds() < 0.22
            || table.mine_window_seconds() > 0.09
            || table.max_early() > 0.25
            || table.taps_only
        {
            return Err("dancing hand judge windows are outside supported contact budgets".into());
        }
        let judged = player.judge().notes();
        if judged
            .iter()
            .any(|n| n.judgeable && !n.note.kind.hand_supported())
        {
            return Err(
                "dancing hand supports taps, holds, rolls, mines, shocks and lifts; this chart has other judged note types"
                    .into(),
            );
        }
        let source: Vec<_> = judged
            .iter()
            .filter(|n| n.judgeable)
            .map(|n| n.note.clone())
            .collect();
        let lanes =
            ddi_chart::Layout::builtin(layout).ok_or("dancing hand layout is not built in")?;
        let source = ddi_hand_plan::prepare_notes(&source, &lanes);
        let first = judged
            .iter()
            .filter(|n| n.judgeable)
            .map(|n| n.seconds)
            .reduce(f64::min)
            .ok_or("dancing hand has no judged notes")?;
        // Leave room for the first lift when a chart starts at/before zero.
        // Only coordinates change: music hit times stay exactly as judged.
        let origin = (first - 0.5).min(0.);
        let mut timing = player.timing().clone();
        timing.offset_seconds += origin;
        let duration = player.song_end_time() + 3. - origin;
        if !origin.is_finite() || !duration.is_finite() || duration <= 0. {
            return Err("invalid dancing hand timeline".into());
        }
        let standard = ddi_hand_plan::standard_rules();
        let rules = if standard.iter().any(|(_, t)| t == table) {
            standard
        } else {
            vec![(player.ruleset().id.clone(), table.clone())]
        };
        Self::from_notes(source, timing, 3., origin, duration, layout, rules)
    }
    fn from_notes(
        source: Vec<ddi_chart::Note>,
        timing: ddi_chart::TimingMap,
        window: f64,
        origin: f64,
        duration: f64,
        layout: &str,
        rules: Vec<(String, ddi_engine::rules::JudgeTable)>,
    ) -> Result<Self, String> {
        let mat = ddi_hand_plan::layout_geometry(layout)?;
        let first_contact = source.first().map(|n| timing.seconds_at(n.tick));
        let planner = RollingPlanner::configured(window, timing.clone(), layout, rules)?;
        let model = bundled_model();
        let q = Plan::empty(duration).seed(&model, 0., None);
        let payload = Rc::new(RefCell::new(ProceduralPose {
            worlds: model.joint_worlds(&q),
            weights: model.morph_weights(&q),
        }));
        Ok(Self {
            model,
            mat,
            shown: None,
            origin,
            duration,
            first_contact,
            source,
            timing,
            cursor: 0,
            planner,
            planning_ms: 0.,
            planning_peak_ms: 0.,
            plan: None,
            runtime: None,
            known: 0.,
            certified: 0.,
            window,
            error: None,
            payload,
            last_time: None,
            period: 1. / 120.,
            presentation_revisions: 0,
            available: false,
            work_ms: 0.,
            peak_work_ms: 0.,
            starved_frames: 0,
            resets: 0,
        })
    }
    pub fn mat(&self) -> ddi_character::appearance::Geometry {
        self.mat
    }
    pub fn start_preloaded(&mut self) {
        self.last_time = None;
    }
    pub fn failure(&self) -> Option<&str> {
        self.error.as_deref().or_else(|| {
            self.runtime
                .as_ref()?
                .report
                .failure
                .as_ref()
                .map(|f| f.reason)
        })
    }
    pub fn origin(&self) -> f64 {
        self.origin
    }

    /// The play stepped down to an easier chart (a match's "At zero
    /// life: Step down"): the hand's notes not given to its planner yet
    /// are the play's from now on (the new chart's, from the boundary;
    /// prepared as the first were, on `layout`). Notes the planner was
    /// given already stay (an autoplay's, never judged); new ones it has
    /// planned past are left out, so its rows stay in order.
    pub fn step_down(&mut self, player: &ddi_engine::player::Player, layout: &str) {
        let Some(lanes) = ddi_chart::Layout::builtin(layout) else {
            return;
        };
        let after = self
            .source
            .get(..self.cursor)
            .and_then(|s| s.last())
            .map_or(f64::NEG_INFINITY, |n| self.timing.seconds_at(n.tick))
            .max(self.known);
        let incoming: Vec<ddi_chart::Note> = player
            .judge()
            .notes()
            .iter()
            .filter(|n| n.judgeable && n.state == ddi_engine::NoteState::Pending)
            .map(|n| n.note.clone())
            .collect();
        let incoming = ddi_hand_plan::prepare_notes(&incoming, &lanes);
        self.source.truncate(self.cursor);
        let timing = &self.timing;
        self.source.extend(
            incoming
                .into_iter()
                .filter(|n| timing.seconds_at(n.tick) > after + 1e-6),
        );
    }
    pub fn duration(&self) -> f64 {
        self.duration
    }
    fn receive(&mut self, now: f64) {
        let known = (now + self.window).min(self.duration);
        let start = self.cursor;
        while self.cursor < self.source.len()
            && self.timing.seconds_at(self.source[self.cursor].tick) <= known
        {
            self.cursor += 1;
        }
        if let Err(e) = self.planner.push(
            now,
            &self.source[start..self.cursor],
            known,
            self.cursor == self.source.len(),
        ) {
            self.error = Some(e);
            return;
        }
        // General play has not adopted the effective input yet. This fixed-chart
        // preview must judge exactly what it plans, including ongoing holds.
        if self.planner.report.removed > 0 {
            self.error = Some("planned input differs from the chart gameplay judges".into());
            return;
        }
        self.planner.request(now);
    }
    fn request_motion(&mut self, now: f64) {
        if let (Some(r), Some(p)) = (&mut self.runtime, &self.plan) {
            r.request_bounded(
                p,
                now,
                self.known,
                self.certified,
                self.known >= self.duration,
            );
        }
    }
    /// One checkpoint begun at `began`: when it ended, or `None` if there
    /// was nothing to do or it failed.
    fn step(&mut self, now: f64, began: f64, clock: &dyn Fn() -> f64) -> Option<f64> {
        if self.planner.stage() != "idle" {
            let search = self.planner.stage() == "search";
            self.planner.step();
            let ended = clock();
            let elapsed = ended - began;
            if search {
                self.planner.charge_search(elapsed);
            }
            if elapsed.is_finite() && elapsed >= 0. {
                self.planning_ms += elapsed * 1000.;
                self.planning_peak_ms = self.planning_peak_ms.max(elapsed * 1000.);
            }
            if let Some(e) = &self.planner.report.failure {
                self.error = Some(e.clone());
                self.available = false;
                return None;
            }
            if let Some(publication) = self.planner.take_publication() {
                match publication.plan.pose_plan() {
                    Ok(mut plan) => {
                        // The audio/motion endpoint is metadata; only received
                        // heads and declared hold obligations enter this plan.
                        plan.center_contacts(self.plan.as_ref().map(|old| (old, self.certified)));
                        plan.duration = self.duration;
                        plan.rebuild_edges();
                        self.known = publication.known_until;
                        self.certified = publication.certified_until;
                        if self.runtime.is_none() {
                            self.runtime =
                                Some(DanceRuntime::new(&self.model, &plan, self.window).unwrap());
                        }
                        self.plan = Some(plan);
                        self.request_motion(now);
                    }
                    Err(e) => {
                        self.error = Some(e);
                        self.available = false;
                        return None;
                    }
                }
            }

            return Some(ended);
        }

        let worked = self
            .runtime
            .as_mut()
            .is_some_and(|r| r.step(&self.model, now));
        worked.then(clock)
    }
    pub fn prepare(&mut self, time: f64, clock: &dyn Fn() -> f64) {
        self.work_ms = 0.;
        if !time.is_finite() || self.error.is_some() {
            return;
        }
        let ready = self.runtime.as_ref().is_some_and(|r| r.report.ready);
        let timeline = time.clamp(0., self.duration);
        if let Some(last) = self.last_time {
            if (time - last).abs() > self.window {
                self.reject_seek();
                return;
            }
            if time <= last && ready {
                // Display predictions can revise an upcoming refresh slightly
                // backwards. Sample its exact already-checked pose, without
                // rewinding received notes, committed contacts or preparation.
                if self.runtime.as_ref().unwrap().pose(timeline).is_none() {
                    self.reject_seek();
                    return;
                }
                self.presentation_revisions += usize::from(time < last);
                self.present(time, timeline);
                return;
            }
            if time < last - 1e-6 {
                self.reject_seek();
                return;
            }
            if time == last && time >= 0. {
                return;
            }
        }
        let delta = self.last_time.map_or(1. / 60., |v| (time - v).max(0.));
        self.last_time = Some(time);
        // No output prefix exists during bootstrap. Prepare its fixed first
        // window deterministically; the first actual contact is the deadline.
        let now = if ready { timeline } else { 0. };
        if delta > 0. {
            self.period += (delta.min(self.period * 1.25) - self.period) * 0.05;
        }
        let ahead = self
            .runtime
            .as_ref()
            .filter(|r| r.report.ready)
            .map(|r| r.report.covered_until - now);
        let budget = (frame_budget(self.period, ahead) * if self.window < 3. { 2. } else { 1. })
            .min(MOST_PER_FRAME);
        let start = clock();
        if start.is_finite() {
            self.receive(now);
            if self.error.is_some() {
                self.available = false;
                return;
            }
            self.request_motion(now);

            // One read a pass: each checkpoint begins when the last one ended.
            let mut at = clock();
            for _ in 0..100000 {
                let elapsed = at - start;
                if !(elapsed >= 0. && elapsed < budget) {
                    break;
                }
                let Some(ended) = self.step(now, at, clock) else {
                    break;
                };
                at = ended;
            }
            let elapsed = clock() - start;
            if elapsed.is_finite() {
                self.work_ms = elapsed.max(0.) * 1000.;
                self.peak_work_ms = self.peak_work_ms.max(self.work_ms);
            }
        }

        if !self.runtime.as_ref().is_some_and(|r| r.report.ready)
            && self.first_contact.is_some_and(|t| timeline >= t)
        {
            self.error = Some("initial hand motion missed its first contact deadline".into());
            self.available = false;
            return;
        }
        self.present(time, timeline);
    }
    fn reject_seek(&mut self) {
        self.error = Some("restart play after a seek in rolling contact preview".into());
        self.available = false;
        self.resets += 1;
    }
    fn present(&mut self, time: f64, timeline: f64) {
        let Some(runtime) = &self.runtime else {
            return;
        };
        let pose = runtime.pose(timeline);
        self.available = pose.is_some() && runtime.report.failure.is_none() && self.error.is_none();
        if pose.is_none() && runtime.report.ready && time >= 0. {
            self.starved_frames += 1;
        }
        if let Some(pose) = pose {
            let mut payload = self.payload.borrow_mut();
            payload.worlds = self.model.joint_worlds(&pose);
            payload.weights = self.model.morph_weights(&pose);
            self.shown = Some((timeline, pose));
        }
    }
    /// The last sample shown against its targets, worked out when the
    /// debug state is read rather than every frame.
    pub fn sample_debug(&self) -> serde_json::Value {
        let (Some((timeline, pose)), Some(runtime), Some(plan)) =
            (&self.shown, &self.runtime, &self.plan)
        else {
            return serde_json::Value::Null;
        };
        let geometry = self.model.geometry(pose);
        let targets = plan.targets(*timeline);
        serde_json::json!({"time":timeline,"stream":runtime.stream_at(*timeline),"parameters":pose,"targets":targets.tips,"contact":targets.contact,"actual":geometry.tips,"knuckle_mm":geometry.knuckles})
    }
    pub fn debug(&self) -> serde_json::Value {
        let report = self
            .runtime
            .as_ref()
            .map(|r| r.report.clone())
            .unwrap_or_default();
        let p = &self.planner.report;
        let planning_report=p.last.as_ref().map(|r|serde_json::json!({"status":r.status,"search_seconds":r.search_seconds,"checkpoints":r.checkpoints,"reason":r.reason,"judge":r.judge,"attempts":r.attempts}));
        serde_json::json!({"target_policy":"tile-centers-with-shared-slots","origin":self.origin,"duration":self.duration,"source_notes":self.source.len(),"window":self.window,"stage":if self.planner.stage()=="idle" {self.runtime.as_ref().map_or("idle",|r|r.stage())}else{self.planner.stage()},"available":self.available,
            "headroom":report.covered_until-self.last_time.unwrap_or(0.).max(0.),"work_ms":self.work_ms,"peak_work_ms":self.peak_work_ms,"starved_frames":self.starved_frames,"presentation_revisions":self.presentation_revisions,"resets":self.resets,"report":report,"error":self.error,
            "planner":{"source":"chart","mode":"rolling","window":self.window,"stage":self.planner.stage(),"known_until":p.known_until,"certified_until":p.certified_until,
                "publications":p.publications,"checkpoints":p.checkpoints,"peak_retained":p.peak_retained,"removed_notes":p.removed,"received_notes":p.received,
                "events":p.committed_heads,"holds":p.committed_holds,"active_ms":self.planning_ms,"peak_checkpoint_ms":self.planning_peak_ms,"report":planning_report,"at_once":"two","failure":p.failure}})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn a_frames_budget_follows_the_refresh_and_what_is_ready() {
        let refresh = 1. / 120.;
        // Nothing ready yet, and the middle of the range: the share.
        assert!((frame_budget(refresh, None) - 0.002).abs() < 1e-12);
        assert!((frame_budget(refresh, Some(1.)) - 0.002).abs() < 1e-12);
        // Plenty ready: half; running out: twice.
        assert!((frame_budget(refresh, Some(2.)) - 0.001).abs() < 1e-12);
        assert!((frame_budget(refresh, Some(0.5)) - 0.004).abs() < 1e-12);
        // Never over a frame's most, however slow the display.
        assert_eq!(frame_budget(1. / 30., Some(0.5)), MOST_PER_FRAME);
    }

    fn ordinary_song(offset: f64) -> ddi_chart::Song {
        ddi_chart::formats::sm::parse_sm(&format!("#TITLE:Ordinary four-panel;#OFFSET:{offset};#BPMS:0=120,12=150;#STOPS:8=0.25;#NOTES:dance-single::Medium:5:0,0,0,0,0:\n1000\n0100\n0010\n0001,\n1100\n0011\n2000\n0000,\n0000\n0001\n3000\n0111,\n1111\n0100\n0001\n1000;" )).unwrap()
    }

    /// A play of every note, as the game plays one beside the hand.
    fn ordinary_player(
        song: &ddi_chart::Song,
        turn: ddi_engine::Turn,
    ) -> ddi_engine::player::Player {
        ddi_engine::player::Player::new(
            song,
            0,
            ddi_engine::rules::presets::itg(),
            ddi_engine::player::PlayOptions {
                transform: ddi_engine::TransformOptions {
                    turn,
                    ..Default::default()
                },
                seed: 73,
                ..Default::default()
            },
        )
    }

    #[test]
    fn ordinary_charts_keep_judged_times_and_contacts_with_turns_offsets_and_immediate_notes() {
        for (layout, offset, turn) in [
            ("dance-double", 0., ddi_engine::Turn::Off),
            ("dance-double", 0.3, ddi_engine::Turn::Mirror),
            ("dance-double", -3.1, ddi_engine::Turn::Off),
            ("dance-double", -3.1, ddi_engine::Turn::Mirror),
            ("dance-double", -0.5, ddi_engine::Turn::Shuffle),
            ("dance-single", 0., ddi_engine::Turn::Off),
            ("dance-single", -1.5, ddi_engine::Turn::Mirror),
            ("dance-single", 0.3, ddi_engine::Turn::Shuffle),
            ("pump-halfdouble", 0., ddi_engine::Turn::Off),
            ("pump-halfdouble", 0.3, ddi_engine::Turn::Mirror),
            ("pump-halfdouble", -3.1, ddi_engine::Turn::Mirror),
            ("pump-double", 0., ddi_engine::Turn::Off),
            ("pump-double", 0.3, ddi_engine::Turn::Mirror),
            ("pump-double", -0.5, ddi_engine::Turn::Shuffle),
            ("pump-single", 0., ddi_engine::Turn::Off),
            ("pump-single", 0.3, ddi_engine::Turn::Mirror),
            ("danoni-7", 0., ddi_engine::Turn::Off),
            ("danoni-7", 0.3, ddi_engine::Turn::Mirror),
            ("techno-single8", 0., ddi_engine::Turn::Off),
            ("techno-single8", 0.3, ddi_engine::Turn::Mirror),
            ("techno-single8", -0.5, ddi_engine::Turn::Left),
            ("danoni-9A", 0., ddi_engine::Turn::Off),
            ("danoni-9A", 0.3, ddi_engine::Turn::Mirror),
            ("danoni-9A", -3.1, ddi_engine::Turn::Off),
            ("danoni-9B", 0., ddi_engine::Turn::Off),
            ("danoni-9B", 0.3, ddi_engine::Turn::Mirror),
        ] {
            let song = if matches!(layout, "pump-halfdouble" | "pump-double") {
                let mut song = ddi_chart::formats::sm::parse_sm(
                    "#BPMS:0=120;#NOTES:pump-double::Medium:5:0,0,0,0,0:1000000000;",
                )
                .unwrap();
                song.timing.offset_seconds = offset;
                song.charts[0].layout = layout.into();
                let panels = ddi_hand_plan::layout_panels(layout).unwrap();
                let mut notes = Vec::new();
                let mut add = |tick, p, kind| {
                    notes.push(ddi_chart::Note::new(
                        ddi_chart::Tick(tick),
                        panels.iter().position(|v| *v == p).unwrap() as u8,
                        kind,
                    ))
                };
                for (i, p) in [5, 9, 10, 14, 16, 3].into_iter().enumerate() {
                    add(i as i64 * 72, p, ddi_chart::NoteKind::Tap);
                }
                add(432, 5, ddi_chart::NoteKind::Tap);
                add(432, 14, ddi_chart::NoteKind::Tap);
                add(
                    528,
                    14,
                    ddi_chart::NoteKind::HoldHead {
                        end: ddi_chart::Tick(816),
                    },
                );
                add(624, 5, ddi_chart::NoteKind::Tap);
                add(720, 9, ddi_chart::NoteKind::Tap);
                add(912, 5, ddi_chart::NoteKind::Tap);
                if layout == "pump-double" {
                    for (i, p) in [1, 7, 18, 12].into_iter().enumerate() {
                        add(1008 + i as i64 * 72, p, ddi_chart::NoteKind::Tap);
                    }
                }
                notes.sort_by_key(|n| (n.tick, n.lane));
                song.charts[0].notes = notes;
                song
            } else if layout == "danoni-9A" {
                ddi_chart::formats::sm::parse_sm(&format!("#TITLE:Ordinary 9A;#OFFSET:{offset};#BPMS:0=120;#NOTES:danoni-9A::Medium:5:0,0,0,0,0:\n100010000\n010000000\n001000000\n000100000,\n000010001\n000001000\n000000100\n000000010,\n000020000\n000001000\n000000100\n000000001,\n000030000\n000010000\n0000M0000\n000010000;")).unwrap()
            } else if layout == "dance-double" {
                ddi_chart::formats::sm::parse_sm(&format!("#TITLE:Ordinary double;#OFFSET:{offset};#BPMS:0=120,12=150;#STOPS:8=0.25;#NOTES:dance-double::Medium:5:0,0,0,0,0:\n10000000\n00010000\n00001000\n00000001,\n00000010\n00000100\n00010000\n10000000,\n00020000\n00001000\n00000010\n00000100,\n00030000\n00000001\n00010000\n10000000;")).unwrap()
            } else if layout == "pump-single" {
                ddi_chart::formats::sm::parse_sm(&format!("#TITLE:Ordinary Pump;#OFFSET:{offset};#BPMS:0=120,12=150;#STOPS:8=0.25;#NOTES:pump-single::Medium:5:0,0,0,0,0:\n10000\n01000\n00100\n00010,\n00001\n10010\n01001\n00100,\n10010\n01001\n00100\n00001,\n10000\n01000\n00010\n00001;")).unwrap()
            } else if layout == "danoni-7" {
                ddi_chart::formats::sm::parse_sm(&format!("#TITLE:Ordinary seven-key;#OFFSET:{offset};#BPMS:0=120,12=150;#STOPS:8=0.25;#NOTES:danoni-7::Medium:5:0,0,0,0,0:\n1000000\n0100000\n0010000\n0001000,\n0000100\n0000010\n0000001\n0001000,\n0100010\n1000001\n0002000\n0000000,\n0000000\n0010000\n0003000\n1000100;")).unwrap()
            } else if layout == "techno-single8" {
                ddi_chart::formats::sm::parse_sm(&format!("#TITLE:Ordinary Techno;#OFFSET:{offset};#BPMS:0=120,12=150;#STOPS:8=0.25;#NOTES:techno-single8::Medium:5:0,0,0,0,0:\n10000000\n01000000\n00100000\n00010000,\n00001000\n00000100\n00000010\n00000001,\n10000100\n00100001\n02000000\n00000000,\n00000000\n00010000\n03000000\n00001010;")).unwrap()
            } else if layout == "danoni-9B" {
                ddi_chart::formats::sm::parse_sm(&format!("#TITLE:Ordinary nine-key;#OFFSET:{offset};#BPMS:0=120,12=150;#STOPS:8=0.25;#NOTES:danoni-9B::Medium:5:0,0,0,0,0:\n100000000\n010000000\n001000000\n000100000,\n000010000\n000001000\n000000100\n000000010,\n000000001\n100000001\n001000100\n020000000,\n000000000\n000100000\n030000000\n000001010;")).unwrap()
            } else {
                ordinary_song(offset)
            };
            let player = ordinary_player(&song, turn);
            let mut r = RuntimePreview::for_player(&player, layout).unwrap();
            let rendered =
                (ddi_hand_plan::layout_geometry(layout).unwrap().pads() == 2).then(|| {
                    let a = ddi_character::Character::load(ddi_character::HAND_MODEL).unwrap();
                    let pose = a.new_pose();
                    (a, pose)
                });
            let mut rendered = rendered;
            // The hand caps its own copy; gameplay keeps every judged note.
            let judged = player.judge().notes();
            assert_eq!(judged.len(), song.charts[0].notes.len());
            assert!(r.source.len() <= judged.len());
            let expected: Vec<_> = judged
                .iter()
                .filter(|n| r.source.contains(&n.note))
                .collect();
            assert_eq!(
                r.source,
                expected.iter().map(|n| n.note.clone()).collect::<Vec<_>>()
            );
            let expected = expected
                .into_iter()
                .filter(|n| {
                    !matches!(
                        n.note.kind,
                        ddi_chart::NoteKind::Mine | ddi_chart::NoteKind::Shock
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(r.window, 3.);
            if layout == "dance-single" {
                assert!(r.source.len() < song.charts[0].notes.len());
            }
            let ticks = Cell::new(0.);
            let clock = || {
                let t = ticks.get();
                ticks.set(t + 0.0000005);
                t
            };
            let mut checked = 0;
            let duration = r.duration;
            for frame in -240..=(duration * 120.).ceil() as i32 {
                let local = (f64::from(frame) / 120.).min(duration);
                r.prepare(local, &clock);
                assert!(
                    r.failure().is_none(),
                    "{layout}/{offset}/{turn:?} at {local}: {}",
                    r.debug()
                );
                if local < 0. {
                    continue;
                }
                assert!(
                    r.available,
                    "{layout}/{offset}/{turn:?} at {local}: {}",
                    r.debug()
                );
                while checked < expected.len() && expected[checked].seconds - r.origin <= local {
                    let n = &expected[checked];
                    let time = n.seconds - r.origin;
                    let plan = r.plan.as_ref().unwrap();
                    let e = plan
                        .events
                        .iter()
                        .find(|e| {
                            e.tick == n.note.tick.0
                                && ddi_dance_mat::MatLayout::from_id(layout)
                                    .unwrap()
                                    .targets(usize::from(n.note.lane))
                                    .iter()
                                    .any(|tile| tile.id() == e.panel)
                        })
                        .expect("same played event");
                    assert!((e.time + r.origin - n.seconds).abs() < 1e-9);
                    let sample = r
                        .runtime
                        .as_ref()
                        .unwrap()
                        .sample(time)
                        .expect("prepared exact head");
                    let g = r.model.geometry(&sample.pose);
                    let d = e.foot.index();
                    assert!(g.tips[d][2].abs() <= 0.25, "{time}: {:?}", g.tips[d]);
                    let target = plan.targets(time).tips[d];
                    assert!(
                        (0..3)
                            .map(|k| (g.tips[d][k] - target[k]).powi(2))
                            .sum::<f64>()
                            .sqrt()
                            <= 2.
                    );
                    if let Some((asset, pose)) = &mut rendered {
                        asset
                            .pose_from_joints(
                                &r.model.joint_worlds(&sample.pose),
                                &r.model.morph_weights(&sample.pose),
                                pose,
                            )
                            .unwrap();
                        let contacts = asset.contacts(pose);
                        let mut lights = ddi_character::appearance::ContactLights::default();
                        lights.update_on(r.mat(), time as f32, &contacts);
                        let slot = (0..18)
                            .find(|&i| {
                                ddi_character::appearance::physical_tile(r.mat(), i)
                                    .unwrap()
                                    .id()
                                    == e.panel
                            })
                            .unwrap();
                        assert!(
                            lights.values[slot] > 0.,
                            "{layout} {turn:?} at {time} panel {}: {contacts:?}",
                            e.panel
                        );
                    }
                    checked += 1;
                }
            }
            assert_eq!(checked, expected.len());
            assert_eq!(r.planner.report.removed, 0);
            assert_eq!(r.starved_frames, 0);
            assert_eq!(r.runtime.as_ref().unwrap().report.guide_late_jobs, 0);
        }
    }

    #[test]
    fn presentation_revisions_sample_prepared_motion_without_rewinding_work() {
        let song = ordinary_song(-1.);
        let player = ordinary_player(&song, ddi_engine::Turn::Off);
        let mut r = RuntimePreview::for_player(&player, "dance-single").unwrap();
        let ticks = Cell::new(0.);
        let clock = || {
            let t = ticks.get();
            ticks.set(t + 0.0000005);
            t
        };
        for _ in 0..2000 {
            r.prepare(-1., &clock);
            if r.available {
                break;
            }
        }
        assert!(r.available);
        r.start_preloaded();
        // The revision may happen during the audio countdown, before the hand
        // would otherwise appear in the first active song frame.
        r.prepare(-0.5, &clock);
        r.prepare(-0.5001, &clock);
        assert!(r.available && r.failure().is_none(), "{}", r.debug());
        assert_eq!(r.last_time, Some(-0.5));
        for frame in 0..=360 {
            let time = frame as f64 / 120.;
            r.prepare(time, &clock);
            assert!(r.available && r.failure().is_none(), "{}", r.debug());
            if frame < 2 {
                continue;
            }
            // A warm desktop pacer can revise its next refresh backwards by
            // a fraction of a millisecond, without seeking the audio clock.
            let revised = time - 0.0001;
            let expected = r.runtime.as_ref().unwrap().sample(revised).unwrap().pose;
            let checkpoints = r.runtime.as_ref().unwrap().report.checkpoints;
            let contacts = r.planner.report.checkpoints;
            let work = ticks.get();
            r.prepare(revised, &clock);
            assert!(r.failure().is_none(), "{}", r.debug());
            assert!(r.available);
            assert_eq!(r.last_time, Some(time));
            assert_eq!(r.runtime.as_ref().unwrap().report.checkpoints, checkpoints);
            assert_eq!(r.planner.report.checkpoints, contacts);
            assert_eq!(ticks.get(), work, "resampling must do no preparation");
            assert_eq!(r.payload.borrow().worlds, r.model.joint_worlds(&expected));
            // Repeating the high-water timestamp must restore its exact pose.
            let expected = r.runtime.as_ref().unwrap().sample(time).unwrap().pose;
            r.prepare(time, &clock);
            assert_eq!(r.payload.borrow().worlds, r.model.joint_worlds(&expected));
        }
        // Even a rewind shorter than the three-second window is rejected
        // once its pose is outside the retained, checked output.
        assert!(r.runtime.as_ref().unwrap().sample(0.1).is_none());
        r.prepare(0.1, &clock);
        assert!(r.failure().unwrap().contains("seek"));
    }

    #[test]
    fn ordinary_adapter_checks_rolls_layout_and_judges_and_caps_its_own_copy() {
        let mut song = ordinary_song(0.);
        let p = ordinary_player(&song, ddi_engine::Turn::Off);
        assert!(RuntimePreview::for_player(&p, "dance-couple").is_err());
        song.charts[0].notes[0].kind = ddi_chart::NoteKind::RollHead {
            end: ddi_chart::Tick(384),
        };
        let p = ordinary_player(&song, ddi_engine::Turn::Off);
        // The play judges every note; the hand plays them up to two at
        // once, the copy a capped play would judge.
        let r = RuntimePreview::for_player(&p, "dance-single").unwrap();
        assert!(p.judge().notes().len() > r.source.len());
        let mut options = *p.options();
        options.transform.at_once = ddi_engine::AtOnce::Two;
        let capped =
            ddi_engine::player::Player::new(&song, 0, ddi_engine::rules::presets::itg(), options);
        assert_eq!(
            r.source,
            capped
                .judge()
                .notes()
                .iter()
                .map(|n| n.note.clone())
                .collect::<Vec<_>>()
        );
        let mut rules = ddi_engine::rules::presets::itg();
        rules.judge.hold_window = 0.01;
        let p = ddi_engine::player::Player::new(&song, 0, rules, *p.options());
        assert!(RuntimePreview::for_player(&p, "dance-single").is_err());
    }

    #[test]
    fn initial_preparation_has_a_contact_deadline_without_a_published_prefix() {
        let song = ordinary_song(0.);
        let player = ordinary_player(&song, ddi_engine::Turn::Off);
        let mut r = RuntimePreview::for_player(&player, "dance-single").unwrap();
        let ticks = Cell::new(0.);
        let clock = || {
            let t = ticks.get();
            ticks.set(t + 0.01);
            t
        };
        r.prepare(0.4, &clock);
        assert!(r.failure().is_none());
        assert!(!r.available);
        r.prepare(0.51, &clock);
        assert!(r.failure().unwrap().contains("first contact deadline"));
        assert!(!r.available);
    }

    #[test]
    fn frozen_prestart_resumes_preparation_without_advancing_or_missing_contacts() {
        let song = ordinary_song(0.);
        let player = ordinary_player(&song, ddi_engine::Turn::Shuffle);
        let mut r = RuntimePreview::for_player(&player, "dance-single").unwrap();
        let ticks = Cell::new(0.);
        let clock = || {
            let t = ticks.get();
            ticks.set(t + 0.00005);
            t
        };
        for _ in 0..2000 {
            r.prepare(-1., &clock);
            assert!(r.failure().is_none(), "{}", r.debug());
            if r.available {
                break;
            }
        }
        assert!(r.available, "{}", r.debug());
        let before = r.runtime.as_ref().unwrap().sample(0.).unwrap().pose;
        let publications = r.runtime.as_ref().unwrap().report.publications;
        r.start_preloaded();
        // An audio countdown can begin earlier than the frozen preparation time.
        r.prepare(-2., &clock);
        assert!(r.available && r.failure().is_none(), "{}", r.debug());
        assert_eq!(r.runtime.as_ref().unwrap().sample(0.).unwrap().pose, before);
        assert_eq!(
            r.runtime.as_ref().unwrap().report.publications,
            publications
        );
        assert_eq!(r.starved_frames, 0);
        assert_eq!(r.resets, 0);
    }
}
