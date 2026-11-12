//! One picker at a time on an online room's song select
//! (`docs/plans/multiplayer-prototype-progress.md`, "One picker at a
//! time"), drawn offscreen on this machine's GPU in the game's theme, over
//! the room model's fake room (`ddi_room::fake`: the server's room in
//! process; no network, no sign-in). Skipped where there is no GPU adapter
//! (CI).
//!
//! The room has its owner (alice), a co-host (bob) and a member (carol),
//! each with a client and a song select of their own. The pictures: the
//! screen before the room answers (nothing of a view), the owner's list,
//! the co-host's following view with Pick songs, the member's following
//! view without it, then both sides after the co-host took the pick.
//! `DDI_OFFSCREEN_PPM=<dir>` keeps them (`<scene>-<w>x<h>-<lang>.ppm`).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use ddi_desktop::gpu::{self, Gpu};
use ddi_room::fake::{FakeServer, FakeSocket, Fault, SharedFake};
use ddi_room::{
    ClientId, Clock, Command, Identity, Intent, ManualClock, ParticipantId, Role, RoomPolicy,
};
use ddi_ui::screens::data::{SelectData, SongEntry};
use ddi_ui::screens::room::{RoomCall, RoomShell};
use ddi_ui::screens::{Env, sample};
use ddi_ui::state::online::api::{ApiRequest, ApiResponse, Method};
use ddi_ui::state::online::{Http, Online, OnlineConfig, Platform, SharedOnline};
use ddi_ui::state::room::{Picked, RoomClient, SPOTLIGHT, SharedRoom, UploadJob};
use ddi_ui::{Ui, UiEvent};

const ROOM: &str = "Room000001";

/// The fake room's HTTP for one person: who they are, the room's name, a
/// socket's ticket, an operator's upload grant.
struct RoomHttp {
    who: &'static str,
    server: SharedFake,
    out: VecDeque<(u64, ApiResponse)>,
}

impl Http for RoomHttp {
    fn start(&mut self, id: u64, request: ApiRequest) {
        let ok = |body: String| ApiResponse {
            status: 200,
            body,
            session: None,
        };
        let no = |status: u16, code: &str| ApiResponse {
            status,
            body: format!(r#"{{"error":"{code}"}}"#),
            session: None,
        };
        let who = self.who;
        let identity = Identity::new(format!("dev:{who}"));
        let room = format!("/rooms/{ROOM}");
        let answer = match (request.method, request.path.as_str()) {
            (Method::Get, "/me") => ok(format!(
                r#"{{"player_id":"dev:{who}","login":"{who}","display_name":"{who}","admin":false,"expires_at_ms":1}}"#
            )),
            (Method::Get, p) if p == room => ok(format!(
                r#"{{"room_id":"{ROOM}","name":"After Hours","revision":1,"phase":"lobby","admission":"open","participant":1}}"#
            )),
            (Method::Post, p) if p == format!("{room}/ticket") => {
                match self.server.borrow_mut().ticket(&identity) {
                    Ok(t) => ok(format!(r#"{{"ticket":"{t}","expires_at_ms":9}}"#)),
                    Err(_) => no(403, "not_a_member"),
                }
            }
            (Method::Post, p) if p == format!("{room}/grant") => {
                let operator = self
                    .server
                    .borrow()
                    .state()
                    .member_by_identity(&identity)
                    .is_some_and(|m| m.admin || matches!(m.role, Role::Owner | Role::CoHost));
                if operator {
                    ok(format!(
                        r#"{{"grant":"grant-{who}","expires_at_ms":9,"max_bytes":100.0}}"#
                    ))
                } else {
                    no(403, "not_an_operator")
                }
            }
            _ => no(403, "not_an_operator"),
        };
        self.out.push_back((id, answer));
    }

    fn poll(&mut self) -> Vec<(u64, ApiResponse)> {
        self.out.drain(..).collect()
    }
}

/// One person in the room: their client, their song select and its UI.
struct Person {
    room: SharedRoom,
    env: Env,
    ui: Ui,
    uploads: Rc<RefCell<Vec<UploadJob>>>,
}

/// Everyone's songs, to read a shared one from (as a shell reads the
/// room's share).
type Songs = Rc<Vec<SongEntry>>;

fn person(
    server: &SharedFake,
    who: &'static str,
    data: SelectData,
    all: &Songs,
    size: [f32; 2],
) -> Person {
    let online: SharedOnline = Online::new(
        Box::new(RoomHttp {
            who,
            server: server.clone(),
            out: VecDeque::new(),
        }),
        OnlineConfig {
            platform: Platform::Web,
            site: "http://localhost:8080/".into(),
        },
    )
    .shared();
    online.borrow_mut().check();
    online.borrow_mut().poll(0);
    let room: SharedRoom = Rc::new(RefCell::new(RoomClient::online(
        online,
        ROOM,
        Box::new(FakeSocket::new(server.clone())),
        ClientId::new(format!("{who}-desktop")),
        1_000,
    )));
    let look = Rc::new(ddi_theme::Infinito::new());
    let env = Env::new(look.clone(), Rc::new(|s: &str| s.to_string()));
    env.reduced_motion.set(true);
    let uploads: Rc<RefCell<Vec<UploadJob>>> = Rc::default();
    let (shared, kept, songs) = (room.clone(), uploads.clone(), all.clone());
    let shell: RoomShell = Rc::new(move |call, cx| match call {
        RoomCall::Leave | RoomCall::Discard => cx.close(true),
        // The room's share of a song, read: the song with that chart.
        RoomCall::Prepare(job) if job.share.is_some() => {
            let found = songs.iter().find_map(|s| {
                let chart = s.charts.iter().find(|c| c.digest == job.digest)?;
                Some(Picked {
                    song: SongEntry {
                        id: job.song.clone(),
                        ..s.clone()
                    },
                    chart: chart.clone(),
                })
            });
            if let Some(picked) = found {
                shared.borrow_mut().finish_shared_load(&job, Ok(picked));
            }
        }
        RoomCall::Prepare(job) => shared.borrow_mut().finish_load(&job, Ok(())),
        RoomCall::Upload(job) => kept.borrow_mut().push(*job),
        RoomCall::PlayStage(_) | RoomCall::Redirect(_) => {}
    });
    let design = ddi_ui::screens::design_for(&*look, size);
    let mut ui = Ui::new(design, ddi_ui::TextMode::Own, false);
    ui.set_focus_ring(false);
    ui.push(sample::room_select(&env, room.clone(), data).with_room(room.clone(), shell));
    ui.handle(UiEvent::Resize { size, ratio: 1.0 });
    Person {
        room,
        env,
        ui,
        uploads,
    }
}

/// The sample library with every chart named by a digest of its own
/// (an online room names a chart by its digest), from song `from` on.
fn library(from: usize, salt: u128) -> SelectData {
    let mut data = sample::select();
    data.songs.rotate_left(from);
    for (i, song) in data.songs.iter_mut().enumerate() {
        for (k, chart) in song.charts.iter_mut().enumerate() {
            chart.digest = Some(ddi_chart::ChartDigest(
                salt + (i as u128) * 64 + k as u128 + 1,
            ));
        }
    }
    data
}

struct Scene {
    clock: ManualClock,
    server: SharedFake,
    people: Vec<Person>,
}

impl Scene {
    /// `seconds` pass: the room's clock, every client and every screen;
    /// the uploads asked for end as the server ends them.
    fn run(&mut self, seconds: f32) {
        for _ in 0..(seconds * 60.0).round() as usize {
            self.clock.advance(16);
            let now = self.clock.now().0;
            for p in &mut self.people {
                p.room.borrow_mut().tick(now);
                p.ui.update(1.0 / 60.0);
                let jobs: Vec<UploadJob> = p.uploads.borrow_mut().drain(..).collect();
                for job in jobs {
                    if job.stage == SPOTLIGHT {
                        let share = format!("share{:04x}", now & 0xffff);
                        let _ = self.server.borrow_mut().authority(Command::SetSpotlightShare {
                            chart: job.content.chart.clone(),
                            share: share.clone(),
                        });
                        p.room.borrow_mut().upload_finished(job.stage, Ok(share));
                    }
                }
            }
        }
    }
}

fn keep(name: &str, size: [u32; 2], lang: &str, pixels: &[u8]) {
    let Some(dir) = std::env::var_os("DDI_OFFSCREEN_PPM") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the pictures' folder");
    let mut ppm = format!("P6\n{} {}\n255\n", size[0], size[1]).into_bytes();
    ppm.extend(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]]),
    );
    let file = dir.join(format!("{name}-{}x{}-{lang}.ppm", size[0], size[1]));
    std::fs::write(file, ppm).expect("ppm");
}

/// Something is drawn in `[x0, y0, x1, y1]` (fractions of the picture):
/// its pixels are not all one color.
fn drawn(pixels: &[u8], size: [u32; 2], [x0, y0, x1, y1]: [f32; 4]) -> bool {
    let at = |x: u32, y: u32| {
        let i = ((y * size[0] + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let (w, h) = (size[0] as f32, size[1] as f32);
    let first = at((x0 * w) as u32, (y0 * h) as u32);
    ((y0 * h) as u32..(y1 * h) as u32)
        .step_by(3)
        .any(|y| ((x0 * w) as u32..(x1 * w) as u32).any(|x| at(x, y) != first))
}

#[test]
fn the_rooms_song_select_changes_hands() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    for (size, lang) in [
        ([1280u32, 720u32], "en"),
        ([1280, 720], "ja"),
        ([844, 390], "en"),
        ([844, 390], "ja"),
    ] {
        assert!(ddi_l10n::set_language(lang));
        let css = [size[0] as f32, size[1] as f32];
        let wide = size[0] >= 1000;
        let clock = ManualClock::new(1_000);
        let server = FakeServer::hosted(
            ROOM,
            Identity::new("dev:alice"),
            "alice",
            RoomPolicy::default(),
            clock.clone(),
        )
        .shared();
        for who in ["bob", "carol"] {
            server
                .borrow_mut()
                .join(Identity::new(format!("dev:{who}")), who, Intent::Play)
                .unwrap();
        }
        server
            .borrow_mut()
            .authority(Command::SetCoHost {
                participant: ParticipantId(2),
                on: true,
            })
            .unwrap();
        // The room's first answer to its owner is a moment on its way.
        server.borrow_mut().fault(ParticipantId(1), Fault::Delay(900));
        let (a, b, c) = (library(0, 0x1000), library(9, 0x9000), library(20, 0x20000));
        let all: Songs = Rc::new(a.songs.iter().chain(&b.songs).cloned().collect());
        let mut scene = Scene {
            clock,
            server: server.clone(),
            people: vec![
                person(&server, "alice", a, &all, css),
                person(&server, "bob", b, &all, css),
                person(&server, "carol", c, &all, css),
            ],
        };
        let mut shot = |scene: &mut Scene, who: usize, name: &str| -> Vec<u8> {
            let pixels = gpu.draw_offscreen(&mut scene.people[who].ui, size, format);
            keep(name, size, lang, &pixels);
            pixels
        };
        let view = |scene: &Scene, who: usize| {
            scene.people[who]
                .env
                .state
                .borrow()
                .room
                .clone()
                .expect("the room said what it shows")
        };
        let list = |scene: &Scene, who: usize| {
            scene.people[who]
                .ui
                .actor("Wheel")
                .is_some_and(|w| w.visible)
        };
        let button = |scene: &Scene, who: usize| scene.people[who].ui.actor("Room.TakePick").is_some();
        // Before the room answers: nothing of a view, the bars and a line.
        scene.run(0.7);
        assert!(!list(&scene, 0) && scene.people[0].ui.actor("RoomJoining").is_some());
        let joining = shot(&mut scene, 0, "0-joining");
        assert!(!drawn(&joining, size, [0.55, 0.3, 0.95, 0.42]), "no list yet");
        // The owner picks; their song rests and the others see it.
        scene.run(5.0);
        let (va, vb, vc) = (view(&scene, 0), view(&scene, 1), view(&scene, 2));
        assert!(va.host && !vb.host && !vc.host, "{lang} {size:?}");
        assert!(vb.can_take_pick && !vc.can_take_pick && !va.can_take_pick);
        assert!(list(&scene, 0) && !list(&scene, 1) && !list(&scene, 2));
        assert!(vb.spotlight.is_some() && vb.spotlight_song.is_some(), "{vb:?}");
        assert_eq!(button(&scene, 1), wide, "a phone's is in More");
        assert!(!button(&scene, 0) && !button(&scene, 2));
        let owner = shot(&mut scene, 0, "1-owner-list");
        assert!(drawn(&owner, size, [0.55, 0.3, 0.95, 0.6]), "the list");
        shot(&mut scene, 1, "2-cohost-following");
        shot(&mut scene, 2, "3-member-following");
        // The co-host takes the pick: their own list, the owner follows
        // their resting song, with the action.
        scene.people[1].room.borrow_mut().take_pick().unwrap();
        scene.run(5.0);
        let (va, vb, vc) = (view(&scene, 0), view(&scene, 1), view(&scene, 2));
        assert!(!va.host && vb.host && !vc.host, "{lang} {size:?}");
        assert!(va.can_take_pick && !vb.can_take_pick && !vc.can_take_pick);
        assert_eq!((va.picker, vc.picker), (Some(2), Some(2)));
        assert!(list(&scene, 1) && !list(&scene, 0));
        assert!(va.spotlight.is_some() && va.spotlight_song.is_some(), "{va:?}");
        assert_eq!(button(&scene, 0), wide);
        shot(&mut scene, 1, "4-cohost-list");
        shot(&mut scene, 0, "5-owner-following");
        shot(&mut scene, 2, "6-member-following");
    }
}
