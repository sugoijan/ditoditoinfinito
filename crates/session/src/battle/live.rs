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
//! Its numbers are `ddi_room::live::LIVE_V1`'s (PROVISIONAL): views a
//! second behind the moments they draw, stale past two seconds, a late
//! join of five seconds at most.

use std::collections::BTreeSet;
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
use ddi_room::live::{LIVE_V1, LiveNews, LiveRefusal, LiveRules, StreamEnd};
use ddi_room::{Finisher, ParticipantId, Phase, StageId};
use ddi_ui::state::room::SharedRoom;

use super::feed;
use super::attacks::micros;

/// What a play of a live stage needs of its shell: the room's client (its
/// socket carries the stream) and the shell's wall clock (ms since the
/// epoch: the clock the room client's estimate of the room's runs on).
#[derive(Clone)]
pub struct LiveHandle {
    pub room: SharedRoom,
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
}

impl RemoteSeat {
    /// Out of the stage (its gauge, or a forfeit).
    pub fn gone(&self) -> bool {
        self.end.is_some_and(|e| e != StreamEnd::Finished)
    }

    pub fn out(&self) -> bool {
        matches!(self.end, Some(StreamEnd::Out { .. }))
    }
}

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

/// DDI's score as shown (the sum) of a play now.
fn score_of(p: &Player) -> i64 {
    p.ruleset().score.view().ddi.map_or(0, |d| d.score)
}

/// A remote moment as its field takes it.
fn feed_moment(m: &Moment, out: bool) -> feed::Moment {
    let kind = match m.kind {
        MomentKind::Tap { tier, .. } => {
            feed::MomentKind::Tap(Judgement::from_index(tier.into()).unwrap_or(Judgement::Miss))
        }
        MomentKind::Shock => feed::MomentKind::ShockHit,
        _ => feed::MomentKind::Mark,
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
        score: sum_view(m.score),
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
        let rules = LIVE_V1;
        let delay = rules.presentation_delay_ms as f64 / 1_000.0;
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
        }
    }

    /// The room's live part of the stage was lost (a restart): said on the
    /// status line.
    pub(crate) fn lost(&self) -> bool {
        self.lost
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
        let (combo, score) = (human.combo(), score_of(human));
        let moments: Vec<Moment> = judged
            .iter()
            .filter_map(|e| {
                let (tick, lane, kind) = moment_of(e, judge)?;
                Some(Moment {
                    at_ms: (e.song_time * 1_000.0).round() as i32,
                    tick: tick.0,
                    lane,
                    kind,
                    life_ppm: life,
                    combo,
                    score,
                })
            })
            .collect();
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
        out.judged_to(t_us);
        for (k, rep) in reports {
            out.report(LiveReport::from_report(k, rep));
        }
        if let Some(at) = human.gauge_failed_at() {
            out.end(StreamEnd::Out { at_us: micros(at) });
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
        let mut room = self.handle.room.borrow_mut();
        if let Some(out) = room.live_stream(self.stage) {
            out.judged_to(micros(t));
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
        for e in &f.entries {
            if e.participant == self.me {
                continue;
            }
            let Some(seat) = self.seats.iter_mut().find(|s| s.participant == e.participant) else {
                continue;
            };
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
                seat.feed.push(track, feed_moment(m, false));
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
                        at: at_us as f64 / 1e6,
                        tick: Tick(0),
                        lane: 0,
                        kind: feed::MomentKind::Mark,
                        life: 0.0,
                        combo: 0,
                        out: true,
                        offset: 0.0,
                        w0: None,
                        score: sum_view(seat.score),
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
            if let Some(deliver) = human
                && fair
                && deliver(&hazards)
            {
                delivered = true;
            }
            if !delivered {
                self.refused += 1;
                if self.streams && !self.ended && !self.lost {
                    let mut room = self.handle.room.borrow_mut();
                    if let Some(out) = room.live_stream(self.stage) {
                        out.moment(Moment {
                            at_ms: (t * 1_000.0).round() as i32,
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
                    "attack {}: refused (it reached this play too late)",
                    c.attack
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
        let room = self.handle.room.borrow();
        let s = room.state();
        let play = s.stage(self.stage)?.play.as_ref()?;
        let result = play.result.as_ref()?;
        let early = matches!(s.phase, Phase::SongCompletion { stage } if stage == self.stage);
        let mut groups: Vec<Vec<Finisher>> = Vec::new();
        let mut last: Option<u16> = None;
        for p in &result.placements {
            let f = Finisher {
                participant: p.participant,
                finish: p.finish,
            };
            if last == Some(p.place) {
                if let Some(g) = groups.last_mut() {
                    g.push(f);
                }
            } else {
                groups.push(vec![f]);
            }
            last = Some(p.place);
        }
        Some((groups, early))
    }

    /// A remote seat's field is behind this play's view of it: its stream
    /// has not been judged to the view's time for longer than the rules'
    /// stale time.
    pub(crate) fn stale(&self, seat: &RemoteSeat, t: f64) -> bool {
        if seat.end.is_some() {
            return false;
        }
        let view_us = micros(t) - self.rules.presentation_delay_ms as i64 * 1_000;
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
        let Some(prep) = &live.prep else {
            return Ok(None);
        };
        let track = feed::TapTrack::full(song, chart)?;
        let plan = prep.plan.clone().filter(|_| order.battle.attacks());
        Ok(Some(LivePlay {
            live: Live::new(
                handle,
                order.stage,
                order.me,
                true,
                plan,
                prep.horizon_us,
                &live.people,
                &track,
                None,
            ),
            track,
        }))
    }

    /// The play judged to song second `t` (`judged`: its events since the
    /// last call) streamed.
    pub fn send(&mut self, t: f64, player: &Player, judged: &[JudgeEvent]) {
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
        self.live
            .receive(&self.track, t, Some(deliver))
            .committed
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
}
