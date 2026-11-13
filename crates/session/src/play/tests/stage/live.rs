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
    /// Each device's audio clock (host seconds) and how many millionths
    /// fast it runs against the host's (and the room's) clock: its sound
    /// card's crystal.
    devices: Vec<(Clock, f64)>,
    /// The device each play of [`sessions`] runs on.
    play_devices: RefCell<Vec<usize>>,
}

impl Room {
    fn new(people: &[(&'static str, Intent)], latency: Option<Latency>) -> Room {
        Room::drifting(people, latency, &[])
    }

    /// [`Room::new`] with each device's audio clock `ppm[i]` millionths
    /// fast (negative: slow).
    fn drifting(people: &[(&'static str, Intent)], latency: Option<Latency>, ppm: &[f64]) -> Room {
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
        let devices = (0..people.len())
            .map(|i| {
                let ppm = ppm.get(i).copied().unwrap_or(0.0);
                let clock = if ppm == 0.0 {
                    host.clone()
                } else {
                    Rc::new(RefCell::new(0.0))
                };
                (clock, ppm)
            })
            .collect();
        Room {
            host,
            server_clock,
            server,
            rooms,
            devices,
            play_devices: RefCell::new(Vec::new()),
        }
    }

    /// Device `i`'s audio clock now (host seconds).
    fn device_now(&self, i: usize) -> f64 {
        self.devices
            .get(i)
            .map_or(*self.host.borrow(), |(c, _)| *c.borrow())
    }

    /// The device play `k` of [`sessions`] runs on.
    fn device_of_play(&self, k: usize) -> usize {
        self.play_devices.borrow().get(k).copied().unwrap_or(k)
    }

    /// Time passes by `dt` seconds, the room's clients ticked (as their
    /// screens tick them outside a play).
    fn step(&self, dt: f64, clients: bool) {
        *self.host.borrow_mut() += dt;
        let host = *self.host.borrow();
        for (c, ppm) in &self.devices {
            if *ppm != 0.0 {
                *c.borrow_mut() = host * (1.0 + ppm / 1e6);
            }
        }
        let now = wall(host);
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
    pick_and_ready_with(r, song, attacks, chart, |_, _| {})
}

/// [`pick_and_ready`], each device's load job changed by `job` first (its
/// player's options, as their settings would have them).
fn pick_and_ready_with(
    r: &Room,
    song: &Song,
    attacks: bool,
    chart: usize,
    mut tweak: impl FnMut(usize, &mut ddi_ui::state::room::LoadJob),
) -> Vec<Option<StageOrder>> {
    for _ in 0..10 {
        r.run(0.5);
        if r.rooms.iter().all(|x| x.borrow().room_now().is_some()) {
            break;
        }
    }
    for room in &r.rooms {
        assert!(room.borrow().plays_live(), "plays live");
        assert!(room.borrow().room_now().is_some(), "knows the room's clock");
    }
    let mut rules = r.rooms[0].borrow().rules().clone();
    rules.set_attacks(attacks);
    rules.stages = 1;
    r.rooms[0].borrow_mut().save_rules(rules).unwrap();
    // (Long enough for the slowest latency tried: 400 ms each way.)
    r.run(2.0);
    let e = entry_at(song, chart);
    r.rooms[0].borrow_mut().pick(&e, &e.charts[0]).unwrap();
    r.run(2.0);
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
    // Each device's stage order taken as its screens take it: the moment
    // its view is the stage's prompt (`screens::room::Pump::open_prompt`),
    // which for the picker is as soon as the readiness window opens,
    // before its own load has finished.
    let mut orders: Vec<Option<StageOrder>> = r
        .rooms
        .iter()
        .map(|room| {
            let prompt = matches!(room.borrow().view(), ddi_ui::state::room::RoomView::Prompt(_));
            prompt.then(|| room.borrow_mut().take_stage_order()).flatten()
        })
        .collect();
    for (i, room) in r.rooms.iter().enumerate() {
        let jobs = room.borrow_mut().take_load_jobs();
        for mut job in jobs {
            tweak(i, &mut job);
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
        // A guest's prompt opens once its song is in, the room's plan not
        // known yet here (the picker's offer is still on its way).
        if orders[i].is_none() && matches!(room.borrow().view(), ddi_ui::state::room::RoomView::Prompt(_)) {
            orders[i] = room.borrow_mut().take_stage_order();
        }
    }
    for _ in 0..10 {
        r.run(0.5);
        for (o, room) in orders.iter_mut().zip(&r.rooms) {
            if o.is_none() {
                *o = room.borrow_mut().take_stage_order();
            }
        }
        if orders.iter().all(Option::is_some) {
            break;
        }
    }
    assert!(
        orders.iter().all(Option::is_some),
        "{:?} {:?}",
        orders.iter().map(Option::is_some).collect::<Vec<_>>(),
        r.server.borrow().state().phase
    );
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
    *r.play_devices.borrow_mut() = orders
        .iter()
        .enumerate()
        .filter_map(|(i, o)| o.as_ref().map(|_| i))
        .collect();
    orders
        .iter()
        .enumerate()
        .filter_map(|(i, o)| Some((i, o.as_ref()?)))
        .map(|(i, order)| session_for(r, song, order, i, bias.get(i).copied().unwrap_or(0.0)))
        .collect()
}

/// Person `i`'s play of `order`, started now (`bias`: its autoplayer's
/// lateness).
fn session_for(
    r: &Room,
    song: &Song,
    order: &StageOrder,
    i: usize,
    bias: f64,
) -> PlaySession<FakeAudio, FakeInputs> {
    let settings = Settings::default();
    let mut config = SessionConfig::from_settings(&settings, true, None);
    config.auto_bias = bias;
    config.opponents = Some(StageSetup::of(order));
    config.live = Some(r.handle(i));
    PlaySession::start(
        song,
        order.chart.index,
        &settings,
        config,
        FakeAudio::new(&r.devices[i].0, 0.02),
        110.0,
        FakeInputs::new(&r.devices[i].0),
    )
    .unwrap()
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
            let (_, event) = s.tick(HostTime(r.device_now(r.device_of_play(i))));
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
    // The picker's order was taken as its prompt opened, before its load
    // finished: what live play needs is read from its room client.
    assert!(o.live.as_ref().unwrap().prep.is_none());
    assert!(r.rooms[0].borrow().live_prep(o.stage).unwrap().plan.is_some());
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
    // Bob, once out, follows Alice large (Tab) and goes back (Escape).
    let mut followed: (Option<f64>, bool, bool) = (None, false, false);
    let mut ended = play_out(&r, &mut plays, 200.0, |t, plays| {
        let host = *r.host.borrow();
        if let Some(w) = plays[1].watch(host) {
            match followed.0 {
                None => {
                    followed.0 = Some(host);
                    plays[1].inputs.key(host + 0.01, "Tab", true);
                    plays[1].inputs.key(host + 0.05, "Tab", false);
                    plays[1].inputs.key(host + 1.0, "Escape", true);
                    plays[1].inputs.key(host + 1.05, "Escape", false);
                }
                Some(at) if host > at + 0.5 && host < at + 0.9 && w.focus == Some(0) => {
                    // Alice's field in his own's place, his own a
                    // miniature marked out.
                    let b = plays[1].battle().unwrap();
                    let f = plays[1].frame(HostTime(host));
                    let want = b.feeds().next().unwrap().shown(f.song_time);
                    followed.1 |= f.combo == want.combo && b.own_feed().shown(f.song_time).out;
                }
                Some(at) if host > at + 1.5 => followed.2 |= w.focus.is_none(),
                _ => {}
            }
        }
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
    assert!(followed.1 && followed.2, "Bob followed Alice and came back: {followed:?}");
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

/// A member who watches gets the live stage: the music from the room's
/// instant, every competitor's field fed by the frames (a second behind),
/// the room's attacks between them; nothing of their own is streamed.
#[test]
fn a_watcher_follows_the_live_stage_from_the_frames() {
    let song = bundled("beatucada");
    let r = Room::new(
        &[
            ("alice", Intent::Play),
            ("bob", Intent::Play),
            ("carol", Intent::Watch),
        ],
        None,
    );
    let orders = pick_and_ready(&r, &song, true, 0);
    let carol = orders[2].as_ref().expect("the watcher's order");
    assert!(carol.watching);
    let mut plays = sessions(&r, &song, &orders, &[0.0, 0.0, 0.0]);
    let mut most = [0u32; 2];
    let mut ended = play_out(&r, &mut plays, 200.0, |_, plays| {
        let views = plays[2].battle().unwrap().views();
        for (m, v) in most.iter_mut().zip(&views) {
            *m = (*m).max(v.combo);
        }
    });
    let (_, watched) = ended[2].take().expect("the watcher's play ended");
    assert!(watched.watched);
    let b = plays[2].battle().unwrap();
    assert_eq!(b.views().len(), 2, "both competitors' fields");
    assert!(most.iter().all(|c| *c > 50), "{most:?}");
    // Their fields carry the shocks the room committed against them.
    let server = r.server.borrow();
    let live = server.live().unwrap().stage_or_last().unwrap();
    let attacks = live.ledger().unwrap().attacks().to_vec();
    let ids: Vec<_> = r.rooms.iter().map(|x| x.borrow().me()).collect();
    for (i, feed) in b.feeds().enumerate() {
        let against = attacks.iter().filter(|a| a.recipient == ids[i]).count() * 8;
        assert_eq!(feed.shocks().len(), against);
    }
    assert_eq!(live.end(ids[2]), None, "the watcher streamed nothing");
    assert!(!live.plays_live(ids[2]));
}

/// A device that could not start on time joins late, up to the late join:
/// its play starts where the room's clock has the song, the notes it
/// missed judged as misses, its stream taken by the room; later than that
/// it does not play: it forfeits and watches the rest.
#[test]
fn a_late_device_joins_part_way_and_a_too_late_one_forfeits_and_watches() {
    let song = bundled("beatucada");
    let r = Room::new(
        &[
            ("alice", Intent::Play),
            ("bob", Intent::Play),
            ("carol", Intent::Play),
        ],
        None,
    );
    let orders = pick_and_ready(&r, &song, false, 0);
    let mut plays = sessions(&r, &song, &orders[..1], &[0.0]);
    let zero = r.rooms[0]
        .borrow()
        .song_zero_at(orders[0].as_ref().unwrap().stage)
        .unwrap();
    // Alice plays; Bob's and Carol's devices are late (their screens still
    // tick their clients).
    let mut late: Vec<PlaySession<FakeAudio, FakeInputs>> = Vec::new();
    for (who, at) in [(1usize, 4.7), (2usize, 7.0)] {
        while (wall(*r.host.borrow()) as f64 - zero.0 as f64) / 1_000.0 < at {
            r.step(1.0 / 60.0, false);
            let now = *r.host.borrow();
            let _ = plays[0].tick(HostTime(now));
            for (i, room) in r.rooms.iter().enumerate() {
                if i >= who {
                    room.borrow_mut().tick(wall(now));
                }
            }
            for p in &mut late {
                let _ = p.tick(HostTime(now));
            }
        }
        late.push(session_for(&r, &song, orders[who].as_ref().unwrap(), who, 0.0));
    }
    plays.extend(late);
    let now = *r.host.borrow();
    let song_now = (wall(now) as f64 - zero.0 as f64) / 1_000.0;
    // Bob: in, part-way, the notes before missed.
    let t = plays[1].player.judge_time(HostTime(now));
    assert!(t > 4.0 && t <= song_now + 0.5, "{t} {song_now}");
    assert!(plays[1].battle().unwrap().me_in(&plays[1].player));
    let before = plays[1]
        .player
        .judge()
        .notes()
        .iter()
        .filter(|n| n.judgeable && n.seconds < t - 0.3)
        .count() as u32;
    // (Beatucada's first note comes after the late join's bound: what
    // came before is checked where there is some.)
    assert!(
        plays[1].player.tally().taps[5] >= before,
        "missed what came before: {} of {before}",
        plays[1].player.tally().taps[5]
    );
    // Carol: past the late join, a forfeit, watching.
    assert!(plays[2].battle().unwrap().forfeited());
    let mut ended = play_out(&r, &mut plays, 200.0, |_, _| {});
    let (_, carol) = ended[2].take().unwrap();
    assert_eq!(carol.mine, ddi_room::Run::Forfeit);
    let server = r.server.borrow();
    let live = server.live().unwrap().stage_or_last().unwrap();
    let ids: Vec<_> = r.rooms.iter().map(|x| x.borrow().me()).collect();
    assert_eq!(live.end(ids[1]), Some(StreamEnd::Finished), "Bob's stream taken");
    assert!(
        matches!(live.end(ids[2]), Some(StreamEnd::Forfeit | StreamEnd::NoShow)),
        "{:?}",
        live.end(ids[2])
    );
}

/// The taps of `who`'s stream as `reader` received it.
fn taps_read(r: &Room, reader: usize, who: ddi_room::ParticipantId) -> Vec<ddi_room::live::Moment> {
    let room = r.rooms[reader].borrow();
    room.live_link()
        .unwrap()
        .reader()
        .unwrap()
        .moments(who)
        .iter()
        .filter(|m| matches!(m.kind, ddi_room::live::MomentKind::Tap { .. }))
        .copied()
        .collect()
}

/// A player's socket dropped mid-stage (the network: no close frame): the
/// play goes on, the client reconnects, sends every batch not acknowledged
/// and resumes the frames, so that everyone's view of the stage ends whole:
/// every tap the play judged in the others' streams, the same as someone
/// who never lost a frame; frames lost on the way down asked for again.
#[test]
fn a_socket_lost_mid_stage_resends_and_resumes_the_stage_whole() {
    let song = bundled("beatucada");
    let r = Room::new(
        &[
            ("alice", Intent::Play),
            ("bob", Intent::Play),
            ("carol", Intent::Watch),
        ],
        Some(Latency {
            down_ms: 20,
            down_jitter_ms: 100,
            up_ms: 20,
            up_jitter_ms: 100,
            seed: 3,
        }),
    );
    let orders = pick_and_ready(&r, &song, true, 0);
    let mut plays = sessions(&r, &song, &orders, &[0.0, 0.02, 0.0]);
    let ids: Vec<_> = r.rooms.iter().map(|x| x.borrow().me()).collect();
    let mut dropped = false;
    let server = r.server.clone();
    let _ = play_out(&r, &mut plays, 200.0, |t, _| {
        if t > 30.0 && !dropped {
            dropped = true;
            server.borrow_mut().drop_socket(ids[1]);
            // Frames lost on their way to Alice too.
            for f in [ddi_room::fake::Fault::Drop, ddi_room::fake::Fault::Drop] {
                server.borrow_mut().fault(ids[0], f);
            }
        }
    });
    assert!(dropped);
    r.run(3.0);
    let bob_taps: u32 = plays[1].player.tally().taps.iter().sum();
    let alice_taps: u32 = plays[0].player.tally().taps.iter().sum();
    assert_eq!(taps_read(&r, 0, ids[1]).len() as u32, bob_taps, "Alice saw every tap of Bob's");
    assert_eq!(taps_read(&r, 2, ids[1]).len() as u32, bob_taps, "so did Carol");
    assert_eq!(taps_read(&r, 1, ids[0]).len() as u32, alice_taps, "Bob saw every tap of Alice's");
    assert_eq!(taps_read(&r, 1, ids[0]), taps_read(&r, 2, ids[0]));
    let live = r.server.borrow();
    let stage = live.live().unwrap().stage_or_last().unwrap();
    assert_eq!(stage.end(ids[1]), Some(StreamEnd::Finished));
}

/// The room's object restarts mid-stage (`FakeServer::restart_live`, as
/// the server's may): its live part is lost, every batch is answered
/// `unknown_stage`. Each play keeps going, stops streaming, says so on the
/// status line (`Battle::live_lost`), and reports its run at its end; the
/// room settles the stage on the reports.
#[test]
fn a_stage_the_room_lost_in_a_restart_plays_on_and_settles_on_the_reports() {
    let song = bundled("beatucada");
    let r = Room::new(&[("alice", Intent::Play), ("bob", Intent::Play)], None);
    let orders = pick_and_ready(&r, &song, true, 0);
    let mut plays = sessions(&r, &song, &orders, &[0.0, 0.0]);
    let server = r.server.clone();
    let mut restarted = false;
    let mut lost_seen = [false, false];
    let mut ended = play_out(&r, &mut plays, 200.0, |t, plays| {
        if t > 20.0 && !restarted {
            restarted = true;
            server.borrow_mut().restart_live();
        }
        for (i, p) in plays.iter().enumerate() {
            lost_seen[i] |= p.battle().is_some_and(|b| b.live_lost());
        }
    });
    assert!(restarted);
    assert_eq!(lost_seen, [true, true], "both plays heard the stage was lost");
    let stage = orders[0].as_ref().unwrap().stage;
    for (i, e) in ended.iter_mut().enumerate() {
        let (event, o) = e.take().expect("the play ended");
        assert!(matches!(event, SessionEvent::Finished { .. }), "finished");
        assert!(!o.confirmed_early, "nothing decided live after the restart");
        r.rooms[i]
            .borrow_mut()
            .report_played(o.stage, o.mine, o.ranking, o.confirmed_early, o.details)
            .unwrap();
    }
    r.run(3.0);
    let s = r.server.borrow().state().clone();
    let play = s.stage(stage).unwrap().play.clone().unwrap();
    assert!(play.song_ended, "settled on the reports: {:?}", s.phase);
    assert_eq!(play.reports.len(), 2);
}

/// A commitment that reaches its target too late to be shown fairly (its
/// first shock's warning already due: here, every frame three seconds
/// late on its way to Bob) is refused by Bob's play and said in his
/// stream: nothing goes into his play, and the others' views take those
/// shocks off his field.
#[test]
fn an_attack_that_comes_too_late_is_refused_and_taken_off_the_views() {
    let song = bundled("beatucada");
    let r = Room::new(&[("alice", Intent::Play), ("bob", Intent::Play)], None);
    let orders = pick_and_ready(&r, &song, true, 0);
    let ids: Vec<_> = r.rooms.iter().map(|x| x.borrow().me()).collect();
    r.server.borrow_mut().set_latency(
        ids[1],
        Latency {
            down_ms: 3_000,
            down_jitter_ms: 0,
            up_ms: 10,
            up_jitter_ms: 0,
            seed: 1,
        },
    );
    let mut plays = sessions(&r, &song, &orders, &[0.0, 0.0]);
    let _ = play_out(&r, &mut plays, 200.0, |_, _| {});
    r.run(5.0);
    let server = r.server.borrow();
    let live = server.live().unwrap().stage_or_last().unwrap();
    let against_bob = live
        .ledger()
        .unwrap()
        .attacks()
        .iter()
        .filter(|a| a.recipient == ids[1])
        .count();
    assert!(against_bob > 0, "attacks against Bob");
    let bob = plays[1].battle().unwrap();
    assert_eq!(bob.refused() as usize, against_bob, "every one refused");
    assert_eq!(
        plays[1].player.results_brief().battle_mines.map_or(0, |b| b.hit),
        0
    );
    let alice = plays[0].battle().unwrap();
    assert_eq!(alice.refused(), 0);
    assert!(
        alice.feeds().next().unwrap().shocks().is_empty(),
        "taken off Bob's field"
    );
}

/// A bot's play (DEVELOPMENT TOOL: `ddi --mp-bots`, a CPU on a real-time
/// song clock, `crate::battle::live::LivePlay`) streams as a person's play
/// does: the person sees its field move, the room's attacks against it go
/// into its CPU's play, and its stream ends before its run.
#[test]
fn a_bots_cpu_play_streams_live_beside_a_person() {
    use crate::battle::live::LivePlay;
    let song = bundled("beatucada");
    let r = Room::new(&[("alice", Intent::Play), ("bot", Intent::Play)], None);
    let orders = pick_and_ready(&r, &song, true, 0);
    let mut plays = sessions(&r, &song, &orders[..1], &[0.0]);
    let order = orders[1].clone().unwrap();
    let mut cpu = ddi_engine::cpu::Opponent::with_skills(
        &song,
        0,
        crate::battle::stage_ruleset(&order.rules),
        9,
        3,
        Default::default(),
    )
    .unwrap();
    cpu.start(0.0);
    let mut bot = LivePlay::new(r.handle(1), &order, &song, 0).unwrap().expect("live");
    let zero = r.rooms[0].borrow().song_zero_at(order.stage).unwrap();
    let mut received = 0;
    let mut bot_done = false;
    let bot_id = order.me;
    let _ = play_out(&r, &mut plays, 200.0, |_, _| {
        let t = (wall(*r.host.borrow()) as f64 - zero.0 as f64) / 1_000.0;
        if bot_done {
            return;
        }
        cpu.advance(t);
        let judged = cpu.take_judged();
        bot.send(t, cpu.player(), &judged);
        let c = &mut cpu;
        let mut deliver = |cells: &[ddi_engine::battle::Hazard]| c.receive_hazards(cells).is_ok();
        received += bot
            .receive(t, &mut deliver)
            .iter()
            .filter(|x| x.1 == bot_id && x.2)
            .count();
        if cpu.finished() && t > cpu.player().end_time() + 1.0 {
            bot.end(StreamEnd::Finished, t);
            bot_done = true;
        }
    });
    r.run(3.0);
    let ids: Vec<_> = r.rooms.iter().map(|x| x.borrow().me()).collect();
    let server = r.server.borrow();
    let live = server.live().unwrap().stage_or_last().unwrap();
    let against = live
        .ledger()
        .unwrap()
        .attacks()
        .iter()
        .filter(|a| a.recipient == ids[1])
        .count();
    assert!(against > 0);
    assert_eq!(received, against, "into the bot's CPU play");
    assert_eq!(live.end(ids[1]), Some(StreamEnd::Finished));
    let seen = taps_read(&r, 0, ids[1]).len();
    assert!(seen > 100, "Alice saw the bot's taps: {seen}");
}

/// What became of the attacks against one competitor of a stage.
#[derive(Debug, Default)]
struct Received {
    /// Committed against them by the room's ledger.
    against: usize,
    taken: usize,
    late: usize,
    other: usize,
    /// Each one's spare time when it came (ms; negative: late).
    spare_ms: Vec<i64>,
}

impl Received {
    fn of(against: usize, incoming: &[crate::battle::live::Incoming]) -> Received {
        use crate::battle::live::Fate;
        Received {
            against,
            taken: incoming.iter().filter(|i| i.fate == Fate::Taken).count(),
            late: incoming.iter().filter(|i| i.fate == Fate::Late).count(),
            other: incoming
                .iter()
                .filter(|i| !matches!(i.fate, Fate::Taken | Fate::Late))
                .count(),
            spare_ms: incoming.iter().map(|i| i.spare_us() / 1_000).collect(),
        }
    }
}

/// A live stage of a person (`person`: their place in the room, the
/// others bots: CPUs at levels 6 to 8 streaming as `--mp-bots` does) at
/// `speed` (XMod), every message `latency` away, with attacks on
/// beatucada's Challenge chart: what became of the attacks against each.
fn person_among_bots(latency: Option<Latency>, person: usize, speed: f64, seed: u64) -> Vec<Received> {
    use crate::battle::live::LivePlay;
    let song = bundled("beatucada");
    let names = ["alice", "bob", "carol", "dave"];
    let people: Vec<(&'static str, Intent)> = names.iter().map(|n| (*n, Intent::Play)).collect();
    let r = Room::new(&people, latency);
    let settings = Settings {
        speed,
        ..Settings::default()
    };
    let orders = pick_and_ready_with(&r, &song, true, 0, |i, job| {
        if i == person
            && let Some(ask) = job.live.as_mut()
        {
            ask.options.scroll.speed = ddi_engine::scroll::SpeedMod::XMod(speed);
        }
    });
    let stage = orders[0].as_ref().unwrap().stage;
    for _ in 0..400 {
        if r.rooms[0].borrow().stage_started(stage) {
            break;
        }
        r.step(0.02, true);
    }
    assert!(r.rooms[0].borrow().stage_started(stage));
    let order = orders[person].clone().unwrap();
    let mut config = SessionConfig::from_settings(&settings, true, None);
    config.opponents = Some(StageSetup::of(&order));
    config.live = Some(r.handle(person));
    let mut plays = vec![
        PlaySession::start(
            &song,
            order.chart.index,
            &settings,
            config,
            FakeAudio::new(&r.host, 0.02),
            110.0,
            FakeInputs::new(&r.host),
        )
        .unwrap(),
    ];
    let zero = r.rooms[0].borrow().song_zero_at(stage).unwrap();
    struct Bot {
        cpu: ddi_engine::cpu::Opponent,
        live: LivePlay,
        done: bool,
    }
    let mut bots: Vec<(usize, Bot)> = (0..names.len())
        .filter(|i| *i != person)
        .map(|i| {
            let o = orders[i].clone().unwrap();
            let mut cpu = ddi_engine::cpu::Opponent::with_skills(
                &song,
                0,
                crate::battle::stage_ruleset(&o.rules),
                5 + i,
                seed + i as u64,
                Default::default(),
            )
            .unwrap();
            cpu.start(0.0);
            let live = LivePlay::new(r.handle(i), &o, &song, 0)
                .unwrap()
                .expect("the bot plays live");
            (i, Bot { cpu, live, done: false })
        })
        .collect();
    let _ = play_out(&r, &mut plays, 200.0, |_, _| {
        let t = (wall(*r.host.borrow()) as f64 - zero.0 as f64) / 1_000.0;
        for (_, b) in &mut bots {
            if b.done {
                continue;
            }
            b.cpu.advance(t);
            let judged = b.cpu.take_judged();
            b.live.send(t, b.cpu.player(), &judged);
            let c = &mut b.cpu;
            let mut deliver = |cells: &[ddi_engine::battle::Hazard]| c.receive_hazards(cells).is_ok();
            let _ = b.live.receive(t, &mut deliver);
            let over = c.out_at().is_some()
                || (c.finished() && t > c.player().end_time() + 1.0);
            if over {
                let end = match c.out_at() {
                    Some(at) => StreamEnd::Out {
                        at_us: (at * 1e6).round() as i64,
                    },
                    None => StreamEnd::Finished,
                };
                b.live.end(end, t);
                b.done = true;
            }
        }
    });
    r.run(3.0);
    let ids: Vec<_> = r.rooms.iter().map(|x| x.borrow().me()).collect();
    let server = r.server.borrow();
    let live = server.live().unwrap().stage_or_last().unwrap();
    let attacks = live.ledger().unwrap().attacks().to_vec();
    let against = |i: usize| attacks.iter().filter(|a| a.recipient == ids[i]).count();
    (0..names.len())
        .map(|i| {
            if i == person {
                Received::of(against(i), plays[0].battle().unwrap().live_incoming())
            } else {
                let b = &bots.iter().find(|(j, _)| *j == i).unwrap().1;
                Received::of(against(i), b.live.incoming())
            }
        })
        .collect()
}

/// The study behind the regression test below: a person among three bots
/// at 20, 80 and 200 ms each way (jitter as large again), as host and as
/// guest, at a fast and a slow speed, over seeds; what became of the
/// attacks against each printed.
/// `cargo test -p ddi-session --lib person_among_bots_study -- --ignored --nocapture`
#[test]
#[ignore]
fn person_among_bots_study() {
    for ms in [20u64, 80, 200] {
        for person in [0usize, 2] {
            for speed in [2.5, 0.5] {
                for seed in 1..=3u64 {
                    let latency = Latency {
                        down_ms: ms,
                        down_jitter_ms: ms,
                        up_ms: ms,
                        up_jitter_ms: ms,
                        seed,
                    };
                    let got = person_among_bots(Some(latency), person, speed, seed);
                    let line: Vec<String> = got
                        .iter()
                        .enumerate()
                        .map(|(i, g)| {
                            let min = g.spare_ms.iter().min().copied().unwrap_or(0);
                            format!(
                                "{}{}: {}/{} late {} other {} min spare {min} ms",
                                if i == person { "P" } else { "b" },
                                i,
                                g.taken,
                                g.against,
                                g.late,
                                g.other
                            )
                        })
                        .collect();
                    println!("{ms} ms, person {person}, x{speed}, seed {seed}: {}", line.join(" | "));
                }
            }
        }
    }
}

/// A person among bots at a slow home link's latency (200 ms each way and
/// as much again of jitter: up to 0.4 s one way, 0.8 s a round trip), as
/// the room's picker (whose prompt opens before their song is loaded) and
/// as a guest (whose prompt opens before the room's plan is known): the
/// room attacks them about as often as the bots (no fewer than the
/// least-attacked bot less one, and at least once), every attack goes
/// into their play, none is refused (the study above found 0.78 s to
/// spare at the least over these settings and seeds), and their stream
/// is the room's to the song's end. Before plays read what live play
/// needs at their start, the picker's play streamed nothing and was
/// never attacked (a no-show to the room).
#[test]
fn a_person_is_attacked_as_the_bots_are_at_a_slow_links_latency() {
    for person in [0usize, 2] {
        for seed in [1u64, 2] {
            let latency = Latency {
                down_ms: 200,
                down_jitter_ms: 200,
                up_ms: 200,
                up_jitter_ms: 200,
                seed,
            };
            let got = person_among_bots(Some(latency), person, 2.5, seed);
            let me = &got[person];
            let least_bot = got
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != person)
                .map(|(_, g)| g.against)
                .min()
                .unwrap();
            let all: Vec<_> = got.iter().map(|g| (g.taken, g.against, g.late)).collect();
            assert!(me.against >= 1 && me.against + 1 >= least_bot, "person {person}, seed {seed}: {all:?}");
            assert_eq!(me.taken, me.against, "person {person}, seed {seed}: {all:?}");
            for g in &got {
                assert_eq!((g.late, g.other), (0, 0), "person {person}, seed {seed}: {all:?}");
                assert_eq!(g.taken, g.against);
            }
        }
    }
}

/// The clocks' drift during a live stage (`ddi_room::live::slew`): two
/// players whose sound cards run 500 millionths fast and slow against the
/// room's clock, both autoplayed. Each play's own judging stays on its
/// audio clock (its judgments exactly those of the same plays without
/// drift), while its stream's and its views' times follow the room's
/// clock: each device's correction is its drift, the other's field shows
/// its taps where its notes are (its clock's shift as its taps say it),
/// and the view's delay is what it is without drift, never a stall.
#[test]
fn a_live_stage_keeps_the_room_s_clock_while_the_devices_drift_and_judges_as_without() {
    let song = bundled("beatucada");
    let lat = Some(Latency {
        down_ms: 40,
        down_jitter_ms: 30,
        up_ms: 40,
        up_jitter_ms: 30,
        seed: 9,
    });
    let people = [("alice", Intent::Play), ("bob", Intent::Play)];
    let run = |ppm: &[f64]| {
        let r = Room::drifting(&people, lat, ppm);
        let orders = pick_and_ready(&r, &song, false, CHART);
        let mut plays = sessions(&r, &song, &orders, &[0.0, 0.0]);
        let start = *r.host.borrow();
        let mut seen = Vec::new();
        let ended = play_out(&r, &mut plays, 400.0, |t, plays| {
            if (t * 60.0).round() as i64 % 60 == 0 {
                let states: Vec<(f64, f64, i64, u32)> = plays
                    .iter()
                    .map(|p| {
                        let b = p.battle().unwrap();
                        let l = b.live_state().unwrap();
                        let shift = b.remote_shift_us().first().copied().unwrap_or(0);
                        (l.slew_ms, l.delay_ms, shift, l.stalls)
                    })
                    .collect();
                seen.push((t, states));
            }
        });
        assert!(ended.iter().all(Option::is_some));
        let judged: Vec<String> = plays
            .iter()
            .map(|p| format!("{:?}", p.player.judge().notes()))
            .collect();
        (judged, seen, *r.host.borrow() - start)
    };
    let (still, steady, _) = run(&[]);
    let (drifting, seen, _) = run(&[500.0, -500.0]);
    assert_eq!(still, drifting, "the plays judged exactly as without drift");
    let (t, last) = seen.iter().rev().find(|(t, _)| *t > 60.0).unwrap().clone();
    // Alice's clock runs fast: her song time ahead of the room's, her
    // correction the drift's (room − own), about 0.5 ms a second.
    let want = 0.5 * t;
    assert!((last[0].0 + want).abs() < 8.0, "{t}: {:?}", last[0]);
    assert!((last[1].0 - want).abs() < 8.0, "{t}: {:?}", last[1]);
    // Each sees the other's taps on the other's clock: its shift is the
    // other's correction (µs) as it was over its last few taps (the
    // median of nine, some seconds back: a few ms behind it now).
    assert!((last[0].2 as f64 / 1e3 - last[1].0).abs() < 12.0, "{last:?}");
    assert!((last[1].2 as f64 / 1e3 - last[0].0).abs() < 12.0, "{last:?}");
    assert!(last[0].2 > 30_000 && last[1].2 < -30_000, "{last:?}");
    // The view's delay as without drift, and no stall.
    let steady_last = &steady.iter().rev().find(|(s, _)| *s <= t).unwrap().1;
    for (a, b) in last.iter().zip(steady_last) {
        assert!(a.1 <= b.1 + 40.0, "{a:?} against {b:?}");
        assert_eq!(a.3, 0, "{a:?}");
    }
}
