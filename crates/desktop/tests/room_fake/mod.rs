//! An online room over the room model's fake room (`ddi_room::fake`: the
//! server's room in process; no network, no sign-in), for the tests that
//! draw its screens offscreen in the game's theme: each person's client
//! and song select with a UI of their own, the fake room's HTTP, the
//! sample library with digests, and the pictures kept
//! (`DDI_OFFSCREEN_PPM=<dir>`: `<scene>-<w>x<h>-<lang>.ppm`).
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use ddi_room::fake::{FakeSocket, SharedFake};
use ddi_room::{ClientId, Clock, Command, Identity, ManualClock, Role};
use ddi_ui::screens::data::{SelectData, SongEntry};
use ddi_ui::screens::room::{RoomCall, RoomShell};
use ddi_ui::screens::{Env, sample};
use ddi_ui::state::online::api::{ApiRequest, ApiResponse, Method};
use ddi_ui::state::online::{Http, Online, OnlineConfig, Platform, SharedOnline};
use ddi_ui::state::room::{Picked, RoomClient, SPOTLIGHT, SharedRoom, UploadJob};
use ddi_ui::{Ui, UiEvent};

pub const ROOM: &str = "Room000001";

/// The fake room's HTTP for one person: who they are, the room's name, a
/// socket's ticket, an operator's upload grant.
pub struct RoomHttp {
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
pub struct Person {
    pub room: SharedRoom,
    pub env: Env,
    pub ui: Ui,
    pub uploads: Rc<RefCell<Vec<UploadJob>>>,
}

/// Everyone's songs, to read a shared one from (as a shell reads the
/// room's share).
pub type Songs = Rc<Vec<SongEntry>>;

pub fn person(
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
pub fn library(from: usize, salt: u128) -> SelectData {
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

pub struct Scene {
    pub clock: ManualClock,
    pub server: SharedFake,
    pub people: Vec<Person>,
}

impl Scene {
    /// `seconds` pass: the room's clock, every client and every screen;
    /// the uploads asked for end as the server ends them.
    pub fn run(&mut self, seconds: f32) {
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

pub fn keep(name: &str, size: [u32; 2], lang: &str, pixels: &[u8]) {
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
pub fn drawn(pixels: &[u8], size: [u32; 2], [x0, y0, x1, y1]: [f32; 4]) -> bool {
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

/// A tap at the middle of the actor named `name` on `ui` (a mouse's),
/// then `seconds` of frames.
pub fn tap(ui: &mut Ui, name: &str, seconds: f32) {
    reveal(ui, name);
    let b = ui
        .bounds(name)
        .unwrap_or_else(|| panic!("no actor {name:?} with a box"));
    let v = ui.viewport();
    let ph = v.to_physical([(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0]);
    let pos = [ph[0] / v.ratio, ph[1] / v.ratio];
    for (phase, buttons) in [
        (ddi_ui::PointerPhase::Down, ddi_ui::event::buttons::PRIMARY),
        (ddi_ui::PointerPhase::Up, 0),
    ] {
        ui.handle(UiEvent::Pointer(ddi_ui::PointerEvent {
            id: 1,
            kind: ddi_ui::PointerKind::Mouse,
            phase,
            pos,
            buttons,
            modifiers: ddi_ui::Modifiers::default(),
            time: 0.0,
        }));
    }
    for _ in 0..(seconds * 60.0).round() as usize {
        ui.update(1.0 / 60.0);
    }
}

/// Scrolls `name` into view whole where a pop-up's rows hold it scrolled
/// away (the Rules pop-up's), by the wheel over the screen's middle, as
/// a person would before tapping it.
fn reveal(ui: &mut Ui, name: &str) {
    let whole = |ui: &Ui| match (ui.bounds(name), ui.visible_bounds(name)) {
        (Some(b), Some(v)) => b.iter().zip(v).all(|(a, b)| (a - b).abs() < 0.5),
        _ => ui.bounds(name).is_none(),
    };
    for dy in [40.0, -40.0] {
        for _ in 0..200 {
            if whole(ui) {
                return;
            }
            let v = ui.viewport();
            let mid = v.to_physical([v.screen.width / 2.0, v.screen.height / 2.0]);
            ui.handle(UiEvent::Wheel(ddi_ui::event::WheelEvent {
                pos: [mid[0] / v.ratio, mid[1] / v.ratio],
                delta: [0.0, dy],
                source: ddi_ui::event::WheelSource::Pixels,
                modifiers: ddi_ui::Modifiers::default(),
                time: 0.0,
            }));
            ui.update(1.0 / 60.0);
        }
    }
}

/// A key pressed and released on `ui`, then `seconds` of frames.
pub fn key(ui: &mut Ui, code: &str, seconds: f32) {
    use ddi_ui::event::KeyEvent;
    ui.handle(UiEvent::Key(KeyEvent::down(code, code)));
    ui.handle(UiEvent::Key(KeyEvent::up(code, code)));
    for _ in 0..(seconds * 60.0).round() as usize {
        ui.update(1.0 / 60.0);
    }
}
