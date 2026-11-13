//! A stage an online room plays live (`crate::battle::live`,
//! `docs/plans/multiplayer-online-play.md`), its plays judged by the
//! engine on the fake clock, through the fake room (`ddi_room::fake`):
//! each device's play started at the room's instant, its moments and
//! reports streamed, the others' fields fed by the room's frames, the
//! room's attacks put into the plays, its decision shown.

use super::*;
use ddi_room::fake::{FakeServer, FakeSocket, Latency, SharedFake};
use ddi_room::live::StreamEnd;
use ddi_room::{Command, ContentStatus, Identity, Intent, ManualClock, RoomPolicy};
use ddi_ui::state::online::api::{ApiRequest, ApiResponse, Method};
use ddi_ui::state::online::{Http, Online, OnlineConfig, Platform, SharedOnline};
use ddi_ui::state::room::{Picked, SharedRoom, prepare_stage};

/// The wall clock's ms at host second 0.
const WALL: u64 = 1_700_000_000_000;

fn wall(host: f64) -> u64 {
    WALL + (host * 1_000.0).round() as u64
}

/// HTTP answered at once from the fake room: who `who` is, the room's
/// summary, its tickets and upload grants.
struct RoomHttp {
    server: SharedFake,
    who: &'static str,
    out: Vec<(u64, ApiResponse)>,
}

fn ok(body: String) -> ApiResponse {
    ApiResponse {
        status: 200,
        body,
        session: None,
    }
}

impl Http for RoomHttp {
    fn start(&mut self, id: u64, req: ApiRequest) {
        let identity = Identity::new(format!("dev:{}", self.who));
        let answer = match (req.method, req.path.as_str()) {
            (Method::Get, "/me") => ok(format!(
                r#"{{"player_id":"dev:{0}","login":"{0}","display_name":"{0}","admin":false,"expires_at_ms":1}}"#,
                self.who
            )),
            (Method::Get, "/rooms/Room000001") => ok(
                r#"{"room_id":"Room000001","name":"Live","revision":1,"phase":"lobby","admission":"open","participant":1}"#
                    .into(),
            ),
            (Method::Post, "/rooms/Room000001/ticket") => {
                match self.server.borrow_mut().ticket(&identity) {
                    Ok(t) => ok(format!(r#"{{"ticket":"{t}","expires_at_ms":9}}"#)),
                    Err(_) => ApiResponse {
                        status: 403,
                        body: r#"{"error":"not_a_member"}"#.into(),
                        session: None,
                    },
                }
            }
            (Method::Post, "/rooms/Room000001/grant") => ok(format!(
                r#"{{"grant":"grant-{}","expires_at_ms":9,"max_bytes":100.0}}"#,
                self.who
            )),
            _ => ApiResponse {
                status: 403,
                body: r#"{"error":"not_an_operator"}"#.into(),
                session: None,
            },
        };
        self.out.push((id, answer));
    }

    fn poll(&mut self) -> Vec<(u64, ApiResponse)> {
        std::mem::take(&mut self.out)
    }
}

fn person(server: &SharedFake, who: &'static str) -> SharedOnline {
    let online = Online::new(
        Box::new(RoomHttp {
            server: server.clone(),
            who,
            out: Vec::new(),
        }),
        OnlineConfig {
            platform: Platform::Web,
            site: "http://localhost:8080/".into(),
        },
    )
    .shared();
    online.borrow_mut().check();
    online.borrow_mut().poll(0);
    online
}

/// A room that plays live: `alice` (its owner) and the others, each
/// device's client on its own socket, every message `latency` away.
struct Room {
    host: Clock,
    server_clock: ManualClock,
    server: SharedFake,
    rooms: Vec<SharedRoom>,
}

impl Room {
    fn new(people: &[(&'static str, Intent)], latency: Option<Latency>) -> Room {
        let host: Clock = Rc::new(RefCell::new(0.0));
        let server_clock = ManualClock::new(wall(0.0));
        let server = FakeServer::hosted(
            "Room000001",
            Identity::new("dev:alice"),
            "alice",
            RoomPolicy::default().with_live(),
            server_clock.clone(),
        )
        .shared();
        let mut rooms = Vec::new();
        for (i, (who, intent)) in people.iter().enumerate() {
            if i > 0 {
                server
                    .borrow_mut()
                    .join(Identity::new(format!("dev:{who}")), who, *intent)
                    .unwrap();
            }
            if let Some(mut l) = latency {
                l.seed = l.seed.wrapping_add(i as u64 * 31);
                server
                    .borrow_mut()
                    .set_latency(ddi_room::ParticipantId(i as u64 + 1), l);
            }
            rooms.push(Rc::new(RefCell::new(RoomClient::online(
                person(&server, who),
                "Room000001",
                Box::new(FakeSocket::new(server.clone())),
                ddi_room::ClientId::new(format!("{who}-test")),
                wall(0.0),
            ))));
        }
        Room {
            host,
            server_clock,
            server,
            rooms,
        }
    }

    /// Time passes by `dt` seconds, the room's clients ticked (as their
    /// screens tick them outside a play).
    fn step(&self, dt: f64, clients: bool) {
        *self.host.borrow_mut() += dt;
        let now = wall(*self.host.borrow());
        self.server_clock.set(now);
        if clients {
            for r in &self.rooms {
                r.borrow_mut().tick(now);
            }
        }
        self.server.borrow_mut().pump();
    }

    fn run(&self, seconds: f64) {
        for _ in 0..(seconds * 50.0) as usize {
            self.step(0.02, true);
        }
    }

    fn handle(&self, i: usize) -> crate::battle::live::LiveHandle {
        let host = self.host.clone();
        crate::battle::live::LiveHandle {
            room: self.rooms[i].clone(),
            wall_ms: Rc::new(move || wall(*host.borrow())),
        }
    }
}

/// The owner picks beatucada's chart `chart` (attacks as `attacks`), the song is
/// shared, everyone prepares it as a shell does and readies; the
/// countdown runs. Each device's stage order (none for one that sits it
/// out).
fn pick_and_ready(r: &Room, song: &Song, attacks: bool, chart: usize) -> Vec<Option<StageOrder>> {
    r.run(1.0);
    for room in &r.rooms {
        assert!(room.borrow().plays_live(), "plays live");
        assert!(room.borrow().room_now().is_some(), "knows the room's clock");
    }
    let mut rules = r.rooms[0].borrow().rules().clone();
    rules.set_attacks(attacks);
    rules.stages = 1;
    r.rooms[0].borrow_mut().save_rules(rules).unwrap();
    r.run(0.5);
    let e = entry_at(song, chart);
    r.rooms[0].borrow_mut().pick(&e, &e.charts[0]).unwrap();
    r.run(0.5);
    let uploads = r.rooms[0].borrow_mut().take_upload_jobs();
    for j in uploads {
        r.server
            .borrow_mut()
            .authority(Command::SetContentStatus {
                stage: j.stage,
                status: ContentStatus::Available,
                share: Some("abc12345".into()),
            })
            .unwrap();
        r.rooms[0]
            .borrow_mut()
            .upload_finished(j.stage, Ok("abc12345".into()));
    }
    r.run(0.5);
    for room in &r.rooms {
        let jobs = room.borrow_mut().take_load_jobs();
        for job in jobs {
            let prepared = prepare_stage(&job, song, &mut |e| panic!("{e}")).unwrap();
            if job.share.is_some() {
                let picked = Picked {
                    song: SongEntry {
                        id: job.song.clone(),
                        ..e.clone()
                    },
                    chart: e.charts[0].clone(),
                };
                room.borrow_mut()
                    .finish_shared_load_live(&job, Ok((picked, prepared.live)));
            } else {
                room.borrow_mut().finish_load_live(&job, Ok(prepared.live));
            }
        }
    }
    r.run(1.0);
    let orders: Vec<Option<StageOrder>> = r
        .rooms
        .iter()
        .map(|room| room.borrow_mut().take_stage_order())
        .collect();
    for room in &r.rooms {
        if room.borrow().in_stage(orders[0].as_ref().unwrap().stage) {
            room.borrow_mut().set_ready(true).unwrap();
        }
    }
    orders
}

/// Each player's play (a session on the shared fake clock), started once
/// the room started the stage, as the prompt starts it: `bias`, its
/// autoplayer's lateness (seconds: far enough, every note missed).
fn sessions(
    r: &Room,
    song: &Song,
    orders: &[Option<StageOrder>],
    bias: &[f64],
) -> Vec<PlaySession<FakeAudio, FakeInputs>> {
    let stage = orders[0].as_ref().unwrap().stage;
    for _ in 0..400 {
        if r.rooms[0].borrow().stage_started(stage) {
            break;
        }
        r.step(0.02, true);
    }
    assert!(r.rooms[0].borrow().stage_started(stage), "the room started it");
    let settings = Settings::default();
    orders
        .iter()
        .enumerate()
        .filter_map(|(i, o)| Some((i, o.as_ref()?)))
        .map(|(i, order)| {
            let mut config = SessionConfig::from_settings(&settings, true, None);
            config.auto_bias = bias.get(i).copied().unwrap_or(0.0);
            config.opponents = Some(StageSetup::of(order));
            config.live = Some(r.handle(i));
            PlaySession::start(
                song,
                order.chart.index,
                &settings,
                config,
                FakeAudio::new(&r.host, 0.02),
                110.0,
                FakeInputs::new(&r.host),
            )
            .unwrap()
        })
        .collect()
}

/// The plays run frame by frame on the shared clock until each ended
/// (its event and outcome kept), the room's clients ticked by the plays
/// themselves (as a play does), the room's afterwards.
#[allow(clippy::type_complexity)]
fn play_out(
    r: &Room,
    plays: &mut [PlaySession<FakeAudio, FakeInputs>],
    limit: f64,
    mut each: impl FnMut(f64, &mut [PlaySession<FakeAudio, FakeInputs>]),
) -> Vec<Option<(SessionEvent, crate::battle::StageOutcome)>> {
    let mut ended: Vec<Option<(SessionEvent, crate::battle::StageOutcome)>> =
        plays.iter().map(|_| None).collect();
    let start = *r.host.borrow();
    while ended.iter().any(Option::is_none) && *r.host.borrow() - start < limit {
        r.step(1.0 / 60.0, false);
        let now = *r.host.borrow();
        for (i, s) in plays.iter_mut().enumerate() {
            if ended[i].is_some() {
                continue;
            }
            let (_, event) = s.tick(HostTime(now));
            if let Some(e) = event {
                let o = s.take_stage_outcome().expect("its outcome");
                ended[i] = Some((e, o));
            }
        }
        each(now - start, plays);
    }
    ended
}

/// Two players and a watcher in a live room with attacks: both plays start
/// at the room's instant within a frame of each other, each streams its
/// moments and reports and draws the other's field from the room's
/// frames (a second behind), the room's attacks go into the plays and
/// onto the fields, the one who misses everything goes out, the room
/// decides the stage early, the winner plays the song out, both report
/// their runs and the room ends the song.
#[test]
fn a_live_stage_is_started_streamed_attacked_and_decided_by_the_room() {
    let song = bundled("beatucada");
    let r = Room::new(
        &[("alice", Intent::Play), ("bob", Intent::Play)],
        Some(Latency {
            down_ms: 30,
            down_jitter_ms: 60,
            up_ms: 30,
            up_jitter_ms: 60,
            seed: 5,
        }),
    );
    let orders = pick_and_ready(&r, &song, true, CHART);
    assert!(orders.iter().all(Option::is_some), "both play it");
    let o = orders[0].as_ref().unwrap();
    assert!(o.live.as_ref().unwrap().prep.as_ref().unwrap().plan.is_some());
    // Bob far late on every note: he misses all and goes out.
    let mut plays = sessions(&r, &song, &orders, &[0.0, 5.0]);
    let zero = r.rooms[0].borrow().song_zero_at(o.stage).unwrap();
    for p in &plays {
        let b = p.battle().unwrap();
        assert!(b.is_live());
        assert_eq!(b.views().len(), 1, "the other one's field");
    }
    let mut seen_other = [false, false];
    let mut in_step = true;
    let mut ended = play_out(&r, &mut plays, 200.0, |t, plays| {
        // A second in: both plays at the same song second (the room's
        // instant), whatever their clocks.
        if (1.0..1.05).contains(&t) {
            let a = plays[0].player.judge_time(HostTime(*r.host.borrow()));
            let b = plays[1].player.judge_time(HostTime(*r.host.borrow()));
            in_step &= (a - b).abs() < 0.02;
            let song_t = (wall(*r.host.borrow()) as f64 - zero.0 as f64) / 1_000.0;
            in_step &= (a - song_t).abs() < 0.05;
        }
        for (i, p) in plays.iter().enumerate() {
            if let Some(v) = p.battle().map(|b| b.views()) {
                seen_other[i] |= v.first().is_some_and(|v| v.combo > 0 || v.out);
            }
        }
    });
    assert!(in_step, "started at the room's instant");
    assert!(seen_other[1], "Bob saw Alice's field move");
    let (alice_event, alice) = ended[0].take().expect("Alice's play ended");
    let (_, bob) = ended[1].take().expect("Bob's play ended");
    assert!(matches!(alice_event, SessionEvent::Finished { .. }));
    assert!(alice.confirmed_early, "decided by the room before the song's end");
    assert_eq!(alice.ranking[0][0].participant, r.rooms[0].borrow().me());
    assert!(matches!(bob.mine, ddi_room::Run::Out { .. }));
    // The runs reported, as the shells report them.
    for (i, o) in [alice, bob].into_iter().enumerate() {
        r.rooms[i]
            .borrow_mut()
            .report_played(o.stage, o.mine, o.ranking, o.confirmed_early, o.details)
            .unwrap();
    }
    r.run(3.0);
    let s = r.server.borrow().state().clone();
    let play = s.stage(o.stage).unwrap().play.clone().unwrap();
    assert!(play.song_ended, "{:?}", s.phase);
    assert_eq!(play.result.unwrap().placements[0].participant, r.rooms[0].borrow().me());
    // The streams ended as they did, the room's ledger ran.
    let server = r.server.borrow();
    let stage = server.live().unwrap().stage_or_last().unwrap();
    assert_eq!(stage.end(r.rooms[0].borrow().me()), Some(StreamEnd::Finished));
    assert!(matches!(stage.end(r.rooms[1].borrow().me()), Some(StreamEnd::Out { .. })));
    assert!(stage.ledger().is_some_and(|l| !l.record().is_empty()));
}

/// Two players who both play the song out in a live room with attacks:
/// the room's ledger commits attacks both ways, each goes into its
/// recipient's play (none refused on a sound network) and onto the
/// other's view of that field, the same shocks everywhere; nothing is
/// decided early, both stream to the end and the room ranks the reports.
#[test]
fn the_rooms_attacks_go_into_the_plays_and_onto_the_fields() {
    let song = bundled("beatucada");
    let r = Room::new(
        &[("alice", Intent::Play), ("bob", Intent::Play)],
        Some(Latency {
            down_ms: 40,
            down_jitter_ms: 80,
            up_ms: 40,
            up_jitter_ms: 80,
            seed: 9,
        }),
    );
    // The Challenge chart: enough notes for full bars.
    let orders = pick_and_ready(&r, &song, true, 0);
    let stage = orders[0].as_ref().unwrap().stage;
    let mut plays = sessions(&r, &song, &orders, &[0.0, 0.01]);
    let mut ended = play_out(&r, &mut plays, 200.0, |_, _| {});
    let outcomes: Vec<_> = ended
        .iter_mut()
        .map(|e| e.take().expect("ended").1)
        .collect();
    let ids: Vec<_> = r.rooms.iter().map(|x| x.borrow().me()).collect();
    let server = r.server.borrow();
    let live = server.live().unwrap().stage_or_last().unwrap();
    let attacks = live.ledger().unwrap().attacks().to_vec();
    assert!(attacks.len() >= 2, "attacks both ways: {attacks:?}");
    for (i, p) in plays.iter().enumerate() {
        let b = p.battle().unwrap();
        assert_eq!(b.refused(), 0, "none refused");
        let summary = b.attack_summary(&p.player).unwrap();
        let taken: u32 = summary.taken.iter().map(|(_, n)| n).sum();
        let sent: u32 = summary.sent.iter().map(|(_, n)| n).sum();
        let against = attacks.iter().filter(|a| a.recipient == ids[i]).count() as u32;
        let by = attacks.iter().filter(|a| a.sender == ids[i]).count() as u32;
        assert_eq!((taken, sent), (against, by), "as the room's ledger has them");
        // The other's field carries the shocks put into their play.
        let other = 1 - i;
        let shocks = b.feeds().next().unwrap().shocks().len();
        let into_other = attacks.iter().filter(|a| a.recipient == ids[other]).count() * 8;
        assert_eq!(shocks, into_other);
        assert!(!outcomes[i].confirmed_early);
    }
    drop(server);
    for (i, o) in outcomes.into_iter().enumerate() {
        r.rooms[i]
            .borrow_mut()
            .report_played(o.stage, o.mine, o.ranking, o.confirmed_early, o.details)
            .unwrap();
    }
    r.run(3.0);
    let s = r.server.borrow().state().clone();
    assert!(s.stage(stage).unwrap().play.as_ref().unwrap().song_ended);
    let server = r.server.borrow();
    let live = server.live().unwrap().stage_or_last().unwrap();
    for id in ids {
        assert_eq!(live.end(id), Some(StreamEnd::Finished));
    }
}
