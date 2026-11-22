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
