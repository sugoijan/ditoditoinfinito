//! A stage an online room plays live, as the desktop window runs it
//! (`docs/plans/multiplayer-online-play.md`), on this machine's GPU
//! offscreen and a fake clock with no sound device: the person's play
//! (autoplayed) at the room's instant through the fake room with latency,
//! against another device's scripted stream (`ddi_room::live`'s moments
//! sent as a device sends them): its hits, a hold held to its end and one
//! let go, a mine stepped on, a shock stepped on that locks its lane. The
//! remote field is compared structurally (what its frame draws: the
//! held hold pinned to the receptor, the dropped one scrolling on, the
//! mine gone with its receptor flashing, the lane's lock) and the view's
//! delay adapting below the live rules' second.
//!
//! Skipped where there is no GPU adapter (CI). `DDI_ROOM_SHOTS=<dir>`
//! keeps pictures (PPM) of the remote field at those moments.

mod common;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_chart::{NoteKind, Song};
use ddi_desktop::gpu::{self, Gpu};
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_engine::frame::SpriteKind;
use ddi_platform::HostTime;
use ddi_room::fake::{FakeServer, FakeSocket, Latency, SharedFake};
use ddi_room::live::{Moment, MomentKind};
use ddi_room::{Command, ContentStatus, Identity, Intent, ManualClock, RoomPolicy};
use ddi_ui::event::UiEvent;
use ddi_ui::screens::data::{ChartEntry, Difficulty, SongEntry};
use ddi_ui::state::online::api::{ApiRequest, ApiResponse, Method};
use ddi_ui::state::online::{Http, Online, OnlineConfig, Platform, SharedOnline};
use ddi_ui::state::room::{Picked, RoomClient, SharedRoom, prepare_stage};

const SIZE: [u32; 2] = [1280, 720];
/// The wall clock's ms at the fake clock's start.
const WALL: u64 = 1_700_000_000_000;
/// A frame of the test: 50 ms (the room's frames come every 100).
const FRAME_NS: u64 = 50_000_000;

/// HTTP answered at once from the fake room (who `who` is, the room's
/// summary, its tickets and upload grants), as the session's live tests
/// answer it.
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

/// The chart of Beatucada's in single with the most of what a person's
/// field draws: holds (two at least) and mines.
fn chart_with_holds_and_mines(song: &Song) -> usize {
    (0..song.charts.len())
        .filter(|&c| song.charts[c].layout == "dance-single")
        .max_by_key(|&c| {
            let n = &song.charts[c].notes;
            let holds = n.iter().filter(|x| matches!(x.kind, NoteKind::HoldHead { .. })).count();
            let mines = n.iter().filter(|x| x.kind == NoteKind::Mine).count();
            (usize::from(holds >= 2) + usize::from(mines >= 1), mines.min(5), holds.min(5))
        })
        .expect("a single chart")
}

/// The other device's stream, scripted: every tap hit (W1), the first
/// hold held to its end, the second let go just after its head, the first
/// mine stepped on, a shock stepped on in lane 2 at `shock_at` seconds.
fn script(song: &Song, chart: usize, shock_at: f64) -> (Vec<Moment>, [usize; 3]) {
    let c = &song.charts[chart];
    let timing = c.timing(song);
    let mut out = Vec::new();
    let (mut holds, mut mine) = (Vec::new(), None);
    let mut combo = 0;
    let at = |s: f64| (s * 1_000.0).round() as i32;
    for (i, n) in c.notes.iter().enumerate() {
        let s = timing.seconds_at(n.tick);
        let tap = |combo: u32| Moment {
            at_ms: at(s),
            tick: n.tick.0,
            lane: n.lane,
            kind: MomentKind::Tap {
                tier: 1,
                offset_ms: 0,
                w0: true,
            },
            life_ppm: 1_000_000,
            combo,
            score: i64::from(combo) * 10_100,
        };
        match n.kind {
            NoteKind::Tap | NoteKind::Lift => {
                combo += 1;
                out.push(tap(combo));
            }
            NoteKind::HoldHead { end } | NoteKind::RollHead { end } => {
                combo += 1;
                out.push(tap(combo));
                let ok = holds.is_empty();
                holds.push(i);
                if holds.len() <= 2 {
                    let end_at = if ok { timing.seconds_at(end) } else { s + 0.1 };
                    out.push(Moment {
                        at_ms: at(end_at),
                        kind: MomentKind::HoldEnd { ok },
                        ..tap(combo)
                    });
                }
            }
            NoteKind::Mine if mine.is_none() => {
                mine = Some(i);
                out.push(Moment {
                    kind: MomentKind::Mine,
                    life_ppm: 900_000,
                    ..tap(combo)
                });
            }
            _ => {}
        }
    }
    out.push(Moment {
        at_ms: at(shock_at),
        tick: 0,
        lane: 2,
        kind: MomentKind::Shock,
        life_ppm: 850_000,
        combo,
        score: 0,
    });
    out.sort_by_key(|m| m.at_ms);
    (out, [holds[0], holds[1], mine.unwrap_or(usize::MAX)])
}

fn ppm(name: &str, pixels: &[u8]) {
    let Some(dir) = std::env::var_os("DDI_ROOM_SHOTS") else {
        return;
    };
    let mut out = format!("P6\n{} {}\n255\n", SIZE[0], SIZE[1]).into_bytes();
    out.extend(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]]),
    );
    std::fs::write(std::path::Path::new(&dir).join(format!("{name}.ppm")), out).unwrap();
}

#[test]
fn a_live_stage_draws_a_persons_holds_mines_a_locked_lane_and_the_adaptive_delay() {
    common::indexes();
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let (ui_device, ui_queue) = gpu::device(&adapter).unwrap();
    let _ui_gpu = Gpu::new(ui_device, ui_queue, wgpu::TextureFormat::Rgba8Unorm);
    let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_lowercase());
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("start", fold);
    env.reduced_motion.set(true);
    ui.handle(UiEvent::Resize {
        size: [SIZE[0] as f32, SIZE[1] as f32],
        ratio: 1.0,
    });
    let mut host = Host::new(Rc::default(), None);
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    host.test_clock = Some(Arc::new(move || c.load(Ordering::SeqCst)));
    let formats = Formats::of(wgpu::TextureFormat::Rgba8Unorm, true);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: SIZE[0],
            height: SIZE[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
    });
    // The wall clock runs with the fake one (ms since the epoch).
    let wall: Rc<dyn Fn() -> u64> = {
        let c = clock.clone();
        Rc::new(move || WALL + (c.load(Ordering::SeqCst) - 1_000_000_000) / 1_000_000)
    };
    let server_clock = ManualClock::new(wall());
    let server = FakeServer::hosted(
        "Room000001",
        Identity::new("dev:alice"),
        "alice",
        RoomPolicy::default().with_live(),
        server_clock.clone(),
    )
    .shared();
    server
        .borrow_mut()
        .join(Identity::new("dev:bob"), "bob", Intent::Play)
        .unwrap();
    for (i, seed) in [(1u64, 3u64), (2, 5)] {
        server.borrow_mut().set_latency(
            ddi_room::ParticipantId(i),
            Latency {
                down_ms: 40,
                down_jitter_ms: 40,
                up_ms: 40,
                up_jitter_ms: 40,
                seed,
            },
        );
    }
    let rooms: Vec<SharedRoom> = ["alice", "bob"]
        .into_iter()
        .map(|who| {
            Rc::new(RefCell::new(RoomClient::online(
                person(&server, who),
                "Room000001",
                Box::new(FakeSocket::new(server.clone())),
                ddi_room::ClientId::new(format!("{who}-test")),
                wall(),
            )))
        })
        .collect();
    // Time passes for the room and its clients (the screens' ticks).
    let step = |ns: u64| {
        clock.fetch_add(ns, Ordering::SeqCst);
        server_clock.set(wall());
        for r in &rooms {
            r.borrow_mut().tick(wall());
        }
        server.borrow_mut().pump();
    };
    let run = |seconds: f64| {
        for _ in 0..(seconds * 20.0) as usize {
            step(FRAME_NS);
        }
    };
    for _ in 0..10 {
        run(0.5);
        if rooms.iter().all(|r| r.borrow().room_now().is_some()) {
            break;
        }
    }
    assert!(rooms.iter().all(|r| r.borrow().plays_live()));
    // The pick: one stage, no attacks; Bob's stream is scripted.
    let song = ddi_desktop::play::load::read_chart("beatucada").unwrap();
    let chart = chart_with_holds_and_mines(&song);
    let mut rules = rooms[0].borrow().rules().clone();
    rules.set_attacks(false);
    rules.stages = 1;
    rooms[0].borrow_mut().save_rules(rules).unwrap();
    run(1.0);
    let entry = SongEntry {
        id: "beatucada".into(),
        title: "Beatucada".into(),
        charts: vec![ChartEntry {
            style: "dance-single".into(),
            difficulty: Difficulty::Hard,
            index: chart,
            digest: Some(song.chart_digest(chart)),
            ..ChartEntry::default()
        }],
        ..SongEntry::default()
    };
    rooms[0].borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
    run(1.0);
    let uploads = rooms[0].borrow_mut().take_upload_jobs();
    for j in uploads {
        server
            .borrow_mut()
            .authority(Command::SetContentStatus {
                stage: j.stage,
                status: ContentStatus::Available,
                share: Some("abc12345".into()),
            })
            .unwrap();
        rooms[0].borrow_mut().upload_finished(j.stage, Ok("abc12345".into()));
    }
    run(0.5);
    let mut orders = vec![None, None];
    for _ in 0..12 {
        for (o, room) in orders.iter_mut().zip(&rooms) {
            let jobs = room.borrow_mut().take_load_jobs();
            for job in jobs {
                let prepared = prepare_stage(&job, &song, &mut |e| panic!("{e}")).unwrap();
                if job.share.is_some() {
                    let picked = Picked {
                        song: SongEntry {
                            id: job.song.clone(),
                            ..entry.clone()
                        },
                        chart: entry.charts[0].clone(),
                    };
                    room.borrow_mut()
                        .finish_shared_load_live(&job, Ok((picked, prepared.live)));
                } else {
                    room.borrow_mut().finish_load_live(&job, Ok(prepared.live));
                }
            }
            if o.is_none() {
                *o = room.borrow_mut().take_stage_order();
            }
        }
        if orders.iter().all(Option::is_some) {
            break;
        }
        run(0.5);
    }
    let orders: Vec<_> = orders.into_iter().map(|o| o.expect("a stage order")).collect();
    let stage = orders[0].stage;
    // Alice's play through the window's play host: its prompt (Ready),
    // the room's start, autoplayed.
    ddi_desktop::room::set_live_for_tests(rooms[0].clone(), wall.clone());
    let mut request = ddi_desktop::play::Request::stage(orders[0].clone());
    request.auto = true;
    *host.requested.borrow_mut() = Some(request);
    fn dev<'a>(device: &'a wgpu::Device, queue: &'a wgpu::Queue, formats: Formats) -> Device<'a> {
        Device {
            device,
            queue,
            formats,
        }
    }
    let mut waited = 0;
    while !host.prompt_ready() {
        host.poll(&mut ui, &env, Some(dev(&device, &queue, formats)), 48_000);
        ui.set_clock(wall() as f64 / 1_000.0);
        ui.update(0.05);
        step(FRAME_NS);
        std::thread::sleep(std::time::Duration::from_millis(10));
        waited += 1;
        assert!(waited < 2_000, "the song was never read");
    }
    for r in &rooms {
        r.borrow_mut().set_ready(true).unwrap();
    }
    let mut waited = 0;
    while !host.playing() {
        step(FRAME_NS);
        ui.set_clock(wall() as f64 / 1_000.0);
        ui.update(0.05);
        host.poll(&mut ui, &env, Some(dev(&device, &queue, formats)), 48_000);
        waited += 1;
        assert!(waited < 400, "the stage never started: {:?}", ui.layers());
    }
    let zero = rooms[1].borrow().song_zero_at(stage).unwrap();
    let shock_at = 12.0;
    let (moments, [held, dropped, mine]) = script(&song, chart, shock_at);
    let timing = song.charts[chart].timing(&song);
    let note_s = |i: usize| timing.seconds_at(song.charts[chart].notes[i].tick);
    let hold_end = |i: usize| match song.charts[chart].notes[i].kind {
        NoteKind::HoldHead { end } | NoteKind::RollHead { end } => timing.seconds_at(end),
        _ => unreachable!(),
    };
    let mut sent = 0;
    // What to look at, at song seconds as the remote field shows them.
    let mut seen = Seen::default();
    let srgb = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        ..Default::default()
    });
    let plain = texture.create_view(&Default::default());
    let mut frames = 0;
    let last = hold_end(held).max(note_s(dropped)).max(note_s(mine.min(held))).max(shock_at) + 4.0;
    loop {
        step(FRAME_NS);
        // Bob's device: his scripted moments judged by now, streamed.
        let bob_now = rooms[1]
            .borrow()
            .room_now()
            .map_or(0, |n| n.0 as i64 - zero.0 as i64);
        {
            let mut r = rooms[1].borrow_mut();
            if let Some(out) = r.live_stream(stage) {
                while let Some(m) = moments.get(sent).filter(|m| i64::from(m.at_ms) <= bob_now) {
                    out.moment(*m);
                    sent += 1;
                }
                out.judged_to(bob_now.max(0) * 1_000);
            }
        }
        let now = clock.load(Ordering::SeqCst);
        let mut encoder = device.create_command_encoder(&Default::default());
        ui.set_clock(wall() as f64 / 1_000.0);
        host.draw_play(
            &dev(&device, &queue, formats),
            &mut encoder,
            &srgb,
            Some(&plain),
            SIZE,
            HostTime(now as f64 / 1e9),
            &mut ui,
            &env,
        );
        queue.submit(Some(encoder.finish()));
        frames += 1;
        let play = host.running().expect("playing");
        let s = &play.session;
        let b = s.battle().expect("a stage");
        assert!(b.is_live());
        let feed = b.feeds().next().expect("Bob's field");
        let t = s.player.judge_time(HostTime(now as f64 / 1e9));
        let v = feed.view_time(t);
        let f = feed.frame(b.track(), s.player.options().scroll, t, 12.0);
        let delay = b.live_state().unwrap().delay_ms;
        seen.delays.push(delay);
        let kind_of = |tick: ddi_chart::Tick, lane: u8| {
            f.notes
                .iter()
                .find(|n| n.lane == lane && (f64::from(n.beat) - tick.beat()).abs() < 1e-3)
                .map(|n| (n.kind, n.y))
        };
        let note = |i: usize| &song.charts[chart].notes[i];
        let mut shot = None;
        // The held hold, mid-way: its head pinned to the receptor.
        let (hs, he) = (note_s(held), hold_end(held));
        if seen.held.is_none() && v > hs + (he - hs) * 0.4 && v < he - 0.05 {
            let k = kind_of(note(held).tick, note(held).lane);
            seen.held = Some(format!("{k:?}"));
            assert!(
                matches!(
                    k,
                    Some((SpriteKind::HoldHead { active: true, dropped: false, .. }
                        | SpriteKind::RollHead { active: true, dropped: false, .. }, y)) if y == 0.0
                ),
                "held: {k:?}"
            );
            assert!(f.receptors[usize::from(note(held).lane)].holding.is_some());
            shot = Some("live-hold-held");
        }
        // The one let go: dropped, scrolling on.
        let ds = note_s(dropped);
        if seen.dropped.is_none() && v > ds + 0.3 && v < hold_end(dropped) - 0.05 {
            let k = kind_of(note(dropped).tick, note(dropped).lane);
            seen.dropped = Some(format!("{k:?}"));
            assert!(
                matches!(
                    k,
                    Some((SpriteKind::HoldHead { dropped: true, .. } | SpriteKind::RollHead { dropped: true, .. }, _))
                ),
                "let go: {k:?}"
            );
            shot = Some("live-hold-dropped");
        }
        // The mine: drawn before it, gone once stepped on, its receptor
        // flashing.
        if mine != usize::MAX {
            let ms = note_s(mine);
            if seen.mine_before.is_none() && v > ms - 0.5 && v < ms - 0.1 {
                let k = kind_of(note(mine).tick, note(mine).lane);
                seen.mine_before = Some(format!("{k:?}"));
                assert!(matches!(k, Some((SpriteKind::Mine, _))), "a mine: {k:?}");
                shot = Some("live-mine");
            }
            if seen.mine_after.is_none() && v > ms + 0.02 && v < ms + 0.2 {
                assert!(kind_of(note(mine).tick, note(mine).lane).is_none());
                assert!(f.receptors[usize::from(note(mine).lane)].mine.is_some());
                seen.mine_after = Some("gone, flashing".into());
            }
        }
        // The shock's lane locked for the lock's time.
        if seen.locked.is_none() && v > shock_at + 0.1 && v < shock_at + 0.5 {
            let locks = feed.locks(b.track().lanes(), t);
            assert!(locks[2].is_some() && locks[0].is_none(), "{locks:?}");
            seen.locked = Some(format!("{locks:?}"));
            shot = Some("live-locked");
        }
        if let Some(name) = shot {
            // The fields' fonts are read on a thread: a moment for them.
            std::thread::sleep(std::time::Duration::from_millis(30));
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            ppm(name, &gpu::read_rgba(&device, &queue, &texture));
        }
        if v > last {
            break;
        }
        assert!(frames < 20 * 400, "the stage went on too long");
    }
    // The miniature beside the field: Bob's, one slot.
    assert_eq!(host.opponent_fields().unwrap().slots.len(), 1);
    eprintln!("seen: held {:?}, dropped {:?}, mine {:?}/{:?}, locked {:?}", seen.held, seen.dropped, seen.mine_before, seen.mine_after, seen.locked);
    assert!(seen.held.is_some() && seen.dropped.is_some() && seen.locked.is_some(), "{seen:?}");
    assert!(mine == usize::MAX || seen.mine_after.is_some(), "{seen:?}");
    // The view's delay adapted under the live rules' second, and settled
    // near half a second at 40 ms each way.
    let tail = &seen.delays[seen.delays.len() / 2..];
    let mean = tail.iter().sum::<f64>() / tail.len() as f64;
    assert!(mean < 600.0 && tail.iter().all(|d| *d < 1_000.0), "{mean}");
}

#[derive(Debug, Default)]
struct Seen {
    held: Option<String>,
    dropped: Option<String>,
    mine_before: Option<String>,
    mine_after: Option<String>,
    locked: Option<String>,
    delays: Vec<f64>,
}

/// CPUs in an online room (`ddi_room::live::cpus`, `online-cpus-v0`) as
/// the desktop window plays a live stage with them: the person's play
/// (autoplayed) beside another device's scripted stream and the match's
/// two CPUs, which this device computes: their miniatures drawn beside
/// Bob's (three slots), their fields fed from the computation (their
/// combos rising), their levels shown (stars, never Bot), and their
/// reports sent to the room, which takes them (no mismatch). The
/// pictures (`DDI_ROOM_SHOTS`): the play with the three miniatures.
#[test]
fn a_live_stage_with_cpus_draws_their_fields_from_this_devices_computation() {
    common::indexes();
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).unwrap();
    let (ui_device, ui_queue) = gpu::device(&adapter).unwrap();
    let _ui_gpu = Gpu::new(ui_device, ui_queue, wgpu::TextureFormat::Rgba8Unorm);
    let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_lowercase());
    let (mut ui, env, _) = ddi_desktop::shell::theme_ui("start", fold);
    env.reduced_motion.set(true);
    ui.handle(UiEvent::Resize {
        size: [SIZE[0] as f32, SIZE[1] as f32],
        ratio: 1.0,
    });
    let mut host = Host::new(Rc::default(), None);
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    host.test_clock = Some(Arc::new(move || c.load(Ordering::SeqCst)));
    let formats = Formats::of(wgpu::TextureFormat::Rgba8Unorm, true);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: SIZE[0],
            height: SIZE[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
    });
    let wall: Rc<dyn Fn() -> u64> = {
        let c = clock.clone();
        Rc::new(move || WALL + (c.load(Ordering::SeqCst) - 1_000_000_000) / 1_000_000)
    };
    let server_clock = ManualClock::new(wall());
    let server = FakeServer::hosted(
        "Room000001",
        Identity::new("dev:alice"),
        "alice",
        RoomPolicy::default().with_live(),
        server_clock.clone(),
    )
    .shared();
    server
        .borrow_mut()
        .join(Identity::new("dev:bob"), "bob", Intent::Play)
        .unwrap();
    let rooms: Vec<SharedRoom> = ["alice", "bob"]
        .into_iter()
        .map(|who| {
            Rc::new(RefCell::new(RoomClient::online(
                person(&server, who),
                "Room000001",
                Box::new(FakeSocket::new(server.clone())),
                ddi_room::ClientId::new(format!("{who}-test")),
                wall(),
            )))
        })
        .collect();
    let step = |ns: u64| {
        clock.fetch_add(ns, Ordering::SeqCst);
        server_clock.set(wall());
        for r in &rooms {
            r.borrow_mut().tick(wall());
        }
        server.borrow_mut().pump();
    };
    let run = |seconds: f64| {
        for _ in 0..(seconds * 20.0) as usize {
            step(FRAME_NS);
        }
    };
    for _ in 0..10 {
        run(0.5);
        if rooms.iter().all(|r| r.borrow().room_now().is_some()) {
            break;
        }
    }
    // One stage, attacks on, two CPUs of the roster.
    let song = ddi_desktop::play::load::read_chart("beatucada").unwrap();
    let chart = 4;
    let mut rules = rooms[0].borrow().rules().clone();
    rules.set_attacks(true);
    rules.stages = 1;
    rules.cpu.mode = ddi_room::CpuMode::Roster;
    rules.cpu.roster = [(2u8, "Ember Ratchet"), (7, "Mossy Accordion")]
        .into_iter()
        .enumerate()
        .map(|(i, (level, name))| ddi_room::OpponentSpec {
            level,
            seed: 90 + i as u64,
            personality: None,
            name: Some(name.into()),
            targeting: None,
        })
        .collect();
    rooms[0].borrow_mut().save_rules(rules).unwrap();
    run(1.0);
    let entry = SongEntry {
        id: "beatucada".into(),
        title: "Beatucada".into(),
        charts: vec![ChartEntry {
            style: "dance-single".into(),
            difficulty: Difficulty::Medium,
            index: chart,
            digest: Some(song.chart_digest(chart)),
            ..ChartEntry::default()
        }],
        ..SongEntry::default()
    };
    rooms[0].borrow_mut().pick(&entry, &entry.charts[0]).unwrap();
    run(1.0);
    let uploads = rooms[0].borrow_mut().take_upload_jobs();
    for j in uploads {
        server
            .borrow_mut()
            .authority(Command::SetContentStatus {
                stage: j.stage,
                status: ContentStatus::Available,
                share: Some("abc12345".into()),
            })
            .unwrap();
        rooms[0].borrow_mut().upload_finished(j.stage, Ok("abc12345".into()));
    }
    run(0.5);
    let mut orders = vec![None, None];
    for _ in 0..12 {
        for (o, room) in orders.iter_mut().zip(&rooms) {
            let jobs = room.borrow_mut().take_load_jobs();
            for job in jobs {
                let prepared = prepare_stage(&job, &song, &mut |e| panic!("{e}")).unwrap();
                if job.share.is_some() {
                    let picked = Picked {
                        song: SongEntry {
                            id: job.song.clone(),
                            ..entry.clone()
                        },
                        chart: entry.charts[0].clone(),
                    };
                    room.borrow_mut()
                        .finish_shared_load_live(&job, Ok((picked, prepared.live)));
                } else {
                    room.borrow_mut().finish_load_live(&job, Ok(prepared.live));
                }
            }
            if o.is_none() {
                *o = room.borrow_mut().take_stage_order();
            }
        }
        if orders.iter().all(Option::is_some) {
            break;
        }
        run(0.5);
    }
    let orders: Vec<_> = orders.into_iter().map(|o| o.expect("a stage order")).collect();
    let stage = orders[0].stage;
    assert_eq!(orders[0].cpus.len(), 2, "the roster seated");
    ddi_desktop::room::set_live_for_tests(rooms[0].clone(), wall.clone());
    let mut request = ddi_desktop::play::Request::stage(orders[0].clone());
    request.auto = true;
    *host.requested.borrow_mut() = Some(request);
    fn dev<'a>(device: &'a wgpu::Device, queue: &'a wgpu::Queue, formats: Formats) -> Device<'a> {
        Device {
            device,
            queue,
            formats,
        }
    }
    let mut waited = 0;
    while !host.prompt_ready() {
        host.poll(&mut ui, &env, Some(dev(&device, &queue, formats)), 48_000);
        ui.set_clock(wall() as f64 / 1_000.0);
        ui.update(0.05);
        step(FRAME_NS);
        std::thread::sleep(std::time::Duration::from_millis(10));
        waited += 1;
        assert!(waited < 2_000, "the song was never read");
    }
    for r in &rooms {
        r.borrow_mut().set_ready(true).unwrap();
    }
    let mut waited = 0;
    while !host.playing() {
        step(FRAME_NS);
        ui.set_clock(wall() as f64 / 1_000.0);
        ui.update(0.05);
        host.poll(&mut ui, &env, Some(dev(&device, &queue, formats)), 48_000);
        waited += 1;
        assert!(waited < 400, "the stage never started: {:?}", ui.layers());
    }
    let zero = rooms[1].borrow().song_zero_at(stage).unwrap();
    let (moments, _) = script(&song, chart, 30.0);
    let mut sent = 0;
    let srgb = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        ..Default::default()
    });
    let plain = texture.create_view(&Default::default());
    let mut shot = false;
    let mut most_combo = [0u32; 2];
    for frame in 0..(20 * 40) {
        step(FRAME_NS);
        let bob_now = rooms[1]
            .borrow()
            .room_now()
            .map_or(0, |n| n.0 as i64 - zero.0 as i64);
        {
            let mut r = rooms[1].borrow_mut();
            if let Some(out) = r.live_stream(stage) {
                while let Some(m) = moments.get(sent).filter(|m| i64::from(m.at_ms) <= bob_now) {
                    out.moment(*m);
                    sent += 1;
                }
                out.judged_to(bob_now.max(0) * 1_000);
            }
        }
        let now = clock.load(Ordering::SeqCst);
        let mut encoder = device.create_command_encoder(&Default::default());
        ui.set_clock(wall() as f64 / 1_000.0);
        host.draw_play(
            &dev(&device, &queue, formats),
            &mut encoder,
            &srgb,
            Some(&plain),
            SIZE,
            HostTime(now as f64 / 1e9),
            &mut ui,
            &env,
        );
        queue.submit(Some(encoder.finish()));
        let play = host.running().expect("playing");
        let b = play.session.battle().expect("a stage");
        let views = b.views();
        assert_eq!(views.len(), 3, "two CPUs and Bob");
        for (k, v) in views.iter().take(2).enumerate() {
            assert!(v.level.is_some(), "a CPU's stars");
            most_combo[k] = most_combo[k].max(v.combo);
        }
        if !shot && frame > 20 * 25 {
            shot = true;
            std::thread::sleep(std::time::Duration::from_millis(30));
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            ppm("live-cpus", &gpu::read_rgba(&device, &queue, &texture));
        }
    }
    assert_eq!(host.opponent_fields().unwrap().slots.len(), 3);
    assert!(most_combo.iter().all(|c| *c > 10), "their fields fed: {most_combo:?}");
    let server = server.borrow();
    let live = server.live().unwrap().stage().unwrap();
    let (messages, reports) = live.cpu_traffic();
    assert!(messages > 0 && reports > 0, "{messages} {reports}");
    assert_eq!(live.cpu_mismatches(), 0);
}
