//! The client's side of live play, sans-IO, inside
//! [`crate::OnlineAuthority`]: on every socket to a room that plays live
//! it says [`ClientLive::Hello`], pings for the clock (a burst at first,
//! then every few seconds), sends what the game queues ([`LiveLink::send`]:
//! a stage's readiness to play live, its battle plan), cuts the local
//! play's stream into batches ([`LiveLink::stream`]) and sends them again
//! until a frame acknowledges them, and reads the room's frames in order
//! ([`FrameReader`]), asking for them again after a gap or a reconnect.
//! What the game draws comes out of [`LiveLink::take_news`].

use crate::ids::{Millis, ParticipantId, StageId};
use crate::live::clock::{ClockEstimate, ClockEstimator, Ping};
use crate::live::server::LiveSnapshot;
use crate::live::cpus::{CpuEntry, CpuNews};
use crate::live::stream::{Frame, FrameReader, LiveReport, Outbox, StreamEnd};
use crate::live::{
    ClientLive, ClientLiveEnvelope, LIVE_VERSION, LiveRefusal, LiveRules, ServerLive, wire,
};
use std::collections::BTreeMap;

/// What the room's live side said, for the game.
#[derive(Clone, Debug, PartialEq)]
pub enum LiveNews {
    /// A frame, taken in order (the reader has it too).
    Frame(Frame),
    /// The stage as it stands; the frames after it follow.
    Snapshot(Box<LiveSnapshot>),
    Refused(LiveRefusal),
}

/// What to put on the socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveSend {
    Text(String),
    Binary(Vec<u8>),
}

/// The live link of one client.
#[derive(Clone, Debug)]
pub struct LiveLink {
    rules: LiveRules,
    clock: ClockEstimator,
    on: bool,
    hello_due: bool,
    next_ping: u32,
    ping_at: u64,
    burst: u32,
    me: Option<ParticipantId>,
    outboxes: BTreeMap<StageId, Outbox>,
    reader: Option<FrameReader>,
    queued: Vec<ClientLive>,
    news: Vec<LiveNews>,
    fresh: bool,
    audience: Option<(u32, bool)>,
    /// The CPUs' news this device computed, by stage (`crate::live::cpus`).
    cpus: BTreeMap<StageId, CpuOutbox>,
}

/// What a device computed of a stage's CPUs, kept to send
/// (`crate::live::cpus`, `online-cpus-v0`): every report made (sent again
/// whole on a new socket or when the device becomes one that sends: the
/// room drops what it has), each CPU's judged-to time and end.
#[derive(Clone, Debug, Default)]
pub struct CpuOutbox {
    /// Whether this device sends ([`crate::live::cpus::sends_for_cpus`]).
    sends: bool,
    cpus: BTreeMap<ParticipantId, CpuKept>,
    last_sent_ms: Option<u64>,
    /// Messages sent so far, for the debug state and the tests.
    pub sent: u32,
}

#[derive(Clone, Debug, Default)]
struct CpuKept {
    reports: Vec<LiveReport>,
    sent: usize,
    through_us: i64,
    through_sent: i64,
    end: Option<StreamEnd>,
    end_sent: bool,
    /// Its run once its stream ended (`ClientLive::CpuRun`), and whether
    /// it went.
    run: Option<(crate::battle::ReportedRun, Option<crate::battle::RunDetail>)>,
    run_sent: bool,
}

impl CpuOutbox {
    /// The CPUs' runs not sent yet (`ClientLive::CpuRun`), marked sent.
    fn runs(&mut self, stage: StageId, all: bool) -> Vec<ClientLive> {
        let mut out = Vec::new();
        for (cpu, k) in &mut self.cpus {
            if all {
                k.run_sent = false;
            }
            if k.run_sent {
                continue;
            }
            if let Some((run, detail)) = &k.run {
                k.run_sent = true;
                out.push(ClientLive::CpuRun {
                    stage,
                    cpu: *cpu,
                    run: *run,
                    detail: detail.clone(),
                });
            }
        }
        out
    }

    fn news(&mut self, stage: StageId, all: bool) -> Vec<CpuNews> {
        let rules = crate::live::cpus::ONLINE_CPUS_V0;
        let mut out = Vec::new();
        let mut cur = CpuNews {
            stage,
            entries: Vec::new(),
        };
        let mut reports = 0usize;
        let mut judged = 0usize;
        for (cpu, k) in &mut self.cpus {
            if all {
                k.sent = 0;
                k.end_sent = false;
            }
            let fresh = k.sent < k.reports.len() || (k.end.is_some() && !k.end_sent);
            if !fresh {
                continue;
            }
            let mut entry = CpuEntry {
                cpu: *cpu,
                through_us: k.through_us,
                reports: Vec::new(),
                end: None,
            };
            for r in &k.reports[k.sent..] {
                // A message's bounds (and well under the room's 64 KiB).
                if reports + 1 > rules.max_reports
                    || judged + r.judged.len() > 8_192
                    || cur.entries.len() + 1 >= rules.max_entries
                {
                    if !entry.reports.is_empty() {
                        cur.entries.push(std::mem::replace(
                            &mut entry,
                            CpuEntry {
                                cpu: *cpu,
                                through_us: k.through_us,
                                reports: Vec::new(),
                                end: None,
                            },
                        ));
                    }
                    out.push(std::mem::replace(
                        &mut cur,
                        CpuNews {
                            stage,
                            entries: Vec::new(),
                        },
                    ));
                    reports = 0;
                    judged = 0;
                }
                reports += 1;
                judged += r.judged.len();
                entry.reports.push(r.clone());
            }
            k.sent = k.reports.len();
            if k.end.is_some() {
                entry.end = k.end;
                k.end_sent = true;
            }
            k.through_sent = k.through_us;
            cur.entries.push(entry);
        }
        if !cur.entries.is_empty() {
            out.push(cur);
        }
        out
    }
}

impl LiveLink {
    pub fn new(rules: LiveRules) -> Self {
        Self {
            rules,
            clock: ClockEstimator::new(),
            on: false,
            hello_due: false,
            next_ping: 1,
            ping_at: 0,
            burst: 0,
            me: None,
            outboxes: BTreeMap::new(),
            reader: None,
            queued: Vec::new(),
            news: Vec::new(),
            fresh: false,
            audience: None,
            cpus: BTreeMap::new(),
        }
    }

    /// Stage `stage`'s CPU `cpu` judged to song µs `through_us`, with the
    /// reports it made since (`crate::live::cpus`): kept, and sent while
    /// this device is one that sends.
    pub fn cpu_news(
        &mut self,
        stage: StageId,
        cpu: ParticipantId,
        through_us: i64,
        reports: Vec<LiveReport>,
        end: Option<StreamEnd>,
    ) {
        let k = self
            .cpus
            .entry(stage)
            .or_default()
            .cpus
            .entry(cpu)
            .or_default();
        k.through_us = k.through_us.max(through_us);
        k.reports.extend(reports);
        if k.end.is_none() {
            k.end = end;
        }
    }

    /// Stage `stage`'s CPU `cpu`'s run as this device computed it, once its
    /// stream ended: sent (as `ClientLive::CpuRun`) while this device is
    /// one that sends.
    pub fn cpu_run(
        &mut self,
        stage: StageId,
        cpu: ParticipantId,
        run: crate::battle::ReportedRun,
        detail: Option<crate::battle::RunDetail>,
    ) {
        let k = self
            .cpus
            .entry(stage)
            .or_default()
            .cpus
            .entry(cpu)
            .or_default();
        if k.run.is_none() {
            k.run = Some((run, detail));
        }
    }

    /// Whether this device sends stage `stage`'s CPUs' news
    /// (`crate::live::cpus::sends_for_cpus`, read from the mirror each
    /// time it changes). Becoming one sends everything kept.
    pub fn set_cpu_sends(&mut self, stage: StageId, sends: bool) {
        let o = self.cpus.entry(stage).or_default();
        if sends && !o.sends {
            for k in o.cpus.values_mut() {
                k.sent = 0;
                k.end_sent = false;
                k.run_sent = false;
            }
        }
        o.sends = sends;
    }

    /// The CPUs' news kept for stage `stage`.
    pub fn cpu_outbox(&self, stage: StageId) -> Option<&CpuOutbox> {
        self.cpus.get(&stage)
    }

    pub fn rules(&self) -> LiveRules {
        self.rules
    }

    /// Whether the socket open now plays live (its room's policy says so
    /// and the transport carries binary messages).
    pub fn is_on(&self) -> bool {
        self.on
    }

    /// The room's clock as estimated so far.
    pub fn clock(&self) -> Option<ClockEstimate> {
        self.clock.estimate()
    }

    /// The room's time at the device's `local_ms`, once estimated.
    pub fn room_ms(&self, local_ms: u64) -> Option<Millis> {
        let e = self.clock()?;
        let us = e.room_us(local_ms as i64 * 1_000);
        Some(Millis(u64::try_from(us.max(0) / 1_000).unwrap_or(0)))
    }

    /// Who this client is in the room (its stream's acknowledgements are
    /// read from its own frame entries).
    pub fn set_me(&mut self, me: ParticipantId) {
        self.me = Some(me);
    }

    /// Send a live message (on the next poll; held while no live socket
    /// is open). [`ClientLive::Hello`] and pings are the link's own.
    pub fn send(&mut self, msg: ClientLive) {
        self.queued.push(msg);
    }

    /// The local play's stream of `stage`.
    pub fn stream(&mut self, stage: StageId) -> &mut Outbox {
        let rules = self.rules;
        self.outboxes
            .entry(stage)
            .or_insert_with(|| Outbox::new(stage, rules))
    }

    /// Stage `stage`'s live part is gone (the room answered its batches
    /// `unknown_stage`: it restarted and lost the stage in play): its
    /// stream and its frames are dropped, nothing more is sent for it.
    pub fn forget(&mut self, stage: StageId) {
        self.outboxes.remove(&stage);
        if self.reader.as_ref().and_then(FrameReader::stage) == Some(stage) {
            self.reader = None;
        }
    }

    pub fn outbox(&self, stage: StageId) -> Option<&Outbox> {
        self.outboxes.get(&stage)
    }

    /// The frames read so far of the stage followed.
    pub fn reader(&self) -> Option<&FrameReader> {
        self.reader.as_ref()
    }

    /// The room's audience as last said (`ServerLive::Audience`): how many
    /// watch and whether it is open; none until said (a room without an
    /// audience never says it).
    pub fn audience(&self) -> Option<(u32, bool)> {
        self.audience
    }

    pub fn take_news(&mut self) -> Vec<LiveNews> {
        std::mem::take(&mut self.news)
    }

    /// A socket to a room that plays live by `rules` (its policy's: the
    /// batches' period) gave its first snapshot.
    pub(crate) fn opened(&mut self, now: u64, rules: LiveRules) {
        if rules != self.rules {
            self.rules = rules;
            for o in self.outboxes.values_mut() {
                o.set_rules(rules);
            }
        }
        self.on = true;
        self.hello_due = true;
        self.burst = self.rules.ping_burst;
        self.ping_at = now;
        // Everything not acknowledged goes again on the new socket.
        self.fresh = true;
        if let Some(r) = self.reader.as_mut() {
            r.ask_resume();
        }
    }

    /// The socket closed.
    pub(crate) fn closed(&mut self) {
        self.on = false;
    }

    /// The stage the room plays live now, if any (from the mirror): a
    /// reader for it.
    /// `started`: its song is playing already (a socket opened mid-stage):
    /// the stage as it stands is asked for at once.
    pub(crate) fn follow(&mut self, stage: Option<StageId>, started: bool) {
        if let Some(stage) = stage
            && self
                .reader
                .as_ref()
                .is_none_or(|r| r.stage().is_none_or(|s| s < stage))
        {
            let mut r = FrameReader::new(stage);
            if started {
                r.ask_resume();
            }
            self.reader = Some(r);
        }
    }

    pub(crate) fn text(&mut self, msg: ServerLive, now: u64) {
        match msg {
            ServerLive::Pong(p) => {
                self.clock.pong(&p, now);
            }
            ServerLive::Refused { refusal } => self.news.push(LiveNews::Refused(refusal)),
            ServerLive::Audience { count, open } => self.audience = Some((count, open)),
            ServerLive::Snapshot(snap) => {
                if self
                    .reader
                    .as_ref()
                    .is_none_or(|r| r.stage().is_none_or(|s| s <= snap.stage))
                {
                    let r = self
                        .reader
                        .get_or_insert_with(|| FrameReader::new(snap.stage));
                    if r.stage() != Some(snap.stage) {
                        *r = FrameReader::new(snap.stage);
                    }
                    r.snapshot(&snap);
                }
                if let Some(me) = self.me
                    && let Some(c) = snap.competitors.iter().find(|c| c.participant == me)
                    && let Some(o) = self.outboxes.get_mut(&snap.stage)
                {
                    o.ack(c.seq);
                }
                self.news.push(LiveNews::Snapshot(snap));
            }
        }
    }

    pub(crate) fn binary(&mut self, bytes: &[u8], now: u64) {
        let Ok(wire::Binary::Frame(frame)) = wire::decode(bytes) else {
            return;
        };
        self.follow(Some(frame.stage), false);
        let Some(r) = self.reader.as_mut() else {
            return;
        };
        if !r.take(&frame, now) {
            return;
        }
        if let Some(me) = self.me
            && let Some(e) = frame.entries.iter().find(|e| e.participant == me)
            && let Some(o) = self.outboxes.get_mut(&frame.stage)
        {
            o.ack(e.seq);
        }
        self.news.push(LiveNews::Frame(frame));
    }

    /// What to send now: the hello, pings, queued messages, batches, a
    /// resume after a gap.
    pub(crate) fn due(&mut self, now: u64) -> Vec<LiveSend> {
        let mut out = Vec::new();
        if !self.on {
            // No socket: the stream is still cut on time, its batches
            // kept for the next socket (sent again as every batch not
            // acknowledged is), so that none of them carries more than an
            // interval's moments however long the link was down.
            for o in self.outboxes.values_mut() {
                let _ = o.due(now);
            }
            return out;
        }
        let text = |m: ClientLive| {
            serde_json::to_string(&ClientLiveEnvelope::new(m))
                .ok()
                .map(LiveSend::Text)
        };
        if self.hello_due {
            self.hello_due = false;
            out.extend(text(ClientLive::Hello {
                version: LIVE_VERSION,
            }));
        }
        if now >= self.ping_at {
            let n = self.next_ping;
            self.next_ping = self.next_ping.wrapping_add(1);
            out.extend(text(ClientLive::Ping(Ping { n, t: now })));
            self.ping_at = if self.burst > 1 {
                self.burst -= 1;
                now + self.rules.ping_burst_gap_ms
            } else {
                self.burst = 0;
                let busy = self.reader.is_some() || self.outboxes.values().any(|o| !o.done());
                now + if busy {
                    self.rules.ping_ms
                } else {
                    self.rules.idle_ping_ms
                }
            };
        }
        for m in std::mem::take(&mut self.queued) {
            out.extend(text(m));
        }
        let fresh = std::mem::take(&mut self.fresh);
        for o in self.outboxes.values_mut() {
            let batches = if fresh {
                let mut b = o.resend(now);
                b.extend(o.due(now));
                b
            } else {
                o.due(now)
            };
            out.extend(
                batches
                    .iter()
                    .map(|b| LiveSend::Binary(wire::encode_batch(b))),
            );
        }
        let send_ms = crate::live::cpus::ONLINE_CPUS_V0.send_ms;
        for (stage, o) in &mut self.cpus {
            if !o.sends {
                continue;
            }
            if !fresh && o.last_sent_ms.is_some_and(|t| now < t + send_ms) {
                continue;
            }
            let news = o.news(*stage, fresh);
            let runs = o.runs(*stage, fresh);
            if news.is_empty() && runs.is_empty() {
                continue;
            }
            o.last_sent_ms = Some(now);
            o.sent += news.len() as u32;
            out.extend(news.iter().map(|n| LiveSend::Binary(wire::encode_cpus(n))));
            // A run after the news of its end.
            out.extend(runs.into_iter().filter_map(text));
        }
        if let Some(r) = self.reader.as_mut()
            && let Some(after) = r.wants_resume(now)
            && let Some(stage) = r.stage()
        {
            out.extend(text(ClientLive::Resume { stage, after }));
        }
        // Streams of stages gone by, ended and acknowledged, are dropped.
        let following = self.reader.as_ref().and_then(FrameReader::stage);
        self.outboxes
            .retain(|s, o| Some(*s) == following || !o.done());
        self.cpus
            .retain(|s, _| following.is_none_or(|f| *s >= f));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::LIVE_V1;
    use crate::live::stream::{Moment, MomentKind};

    fn moment(at_ms: i32) -> Moment {
        Moment {
            at_ms,
            tick: i64::from(at_ms),
            lane: (at_ms % 4) as u8,
            kind: MomentKind::Tap {
                tier: 1,
                offset_ms: -12,
                w0: false,
            },
            life_ppm: 987_654,
            combo: 1_234,
            score: 4_321_000,
        }
    }

    /// The server refuses a binary message over 64 KiB (`bad_batch`): a
    /// batch never comes near it. Cut every interval, also while the link
    /// is down (a long reconnect at 20 notes a second), none carries more
    /// than an interval's moments, and the largest the room takes (its
    /// bounds' moments, reports of a dense checkpoint) is well under.
    #[test]
    fn batches_stay_small_through_a_long_disconnect() {
        let mut link = LiveLink::new(LIVE_V1);
        let stage = StageId(1);
        link.opened(0, LIVE_V1);
        let _ = link.due(0);
        link.closed();
        let mut sent = Vec::new();
        for ms in (0..60_000).step_by(50) {
            let o = link.stream(stage);
            o.moment(moment(ms));
            o.judged_to(i64::from(ms) * 1_000);
            sent.extend(link.due(ms as u64));
        }
        assert!(sent.is_empty(), "nothing sent while down");
        link.opened(60_000, LIVE_V1);
        let batches: Vec<Vec<u8>> = link
            .due(60_000)
            .into_iter()
            .filter_map(|s| match s {
                LiveSend::Binary(b) => Some(b),
                LiveSend::Text(_) => None,
            })
            .collect();
        assert!(batches.len() > 200, "{} batches", batches.len());
        for b in &batches {
            let Ok(wire::Binary::Batch(batch)) = wire::decode(b) else {
                panic!("a batch");
            };
            assert!(batch.moments.len() <= 6, "{}", batch.moments.len());
            assert!(b.len() < 1_024);
        }
        // The largest batch the room's bounds allow, reports of a
        // checkpoint of a very dense chart (64 judgments) included.
        let mut o = Outbox::new(stage, LIVE_V1);
        o.judged_to(1);
        for i in 0..LIVE_V1.max_batch_moments {
            o.moment(moment(i as i32 * 7));
        }
        for k in 0..LIVE_V1.max_batch_reports {
            o.report(LiveReport {
                k: k as u32,
                judged: (0..64)
                    .map(|j| crate::attacks::Judged {
                        at_us: j * 31_250,
                        tier: (j % 6) as u8,
                    })
                    .collect(),
                life_ppm: 500_000,
                out_at_us: None,
                targeting: None,
            });
        }
        let biggest = o.due(0).pop().unwrap();
        let bytes = wire::encode_batch(&biggest);
        assert!(bytes.len() < 32 * 1_024, "{} bytes", bytes.len());
    }
}
