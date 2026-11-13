//! The play stream: each competitor's judged moments and checkpoint
//! reports, sent in numbered batches ([`Batch`], kept by the sender's
//! [`Outbox`] until the room acknowledges them), gathered by the room into
//! one combined [`Frame`] a tick for everyone who follows the stage, and
//! read back in order by each recipient ([`FrameReader`]).
//!
//! A moment is what a field needs to draw a competitor's play without
//! judging it again: when (song ms), which note (its tick and lane: the
//! chart's, or the battle shock's), what happened (a tap's tier with its
//! offset and W0, a hold's end, a mine or a shock stepped on, a counted
//! stray press, a refused attack) and the HUD's numbers after it (life,
//! combo, score). The room never reads a moment; it only orders and
//! relays them.

use crate::attacks::{AttackId, CheckpointRecord, Judged, Report, Settling, Targeting};
use crate::ids::{Millis, ParticipantId, StageId};
use crate::live::LiveRules;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

/// What a moment judged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "snake_case")]
pub enum MomentKind {
    /// A note of the chart and its judgment: its tier (the engine's
    /// `Judgement::index`, the best first, a miss last), the press's
    /// offset from the note (ms, negative early; 0 for a miss) and, under
    /// DDI, whether it was a W0 (judged inside W1, reported as it).
    Tap { tier: u8, offset_ms: i16, w0: bool },
    /// A hold or roll (its head's tick) let go too soon or held to its end.
    HoldEnd { ok: bool },
    /// A mine of the chart stepped on.
    Mine,
    /// A battle shock stepped on (its tick and lane). Where the stage's
    /// rules lock a lane a shock is stepped on in, the view locks it from
    /// this moment for the rules' time.
    Shock,
    /// A stray press that counts (DDI's Boos): its lane, no note.
    Boo,
    /// An attack's shocks the device refused to put into its play (they
    /// reached it too late to be shown fairly): the views take them off.
    Refused { attack: AttackId },
    /// Their life ran out and they stepped down to rung `rung` (from 1)
    /// of the stage's ladder (a match's "At zero life: Step down",
    /// `step-down-v0`): the moment's time is when, its tick the
    /// boundary's (from which the rung's notes are theirs), its life what
    /// was given back. ADDITIVE on the wire ([`crate::live::wire`]): a
    /// refusal's kind with its flag, so that a reader that predates it
    /// reads the refusal of an attack no stage has and does nothing.
    StepDown { rung: u8 },
}

/// One judged moment of a competitor's play.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Moment {
    /// When it was judged, song milliseconds.
    pub at_ms: i32,
    /// Which note (its row, the chart's ticks) and lane; 0 where there is
    /// no note (a Boo's tick, a refusal's).
    pub tick: i64,
    pub lane: u8,
    pub kind: MomentKind,
    /// Life after it (millionths), combo after it, the score after it as
    /// the room shows it (DDI's sum).
    pub life_ppm: u32,
    pub combo: u32,
    pub score: i64,
}

/// What a competitor reports at checkpoint `k` once its play has been
/// judged past it: `crate::attacks::Report` without the participant (the
/// room knows who sent it, and never takes it from the message).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveReport {
    pub k: u32,
    pub judged: Vec<Judged>,
    pub life_ppm: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out_at_us: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targeting: Option<Targeting>,
}

impl LiveReport {
    pub fn from_report(k: usize, r: Report) -> LiveReport {
        LiveReport {
            k: k as u32,
            judged: r.judged,
            life_ppm: r.life_ppm,
            out_at_us: r.out_at_us,
            targeting: r.targeting,
        }
    }

    pub fn into_report(self, participant: ParticipantId) -> Report {
        Report {
            participant,
            judged: self.judged,
            life_ppm: self.life_ppm,
            out_at_us: self.out_at_us,
            targeting: self.targeting,
        }
    }
}

/// How a competitor's stream ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "end", rename_all = "snake_case")]
pub enum StreamEnd {
    /// Played to the song's end (its run follows as `ReportOutcome`).
    Finished,
    /// Its gauge emptied at song µs `at_us`: out of the stage.
    Out { at_us: i64 },
    /// It left the stage while competing (a quit; a device that could not
    /// start within the late join).
    Forfeit,
    /// The room's: its connection was lost past the reconnect grace.
    Dropped,
    /// The room's: it was in the lineup and never streamed.
    NoShow,
}

impl StreamEnd {
    /// Whether it leaves the stage without a run of its own (a forfeit to
    /// the ranking).
    pub fn forfeit(self) -> bool {
        matches!(self, Self::Forfeit | Self::Dropped | Self::NoShow)
    }
}

/// One batch of a competitor's stream: its number (from 1, one more each
/// batch), the song time its play has been judged to (µs), what happened
/// since the last batch, and its end where it ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Batch {
    pub stage: StageId,
    pub seq: u32,
    pub through_us: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moments: Vec<Moment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reports: Vec<LiveReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<StreamEnd>,
}

/// The sender's side: what its play judged, batched every
/// [`LiveRules::batch_ms`] and kept until a frame acknowledges it (a frame
/// entry's `seq`), sent again after [`LiveRules::resend_after_ms`] or a
/// reconnect.
#[derive(Clone, Debug)]
pub struct Outbox {
    stage: StageId,
    rules: LiveRules,
    next_seq: u32,
    acked: u32,
    moments: Vec<Moment>,
    reports: Vec<LiveReport>,
    through_us: i64,
    sent_through_us: Option<i64>,
    end: Option<StreamEnd>,
    end_sent: bool,
    last_flush: Option<u64>,
    /// Sent and not acknowledged, with when each was last sent (ms).
    unacked: VecDeque<(Batch, u64)>,
    /// Resends since the room last acknowledged anything: each doubles the
    /// wait before the next (up to [`Outbox::MAX_BACKOFF`]), so that a
    /// room that has fallen behind is not sent its own backlog again and
    /// again (2026-11-13: under `live-v2`'s ten batches a second, resending
    /// every unacknowledged batch each second grew without bound).
    backoff: u32,
}

impl Outbox {
    pub fn new(stage: StageId, rules: LiveRules) -> Self {
        Self {
            stage,
            rules,
            next_seq: 1,
            acked: 0,
            moments: Vec::new(),
            reports: Vec::new(),
            through_us: i64::MIN,
            sent_through_us: None,
            end: None,
            end_sent: false,
            last_flush: None,
            unacked: VecDeque::new(),
            backoff: 0,
        }
    }

    /// The most a resend's wait doubles: 8 times `resend_after_ms`.
    pub const MAX_BACKOFF: u32 = 3;

    pub fn stage(&self) -> StageId {
        self.stage
    }

    /// The room's rules changed (a socket opened on its policy): the
    /// period batches are cut on from now.
    pub fn set_rules(&mut self, rules: LiveRules) {
        self.rules = rules;
    }

    /// A judged moment (in the order judged).
    pub fn moment(&mut self, m: Moment) {
        if self.end.is_none() {
            self.moments.push(m);
        }
    }

    /// The report of a checkpoint the play was judged past.
    pub fn report(&mut self, r: LiveReport) {
        if self.end.is_none() {
            self.reports.push(r);
        }
    }

    /// The play has been judged to song µs `through_us`.
    pub fn judged_to(&mut self, through_us: i64) {
        self.through_us = self.through_us.max(through_us);
    }

    /// The stream ends (sent with the next batch, at once).
    pub fn end(&mut self, end: StreamEnd) {
        if self.end.is_none() {
            self.end = Some(end);
        }
    }

    pub fn ended(&self) -> bool {
        self.end.is_some()
    }

    /// Ended, sent and acknowledged: nothing more to do.
    pub fn done(&self) -> bool {
        self.end_sent && self.unacked.is_empty()
    }

    /// The highest batch the room acknowledged.
    pub fn acked(&self) -> u32 {
        self.acked
    }

    pub fn unacked(&self) -> usize {
        self.unacked.len()
    }

    /// The room has every batch up to `seq` (a frame said so).
    pub fn ack(&mut self, seq: u32) {
        if seq > self.acked {
            self.acked = seq;
            self.backoff = 0;
        }
        while self
            .unacked
            .front()
            .is_some_and(|(b, _)| b.seq <= self.acked)
        {
            self.unacked.pop_front();
        }
    }

    /// What to send now (`now` the device's clock, ms): a new batch when
    /// the batch interval has passed and there is news (or at once for the
    /// end), and the unacknowledged ones again once they are overdue.
    pub fn due(&mut self, now: u64) -> Vec<Batch> {
        let mut out = Vec::new();
        let wait = self.rules.resend_after_ms << self.backoff;
        let overdue = self.unacked.front().is_some_and(|(_, at)| now >= at + wait);
        if overdue {
            for (b, at) in &mut self.unacked {
                *at = now;
                out.push(b.clone());
            }
            self.backoff = (self.backoff + 1).min(Self::MAX_BACKOFF);
        }
        let interval = self
            .last_flush
            .is_none_or(|t| now >= t + self.rules.batch_ms);
        let news = !self.moments.is_empty()
            || !self.reports.is_empty()
            || self.sent_through_us.is_none_or(|t| self.through_us > t);
        let ending = self.end.is_some() && !self.end_sent;
        if (interval && news && self.through_us != i64::MIN) || ending {
            let b = self.cut();
            self.last_flush = Some(now);
            self.unacked.push_back((b.clone(), now));
            out.push(b);
        }
        out
    }

    /// Every batch not acknowledged, again (a new socket: its wait starts
    /// over).
    pub fn resend(&mut self, now: u64) -> Vec<Batch> {
        self.backoff = 0;
        self.unacked
            .iter_mut()
            .map(|(b, at)| {
                *at = now;
                b.clone()
            })
            .collect()
    }

    fn cut(&mut self) -> Batch {
        let seq = self.next_seq;
        self.next_seq += 1;
        let through = if self.through_us == i64::MIN {
            0
        } else {
            self.through_us
        };
        self.sent_through_us = Some(through);
        let end = if self.end_sent { None } else { self.end };
        if end.is_some() {
            self.end_sent = true;
        }
        Batch {
            stage: self.stage,
            seq,
            through_us: through,
            moments: std::mem::take(&mut self.moments),
            reports: std::mem::take(&mut self.reports),
            end,
        }
    }
}

/// An attack the room committed: everything a device needs to put the
/// same shocks into the same play. The cells are the stage's battle plan's
/// (`BattlePlan::schedule`, slot `slot`), which every device has computed
/// and checked by digest; the engine places them by tick and lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commitment {
    pub attack: AttackId,
    pub checkpoint: u32,
    pub slot: u32,
    pub sender: ParticipantId,
    pub recipient: ParticipantId,
}

/// One competitor's news in a frame: the last batch the room took from it
/// (its acknowledgement), the song time its play has been judged to, the
/// moments since the last frame and its end.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameEntry {
    pub participant: ParticipantId,
    pub seq: u32,
    pub through_us: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moments: Vec<Moment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<StreamEnd>,
}

/// A checkpoint the room ran: its number and the ledger's record of it
/// (everyone's power after it, what it changed).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointNews {
    pub k: u32,
    pub record: CheckpointRecord,
}

/// One combined frame of a stage, the same bytes for every recipient: its
/// number (from 1, one more each frame sent), the room's time when it was
/// built, each competitor with news, the attacks committed and the
/// checkpoints run since the frame before, and, while the stage waits on
/// its last attack, who it waits with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    pub stage: StageId,
    pub n: u32,
    pub at: Millis,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<FrameEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commitments: Vec<Commitment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checkpoints: Vec<CheckpointNews>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settling: Option<Settling>,
}

impl Frame {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
            && self.commitments.is_empty()
            && self.checkpoints.is_empty()
            && self.settling.is_none()
    }
}

/// What a recipient makes of the frames: taken in order once each, a gap
/// asking for the frames again ([`FrameReader::wants_resume`]).
#[derive(Clone, Debug, Default)]
pub struct FrameReader {
    stage: Option<StageId>,
    last: Option<u32>,
    /// A resume asked for (from frame `after`), when (ms).
    resuming: Option<u64>,
    want_resume: bool,
    /// Each competitor's moments as received, in order.
    moments: BTreeMap<ParticipantId, Vec<Moment>>,
    ends: BTreeMap<ParticipantId, StreamEnd>,
    through: BTreeMap<ParticipantId, i64>,
    commitments: Vec<Commitment>,
    checkpoints: Vec<CheckpointNews>,
}

impl FrameReader {
    /// How long a resume's answer may take before a later gap asks again.
    pub const RESUME_AGAIN_MS: u64 = 2_000;

    pub fn new(stage: StageId) -> Self {
        Self {
            stage: Some(stage),
            ..Self::default()
        }
    }

    pub fn stage(&self) -> Option<StageId> {
        self.stage
    }

    /// The last frame taken.
    pub fn last(&self) -> Option<u32> {
        self.last
    }

    /// Take `frame` (at the device's `now`, ms): `true` if it was the next
    /// one and was taken; an old one is passed over; a later one is a gap
    /// (asking for a resume, at most once per [`Self::RESUME_AGAIN_MS`]).
    pub fn take(&mut self, frame: &Frame, now: u64) -> bool {
        if self.stage != Some(frame.stage) {
            return false;
        }
        let next = self.last.map_or(1, |l| l + 1);
        if frame.n < next {
            return false;
        }
        if frame.n > next {
            if self
                .resuming
                .is_none_or(|at| now >= at + Self::RESUME_AGAIN_MS)
            {
                self.want_resume = true;
            }
            return false;
        }
        self.last = Some(frame.n);
        self.resuming = None;
        for e in &frame.entries {
            self.moments
                .entry(e.participant)
                .or_default()
                .extend(e.moments.iter().copied());
            self.through.insert(e.participant, e.through_us);
            if let Some(end) = e.end {
                self.ends.insert(e.participant, end);
            }
        }
        self.commitments.extend(frame.commitments.iter().copied());
        self.checkpoints.extend(frame.checkpoints.iter().cloned());
        true
    }

    /// A live snapshot came: the frames from `first` on follow it.
    pub fn snapshot(&mut self, snap: &crate::live::server::LiveSnapshot) {
        self.stage = Some(snap.stage);
        self.last = snap.first_frame.checked_sub(1);
        self.resuming = None;
        self.want_resume = false;
        for c in &snap.competitors {
            self.through.insert(c.participant, c.through_us);
            if let Some(end) = c.end {
                self.ends.insert(c.participant, end);
            }
        }
        for c in &snap.commitments {
            if !self.commitments.iter().any(|k| k.attack == c.attack) {
                self.commitments.push(*c);
            }
        }
    }

    /// Whether to ask for the frames after [`Self::last`] now; asking
    /// counts as asked (at `now`).
    pub fn wants_resume(&mut self, now: u64) -> Option<Option<u32>> {
        if !self.want_resume {
            return None;
        }
        self.want_resume = false;
        self.resuming = Some(now);
        Some(self.last)
    }

    /// Ask for a resume from the start (a new socket).
    pub fn ask_resume(&mut self) {
        self.want_resume = true;
        self.resuming = None;
    }

    pub fn moments(&self, p: ParticipantId) -> &[Moment] {
        self.moments.get(&p).map_or(&[], Vec::as_slice)
    }

    pub fn end(&self, p: ParticipantId) -> Option<StreamEnd> {
        self.ends.get(&p).copied()
    }

    pub fn through_us(&self, p: ParticipantId) -> Option<i64> {
        self.through.get(&p).copied()
    }

    pub fn commitments(&self) -> &[Commitment] {
        &self.commitments
    }

    pub fn checkpoints(&self) -> &[CheckpointNews] {
        &self.checkpoints
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::LIVE_V1;

    fn tap(at_ms: i32, lane: u8) -> Moment {
        Moment {
            at_ms,
            tick: i64::from(at_ms) / 10,
            lane,
            kind: MomentKind::Tap {
                tier: 0,
                offset_ms: -3,
                w0: true,
            },
            life_ppm: 1_000_000,
            combo: 1,
            score: 100,
        }
    }

    #[test]
    fn batches_are_cut_on_the_interval_and_kept_until_acknowledged() {
        let mut out = Outbox::new(StageId(4), LIVE_V1);
        // Nothing judged yet: nothing sent.
        assert!(out.due(0).is_empty());
        out.judged_to(-500_000);
        out.moment(tap(10, 0));
        let b = out.due(0);
        assert_eq!(b.len(), 1);
        assert_eq!((b[0].seq, b[0].moments.len()), (1, 1));
        out.moment(tap(20, 1));
        out.judged_to(30_000);
        // Not before the interval.
        assert!(out.due(100).is_empty());
        let b = out.due(250);
        assert_eq!((b[0].seq, b[0].through_us), (2, 30_000));
        assert_eq!(out.unacked(), 2);
        out.ack(1);
        assert_eq!(out.unacked(), 1);
        // Overdue: sent again with the next.
        out.judged_to(40_000);
        let b = out.due(1_300);
        assert_eq!(b.iter().map(|b| b.seq).collect::<Vec<_>>(), vec![2, 3]);
        // Unanswered, the next resend waits twice as long, then four
        // times, never more than eight: a room behind is not flooded.
        let b = out.due(2_300);
        assert!(b.iter().all(|b| b.seq > 3), "not again after a second");
        let b = out.due(3_300);
        assert_eq!(b.iter().filter(|b| b.seq <= 3).count(), 2, "after two");
        assert!(out.due(6_300).iter().all(|b| b.seq > 3), "four now");
        assert_eq!(out.due(7_300).iter().filter(|b| b.seq <= 3).count(), 2);
        // An acknowledgement starts the wait over.
        out.ack(2);
        // The end goes at once, then nothing more.
        out.end(StreamEnd::Out { at_us: 41_000 });
        out.moment(tap(99, 2));
        let b = out.due(1_310);
        assert_eq!(
            b.last().unwrap().end,
            Some(StreamEnd::Out { at_us: 41_000 })
        );
        assert!(b.last().unwrap().moments.is_empty());
        out.ack(4);
        assert!(out.done());
    }

    #[test]
    fn a_reader_takes_frames_in_order_and_asks_again_after_a_gap() {
        let frame = |n: u32| Frame {
            stage: StageId(4),
            n,
            at: Millis(u64::from(n) * 250),
            entries: vec![FrameEntry {
                participant: ParticipantId(2),
                seq: n,
                through_us: i64::from(n) * 250_000,
                moments: vec![tap(n as i32, 0)],
                end: None,
            }],
            commitments: Vec::new(),
            checkpoints: Vec::new(),
            settling: None,
        };
        let mut r = FrameReader::new(StageId(4));
        assert!(r.take(&frame(1), 0));
        assert!(!r.take(&frame(1), 0), "a duplicate");
        assert!(!r.take(&frame(3), 10), "a gap");
        assert_eq!(r.wants_resume(10), Some(Some(1)));
        assert!(!r.take(&frame(4), 20));
        assert_eq!(r.wants_resume(20), None, "asked already");
        assert!(r.take(&frame(2), 30));
        assert!(r.take(&frame(3), 30));
        assert!(r.take(&frame(4), 30));
        assert_eq!(r.moments(ParticipantId(2)).len(), 4);
        assert_eq!(r.through_us(ParticipantId(2)), Some(1_000_000));
    }
}
