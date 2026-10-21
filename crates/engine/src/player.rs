//! The orchestrator: clock + judge + rules → frames and results.

use ddi_chart::{Layout, Note, NoteKind, Song, TimingMap};
use ddi_platform::{ClockSample, HostTime};
use serde::{Deserialize, Serialize};

use crate::appearance::Appearance;
use crate::clock::{ClockOptions, SongClock};
use crate::cover::{COVER_LABEL_SECONDS, CoverChange, CoverView, LaneCover};
use std::collections::VecDeque;

use crate::frame::{
    Frame, HoldFlash, HoldJudgements, HoldKind, HoldPlace, JudgementFlash, NoteSprite,
    ReceptorState, SkinEvent, SkinEventKind, SpriteKind,
};
use crate::history::{LoggedInput, PlayLog};
use crate::input::InputEvent;
use crate::judge::{Judge, JudgeEvent, JudgeEventKind, NoteState};
use crate::rules::ddr::{DdrClear, flare_points};
use crate::rules::{
    FailPolicy, FullCombo, Judgement, LifeType, Ruleset, ScoreCtx, ScoreView, Tally,
};
use crate::scroll::{ScrollAction, ScrollOptions, ScrollState};
use crate::transform::{self, TransformOptions};

/// Default [`Frame::visible_range`], in arrow heights.
pub const DEFAULT_VISIBLE_RANGE: f32 = 12.0;

/// Seconds a judgement or receptor flash is reported in frames.
const FLASH_SECONDS: f32 = 1.0;

/// Silence after the last note before the play finishes.
const END_MARGIN: f64 = 1.0;
/// Seconds what is judged by time alone (misses, mines passed, holds kept
/// or dropped) lags the clock in a live play ([`Player::update`]). Edges
/// reach the player late: a key or touch carries the time it happened,
/// while a frame updates at the time the frame began, and the browser
/// hands over an edge that happened during the last frame's work after
/// that frame's update (a frame or less, 7–17 ms); an output latency the
/// audio clock reports anew moves the heard time back by its step (8 ms
/// seen). An edge up to this late is judged at its own time, before the
/// time-judged events after it; only those events (a miss, a hold kept)
/// are reported this much later. Later edges count as happening at the
/// time judged ([`Results::late_edges`]). Replays feed judged times and
/// do not depend on it.
pub const JUDGE_LAG: f64 = 0.03;
/// Skin events kept for the renderer to catch up on.
const SKIN_EVENTS: usize = 256;
/// `[Player] BrightGhostComboThreshold` (`Themes/_fallback/metrics.ini`):
/// above this combo explosions are bright.
const BRIGHT_COMBO: u32 = 100;
/// Seconds the arrows still to come are hidden after a shock is stepped
/// on. DDR hides them "for a short time" (RemyWiki's DDR glossary), "a
/// few frames" (Rhythmic Keystrokes); no measured figure is published,
/// so this is an estimate (`docs/research/rules-ddr-itg-dwi.md`).
pub const SHOCK_BLIND_SECONDS: f64 = 0.2;

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayOptions {
    pub scroll: ScrollOptions,
    /// Turn and Cut, applied to the chart before judging.
    pub transform: TransformOptions,
    /// Hidden / Sudden / Stealth; drawn only.
    pub appearance: Appearance,
    /// The lane covers as the play starts (Sudden+, Hidden+); drawn only,
    /// changed during play ([`Player::set_cover`]).
    pub cover: LaneCover,
    /// Fixes the Shuffle permutation, so a play can be reproduced.
    pub seed: u64,
    pub clock: ClockOptions,
    /// Arrow heights above the receptor to include in frames.
    pub visible_range: f32,
    /// Draw a note exactly on the receptor in the frame whose presentation
    /// time is closest to the note's arrival (within half a frame), so the
    /// landing is a crisp single-frame event instead of straddling two
    /// frames. Judging is unaffected.
    pub receptor_snap: bool,
}

impl Default for PlayOptions {
    fn default() -> PlayOptions {
        PlayOptions {
            scroll: ScrollOptions::default(),
            transform: TransformOptions::default(),
            appearance: Appearance::Visible,
            cover: LaneCover::default(),
            seed: 0,
            clock: ClockOptions::default(),
            visible_range: DEFAULT_VISIBLE_RANGE,
            receptor_snap: true,
        }
    }
}

/// End-of-play summary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Results {
    /// Counts per judgement (per row when the ruleset is per-row) and
    /// hold/mine/boo totals.
    pub tally: Tally,
    pub max_combo: u32,
    pub score: ScoreView,
    pub grade: String,
    pub full_combo: FullCombo,
    pub failed: bool,
    /// Hits early by more than the ruleset's dead zone
    /// ([`JudgeTable::fast_slow`](crate::rules::JudgeTable::fast_slow)).
    pub fast: u32,
    /// Hits late by more than the dead zone.
    pub slow: u32,
    /// Hits counted in the mean and deviation (every hit, fast, slow or
    /// neither).
    #[serde(default)]
    pub hits: u32,
    /// Edges that reached the play later than it had judged by time
    /// ([`JUDGE_LAG`]) and were judged at that later time: should stay 0.
    #[serde(default)]
    pub late_edges: u32,
    /// Mean of hit deltas in seconds (0 when nothing was hit).
    pub mean_delta: f64,
    /// Population standard deviation of hit deltas.
    pub stddev_delta: f64,
    /// The chart transforms played.
    pub transform: TransformOptions,
    /// `take_from[new_lane] = old_lane` of the turn played.
    pub lane_map: Vec<u8>,
    /// The play's seed, which reproduces a Shuffle.
    pub seed: u64,
    pub appearance: Appearance,
    /// The lane covers as the play ended (the last shown, if the play
    /// ended with them toggled off).
    #[serde(default)]
    pub cover: LaneCover,
    pub scroll_action: ScrollAction,
    /// Notes were removed or simplified ([`TransformOptions::is_assist`]).
    pub assist: bool,
    /// Every judgement in play order, as judged (before a per-row
    /// ruleset groups a row): what the results' panels draw (timing
    /// histogram, offsets over the song, lanes).
    #[serde(default)]
    pub timeline: Vec<Judged>,
    /// The life after each change, `[song second, life 0..=1]`, from
    /// the start of play.
    #[serde(default)]
    pub life: Vec<[f32; 2]>,
    /// The song second the gauge first said failed, whatever the fail
    /// policy made of it (a play under fail off or at the end goes on).
    #[serde(default)]
    pub gauge_failed_at: Option<f32>,
    /// Under DDR A's rules, the gauge, FLARE rank and FLARE SKILL
    /// (`docs/plans/rulesets.md`).
    #[serde(default)]
    pub ddr: Option<DdrClear>,
}

/// One judgement of a play ([`Results::timeline`]); a tap's offset to
/// the microsecond.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Judged {
    /// Song second it happened at.
    pub time: f32,
    /// The lane played (after a turn).
    pub lane: u8,
    pub kind: JudgeEventKind,
}

impl Results {
    /// Taps and misses per lane played: `[lane][tier]`, tiers in
    /// [`Judgement::ALL`] order (the best first, misses last).
    pub fn taps_by_lane(&self, lanes: usize) -> Vec<[u32; 6]> {
        let mut out = vec![[0; Judgement::ALL.len()]; lanes];
        for j in &self.timeline {
            if let JudgeEventKind::Tap(t, _) = j.kind
                && let Some(l) = out.get_mut(usize::from(j.lane))
            {
                l[t.index()] += 1;
            }
        }
        out
    }
}

/// Partially judged row (per-row combo grouping).
#[derive(Clone, Debug, Default)]
struct RowAcc {
    judged: u32,
    worst: Option<JudgeEvent>,
}

/// One play of one chart.
pub struct Player {
    timing: TimingMap,
    ruleset: Ruleset,
    options: PlayOptions,
    chart_assist: bool,
    /// The chart's meter ([`ScoreCtx::meter`]).
    meter: u32,
    clock: SongClock,
    judge: Judge,
    lanes: u8,
    held: Vec<bool>,
    rows: Vec<RowAcc>,
    ctx: ScoreCtx,
    started: bool,
    song_offset: f64,
    /// Edges judged later than they happened ([`Results::late_edges`]).
    late_edges: u32,
    combo: u32,
    max_combo: u32,
    tally: Tally,
    fast: u32,
    slow: u32,
    delta_sum: f64,
    delta_sq_sum: f64,
    delta_count: u32,
    /// `(judgement, delta, song_time, combo_at)`.
    last_flash: Option<(Judgement, f64, f64, u32)>,
    /// The last shock stepped on: from when to when the notes to come are
    /// hidden.
    shock_blind: Option<(f64, f64)>,
    /// Every lane's presses judged in this one ([`Player::judge_all_in`]).
    judged_lane: Option<u8>,
    /// While a press is applied with `judged_lane`: the lane pressed,
    /// where its flashes show.
    pressing: Option<u8>,
    /// Per lane `(judgement, song_time)`.
    lane_flash: Vec<Option<(Judgement, f64)>>,
    /// Per lane, a hold's or shock's outcome `(held, song_time)`, and
    /// where they show.
    hold_flash: Vec<Option<(bool, f64)>>,
    hold_place: HoldPlace,
    /// Holds' own outcomes are shown (not with ticked holds: their
    /// checkpoints are their judgement; shocks' still are).
    holds_shown: bool,
    failed: bool,
    /// When the gauge first said failed ([`Results::gauge_failed_at`]).
    gauge_failed_at: Option<f64>,
    finished: bool,
    /// Every press and release judged, at its judged time ([`Player::log`]).
    log: PlayLog,
    /// [`Results::timeline`] and [`Results::life`] as they grow.
    timeline: Vec<Judged>,
    life: Vec<[f32; 2]>,
    /// Display frame interval in seconds, for receptor snapping.
    frame_interval: f64,
    /// `take_from[new_lane] = old_lane` of the turn.
    lane_map: Vec<u8>,
    /// Displayed beat of each judged note's head and tail (they never
    /// change, and computing them is linear in the scroll segments).
    displayed: Vec<(f64, f64)>,
    /// The (lane, stable note index) pairs of each judged row.
    row_notes: Vec<Vec<(u8, usize)>>,
    /// Notes of each row judged so far, for explosions: in the dance game
    /// StepMania flashes a row once it is completely judged, whatever the
    /// ruleset counts (`m_bCountNotesSeparately`, `GameManager.cpp:506`).
    row_judged: Vec<u32>,
    /// The latest skin events, oldest first.
    skin_events: VecDeque<SkinEvent>,
    autoplay: bool,
    next_seq: u64,
    /// The lane covers now, the song second they last changed at, and the
    /// last ones shown ([`Results::cover`]).
    cover: LaneCover,
    cover_changed: Option<f64>,
    shown_cover: LaneCover,
}

impl Player {
    /// Sets up a play of `song.charts[chart_index]` with the chart
    /// transformed by `options.transform`.
    ///
    /// # Panics
    /// If `chart_index` is out of range.
    pub fn new(song: &Song, chart_index: usize, ruleset: Ruleset, options: PlayOptions) -> Player {
        let chart = &song.charts[chart_index];
        let timing = chart.timing(song).clone();
        let chart_lanes = chart
            .notes
            .iter()
            .map(|n| usize::from(n.lane) + 1)
            .max()
            .unwrap_or(0);
        let layout = song
            .layout_of(chart)
            .unwrap_or_else(|| Layout::generic(chart_lanes));
        let mut transformed =
            transform::apply(&chart.notes, &layout, &options.transform, options.seed);
        if ruleset.judge.shock_rows {
            shock_rows(&mut transformed.notes, layout.lane_count().max(chart_lanes));
        }
        let judge = Judge::new(&transformed.notes, &timing, ruleset.judge.clone());
        let lanes = layout
            .lane_count()
            .max(chart_lanes)
            .max(judge.lane_count())
            .min(usize::from(u8::MAX)) as u8;
        let ruleset = ruleset.fresh();
        // StepMania's `HoldJudgment`: one a column in the dance game, none
        // in the pump game (`HoldJudgmentYStandard` −99999 there), and
        // for shocks only with ticked holds (their checkpoints are judged
        // instead); Dancing☆Onigiri shows its freeze judgement beside
        // the tap judgement.
        let holds_shown = !ruleset.judge.hold.is_ticked();
        let hold_place = if layout.id.starts_with("pump") {
            HoldPlace::None
        } else if ruleset.id == "danoni" {
            HoldPlace::Centre
        } else {
            HoldPlace::Lanes
        };
        let mut player = Player {
            chart_assist: chart.derivation.is_some(),
            meter: chart.meter,
            timing,
            clock: SongClock::new(options.clock),
            options,
            lanes,
            held: vec![false; usize::from(lanes)],
            rows: Vec::new(),
            ctx: ScoreCtx::default(),
            started: false,
            song_offset: 0.0,
            late_edges: 0,
            combo: 0,
            max_combo: 0,
            tally: Tally::default(),
            fast: 0,
            slow: 0,
            delta_sum: 0.0,
            delta_sq_sum: 0.0,
            delta_count: 0,
            last_flash: None,
            shock_blind: None,
            judged_lane: None,
            pressing: None,
            lane_flash: vec![None; usize::from(lanes)],
            hold_flash: vec![None; usize::from(lanes)],
            hold_place,
            holds_shown,
            failed: false,
            gauge_failed_at: None,
            finished: false,
            log: PlayLog::default(),
            timeline: Vec::new(),
            life: Vec::new(),
            frame_interval: 1.0 / 60.0,
            lane_map: transformed.take_from,
            displayed: Vec::new(),
            row_notes: Vec::new(),
            row_judged: Vec::new(),
            skin_events: VecDeque::new(),
            autoplay: false,
            next_seq: 0,
            cover: options.cover,
            cover_changed: None,
            shown_cover: options.cover,
            judge,
            ruleset,
        };
        player.row_notes = vec![Vec::new(); player.judge.rows().len()];
        for (index, n) in player.judge.notes().iter().enumerate() {
            if let Some(r) = n.row {
                player.row_notes[r].push((n.note.lane, index));
            }
        }
        player.displayed = player
            .judge
            .notes()
            .iter()
            .map(|n| {
                let at = |t: ddi_chart::Tick| player.timing.displayed_beat(t.beat());
                (at(n.note.tick), at(n.note.end_tick()))
            })
            .collect();
        player.reset_counts();
        player
    }

    fn reset_counts(&mut self) {
        self.rows = vec![RowAcc::default(); self.judge.rows().len()];
        self.row_judged = vec![0; self.judge.rows().len()];
        self.ctx = ScoreCtx {
            steps: if self.ruleset.combo.per_row {
                self.judge.row_count()
            } else {
                self.judge.judged_note_count()
            },
            holds: self.judge.hold_count(),
            mines: self.judge.mine_count(),
            shocks: self.judge.shock_count(),
            combo: 0,
            ticked: self.ruleset.judge.hold.is_ticked(),
            checkpoints: self.judge.checkpoint_count(),
            w0: false,
            meter: self.meter,
        };
        self.ruleset.score.begin(&self.ctx);
        self.ruleset.gauge.begin(&self.ctx);
        self.timeline.clear();
        self.life = vec![[self.song_offset as f32, self.ruleset.gauge.life()]];
    }

    /// The audio was scheduled from `song_offset` seconds into the song at
    /// `start_context_time` on the audio clock. Notes before the offset are
    /// skipped.
    pub fn start(&mut self, start_context_time: f64, song_offset: f64) {
        self.clock.start(start_context_time, song_offset);
        self.started = true;
        self.song_offset = song_offset;
        self.log = PlayLog {
            start: song_offset,
            ..PlayLog::default()
        };
        if song_offset > 0.0 {
            self.judge.skip_before(song_offset);
            self.reset_counts();
        }
    }

    pub fn clock_sample(&mut self, s: ClockSample) {
        self.clock.update(s);
    }

    pub fn clock(&self) -> &SongClock {
        &self.clock
    }

    /// Replace the calibration offsets and rate mid-play (used by the
    /// calibration flow to apply a measured correction and keep measuring).
    pub fn set_clock_options(&mut self, options: crate::clock::ClockOptions) {
        self.clock.set_options(options);
    }

    /// Tell the player how long a display frame lasts (seconds); drives
    /// receptor snapping.
    pub fn set_frame_interval(&mut self, seconds: f64) {
        if seconds.is_finite() && seconds > 0.0 {
            self.frame_interval = seconds;
        }
    }

    pub fn judge(&self) -> &Judge {
        &self.judge
    }

    pub fn ruleset(&self) -> &Ruleset {
        &self.ruleset
    }

    pub fn options(&self) -> &PlayOptions {
        &self.options
    }

    pub fn timing(&self) -> &TimingMap {
        &self.timing
    }

    pub fn lanes(&self) -> u8 {
        self.lanes
    }

    /// `take_from[new_lane] = old_lane` of the turn played.
    pub fn lane_map(&self) -> &[u8] {
        &self.lane_map
    }

    pub fn started(&self) -> bool {
        self.started
    }

    pub fn finished(&self) -> bool {
        self.finished
    }

    pub fn failed(&self) -> bool {
        self.failed
    }

    pub fn combo(&self) -> u32 {
        self.combo
    }

    pub fn max_combo(&self) -> u32 {
        self.max_combo
    }

    pub fn tally(&self) -> &Tally {
        &self.tally
    }

    /// Chart totals fed to the score and gauge.
    pub fn score_ctx(&self) -> ScoreCtx {
        self.ctx
    }

    /// Song second after which the play finishes: last note end + 1 s
    /// (later while a hold's or shock's outcome still shows,
    /// [`Player::end_time`]).
    pub fn song_end_time(&self) -> f64 {
        self.judge.last_end_seconds().unwrap_or(self.song_offset) + END_MARGIN
    }

    /// When the play finishes as far as is known now: at
    /// [`Player::song_end_time`], or once the last O.K. or N.G. has been
    /// shown, so the last hold's outcome is not cut short (StepMania keeps
    /// drawing judgements through its out transition).
    pub fn end_time(&self) -> f64 {
        self.song_end_time().max(self.flashes_end())
    }

    /// When the holds' and shocks' outcomes shown so far are over.
    fn flashes_end(&self) -> f64 {
        self.hold_flash
            .iter()
            .flatten()
            .map(|&(held, at)| at + f64::from(self.hold_place.seconds(held)))
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Heard song time of a host timestamp, shifted by the judge offset: a
    /// positive offset means the notes are expected later, so the press
    /// counts as earlier (see [`crate::rules::JudgeTable::judge_offset`]).
    pub fn judge_time(&self, host: HostTime) -> f64 {
        self.clock.song_time_at_host(host) - self.ruleset.judge.judge_offset
    }

    /// Judges a press in any lane as one in `lane` (calibration by ear:
    /// any key for the one lane), while the receptors show the lane really
    /// pressed. `None`: each lane its own.
    pub fn judge_all_in(&mut self, lane: Option<u8>) {
        self.judged_lane = lane.filter(|&l| usize::from(l) < self.held.len());
    }

    /// The presses and releases judged so far, at the song time each was
    /// judged at ([`Player::input`]), and where the play started: what
    /// [`crate::history::replay`] judges again. Every edge that reached a
    /// started play before it ended is there, also those it ignored (a
    /// lane it does not have, a key already down). Not under
    /// [`Player::judge_all_in`], which a replay does not know.
    pub fn log(&self) -> &PlayLog {
        &self.log
    }

    /// The lane covers now.
    pub fn cover(&self) -> LaneCover {
        self.cover
    }

    /// Changes the lane covers at host time `host` (a key during play):
    /// drawn from the next frame, kept in the log at the song second it
    /// is judged at. Nothing judged changes.
    pub fn set_cover(&mut self, cover: LaneCover, host: HostTime) {
        let t = self.judge_time(host);
        self.set_cover_at(cover, t);
    }

    /// [`Player::set_cover`] at a judged song second (what a replay
    /// feeds). Ignored before the start, after the end, or when nothing
    /// changes.
    pub fn set_cover_at(&mut self, cover: LaneCover, song_time: f64) {
        if !self.started || self.finished || !song_time.is_finite() || cover == self.cover {
            return;
        }
        let time = to_microsecond(song_time);
        self.cover = cover;
        self.cover_changed = Some(time);
        if cover.is_on() {
            self.shown_cover = cover;
        }
        self.log.covers.push(CoverChange { time, cover });
    }

    /// Feeds a lane edge. Returns the per-note judge events it caused.
    ///
    /// It is judged at its heard song time, to the microsecond (what the
    /// log keeps exactly), and never before the time the play was last
    /// judged at: judging only moves forward ([`Judge::update`]). Updates
    /// judge [`JUDGE_LAG`] behind the clock, so an edge that reaches the
    /// player up to that late is judged at its own time; a later one
    /// counts as happening at the time judged ([`Results::late_edges`]).
    pub fn input(&mut self, ev: InputEvent) -> Vec<JudgeEvent> {
        if !self.started || self.finished {
            return Vec::new();
        }
        let t = self.judge_time(ev.host_time);
        self.input_at(ev.lane, ev.pressed, t)
    }

    /// [`Player::input`] at a judged song time (already after the clock,
    /// the latency and the offsets): what a replay feeds.
    pub fn input_at(&mut self, lane: u8, pressed: bool, song_time: f64) -> Vec<JudgeEvent> {
        if !self.started || self.finished || !song_time.is_finite() {
            return Vec::new();
        }
        let at = to_microsecond(song_time);
        let judged = self.judge.last_time().unwrap_or(f64::NEG_INFINITY);
        if at < judged {
            self.late_edges += 1;
        }
        let t = at.max(judged);
        if self.judged_lane.is_none() {
            self.log.inputs.push(LoggedInput {
                time: t,
                lane,
                pressed,
            });
        }
        if usize::from(lane) >= self.held.len() {
            return Vec::new();
        }
        self.held[usize::from(lane)] = pressed;
        let target = self.judged_lane.unwrap_or(lane);
        let events = if pressed {
            self.judge.press(target, t)
        } else if self.judged_lane.is_some() && self.held.iter().any(|&h| h) {
            // Another lane still holds the judged one down.
            Vec::new()
        } else {
            self.judge.release(target, t)
        };
        if pressed {
            // `ReceptorArrow::Step` plays the press's score at once; `Press`
            // comes from the next draw (`ReceptorArrow::DrawPrimitives`).
            // An older note missed on the way (`Judge::press` expires first)
            // is not the press's.
            let step = events
                .iter()
                .filter(|e| e.lane == target)
                .find_map(|e| match e.kind {
                    JudgeEventKind::Tap(j, _) if !j.is_miss() => Some(SkinEventKind::Step(Some(j))),
                    JudgeEventKind::HitMine | JudgeEventKind::HitShock => {
                        Some(SkinEventKind::StepMine)
                    }
                    _ => None,
                })
                .unwrap_or(SkinEventKind::Step(None));
            self.skin_event(lane, step, t);
            self.skin_event(lane, SkinEventKind::Press, t);
        } else {
            self.skin_event(lane, SkinEventKind::Lift, t);
        }
        self.pressing = self.judged_lane.map(|_| lane);
        let mut events = events;
        self.apply(&mut events);
        self.pressing = None;
        events
    }

    fn skin_event(&mut self, lane: u8, kind: SkinEventKind, song_time: f64) {
        if self.skin_events.len() >= SKIN_EVENTS {
            self.skin_events.pop_front();
        }
        self.skin_events.push_back(SkinEvent {
            seq: self.next_seq,
            lane,
            kind,
            song_time,
            autoplay: self.autoplay,
            danoni_judgments: self.ruleset.id == "danoni",
            note: None,
        });
        self.next_seq += 1;
    }

    /// The skin events numbered `seq` and on still kept, oldest first.
    pub fn skin_events_since(&self, seq: u64) -> impl Iterator<Item = &SkinEvent> {
        self.skin_events.iter().filter(move |e| e.seq >= seq)
    }

    /// Noteskin presentation hint only. Input timestamps, judgments, scores
    /// and replay logs follow the same path in both modes.
    pub fn set_autoplay(&mut self, autoplay: bool) {
        self.autoplay = autoplay;
    }

    /// Explosions are bright above a combo of 100.
    fn bright(&self) -> bool {
        self.combo > BRIGHT_COMBO
    }

    /// Advances to `host_now`: misses, hold life, fail and finish checks,
    /// judged [`JUDGE_LAG`] behind it. What is judged does not depend on
    /// how often this is called ([`Judge::update`]).
    pub fn update(&mut self, host_now: HostTime) -> Vec<JudgeEvent> {
        if !self.started || self.finished {
            return Vec::new();
        }
        let t = self.judge_time(host_now);
        self.update_at(t)
    }

    /// [`Player::update`] to a judged song time: what a replay calls (it
    /// too judges [`JUDGE_LAG`] behind; the play finishes once `song_time`
    /// passes [`Player::end_time`], every note judged).
    pub fn update_at(&mut self, song_time: f64) -> Vec<JudgeEvent> {
        if !self.started || self.finished || !song_time.is_finite() {
            return Vec::new();
        }
        let t = to_microsecond(song_time);
        let mut events = self.judge.update(to_microsecond(t - JUDGE_LAG));
        self.apply(&mut events);
        if !self.finished && t >= self.end_time() && self.judge.all_resolved() {
            self.finish();
        }
        events
    }

    fn finish(&mut self) {
        self.finished = true;
        self.ruleset.score.finish();
        if let FailPolicy::EndOfSong { min_life } = self.ruleset.fail
            && match min_life {
                Some(min) => self.ruleset.gauge.fails_at_end(min),
                None => self.ruleset.gauge.failed(),
            }
        {
            self.failed = true;
        }
    }

    /// Routes raw judge events through row grouping into combo/score/gauge.
    /// Those after the play ended (an immediate fail) are not judged, and
    /// are dropped from `events`.
    fn apply(&mut self, events: &mut Vec<JudgeEvent>) {
        for (i, ev) in events.iter().enumerate() {
            if self.finished {
                events.truncate(i);
                return;
            }
            // The offset to the microsecond, as a record keeps it
            // ([`crate::history::PlayTiming`]).
            let kind = match ev.kind {
                JudgeEventKind::Tap(j, delta) => JudgeEventKind::Tap(j, to_microsecond(delta)),
                other => other,
            };
            self.timeline.push(Judged {
                time: ev.song_time as f32,
                lane: ev.lane,
                kind,
            });
            if let JudgeEventKind::Tap(j, delta) = ev.kind
                && j != Judgement::Miss
            {
                match self.ruleset.judge.fast_slow(delta) {
                    Some(true) => self.fast += 1,
                    Some(false) => self.slow += 1,
                    None => {}
                }
                self.delta_sum += delta;
                self.delta_sq_sum += delta * delta;
                self.delta_count += 1;
                let shown = self.pressing.unwrap_or(ev.lane);
                self.lane_flash[usize::from(shown)] = Some((j, ev.song_time));
            }
            // `FlashGhostRow` runs before the row's score raises the combo.
            let bright = self.bright();
            let row = match (self.ruleset.combo.per_row, ev.kind, ev.note_index) {
                (true, JudgeEventKind::Tap(..), Some(idx)) => {
                    self.judge.note(idx).and_then(|n| n.row)
                }
                _ => None,
            };
            match row {
                Some(r) => {
                    let size = self.judge.rows()[r].size;
                    let acc = &mut self.rows[r];
                    acc.judged += 1;
                    let worse = match (&acc.worst, ev.kind) {
                        (Some(w), JudgeEventKind::Tap(j, _)) => {
                            !matches!(w.kind, JudgeEventKind::Tap(wj, _) if wj >= j)
                        }
                        _ => true,
                    };
                    if worse {
                        acc.worst = Some(*ev);
                    }
                    if acc.judged >= size
                        && let Some(worst) = acc.worst.take()
                    {
                        self.score_event(&worst);
                    }
                }
                None => self.score_event(ev),
            }
            self.flash(ev, bright);
        }
    }

    fn skin_note(&self, index: usize, song_time: f64) -> Option<crate::frame::SkinNote> {
        let n = self.judge.note(index)?;
        let &(beat, _) = self.displayed.get(index)?;
        let scroll = ScrollState::at(
            self.options.scroll,
            &self.timing,
            song_time + self.clock.options().visual_offset,
        );
        Some(crate::frame::SkinNote {
            index,
            y: scroll.y_displayed(beat, n.seconds) * n.note.speed_mul.unwrap_or(1.0),
        })
    }

    /// The explosions an event sets off (`Player::Step`, `FlashGhostRow`,
    /// `Player.cpp:2555-2575,2834-2856`).
    fn flash(&mut self, ev: &JudgeEvent, bright: bool) {
        match ev.kind {
            JudgeEventKind::Tap(j, _) => {
                let Some(r) = ev
                    .note_index
                    .and_then(|i| self.judge.note(i))
                    .and_then(|n| n.row)
                else {
                    return;
                };
                if !j.is_miss() {
                    let hit = ev
                        .note_index
                        .and_then(|index| self.skin_note(index, ev.song_time));
                    self.skin_event(
                        self.pressing.unwrap_or(ev.lane),
                        SkinEventKind::NoteTap {
                            judgement: j,
                            bright,
                        },
                        ev.song_time,
                    );
                    if let Some(event) = self.skin_events.back_mut() {
                        event.note = hit;
                    }
                }
                self.row_judged[r] += 1;
                // Every lane of the row with the last judgement, once the
                // row is completely judged by a press: a row that ends on a
                // miss (judged by time) does not flash.
                if self.row_judged[r] >= self.judge.rows()[r].size && !j.is_miss() {
                    // With `judged_lane`, where the press was.
                    let lanes = match self.pressing {
                        Some(lane) => vec![lane],
                        None => self.row_notes[r].iter().map(|(lane, _)| *lane).collect(),
                    };
                    for lane in lanes {
                        let note = self.row_notes[r]
                            .iter()
                            .find(|(l, _)| *l == lane)
                            .map(|(_, i)| *i)
                            .or(ev.note_index);
                        let hit = note.and_then(|index| {
                            let n = self.judge.note(index)?;
                            let at = match n.state {
                                NoteState::Hit { delta, .. } => n.seconds + delta,
                                _ => ev.song_time,
                            };
                            self.skin_note(index, at)
                        });
                        self.skin_event(
                            lane,
                            SkinEventKind::Tap {
                                judgement: j,
                                bright,
                            },
                            ev.song_time,
                        );
                        if let Some(event) = self.skin_events.back_mut() {
                            event.note = hit;
                        }
                    }
                }
            }
            JudgeEventKind::Held => {
                if self.holds_shown {
                    self.hold_flash[usize::from(ev.lane)] = Some((true, ev.song_time));
                }
                self.skin_event(ev.lane, SkinEventKind::Held { bright }, ev.song_time)
            }
            JudgeEventKind::LetGo => {
                if self.holds_shown {
                    self.hold_flash[usize::from(ev.lane)] = Some((false, ev.song_time));
                }
            }
            // A shock row's outcome shows in each of its lanes.
            JudgeEventKind::AvoidShock => {
                for lane in self.shock_lanes(ev.tick) {
                    self.hold_flash[usize::from(lane)] = Some((true, ev.song_time));
                }
            }
            JudgeEventKind::HitMine => {
                self.skin_event(ev.lane, SkinEventKind::HitMine, ev.song_time)
            }
            // A shock goes off in every lane, and hides what comes next.
            JudgeEventKind::HitShock => {
                for lane in self.shock_lanes(ev.tick) {
                    self.hold_flash[usize::from(lane)] = Some((false, ev.song_time));
                    self.skin_event(lane, SkinEventKind::HitMine, ev.song_time);
                }
                self.shock_blind = Some((ev.song_time, ev.song_time + SHOCK_BLIND_SECONDS));
            }
            // `CheckpointsFlashOnHold` (pump): every hold of a row kept
            // flashes as a completed hold, by the combo after the row.
            JudgeEventKind::Checkpoint {
                missed: 0, lanes, ..
            } => {
                let bright = self.bright();
                for lane in (0..64u8).filter(|l| lanes & (1 << l) != 0) {
                    self.skin_event(lane, SkinEventKind::Held { bright }, ev.song_time);
                }
            }
            JudgeEventKind::Boo | JudgeEventKind::Checkpoint { .. } => {}
        }
    }

    /// The lanes of the shock row at `tick`.
    fn shock_lanes(&self, tick: ddi_chart::Tick) -> Vec<u8> {
        self.judge
            .notes()
            .iter()
            .filter(|n| n.note.kind == NoteKind::Shock && n.note.tick == tick)
            .map(|n| n.note.lane)
            .collect()
    }

    /// One scored event (a note, or a whole row when per-row).
    fn score_event(&mut self, ev: &JudgeEvent) {
        let combo = &self.ruleset.combo;
        // `#COMBOS` (StepMania 5.1's `ScoreKeeperNormal`): a row or
        // checkpoint row that continues the combo adds the segment's combo
        // to it and counts its judgement that many times, one that breaks
        // it counts as the segment's miss combo; dance points and the
        // holds', mines' and shocks' own outcomes are unchanged. A
        // StepMania rule, so not under Dancing☆Onigiri's.
        let (hit_times, miss_times) = if self.ruleset.id == "danoni" {
            (1, 1)
        } else {
            self.timing.combo_at(ev.tick)
        };
        let mut times = 1;
        // A hit inside the FA+ window: counted, and weighed apart by the
        // EX score.
        let w0 = match ev.kind {
            JudgeEventKind::Tap(j, delta) => self.ruleset.judge.is_w0(j, delta) == Some(true),
            _ => false,
        };
        match ev.kind {
            JudgeEventKind::Tap(j, _) => {
                if combo.neutral == Some(j) {
                    // Neither adds to nor breaks the combo.
                } else if j <= combo.continue_min {
                    // A jump adds one a note where the ruleset says so.
                    let notes = if combo.per_row && combo.jumps_add_each {
                        ev.note_index
                            .and_then(|i| self.judge.note(i)?.row)
                            .map_or(1, |r| self.judge.rows()[r].size)
                    } else {
                        1
                    };
                    self.combo += hit_times * notes;
                    times = hit_times;
                } else {
                    self.combo = 0;
                    times = miss_times;
                }
            }
            JudgeEventKind::Held => {
                if combo.held_increments {
                    self.combo += 1;
                }
            }
            JudgeEventKind::LetGo => {
                if combo.let_go_breaks {
                    self.combo = 0;
                }
            }
            JudgeEventKind::HitMine => {
                if combo.mine_breaks {
                    self.combo = 0;
                }
            }
            // DDR: "avoided shock arrows ... increment the combo by one";
            // stepping on one breaks it (X onwards).
            JudgeEventKind::AvoidShock => self.combo += 1,
            JudgeEventKind::HitShock => self.combo = 0,
            JudgeEventKind::Boo => {}
            // `HandleComboInternal`: the holds kept add to the combo (one
            // under per-row combo), a missed one breaks it.
            JudgeEventKind::Checkpoint { held, missed, .. } => {
                if missed > 0 {
                    self.combo = 0;
                    times = miss_times;
                } else if combo.per_row {
                    self.combo += hit_times;
                    times = hit_times;
                } else {
                    self.combo += u32::from(held) * hit_times;
                    times = hit_times;
                }
            }
        }
        self.max_combo = self.max_combo.max(self.combo);
        self.tally.record_times(&ev.kind, times);
        if w0 {
            self.tally.w0 += times;
        }
        self.ctx.combo = self.combo;
        self.ctx.w0 = w0;
        self.ruleset.score.on_event(ev, &self.ctx);
        // A player failed and playing on gets no life back (StepMania's
        // `LifeMeterBar::ChangeLife`: "no point in letting them fill up
        // the bar again").
        if !self.failed {
            self.ruleset.gauge.on_event(ev, &self.ctx);
        }
        let life = self.ruleset.gauge.life();
        if self.life.last().is_none_or(|l| l[1] != life) {
            // In time order: a press judged after a miss in the same
            // update may be stamped a little before it.
            let at = (ev.song_time as f32).max(self.life.last().map_or(f32::MIN, |l| l[0]));
            self.life.push([at, life]);
        }
        if let JudgeEventKind::Tap(j, delta) = ev.kind {
            let delta = if j.is_miss() { 0.0 } else { delta };
            self.last_flash = Some((j, delta, ev.song_time, self.combo));
        }
        if self.gauge_failed_at.is_none() && self.ruleset.gauge.failed() {
            self.gauge_failed_at = Some(ev.song_time);
        }
        if !self.failed && self.ruleset.gauge.failed() {
            match self.ruleset.fail {
                FailPolicy::Immediate => {
                    self.failed = true;
                    self.finished = true;
                    self.ruleset.score.finish();
                }
                FailPolicy::ImmediateContinue => self.failed = true,
                FailPolicy::EndOfSong { .. } | FailPolicy::Off => {}
            }
        }
    }

    /// Render snapshot for a frame presented at `predicted_present`.
    pub fn frame(&self, predicted_present: HostTime) -> Frame {
        let song_time = self.clock.render_time(predicted_present);
        let scroll = ScrollState::at(self.options.scroll, &self.timing, song_time);
        let beat = scroll.beat();
        let visible_range = self.options.visible_range;
        let age = |at: f64| (song_time - at).max(0.0) as f32;

        let mut notes = Vec::new();
        let mut holding = vec![None; usize::from(self.lanes)];
        // After an immediate fail nothing is judged any more, so draw nothing:
        // the field goes blank while the shell fades the music out.
        let field_cleared = self.failed && matches!(self.ruleset.fail, FailPolicy::Immediate);
        let blind = self
            .shock_blind
            .filter(|&(from, until)| song_time >= from && song_time < until);
        for (note_index, (n, &(head_beat, tail_beat))) in
            self.judge.notes().iter().zip(&self.displayed).enumerate()
        {
            if field_cleared {
                break;
            }
            // A shock stepped on: the notes still to come vanish a moment.
            if blind.is_some() && n.state == NoteState::Pending {
                continue;
            }
            let kind = match n.note.kind {
                NoteKind::Tap => match n.state {
                    NoteState::Hit { .. } => continue,
                    _ => SpriteKind::Tap,
                },
                NoteKind::Lift => match n.state {
                    NoteState::Hit { .. } => continue,
                    _ => SpriteKind::Lift,
                },
                NoteKind::HoldHead { .. } | NoteKind::RollHead { .. } => {
                    if n.state == NoteState::Held {
                        continue;
                    }
                    // A boosted hold moves as a whole, tail included
                    // (danoniplus scales the freeze bar too).
                    let tail_y = scroll.y_displayed(tail_beat, n.end_seconds)
                        * n.note.speed_mul.unwrap_or(1.0);
                    let active = matches!(n.state, NoteState::HoldActive { .. });
                    let dropped = matches!(n.state, NoteState::LetGo | NoteState::Missed);
                    if let NoteState::HoldActive { life } = n.state
                        && life > 0.0
                        && let Some(h) = holding.get_mut(usize::from(n.note.lane))
                    {
                        *h = Some(if matches!(n.note.kind, NoteKind::RollHead { .. }) {
                            HoldKind::Roll
                        } else {
                            HoldKind::Hold
                        });
                    }
                    if matches!(n.note.kind, NoteKind::HoldHead { .. }) {
                        SpriteKind::HoldHead {
                            tail_y,
                            active,
                            dropped,
                        }
                    } else {
                        SpriteKind::RollHead {
                            tail_y,
                            active,
                            dropped,
                        }
                    }
                }
                NoteKind::Mine => match n.state {
                    NoteState::MineHit => continue,
                    _ => SpriteKind::Mine,
                },
                NoteKind::Shock => match n.state {
                    NoteState::MineHit => continue,
                    _ => SpriteKind::Shock,
                },
                NoteKind::Fake => SpriteKind::Fake,
                NoteKind::Dummy => SpriteKind::Dummy,
                NoteKind::AutoKeysound => continue,
            };
            let mut y = scroll.y_displayed(head_beat, n.seconds);
            if let Some(m) = n.note.speed_mul {
                y *= m;
            }
            // Receptor snap: the frame closest to the note's arrival shows it
            // exactly on the receptor.
            if self.options.receptor_snap
                && n.state == NoteState::Pending
                && (n.seconds - song_time).abs() < self.frame_interval / 2.0
            {
                y = 0.0;
            }
            let (top, bottom) = match kind {
                SpriteKind::HoldHead { tail_y, active, .. }
                | SpriteKind::RollHead { tail_y, active, .. } => {
                    if active {
                        y = y.max(0.0);
                    }
                    (y.max(tail_y), y.min(tail_y))
                }
                _ => (y, y),
            };
            // Skip only when the whole span is off screen: a hold whose head is
            // visible must be drawn even while its tail is still above the top.
            if top < -2.0 || bottom > visible_range {
                continue;
            }
            let beat_pos = n.note.tick.beat();
            let (alpha, glow) = self.options.appearance.visibility(y);
            let hold_life = match n.state {
                NoteState::HoldActive { life } => life,
                NoteState::LetGo | NoteState::Missed => 0.0,
                _ => 1.0,
            };
            notes.push(NoteSprite {
                note_index,
                lane: n.note.lane,
                y,
                kind,
                quantization: n.note.tick.quantization(),
                beat_frac: (beat_pos - beat_pos.floor()) as f32,
                color: n.note.color,
                alpha,
                glow,
                beat: beat_pos as f32,
                hold_life,
            });
        }

        let receptors = (0..usize::from(self.lanes))
            .map(|lane| ReceptorState {
                pressed: self.held[lane],
                holding: holding[lane],
                flash: self.lane_flash[lane]
                    .map(|(j, at)| (j, age(at)))
                    .filter(|(_, a)| *a < FLASH_SECONDS),
            })
            .collect();

        let judgement = self
            .last_flash
            .map(|(judgement, delta_seconds, at, combo_at)| JudgementFlash {
                judgement,
                delta_seconds,
                age: age(at),
                combo_at,
                w0: self.ruleset.judge.is_w0(judgement, delta_seconds),
            })
            .filter(|f| f.age < FLASH_SECONDS);

        let mut flashes: Vec<HoldFlash> = match self.hold_place {
            HoldPlace::None => Vec::new(),
            _ => self
                .hold_flash
                .iter()
                .enumerate()
                .filter_map(|(lane, f)| {
                    let (held, at) = (*f)?;
                    Some(HoldFlash {
                        lane: lane as u8,
                        held,
                        age: age(at),
                    })
                })
                .filter(|f| f.age < self.hold_place.seconds(f.held))
                .collect(),
        };
        if self.hold_place == HoldPlace::Centre {
            // The latest only.
            flashes.sort_by(|a, b| a.age.total_cmp(&b.age));
            flashes.truncate(1);
        }

        Frame {
            song_time,
            beat,
            lanes: self.lanes,
            receptors,
            notes,
            judgement,
            holds: HoldJudgements {
                place: self.hold_place,
                flashes,
            },
            combo: self.combo,
            score: self.ruleset.score.view(),
            life: self.ruleset.gauge.life(),
            danger: self.ruleset.gauge.danger(),
            failed: self.failed && self.finished,
            finished: self.finished,
            visible_range,
            appearance: self.options.appearance,
            cover: CoverView {
                cover: self.cover,
                changed: self
                    .cover_changed
                    .map(age)
                    .filter(|&a| a < COVER_LABEL_SECONDS),
            },
        }
    }

    /// Summary of the play so far (final once `finished()`).
    pub fn results(&self) -> Results {
        let mut r = self.results_brief();
        r.timeline = self.timeline.clone();
        r.life = self.life.clone();
        r
    }

    /// [`Player::results`] without the timeline and the life (which it
    /// would copy): for what reads them every frame.
    pub fn results_brief(&self) -> Results {
        let score = self.ruleset.score.view();
        let n = f64::from(self.delta_count);
        let mean_delta = if n > 0.0 { self.delta_sum / n } else { 0.0 };
        let variance = if n > 0.0 {
            (self.delta_sq_sum / n - mean_delta * mean_delta).max(0.0)
        } else {
            0.0
        };
        Results {
            tally: self.tally,
            max_combo: self.max_combo,
            score,
            grade: self.ruleset.grades.grade(&score, &self.tally, self.failed),
            full_combo: FullCombo::evaluate(&self.tally, &self.ruleset.combo),
            failed: self.failed,
            fast: self.fast,
            slow: self.slow,
            hits: self.delta_count,
            late_edges: self.late_edges,
            mean_delta,
            stddev_delta: variance.sqrt(),
            transform: self.options.transform,
            lane_map: self.lane_map.clone(),
            seed: self.options.seed,
            appearance: self.options.appearance,
            cover: self.shown_cover,
            scroll_action: self.options.scroll.scroll_action,
            assist: self.chart_assist || self.options.transform.is_assist(),
            timeline: Vec::new(),
            life: Vec::new(),
            gauge_failed_at: self.gauge_failed_at.map(|t| t as f32),
            ddr: self.ddr_clear(),
        }
    }

    /// [`Results::ddr`]: under DDR A's rules with a bar, the rank kept by
    /// a play whose gauge never emptied, and its points unless assisted.
    fn ddr_clear(&self) -> Option<DdrClear> {
        let rules = &self.ruleset;
        if !rules.takes_ddr_gauge() || rules.stepmania.life_type != LifeType::Bar {
            return None;
        }
        let cleared = !self.failed && self.gauge_failed_at.is_none();
        let flare = cleared.then(|| rules.gauge.flare_rank().unwrap_or(0));
        let assist = self.chart_assist || self.options.transform.is_assist();
        Some(DdrClear {
            gauge: rules.stepmania.gauge,
            flare,
            points: flare
                .filter(|_| !assist)
                .and_then(|rank| flare_points(self.meter, rank)),
        })
    }
}

/// DDR's shock arrows as StepMania-format charts write them, a mine in
/// every lane of a row ("only full lines of four or eight Shock Arrows
/// appear", RemyWiki's DDR glossary): those rows become shocks.
fn shock_rows(notes: &mut [Note], lanes: usize) {
    if lanes == 0 {
        return;
    }
    let mut start = 0;
    while start < notes.len() {
        let tick = notes[start].tick;
        let end = start + notes[start..].iter().take_while(|n| n.tick == tick).count();
        let mut mined = vec![false; lanes];
        for n in &notes[start..end] {
            if n.kind == NoteKind::Mine
                && let Some(m) = mined.get_mut(usize::from(n.lane))
            {
                *m = true;
            }
        }
        if mined.iter().all(|&m| m) {
            for n in &mut notes[start..end] {
                if n.kind == NoteKind::Mine {
                    n.kind = NoteKind::Shock;
                }
            }
        }
        start = end;
    }
}

/// `seconds` to the nearest microsecond, as the input log keeps it
/// ([`crate::history::PlayLog::encode`]).
pub(crate) fn to_microsecond(seconds: f64) -> f64 {
    if seconds.is_finite() {
        (seconds * 1e6).round() / 1e6
    } else {
        seconds
    }
}
