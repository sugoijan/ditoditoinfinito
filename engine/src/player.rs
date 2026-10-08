//! The orchestrator: clock + judge + rules → frames and results.

use ddi_chart::{Layout, NoteKind, Song, TimingMap};
use ddi_platform::{ClockSample, HostTime};
use serde::{Deserialize, Serialize};

use crate::appearance::Appearance;
use crate::clock::{ClockOptions, SongClock};
use crate::frame::{Frame, JudgementFlash, NoteSprite, ReceptorState, SpriteKind};
use crate::input::InputEvent;
use crate::judge::{Judge, JudgeEvent, JudgeEventKind, NoteState};
use crate::rules::{FailPolicy, FullCombo, Judgement, Ruleset, ScoreCtx, ScoreView, Tally};
use crate::scroll::{ScrollAction, ScrollOptions, ScrollState};
use crate::transform::{self, TransformOptions};

/// Default [`Frame::visible_range`], in arrow heights.
pub const DEFAULT_VISIBLE_RANGE: f32 = 12.0;

/// Seconds a judgement or receptor flash is reported in frames.
const FLASH_SECONDS: f32 = 1.0;

/// Silence after the last note before the play finishes.
const END_MARGIN: f64 = 1.0;

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayOptions {
    pub scroll: ScrollOptions,
    /// Turn and Cut, applied to the chart before judging.
    pub transform: TransformOptions,
    /// Hidden / Sudden / Stealth; drawn only.
    pub appearance: Appearance,
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
    /// Hits with a negative delta (early).
    pub fast: u32,
    /// Hits with a positive delta (late).
    pub slow: u32,
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
    pub scroll_action: ScrollAction,
    /// Notes were removed or simplified ([`TransformOptions::is_assist`]).
    pub assist: bool,
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
    clock: SongClock,
    judge: Judge,
    lanes: u8,
    held: Vec<bool>,
    rows: Vec<RowAcc>,
    ctx: ScoreCtx,
    started: bool,
    song_offset: f64,
    last_judge_time: Option<f64>,
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
    /// Per lane `(judgement, song_time)`.
    lane_flash: Vec<Option<(Judgement, f64)>>,
    failed: bool,
    finished: bool,
    /// Display frame interval in seconds, for receptor snapping.
    frame_interval: f64,
    /// `take_from[new_lane] = old_lane` of the turn.
    lane_map: Vec<u8>,
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
        let transformed = transform::apply(&chart.notes, &layout, &options.transform, options.seed);
        let judge = Judge::new(&transformed.notes, &timing, ruleset.judge.clone());
        let lanes = layout
            .lane_count()
            .max(chart_lanes)
            .max(judge.lane_count())
            .min(usize::from(u8::MAX)) as u8;
        let ruleset = ruleset.fresh();
        let mut player = Player {
            timing,
            clock: SongClock::new(options.clock),
            options,
            lanes,
            held: vec![false; usize::from(lanes)],
            rows: Vec::new(),
            ctx: ScoreCtx::default(),
            started: false,
            song_offset: 0.0,
            last_judge_time: None,
            combo: 0,
            max_combo: 0,
            tally: Tally::default(),
            fast: 0,
            slow: 0,
            delta_sum: 0.0,
            delta_sq_sum: 0.0,
            delta_count: 0,
            last_flash: None,
            lane_flash: vec![None; usize::from(lanes)],
            failed: false,
            finished: false,
            frame_interval: 1.0 / 60.0,
            lane_map: transformed.take_from,
            judge,
            ruleset,
        };
        player.reset_counts();
        player
    }

    fn reset_counts(&mut self) {
        self.rows = vec![RowAcc::default(); self.judge.rows().len()];
        self.ctx = ScoreCtx {
            steps: if self.ruleset.combo.per_row {
                self.judge.row_count()
            } else {
                self.judge.judged_note_count()
            },
            holds: self.judge.hold_count(),
            mines: self.judge.mine_count(),
            combo: 0,
        };
        self.ruleset.score.begin(&self.ctx);
        self.ruleset.gauge.begin(&self.ctx);
    }

    /// The audio was scheduled from `song_offset` seconds into the song at
    /// `start_context_time` on the audio clock. Notes before the offset are
    /// skipped.
    pub fn start(&mut self, start_context_time: f64, song_offset: f64) {
        self.clock.start(start_context_time, song_offset);
        self.started = true;
        self.song_offset = song_offset;
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

    /// Song second after which the play finishes: last note end + 1 s.
    pub fn song_end_time(&self) -> f64 {
        self.judge.last_end_seconds().unwrap_or(self.song_offset) + END_MARGIN
    }

    /// Heard song time of a host timestamp, shifted by the judge offset: a
    /// positive offset means the notes are expected later, so the press
    /// counts as earlier (see [`crate::rules::JudgeTable::judge_offset`]).
    fn judge_time(&self, host: HostTime) -> f64 {
        self.clock.song_time_at_host(host) - self.ruleset.judge.judge_offset
    }

    /// Feeds a lane edge. Returns the per-note judge events it caused.
    pub fn input(&mut self, ev: InputEvent) -> Vec<JudgeEvent> {
        if !self.started || self.finished || usize::from(ev.lane) >= self.held.len() {
            return Vec::new();
        }
        let t = self.judge_time(ev.host_time);
        self.held[usize::from(ev.lane)] = ev.pressed;
        let events = if ev.pressed {
            self.judge.press(ev.lane, t)
        } else {
            self.judge.release(ev.lane, t)
        };
        self.apply(&events);
        events
    }

    /// Advances to `host_now`: misses, hold life, fail and finish checks.
    pub fn update(&mut self, host_now: HostTime) -> Vec<JudgeEvent> {
        if !self.started || self.finished {
            return Vec::new();
        }
        let t = self.judge_time(host_now);
        self.last_judge_time = Some(t);
        let events = self.judge.update(t, &self.held);
        self.apply(&events);
        if !self.finished && t >= self.song_end_time() && self.judge.all_resolved() {
            self.finish();
        }
        events
    }

    fn finish(&mut self) {
        self.finished = true;
        self.ruleset.score.finish();
        if let FailPolicy::EndOfSong { min_life } = self.ruleset.fail
            && self.ruleset.gauge.life() < min_life
        {
            self.failed = true;
        }
    }

    /// Routes raw judge events through row grouping into combo/score/gauge.
    fn apply(&mut self, events: &[JudgeEvent]) {
        for ev in events {
            if self.finished {
                break;
            }
            if let JudgeEventKind::Tap(j, delta) = ev.kind
                && j != Judgement::Miss
            {
                if delta < 0.0 {
                    self.fast += 1;
                } else if delta > 0.0 {
                    self.slow += 1;
                }
                self.delta_sum += delta;
                self.delta_sq_sum += delta * delta;
                self.delta_count += 1;
                self.lane_flash[usize::from(ev.lane)] = Some((j, ev.song_time));
            }
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
        }
    }

    /// One scored event (a note, or a whole row when per-row).
    fn score_event(&mut self, ev: &JudgeEvent) {
        let combo = &self.ruleset.combo;
        match ev.kind {
            JudgeEventKind::Tap(j, _) => {
                if j <= combo.continue_min {
                    self.combo += 1;
                } else {
                    self.combo = 0;
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
            JudgeEventKind::Boo => {}
        }
        self.max_combo = self.max_combo.max(self.combo);
        self.tally.record(&ev.kind);
        self.ctx.combo = self.combo;
        self.ruleset.score.on_event(ev, &self.ctx);
        self.ruleset.gauge.on_event(ev, &self.ctx);
        if let JudgeEventKind::Tap(j, delta) = ev.kind {
            let delta = if j.is_miss() { 0.0 } else { delta };
            self.last_flash = Some((j, delta, ev.song_time, self.combo));
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
        let beat = self.timing.beat_at(song_time);
        let scroll = ScrollState::at(self.options.scroll, &self.timing, song_time);
        let visible_range = self.options.visible_range;
        let age = |at: f64| (song_time - at).max(0.0) as f32;

        let mut notes = Vec::new();
        // After an immediate fail nothing is judged any more, so draw nothing:
        // the field goes blank while the shell fades the music out.
        let field_cleared = self.failed && matches!(self.ruleset.fail, FailPolicy::Immediate);
        for n in self.judge.notes() {
            if field_cleared {
                break;
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
                NoteKind::HoldHead { end } | NoteKind::RollHead { end } => {
                    if n.state == NoteState::Held {
                        continue;
                    }
                    let tail_y = scroll.y(&self.timing, end, n.end_seconds);
                    let active = matches!(n.state, NoteState::HoldActive { .. });
                    let dropped = matches!(n.state, NoteState::LetGo | NoteState::Missed);
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
            let mut y = scroll.y(&self.timing, n.note.tick, n.seconds);
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
            notes.push(NoteSprite {
                lane: n.note.lane,
                y,
                kind,
                quantization: n.note.tick.quantization(),
                beat_frac: (beat_pos - beat_pos.floor()) as f32,
                color: n.note.color,
                alpha,
                glow,
            });
        }

        let receptors = (0..usize::from(self.lanes))
            .map(|lane| ReceptorState {
                pressed: self.held[lane],
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
            })
            .filter(|f| f.age < FLASH_SECONDS);

        Frame {
            song_time,
            beat,
            lanes: self.lanes,
            receptors,
            notes,
            judgement,
            combo: self.combo,
            score: self.ruleset.score.view(),
            life: self.ruleset.gauge.life(),
            danger: self.ruleset.gauge.danger(),
            failed: self.failed,
            finished: self.finished,
            visible_range,
            appearance: self.options.appearance,
        }
    }

    /// Summary of the play so far (final once `finished()`).
    pub fn results(&self) -> Results {
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
            mean_delta,
            stddev_delta: variance.sqrt(),
            transform: self.options.transform,
            lane_map: self.lane_map.clone(),
            seed: self.options.seed,
            appearance: self.options.appearance,
            scroll_action: self.options.scroll.scroll_action,
            assist: self.options.transform.is_assist(),
        }
    }
}
