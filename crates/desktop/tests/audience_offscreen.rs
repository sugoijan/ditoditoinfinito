//! A stage watched with a room's audience (`docs/plans/multiplayer-audience.md`),
//! as the desktop window runs it, on this machine's GPU offscreen and a
//! fake clock with no sound device: two members' scripted streams
//! through the fake room, a seat in its audience on a relay of the fake's
//! (its socket 60 ms away), the stage handed over by the seat's client and
//! drawn by the play host as the spectator view (the grid), its music and
//! fields the audience's delay behind the room's song time. Compared
//! structurally (two fields, both moving, the grid, the delay within the
//! audience's ceiling) and kept as a picture with `DDI_ROOM_SHOTS=<dir>`.
//!
//! Skipped where there is no GPU adapter (CI).

mod common;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_chart::{NoteKind, Song};
use ddi_desktop::gpu::{self, Gpu};
use ddi_desktop::play::host::{Device, Host};
use ddi_desktop::play::view::Formats;
use ddi_platform::HostTime;
use ddi_room::fake::{FakeServer, FakeSocket, Latency, SharedFake};
use ddi_room::live::{Moment, MomentKind};
use ddi_room::{Command, ContentStatus, Identity, Intent, ManualClock, RoomPolicy};
use ddi_ui::event::UiEvent;
use ddi_room::fake::FakeAudienceSocket;
use ddi_ui::state::audience::AudienceClient;

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


/// The audience join answered from the fake room's audience.
struct AudienceHttp {
    server: SharedFake,
    out: Vec<(u64, ApiResponse)>,
}

impl Http for AudienceHttp {
    fn start(&mut self, id: u64, req: ApiRequest) {
        let answer = match (req.method, req.path.as_str()) {
            (Method::Get, "/me") => ok(
                r#"{"player_id":"dev:dora","login":"dora","display_name":"dora","admin":false,"expires_at_ms":1}"#
                    .into(),
            ),
            // As the server answers it (`FakeServer::audience_join_answer`).
            (Method::Post, "/rooms/Room000001/audience") => {
                let latency = Latency {
                    down_ms: 60,
                    down_jitter_ms: 40,
                    up_ms: 60,
                    up_jitter_ms: 40,
                    seed: 7,
                };
                let body: serde_json::Value =
                    serde_json::from_str(req.body.as_deref().unwrap_or("{}")).unwrap();
                let (status, body) = self.server.borrow_mut().audience_join_answer(
                    "dev:dora",
                    body["invite"].as_str().unwrap_or_default(),
                    Some(latency),
                );
                assert_eq!(status, 200, "{body}");
                ok(body)
            }
            _ => panic!("unexpected {req:?}"),
        };
        self.out.push((id, answer));
    }

    fn poll(&mut self) -> Vec<(u64, ApiResponse)> {
        std::mem::take(&mut self.out)
    }
}

/// Every tap of chart `chart` hit, by its time (ms), with its combo.
fn taps(song: &Song, chart: usize) -> Vec<Moment> {
    let c = &song.charts[chart];
    let timing = c.timing(song);
    let mut combo = 0;
    c.notes
        .iter()
        .filter(|n| matches!(n.kind, NoteKind::Tap))
        .map(|n| {
            combo += 1;
            Moment {
                at_ms: (timing.seconds_at(n.tick) * 1_000.0).round() as i32,
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
            }
        })
        .collect()
}

#[test]
fn a_stage_watched_with_the_audience_is_drawn_behind_the_room() {
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
        RoomPolicy::default().with_live().with_audience(),
        server_clock.clone(),
    )
    .shared();
    server.borrow_mut().set_room_name("Friday night");
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
    let online = Online::new(
        Box::new(AudienceHttp {
            server: server.clone(),
            out: Vec::new(),
        }),
        OnlineConfig {
            platform: Platform::Desktop,
            site: "http://localhost:8080/".into(),
        },
    )
    .shared();
    online.borrow_mut().check();
    online.borrow_mut().poll(0);
    let audience = Rc::new(RefCell::new(AudienceClient::new(
        online,
        "Room000001.audienceAUDIENCEaudien01",
        Box::new(FakeAudienceSocket::new(server.clone())),
        None,
        wall(),
    )));
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
            audience.borrow_mut().tick(wall());
        }
    };
    for _ in 0..10 {
        run(0.5);
        if rooms.iter().all(|r| r.borrow().room_now().is_some()) {
            break;
        }
    }
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
    // Everyone prepares the song as a shell does (the audience from the
    // room's share as its view names it); the members ready.
    let picked = |id: &str| Picked {
        song: SongEntry {
            id: id.into(),
            ..entry.clone()
        },
        chart: entry.charts[0].clone(),
    };
    let mut readied = false;
    for _ in 0..40 {
        run(0.5);
        for room in &rooms {
            let jobs = room.borrow_mut().take_load_jobs();
            for job in jobs {
                let prepared = prepare_stage(&job, &song, &mut |e| panic!("{e}")).unwrap();
                if job.share.is_some() {
                    room.borrow_mut()
                        .finish_shared_load_live(&job, Ok((picked(&job.song), prepared.live)));
                } else {
                    room.borrow_mut().finish_load_live(&job, Ok(prepared.live));
                }
            }
            let _ = room.borrow_mut().take_stage_order();
        }
        let jobs = audience.borrow_mut().take_load_jobs();
        for job in jobs {
            let prepared = prepare_stage(&job, &song, &mut |e| panic!("{e}")).unwrap();
            // (The share's song is the bundled one here: the window reads
            // it by its library id rather than from a share's cache.)
            assert!(job.song.starts_with("shared:abc12345"));
            audience
                .borrow_mut()
                .finish_load(&job, Ok((picked("beatucada"), prepared.live)));
        }
        if !readied && rooms.iter().all(|r| r.borrow_mut().set_ready(true).is_ok()) {
            readied = true;
        }
        let stage = audience.borrow().current_stage();
        if stage.is_some_and(|s| rooms[0].borrow().stage_started(s)) {
            break;
        }
    }
    let stage = audience.borrow().current_stage().expect("the stage");
    assert!(rooms[0].borrow().stage_started(stage), "the room started it");
    let zero = rooms[0].borrow().song_zero_at(stage).unwrap();
    let scripted = taps(&song, chart);
    let mut sent = [0usize; 2];
    // The members' devices: their taps judged by now, streamed.
    let stream = |sent: &mut [usize; 2]| {
        for (i, room) in rooms.iter().enumerate() {
            let now = room
                .borrow()
                .room_now()
                .map_or(0, |n| n.0 as i64 - zero.0 as i64);
            let mut r = room.borrow_mut();
            if let Some(out) = r.live_stream(stage) {
                while let Some(m) = scripted.get(sent[i]).filter(|m| i64::from(m.at_ms) <= now) {
                    out.moment(*m);
                    sent[i] += 1;
                }
                out.judged_to(now.max(0) * 1_000);
            }
        }
    };
    let mut order = None;
    for _ in 0..200 {
        step(FRAME_NS);
        stream(&mut sent);
        audience.borrow_mut().tick(wall());
        order = audience.borrow_mut().take_stage_order();
        if order.is_some() {
            break;
        }
    }
    let order = order.expect("the audience's order");
    // A seat that arrives during the stage: not handed it by itself; its
    // page offers Watch, focused first, and Enter takes it into the stage
    // as it stands (the shell told of the press first). The seat that was
    // handed it is offered it again (back from it, as after Back).
    {
        use ddi_ui::screens::audience::{AudienceCall, AudienceScreen};
        let online = Online::new(
            Box::new(AudienceHttp {
                server: server.clone(),
                out: Vec::new(),
            }),
            OnlineConfig {
                platform: Platform::Desktop,
                site: "http://localhost:8080/".into(),
            },
        )
        .shared();
        online.borrow_mut().check();
        online.borrow_mut().poll(0);
        let late = Rc::new(RefCell::new(AudienceClient::new(
            online,
            "Room000001.audienceAUDIENCEaudien01",
            Box::new(FakeAudienceSocket::new(server.clone())),
            None,
            wall(),
        )));
        let calls: Rc<RefCell<Vec<&'static str>>> = Rc::default();
        let c = calls.clone();
        let jobs: Rc<RefCell<Vec<ddi_ui::state::room::LoadJob>>> = Rc::default();
        let j = jobs.clone();
        let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_lowercase());
        let (mut page_ui, page_env, _) = ddi_desktop::shell::theme_ui("start", fold);
        page_env.reduced_motion.set(true);
        page_ui.handle(UiEvent::Resize {
            size: [SIZE[0] as f32, SIZE[1] as f32],
            ratio: 1.0,
        });
        page_ui.push_over(AudienceScreen::new(
            page_env.clone(),
            late.clone(),
            Rc::new(move |call, _| {
                c.borrow_mut().push(match call {
                    AudienceCall::WatchPressed => "pressed",
                    AudienceCall::Watch(_) => "watch",
                    AudienceCall::Prepare(job) => {
                        j.borrow_mut().push(job);
                        "prepare"
                    }
                    _ => "other",
                })
            }),
        ));
        for _ in 0..200 {
            step(FRAME_NS);
            stream(&mut sent);
            audience.borrow_mut().tick(wall());
            let mut todo = late.borrow_mut().take_load_jobs();
            todo.extend(jobs.borrow_mut().drain(..));
            for job in todo {
                let prepared = prepare_stage(&job, &song, &mut |e| panic!("{e}")).unwrap();
                late.borrow_mut()
                    .finish_load(&job, Ok((picked("beatucada"), prepared.live)));
            }
            page_ui.set_clock(wall() as f64 / 1_000.0);
            page_ui.update(0.05);
            if late.borrow().watchable().is_some() {
                break;
            }
        }
        assert_eq!(late.borrow().watchable(), Some(stage), "the late seat may watch");
        for _ in 0..4 {
            page_ui.update(0.05);
        }
        assert!(!calls.borrow().contains(&"watch"), "never handed over by itself");
        assert!(
            page_ui.actor("Item.watch").is_some() || page_env.state.borrow().page_items.iter().any(|i| i.contains("watch")),
            "Watch on the page: {:?}",
            page_env.state.borrow().page_items
        );
        page_ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::down("Enter", "Enter")));
        page_ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::up("Enter", "Enter")));
        page_ui.update(0.05);
        let seen = calls.borrow().iter().filter(|c| **c != "prepare").copied().collect::<Vec<_>>();
        assert_eq!(seen, vec!["pressed", "watch"], "Watch focused first and pressed");
        // The first seat, handed it before: offered it again.
        assert_eq!(audience.borrow().watchable(), Some(stage));
        assert!(audience.borrow_mut().take_stage_order().is_none(), "not twice by itself");
        assert_eq!(audience.borrow_mut().watch_now().map(|o| o.stage), Some(stage));
    }
    // Handed to the play host as the audience's page hands it.
    ddi_desktop::room::set_audience_for_tests(stage, audience.clone(), wall.clone());
    *host.requested.borrow_mut() = Some(ddi_desktop::play::Request::stage(order));
    fn dev<'a>(device: &'a wgpu::Device, queue: &'a wgpu::Queue, formats: Formats) -> Device<'a> {
        Device {
            device,
            queue,
            formats,
        }
    }
    let mut waited = 0;
    // (A watched stage may start by itself while it loads: its prompt
    // is then never seen ready.)
    while !host.prompt_ready() && !host.playing() {
        host.poll(&mut ui, &env, Some(dev(&device, &queue, formats)), 48_000);
        ui.set_clock(wall() as f64 / 1_000.0);
        ui.update(0.05);
        step(FRAME_NS);
        stream(&mut sent);
        audience.borrow_mut().tick(wall());
        std::thread::sleep(std::time::Duration::from_millis(10));
        waited += 1;
        assert!(
            waited < 2_000,
            "the song was never read: {:?} {:?}",
            host.prompt_waiting(),
            host.playing()
        );
    }
    // Start (the prompt's countdown would; pressed here at once).
    if !host.playing() {
        ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::down("Enter", "Enter")));
        ui.handle(UiEvent::Key(ddi_ui::event::KeyEvent::up("Enter", "Enter")));
    }
    let mut waited = 0;
    while !host.playing() {
        step(FRAME_NS);
        stream(&mut sent);
        audience.borrow_mut().tick(wall());
        ui.set_clock(wall() as f64 / 1_000.0);
        ui.update(0.05);
        host.poll(&mut ui, &env, Some(dev(&device, &queue, formats)), 48_000);
        waited += 1;
        assert!(waited < 400, "the watched stage never started: {:?}", ui.layers());
    }
    let srgb = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        ..Default::default()
    });
    let plain = texture.create_view(&Default::default());
    let mut behind = Vec::new();
    let mut combos = [0u32; 2];
    let mut delays = Vec::new();
    let mut grid = false;
    let mut shot = false;
    let delay = audience.borrow().music_delay_ms(stage) as f64 / 1_000.0;
    for _ in 0..(20 * 30) {
        step(FRAME_NS);
        stream(&mut sent);
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
        let Some(play) = host.running() else {
            break;
        };
        let s = &play.session;
        let b = s.battle().expect("a stage");
        assert!(b.is_live() && !b.competes(), "watched, live");
        assert_eq!(b.views().len(), 2, "both members' fields");
        for (c, v) in combos.iter_mut().zip(b.views()) {
            *c = (*c).max(v.combo);
        }
        let t = s.player.judge_time(HostTime(now as f64 / 1e9));
        let song_t = (wall() as f64 - zero.0 as f64) / 1_000.0;
        if song_t > 3.0 {
            behind.push(song_t - t);
            delays.push(b.live_state().unwrap().delay_ms);
        }
        grid |= s.watch(now as f64 / 1e9).is_some_and(|w| w.focus.is_none());
        if !shot && song_t > 20.0 {
            shot = true;
            std::thread::sleep(std::time::Duration::from_millis(30));
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            ppm("audience-grid", &gpu::read_rgba(&device, &queue, &texture));
        }
    }
    assert!(grid, "the grid: nobody followed");
    assert!(combos.iter().all(|c| *c > 10), "both fields moved: {combos:?}");
    let (lo, hi) = behind
        .iter()
        .fold((f64::MAX, f64::MIN), |(l, h), x| (l.min(*x), h.max(*x)));
    eprintln!("audience: {lo:.3}..{hi:.3} s behind (music delay {delay}), fields {:.0} ms more at most", delays.iter().cloned().fold(0.0, f64::max));
    assert!(lo >= delay - 0.06 && hi <= delay + 0.35, "{lo}..{hi} for {delay}");
    let ceiling = ddi_room::audience::AUDIENCE_V0.presentation_delay_ms as f64;
    assert!(delays.iter().all(|d| d + hi * 1_000.0 <= ceiling), "{delays:?}");
    assert_eq!(audience.borrow().sockets(), 1);
}
