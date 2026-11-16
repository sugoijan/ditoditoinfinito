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
    ATTACKS, Attack, AttackEvent, AttackState, BlockReason, CheckpointRecord, LEAD_V0,
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
use super::attacks::{hazards, hazards_of, judged_taps, life_ppm, micros};

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
    pub name: Rc<str>,
    pub feed: feed::FieldFeed,
    pub through_us: Option<i64>,
    pub end: Option<StreamEnd>,
    /// Its score as its stream last told it (DDI's sum).
    pub score: i64,
    /// The W0s its stream told so far (its taps' `w0`): with the sum and
    /// the chart's most, what its percent is worked out from
    /// ([`ddi_of_sum`]).
    pub w0s: u32,
    /// The combo part its stream's combos added so far (each moment's
    /// combo, as the engine's `ddi::DdiScore` counts it, from the first
    /// moment this device read): taken off the sum for its percent.
    pub combo_part: i64,
    /// The combo [`RemoteSeat::combo_part`] has counted to.
    counted: u32,
    /// Its song clock against the room's (µs, room − its own: its
    /// device's correction, `ddi_room::live::slew`), as its taps say it
    /// (each tap's time on the room's clock less its offset less its
    /// note's time): the median of the last few. Its moments are put back
    /// on its own song clock with it, so that its hits meet its notes.
    pub shift_us: i64,
    shifts: VecDeque<i64>,
    /// The rung its stream last said it stepped to (a match's "Charts:
    /// Adaptive"; 0 the stage's chart).
    pub steps: u8,
    /// The rung it stepped from at its last step.
    steps_from: u8,
}

impl RemoteSeat {
    /// Its stream said it stepped to rung `rung` of the stage's ladder at
    /// song second `at` (on its own song clock), the rung's notes its own
    /// from the boundary at `tick`: its field draws the rung's chart from
    /// there (the stage's again for rung 0). `rungs`: each rung's chart
    /// and whether it is above the stage's.
    fn step(
        &mut self,
        track: &Rc<feed::TapTrack>,
        rungs: &[(Rc<feed::TapTrack>, bool)],
        rung: u8,
        at: f64,
        tick: Tick,
    ) {
        let r = match usize::from(rung).checked_sub(1) {
            None => Some((Rc::clone(track), false)),
            Some(i) => rungs.get(i).cloned(),
        };
        let Some((r, up)) = r else {
            return;
        };
        // The same step said again (a snapshot's, then its moment in the
        // frames after it, the seat's song clock shifted a little
        // between): taken once.
        if self.feed.steps().any(|(a, _, to)| to == rung && (a - at).abs() < 0.5) {
            return;
        }
        self.steps = rung;
        let boundary = track.seconds_at(tick);
        // Back to the stage's chart: up where it came from a rung below
        // it.
        let up = up
            || (rung == 0
                && usize::from(self.steps_from)
                    .checked_sub(1)
                    .and_then(|i| rungs.get(i))
                    .is_some_and(|f| !f.1));
        self.steps_from = rung;
        self.feed.step(r, (at, boundary, tick), rung, up);
    }

    /// A snapshot's word on the chart it is on (its last step,
    /// `ddi_room::live::StepStatus`; revision 11), for a field that
    /// starts following part-way: where the field knows no step of its
    /// own yet, the rung it stepped from as the chart it was on all along
    /// (nothing of the stage's chart is drawn where it left it before
    /// this device looked), then the step itself, as its moment would
    /// have said it. A field that followed the stream knows its steps
    /// and takes the same step again as nothing.
    fn step_status(
        &mut self,
        track: &Rc<feed::TapTrack>,
        rungs: &[(Rc<feed::TapTrack>, bool)],
        step: ddi_room::live::StepStatus,
    ) {
        if self.feed.steps().next().is_none()
            && step.from != 0
            && let Some((r, _)) = rungs.get(usize::from(step.from) - 1)
        {
            self.feed.step(
                Rc::clone(r),
                (f64::NEG_INFINITY, f64::NEG_INFINITY, Tick(i64::MIN)),
                step.from,
                false,
            );
            self.steps_from = step.from;
        }
        let at = (i64::from(step.at_ms) * 1_000 - self.shift_us) as f64 / 1e6;
        self.step(track, rungs, step.rung, at, Tick(step.tick));
    }

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
    /// The checkpoints whose records came: one past the last one's index
    /// (`CheckpointNews::k`), never a count of records, which falls behind
    /// for good after a snapshot skipped some (review finding session-2).
    pub checkpoints: usize,
}

impl RemoteLedger {
    /// The attacks `p` launched and those sent against it.
    pub fn launched(&self, p: ParticipantId) -> (u32, u32) {
        let sent = self.attacks.iter().filter(|a| a.sender == p).count();
        let got = self.attacks.iter().filter(|a| a.recipient == p).count();
        (sent as u32, got as u32)
    }

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

    /// Checkpoint `k`'s record.
    fn apply(&mut self, k: u32, record: &CheckpointRecord) {
        self.checkpoints = self.checkpoints.max(k as usize + 1);
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
            a.recipient == p && a.state == AttackState::Active && a.clear_us >= now_us
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
    /// Attacks committed against someone this play has no stream of (a
    /// CPU computed here, `crate::battle::cpus`): the recipient, its
    /// checkpoint, the attack and its shocks.
    pub others: Vec<(ParticipantId, usize, u32, Vec<Hazard>)>,
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
    /// The stage's ladder as the fields draw it (each rung's chart, and
    /// whether it is above the stage's), where the charts adapt, and the
    /// local play's steps streamed so far.
    rungs: Vec<(Rc<feed::TapTrack>, bool)>,
    steps_sent: usize,
    /// The checkpoints whose news came in a snapshot (it carries their
    /// commitments, not their records): those whose deadline had passed
    /// at its time.
    snapshot_known: usize,
    /// [`Live::cpu_lead_us`].
    cpu_lead_us: i64,
    /// The stage's people not in its lineup when this play started, who
    /// may still enter it late (round P1b, `Event::EnteredLate`): a field
    /// each once the room's lineup has them ([`Live::admit_late`]), drawn
    /// on `track` with `lock`.
    late: Vec<(ParticipantId, String)>,
    track: feed::TapTrack,
    lock: Option<f64>,
    /// The shell's wall clock (ms) when the room's news was last read
    /// (none: never yet, by a play that started part-way and has the
    /// stage's backlog to read first).
    read_at: Option<u64>,
    /// The news read this frame came since a reading at most
    /// [`READ_GAP_MS`] ago: read as it arrived, so how far a stream had
    /// fallen behind when it came measures the network. News that waited
    /// longer in the room's client (the backlog a play that starts
    /// part-way reads first, what piled up while the song loaded or a
    /// frame was held) waited for this device, not for the network: its
    /// lag is not sampled, or a late start would raise the views' delay
    /// to its ceiling for many seconds (round M).
    arriving: bool,
}

/// The longest since the last reading of the room's news for this
/// reading's frames to count as read as they arrived (ms): the quarter
/// second a view's delay takes a held frame for
/// (`ddi_room::live::ViewDelay::advance`).
pub const READ_GAP_MS: u64 = 250;

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
/// (`sum`: the proportional part, the absolute score and the combo part),
/// its W0s so far (`w0s`, counted from its taps), its combo part (`combo`,
/// counted from its moments' combos, [`RemoteSeat::combo_part`]) and the
/// chart's `most`: the points are the one count whose `floor(1,000,000 ×
/// points ÷ most) + 100 × points` is the sum less the W0s and the combo
/// part (strictly increasing in the points, so at most one), and the view
/// (the percent among it) follows from them exactly (`DdiScoreView::of`).
/// Where no count gives it (the W0s not all told, another copy of the
/// chart), the nearest below; a stream read from part-way has not told
/// the combo part of runs that ended before, which leaves the points a
/// little high (a few of a long chart's). None without a most. What a
/// remote person's percent is shown from: the stream carries the sum
/// alone, and nothing is added to it.
pub fn ddi_of_sum(
    sum: i64,
    w0s: u32,
    combo: i64,
    most: Option<i64>,
) -> Option<ddi_engine::rules::ddi::DdiScoreView> {
    let most = most.filter(|m| *m > 0)?;
    let target = i128::from(sum) - i128::from(w0s) - i128::from(combo);
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
    let view = ddi_engine::rules::ddi::DdiScoreView::of(points, most, w0s);
    // The sum as told: what the points leave is the combo part.
    Some(view.with_combo(sum - view.ratio - view.absolute))
}

/// A remote moment as its field takes it (`score`: its score's view,
/// [`seat_view`]).
fn feed_moment(m: &Moment, out: bool, score: ScoreView) -> feed::Moment {
    let kind = match m.kind {
        MomentKind::Tap { tier, .. } => {
            feed::MomentKind::Tap(Judgement::from_index(tier.into()).unwrap_or(Judgement::Miss))
        }
        MomentKind::Shock => feed::MomentKind::ShockHit,
        MomentKind::HoldEnd { ok } => feed::MomentKind::HoldEnd(ok),
        MomentKind::Mine => feed::MomentKind::MineHit,
        MomentKind::Boo => feed::MomentKind::Boo,
        MomentKind::Refused { .. } => feed::MomentKind::Mark,
        MomentKind::Step { rung } => feed::MomentKind::Step(rung),
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
        score,
    }
}

/// A remote competitor's score view from its stream's `sum`, its W0s,
/// its combo part and the chart's `most`: worked out from the sum where
/// it can be ([`ddi_of_sum`]), else the sum alone ([`sum_view`]).
fn seat_view(sum: i64, w0s: u32, combo: i64, most: Option<i64>) -> ScoreView {
    match ddi_of_sum(sum, w0s, combo, most) {
        Some(d) => ScoreView {
            ddi: Some(d),
            ..ScoreView::default()
        },
        None => sum_view(sum),
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

/// A field of `p` (named `name`) drawn on `track`, `delay` seconds behind
/// the song, its lanes locked `lock` seconds by a shock.
fn remote_seat(
    p: ParticipantId,
    name: &str,
    track: &feed::TapTrack,
    delay: f64,
    lock: Option<f64>,
) -> RemoteSeat {
    let mut feed = feed::FieldFeed::new(track, 1.0, delay);
    feed.set_lock(lock);
    RemoteSeat {
        participant: p,
        name: name.into(),
        feed,
        through_us: None,
        end: None,
        score: 0,
        w0s: 0,
        combo_part: 0,
        counted: 0,
        shift_us: 0,
        shifts: VecDeque::new(),
        steps: 0,
        steps_from: 0,
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
            let delay = room.view_delay_for(stage, &rules);
            (rules, delay)
        };
        let cpu_lead_us = handle.room.borrow().cpu_lead_ms(stage) as i64 * 1_000;
        // The lineup the room committed: the order (taken as the prompt
        // opened) named every candidate, some of whom sat it out.
        let lineup = handle.room.borrow().lineup(stage);
        let delay = view_delay.delay_ms() / 1_000.0;
        let (seated, late): (Vec<_>, Vec<_>) = people
            .iter()
            .filter(|(p, _)| *p != me)
            .cloned()
            .partition(|(p, _)| lineup.as_ref().is_none_or(|l| l.contains(p)));
        let seats = seated
            .iter()
            .map(|(p, name)| remote_seat(*p, name, track, delay, lock))
            .collect();
        let reporter = (competes && plan.is_some())
            .then(|| Reporter::new(me, plan.as_ref().map_or_else(Vec::new, |p| p.checkpoints_us.clone())));
        // A play that starts part-way (an audience's Watch, a reload, a
        // member handed a running stage) reads what the room's client
        // kept of the stage (its last snapshot and the frames since) with
        // the rest at its first `receive`, and where a play before this
        // one read part of it, asks for the stage again as it stands
        // (review finding session-1). One that starts with the song, as
        // ever, from now on: what came before is the countdown's. Either
        // way what waited in the client is no measure of the network's
        // delay ([`Live::read_at`]): a backlog's first reading samples
        // nothing.
        let read_at = {
            let mut room = handle.room.borrow_mut();
            let part_way = room
                .song_zero_in(stage)
                .is_some_and(|z| z * 1_000.0 < -(rules.stale_ms as f64));
            if !part_way {
                let _ = room.take_live_news();
                Some((handle.wall_ms)())
            } else {
                if !room.live_news_whole(stage) {
                    room.read_live_again(stage);
                }
                None
            }
        };
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
            snapshot_known: 0,
            cpu_lead_us,
            read_at,
            arriving: false,
            late,
            track: track.clone(),
            lock,
        }
    }

    /// Those of the stage's people the room's lineup has now and this play
    /// has no field of: they entered the stage late (round P1b), a field
    /// each from now, after the others (the seats' numbers kept).
    fn admit_late(&mut self) {
        if self.late.is_empty() {
            return;
        }
        let Some(lineup) = self.handle.room.borrow().lineup(self.stage) else {
            return;
        };
        let delay = self.delay.delay_ms() / 1_000.0;
        let (entered, still): (Vec<_>, Vec<_>) = std::mem::take(&mut self.late)
            .into_iter()
            .partition(|(p, _)| lineup.contains(p));
        self.late = still;
        for (p, name) in entered {
            self.seats
                .push(remote_seat(p, &name, &self.track, delay, self.lock));
        }
    }

    /// How far ahead of this play's song time the CPUs computed here are
    /// judged (µs): a member watching with the music delayed judges them
    /// on the room's song time (`LiveRoom::cpu_lead_ms`).
    pub(crate) fn cpu_lead_us(&self) -> i64 {
        self.cpu_lead_us
    }

    /// The room's client and the shell's clock.
    pub(crate) fn handle(&self) -> &LiveHandle {
        &self.handle
    }

    /// The stage's checkpoints whose news has come (their records, and
    /// with them every commitment they made): what a CPU computed here
    /// may be judged up to the shocks of (`crate::battle::cpus::Drive`).
    pub(crate) fn known_checkpoints(&self) -> usize {
        self.ledger.checkpoints.max(self.snapshot_known)
    }

    /// The stage's ladder (each rung's chart and whether it is above the
    /// stage's, numbered as `Ladder::rungs`), where the charts adapt: what
    /// a remote field draws once its stream says it stepped.
    pub(crate) fn set_rungs(&mut self, rungs: Vec<(Rc<feed::TapTrack>, bool)>) {
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
        self.admit_late();
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
    pub(crate) fn room_now_us(&self, t: f64) -> i64 {
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
        let life = life_ppm(human.ruleset().gauge.life());
        // Its sum as earned, whatever charts it stepped to (as the room
        // compares it under adaptive charts).
        let score = score_of(human);
        let combo = human.combo();
        let slew = &self.slew;
        // Its steps, each a moment: when, to which rung, from which
        // boundary (its tick), the life then.
        let steps: Vec<Moment> = human
            .steps()
            .iter()
            .skip(self.steps_sent)
            .map(|st| Moment {
                at_ms: (slew.to_room(micros(st.at)) as f64 / 1_000.0).round() as i32,
                tick: st.tick.0,
                lane: 0,
                kind: MomentKind::Step { rung: st.rung },
                life_ppm: life_ppm(human.life_at(st.at + 1e-6)),
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
            for j in judged_taps(judged) {
                r.judged(j);
            }
            for (at, targeting) in self.retargets.drain(..) {
                r.retarget(at, targeting);
            }
            if let Some(at) = human.gauge_failed_at() {
                r.out_at(micros(at));
            }
            let life_at = |us: i64| life_ppm(human.life_at(us as f64 / 1e6));
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
        track: &Rc<feed::TapTrack>,
        t: f64,
        mut human: Option<&mut Deliver<'_>>,
    ) -> Received {
        let mut got = Received::default();
        let now = (self.handle.wall_ms)();
        self.arriving = self
            .read_at
            .is_some_and(|last| now.saturating_sub(last) <= READ_GAP_MS);
        self.read_at = Some(now);
        let news = self.handle.room.borrow_mut().take_live_news();
        for n in news {
            match n {
                LiveNews::Frame(f) if f.stage == self.stage => {
                    self.frame(track, &f, t, human.as_deref_mut(), &mut got)
                }
                LiveNews::Snapshot(s) if s.stage == self.stage => {
                    let at_us = ddi_room::live::song_us_at(s.song_zero_at, s.at);
                    for c in &s.competitors {
                        if let Some(seat) = self.seats.iter_mut().find(|x| x.participant == c.participant) {
                            seat.through_us = Some(seat.through_us.map_or(c.through_us, |x| x.max(c.through_us)));
                            seat.score = c.score;
                            // Its field as the room had it then (life,
                            // combo, score), where no moment it streamed
                            // came after (session-3).
                            let at = (at_us.min(c.through_us) - seat.shift_us) as f64 / 1e6;
                            if seat.feed.moments().last().is_none_or(|m| m.at < at) {
                                seat.feed.push(
                                    track,
                                    feed::Moment {
                                        at,
                                        tick: Tick(0),
                                        lane: 0,
                                        kind: feed::MomentKind::Mark,
                                        life: c.life_ppm as f32 / 1e6,
                                        combo: c.combo,
                                        out: false,
                                        offset: 0.0,
                                        w0: None,
                                        score: seat_view(c.score, seat.w0s, seat.combo_part, self.most),
                                    },
                                );
                            }
                            // The chart of the ladder it is on, where
                            // this field has not followed its steps.
                            if let Some(step) = c.step {
                                seat.step_status(track, &self.rungs, step);
                            }
                            if let Some(end) = c.end {
                                Self::ended(seat, track, end, self.most, &mut got);
                            }
                        }
                    }
                    if !s.competitors.is_empty() {
                        self.ledger.power = s.competitors.iter().map(|c| (c.participant, c.power)).collect();
                    }
                    self.settling = s.settling;
                    for c in s.commitments.clone() {
                        self.commit(track, c, t, human.as_deref_mut(), &mut got);
                    }
                    // The checkpoints run by the snapshot's time: every
                    // one whose deadline (and a tick) had passed.
                    if let Some(plan) = &self.plan {
                        let ran = plan
                            .checkpoints_us
                            .iter()
                            .take_while(|cp| {
                                **cp + (self.rules.report_deadline_ms + self.rules.tick_ms) as i64 * 1_000
                                    < at_us
                            })
                            .count();
                        self.snapshot_known = self.snapshot_known.max(ran);
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
        track: &Rc<feed::TapTrack>,
        f: &Frame,
        t: f64,
        mut human: Option<&mut Deliver<'_>>,
        got: &mut Received,
    ) {
        let room_now = self.room_now_us(t);
        // A frame built long before it is read (what was kept of the
        // stage before this play began, or a resume's) says nothing of
        // the network's delay now: its lag is not sampled. Nor is that
        // of any frame that waited in the client ([`Live::arriving`]).
        let fresh = self.arriving
            && self
                .handle
                .room
                .borrow()
                .song_zero_at(self.stage)
                .is_none_or(|z| room_now - song_us_at(z, f.at) <= self.rules.stale_ms as i64 * 1_000);
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
                && fresh
            {
                self.delay.sample(room_now as f64 / 1_000.0, micros_s(room_now - before) * 1_000.0);
            }
            seat.through_us = Some(seat.through_us.map_or(e.through_us, |x| x.max(e.through_us)));
            for m in &e.moments {
                if let MomentKind::Refused { attack } = m.kind {
                    // The views take its shocks off that field.
                    if let Some(a) = self.ledger.attacks.iter().find(|a| a.id == attack) {
                        seat.feed.remove_hazards(&hazards(a));
                    }
                    continue;
                }
                seat.score = m.score;
                if let MomentKind::Tap { w0: true, .. } = m.kind {
                    seat.w0s += 1;
                }
                // Its combo part, as its play counts it
                // (`ddi::DdiScore`): each step the combo grew by.
                if m.combo > seat.counted {
                    seat.combo_part += (seat.counted + 1..=m.combo)
                        .map(ddi_engine::rules::ddi::combo_points)
                        .sum::<i64>();
                }
                seat.counted = m.combo;
                if let MomentKind::Tap { tier, offset_ms, .. } = m.kind
                    && !Judgement::from_index(tier.into()).is_none_or(Judgement::is_miss)
                {
                    let note_us = micros(track.seconds_at(Tick(m.tick)));
                    seat.shift_sample(
                        i64::from(m.at_ms) * 1_000 - i64::from(offset_ms) * 1_000 - note_us,
                    );
                }
                let mut fm = feed_moment(
                    m,
                    false,
                    seat_view(m.score, seat.w0s, seat.combo_part, self.most),
                );
                // Back on its own song clock, where its notes are.
                fm.at -= seat.shift_us as f64 / 1e6;
                // A step: its field draws the rung's chart from the
                // boundary (the stage's again for rung 0).
                if let MomentKind::Step { rung } = m.kind {
                    seat.step(track, &self.rungs, rung, fm.at, Tick(m.tick));
                }
                seat.feed.push(track, fm);
            }
            if let Some(end) = e.end {
                Self::ended(seat, track, end, self.most, got);
            }
        }
        for c in &f.commitments {
            self.commit(track, *c, t, human.as_deref_mut(), got);
        }
        for CheckpointNews { k, record } in &f.checkpoints {
            self.ledger.apply(*k, record);
        }
        if f.settling.is_some() {
            self.settling = f.settling;
        }
    }

    /// A remote seat's stream ended (`end`, as a frame or a snapshot said
    /// it; the first said is kept): out of the stage, its field marked out
    /// from then (its going out's moment, else as far as it was judged),
    /// and as having left where it did not go out by its life (a quit, a
    /// connection lost for good, never started: `StreamEnd::forfeit`), so
    /// that every device ranks it last, as the stage does.
    fn ended(
        seat: &mut RemoteSeat,
        track: &feed::TapTrack,
        end: StreamEnd,
        most: Option<i64>,
        got: &mut Received,
    ) {
        if seat.end.is_some() {
            return;
        }
        seat.end = Some(end);
        let at = match end {
            StreamEnd::Out { at_us } => Some(at_us),
            StreamEnd::Finished => None,
            _ => Some(seat.through_us.unwrap_or(0)),
        };
        if let Some(at_us) = at {
            seat.feed.push(
                track,
                feed::Moment {
                    at: (at_us - seat.shift_us) as f64 / 1e6,
                    tick: Tick(0),
                    lane: 0,
                    kind: if end.forfeit() {
                        feed::MomentKind::Left
                    } else {
                        feed::MomentKind::Mark
                    },
                    life: 0.0,
                    combo: 0,
                    out: true,
                    offset: 0.0,
                    w0: None,
                    score: seat_view(seat.score, seat.w0s, seat.combo_part, most),
                },
            );
            got.outs.push(seat.participant);
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
        let hazards = hazards_of(&section.cells);
        let first_us = section.cells.first().map_or(section.clear_us, |x| x.at_us);
        self.ledger.attacks.push(Attack {
            id: c.attack,
            sender: c.sender,
            recipient: c.recipient,
            checkpoint: c.checkpoint as usize,
            slot: c.slot as usize,
            cells: section.cells.clone(),
            clear_us: section.clear_us,
            state: AttackState::Active,
        });
        self.learned.push((c.attack, t.max(committed_us as f64 / 1e6)));
        let delivered;
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
                    "attack {}: taken, {} shocks ({:.3} s to spare, first shock {:.3} s)",
                    c.attack,
                    hazards.len(),
                    micros_s(due - micros(t)),
                    micros_s(first_us),
                ));
            }
        } else if let Some(seat) = self.seats.iter_mut().find(|s| s.participant == c.recipient) {
            seat.feed.add_hazards(track, &hazards, committed_us as f64 / 1e6);
            delivered = true;
        } else {
            // A CPU computed here: its play takes them at the song time
            // every device does (`crate::battle::cpus`).
            got.others
                .push((c.recipient, c.checkpoint as usize, c.attack, hazards.clone()));
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
    track: Rc<feed::TapTrack>,
    log: Vec<String>,
    /// The stage's CPUs, computed here as on every device
    /// (`crate::battle::cpus`): a bot reports for them as a person's
    /// device does.
    cpus: Vec<BotCpu>,
    /// The stage's ladder (its rungs' names, for a CPU's run's chart).
    rungs: Vec<crate::battle::Rung>,
}

/// A CPU of the stage as a bot's play computes it.
struct BotCpu {
    opponent: ddi_engine::cpu::Opponent,
    drive: crate::battle::cpus::Drive,
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
        let track = Rc::new(feed::TapTrack::full(song, chart)?);
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
        let mut log = vec![play.describe()];
        let setup = crate::battle::StageSetup::of(order);
        let ruleset = crate::battle::stage_ruleset(&order.rules);
        let ladder = crate::battle::stage_ladder(song, chart, &setup);
        let checkpoints = prep
            .plan
            .as_ref()
            .filter(|_| order.battle.attacks())
            .map(|p| p.checkpoints_us.clone());
        let mut home = None;
        let cpus = order
            .cpus
            .iter()
            .map(|seat| {
                let home = crate::battle::cpus::home(&mut home, song, chart)?;
                let mut opponent = crate::battle::cpus::opponent(home, &ruleset, seat, &ladder)?;
                let drive = crate::battle::cpus::Drive::new(
                    ddi_room::live::cpus::ONLINE_CPUS_V0,
                    seat.participant,
                    checkpoints.clone(),
                    &mut opponent,
                );
                Ok(BotCpu { opponent, drive })
            })
            .collect::<Result<Vec<_>, String>>()?;
        if !cpus.is_empty() {
            log.push(format!("live: {} CPUs computed here", cpus.len()));
        }
        Ok(Some(LivePlay {
            live: play,
            track,
            log,
            cpus,
            rungs: crate::battle::ladder_rungs(song, &ladder)?,
        }))
    }

    /// The stage's CPUs judged on to song second `t` (the room's song
    /// clock as this bot has it), `others` the shocks the room committed
    /// against them: their reports, ends and runs to the room.
    fn step_cpus(&mut self, t: f64, others: &[(ParticipantId, usize, u32, Vec<Hazard>)]) {
        if self.cpus.is_empty() {
            return;
        }
        for (cpu, k, attack, cells) in others {
            if let Some(c) = self.cpus.iter_mut().find(|c| c.drive.participant() == *cpu) {
                c.drive.commit(*k, *attack, cells.clone());
            }
        }
        let known = self.live.known_checkpoints();
        let limit = self.live.room_now_us(t);
        let stage = self.live.stage;
        let room = &self.live.handle.room;
        for c in &mut self.cpus {
            let (_, ended) = crate::battle::cpus::step(&mut c.drive, &mut c.opponent, room, stage, (known, limit));
            if ended {
                // As a person's device reports it: with the rung's chart
                // it finished on.
                let launched = self.live.ledger.launched(c.drive.participant());
                let detail = crate::battle::cpu_detail(&c.opponent, Some(launched), &self.rungs);
                crate::battle::cpus::report_run(room, stage, &c.drive, &c.opponent, detail);
            }
        }
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
        self.step_cpus(t, &got.others);
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
    /// ones too), W0s, combo part and chart size; nothing without a most.
    #[test]
    fn the_percent_is_worked_out_from_the_sum_exactly() {
        for most in [50, 350, 5_000, 50_000, 123_450] {
            for points in [-500, -1, 0, 1, 7, most / 3, most - 1, most] {
                for w0s in [0u32, 1, 99, 100, 101, 900] {
                    for combo in [0i64, 6, 200_672, 1_062_400] {
                        let own = DdiScoreView::of(points, most, w0s).with_combo(combo);
                        let seen = ddi_of_sum(own.score, w0s, combo, Some(most)).unwrap();
                        assert_eq!(seen, own, "{most} {points} {w0s} {combo}");
                    }
                }
            }
        }
        assert_eq!(ddi_of_sum(5_000, 0, 0, None), None);
        assert_eq!(ddi_of_sum(5_000, 0, 0, Some(0)), None);
        // W0s not all told: the nearest count below, never above, the
        // score as told.
        let own = DdiScoreView::of(400, 5_000, 30).with_combo(1_000);
        let seen = ddi_of_sum(own.score, 0, 1_000, Some(5_000)).unwrap();
        assert_eq!((seen.points, seen.score), (400, own.score));
    }
}
