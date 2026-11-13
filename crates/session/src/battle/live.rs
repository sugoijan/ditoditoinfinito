//! A stage an online room plays live, on this device
//! (`docs/plans/multiplayer-online-play.md`, "What the client front
//! builds"): the play started at the room's instant, its judged moments
//! and checkpoint reports streamed to the room, and the room's frames read
//! back: the other competitors' fields, the attacks the room's ledger
//! committed (into this play, onto their fields), its checkpoints' records
//! (the power bars, the waiting lines, who attacks whom) and its decision.
//!
//! Where practice decides on this device (`crate::battle::attacks`), a
//! live stage only shows what the room decided: one code path for the
//! views (`crate::battle::Battle`), the decisions from elsewhere. The room
//! client (`ddi_ui::state::room::RoomClient`) is ticked here each frame,
//! as the screens tick it outside a play: its socket carries the stream.
//!
//! Its numbers are the room's live rules' (`ddi_room::live::LIVE_V2` in a
//! room this build makes, `LIVE_V1` in one made before; PROVISIONAL): the
//! others' fields drawn as far behind the song as this device's view delay
//! says (`ddi_room::live::delay`, `VIEW_DELAY_V0`: what its frames
//! measure, a second at most), stale past two seconds, a late join of
//! five seconds at most.

use std::collections::{BTreeSet, VecDeque};
use std::rc::Rc;

use ddi_chart::Tick;
use ddi_engine::battle::{HAZARD_INDEX_BASE, Hazard};
use ddi_engine::judge::{JudgeEvent, JudgeEventKind};
use ddi_engine::player::Player;
use ddi_engine::rules::{Judgement, ScoreView};
use ddi_room::attacks::{
    ATTACKS, Attack, AttackEvent, AttackState, BlockReason, CheckpointRecord, Judged, LEAD_V0,
    Reporter, Settling, Targeting,
};
use ddi_room::live::plan::BattlePlan;
use ddi_room::live::stream::{CheckpointNews, Commitment, Frame, LiveReport, Moment, MomentKind};
use ddi_room::live::{
    LiveNews, LiveRefusal, LiveRules, SLEW_V0, SongSlew, StreamEnd, ViewDelay, song_us_at,
};
use ddi_room::{Finisher, ParticipantId, StageId};
use std::cell::RefCell;

use ddi_ui::state::room::LiveRoom;

use super::feed;
use super::attacks::micros;

/// What a play of a live stage needs of its shell: the room's client (its
/// socket carries the stream; a member's `SharedRoom`, or an audience
/// seat's client, `ddi_ui::state::audience`) and the shell's wall clock
/// (ms since the epoch: the clock the client's estimate of the room's
/// runs on).
#[derive(Clone)]
pub struct LiveHandle {
    pub room: Rc<RefCell<dyn LiveRoom>>,
    pub wall_ms: Rc<dyn Fn() -> u64>,
}

/// Where a live stage's play starts on this device
/// ([`live_start`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiveStart {
    /// Song time zero is scheduled this long from now on the audio clock
    /// (seconds; negative once it has passed): heard at the room's instant.
    pub start: f64,
    /// The song second the play starts from: 0 on time, later where it
    /// joins late (the notes before it judged as misses).
    pub from: f64,
    /// Later than the late join allows: it does not play, it forfeits and
    /// watches.
    pub late: bool,
}

/// How far ahead of now a late join's music is scheduled, seconds.
pub const LATE_AHEAD: f64 = 0.15;

/// A live stage's start: song time zero is heard `zero_in` seconds from
/// now (the room's instant on this device's clock,
/// `RoomClient::song_zero_in`) through an output `latency` seconds long;
/// a device later than [`LATE_AHEAD`] before it joins from where the
/// room's clock has the song, up to `rules`' late join.
pub fn live_start(zero_in: f64, latency: f64, rules: &LiveRules) -> LiveStart {
    let start = zero_in - latency.max(0.0);
    let from = (LATE_AHEAD - start).max(0.0);
    LiveStart {
        start,
        from,
        late: from * 1_000.0 > rules.late_join_ms as f64,
    }
}

/// What this device worked out for stage `stage`'s live play (its reading
/// horizon, its battle plan), as its room client has it now: read when the
/// play starts, never from the stage's order alone. The order is taken
/// when the stage's prompt opens, which for the picker is as soon as the
/// readiness window opens, before its own load has finished and it said
/// it plays live (and for anyone, before the room's plan was known): a
/// play started from that copy streamed nothing and took no attack, while
/// the room had it in its ledger (then a no-show, out from the start).
/// By the stage's start the device has reported Loaded, which in a live
/// room comes only after it said it plays live.
pub fn prep_now(
    handle: &LiveHandle,
    stage: StageId,
    order: &ddi_ui::state::room::LiveOrder,
) -> Option<ddi_ui::state::room::LivePrep> {
    handle
        .room
        .borrow()
        .live_prep(stage)
        .or_else(|| order.prep.clone())
}

/// A live play's own part, as the debug state shows it.
pub struct LiveState<'a> {
    /// Its play is streamed to the room (it competes and said it plays
    /// live).
    pub streams: bool,
    /// It has the stage's battle plan (attacks can go into it).
    pub plan: bool,
    pub horizon_us: i64,
    /// Every attack committed against it, as it came.
    pub incoming: &'a [Incoming],
    /// The others' fields' delay behind the song (ms), what it moves
    /// toward, its rules' id, and the stalls seen so far (a field whose
    /// stream was not judged to its view's time), with those stalled now.
    pub delay_ms: f64,
    pub delay_target_ms: f64,
    pub delay_rules: &'static str,
    pub stalls: u32,
    pub stalled_now: usize,
    /// The room's live rules' id (`live-v1`, `live-v2`).
    pub rules: &'static str,
    /// The stream's and the views' song clock against this play's own
    /// (`ddi_room::live::slew`): the correction (ms, room − own), how far
    /// the room's clock as estimated said it is off now (ms), whether
    /// that is past the estimate's uncertainty, and the rules' id.
    pub slew_ms: f64,
    pub slew_off_ms: f64,
    pub slew_disagrees: bool,
    pub slew_rules: &'static str,
}

/// Another competitor of the stage, as its stream draws it.
pub(crate) struct RemoteSeat {
    pub participant: ParticipantId,
    pub name: String,
    pub feed: feed::FieldFeed,
    pub through_us: Option<i64>,
    pub end: Option<StreamEnd>,
    pub life: f32,
    pub combo: u32,
    pub score: i64,
    /// The W0s its stream told so far (its taps' `w0`): with the sum and
    /// the chart's most, what its percent is worked out from
    /// ([`ddi_of_sum`]).
    pub w0s: u32,
    /// Its song clock against the room's (µs, room − its own: its
    /// device's correction, `ddi_room::live::slew`), as its taps say it
    /// (each tap's time on the room's clock less its offset less its
    /// note's time): the median of the last few. Its moments are put back
    /// on its own song clock with it, so that its hits meet its notes.
    pub shift_us: i64,
    shifts: VecDeque<i64>,
    /// The steps down its stream said (a match's "At zero life: Step
    /// down"): the rung it plays.
    pub steps: u8,
}

impl RemoteSeat {
    /// Out of the stage (its gauge, or a forfeit).
    pub fn gone(&self) -> bool {
        self.end.is_some_and(|e| e != StreamEnd::Finished)
    }

    pub fn out(&self) -> bool {
        matches!(self.end, Some(StreamEnd::Out { .. }))
    }

    /// A tap's moment's say on its song clock's shift: the median of the
    /// last [`SHIFT_SAMPLES`].
    fn shift_sample(&mut self, us: i64) {
        if self.shifts.len() == SHIFT_SAMPLES {
            self.shifts.pop_front();
        }
        self.shifts.push_back(us);
        let mut v: Vec<i64> = self.shifts.iter().copied().collect();
        v.sort_unstable();
        self.shift_us = v[v.len() / 2];
    }
}

/// The taps a remote competitor's clock's shift is the median of.
const SHIFT_SAMPLES: usize = 9;

/// What the room's ledger said, as the views read it: every competitor's
/// power after the last checkpoint run, the waiting bars, the attacks
/// committed (their shocks from the stage's plan) and where they are.
#[derive(Default)]
pub(crate) struct RemoteLedger {
    pub power: Vec<(ParticipantId, u32)>,
    pub blocked: Vec<(ParticipantId, BlockReason)>,
    pub targeting: Vec<(ParticipantId, Targeting)>,
    pub attacks: Vec<Attack>,
    pub checkpoints: usize,
}

impl RemoteLedger {
    pub fn power(&self, p: ParticipantId) -> u32 {
        self.power
            .iter()
            .find(|(q, _)| *q == p)
            .map_or(0, |(_, x)| *x)
    }

    /// Its bar's share and whether it is full.
    pub fn bar(&self, p: ParticipantId) -> (f32, bool) {
        let power = self.power(p);
        (
            (power as f32 / ATTACKS.cap.max(1) as f32).min(1.0),
            power >= ATTACKS.cost,
        )
    }

    pub fn waiting(&self, p: ParticipantId) -> Option<BlockReason> {
        (self.power(p) >= ATTACKS.cost)
            .then(|| self.blocked.iter().find(|(q, _)| *q == p).map(|(_, r)| *r))
            .flatten()
    }

    fn apply(&mut self, record: &CheckpointRecord) {
        self.checkpoints += 1;
        self.power.clone_from(&record.power);
        for e in &record.events {
            match *e {
                AttackEvent::Blocked { sender, reason } => {
                    self.blocked.retain(|(p, _)| *p != sender);
                    self.blocked.push((sender, reason));
                }
                AttackEvent::Committed { sender, .. } => {
                    self.blocked.retain(|(p, _)| *p != sender);
                }
                AttackEvent::Cleared { attack } => self.set(attack, AttackState::Cleared),
                AttackEvent::Recovered { attack } => self.set(attack, AttackState::Recovered),
                AttackEvent::Expired { attack } => self.set(attack, AttackState::Expired),
                AttackEvent::Retargeted {
                    participant,
                    targeting,
                } => {
                    self.targeting.retain(|(p, _)| *p != participant);
                    self.targeting.push((participant, targeting));
                }
                AttackEvent::Charged { .. } => {}
            }
        }
    }

    fn set(&mut self, attack: u32, state: AttackState) {
        if let Some(a) = self.attacks.iter_mut().find(|a| a.id == attack) {
            a.state = state;
        }
    }

    /// The attacks against `p` on their way or passing, from `now_us`.
    pub fn incoming(&self, p: ParticipantId, now_us: i64) -> impl Iterator<Item = &Attack> {
        self.attacks.iter().filter(move |a| {
            a.recipient == p
                && matches!(a.state, AttackState::Active | AttackState::Cleared)
                && a.clear_us >= now_us
        })
    }
}

/// Puts an attack's shocks into this device's play (`Player::add_hazards`,
/// a bot's CPU's `Opponent::receive_hazards`): whether they went in.
pub type Deliver<'a> = dyn FnMut(&[Hazard]) -> bool + 'a;

/// What became of an attack the room committed against this play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fate {
    /// Its shocks went into the play.
    Taken,
    /// It came after its first shock's warning was due: refused.
    Late,
    /// The play was out of the stage (or not playing) when it came.
    Out,
    /// The play would not take its shocks (the engine refused them).
    Rejected,
}

impl Fate {
    pub fn word(self) -> &'static str {
        match self {
            Fate::Taken => "taken",
            Fate::Late => "late",
            Fate::Out => "out",
            Fate::Rejected => "rejected",
        }
    }
}

/// An attack the room committed against this play, as it reached it: the
/// diagnostics of a live stage's attacks (`__DDI_DEBUG.live.me.incoming`,
/// desktop's log). Song µs throughout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Incoming {
    pub attack: u32,
    pub sender: ParticipantId,
    /// Its checkpoint's time.
    pub checkpoint_us: i64,
    /// Its first shock's time.
    pub first_us: i64,
    /// The latest it could come and still be shown fairly: its first
    /// shock a reading horizon and the warning away.
    pub due_us: i64,
    /// This play's judged time when it came.
    pub arrived_us: i64,
    pub fate: Fate,
}

impl Incoming {
    /// How much sooner than it had to it came (negative: how late).
    pub fn spare_us(&self) -> i64 {
        self.due_us - self.arrived_us
    }
}

/// What reading the room's news this frame brought, for the battle's
/// moments.
#[derive(Default)]
pub(crate) struct Received {
    /// Attacks committed: (attack, sender, recipient, first shock µs,
    /// delivered into this play).
    pub committed: Vec<(u32, ParticipantId, ParticipantId, i64, bool)>,
    /// Competitors whose stream ended out of the stage this frame.
    pub outs: Vec<ParticipantId>,
    pub log: Vec<String>,
}

/// One stage played live on this device.
pub(crate) struct Live {
    handle: LiveHandle,
    stage: StageId,
    me: ParticipantId,
    rules: LiveRules,
    plan: Option<BattlePlan>,
    horizon_us: i64,
    reporter: Option<Reporter>,
    /// Policy changes not given to the reporter yet.
    retargets: Vec<(i64, Targeting)>,
    /// This device streams its play (it competes, and said it plays
    /// live); its stream ended.
    streams: bool,
    ended: bool,
    /// The room no longer knows the stage live (it restarted): nothing
    /// more is streamed; the run is reported at the end as before.
    lost: bool,
    pub seats: Vec<RemoteSeat>,
    pub ledger: RemoteLedger,
    seen: BTreeSet<u32>,
    pub settling: Option<Settling>,
    /// When this play learned of each attack (its id, judged song
    /// seconds).
    pub learned: Vec<(u32, f64)>,
    pub refused: u32,
    /// Every attack committed against this play, as it came.
    pub incoming: Vec<Incoming>,
    /// How far behind the song the others' fields are drawn, as this
    /// device measures its frames (`ddi_room::live::delay`).
    delay: ViewDelay,
    /// The seats whose fields are stalled now (their stream not judged to
    /// the view's time).
    stalled: BTreeSet<ParticipantId>,
    /// The most the chart gives under the stage's rules (DDI): what a
    /// remote competitor's percent is worked out against.
    most: Option<i64>,
    /// This play's song clock kept in step with the room's for its stream
    /// and its views (never its judging): [`Live::sync`].
    slew: SongSlew,
    /// The room's song time now as this device streams and draws it (µs).
    room_us: i64,
    /// The disagreement with the room's clock was said in the log.
    said_off: bool,
    /// Why this device's link to the stage ended for good, once it did
    /// (an audience's seat: `LiveRoom::seat_end`): the view is stale.
    seat_end: Option<ddi_ui::state::room::SeatEnd>,
    /// The stage's ladder as the fields draw it (each rung's chart), where
    /// zero life steps down, and the local play's steps streamed so far.
    rungs: Vec<feed::TapTrack>,
    steps_sent: usize,
}

/// A judge event as a moment of the stream (none: nothing a field
/// shows).
fn moment_of(e: &JudgeEvent, judge: &ddi_engine::rules::JudgeTable) -> Option<(Tick, u8, MomentKind)> {
    let kind = match e.kind {
        JudgeEventKind::Tap(j, delta) => MomentKind::Tap {
            tier: j.index() as u8,
            offset_ms: if j.is_miss() {
                0
            } else {
                (delta * 1_000.0).round().clamp(-32_000.0, 32_000.0) as i16
            },
            w0: !j.is_miss() && judge.is_w0(j, delta) == Some(true),
        },
        JudgeEventKind::Held => MomentKind::HoldEnd { ok: true },
        JudgeEventKind::LetGo => MomentKind::HoldEnd { ok: false },
        JudgeEventKind::HitMine
            if e.note_index.is_some_and(|n| n >= HAZARD_INDEX_BASE) =>
        {
            MomentKind::Shock
        }
        JudgeEventKind::HitMine => MomentKind::Mine,
        JudgeEventKind::Boo => MomentKind::Boo,
        _ => return None,
    };
    Some((e.tick, e.lane, kind))
}

fn micros_s(us: i64) -> f64 {
    us as f64 / 1e6
}

/// DDI's score as shown (the sum) of a play now.
fn score_of(p: &Player) -> i64 {
    p.ruleset().score.view().ddi.map_or(0, |d| d.score)
}

/// DDI's score view of a competitor whose stream told its sum alone
/// (`sum`: the proportional part plus the absolute score), its W0s so far
/// (`w0s`, counted from its taps) and the chart's `most`: the points are
/// the one count whose `floor(1,000,000 × points ÷ most) + 100 × points`
/// is the sum less the W0s (strictly increasing in the points, so at most
/// one), and the view (the percent among it) follows from them exactly
/// (`DdiScoreView::of`). Where no count gives it (the W0s not all told,
/// another copy of the chart), the nearest below. None without a most.
/// What a remote person's percent is shown from: the stream carries the
/// sum alone, and nothing is added to it.
pub fn ddi_of_sum(sum: i64, w0s: u32, most: Option<i64>) -> Option<ddi_engine::rules::ddi::DdiScoreView> {
    let most = most.filter(|m| *m > 0)?;
    let target = i128::from(sum) - i128::from(w0s);
    let f = |p: i128| (p * 1_000_000).div_euclid(i128::from(most)) + 100 * p;
    // f grows by at least 100 a point: a guess, then a step or two.
    let slope = 100.0 + 1_000_000.0 / most as f64;
    let mut p = (target as f64 / slope).floor() as i128;
    while f(p) > target {
        p -= 1;
    }
    while f(p + 1) <= target {
        p += 1;
    }
    let points = i64::try_from(p).ok()?;
    Some(ddi_engine::rules::ddi::DdiScoreView::of(points, most, w0s))
}

/// A remote moment as its field takes it (`ddi`: its score's view as
/// worked out from the sum, where it can be).
fn feed_moment(
    m: &Moment,
    out: bool,
    ddi: Option<ddi_engine::rules::ddi::DdiScoreView>,
) -> feed::Moment {
    let kind = match m.kind {
        MomentKind::Tap { tier, .. } => {
            feed::MomentKind::Tap(Judgement::from_index(tier.into()).unwrap_or(Judgement::Miss))
        }
        MomentKind::Shock => feed::MomentKind::ShockHit,
        MomentKind::HoldEnd { ok } => feed::MomentKind::HoldEnd(ok),
        MomentKind::Mine => feed::MomentKind::MineHit,
        MomentKind::Boo => feed::MomentKind::Boo,
        MomentKind::Refused { .. } => feed::MomentKind::Mark,
        MomentKind::StepDown { rung } => feed::MomentKind::StepDown(rung),
    };
    let (offset, w0) = match m.kind {
        MomentKind::Tap { offset_ms, w0, .. } => (f32::from(offset_ms) / 1_000.0, Some(w0)),
        _ => (0.0, None),
    };
    feed::Moment {
        at: f64::from(m.at_ms) / 1_000.0,
        tick: Tick(m.tick),
        lane: m.lane,
        kind,
        life: m.life_ppm as f32 / 1e6,
        combo: m.combo,
        out,
        offset,
        w0,
        score: match ddi {
            Some(d) => ScoreView {
                ddi: Some(d),
                ..ScoreView::default()
            },
            None => sum_view(m.score),
        },
    }
}

/// A score the stream told (DDI's sum alone) as a view shows it.
pub(crate) fn sum_view(score: i64) -> ScoreView {
    ScoreView {
        ddi: Some(ddi_engine::rules::ddi::DdiScoreView {
            score,
            ..Default::default()
        }),
        ..ScoreView::default()
    }
}

impl Live {
    /// Stage `stage`'s live play for `me` (`competes`: they play it, not
    /// watch it), with `plan` and `horizon_us` as this device worked them
    /// out, the other competitors `people` drawn on `track` (their fields'
    /// lanes locked `lock` seconds by a shock where the rules lock them).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        handle: LiveHandle,
        stage: StageId,
        me: ParticipantId,
        competes: bool,
        plan: Option<BattlePlan>,
        horizon_us: i64,
        people: &[(ParticipantId, String)],
        track: &feed::TapTrack,
        lock: Option<f64>,
    ) -> Live {
        let (rules, view_delay) = {
            let room = handle.room.borrow();
            let rules = room.live_rules();
            let delay = room.view_delay(&rules);
            (rules, delay)
        };
        let delay = view_delay.delay_ms() / 1_000.0;
        let seats = people
            .iter()
            .filter(|(p, _)| *p != me)
            .map(|(p, name)| {
                let mut feed = feed::FieldFeed::new(track, 1.0, delay);
                feed.set_lock(lock);
                RemoteSeat {
                    participant: *p,
                    name: name.clone(),
                    feed,
                    through_us: None,
                    end: None,
                    life: 1.0,
                    combo: 0,
                    score: 0,
                    w0s: 0,
                    shift_us: 0,
                    shifts: VecDeque::new(),
                    steps: 0,
                }
            })
            .collect();
        let reporter = (competes && plan.is_some())
            .then(|| Reporter::new(me, plan.as_ref().map_or_else(Vec::new, |p| p.checkpoints_us.clone())));
        // Its frames are kept for this play from now on.
        let _ = handle.room.borrow_mut().take_live_news();
        Live {
            handle,
            stage,
            me,
            rules,
            plan,
            horizon_us,
            reporter,
            retargets: Vec::new(),
            streams: competes,
            ended: false,
            lost: false,
            seats,
            ledger: RemoteLedger::default(),
            seen: BTreeSet::new(),
            settling: None,
            learned: Vec::new(),
            refused: 0,
            incoming: Vec::new(),
            delay: view_delay,
            stalled: BTreeSet::new(),
            most: None,
            slew: SongSlew::new(SLEW_V0),
            room_us: i64::MIN,
            said_off: false,
            seat_end: None,
            rungs: Vec::new(),
            steps_sent: 0,
        }
    }

    /// The stage's ladder (each rung's chart, the next easier first), where
    /// zero life steps down: what a remote field draws once its stream
    /// says it stepped down.
    pub(crate) fn set_rungs(&mut self, rungs: Vec<feed::TapTrack>) {
        self.rungs = rungs;
    }

    /// This frame's song time `t` (this play's judged song second, on its
    /// audio clock) on the room's song clock, as the stream and the views
    /// take it: the play's own time plus a correction that follows the
    /// room's clock as its estimate is refreshed (`ddi_room::live::slew`:
    /// smoothed, at most 2 ms a second, never backwards). The play's
    /// judging stays on its own clock: its presses, its notes' times and
    /// its music are untouched. A disagreement past the estimate's
    /// uncertainty is said in the log (the room is told nothing more: the
    /// stream's times are its clock's already).
    pub(crate) fn sync(&mut self, t: f64) -> Vec<String> {
        let mut log = Vec::new();
        let (room_us, uncertainty) = {
            let room = self.handle.room.borrow();
            let zero = room.song_zero_at(self.stage);
            let now = room.room_ms_at((self.handle.wall_ms)());
            let unc = room.clock_uncertainty_us();
            (zero.zip(now).map(|(z, n)| song_us_at(z, n)), unc)
        };
        self.room_us = self.slew.step(micros(t), room_us, uncertainty);
        let off = self.slew.disagrees();
        if off != self.said_off {
            self.said_off = off;
            log.push(if off {
                format!(
                    "live: this device's song clock and the room's disagree by {:.1} ms (estimate within {:.1} ms); its stream and views follow the room's at {} µs a second, its judging stays on its own",
                    self.slew.off_us() as f64 / 1e3,
                    uncertainty as f64 / 1e3,
                    SLEW_V0.max_rate_ppm,
                )
            } else {
                format!(
                    "live: the song clock is in step with the room's again (correction {:.1} ms)",
                    self.slew.correction_us() as f64 / 1e3
                )
            });
        }
        log
    }

    /// The room's song time now as streamed and drawn (µs; this play's own
    /// before the first [`Live::sync`]).
    fn room_now_us(&self, t: f64) -> i64 {
        if self.room_us == i64::MIN {
            micros(t)
        } else {
            self.room_us
        }
    }

    /// How far behind this play's song time (seconds) a remote
    /// competitor's field is drawn: the view's delay, less this device's
    /// correction, plus that competitor's (its notes and moments are on
    /// its own song clock).
    pub(crate) fn field_delay_s(&self, p: ParticipantId) -> f64 {
        let shift = self
            .seats
            .iter()
            .find(|s| s.participant == p)
            .map_or(0, |s| s.shift_us);
        self.delay.delay_ms() / 1_000.0 - self.slew.correction_us() as f64 / 1e6
            + shift as f64 / 1e6
    }

    /// A remote seat's field has shown its stream to its end (`end_us`,
    /// the room's song clock) at this play's song second `t`.
    pub(crate) fn shown_to(&self, end_us: i64, t: f64) -> bool {
        self.room_now_us(t) - (self.delay.delay_ms() * 1_000.0).round() as i64 >= end_us
    }

    /// The most the chart gives under the stage's rules (DDI's): the
    /// others' percents are worked out from their sums against it.
    pub(crate) fn set_most(&mut self, most: Option<i64>) {
        self.most = most;
    }

    /// The view at song second `t` (after this frame's news): the delay
    /// follows what the frames measured, every field is drawn that far
    /// behind, and a field whose stream was not judged to the view's time
    /// is a stall (counted, and the delay rises for it at once).
    pub(crate) fn view(&mut self, t: f64) {
        let room_us = self.room_now_us(t);
        let now_ms = room_us as f64 / 1_000.0;
        let d = self.delay.advance(now_ms);
        if room_us > 0 {
            self.handle.room.borrow_mut().note_view_delay(self.stage, d);
        }
        let view_us = room_us - (d * 1_000.0).round() as i64;
        let correction = self.slew.correction_us() as f64 / 1e6;
        for seat in &mut self.seats {
            seat.feed
                .set_delay(d / 1_000.0 - correction + seat.shift_us as f64 / 1e6);
            let behind = match (seat.end, seat.through_us) {
                (None, Some(through)) if view_us > 0 && through < view_us => {
                    Some(micros_s(room_us - through) * 1_000.0)
                }
                _ => None,
            };
            match behind {
                Some(lag_ms) => {
                    if self.stalled.insert(seat.participant) {
                        self.delay.stalled(now_ms, lag_ms);
                    }
                }
                None => {
                    self.stalled.remove(&seat.participant);
                }
            }
        }
    }

    /// What this play's live part is, for the debug state.
    pub(crate) fn state(&self) -> LiveState<'_> {
        LiveState {
            streams: self.streams,
            plan: self.plan.is_some(),
            horizon_us: self.horizon_us,
            incoming: &self.incoming,
            delay_ms: self.delay.delay_ms(),
            delay_target_ms: self.delay.target_ms(),
            delay_rules: self.delay.rules().id,
            stalls: self.delay.stalls(),
            stalled_now: self.stalled.len(),
            rules: self.rules.id,
            slew_ms: self.slew.correction_us() as f64 / 1e3,
            slew_off_ms: self.slew.off_us() as f64 / 1e3,
            slew_disagrees: self.slew.disagrees(),
            slew_rules: self.slew.rules().id,
        }
    }

    /// A log's line for this play's live part as it starts (English).
    pub(crate) fn describe(&self) -> String {
        format!(
            "live: stage {}: {}, {}, reading {:.2} s ahead",
            self.stage.0,
            if self.streams {
                "streamed"
            } else {
                "not streamed (watching, or this device never said it plays live)"
            },
            if self.plan.is_some() {
                "with the battle plan"
            } else {
                "no battle plan (no attacks into this play)"
            },
            self.horizon_us as f64 / 1e6,
        )
    }

    /// The room's live part of the stage was lost (a restart): said on the
    /// status line.
    pub(crate) fn lost(&self) -> bool {
        self.lost
    }

    /// Why this device's link to the stage ended, once it did.
    pub(crate) fn seat_end(&self) -> Option<&ddi_ui::state::room::SeatEnd> {
        self.seat_end.as_ref()
    }

    /// Time passes for the room's client (its socket, its clock, what is
    /// due to send).
    pub(crate) fn tick_room(&self) {
        let now = (self.handle.wall_ms)();
        self.handle.room.borrow_mut().tick(now);
    }

    /// This play's policy changed at judged song second `at`.
    pub(crate) fn retarget(&mut self, at: f64, targeting: Targeting) {
        self.retargets.push((micros(at), targeting));
    }

    /// The local play judged to song second `t`, `judged` its events since
    /// the last frame: its moments and the reports of the checkpoints it
    /// was judged past, into the stream (practice's counts: the chart's
    /// taps, its going out, its policy changes).
    pub(crate) fn send(&mut self, t: f64, human: &Player, judged: &[JudgeEvent]) {
        if !self.streams || self.ended || self.lost {
            return;
        }
        let judge = &human.ruleset().judge;
        let life = (f64::from(human.ruleset().gauge.life().clamp(0.0, 1.0)) * 1e6).round() as u32;
        // A play that stepped down: its sum on the stage chart's scale, as
        // the room compares it (`CPU_SCORE_V0`'s way).
        let score = if human.steps().is_empty() {
            score_of(human)
        } else {
            crate::battle::cpu_score(&human.ruleset().score.view(), self.most)
                .ddi
                .map_or(0, |d| d.score)
        };
        let combo = human.combo();
        let slew = &self.slew;
        // Its steps down, each a moment: when, to which rung, from which
        // boundary (its tick), the life given back.
        let steps: Vec<Moment> = human
            .steps()
            .iter()
            .skip(self.steps_sent)
            .map(|st| Moment {
                at_ms: (slew.to_room(micros(st.at)) as f64 / 1_000.0).round() as i32,
                tick: st.tick.0,
                lane: 0,
                kind: MomentKind::StepDown { rung: st.rung },
                life_ppm: (f64::from(human.life_at(st.at + 1e-6).clamp(0.0, 1.0)) * 1e6).round()
                    as u32,
                combo,
                score,
            })
            .collect();
        self.steps_sent = human.steps().len();
        let slew = &self.slew;
        let mut moments: Vec<Moment> = judged
            .iter()
            .filter_map(|e| {
                let (tick, lane, kind) = moment_of(e, judge)?;
                Some(Moment {
                    // On the room's song clock (its judging on its own).
                    at_ms: (slew.to_room(micros(e.song_time)) as f64 / 1_000.0).round() as i32,
                    tick: tick.0,
                    lane,
                    kind,
                    life_ppm: life,
                    combo,
                    score,
                })
            })
            .collect();
        moments.extend(steps);
        moments.sort_by_key(|m| m.at_ms);
        let t_us = micros(t);
        let mut reports = Vec::new();
        if let Some(r) = self.reporter.as_mut() {
            for e in judged {
                if let JudgeEventKind::Tap(j, _) = e.kind {
                    r.judged(Judged {
                        at_us: micros(e.song_time),
                        tier: j.index() as u8,
                    });
                }
            }
            for (at, targeting) in self.retargets.drain(..) {
                r.retarget(at, targeting);
            }
            if let Some(at) = human.gauge_failed_at() {
                r.out_at(micros(at));
            }
            let life_at = |us: i64| {
                (f64::from(human.life_at(us as f64 / 1e6).clamp(0.0, 1.0)) * 1e6).round() as u32
            };
            reports = r.due(t_us, &life_at);
        }
        let mut room = self.handle.room.borrow_mut();
        let Some(out) = room.live_stream(self.stage) else {
            return;
        };
        for m in moments {
            out.moment(m);
        }
        let room_us = self.room_us.max(self.slew.to_room(t_us));
        out.judged_to(room_us);
        for (k, rep) in reports {
            out.report(LiveReport::from_report(k, rep));
        }
        if let Some(at) = human.gauge_failed_at() {
            out.end(StreamEnd::Out {
                at_us: self.slew.to_room(micros(at)),
            });
            drop(room);
            self.ended = true;
        }
    }

    /// The local play's stream ends (its song's end, a forfeit), sent at
    /// once, before the run it is reported with.
    pub(crate) fn end(&mut self, end: StreamEnd, t: f64) {
        if !self.streams || self.ended || self.lost {
            self.ended = true;
            return;
        }
        self.ended = true;
        let room_us = self.room_us.max(self.slew.to_room(micros(t)));
        let mut room = self.handle.room.borrow_mut();
        if let Some(out) = room.live_stream(self.stage) {
            out.judged_to(room_us);
            out.end(end);
        }
        drop(room);
        self.tick_room();
    }

    /// The room's news this frame read: the other competitors' fields fed,
    /// the attacks committed put into this play (`human`, while it is in:
    /// refused where too late to be shown fairly) and onto their fields,
    /// the ledger's records kept. `t`: this play's judged song second.
    pub(crate) fn receive(
        &mut self,
        track: &feed::TapTrack,
        t: f64,
        mut human: Option<&mut Deliver<'_>>,
    ) -> Received {
        let mut got = Received::default();
        let news = self.handle.room.borrow_mut().take_live_news();
        for n in news {
            match n {
                LiveNews::Frame(f) if f.stage == self.stage => {
                    self.frame(track, &f, t, human.as_deref_mut(), &mut got)
                }
                LiveNews::Snapshot(s) if s.stage == self.stage => {
                    for c in &s.competitors {
                        if let Some(seat) = self.seats.iter_mut().find(|x| x.participant == c.participant) {
                            seat.through_us = Some(c.through_us);
                            seat.life = c.life_ppm as f32 / 1e6;
                            seat.combo = c.combo;
                            seat.score = c.score;
                            if seat.end.is_none() && c.end.is_some() {
                                seat.end = c.end;
                                if seat.gone() {
                                    got.outs.push(c.participant);
                                }
                            }
                        }
                    }
                    self.settling = s.settling;
                    for c in s.commitments.clone() {
                        self.commit(track, c, t, human.as_deref_mut(), &mut got);
                    }
                }
                LiveNews::Refused(LiveRefusal::UnknownStage { stage }) if stage == self.stage => {
                    // The room's live part is gone (it restarted): keep
                    // playing, stop streaming, report at the end.
                    if !self.lost {
                        self.lost = true;
                        self.handle.room.borrow_mut().forget_live(self.stage);
                        got.log.push("live: the room lost the stage's live part; streaming stops".into());
                    }
                }
                _ => {}
            }
        }
        if self.seat_end.is_none() {
            self.seat_end = self.handle.room.borrow().seat_end();
            if let Some(e) = &self.seat_end {
                got.log.push(format!("live: the seat watching the stage ended ({e:?}); its view stops"));
            }
        }
        self.view(t);
        got
    }

    fn frame(
        &mut self,
        track: &feed::TapTrack,
        f: &Frame,
        t: f64,
        mut human: Option<&mut Deliver<'_>>,
        got: &mut Received,
    ) {
        let room_now = self.room_now_us(t);
        for e in &f.entries {
            if e.participant == self.me {
                continue;
            }
            let Some(seat) = self.seats.iter_mut().find(|s| s.participant == e.participant) else {
                continue;
            };
            // How far its stream had fallen behind this device's song
            // time just before its news came: what the view must cover.
            if let Some(before) = seat.through_us
                && seat.end.is_none()
            {
                self.delay.sample(room_now as f64 / 1_000.0, micros_s(room_now - before) * 1_000.0);
            }
            seat.through_us = Some(seat.through_us.map_or(e.through_us, |x| x.max(e.through_us)));
            for m in &e.moments {
                if let MomentKind::Refused { attack } = m.kind {
                    // The views take its shocks off that field.
                    if let Some(a) = self.ledger.attacks.iter().find(|a| a.id == attack) {
                        let cells: Vec<Hazard> = a
                            .cells
                            .iter()
                            .map(|c| Hazard {
                                tick: Tick(c.tick),
                                lane: c.lane,
                            })
                            .collect();
                        seat.feed.remove_hazards(&cells);
                    }
                    continue;
                }
                seat.life = m.life_ppm as f32 / 1e6;
                seat.combo = m.combo;
                seat.score = m.score;
                if let MomentKind::Tap { w0: true, .. } = m.kind {
                    seat.w0s += 1;
                }
                if let MomentKind::Tap { tier, offset_ms, .. } = m.kind
                    && !Judgement::from_index(tier.into()).is_none_or(Judgement::is_miss)
                {
                    let note_us = micros(track.seconds_at(Tick(m.tick)));
                    seat.shift_sample(
                        i64::from(m.at_ms) * 1_000 - i64::from(offset_ms) * 1_000 - note_us,
                    );
                }
                let ddi = ddi_of_sum(m.score, seat.w0s, self.most);
                let mut fm = feed_moment(m, false, ddi);
                // Back on its own song clock, where its notes are.
                fm.at -= seat.shift_us as f64 / 1e6;
                // A step down: its field draws the rung's chart from the
                // boundary.
                if let MomentKind::StepDown { rung } = m.kind
                    && let Some(r) = self.rungs.get(usize::from(rung).wrapping_sub(1))
                {
                    seat.steps = seat.steps.max(rung);
                    seat.feed.step_down(
                        r.clone(),
                        fm.at,
                        track.seconds_at(Tick(m.tick)),
                        rung,
                    );
                }
                seat.feed.push(track, fm);
            }
            if let Some(end) = e.end
                && seat.end.is_none()
            {
                seat.end = Some(end);
                let at = match end {
                    StreamEnd::Out { at_us } => Some(at_us),
                    StreamEnd::Finished => None,
                    _ => Some(seat.through_us.unwrap_or(0)),
                };
                if let Some(at_us) = at {
                    seat.life = 0.0;
                    seat.feed.push(
                        track,
                        feed::Moment {
                        at: (at_us - seat.shift_us) as f64 / 1e6,
                        tick: Tick(0),
                        lane: 0,
                        kind: feed::MomentKind::Mark,
                        life: 0.0,
                        combo: 0,
                        out: true,
                        offset: 0.0,
                        w0: None,
                        score: match ddi_of_sum(seat.score, seat.w0s, self.most) {
                            Some(d) => ScoreView {
                                ddi: Some(d),
                                ..ScoreView::default()
                            },
                            None => sum_view(seat.score),
                        },
                        },
                    );
                    got.outs.push(seat.participant);
                }
            }
        }
        for c in &f.commitments {
            self.commit(track, *c, t, human.as_deref_mut(), got);
        }
        for CheckpointNews { record, .. } in &f.checkpoints {
            self.ledger.apply(record);
        }
        if f.settling.is_some() {
            self.settling = f.settling;
        }
    }

    /// An attack the room committed: its shocks (the plan's slot) into
    /// this play where it is the recipient and they can still be shown
    /// fairly (else refused, said in the stream), onto its recipient's
    /// field otherwise.
    fn commit(
        &mut self,
        track: &feed::TapTrack,
        c: Commitment,
        t: f64,
        human: Option<&mut Deliver<'_>>,
        got: &mut Received,
    ) {
        if !self.seen.insert(c.attack) {
            return;
        }
        let Some(plan) = &self.plan else {
            return;
        };
        let Some(section) = plan
            .schedule
            .slots
            .get(c.slot as usize)
            .and_then(|s| s.section.clone())
        else {
            return;
        };
        let committed_us = plan
            .checkpoints_us
            .get(c.checkpoint as usize)
            .copied()
            .unwrap_or(0);
        let hazards: Vec<Hazard> = section
            .cells
            .iter()
            .map(|x| Hazard {
                tick: Tick(x.tick),
                lane: x.lane,
            })
            .collect();
        let first_us = section.cells.first().map_or(section.clear_us, |x| x.at_us);
        self.ledger.attacks.push(Attack {
            id: c.attack,
            sender: c.sender,
            recipient: c.recipient,
            checkpoint: c.checkpoint as usize,
            slot: c.slot as usize,
            cells: section.cells.clone(),
            clear_us: section.clear_us,
            recover_after_us: section.recover_after_us,
            state: AttackState::Active,
            after_clear: 0,
        });
        self.learned.push((c.attack, t.max(committed_us as f64 / 1e6)));
        let mut delivered = false;
        if c.recipient == self.me {
            // Shown fairly: its first shock still a reading horizon and
            // the warning away.
            let due = first_us - self.horizon_us - LEAD_V0.warning_us;
            let fair = micros(t) <= due;
            let fate = match human {
                None => Fate::Out,
                Some(_) if !fair => Fate::Late,
                Some(deliver) => {
                    if deliver(&hazards) {
                        Fate::Taken
                    } else {
                        Fate::Rejected
                    }
                }
            };
            delivered = fate == Fate::Taken;
            self.incoming.push(Incoming {
                attack: c.attack,
                sender: c.sender,
                checkpoint_us: committed_us,
                first_us,
                due_us: due,
                arrived_us: micros(t),
                fate,
            });
            if !delivered {
                self.refused += 1;
                if self.streams && !self.ended && !self.lost {
                    let mut room = self.handle.room.borrow_mut();
                    if let Some(out) = room.live_stream(self.stage) {
                        out.moment(Moment {
                            at_ms: (self.room_now_us(t) as f64 / 1_000.0).round() as i32,
                            tick: 0,
                            lane: 0,
                            kind: MomentKind::Refused { attack: c.attack },
                            life_ppm: 0,
                            combo: 0,
                            score: 0,
                        });
                    }
                }
                got.log.push(format!(
                    "attack {}: refused ({}: came at {:.3} s, due by {:.3} s, first shock {:.3} s)",
                    c.attack,
                    fate.word(),
                    micros_s(micros(t)),
                    micros_s(due),
                    micros_s(first_us),
                ));
            } else {
                got.log.push(format!(
                    "attack {}: taken ({:.3} s to spare, first shock {:.3} s)",
                    c.attack,
                    micros_s(due - micros(t)),
                    micros_s(first_us),
                ));
            }
        } else if let Some(seat) = self.seats.iter_mut().find(|s| s.participant == c.recipient) {
            seat.feed.add_hazards(track, &hazards, committed_us as f64 / 1e6);
            delivered = true;
        }
        got.committed
            .push((c.attack, c.sender, c.recipient, first_us, delivered));
    }

    /// The room's decision of the stage, once it is confirmed: its ranking
    /// (best first, groups tied), and whether it was decided before the
    /// song's end.
    pub(crate) fn decision(&self) -> Option<(Vec<Vec<Finisher>>, bool)> {
        self.handle.room.borrow().decision(self.stage)
    }

    /// A remote seat's field is behind this play's view of it: its stream
    /// has not been judged to the view's time for longer than the rules'
    /// stale time.
    pub(crate) fn stale(&self, seat: &RemoteSeat, t: f64) -> bool {
        if seat.end.is_some() {
            return false;
        }
        let view_us = self.room_now_us(t) - (self.delay.delay_ms() * 1_000.0).round() as i64;
        let behind = view_us - seat.through_us.unwrap_or(0);
        view_us > 0 && behind > self.rules.stale_ms as i64 * 1_000
    }

    /// Everyone still in by their streams (this play's own aside).
    pub(crate) fn others_in(&self) -> usize {
        self.seats.iter().filter(|s| !s.gone()).count()
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        if let Ok(mut room) = self.handle.room.try_borrow_mut() {
            room.stop_live_news();
        }
    }
}

/// A live stage played by something other than a [`crate::play::PlaySession`]
/// (DEVELOPMENT TOOL: `ddi --mp-bots`' bots, each a CPU's play on a real-time
/// song clock): the same stream and the same delivery of the room's attacks
/// as a person's play.
pub struct LivePlay {
    live: Live,
    track: feed::TapTrack,
    log: Vec<String>,
}

impl LivePlay {
    /// Stage `order` of chart `chart` of `song` played live through
    /// `handle` (none where the room does not play it live, or this device
    /// did not say it plays it live).
    pub fn new(
        handle: LiveHandle,
        order: &ddi_ui::state::room::StageOrder,
        song: &ddi_chart::Song,
        chart: usize,
    ) -> Result<Option<LivePlay>, String> {
        let Some(live) = &order.live else {
            return Ok(None);
        };
        let Some(prep) = prep_now(&handle, order.stage, live) else {
            return Ok(None);
        };
        let track = feed::TapTrack::full(song, chart)?;
        let plan = prep.plan.clone().filter(|_| order.battle.attacks());
        let play = Live::new(
            handle,
            order.stage,
            order.me,
            true,
            plan,
            prep.horizon_us,
            &live.people,
            &track,
            None,
        );
        let log = vec![play.describe()];
        Ok(Some(LivePlay {
            live: play,
            track,
            log,
        }))
    }

    /// The play judged to song second `t` (`judged`: its events since the
    /// last call) streamed.
    pub fn send(&mut self, t: f64, player: &Player, judged: &[JudgeEvent]) {
        let log = self.live.sync(t);
        self.log.extend(log);
        self.live.send(t, player, judged);
        self.live.tick_room();
    }

    /// The room's news read at song second `t`; `deliver` puts an attack
    /// committed against this play into it. The attacks committed: sender,
    /// recipient and whether it went into this play.
    pub fn receive(
        &mut self,
        t: f64,
        deliver: &mut Deliver<'_>,
    ) -> Vec<(ParticipantId, ParticipantId, bool)> {
        let got = self.live.receive(&self.track, t, Some(deliver));
        self.log.extend(got.log);
        got.committed
            .into_iter()
            .map(|(_, from, to, _, delivered)| (from, to, delivered))
            .collect()
    }

    /// The stream ends (sent at once, before the run reported with it).
    pub fn end(&mut self, end: StreamEnd, t: f64) {
        self.live.end(end, t);
    }

    /// The room decided the stage before the song's end.
    pub fn decided_early(&self) -> bool {
        self.live.decision().is_some_and(|(_, early)| early)
    }

    /// Attacks against this play it refused (too late).
    pub fn refused(&self) -> u32 {
        self.live.refused
    }

    /// Every attack committed against this play, as it came.
    pub fn incoming(&self) -> &[Incoming] {
        &self.live.incoming
    }

    /// What reading the room's news logged (the attacks against this
    /// play, taken or refused), since the last call.
    pub fn take_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_engine::rules::ddi::DdiScoreView;

    /// A remote person's percent from the stream's sum: exactly the
    /// view their own play shows, for every count of points (negative
    /// ones too), W0s and chart size; nothing without a most.
    #[test]
    fn the_percent_is_worked_out_from_the_sum_exactly() {
        for most in [50, 350, 5_000, 50_000, 123_450] {
            for points in [-500, -1, 0, 1, 7, most / 3, most - 1, most] {
                for w0s in [0u32, 1, 99, 100, 101, 900] {
                    let own = DdiScoreView::of(points, most, w0s);
                    let seen = ddi_of_sum(own.score, w0s, Some(most)).unwrap();
                    assert_eq!(seen, own, "{most} {points} {w0s}");
                }
            }
        }
        assert_eq!(ddi_of_sum(5_000, 0, None), None);
        assert_eq!(ddi_of_sum(5_000, 0, Some(0)), None);
        // W0s not all told: the nearest count below, never above.
        let own = DdiScoreView::of(400, 5_000, 30);
        let seen = ddi_of_sum(own.score, 0, Some(5_000)).unwrap();
        assert_eq!(seen.points, 400);
    }
}
