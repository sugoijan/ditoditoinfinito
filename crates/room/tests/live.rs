//! Live play through the fake room (`ddi_room::live`,
//! `docs/plans/multiplayer-online-play.md`): competitors' plays streamed
//! with jitter, loss and a reconnect, every recipient's fields ending equal
//! to what was sent; one attack trace through the ledger on one device and
//! through the room, with reports in different orders and some late; the
//! battle plan's digest checked; the stage settled by the room; 73
//! competitors at speed, with what a tick costs.

use ddi_room::attacks::{
    ATTACKS, AttackEvent, CheckpointRecord, Judged, POWER_CLASSES_V0, Reporter, RosterSeat,
    StageBattle, StageRoster,
};
use ddi_room::fake::{FakeServer, FakeSocket, Fault, Latency, SharedFake};
use ddi_room::live::plan::BattlePlan;
use ddi_room::live::server::{LiveDurable, LiveServer, Prepared};
use ddi_room::live::stream::{LiveReport, Moment, MomentKind, StreamEnd};
use ddi_room::live::{
    ClientLive, LIVE_V1, LiveNews, LiveRefusal, PlanDigest, SLEW_V0, SongSlew, VIEW_DELAY_V0,
    ViewDelay, ViewMeter, song_us_at, wire,
};
use ddi_room::{
    ChartKey, ClientEnvelope, ClientId, Clock, Command, ContentId, ContentStatus, Finish, Identity,
    Intent, Loading, ManualClock, MatchRules, Millis, OnlineAuthority, ParticipantId, Phase,
    ReportedRun, RoomAuthority, RoomId, RoomPolicy, RoomState, SongKey, StageId, TicketProblem,
};
use std::time::Instant;

const ROOM: &str = "Room000001";
const STAGE: StageId = StageId(1);
const STEP_MS: u64 = 20;

fn content() -> ContentId {
    ContentId {
        song: SongKey::new("song-1"),
        chart: ChartKey::new("digest-1"),
    }
}

/// A plan of `n` slots of 2 s, each with eight shocks a quarter second
/// apart where `room(j)`; checkpoints every 2 s from 2 s.
fn plan(n: usize, seed: u64, room: impl Fn(usize) -> bool) -> BattlePlan {
    use ddi_room::attacks::{Cell, Schedule, Section, Slot};
    const S: i64 = 1_000_000;
    let slots = (0..n)
        .map(|j| {
            let start_us = j as i64 * 2 * S;
            Slot {
                start_us,
                section: room(j).then(|| {
                    let cells: Vec<Cell> = (0..8)
                        .map(|i| Cell {
                            tick: (j as i64 * 4 * 48) + i * 24,
                            lane: (i % 4) as u8,
                            at_us: start_us + i * S / 4,
                        })
                        .collect();
                    let last = cells.last().unwrap().at_us;
                    Section {
                        clear_us: last + 100_000,
                        recover_after_us: last + 100_000,
                        cells,
                    }
                }),
            }
        })
        .collect();
    BattlePlan::new(
        ATTACKS.id,
        "placement-v2",
        POWER_CLASSES_V0.id,
        "ddi",
        seed,
        (1..=n as i64).map(|k| k * 2 * S).collect(),
        Schedule { slots },
    )
}

/// A competitor's play, scripted: its moments in time order.
#[derive(Clone, Debug)]
struct Script {
    moments: Vec<Moment>,
    /// Out at this song ms (its life emptied), if it goes out.
    out_ms: Option<i32>,
    /// Plays to this song ms.
    end_ms: i32,
}

fn script(seed: u64, gap_ms: i32, end_ms: i32, out_ms: Option<i32>) -> Script {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let (mut life, mut combo, mut score) = (1_000_000i64, 0u32, 0i64);
    let mut moments = Vec::new();
    let mut at = 1_000 + (seed % 7) as i32 * 13;
    while at < out_ms.unwrap_or(end_ms).min(end_ms) {
        let r = next() % 100;
        let tier = match r {
            0..=79 => 0u8,
            80..=93 => 1,
            94..=97 => 2,
            _ => 5,
        };
        if tier == 5 {
            life -= 40_000;
            combo = 0;
        } else {
            life += 2_000;
            combo += 1;
            score += 6_000 - i64::from(tier) * 1_000;
        }
        life = life.clamp(1, 1_000_000);
        let kind = if next() % 29 == 0 {
            MomentKind::HoldEnd { ok: tier < 5 }
        } else {
            MomentKind::Tap {
                tier,
                offset_ms: if tier == 5 {
                    0
                } else {
                    (next() % 41) as i16 - 20
                },
                w0: tier == 0 && next() % 2 == 0,
            }
        };
        moments.push(Moment {
            at_ms: at,
            tick: i64::from(at) * 48 * 2 / 1_000,
            lane: (next() % 4) as u8,
            kind,
            life_ppm: life as u32,
            combo,
            score,
        });
        at += gap_ms;
    }
    if let Some(out) = out_ms {
        moments.push(Moment {
            at_ms: out,
            tick: i64::from(out) * 48 * 2 / 1_000,
            lane: 0,
            kind: MomentKind::Tap {
                tier: 5,
                offset_ms: 0,
                w0: false,
            },
            life_ppm: 0,
            combo: 0,
            score,
        });
    }
    Script {
        moments,
        out_ms,
        end_ms,
    }
}

impl Script {
    /// The judgments of the chart's notes, as the ledger counts them.
    fn judged(&self) -> Vec<Judged> {
        self.moments
            .iter()
            .filter_map(|m| match m.kind {
                MomentKind::Tap { tier, .. } => Some(Judged {
                    at_us: i64::from(m.at_ms) * 1_000,
                    tier,
                }),
                _ => None,
            })
            .collect()
    }

    /// Life just before song µs `at`.
    fn life(&self, at: i64) -> u32 {
        self.moments
            .iter()
            .take_while(|m| i64::from(m.at_ms) * 1_000 < at)
            .last()
            .map_or(1_000_000, |m| m.life_ppm)
    }
}

/// A device's own clock: the room's, `offset_ms` ahead (negative: behind),
/// and running `ppm` millionths fast (negative: slow) from `origin` (the
/// room's ms), as a device's clock (or its sound card's) drifts.
#[derive(Clone, Debug)]
struct DriftClock {
    base: ManualClock,
    offset_ms: i64,
    ppm: i64,
    origin: u64,
}

impl Clock for DriftClock {
    fn now(&self) -> Millis {
        let room = self.base.now().0;
        let since = room.saturating_sub(self.origin) as i64;
        let drift = since * self.ppm / 1_000_000;
        Millis(room.saturating_add_signed(self.offset_ms + drift))
    }
}

/// A person in the room: the online authority on a fake socket with a
/// clock of its own, and the play it streams.
struct Player {
    who: Identity,
    id: ParticipantId,
    client: ClientId,
    seq: u64,
    link: OnlineAuthority<FakeSocket, DriftClock>,
    server: SharedFake,
    play: Option<Script>,
    sent: usize,
    reporter: Option<Reporter>,
    judged: Vec<Judged>,
    judged_sent: usize,
    ended: bool,
    /// Stops pumping (its device gone) from this room ms.
    gone_at: Option<u64>,
    news: Vec<LiveNews>,
    /// Its song clock mapped from the room's once, when it first knew the
    /// song's zero (its own clock's ms then, the song µs then), and run on
    /// its own clock from there, as a play's audio clock is (the game's
    /// way); none: the room's clock as estimated now, every time.
    mapped_once: bool,
    song_map: Option<(u64, i64)>,
    /// Mapped once and kept in step with the room's clock as estimated
    /// (`ddi_room::live::slew`, the game's way since 2026-11-13): its
    /// stream's and its views' times the room's, its judging its own.
    slew: Option<SongSlew>,
    /// Its device's clock (the link's own copy runs the same).
    clock: DriftClock,
}

impl Player {
    /// Its song time now (µs), where the stage's zero is known.
    fn song_us(&mut self) -> Option<i64> {
        let zero = self
            .link
            .room()
            .and_then(|r| r.stage(STAGE))
            .and_then(|s| s.play.as_ref())
            .and_then(|p| p.song_zero_at)?;
        let now = self.link.room_now()?;
        let estimated = song_us_at(zero, now);
        if !self.mapped_once {
            return Some(estimated);
        }
        let local = self.clock.now().0;
        let (at, song) = *self.song_map.get_or_insert((local, estimated));
        let mapped = song + (local as i64 - at as i64) * 1_000;
        let uncertainty = self.link.live().clock().map_or(0, |c| c.uncertainty_us);
        Some(match self.slew.as_mut() {
            Some(slew) => slew.step(mapped, Some(estimated), uncertainty),
            None => mapped,
        })
    }

    fn send(&mut self, cmd: Command) {
        self.seq += 1;
        let base = self.link.room().map(|r| r.revision);
        self.link.submit(ClientEnvelope::new(
            self.client.clone(),
            self.seq,
            base,
            cmd,
        ));
    }

    fn room(&self) -> &RoomState {
        self.link.room().expect("a snapshot")
    }

    fn pump(&mut self, now: u64) {
        if self.gone_at.is_some_and(|g| now >= g) {
            return;
        }
        if self.link.wants_ticket() {
            self.link.ticket_asked();
            let t = self.server.borrow_mut().ticket(&self.who);
            self.link.ticket(t.map_err(|_| TicketProblem::NotAMember));
        }
        self.feed();
        let _ = self.link.poll();
        self.news.extend(self.link.live_mut().take_news());
    }

    /// Its play, judged to the song time its device has now.
    fn feed(&mut self) {
        let Some(play) = self.play.clone() else {
            return;
        };
        if self.ended {
            return;
        }
        let Some(t_us) = self.song_us() else {
            return;
        };
        if t_us < -800_000 {
            return;
        }
        let out = self.link.live_mut().stream(STAGE);
        while let Some(m) = play.moments.get(self.sent) {
            if i64::from(m.at_ms) * 1_000 > t_us {
                break;
            }
            out.moment(*m);
            self.sent += 1;
        }
        let judged_to = t_us.min(i64::from(play.out_ms.unwrap_or(play.end_ms)) * 1_000);
        out.judged_to(judged_to);
        if let Some(r) = self.reporter.as_mut() {
            while let Some(j) = self.judged.get(self.judged_sent) {
                if j.at_us > t_us {
                    break;
                }
                r.judged(*j);
                self.judged_sent += 1;
            }
            if let Some(o) = play.out_ms
                && i64::from(o) * 1_000 <= t_us
            {
                r.out_at(i64::from(o) * 1_000);
            }
            for (k, rep) in r.due(judged_to, &|at| play.life(at)) {
                out.report(LiveReport::from_report(k, rep));
            }
        }
        if let Some(o) = play.out_ms
            && i64::from(o) * 1_000 <= t_us
        {
            out.end(StreamEnd::Out {
                at_us: i64::from(o) * 1_000,
            });
            self.ended = true;
            self.send(Command::ReportOutcome {
                stage: STAGE,
                run: ReportedRun::Out { at_ms: o as u64 },
                detail: None,
            });
        } else if t_us >= i64::from(play.end_ms) * 1_000 {
            out.end(StreamEnd::Finished);
            self.ended = true;
            let last = play.moments.last();
            self.send(Command::ReportOutcome {
                stage: STAGE,
                run: ReportedRun::Live {
                    life_ppm: last.map_or(1_000_000, |m| m.life_ppm),
                    accuracy_ppm: 900_000,
                    absolute: None,
                    score: last.map(|m| m.score),
                    steps: 0,
                },
                detail: None,
            });
        }
    }
}

struct Room {
    clock: ManualClock,
    server: SharedFake,
    players: Vec<Player>,
}

impl Room {
    fn now(&self) -> u64 {
        self.clock.now().0
    }

    fn pump(&mut self) {
        let now = self.now();
        for p in &mut self.players {
            p.pump(now);
        }
        self.server.borrow_mut().pump();
    }

    fn run(&mut self, ms: u64) {
        let end = self.now() + ms;
        while self.now() < end {
            self.clock.advance(STEP_MS);
            self.pump();
        }
    }

    fn state(&self) -> RoomState {
        self.server.borrow().state().clone()
    }

    fn player(&mut self, id: ParticipantId) -> &mut Player {
        self.players.iter_mut().find(|p| p.id == id).unwrap()
    }
}

struct Setup {
    competitors: usize,
    watchers: usize,
    attacks: bool,
    policy: RoomPolicy,
    latency: Option<Latency>,
    /// Each device's clock drift (millionths; alternately fast and slow),
    /// and whether its song clock is mapped once ([`Player::song_us`]).
    drift_ppm: i64,
    mapped_once: bool,
    slewed: bool,
}

impl Setup {
    fn new(competitors: usize) -> Setup {
        Setup {
            competitors,
            watchers: 0,
            drift_ppm: 0,
            mapped_once: false,
            slewed: false,
            attacks: false,
            policy: RoomPolicy::default().with_live(),
            latency: Some(Latency {
                down_ms: 20,
                down_jitter_ms: 120,
                up_ms: 20,
                up_jitter_ms: 150,
                seed: 1,
            }),
        }
    }
}

/// A room of `setup.competitors` people (the first its owner) and its
/// watchers, connected, each device's clock seconds off the room's.
fn room(setup: &Setup) -> Room {
    let clock = ManualClock::new(1_000_000);
    let server = FakeServer::hosted(
        ROOM,
        Identity::new("p:1"),
        "P1",
        setup.policy.clone(),
        clock.clone(),
    )
    .shared();
    let n = setup.competitors + setup.watchers;
    let mut players = Vec::new();
    for i in 1..=n {
        let who = Identity::new(format!("p:{i}"));
        let intent = if i <= setup.competitors {
            Intent::Play
        } else {
            Intent::Watch
        };
        let id = if i == 1 {
            ParticipantId(1)
        } else {
            server
                .borrow_mut()
                .join(who.clone(), &format!("P{i}"), intent)
                .unwrap()
        };
        if let Some(mut l) = setup.latency {
            l.seed = l.seed.wrapping_add(i as u64 * 7919);
            server.borrow_mut().set_latency(id, l);
        }
        let offset_ms = (i as i64 * 7_919 % 20_000) - 10_000;
        let clock = DriftClock {
            base: clock.clone(),
            offset_ms,
            ppm: if i % 2 == 0 {
                setup.drift_ppm
            } else {
                -setup.drift_ppm
            },
            origin: clock.now().0,
        };
        let device = clock.clone();
        let mut link =
            OnlineAuthority::new(RoomId::new(ROOM), FakeSocket::new(server.clone()), clock);
        link.live_mut().set_me(id);
        players.push(Player {
            who,
            id,
            client: ClientId::new(format!("p{i}")),
            seq: 0,
            link,
            server: server.clone(),
            play: None,
            sent: 0,
            reporter: None,
            judged: Vec::new(),
            judged_sent: 0,
            ended: false,
            gone_at: None,
            news: Vec::new(),
            mapped_once: setup.mapped_once,
            song_map: None,
            slew: setup.slewed.then(|| SongSlew::new(SLEW_V0)),
            clock: device,
        });
    }
    let mut r = Room {
        clock,
        server,
        players,
    };
    r.run(1_000);
    // A slow link's first exchanges take longer.
    for _ in 0..8 {
        if r.players.iter().all(|p| p.link.live().clock().is_some()) {
            break;
        }
        r.run(500);
    }
    for p in &r.players {
        assert!(p.link.live().is_on(), "{:?} plays live", p.id);
        assert!(p.link.live().clock().is_some(), "{:?} has the clock", p.id);
    }
    // The owner's rules: a seat each, attacks as asked.
    let mut rules = MatchRules::hosted();
    rules.seats = setup.competitors as u16;
    rules.set_attacks(setup.attacks);
    r.players[0].send(Command::SaveNextRules { rules });
    r.run(500);
    r.players[0].send(Command::ChooseContent {
        stage: STAGE,
        content: Some(content()),
    });
    r.run(500);
    r.server
        .borrow_mut()
        .authority(Command::SetContentStatus {
            stage: STAGE,
            status: ContentStatus::Available,
            share: Some("abc12345".into()),
        })
        .unwrap();
    r.run(500);
    r
}

/// Everyone prepared (with `digest` where the stage has a plan), loaded
/// and ready; the owner starts; the countdown runs to the song's zero.
fn start(r: &mut Room, digest: Option<PlanDigest>, competitors: usize) {
    let rev = r.state().stage(STAGE).unwrap().readiness_rev;
    for p in r.players.iter_mut().take(competitors) {
        p.link.live_mut().send(ClientLive::Prepared {
            stage: STAGE,
            horizon_us: 2_000_000,
            plan: digest.clone(),
        });
        p.send(Command::ReportLoading {
            stage: STAGE,
            readiness_rev: rev,
            loading: Loading::Loaded,
        });
        p.send(Command::SetReady {
            stage: STAGE,
            readiness_rev: rev,
            ready: true,
        });
    }
    r.run(1_000);
    r.players[0].send(Command::StartStage {
        stage: STAGE,
        readiness_rev: rev,
    });
    r.run(500);
}

fn started(r: &Room) {
    let s = r.state();
    assert!(
        matches!(s.phase, Phase::Countdown { .. } | Phase::Stage { .. }),
        "{:?}",
        s.phase
    );
}

fn give_plays(r: &mut Room, scripts: &[Script], checkpoints: Option<&[i64]>) {
    for (p, s) in r.players.iter_mut().zip(scripts) {
        p.play = Some(s.clone());
        p.judged = s.judged();
        p.reporter = checkpoints.map(|c| Reporter::new(p.id, c.to_vec()));
    }
}

#[test]
fn every_recipients_fields_end_equal_to_what_was_sent_through_jitter_loss_and_a_reconnect() {
    let mut setup = Setup::new(5);
    setup.watchers = 1;
    let mut r = room(&setup);
    start(&mut r, None, 5);
    started(&r);
    let scripts: Vec<Script> = (1..=5)
        .map(|i| script(i, 120 + i as i32 * 9, 24_000, None))
        .collect();
    give_plays(&mut r, &scripts, None);
    r.run(6_000);
    // Frames lost on the way to two recipients: a gap, a resume.
    for f in [
        Fault::Drop,
        Fault::Drop,
        Fault::Delay(700),
        Fault::Duplicate,
    ] {
        r.server.borrow_mut().fault(ParticipantId(3), f);
    }
    r.server.borrow_mut().fault(ParticipantId(6), Fault::Drop);
    // Batches lost on the way up: sent again until acknowledged.
    r.server
        .borrow_mut()
        .fault_uplink(ParticipantId(2), Fault::Drop);
    r.server
        .borrow_mut()
        .fault_uplink(ParticipantId(2), Fault::Drop);
    r.run(5_000);
    // A competitor's socket dropped mid-stage: it reconnects, sends what
    // was not acknowledged and resumes the frames.
    r.server.borrow_mut().drop_socket(ParticipantId(4));
    r.run(20_000);
    let s = r.state();
    assert!(
        matches!(s.phase, Phase::MatchResults | Phase::Intermission { .. }),
        "{:?}",
        s.phase
    );
    for p in &r.players {
        let reader = p.link.live().reader().expect("followed the stage");
        for (q, sc) in r.players.iter().zip(&scripts) {
            assert_eq!(
                reader.moments(q.id),
                &sc.moments[..],
                "{:?}'s field of {:?}",
                p.id,
                q.id
            );
            assert_eq!(reader.end(q.id), Some(StreamEnd::Finished));
        }
    }
    // The lost frames were asked for again.
    for id in [ParticipantId(3), ParticipantId(4), ParticipantId(6)] {
        let p = r.players.iter().find(|p| p.id == id).unwrap();
        assert!(p.link.live().reader().unwrap().last().unwrap() > 50);
    }
}

/// The ledger on one device (practice's way: each checkpoint run once
/// every competitor has been judged past it).
fn local_record(
    scripts: &[Script],
    plan: &BattlePlan,
    ids: &[ParticipantId],
) -> Vec<CheckpointRecord> {
    let roster = StageRoster {
        seats: ids
            .iter()
            .map(|&participant| RosterSeat {
                participant,
                targeting: Default::default(),
                lead_us: LIVE_V1.lead_us(2_000_000),
            })
            .collect(),
        seed: plan.seed,
    };
    let mut ledger = StageBattle::new(
        ATTACKS,
        POWER_CLASSES_V0,
        "ddi",
        plan.checkpoints_us.clone(),
        roster,
        plan.schedule.clone(),
    );
    let mut reporters: Vec<(Reporter, &Script)> = ids
        .iter()
        .zip(scripts)
        .map(|(&p, s)| {
            let mut r = Reporter::new(p, plan.checkpoints_us.clone());
            for j in s.judged() {
                r.judged(j);
            }
            if let Some(o) = s.out_ms {
                r.out_at(i64::from(o) * 1_000);
            }
            (r, s)
        })
        .collect();
    while let Some((k, at)) = ledger.next_checkpoint() {
        let mut reports = Vec::new();
        for (r, s) in &mut reporters {
            let to = at.min(i64::from(s.out_ms.unwrap_or(s.end_ms)) * 1_000);
            for (kk, rep) in r.due(to.max(at), &|t| s.life(t)) {
                assert_eq!(kk, k);
                reports.push(rep);
            }
        }
        ledger.checkpoint(k, &reports);
    }
    ledger.record().to_vec()
}

fn attack_room(scripts: &[Script], plan: &BattlePlan, latency: Option<Latency>) -> Room {
    let mut setup = Setup::new(scripts.len());
    setup.attacks = true;
    setup.latency = latency;
    let mut r = room(&setup);
    r.players[0].link.live_mut().send(ClientLive::OfferPlan {
        stage: STAGE,
        plan: Box::new(plan.clone()),
    });
    r.run(500);
    let s = r.state();
    assert_eq!(
        s.stage(STAGE).unwrap().content.as_ref().unwrap().battle,
        Some(plan.digest.clone()),
        "the room keeps the plan's digest"
    );
    for p in &r.players {
        assert_eq!(
            p.room()
                .stage(STAGE)
                .unwrap()
                .content
                .as_ref()
                .unwrap()
                .battle,
            Some(plan.digest.clone())
        );
    }
    start(&mut r, Some(plan.digest.clone()), scripts.len());
    started(&r);
    give_plays(&mut r, scripts, Some(&plan.checkpoints_us));
    r
}

#[test]
fn one_attack_trace_gives_the_same_ledger_locally_and_through_the_room() {
    let plan = plan(30, 99, |j| j % 4 != 1);
    // Four competitors, dense enough to fill bars every few seconds; the
    // reports reach the room in whatever order the jitter makes.
    let scripts: Vec<Script> = (1..=4)
        .map(|i| script(10 + i, 95 + i as i32 * 7, 56_000, None))
        .collect();
    let ids: Vec<ParticipantId> = (1..=4).map(ParticipantId).collect();
    let local = local_record(&scripts, &plan, &ids);
    let launched = local
        .iter()
        .flat_map(|c| &c.events)
        .filter(|e| matches!(e, AttackEvent::Committed { .. }))
        .count();
    assert!(launched >= 4, "the trace launches attacks: {launched}");
    let mut r = attack_room(&scripts, &plan, Setup::new(4).latency);
    r.run(66_000);
    let server = r.server.borrow();
    let stage = server
        .live()
        .unwrap()
        .stage_or_last()
        .expect("the stage played");
    let room = stage.ledger().expect("a ledger").record().to_vec();
    assert_eq!(room.len(), local.len(), "every checkpoint ran");
    assert_eq!(room, local, "the same reports, the same ledger");
    drop(server);
    // Every device read the same checkpoints and the commitments.
    for p in &r.players {
        let reader = p.link.live().reader().unwrap();
        let read: Vec<CheckpointRecord> = reader
            .checkpoints()
            .iter()
            .map(|c| c.record.clone())
            .collect();
        assert_eq!(read, local, "{:?}", p.id);
        assert_eq!(reader.commitments().len(), launched);
        for c in reader.commitments() {
            let cells = &plan.schedule.slots[c.slot as usize]
                .section
                .as_ref()
                .unwrap()
                .cells;
            assert_eq!(cells.len(), 8, "every device places the same eight shocks");
        }
    }
}

#[test]
fn a_late_report_counts_at_the_next_checkpoint_and_never_stalls_the_room() {
    let plan = plan(30, 99, |j| j % 4 != 1);
    let scripts: Vec<Script> = (1..=4)
        .map(|i| script(10 + i, 95 + i as i32 * 7, 56_000, None))
        .collect();
    let ids: Vec<ParticipantId> = (1..=4).map(ParticipantId).collect();
    let local = local_record(&scripts, &plan, &ids);
    let mut r = attack_room(&scripts, &plan, None);
    // Around song second 20, P3's uplink stalls for 2.5 s: its reports for
    // a checkpoint or two come after their deadlines.
    let zero = r
        .state()
        .stage(STAGE)
        .unwrap()
        .play
        .as_ref()
        .unwrap()
        .song_zero_at
        .unwrap();
    let until = zero.0 + 20_000;
    r.run(until - r.now());
    r.server
        .borrow_mut()
        .fault_uplink(ParticipantId(3), Fault::Delay(2_500));
    // The others are not held up: checkpoint 10 (song 20 s) runs by its
    // deadline though P3's report has not come.
    r.run(1_200);
    {
        let server = r.server.borrow();
        let ledger = server.live().unwrap().stage().unwrap().ledger().unwrap();
        assert!(ledger.record().len() >= 10, "{}", ledger.record().len());
    }
    r.run(45_000);
    let server = r.server.borrow();
    let room = server
        .live()
        .unwrap()
        .stage_or_last()
        .unwrap()
        .ledger()
        .unwrap()
        .record()
        .to_vec();
    assert_eq!(room.len(), local.len(), "every checkpoint ran");
    // The same up to the first late one; there P3's power is short of the
    // local count (its judgments counted at the next instead), and the
    // power earned in all is the same once it caught up.
    let first = room
        .iter()
        .zip(&local)
        .position(|(a, b)| a != b)
        .expect("a late report changes the record");
    assert!(first >= 9, "{first}");
    let p3 = |c: &CheckpointRecord| {
        c.power
            .iter()
            .find(|(p, _)| *p == ParticipantId(3))
            .unwrap()
            .1
    };
    assert!(
        p3(&room[first]) < p3(&local[first]),
        "{:?} {:?}",
        room[first],
        local[first]
    );
    let launched = |rec: &[CheckpointRecord]| {
        rec.iter()
            .flat_map(|c| &c.events)
            .filter(|e| matches!(e, AttackEvent::Committed { .. }))
            .count()
    };
    eprintln!(
        "late reports: first difference at checkpoint {first}; launches {} locally, {} through the room",
        launched(&local),
        launched(&room)
    );
}

#[test]
fn a_plan_is_checked_by_its_digest_and_a_device_that_computed_another_sits_out() {
    let mut setup = Setup::new(3);
    setup.attacks = true;
    let mut r = room(&setup);
    let good = plan(20, 5, |j| j % 2 == 0);
    // Only the picker (or an operator) offers it.
    r.players[1].link.live_mut().send(ClientLive::OfferPlan {
        stage: STAGE,
        plan: Box::new(good.clone()),
    });
    // A plan whose digest is not its own.
    let mut forged = good.clone();
    forged.seed += 1;
    r.players[0].link.live_mut().send(ClientLive::OfferPlan {
        stage: STAGE,
        plan: Box::new(forged),
    });
    r.run(500);
    assert!(
        r.players[1]
            .news
            .iter()
            .any(|n| matches!(n, LiveNews::Refused(LiveRefusal::NotPicker { .. })))
    );
    assert!(
        r.players[0]
            .news
            .iter()
            .any(|n| matches!(n, LiveNews::Refused(LiveRefusal::BadPlan { .. })))
    );
    assert_eq!(
        r.state()
            .stage(STAGE)
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .battle,
        None
    );
    r.players[0].link.live_mut().send(ClientLive::OfferPlan {
        stage: STAGE,
        plan: Box::new(good.clone()),
    });
    r.run(500);
    // Another plan after it is refused; the room keeps the first.
    let other = plan(20, 6, |j| j % 2 == 0);
    r.players[0].link.live_mut().send(ClientLive::OfferPlan {
        stage: STAGE,
        plan: Box::new(other.clone()),
    });
    // P3 computed another from the same chart: a content mismatch.
    r.players[2].link.live_mut().send(ClientLive::Prepared {
        stage: STAGE,
        horizon_us: 2_000_000,
        plan: Some(other.digest.clone()),
    });
    r.run(500);
    assert!(r.players[0].news.iter().any(|n| matches!(
        n,
        LiveNews::Refused(LiveRefusal::PlanSet { digest, .. }) if *digest == good.digest
    )));
    assert!(r.players[2].news.iter().any(|n| matches!(
        n,
        LiveNews::Refused(LiveRefusal::PlanMismatch { room, .. }) if *room == good.digest
    )));
    assert_eq!(
        r.state()
            .stage(STAGE)
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .battle,
        Some(good.digest.clone())
    );
    // P3 sits the stage out (its own device's rule: a failed load), the
    // others start it with the plan.
    let rev = r.state().stage(STAGE).unwrap().readiness_rev;
    r.players[2].send(Command::ReportLoading {
        stage: STAGE,
        readiness_rev: rev,
        loading: Loading::Failed {
            code: "battle_plan_mismatch".into(),
        },
    });
    let mut two = std::mem::take(&mut r.players);
    let rest = two.split_off(2);
    r.players = two;
    start(&mut r, Some(good.digest.clone()), 2);
    r.players.extend(rest);
    // P3 could still load: the readiness window waits for it, then the
    // stage starts without it.
    r.run(31_000);
    let s = r.state();
    let play = s.stage(STAGE).unwrap().play.as_ref().unwrap();
    assert!(!play.lineup.contains(&ParticipantId(3)));
    let server = r.server.borrow();
    let stage = server.live().unwrap().stage().unwrap();
    assert!(stage.plays_live(ParticipantId(1)) && stage.plays_live(ParticipantId(2)));
    assert!(stage.ledger().is_some());
}

#[test]
fn the_room_decides_a_survival_stage_as_it_goes_and_the_winner_finishes() {
    let mut r = room(&Setup::new(3));
    start(&mut r, None, 3);
    started(&r);
    let scripts = vec![
        script(1, 150, 30_000, None),
        script(2, 150, 30_000, Some(9_000)),
        script(3, 150, 30_000, Some(14_000)),
    ];
    give_plays(&mut r, &scripts, None);
    r.run(4_000 + 14_000 + 1_500);
    // Decided by the room once P3 went out: P1 first, P3, then P2.
    let s = r.state();
    assert_eq!(s.phase, Phase::SongCompletion { stage: STAGE });
    let result = s
        .stage(STAGE)
        .unwrap()
        .play
        .as_ref()
        .unwrap()
        .result
        .clone()
        .unwrap();
    assert_eq!(result.placement(ParticipantId(1)).unwrap().place, 1);
    assert_eq!(result.placement(ParticipantId(3)).unwrap().place, 2);
    assert_eq!(result.placement(ParticipantId(2)).unwrap().place, 3);
    assert_eq!(
        result.placement(ParticipantId(2)).unwrap().finish,
        Finish::Eliminated
    );
    // The winner finishes the song; its report is kept for the standings
    // and the song ends.
    r.run(18_000);
    let s = r.state();
    assert!(
        matches!(s.phase, Phase::MatchResults | Phase::Intermission { .. }),
        "{:?}",
        s.phase
    );
    let play = s.stage(STAGE).unwrap().play.as_ref().unwrap();
    assert!(play.song_ended);
    assert_eq!(play.reports.len(), 3);
    assert_eq!(
        play.result.as_ref().unwrap(),
        &result,
        "nothing after changed it"
    );
}

#[test]
fn a_connection_lost_past_the_grace_is_a_forfeit_and_does_not_hold_the_stage_up() {
    let mut r = room(&Setup::new(3));
    r.server.borrow_mut().set_grace(Some(3_000));
    start(&mut r, None, 3);
    started(&r);
    let scripts = vec![
        script(1, 150, 30_000, None),
        script(2, 150, 30_000, Some(10_000)),
        script(3, 150, 30_000, None),
    ];
    give_plays(&mut r, &scripts, None);
    r.run(6_000);
    // P3's device goes away for good (its socket's end seen by the room,
    // no new one).
    r.server.borrow_mut().drop_socket(ParticipantId(3));
    r.run(100);
    let now = r.now();
    r.player(ParticipantId(3)).gone_at = Some(now);
    r.run(10_000);
    let s = r.state();
    assert_eq!(
        s.phase,
        Phase::SongCompletion { stage: STAGE },
        "decided early"
    );
    let result = s
        .stage(STAGE)
        .unwrap()
        .play
        .as_ref()
        .unwrap()
        .result
        .clone()
        .unwrap();
    assert_eq!(result.placement(ParticipantId(1)).unwrap().place, 1);
    assert_eq!(result.placement(ParticipantId(2)).unwrap().place, 2);
    assert_eq!(
        result.placement(ParticipantId(3)).unwrap().finish,
        Finish::Forfeit
    );
    {
        let server = r.server.borrow();
        let stage = server.live().unwrap().stage().unwrap();
        assert_eq!(stage.end(ParticipantId(3)), Some(StreamEnd::Dropped));
    }
    // The song ends once the others reported, P3's report never coming.
    r.run(20_000);
    let s = r.state();
    assert!(
        matches!(s.phase, Phase::MatchResults | Phase::Intermission { .. }),
        "{:?}",
        s.phase
    );
    // The others' fields of P3 end where its stream did.
    let reader = r.players[0].link.live().reader().unwrap();
    assert_eq!(reader.end(ParticipantId(3)), Some(StreamEnd::Dropped));
}

#[test]
fn a_stage_everyone_misses_is_decided_and_its_song_ended_in_the_same_tick() {
    // Everyone said they would play live and nobody streamed: the room ends
    // every stream as a no-show in one tick, decides the stage and ends its
    // song at once (the song's end after the decision takes no ranking).
    let mut r = room(&Setup::new(2));
    start(&mut r, None, 2);
    started(&r);
    let zero = r
        .state()
        .stage(STAGE)
        .unwrap()
        .play
        .as_ref()
        .unwrap()
        .song_zero_at
        .unwrap();
    r.run(zero.0 + 5_750 + 1_000 - r.now());
    let s = r.state();
    assert!(
        matches!(s.phase, Phase::MatchResults | Phase::Intermission { .. }),
        "the song ended at once, not at the report deadline: {:?}",
        s.phase
    );
    let play = s.stage(STAGE).unwrap().play.as_ref().unwrap();
    assert!(play.song_ended);
    let result = play.result.as_ref().unwrap();
    for p in [ParticipantId(1), ParticipantId(2)] {
        assert_eq!(result.placement(p).unwrap().finish, Finish::Forfeit);
    }
}

#[test]
fn a_final_attack_holds_the_decision_and_its_knockout_is_a_shared_first() {
    let plan = plan(30, 3, |_| true);
    let ids: Vec<ParticipantId> = (1..=3).map(ParticipantId).collect();
    // P1 and P2 strong (their bars fill), P3 weak; find the first attack
    // against P1 in the local ledger, then script P2 and P3 out before it
    // lands and P1 knocked out by it.
    let base = vec![
        script(21, 90, 56_000, None),
        script(22, 90, 56_000, None),
        script(23, 400, 56_000, None),
    ];
    let local = local_record(&base, &plan, &ids);
    let ledger_attack = local
        .iter()
        .enumerate()
        .flat_map(|(k, c)| c.events.iter().map(move |e| (k, e)))
        .find_map(|(k, e)| match e {
            AttackEvent::Committed {
                recipient, attack, ..
            } if *recipient == ParticipantId(1) => Some((k, *attack)),
            _ => None,
        })
        .expect("an attack against P1");
    let (k, attack) = ledger_attack;
    let committed_us = plan.checkpoints_us[k];
    // Its section, as the room's ledger will have it (the same trace up to
    // its commitment).
    let mut local_battle = StageBattle::new(
        ATTACKS,
        POWER_CLASSES_V0,
        "ddi",
        plan.checkpoints_us.clone(),
        StageRoster {
            seats: ids
                .iter()
                .map(|&participant| RosterSeat {
                    participant,
                    targeting: Default::default(),
                    lead_us: LIVE_V1.lead_us(2_000_000),
                })
                .collect(),
            seed: plan.seed,
        },
        plan.schedule.clone(),
    );
    let mut reps: Vec<Reporter> = ids
        .iter()
        .zip(&base)
        .map(|(&p, s)| {
            let mut r = Reporter::new(p, plan.checkpoints_us.clone());
            for j in s.judged() {
                r.judged(j);
            }
            r
        })
        .collect();
    for (kk, &at) in plan.checkpoints_us[..=k].iter().enumerate() {
        let reports: Vec<_> = reps
            .iter_mut()
            .zip(&base)
            .flat_map(|(r, s)| r.due(at, &|t| s.life(t)))
            .map(|(_, r)| r)
            .collect();
        local_battle.checkpoint(kk, &reports);
    }
    let a = local_battle.attack(attack).unwrap().clone();
    let first_shock_ms = (a.cells[0].at_us / 1_000) as i32;
    // P3 goes out just after the commitment, P2 a checkpoint interval
    // later (before the attack's first shock: its section starts a lead
    // after the commitment), P1 on the attack's first shock (the final
    // attack's knockout).
    let out3 = (committed_us / 1_000) as i32 + 300;
    let out2 = (committed_us / 1_000) as i32 + 2_300;
    assert!(i64::from(out2) * 1_000 + 1_000_000 < a.cells[0].at_us);
    let scripts = vec![
        script(21, 90, 56_000, Some(first_shock_ms)),
        script(22, 90, 56_000, Some(out2)),
        script(23, 400, 56_000, Some(out3)),
    ];
    let mut r = attack_room(&scripts, &plan, Setup::new(3).latency);
    let zero = r
        .state()
        .stage(STAGE)
        .unwrap()
        .play
        .as_ref()
        .unwrap()
        .song_zero_at
        .unwrap();
    // P1 alone in, the attack against it still to come: the stage waits.
    let wait_until = zero.0 + (out2 as u64) + 1_000;
    r.run(wait_until - r.now());
    let s = r.state();
    assert_eq!(
        s.phase,
        Phase::Stage { stage: STAGE },
        "waits for the final attack"
    );
    {
        let server = r.server.borrow();
        let reader = r.players[0].link.live().reader().unwrap();
        assert!(reader.commitments().iter().any(|c| c.attack == attack));
        let snap = server
            .live()
            .unwrap()
            .stage()
            .unwrap()
            .snapshot(1, Millis(0));
        assert_eq!(snap.settling.map(|s| s.survivor), Some(ParticipantId(1)));
    }
    r.run(20_000);
    let s = r.state();
    let result = s
        .stage(STAGE)
        .unwrap()
        .play
        .as_ref()
        .unwrap()
        .result
        .clone()
        .unwrap();
    // The knockout shares first with the last one out before it.
    assert_eq!(result.placement(ParticipantId(1)).unwrap().place, 1);
    assert_eq!(result.placement(ParticipantId(2)).unwrap().place, 1);
    assert_eq!(result.placement(ParticipantId(3)).unwrap().place, 3);
}

#[test]
fn a_room_that_does_not_play_live_is_never_sent_a_live_frame() {
    let mut setup = Setup::new(2);
    setup.policy = RoomPolicy::default();
    setup.latency = None;
    let clock = ManualClock::new(1_000_000);
    let server = FakeServer::hosted(
        ROOM,
        Identity::new("p:1"),
        "P1",
        setup.policy,
        clock.clone(),
    )
    .shared();
    let mut link = OnlineAuthority::new(
        RoomId::new(ROOM),
        FakeSocket::new(server.clone()),
        clock.clone(),
    );
    for _ in 0..4 {
        if link.wants_ticket() {
            link.ticket_asked();
            let t = server.borrow_mut().ticket(&Identity::new("p:1"));
            link.ticket(t.map_err(|_| TicketProblem::NotAMember));
        }
        clock.advance(100);
        let _ = link.poll();
    }
    assert!(!link.live().is_on());
    link.live_mut().send(ClientLive::Hello { version: 1 });
    clock.advance(5_000);
    let _ = link.poll();
    assert_eq!(server.borrow().sockets(), 1, "nothing live was sent");
    // Had it been, the room (as an authority that predates live play)
    // would have closed the socket.
    let mut raw = FakeSocket::new(server.clone());
    let t = server.borrow_mut().ticket(&Identity::new("p:1")).unwrap();
    use ddi_room::Transport;
    raw.open(&RoomId::new(ROOM), &t);
    raw.send(r#"{"v":2,"live":{"kind":"hello","version":1}}"#.into());
    let events = raw.poll();
    assert!(events.contains(&ddi_room::TransportEvent::Closed { code: 4400 }));
}

#[test]
fn seventy_three_competitors_stream_through_the_fake_at_speed() {
    // The room's own cap and member bound (2026-11-13): 73 play, and
    // members beyond them watch.
    let mut setup = Setup::new(73);
    setup.attacks = true;
    setup.watchers = 12;
    setup.policy = RoomPolicy::default().with_live();
    let plan = plan(16, 7, |j| j % 3 != 2);
    let scripts: Vec<Script> = (1..=73u64)
        .map(|i| {
            // About 8 notes a second; a few go out on the way.
            let out = (i % 11 == 0).then_some(8_000 + i as i32 * 200);
            script(100 + i, 110 + (i % 5) as i32 * 10, 30_000, out)
        })
        .collect();
    let begun = Instant::now();
    let mut r = {
        let mut r = room(&setup);
        r.players[0].link.live_mut().send(ClientLive::OfferPlan {
            stage: STAGE,
            plan: Box::new(plan.clone()),
        });
        r.run(500);
        start(&mut r, Some(plan.digest.clone()), 73);
        started(&r);
        give_plays(&mut r, &scripts, Some(&plan.checkpoints_us));
        r
    };
    r.run(40_000);
    let elapsed = begun.elapsed();
    let s = r.state();
    assert!(
        matches!(s.phase, Phase::MatchResults | Phase::Intermission { .. }),
        "{:?}",
        s.phase
    );
    // Every recipient's fields equal what was sent (two recipients
    // checked in full, everyone's count).
    for p in r.players.iter().step_by(36) {
        let reader = p.link.live().reader().unwrap();
        for (q, sc) in r.players.iter().zip(&scripts) {
            assert_eq!(reader.moments(q.id), &sc.moments[..]);
        }
    }
    // Settled by the top-biased table at the default Strong bias: 73
    // places, 12 for first alone, 1 for the last, everyone placed.
    let result = s
        .current
        .as_ref()
        .or(s.history.last().map(|h| &h.played))
        .and_then(|m| m.stage(STAGE))
        .and_then(|st| st.play.as_ref())
        .and_then(|p| p.result.as_ref())
        .expect("a confirmed result");
    assert_eq!(result.rule, ddi_room::AwardRule::TopBiasedStrongV1);
    assert_eq!(result.placements.len(), 73);
    // Watchers saw every competitor's fields too.
    let watcher = r.players.last().unwrap();
    let reader = watcher.link.live().reader().unwrap();
    for (q, sc) in r.players.iter().zip(&scripts) {
        assert_eq!(reader.moments(q.id), &sc.moments[..]);
    }
    let server = r.server.borrow();
    let stage = server.live().unwrap().stage_or_last();
    eprintln!(
        "73 competitors, 40 s of room time through the fake: {:.1} s of test time, {} frames",
        elapsed.as_secs_f64(),
        stage.map_or(0, |s| s.frames())
    );
}

/// What a tick costs the room with 73 competitors: their batches read
/// (decoded, taken), the ledger's checkpoints run, the frame built and
/// encoded once. Printed for the plan (`--nocapture`); debug builds are
/// several times slower than the server's release build.
#[test]
fn what_a_tick_costs_with_seventy_three_competitors() {
    let n = 73usize;
    let mut setup = Setup::new(n);
    setup.attacks = true;
    setup.latency = None;
    setup.policy = RoomPolicy::default().with_live();
    let plan = plan(40, 7, |j| j % 3 != 2);
    let mut r = room(&setup);
    r.players[0].link.live_mut().send(ClientLive::OfferPlan {
        stage: STAGE,
        plan: Box::new(plan.clone()),
    });
    r.run(500);
    start(&mut r, Some(plan.digest.clone()), n);
    started(&r);
    r.run(4_200);
    let state = r.state();
    assert_eq!(state.phase, Phase::Stage { stage: STAGE });
    let zero = state
        .stage(STAGE)
        .unwrap()
        .play
        .as_ref()
        .unwrap()
        .song_zero_at
        .unwrap();
    // A room's live side of its own on the same state, fed directly, as
    // the host restores it from its storage.
    let mut durable = LiveDurable::default();
    durable.plans.insert(STAGE, plan.clone());
    durable.prepared.insert(
        STAGE,
        (1..=n as u64)
            .map(|i| {
                (
                    ParticipantId(i),
                    Prepared {
                        horizon_us: 2_000_000,
                        plan: Some(plan.digest.clone()),
                    },
                )
            })
            .collect(),
    );
    let mut live = LiveServer::restore(LIVE_V1, durable);
    let _ = live.tick(&state, zero);
    assert!(live.stage().unwrap().ledger().is_some());
    let scripts: Vec<Script> = (1..=n as u64)
        .map(|i| script(200 + i, 83, 60_000, None))
        .collect();
    let mut reporters: Vec<Reporter> = (1..=n as u64)
        .map(|i| Reporter::new(ParticipantId(i), plan.checkpoints_us.clone()))
        .collect();
    let judged: Vec<Vec<Judged>> = scripts.iter().map(Script::judged).collect();
    let mut cursor = vec![(0usize, 0usize); n];
    let mut seq = vec![0u32; n];
    let (mut read_ns, mut tick_ns, mut frame_bytes, mut batch_bytes, mut ticks) =
        (0u128, 0u128, 0usize, 0usize, 0u32);
    let mut max_tick_ns = 0u128;
    for t in 1..=240u64 {
        let now = Millis(zero.0 + t * 250);
        let song_us = t as i64 * 250_000;
        for i in 0..n {
            let s = &scripts[i];
            let (sent, js) = &mut cursor[i];
            let mut moments = Vec::new();
            while let Some(m) = s.moments.get(*sent) {
                if i64::from(m.at_ms) * 1_000 > song_us {
                    break;
                }
                moments.push(*m);
                *sent += 1;
            }
            while let Some(j) = judged[i].get(*js) {
                if j.at_us > song_us {
                    break;
                }
                reporters[i].judged(*j);
                *js += 1;
            }
            let reports = reporters[i]
                .due(song_us, &|at| s.life(at))
                .into_iter()
                .map(|(k, rep)| LiveReport::from_report(k, rep))
                .collect();
            seq[i] += 1;
            let bytes = wire::encode_batch(&ddi_room::live::Batch {
                stage: STAGE,
                seq: seq[i],
                through_us: song_us,
                moments,
                reports,
                end: None,
            });
            batch_bytes += bytes.len();
            let begun = Instant::now();
            let out = live.receive_binary(&state, ParticipantId(i as u64 + 1), &bytes, now);
            read_ns += begun.elapsed().as_nanos();
            assert!(out.reply.is_empty(), "{:?}", out.reply);
        }
        let begun = Instant::now();
        let out = live.tick(&state, now);
        let took = begun.elapsed().as_nanos();
        tick_ns += took;
        max_tick_ns = max_tick_ns.max(took);
        ticks += 1;
        frame_bytes += out.broadcast.iter().map(Vec::len).sum::<usize>();
    }
    let stage = live.stage().unwrap();
    let ledger = stage.ledger().unwrap();
    let launched = ledger.attacks().len();
    eprintln!(
        "73 competitors at 12 notes/s for 60 s: a batch {:.0} B; reading 73 batches {:.0} µs a tick; \
         the tick (checkpoints, frame) {:.0} µs on average, {:.0} µs at most; a frame {:.0} B \
         ({:.1} KB/s to each socket); {} checkpoints, {} attacks",
        batch_bytes as f64 / (ticks as f64 * n as f64),
        read_ns as f64 / ticks as f64 / 1e3,
        tick_ns as f64 / ticks as f64 / 1e3,
        max_tick_ns as f64 / 1e3,
        frame_bytes as f64 / ticks as f64,
        frame_bytes as f64 / 60.0 / 1e3,
        ledger.record().len(),
        launched,
    );
    assert!(ledger.record().len() >= 29);
    assert!(launched > 0);
}

/// The stream's sizes for the plan (`--nocapture`): what one competitor
/// sends a second at 4 and 12 notes a second, as JSON and binary, and a
/// frame for 9, 33 and 73 competitors at 12 notes a second.
#[test]
fn the_streams_sizes() {
    use ddi_room::live::stream::{Batch, Frame, FrameEntry};
    let checkpoints: Vec<i64> = (1..=40).map(|k| k * 1_600_000).collect();
    let per_second = |gap_ms: i32| {
        let s = script(5, gap_ms, 61_000, None);
        let mut rep = Reporter::new(ParticipantId(1), checkpoints.clone());
        let (mut json, mut bin, mut batches) = (0usize, 0usize, 0usize);
        let mut sent = 0;
        for (seq, t) in (1..=240).enumerate() {
            let song_us = t as i64 * 250_000;
            let mut moments = Vec::new();
            while let Some(m) = s.moments.get(sent) {
                if i64::from(m.at_ms) * 1_000 > song_us {
                    break;
                }
                moments.push(*m);
                if let MomentKind::Tap { tier, .. } = m.kind {
                    rep.judged(Judged {
                        at_us: i64::from(m.at_ms) * 1_000,
                        tier,
                    });
                }
                sent += 1;
            }
            let reports = rep
                .due(song_us, &|at| s.life(at))
                .into_iter()
                .map(|(k, r)| LiveReport::from_report(k, r))
                .collect();
            let b = Batch {
                stage: STAGE,
                seq: seq as u32 + 1,
                through_us: song_us,
                moments,
                reports,
                end: None,
            };
            // Its JSON as a live message would carry it.
            json += serde_json::to_string(&b).unwrap().len()
                + r#"{"v":2,"live":{"kind":"batch",}}"#.len();
            bin += wire::encode_batch(&b).len();
            batches += 1;
        }
        (json / 60, bin / 60, batches / 60)
    };
    for (rate, gap) in [(4, 250), (12, 83)] {
        let (json, bin, n) = per_second(gap);
        eprintln!("{rate} notes/s: {n} batches/s, JSON {json} B/s, binary {bin} B/s");
    }
    for n in [9u64, 33, 73] {
        let frame = Frame {
            stage: STAGE,
            n: 1234,
            at: Millis(1_731_400_000_000),
            entries: (1..=n)
                .map(|p| {
                    let s = script(p, 83, 40_000, None);
                    FrameEntry {
                        participant: ParticipantId(p),
                        seq: 120,
                        through_us: 30_000_000,
                        moments: s.moments[300..303].to_vec(),
                        end: None,
                    }
                })
                .collect(),
            commitments: Vec::new(),
            checkpoints: Vec::new(),
            settling: None,
        };
        let json = serde_json::to_string(&frame).unwrap().len();
        let bin = wire::encode_frame(&frame).len();
        eprintln!("frame of {n} at 12 notes/s: JSON {json} B, binary {bin} B");
        assert!(bin * 5 < json);
    }
}

#[test]
fn someone_who_comes_in_mid_stage_gets_the_stage_as_it_stands_and_follows_it() {
    let mut r = room(&Setup::new(2));
    start(&mut r, None, 2);
    started(&r);
    let scripts = vec![script(1, 140, 30_000, None), script(2, 140, 30_000, None)];
    give_plays(&mut r, &scripts, None);
    r.run(14_000);
    // A member enters to watch while the song plays.
    let who = Identity::new("p:9");
    let id = r
        .server
        .borrow_mut()
        .join(who.clone(), "P9", Intent::Watch)
        .unwrap();
    let clock = DriftClock {
        base: r.clock.clone(),
        offset_ms: 4_321,
        ppm: 0,
        origin: 0,
    };
    let watcher_clock = clock.clone();
    let mut link =
        OnlineAuthority::new(RoomId::new(ROOM), FakeSocket::new(r.server.clone()), clock);
    link.live_mut().set_me(id);
    r.players.push(Player {
        who,
        id,
        client: ClientId::new("p9"),
        seq: 0,
        link,
        server: r.server.clone(),
        play: None,
        sent: 0,
        reporter: None,
        judged: Vec::new(),
        judged_sent: 0,
        ended: false,
        gone_at: None,
        news: Vec::new(),
        mapped_once: false,
        song_map: None,
        slew: None,
        clock: watcher_clock,
    });
    r.run(20_000);
    let watcher = r.players.last().unwrap();
    assert!(
        watcher
            .news
            .iter()
            .any(|n| matches!(n, LiveNews::Snapshot(s) if s.competitors.len() == 2)),
        "the stage as it stood"
    );
    let reader = watcher.link.live().reader().unwrap();
    for (q, sc) in r.players.iter().zip(&scripts) {
        let seen = reader.moments(q.id);
        // The last few seconds before it came, then everything.
        assert!(seen.len() > sc.moments.len() / 2, "{}", seen.len());
        assert_eq!(seen, &sc.moments[sc.moments.len() - seen.len()..]);
        assert_eq!(reader.end(q.id), Some(StreamEnd::Finished));
    }
}

/// One device's view of the others, as a play's would be
/// (`ddi_room::live::delay`'s `ViewMeter`), fed as its device reads the
/// frames.
struct View {
    meter: Option<ViewMeter>,
    read: usize,
}

impl View {
    fn new() -> View {
        View {
            meter: None,
            read: 0,
        }
    }

    /// One step of `p`'s device (`fixed`: `live-v1`'s constant second);
    /// measured from song ms `from` on.
    fn step(&mut self, p: &mut Player, fixed: bool, from: f64) {
        let Some(now) = p.song_us() else {
            return;
        };
        let now = now as f64 / 1_000.0;
        if now < -1_000.0 {
            return;
        }
        let rules = p.link.live().rules();
        let meter = self.meter.get_or_insert_with(|| {
            let delay = if fixed {
                ViewDelay::fixed(rules.presentation_delay_ms as f64)
            } else {
                let u = p
                    .link
                    .live()
                    .clock()
                    .map_or(100.0, |c| c.uncertainty_us as f64 / 1_000.0);
                let ceiling = rules.presentation_delay_ms as f64;
                ViewDelay::new(
                    VIEW_DELAY_V0,
                    ceiling,
                    VIEW_DELAY_V0.initial_ms(&rules, u, ceiling),
                )
            };
            ViewMeter::new(delay, Some(p.id))
        });
        for n in &p.news[self.read..] {
            if let LiveNews::Frame(f) = n {
                meter.frame(f, now);
            }
        }
        self.read = p.news.len();
        meter.step(now, from);
    }
}

/// What the views of one stage measured: the delay drawn with (mean, 95th
/// percentile, most, ms), the share of field-steps stalled and the stalls
/// begun a minute a field.
#[derive(Debug)]
struct Measured {
    mean: f64,
    p95: f64,
    max: f64,
    stalled: f64,
    onsets_per_min: f64,
    frames_per_s: f64,
}

/// A stage of `competitors` and a watcher at `latency` (each way: floor
/// and jitter, ms) under `policy`, every device's views measured from the
/// song's eighth second to its 38th (`fixed`: `live-v1`'s constant
/// second).
fn measure_views(
    competitors: usize,
    latency: (u64, u64),
    policy: RoomPolicy,
    fixed: bool,
    seed: u64,
) -> Measured {
    measure_drifting(competitors, latency, policy, fixed, seed, Drift::NONE)
}

/// How the devices' clocks run in a measured stage: their drift (each
/// alternately fast and slow), whether a device's song clock is mapped
/// from the room's once (as a play's audio clock is) or read from the
/// room's clock as estimated every time, the song's length and where the
/// measuring starts (song ms).
#[derive(Clone, Copy, Debug)]
struct Drift {
    ppm: i64,
    mapped_once: bool,
    /// Mapped once and kept in step ([`Player::slew`]).
    slewed: bool,
    song_ms: i32,
    from: f64,
}

impl Drift {
    const NONE: Drift = Drift {
        ppm: 0,
        mapped_once: false,
        slewed: false,
        song_ms: 40_000,
        from: 8_000.0,
    };
}

fn measure_drifting(
    competitors: usize,
    latency: (u64, u64),
    policy: RoomPolicy,
    fixed: bool,
    seed: u64,
    drift: Drift,
) -> Measured {
    let mut setup = Setup::new(competitors);
    setup.watchers = 1;
    setup.policy = policy;
    setup.drift_ppm = drift.ppm;
    setup.mapped_once = drift.mapped_once || drift.slewed;
    setup.slewed = drift.slewed;
    let from = drift.from;
    setup.latency = Some(Latency {
        down_ms: latency.0,
        down_jitter_ms: latency.1,
        up_ms: latency.0,
        up_jitter_ms: latency.1,
        seed,
    });
    let mut r = room(&setup);
    start(&mut r, None, competitors);
    started(&r);
    let scripts: Vec<Script> = (1..=competitors as u64)
        .map(|i| script(seed * 31 + i, 90 + (i % 7) as i32 * 10, drift.song_ms, None))
        .collect();
    give_plays(&mut r, &scripts, None);
    let mut views: Vec<View> = r.players.iter().map(|_| View::new()).collect();
    let end = r.now() + drift.song_ms as u64;
    while r.now() < end {
        r.clock.advance(STEP_MS);
        r.pump();
        for (v, p) in views.iter_mut().zip(r.players.iter_mut()) {
            v.step(p, fixed, from);
        }
    }
    let meters: Vec<&ViewMeter> = views.iter().filter_map(|v| v.meter.as_ref()).collect();
    let mut all: Vec<f64> = meters.iter().flat_map(|m| m.delays.iter().copied()).collect();
    all.sort_by(f64::total_cmp);
    let field_ms: f64 = meters.iter().map(|m| m.field_ms).sum();
    let stalled_ms: f64 = meters.iter().map(|m| m.stalled_ms).sum();
    let onsets: u32 = meters.iter().map(|m| m.onsets).sum();
    let field_minutes = field_ms / 60_000.0;
    let frames = r
        .server
        .borrow()
        .live()
        .and_then(|l| l.stage_or_last().map(|s| s.frames()))
        .unwrap_or(0);
    Measured {
        mean: all.iter().sum::<f64>() / all.len().max(1) as f64,
        p95: all.get(all.len() * 95 / 100).copied().unwrap_or(0.0),
        max: all.last().copied().unwrap_or(0.0),
        stalled: stalled_ms / field_ms.max(1e-9),
        onsets_per_min: f64::from(onsets) / field_minutes.max(1e-9),
        frames_per_s: f64::from(frames) / (f64::from(drift.song_ms) / 1_000.0 + 1.0),
    }
}

/// `live-v2` with the view delay: half a second or less at typical
/// latencies (here 80 ms each way with up to 40 ms of jitter), stalls
/// rare; `live-v1`'s constant second never stalls there either.
#[test]
fn the_views_follow_half_a_second_behind_at_typical_latencies() {
    let v2 = measure_views(5, (80, 40), RoomPolicy::default().with_live(), false, 3);
    eprintln!("live-v2, 80+40 ms: {v2:?}");
    assert!(v2.mean <= 500.0 && v2.p95 <= 520.0, "{v2:?}");
    assert!(v2.stalled < 0.005, "{v2:?}");
    assert!(v2.frames_per_s > 9.0, "{v2:?}");
    let old = RoomPolicy {
        live: Some(ddi_room::LivePolicy::v1()),
        ..RoomPolicy::default()
    };
    let v1 = measure_views(5, (80, 40), old, true, 3);
    eprintln!("live-v1, 80+40 ms, a constant second: {v1:?}");
    assert_eq!((v1.mean, v1.stalled), (1_000.0, 0.0), "{v1:?}");
    assert!(v1.frames_per_s < 4.5, "{v1:?}");
}

/// The study behind `view-delay-v0` (`cargo test -p ddi-room --test live
/// view_delay_study -- --ignored --nocapture`): the delay reached and the
/// stalls at latencies from 20 to 200 ms each way, `live-v1`'s timing and
/// `live-v2`'s, nine competitors and a watcher, three seeds each.
#[test]
#[ignore]
fn view_delay_study() {
    let old = RoomPolicy {
        live: Some(ddi_room::LivePolicy::v1()),
        ..RoomPolicy::default()
    };
    let new = RoomPolicy::default().with_live();
    eprintln!("| timing | each way (ms) | delay mean / p95 / max (ms) | field-steps stalled | stalls a field-minute |");
    eprintln!("| --- | --- | --- | --- | --- |");
    for (name, policy) in [("live-v1, view-delay-v0", &old), ("live-v2, view-delay-v0", &new)] {
        for lat in [(20, 20), (50, 30), (80, 40), (80, 80), (150, 75), (200, 100), (200, 200)] {
            let runs: Vec<Measured> = (1..=3)
                .map(|seed| measure_views(9, lat, policy.clone(), false, seed))
                .collect();
            let n = runs.len() as f64;
            eprintln!(
                "| {name} | {} + up to {} | {:.0} / {:.0} / {:.0} | {:.3}% | {:.2} |",
                lat.0,
                lat.1,
                runs.iter().map(|m| m.mean).sum::<f64>() / n,
                runs.iter().map(|m| m.p95).fold(0.0, f64::max),
                runs.iter().map(|m| m.max).fold(0.0, f64::max),
                100.0 * runs.iter().map(|m| m.stalled).sum::<f64>() / n,
                runs.iter().map(|m| m.onsets_per_min).sum::<f64>() / n,
            );
        }
    }
    // A full field: 73 competitors and a watcher, one seed.
    for lat in [(20, 20), (80, 40), (200, 100)] {
        let m = measure_views(73, lat, new.clone(), false, 1);
        eprintln!(
            "| live-v2, 73 competitors | {} + up to {} | {:.0} / {:.0} / {:.0} | {:.3}% | {:.2} |",
            lat.0,
            lat.1,
            m.mean,
            m.p95,
            m.max,
            100.0 * m.stalled,
            m.onsets_per_min
        );
    }
}

/// Clock drift over a long song (`cargo test --release -p ddi-room --test
/// live clock_drift_study -- --ignored --nocapture`): every device's clock
/// 0, 50 or 500 millionths fast or slow (alternately), a five-minute song,
/// the views measured over its last minute; each device's song clock
/// mapped from the room's once at the start and run on its own clock (a
/// play's audio clock, the game's way), or read from the room's clock as
/// estimated every time (resynced by the pings). Live-v2, 80 ms each way
/// with up to 40 ms of jitter, nine competitors and a watcher.
#[test]
#[ignore]
fn clock_drift_study() {
    let policy = RoomPolicy::default().with_live();
    eprintln!("| drift (ppm) | song clock | delay mean / p95 / max (ms) | field-time stalled | stalls a field-minute |");
    eprintln!("| --- | --- | --- | --- | --- |");
    for ppm in [0, 50, 500] {
        for (mapped_once, slewed) in [(true, false), (true, true), (false, false)] {
            for fixed in [true, false] {
                let m = measure_drifting(
                    9,
                    (80, 40),
                    policy.clone(),
                    fixed,
                    1,
                    Drift {
                        ppm,
                        mapped_once,
                        slewed,
                        song_ms: 300_000,
                        from: 240_000.0,
                    },
                );
                eprintln!(
                    "| {ppm} | {}{} | {:.0} / {:.0} / {:.0} | {:.3}% | {:.2} |",
                    match (mapped_once, slewed) {
                        (_, true) => "mapped once, kept in step",
                        (true, false) => "mapped once",
                        _ => "resynced",
                    },
                    if fixed { ", a constant second" } else { "" },
                    m.mean,
                    m.p95,
                    m.max,
                    100.0 * m.stalled,
                    m.onsets_per_min
                );
            }
        }
    }
}

/// The clocks' drift kept out of the views (`ddi_room::live::slew`, the
/// regression test of `clock_drift_study`'s rows): every device's clock 50
/// or 500 millionths fast or slow over a five-minute song, its song clock
/// mapped once (its judging's) and its stream's and views' times kept in
/// step with the room's: the delay over the song's last minute as without
/// drift (480 ms on average at 80 ms each way; mapped once alone, 608 at
/// 500), and no stall.
#[test]
fn the_views_keep_their_delay_when_the_clocks_drift() {
    let policy = RoomPolicy::default().with_live();
    for (ppm, competitors) in [(50, 3), (500, 3)] {
        let at = |slewed: bool, mapped_once: bool| {
            measure_drifting(
                competitors,
                (80, 40),
                policy.clone(),
                false,
                1,
                Drift {
                    ppm,
                    mapped_once,
                    slewed,
                    song_ms: 300_000,
                    from: 240_000.0,
                },
            )
        };
        let kept = at(true, true);
        let once = at(false, true);
        eprintln!("{ppm} ppm: kept in step {kept:?}; mapped once {once:?}");
        assert!(kept.mean <= 500.0, "{ppm}: {kept:?}");
        assert!(kept.stalled < 0.005, "{ppm}: {kept:?}");
        if ppm == 500 {
            assert!(once.mean > kept.mean + 60.0, "the drift shows without it: {once:?}");
        }
    }
}
