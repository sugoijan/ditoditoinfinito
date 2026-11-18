//! The audience through the fake room (`ddi_room::audience`,
//! `docs/plans/multiplayer-audience.md`): the public view and what it must
//! not say; the bound, the count, the host's closing and the room's end;
//! two thousand seats on relays following a 73-player stage at four
//! frames a second, every seat's fields equal to the room's; a relay's
//! restart mid-stage; the members' own stream unchanged by an audience;
//! what a tick costs at 1,000, 5,000 and 20,000 seats (`--ignored`).

use ddi_room::attacks::{ATTACKS, Reporter};
use ddi_room::audience::frames::{self, Coalesced, Coalescer};
use ddi_room::audience::hub::AudienceDurable;
use ddi_room::audience::public::PublicView;
use ddi_room::audience::relay::Relay;
use ddi_room::audience::{
    AUDIENCE_V0, Audience, AudienceEnd, AudienceNews, AudienceRefusal, AudienceRules, RelayDown,
    RelayDownEnvelope,
};
use ddi_room::fake::{
    FakeAudienceSocket, FakeServer, FakeSocket, Latency, OffsetClock, SharedFake,
};
use ddi_room::live::plan::BattlePlan;
use ddi_room::live::stream::{Frame, FrameEntry};
use ddi_room::live::{ClientLive, LiveNews, wire};
use ddi_room::{
    ChartKey, ClientEnvelope, ClientId, Clock, Closure, Command, Connection, ContentCard,
    ContentId, ContentStatus, Identity, Intent, Loading, ManualClock, MatchRules, Millis,
    OnlineAuthority, ParticipantId, Phase, RoomAuthority, RoomId, RoomPolicy,
    RoomState, SongKey, StageId, TicketProblem,
};
use std::time::Instant;

mod common;

use common::{Script, Stream, plan_under};

const ROOM: &str = "Room000001";
const STAGE: StageId = StageId(1);
const STEP_MS: u64 = 20;

fn policy() -> RoomPolicy {
    RoomPolicy::default().with_live().with_audience()
}

fn content() -> ContentId {
    ContentId {
        song: SongKey::new("library-song-key-1"),
        chart: ChartKey::new("digest-1"),
    }
}

/// A plan of `n` slots of 2 s, eight shocks in each where `room(j)`;
/// checkpoints every 2 s.
fn plan(n: usize, seed: u64, room: impl Fn(usize) -> bool) -> BattlePlan {
    plan_under(ATTACKS.id, n, seed, room)
}

/// A play of taps alone ([`common::script`]).
fn script(seed: u64, gap_ms: i32, end_ms: i32, out_ms: Option<i32>) -> Script {
    common::script(seed, gap_ms, end_ms, out_ms, false)
}

/// A member: the online authority on a fake socket, and the play it
/// streams; every frame it took with the room's time it took it at.
struct Player {
    who: Identity,
    id: ParticipantId,
    client: ClientId,
    seq: u64,
    link: OnlineAuthority<FakeSocket, OffsetClock>,
    server: SharedFake,
    stream: Stream,
    frames: Vec<(u64, Vec<u8>)>,
}

impl Player {
    fn song_us(&self) -> Option<i64> {
        let zero = self
            .link
            .room()
            .and_then(|r| r.stage(STAGE))
            .and_then(|s| s.play.as_ref())
            .and_then(|p| p.song_zero_at)?;
        let now = self.link.room_now()?;
        Some(ddi_room::live::song_us_at(zero, now))
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

    fn pump(&mut self, now: u64) {
        if self.link.wants_ticket() {
            self.link.ticket_asked();
            let t = self.server.borrow_mut().ticket(&self.who);
            self.link.ticket(t.map_err(|_| TicketProblem::NotAMember));
        }
        self.feed();
        let _ = self.link.poll();
        for n in self.link.live_mut().take_news() {
            if let LiveNews::Frame(f) = n {
                self.frames.push((now, wire::encode_frame(&f)));
            }
        }
    }

    fn feed(&mut self) {
        if !self.stream.streaming() {
            return;
        }
        let Some(t_us) = self.song_us() else {
            return;
        };
        if let Some(report) = self.stream.feed(self.link.live_mut(), STAGE, t_us) {
            self.send(report);
        }
    }
}

/// An audience seat: the device's link over a fake socket on a clock of
/// its own, and the lateness of every moment it was given (room ms from
/// the moment's song time to its arrival).
struct Seat {
    who: String,
    audience: Audience<FakeAudienceSocket, OffsetClock>,
    latency: Option<Latency>,
    refused: Option<AudienceRefusal>,
    lateness: Vec<i64>,
    frames: u32,
}

struct Room {
    clock: ManualClock,
    server: SharedFake,
    players: Vec<Player>,
    seats: Vec<Seat>,
    song_zero: Option<u64>,
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
        let key = self.server.borrow().audience_key().unwrap_or_default();
        let zero = self.song_zero.or_else(|| {
            self.server
                .borrow()
                .state()
                .stage(STAGE)
                .and_then(|s| s.play.as_ref())
                .and_then(|p| p.song_zero_at)
                .map(|m| m.0)
        });
        self.song_zero = zero;
        for s in &mut self.seats {
            if s.audience.wants_ticket() && s.refused.is_none() {
                let joined = self
                    .server
                    .borrow_mut()
                    .audience_join(&s.who, &key, s.latency);
                match joined {
                    Ok(t) => s.audience.ticket(&t),
                    Err(r) => s.refused = Some(r),
                }
            }
            for n in s.audience.poll() {
                if let AudienceNews::Frame(f) = n {
                    s.frames += 1;
                    if let Some(zero) = zero {
                        for e in &f.entries {
                            for m in &e.moments {
                                s.lateness
                                    .push(now as i64 - (zero as i64 + i64::from(m.at_ms)));
                            }
                        }
                    }
                }
            }
        }
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

    fn add_seats(&mut self, n: usize, latency: Option<(u64, u64)>, seed: u64) {
        let base = self.seats.len();
        for i in base..base + n {
            let clock = OffsetClock {
                base: self.clock.clone(),
                offset_ms: (i as i64 * 7_919 % 20_000) - 10_000,
            };
            self.seats.push(Seat {
                who: format!("twitch:viewer-{i}"),
                audience: Audience::new(
                    RoomId::new(ROOM),
                    FakeAudienceSocket::new(self.server.clone()),
                    clock,
                ),
                latency: latency.map(|(floor, jitter)| Latency {
                    down_ms: floor,
                    down_jitter_ms: jitter,
                    up_ms: floor,
                    up_jitter_ms: jitter,
                    seed: seed.wrapping_add(i as u64 * 104_729),
                }),
                refused: None,
                lateness: Vec::new(),
                frames: 0,
            });
        }
    }
}

/// A room of `competitors` people (the first its owner) and `watchers`
/// members watching, connected, the stage's song chosen and available.
fn room(competitors: usize, watchers: usize, policy: RoomPolicy, attacks: bool) -> Room {
    let clock = ManualClock::new(1_000_000);
    let server =
        FakeServer::hosted(ROOM, Identity::new("p:1"), "P1", policy, clock.clone()).shared();
    server.borrow_mut().set_room_name("Friday night");
    let mut players = Vec::new();
    for i in 1..=competitors + watchers {
        let who = Identity::new(format!("p:{i}"));
        let intent = if i <= competitors {
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
        let mut l = Latency {
            down_ms: 20,
            down_jitter_ms: 60,
            up_ms: 20,
            up_jitter_ms: 60,
            seed: 1,
        };
        l.seed = l.seed.wrapping_add(i as u64 * 7919);
        server.borrow_mut().set_latency(id, l);
        let clock = OffsetClock {
            base: clock.clone(),
            offset_ms: (i as i64 * 7_919 % 20_000) - 10_000,
        };
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
            stream: Stream::default(),
            frames: Vec::new(),
        });
    }
    let mut r = Room {
        clock,
        server,
        players,
        seats: Vec::new(),
        song_zero: None,
    };
    r.run(1_000);
    for _ in 0..8 {
        if r.players.iter().all(|p| p.link.live().clock().is_some()) {
            break;
        }
        r.run(500);
    }
    let mut rules = MatchRules::hosted();
    rules.seats = competitors as u16;
    rules.set_attacks(attacks);
    r.players[0].send(Command::SaveNextRules { rules });
    r.run(500);
    r.players[0].send(Command::Pick {
        content: content(),
        card: None,
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

fn start(r: &mut Room, plan: Option<&BattlePlan>, competitors: usize, scripts: &[Script]) {
    if let Some(plan) = plan {
        r.players[0].link.live_mut().send(ClientLive::OfferPlan {
            stage: STAGE,
            plan: Box::new(plan.clone()),
        });
        r.run(500);
    }
    let rev = r.state().stage(STAGE).unwrap().readiness_rev;
    for p in r.players.iter_mut().take(competitors) {
        p.link.live_mut().send(ClientLive::Prepared {
            stage: STAGE,
            horizon_us: 2_000_000,
            plan: plan.map(|p| p.digest.clone()),
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
    // Everyone ready: the stage starts at once.
    r.run(1_500);
    let s = r.state();
    assert!(
        matches!(s.phase, Phase::Countdown { .. } | Phase::Stage { .. }),
        "{:?}",
        s.phase
    );
    for (p, s) in r.players.iter_mut().zip(scripts) {
        let reporter = plan.map(|plan| Reporter::new(p.id, plan.checkpoints_us.clone()));
        p.stream.give(s.clone(), reporter);
    }
}

fn settled(r: &Room) {
    let s = r.state();
    assert!(
        matches!(s.phase, Phase::MatchResults | Phase::Intermission { .. }),
        "{:?}",
        s.phase
    );
}

/// Every seat followed every competitor's field to its end, equal to what
/// was sent (a seat that came in mid-stage: the last seconds before it
/// came, then everything).
fn every_seat_saw_everything(r: &Room, scripts: &[Script]) {
    every_seat_saw(r, scripts, &[]);
}

fn every_seat_saw(r: &Room, scripts: &[Script], late: &[usize]) {
    for (i, s) in r.seats.iter().enumerate() {
        let reader = s
            .audience
            .link()
            .reader()
            .unwrap_or_else(|| panic!("seat {i} followed the stage"));
        for (q, sc) in r.players.iter().zip(scripts) {
            let seen = reader.moments(q.id);
            if late.contains(&i) {
                assert!(
                    seen.len() > sc.moments.len() / 3,
                    "seat {i}: {}",
                    seen.len()
                );
                assert_eq!(seen, &sc.moments[sc.moments.len() - seen.len()..]);
            } else {
                assert_eq!(seen, &sc.moments[..], "seat {i}, {:?}", q.id);
            }
            assert!(reader.end(q.id).is_some(), "seat {i}, {:?}'s end", q.id);
        }
    }
}

fn percentile(v: &mut [i64], p: f64) -> i64 {
    v.sort_unstable();
    v.get(((v.len() as f64 - 1.0) * p).round() as usize)
        .copied()
        .unwrap_or(0)
}

#[test]
fn the_public_view_says_nothing_a_member_alone_knows() {
    let clock = ManualClock::new(1_000_000);
    let mut s = FakeServer::hosted(
        ROOM,
        Identity::new("twitch:owner-identity-1234"),
        "Hostess",
        policy(),
        clock,
    );
    s.set_room_name("Friday night");
    let p2 = s
        .join(
            Identity::new("twitch:player-two-777"),
            "Second",
            Intent::Play,
        )
        .unwrap();
    let bot = s
        .join(Identity::bot("lumen-kite"), "Lumen Kite", Intent::Play)
        .unwrap();
    let quiet = s
        .join(
            Identity::new("twitch:quiet-watcher-55"),
            "Quiet Watcher",
            Intent::Watch,
        )
        .unwrap();
    let held = s
        .join_watch_only(Identity::new("twitch:held-watcher-66"), "Held Watcher")
        .unwrap();
    s.authority(Command::SetContentStatus {
        stage: STAGE,
        status: ContentStatus::Pending,
        share: None,
    })
    .ok();
    // Facts only members see, set as a room would have them.
    s.rewrite(|st| {
        let m = st.members.get_mut(&p2).unwrap();
        m.role = ddi_room::Role::CoHost;
        m.admin = true;
        m.targeting = ddi_room::attacks::Targeting::Leader;
        m.connection = Connection::Reconnecting;
        st.access.invite_generation = 7;
        st.spotlight = Some(ddi_room::Spotlight {
            content: ContentId {
                song: SongKey::new("spotlight-song-key"),
                chart: ChartKey::new("spotlight-digest"),
            },
            card: ContentCard {
                title: "Spotlight Title".into(),
                ..ContentCard::default()
            },
            by: ParticipantId(1),
            share: Some("spotshare".into()),
        });
        st.playlist[0].content = Some(ddi_room::StageContent {
            id: content(),
            status: ContentStatus::Available,
            share: Some("abc12345".into()),
            card: Some(ContentCard {
                title: "Stage Song".into(),
                artist: "Artist".into(),
                ..ContentCard::default()
            }),
            battle: None,
        });
        st.playlist[0].prep.insert(
            p2,
            ddi_room::Prep {
                ready: true,
                loading: Loading::Failed {
                    code: "secret_failure_code".into(),
                },
            },
        );
    });
    let view = PublicView::of(s.state(), "Friday night");
    let json = serde_json::to_string(&view).unwrap();
    for leak in [
        "twitch:",
        "bot:",
        "owner-identity",
        "Quiet Watcher",
        "Held Watcher",
        "admin",
        "co_host",
        "role",
        "connection",
        "reconnecting",
        "targeting",
        "leader",
        "watch_only",
        "intent",
        "invite",
        "generation",
        "admission",
        "spotlight",
        "Spotlight Title",
        "spotshare",
        "library-song-key-1",
        "secret_failure_code",
        "ready",
        "prep",
        "picking",
        "picker",
        "next",
    ] {
        assert!(!json.contains(leak), "the view says {leak:?}: {json}");
    }
    // What it does say.
    assert_eq!(view.name, "Friday night");
    assert_eq!(view.host.as_deref(), Some("Hostess"));
    let names: Vec<_> = view.players.iter().map(|p| p.name.clone()).collect();
    assert_eq!(
        names,
        vec![
            Some("Hostess".to_string()),
            Some("Second".into()),
            Some("Lumen Kite".into())
        ]
    );
    assert_eq!(
        view.players[2].kind,
        ddi_room::audience::public::PublicKind::Bot
    );
    assert!(!view.players.iter().any(|p| p.id == quiet || p.id == held));
    assert_eq!(view.players[2].id, bot);
    let stage = &view.stages[0];
    assert_eq!(stage.share.as_deref(), Some("abc12345"));
    assert_eq!(stage.chart, Some(ChartKey::new("digest-1")));
    assert_eq!(stage.card.as_ref().unwrap().title, "Stage Song");
    // Deterministic: the same room, the same bytes.
    let again = serde_json::to_string(&PublicView::of(s.state(), "Friday night")).unwrap();
    assert_eq!(json, again);
}

/// Round D1, for the audience's page: the view carries what a banner
/// needs of each stage's song (the room's share and the chart's digest:
/// what a member's client finds the song and its banner in the share's
/// listing by) and everything a member's Standings table shows of a
/// settled stage (each starter's place, finish and award, their
/// details, the match's totals and places, their names), and, once the
/// room ended, why. Over the room revision 10 stored (stage 1 settled,
/// stage 2 next, a spotlight on another song).
#[test]
fn the_view_has_what_a_banner_and_the_standings_need() {
    use ddi_room::{Actor, Closure, Command, Millis, ParticipantId, RoomState, StageId};
    let mut state: RoomState = serde_json::from_str(include_str!("room-revision-10.json")).unwrap();
    let view = PublicView::of(&state, "Friday night");
    let one = &view.stages[0];
    assert_eq!((one.share.as_deref(), one.chart.clone()), (Some("Share001"), Some(ChartKey::new("digest-1"))));
    assert_eq!(one.card.as_ref().unwrap().title, "Song 1");
    let play = one.play.as_ref().unwrap();
    let result = play.result.as_ref().unwrap();
    assert_eq!(result.placements.len(), 5);
    assert_eq!(result.placements[0].participant, ParticipantId(7));
    assert_eq!(result.placements[4].finish, ddi_room::Finish::Forfeit);
    assert_eq!(play.details.len(), 4, "everyone who reported how they did");
    let host = play.details.iter().find(|(p, _)| *p == ParticipantId(1)).unwrap();
    assert_eq!((host.1.chart.as_deref(), host.1.harder), (Some("Challenge"), true));
    assert_eq!(view.standings.len(), 5);
    assert_eq!(view.standings[0].rank, 1);
    assert!(view.stages[1].rules.is_some(), "stage 2's changed rules");
    // Every row has a name to show (a CPU's its roster's).
    for st in &view.standings {
        let p = view.players.iter().find(|p| p.id == st.participant).unwrap();
        assert!(p.name.is_some());
    }
    // Nothing of the spotlight, which only members see.
    let json = serde_json::to_string(&view).unwrap();
    assert!(!json.contains("Song 2") && !json.contains("closure"), "{json}");
    // The song picked for the next stage: its card at once, its share
    // once the room has it.
    state
        .apply(
            Actor::Member(ParticipantId(2)),
            Some(state.revision),
            &Command::Pick {
                content: ddi_room::ContentId {
                    song: ddi_room::SongKey::new("song-3"),
                    chart: ChartKey::new("digest-3"),
                },
                card: Some(Box::new(ContentCard {
                    title: "Song 3".into(),
                    ..ContentCard::default()
                })),
            },
            Millis(2_000_000),
        )
        .unwrap();
    let next = PublicView::of(&state, "Friday night").stages[1].clone();
    assert_eq!(next.id, StageId(2));
    assert_eq!((next.card.unwrap().title.as_str(), next.share), ("Song 3", None));
    state
        .apply(
            Actor::Authority,
            None,
            &Command::SetContentStatus {
                stage: StageId(2),
                status: ContentStatus::Available,
                share: Some("Share003".into()),
            },
            Millis(2_000_100),
        )
        .unwrap();
    let next = PublicView::of(&state, "Friday night").stages[1].clone();
    assert_eq!((next.share.as_deref(), next.chart), (Some("Share003"), Some(ChartKey::new("digest-3"))));
    // Closed by the server's operator: the view says so, and why.
    state
        .apply(
            Actor::Authority,
            None,
            &Command::CloseRoom { reason: Closure::ByAdministrator },
            Millis(2_000_200),
        )
        .unwrap();
    let closed = PublicView::of(&state, "Friday night");
    assert!(closed.closed);
    assert_eq!(closed.closure, Some(Closure::ByAdministrator));
    let json = serde_json::to_value(&closed).unwrap();
    assert_eq!(json["closure"], "by_administrator");
    // A view from before the field reads.
    let mut old = json.clone();
    old.as_object_mut().unwrap().remove("closure");
    assert_eq!(serde_json::from_value::<PublicView>(old).unwrap().closure, None);
}

#[test]
fn the_view_changes_only_with_what_it_shows() {
    let clock = ManualClock::new(1_000_000);
    let mut s = FakeServer::hosted(ROOM, Identity::new("p:1"), "P1", policy(), clock);
    let p2 = s.join(Identity::new("p:2"), "P2", Intent::Play).unwrap();
    let before = PublicView::of(s.state(), "R");
    // A member's connection, someone joining to watch: the room moves on,
    // the audience's view does not.
    s.authority(Command::SetConnection {
        participant: p2,
        connection: Connection::Connected,
    })
    .unwrap();
    s.join(Identity::new("p:3"), "P3", Intent::Watch).unwrap();
    let mut after = PublicView::of(s.state(), "R");
    let revision = after.revision;
    assert_ne!(before.revision, revision);
    assert!(before.same_as(&mut after));
    assert_eq!(after.revision, revision, "the view compared is put back");
    // A player joining does change it.
    s.join(Identity::new("p:4"), "P4", Intent::Play).unwrap();
    assert!(!after.same_as(&mut PublicView::of(s.state(), "R")));
}

#[test]
fn the_audience_is_bounded_counted_and_closed_plainly() {
    let rules = AudienceRules {
        max_audience: 6,
        relay_seats: 3,
        ..AUDIENCE_V0
    };
    let mut r = room(2, 0, policy(), false);
    r.server.borrow_mut().set_audience_rules(rules);
    let key = r.server.borrow().audience_key().unwrap();
    // A link that is not the room's.
    assert_eq!(
        r.server
            .borrow_mut()
            .audience_join("twitch:x", "wrong", None),
        Err(AudienceRefusal::BadLink)
    );
    // Two seats an identity.
    r.seats.clear();
    for who in ["twitch:a", "twitch:a"] {
        r.add_seats(1, Some((10, 10)), 3);
        r.seats.last_mut().unwrap().who = who.into();
    }
    r.run(200);
    assert!(r.seats.iter().all(|s| s.refused.is_none()));
    assert_eq!(
        r.server.borrow_mut().audience_join("twitch:a", &key, None),
        Err(AudienceRefusal::TooManySeats)
    );
    // Up to the bound, then plainly full.
    r.add_seats(4, Some((10, 10)), 5);
    r.run(200);
    assert!(r.seats.iter().all(|s| s.refused.is_none()), "six seats");
    assert_eq!(
        r.server
            .borrow_mut()
            .audience_join("twitch:late", &key, None),
        Err(AudienceRefusal::Full)
    );
    assert_eq!(r.server.borrow().audience_stats().relays, 2);
    // The count: at the room and its members within a second or so, at
    // every seat between stages within `idle_count_ms`.
    r.run(6_000);
    assert_eq!(r.server.borrow().audience_hub().unwrap().count(), 6);
    for p in &r.players {
        assert_eq!(p.link.live().audience(), Some((6, true)), "{:?}", p.id);
    }
    for s in &r.seats {
        assert_eq!(s.audience.link().count(), Some(6));
        let v = s.audience.link().view().expect("the view");
        assert_eq!(v.name, "Friday night");
    }
    // A seat leaves: a place frees up, the count follows.
    r.seats[5].audience.leave();
    r.run(6_000);
    assert_eq!(r.players[0].link.live().audience(), Some((5, true)));
    assert!(
        r.server
            .borrow_mut()
            .audience_join("twitch:late", &key, None)
            .is_ok()
    );
    // Someone not the owner cannot close it.
    r.players[1]
        .link
        .live_mut()
        .send(ClientLive::Audience { open: false });
    r.run(500);
    assert!(r.server.borrow().audience_hub().unwrap().is_open());
    // A co-host closes it (an operator's, round B1): every seat goes,
    // said why; nobody gets in.
    let co = r.players[1].id;
    r.server.borrow_mut().rewrite(|s| {
        s.members.get_mut(&co).unwrap().role = ddi_room::Role::CoHost;
    });
    r.run(500);
    r.players[1]
        .link
        .live_mut()
        .send(ClientLive::Audience { open: false });
    r.run(3_000);
    assert_eq!(
        r.server.borrow().audience_hub().unwrap().durable(),
        AudienceDurable { closed: true }
    );
    for s in r.seats.iter().take(5) {
        assert_eq!(s.audience.link().ended(), Some(AudienceEnd::Closed));
        assert!(!s.audience.wants_ticket());
    }
    assert_eq!(
        r.server.borrow_mut().audience_join("twitch:b", &key, None),
        Err(AudienceRefusal::Closed)
    );
    assert_eq!(r.players[0].link.live().audience(), Some((0, false)));
    // Opened again: people come back in.
    r.players[0]
        .link
        .live_mut()
        .send(ClientLive::Audience { open: true });
    r.run(500);
    assert!(
        r.server
            .borrow_mut()
            .audience_join("twitch:b", &key, None)
            .is_ok()
    );
    // The room ends: every seat is told and closed (4410), nobody gets in.
    r.add_seats(1, None, 9);
    r.run(500);
    r.server
        .borrow_mut()
        .rewrite(|s| s.closed = Some(Closure::HostLeft));
    r.run(1_000);
    assert_eq!(
        r.seats.last().unwrap().audience.link().ended(),
        Some(AudienceEnd::RoomClosed)
    );
    assert_eq!(
        r.server.borrow_mut().audience_join("twitch:c", &key, None),
        Err(AudienceRefusal::RoomClosed)
    );
}

#[test]
fn a_room_without_an_audience_has_no_seats() {
    let mut s = FakeServer::hosted(
        ROOM,
        Identity::new("p:1"),
        "P1",
        RoomPolicy::default().with_live(),
        ManualClock::new(1),
    );
    assert_eq!(
        s.audience_join("twitch:a", "k", None),
        Err(AudienceRefusal::NoAudience)
    );
}

#[test]
fn two_thousand_watch_a_seventy_three_player_stage_at_four_frames_a_second() {
    let begun = Instant::now();
    let plan = plan(16, 7, |j| j % 3 != 2);
    let scripts: Vec<Script> = (1..=73u64)
        .map(|i| {
            let out = (i % 11 == 0).then_some(8_000 + i as i32 * 200);
            script(100 + i, 110 + (i % 5) as i32 * 10, 30_000, out)
        })
        .collect();
    let mut r = room(73, 0, policy(), true);
    // The audience arrives before the stage, at 20 to 120 ms each way.
    r.add_seats(2_000, Some((20, 100)), 11);
    r.run(2_000);
    assert_eq!(r.server.borrow().audience_sockets(), 2_000);
    start(&mut r, Some(&plan), 73, &scripts);
    r.run(40_000);
    settled(&r);
    every_seat_saw_everything(&r, &scripts);
    let server = r.server.borrow();
    let stage = server.live().unwrap().stage_or_last().unwrap();
    let audience_frames = server.audience_stats().frames;
    // The coarser rate: one audience frame for every two or three of the
    // members' (100 ms ticks, 250 ms audience frames).
    let ratio = f64::from(stage.frames()) / f64::from(audience_frames);
    assert!((2.0..=3.0).contains(&ratio), "{ratio}");
    // Four homes of 500 seats, and the few their unevenness overflows to.
    let relays = server.audience_stats().relays;
    assert!((4..=8).contains(&relays), "{relays}");
    assert_eq!(server.audience_hub().unwrap().count(), 2_000);
    drop(server);
    for p in &r.players {
        assert_eq!(p.link.live().audience(), Some((2_000, true)));
    }
    for s in &r.seats {
        assert_eq!(s.audience.link().count(), Some(2_000));
        assert!(s.audience.link().room_ms(0).is_some());
    }
    let mut late: Vec<i64> = r.seats.iter().flat_map(|s| s.lateness.clone()).collect();
    let (p50, p99) = (percentile(&mut late, 0.5), percentile(&mut late, 0.99));
    let max = *late.last().unwrap();
    eprintln!(
        "2,000 seats on {relays} relays, 73 competitors, 40 s: {} member frames, {} audience frames; \
         a moment's lateness at a seat p50 {p50} ms, p99 {p99} ms, max {max} ms; {:.1} s of test time",
        r.server
            .borrow()
            .live()
            .unwrap()
            .stage_or_last()
            .unwrap()
            .frames(),
        audience_frames,
        begun.elapsed().as_secs_f64(),
    );
    // Every moment reached every seat within the audience's delay.
    assert!(max < AUDIENCE_V0.presentation_delay_ms as i64, "{max}");
}

#[test]
fn a_relay_that_restarts_mid_stage_gives_its_seats_the_stage_again() {
    let scripts: Vec<Script> = (1..=9u64)
        .map(|i| script(i, 100 + i as i32 * 7, 24_000, (i == 4).then_some(15_000)))
        .collect();
    let mut r = room(9, 0, policy(), false);
    r.add_seats(300, Some((20, 60)), 21);
    r.run(1_000);
    start(&mut r, None, 9, &scripts);
    r.run(8_000);
    // Relay 0's object loses its memory (its seats' sockets kept); later a
    // seat's socket drops and it comes back by a new ticket.
    let relays = r.server.borrow().audience_stats().relays;
    assert!(relays >= 1);
    r.server.borrow_mut().restart_relay(0);
    r.run(4_000);
    r.server.borrow_mut().restart_relay(0);
    r.seats[7].audience.leave();
    r.seats[7].audience = Audience::new(
        RoomId::new(ROOM),
        FakeAudienceSocket::new(r.server.clone()),
        OffsetClock {
            base: r.clock.clone(),
            offset_ms: 0,
        },
    );
    r.run(20_000);
    settled(&r);
    every_seat_saw(&r, &scripts, &[7]);
    assert_eq!(r.server.borrow().audience_hub().unwrap().count(), 300);
    // The relay's own record of the stage is the room's.
    let server = r.server.borrow();
    let relay = server.relay(0).unwrap();
    let kept = relay.stage_snapshot().unwrap();
    let room = server
        .live()
        .unwrap()
        .stage_or_last()
        .unwrap()
        .snapshot(1, Millis(0));
    assert_eq!(kept.competitors.len(), room.competitors.len());
    for (a, b) in kept.competitors.iter().zip(&room.competitors) {
        assert_eq!(
            (
                a.participant,
                a.end,
                a.life_ppm,
                a.combo,
                a.score,
                a.through_us
            ),
            (
                b.participant,
                b.end,
                b.life_ppm,
                b.combo,
                b.score,
                b.through_us
            )
        );
    }
}

#[test]
fn the_members_stream_is_the_same_with_an_audience() {
    let run = |seats: usize| {
        let scripts: Vec<Script> = (1..=9u64)
            .map(|i| script(i, 90 + i as i32 * 5, 20_000, None))
            .collect();
        let mut r = room(9, 1, policy(), false);
        r.add_seats(seats, Some((20, 80)), 31);
        r.run(1_000);
        start(&mut r, None, 9, &scripts);
        r.run(25_000);
        settled(&r);
        if seats > 0 {
            every_seat_saw_everything(&r, &scripts);
        }
        r.players
            .iter()
            .map(|p| p.frames.clone())
            .collect::<Vec<_>>()
    };
    let without = run(0);
    let with = run(500);
    assert!(without.iter().all(|f| f.len() > 100));
    // Every member took the same frames, byte for byte, at the same times.
    assert_eq!(without, with);
}

/// What the audience costs a tick (`--release --ignored --nocapture`):
/// the room's coalescing and encoding of one audience frame, a relay's
/// reading and keeping it, and handing its bytes to every seat (a copy
/// into each seat's queue, as a host's send copies into the socket's
/// buffer: natively, without the platform's own cost of a send), at 9 and
/// 73 competitors and 1,000, 5,000 and 20,000 seats; the bytes and sends
/// a second.
#[test]
#[ignore]
fn what_the_audience_costs() {
    println!(
        "| Competitors | Seats | Relays | Audience frame | Room: a frame built (µs) | A relay: a frame read (µs) | A relay: sent to its seats (µs) | All relays a second (ms) | Sends a second | Bytes a second out |"
    );
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    for n in [9u64, 73] {
        let scripts: Vec<Script> = (1..=n).map(|p| script(p, 83, 70_000, None)).collect();
        // The members' frames: one a 100 ms tick, every competitor's
        // moments of that tick.
        let member: Vec<Vec<u8>> = (1..=600u32)
            .map(|t| {
                let from = (t as i32 - 1) * 100;
                let to = t as i32 * 100;
                wire::encode_frame(&Frame {
                    stage: STAGE,
                    n: t,
                    at: Millis(u64::from(t) * 100),
                    entries: scripts
                        .iter()
                        .enumerate()
                        .map(|(i, s)| FrameEntry {
                            participant: ParticipantId(i as u64 + 1),
                            seq: t,
                            through_us: i64::from(to) * 1_000,
                            moments: s
                                .moments
                                .iter()
                                .filter(|m| m.at_ms >= from && m.at_ms < to)
                                .copied()
                                .collect(),
                            end: None,
                        })
                        .collect(),
                    commitments: Vec::new(),
                    checkpoints: Vec::new(),
                    settling: None,
                })
            })
            .collect();
        let mut c = Coalescer::new(AUDIENCE_V0.frame_ms, 120);
        let mut built: Vec<Vec<u8>> = Vec::new();
        let mut build_ns = 0u128;
        for (i, b) in member.iter().enumerate() {
            let t = Instant::now();
            c.member_frame(b);
            for out in c.tick(Millis((i as u64 + 1) * 100), Some(5_000)) {
                if let Coalesced::Frame(_, bytes) = out {
                    built.push(bytes);
                }
            }
            build_ns += t.elapsed().as_nanos();
        }
        let per_frame_build = build_ns as f64 / built.len() as f64 / 1e3;
        let frame_bytes = built.iter().map(Vec::len).sum::<usize>() as f64 / built.len() as f64;
        let frames_per_s = 1_000.0 / AUDIENCE_V0.frame_ms as f64;
        for seats in [1_000usize, 5_000, 20_000] {
            // The four home relays first, then as many as the seats fill.
            let relays = seats
                .div_ceil(AUDIENCE_V0.relay_seats as usize)
                .max(AUDIENCE_V0.home_relays as usize);
            let per_relay = seats.div_ceil(relays);
            let mut relay = Relay::new(AUDIENCE_V0, 0);
            let _ = relay.from_room_text(
                &RelayDownEnvelope::text(RelayDown::Admission {
                    key: "k".into(),
                    open: true,
                    seats: AUDIENCE_V0.relay_seats,
                }),
                0,
            );
            let mut queues: Vec<Vec<Vec<u8>>> = vec![Vec::with_capacity(8); per_relay];
            let (mut read_ns, mut send_ns) = (0u128, 0u128);
            for (k, b) in built.iter().enumerate() {
                let t = Instant::now();
                let out = relay.from_room_binary(b, k as u64 * 250);
                read_ns += t.elapsed().as_nanos();
                let t = Instant::now();
                for (_, send) in &out.to_seats {
                    if let ddi_room::online::Message::Binary(bytes) = send {
                        for q in queues.iter_mut() {
                            q.push(bytes.clone());
                        }
                    }
                }
                send_ns += t.elapsed().as_nanos();
                for q in queues.iter_mut() {
                    q.clear();
                }
            }
            let read = read_ns as f64 / built.len() as f64 / 1e3;
            let send = send_ns as f64 / built.len() as f64 / 1e3;
            println!(
                "| {n} | {seats} | {relays} | {frame_bytes:.0} B | {per_frame_build:.0} | {read:.0} | {send:.0} | {:.1} | {:.0} | {:.1} MB |",
                (read + send) * frames_per_s * relays as f64 / 1e3,
                frames_per_s * seats as f64,
                frame_bytes * frames_per_s * seats as f64 / 1e6,
            );
        }
    }
    // The view at a full field.
    let mut s = FakeServer::hosted(
        ROOM,
        Identity::new("p:1"),
        "P1",
        policy(),
        ManualClock::new(1),
    );
    for i in 2..=73 {
        s.join(
            Identity::new(format!("p:{i}")),
            &format!("Player {i}"),
            Intent::Play,
        )
        .unwrap();
    }
    let view = serde_json::to_string(&PublicView::of(s.state(), "A room")).unwrap();
    println!("The view of a room of 73: {} B as JSON", view.len());
    let a = frames::decode(&frames::encode(&ddi_room::audience::AudienceFrame {
        count: Some(1),
        frame: Frame {
            stage: STAGE,
            n: 1,
            at: Millis(0),
            entries: Vec::new(),
            commitments: Vec::new(),
            checkpoints: Vec::new(),
            settling: None,
        },
    }))
    .unwrap();
    assert_eq!(a.count, Some(1));
}
