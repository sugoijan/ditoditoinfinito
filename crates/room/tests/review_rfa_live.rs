//! Live play's fixes from the code review of 2026-11-22 (round RFa): the
//! room's live side fed what a modified client could send, each case
//! reproduced first with small numbers (never the attack itself).

#![allow(dead_code)]

mod common;

use common::{Room, plan_under, song};
use ddi_room::attacks::ATTACKS;
use ddi_room::live::plan::{BattlePlan, seed_for};
use ddi_room::live::server::{LiveOut, LiveReply, LiveServer};
use ddi_room::live::stream::{Batch, Moment, MomentKind};
use ddi_room::live::{ClientLive, LIVE_V2, LiveRefusal, ServerLive, wire};
use ddi_room::*;

/// A stage played live: the room's state through the reducer, its live
/// side beside it, as the server holds them.
struct Live {
    r: Room,
    server: LiveServer,
    stage: StageId,
    host: ParticipantId,
    ana: ParticipantId,
    watcher: ParticipantId,
    cpus: Vec<ParticipantId>,
}

/// The stage's ladder: two rungs besides its chart.
const LADDER: [&str; 2] = ["digest-1-easy", "digest-1-hard"];

impl Live {
    /// A hosted live room of the host, ana and a watcher (with `cpus`
    /// CPUs of a roster), its first stage started: with attacks, a battle
    /// plan of two rungs offered by the host and taken.
    fn new(attacks: bool, cpus: usize) -> Live {
        let mut policy = RoomPolicy::default().with_live().with_audience();
        policy.readiness.window_ms = 15_000;
        let mut rules = MatchRules::hosted();
        rules.stages = 2;
        rules.set_attacks(attacks);
        rules.seats = 2 + cpus as u16;
        if cpus > 0 {
            rules.cpu.mode = CpuMode::Roster;
            rules.cpu.roster = (0..cpus)
                .map(|i| OpponentSpec {
                    level: 5,
                    seed: 1_000 + i as u64,
                    personality: None,
                    name: Some(format!("Cpu{i}")),
                    targeting: None,
                })
                .collect();
        }
        let mut r = Room::open("host", "Host", policy, rules, ContentSource::Shared);
        let host = r.host;
        let ana = r.join_as("ana", Intent::Play, false).unwrap();
        let watcher = r.join_as("wes", Intent::Watch, false).unwrap();
        r.by(host, Command::Pick { content: song(1), card: None }).unwrap();
        let stage = r.s.upcoming_stage().unwrap();
        r.sys(Command::SetContentStatus {
            stage,
            status: ContentStatus::Available,
            share: Some("sh".into()),
        })
        .unwrap();
        let mut server = LiveServer::new(LIVE_V2);
        let digest = attacks.then(|| {
            let plan = Self::plan(&r, stage);
            let out = server.receive(
                &r.s,
                host,
                ClientLive::OfferPlan { stage, plan: Box::new(plan.clone()) },
                Millis(r.now),
            );
            assert!(out.reply.is_empty(), "{:?}", out.reply);
            for c in out.commands {
                r.sys(c).unwrap();
            }
            plan.digest
        });
        for p in [host, ana] {
            server.receive(
                &r.s,
                p,
                ClientLive::Prepared { stage, horizon_us: 2_000_000, plan: digest.clone() },
                Millis(r.now),
            );
            r.load_for(p, stage, Loading::Loaded).unwrap();
            r.ready_for(p, stage, true).unwrap();
        }
        let mut live = Live { r, server, stage, host, ana, watcher, cpus: Vec::new() };
        for _ in 0..2_000 {
            if matches!(live.r.s.phase, Phase::Stage { .. }) {
                break;
            }
            live.wait(50);
        }
        assert!(matches!(live.r.s.phase, Phase::Stage { .. }), "{:?}", live.r.s.phase);
        live.cpus = live
            .r
            .s
            .stage(stage)
            .unwrap()
            .play
            .as_ref()
            .unwrap()
            .lineup
            .iter()
            .copied()
            .filter(|p| live.r.s.member(*p).is_some_and(Member::is_cpu))
            .collect();
        live
    }

    /// The stage's plan as every device works it out: its seed the
    /// stage's, two rungs.
    fn plan(r: &Room, stage: StageId) -> BattlePlan {
        let seed = seed_for(&r.s.room, stage, &song(1).chart);
        plan_under(ATTACKS.id, 60, seed, |j| j % 2 == 1)
            .with_at_once(Some(3))
            .with_ladder(LADDER.iter().map(|d| d.to_string()).collect())
    }

    /// `ms` pass on the room's clock, its live side ticking.
    fn wait(&mut self, ms: u64) -> LiveOut {
        let to = self.r.now + ms;
        self.r.wait_to(to);
        let out = self.server.tick(&self.r.s, Millis(self.r.now));
        self.apply(&out);
        out
    }

    fn apply(&mut self, out: &LiveOut) {
        for c in &out.commands {
            let _ = self.r.sys(c.clone());
        }
    }

    fn song_ms(&self) -> i32 {
        let zero = self.r.s.stage(self.stage).unwrap().play.as_ref().unwrap().song_zero_at.unwrap();
        (self.r.now as i64 - zero.0 as i64) as i32
    }

    /// `from`'s batch, as its bytes come.
    fn send(&mut self, from: ParticipantId, b: &Batch) -> LiveOut {
        let out = self
            .server
            .receive_binary(&self.r.s, from, &wire::encode_batch(b), Millis(self.r.now));
        self.apply(&out);
        out
    }

    fn batch(&self, seq: u32, moments: Vec<Moment>) -> Batch {
        let through_us = moments.last().map_or(0, |m| i64::from(m.at_ms) * 1_000);
        Batch { stage: self.stage, seq, through_us, moments, reports: Vec::new(), end: None }
    }

    fn seq(&self, p: ParticipantId) -> u32 {
        self.server
            .stage()
            .unwrap()
            .snapshot(1, Millis(self.r.now))
            .competitors
            .iter()
            .find(|c| c.participant == p)
            .unwrap()
            .seq
    }
}

fn tap(at_ms: i32, combo: u32) -> Moment {
    Moment {
        at_ms,
        tick: i64::from(at_ms),
        lane: (at_ms % 4) as u8,
        kind: MomentKind::Tap { tier: 1, offset_ms: -5, w0: false },
        life_ppm: 900_000,
        combo,
        score: 100,
    }
}

fn refused(out: &LiveOut) -> Option<LiveRefusal> {
    out.reply.iter().find_map(|r| match r {
        LiveReply::Message(ServerLive::Refused { refusal }) => Some(refusal.clone()),
        _ => None,
    })
}

/// Finding 1: the room relayed a member's moments verbatim; it refuses a
/// batch whose moments no play says: a life past a full gauge, a step to
/// a rung past the plan's ladder, more steps than a whole play could
/// make (`LiveRules::steps_a_batch`). An honest batch, a step to the
/// ladder's last rung among them, is taken.
#[test]
fn a_batch_no_play_could_send_is_refused() {
    let mut l = Live::new(true, 0);
    let (ana, stage) = (l.ana, l.stage);
    let at = l.song_ms();
    let mut life = tap(at, 1);
    life.life_ppm = 1_000_001;
    assert_eq!(refused(&l.send(ana, &l.batch(1, vec![life]))), Some(LiveRefusal::BadBatch { stage }));
    assert_eq!(l.seq(ana), 0, "nothing of it taken");
    let step = |at_ms: i32, rung: u8| Moment { kind: MomentKind::Step { rung }, ..tap(at_ms, 1) };
    assert_eq!(
        refused(&l.send(ana, &l.batch(1, vec![step(at, 3)]))),
        Some(LiveRefusal::BadBatch { stage })
    );
    let many: Vec<Moment> = (0..100).map(|i| step(at + i, 1 + (i % 2) as u8)).collect();
    assert_eq!(refused(&l.send(ana, &l.batch(1, many))), Some(LiveRefusal::BadBatch { stage }));
    assert_eq!(l.seq(ana), 0);
    // An honest batch: a full gauge, a step to the last rung.
    let mut full = tap(at, 2);
    full.life_ppm = 1_000_000;
    let out = l.send(ana, &l.batch(1, vec![full, step(at + 1, 2)]));
    assert_eq!(refused(&out), None);
    assert_eq!(l.seq(ana), 1);
    assert_eq!(l.server.stage().unwrap().steps(ana), 2);
}

/// `n` taps from `at`, a millisecond apart.
fn taps(at: i32, n: usize) -> Vec<Moment> {
    (0..n).map(|i| tap(at + i as i32, i as u32)).collect()
}

/// Finding 2: the room took a member's batches as fast as they came and
/// sent them all in its next frame, kept 120 times; it takes at most
/// `LiveRules::max_tick_moments_each` moments of one member a tick
/// (four full batches: an honest reconnect's resend of every batch not
/// acknowledged is taken over a few ticks) and `max_tick_moments` of
/// everyone's; a batch past them is not taken yet, as one that came out
/// of order, and is taken when sent again. Proven with a room's total
/// of 3,000 moments a tick.
#[test]
fn a_tick_takes_a_bounded_number_of_moments() {
    let mut l = Live::new(true, 0);
    let rules = ddi_room::live::LiveRules { max_tick_moments: 3_000, ..LIVE_V2 };
    assert_eq!(LIVE_V2.max_tick_moments_each, 4 * LIVE_V2.max_batch_moments);
    l.server = LiveServer::new(rules);
    l.wait(100);
    let (ana, host) = (l.ana, l.host);
    let at = l.song_ms();
    let full = LIVE_V2.max_batch_moments;
    for seq in 1..=5 {
        let b = l.batch(seq, taps(at, full));
        assert_eq!(refused(&l.send(ana, &b)), None);
    }
    assert_eq!(l.seq(ana), 4, "four full batches a tick from one member");
    // Everyone's: the host's first two batches fit, the third does not.
    for seq in 1..=3 {
        let b = l.batch(seq, taps(at, 400));
        l.send(host, &b);
    }
    assert_eq!(l.seq(host), 2);
    let out = l.wait(100);
    let sent: usize = out.broadcast.iter().flat_map(|f| &f.frame.entries).map(|e| e.moments.len()).sum();
    assert_eq!(sent, 4 * full + 800);
    // Sent again, the rest is taken in the next tick.
    let b = l.batch(5, taps(at, full));
    l.send(ana, &b);
    let b = l.batch(3, taps(at, 400));
    l.send(host, &b);
    assert_eq!((l.seq(ana), l.seq(host)), (5, 3));
}

/// Finding 2: `Resume` was answered at any rate with a snapshot and up to
/// 120 kept frames; a member is answered at most
/// `LiveRules::resumes_each` times in `LiveRules::resume_window_ms`
/// (an honest device asks on each new socket and at most once in 2 s
/// after a gap), the rest passed over (its reader asks again).
#[test]
fn resumes_are_answered_at_a_bounded_rate() {
    let mut l = Live::new(true, 0);
    l.wait(100);
    let (ana, stage) = (l.ana, l.stage);
    let mut answered = 0;
    for _ in 0..10 {
        let out = l.server.receive(&l.r.s, ana, ClientLive::Resume { stage, after: None }, Millis(l.r.now));
        if !out.reply.is_empty() {
            answered += 1;
        }
    }
    assert_eq!(answered, LIVE_V2.resumes_each);
    // The host is answered still.
    let out = l.server.receive(&l.r.s, l.host, ClientLive::Resume { stage, after: None }, Millis(l.r.now));
    assert!(!out.reply.is_empty());
    l.wait(LIVE_V2.resume_window_ms);
    let out = l.server.receive(&l.r.s, ana, ClientLive::Resume { stage, after: None }, Millis(l.r.now));
    assert!(!out.reply.is_empty(), "answered again after the window");
}
