//! An online room over the room model's fake room (`ddi_room::fake`: the
//! server's room in process; no network, no sign-in), for the tests that
//! draw its screens offscreen in the game's theme: each person's client
//! and song select with a UI of their own, the fake room's HTTP, the
//! sample library with digests, and the pictures kept
//! (`<scene>-<w>x<h>-<lang>.ppm`, [`super::shot`]).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use ddi_room::fake::{FakeSocket, HttpRoom, SharedFake};
use ddi_room::{ClientId, Clock, Command, ManualClock};
use ddi_ui::screens::data::{SelectData, SongEntry};
use ddi_ui::screens::room::{RoomCall, RoomShell};
use ddi_ui::screens::{Env, sample};
use ddi_ui::state::online::api::{ApiRequest, ApiResponse};
use ddi_ui::state::online::{Http, Online, OnlineConfig, Platform, SharedOnline};
use ddi_ui::state::room::{Picked, RoomClient, SPOTLIGHT, SharedRoom, UploadJob};
use ddi_ui::{Ui, UiEvent};

pub const ROOM: &str = "Room000001";

/// The fake room's HTTP for one person: who they are, the room's name, a
/// socket's ticket, an operator's upload grant. A live room's (as the
/// session's live tests answer it) is named Live, reads no invite and
/// grants anyone an upload.
pub struct RoomHttp {
    who: &'static str,
    server: SharedFake,
    out: VecDeque<(u64, ApiResponse)>,
    live: bool,
}

impl RoomHttp {
    pub fn new(server: &SharedFake, who: &'static str) -> RoomHttp {
        RoomHttp {
            who,
            server: server.clone(),
            out: VecDeque::new(),
            live: false,
        }
    }

    pub fn live(server: &SharedFake, who: &'static str) -> RoomHttp {
        RoomHttp {
            live: true,
            ..RoomHttp::new(server, who)
        }
    }
}

pub fn ok(body: String) -> ApiResponse {
    ApiResponse {
        status: 200,
        body,
        session: None,
    }
}

impl Http for RoomHttp {
    fn start(&mut self, id: u64, request: ApiRequest) {
        // As the server answers it (`FakeServer::answer_http`); a live
        // room's reads no invite.
        let room = HttpRoom {
            name: if self.live { "Live" } else { "After Hours" },
            grants_anyone: self.live,
        };
        let invite = request.path == format!("/rooms/{ROOM}/invite");
        let asked = (
            request.method.as_str(),
            request.path.as_str(),
            request.body.as_deref(),
        );
        let answered = match self.live && invite {
            true => None,
            false => self
                .server
                .borrow_mut()
                .answer_http(Some(self.who), room, asked, None),
        };
        let (status, body) =
            answered.unwrap_or_else(|| (403, r#"{"error":"not_an_operator"}"#.into()));
        self.out.push_back((
            id,
            ApiResponse {
                status,
                body,
                session: None,
            },
        ));
    }

    fn poll(&mut self) -> Vec<(u64, ApiResponse)> {
        self.out.drain(..).collect()
    }
}

/// `http`'s person signed in (their `/me` answered).
pub fn signed_in(http: RoomHttp) -> SharedOnline {
    let online: SharedOnline = Online::new(
        Box::new(http),
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
    let online = signed_in(RoomHttp::new(server, who));
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
                    // A song added to the room's list (revision 17).
                    if let Some(song) = job.entry {
                        let share = format!("song{:04x}", song.0);
                        let _ = self.server.borrow_mut().authority(Command::SetSongContent {
                            song,
                            status: ddi_room::ContentStatus::Available,
                            share: Some(share.clone()),
                            charts: None,
                        });
                        p.room.borrow_mut().song_upload_finished(song, Ok(share));
                        continue;
                    }
                    if job.stage == SPOTLIGHT {
                        let share = format!("share{:04x}", now & 0xffff);
                        let _ = self
                            .server
                            .borrow_mut()
                            .authority(Command::SetSpotlightShare {
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

/// A scene's picture kept: `<name>-<w>x<h>-<lang>.ppm` ([`super::shot`]).
pub fn keep(name: &str, size: [u32; 2], lang: &str, pixels: &[u8]) {
    super::shot(
        &format!("{name}-{}x{}-{lang}.ppm", size[0], size[1]),
        pixels,
        size,
    );
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
