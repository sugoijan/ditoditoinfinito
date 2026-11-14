//! From the multiplayer home into a private room, frame by frame
//! (`docs/plans/multiplayer-prototype-progress.md`, "Feedback round D"):
//! the home's Private room pressed signed in, the room made by the fake
//! server's HTTP after a moment, the room's song select pushed (desktop)
//! or a page of its own with the bars carried (the web), its first answer
//! a moment after that. Every frame is drawn offscreen on this machine's
//! GPU in the game's theme and kept with `DDI_OFFSCREEN_PPM=<dir>`
//! (`entry-<shell>-<motion>-<lag>-<frame>`). Each frame: the bars drawn
//! (never blank, never an empty footer), the home's column still while
//! the room is made, the room's song select never drawn as built (its
//! search's Clear hidden), its view coming in once; a quick room says
//! nothing of joining, a slow one does. Skipped where there is no GPU
//! adapter (CI).

mod room_fake;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use ddi_desktop::gpu::{self, Gpu};
use ddi_room::fake::{FakeServer, FakeSocket, Fault, SharedFake};
use ddi_room::{ClientId, Clock, Identity, ManualClock, ParticipantId, RoomPolicy};
use ddi_ui::screens::online::{OnlineCall, OnlinePage, OnlineScreen};
use ddi_ui::screens::room::{RoomCall, RoomShell};
use ddi_ui::screens::{Env, sample};
use ddi_ui::state::online::api::{ApiRequest, ApiResponse, Method};
use ddi_ui::state::online::{Http, Online, OnlineConfig, Platform, SharedOnline};
use ddi_ui::state::room::{RoomClient, SharedRoom};
use ddi_ui::{Ui, UiEvent, UiRequest};
use room_fake::{ROOM, keep, library};

/// The fake room's HTTP for its owner, each answer `lag` ms after its
/// request on the scene's clock.
struct LaggyHttp {
    server: SharedFake,
    clock: ManualClock,
    lag: u64,
    out: VecDeque<(u64, u64, ApiResponse)>,
}

impl Http for LaggyHttp {
    fn start(&mut self, id: u64, request: ApiRequest) {
        let ok = |body: String| ApiResponse {
            status: 200,
            body,
            session: None,
        };
        let room = format!("/rooms/{ROOM}");
        let answer = match (request.method, request.path.as_str()) {
            (Method::Get, "/me") => ok(
                r#"{"player_id":"dev:alice","login":"alice","display_name":"alice","admin":false,"expires_at_ms":1}"#
                    .into(),
            ),
            (Method::Post, "/rooms") => ok(format!(
                r#"{{"room_id":"{ROOM}","invite":"{ROOM}.invite"}}"#
            )),
            (Method::Get, p) if p == room => ok(format!(
                r#"{{"room_id":"{ROOM}","name":"After Hours","revision":1,"phase":"lobby","admission":"open","participant":1}}"#
            )),
            (Method::Post, p) if p == format!("{room}/ticket") => {
                let t = self
                    .server
                    .borrow_mut()
                    .ticket(&Identity::new("dev:alice"))
                    .expect("a ticket");
                ok(format!(r#"{{"ticket":"{t}","expires_at_ms":9}}"#))
            }
            (_, p) if p == format!("{room}/invite") => ok(format!(
                r#"{{"invite":"{ROOM}.invite","watch_invite":"{ROOM}.watch","expires_at_ms":9,"admission":"open"}}"#
            )),
            _ => ApiResponse {
                status: 403,
                body: r#"{"error":"not_an_operator"}"#.into(),
                session: None,
            },
        };
        let at = self.clock.now().0 + self.lag;
        self.out.push_back((at, id, answer));
    }

    fn poll(&mut self) -> Vec<(u64, ApiResponse)> {
        let now = self.clock.now().0;
        let mut v = Vec::new();
        while self.out.front().is_some_and(|(at, ..)| *at <= now) {
            let (_, id, a) = self.out.pop_front().expect("checked");
            v.push((id, a));
        }
        v
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Shell {
    /// One UI: the room's song select pushed over the home.
    Desktop,
    /// A UI a page: the home leaves with a navigation keeping its bars,
    /// the room's page's UI shows them at once.
    Web,
}

#[test]
fn from_the_home_into_a_private_room_frame_by_frame() {
    let instance = gpu::instance();
    let Some(adapter) = gpu::adapter(&instance, None) else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let (device, queue) = gpu::device(&adapter).expect("device");
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut gpu = Gpu::new(device, queue, format);
    let size = [1280u32, 720u32];
    let css = [1280.0f32, 720.0];
    for shell in [Shell::Desktop, Shell::Web] {
        for (reduced, lag) in [(false, 200), (true, 200), (false, 900)] {
            let clock = ManualClock::new(1_000);
            let server = FakeServer::hosted(
                ROOM,
                Identity::new("dev:alice"),
                "alice",
                RoomPolicy::default(),
                clock.clone(),
            )
            .shared();
            server
                .borrow_mut()
                .fault(ParticipantId(1), Fault::Delay(lag));
            let online: SharedOnline = Online::new(
                Box::new(LaggyHttp {
                    server: server.clone(),
                    clock: clock.clone(),
                    lag,
                    out: VecDeque::new(),
                }),
                OnlineConfig {
                    platform: Platform::Web,
                    site: "http://localhost:8080/".into(),
                },
            )
            .shared();
            let look = Rc::new(ddi_theme::Infinito::new());
            let env = Env::new(look.clone(), Rc::new(|s: &str| s.to_string()));
            env.reduced_motion.set(reduced);
            let room_slot: Rc<RefCell<Option<SharedRoom>>> = Rc::default();
            let make_room = Rc::new({
                let (online, server, clock, env, slot) = (
                    online.clone(),
                    server.clone(),
                    clock.clone(),
                    env.clone(),
                    room_slot.clone(),
                );
                move || {
                    let room: SharedRoom = Rc::new(RefCell::new(RoomClient::online(
                        online.clone(),
                        ROOM,
                        Box::new(FakeSocket::new(server.clone())),
                        ClientId::new("alice-desktop"),
                        clock.now().0,
                    )));
                    *slot.borrow_mut() = Some(room.clone());
                    let r = room.clone();
                    let shell: RoomShell = Rc::new(move |call, cx| match call {
                        RoomCall::Leave | RoomCall::Discard => cx.close(true),
                        RoomCall::Prepare(job) => r.borrow_mut().finish_load(&job, Ok(())),
                        _ => {}
                    });
                    sample::room_select(&env, room.clone(), library(0, 0x1000))
                        .with_room(room, shell)
                }
            });
            let design = ddi_ui::screens::design_for(&*look, css);
            let new_ui = || {
                let mut ui = Ui::new(design, ddi_ui::TextMode::Own, false);
                ui.set_focus_ring(false);
                ui.set_bars_route(Rc::new(|r| matches!(r, UiRequest::Navigate(_))));
                ui
            };
            let mut ui = new_ui();
            let home_shell: ddi_ui::screens::online::OnlineShell = {
                let make = make_room.clone();
                Rc::new(move |call, cx| {
                    if let OnlineCall::EnterRoom(id) = call {
                        match shell {
                            Shell::Desktop => cx.push(make()),
                            Shell::Web => {
                                cx.request(UiRequest::Navigate(format!("#/mp/room/{id}")))
                            }
                        }
                    }
                })
            };
            ui.push(OnlineScreen::new(
                env.clone(),
                online.clone(),
                OnlinePage::Home,
                home_shell,
            ));
            ui.handle(UiEvent::Resize {
                size: css,
                ratio: 1.0,
            });
            let step = |ui: &mut Ui| {
                clock.advance(16);
                ui.set_clock(clock.now().0 as f64 / 1000.0);
                ui.update(1.0 / 60.0);
            };
            // Signed in, at rest.
            for _ in 0..90 {
                step(&mut ui);
            }
            assert!(online.borrow().me().is_some(), "signed in");
            let before = ui.bounds("Choice.find").expect("the column");
            room_fake::tap(&mut ui, "Choice.private", 0.0);
            let motion = if reduced { "reduced" } else { "full" };
            let what = format!("{shell:?} {motion} lag {lag}");
            let mut room_ui: Option<Ui> = None;
            let mut line_seen = false;
            let mut view_seen = false;
            for frame in 0..(150 + lag / 5) {
                let ui_now = room_ui.as_mut().unwrap_or(&mut ui);
                step(ui_now);
                if let Some(room) = room_slot.borrow().as_ref() {
                    room.borrow_mut().tick(clock.now().0);
                }
                if room_ui.is_none() {
                    let requests = ui.take_requests();
                    if requests.iter().any(|r| matches!(r, UiRequest::Navigate(_))) {
                        // The web: the page left kept its bars; the next
                        // page's UI shows them at once, updated before
                        // its first frame (as the web's frame does).
                        assert!(ui.bars_kept(), "{what}: the bars kept");
                        let mut next = new_ui();
                        next.carry_bars(true);
                        next.push(make_room());
                        next.handle(UiEvent::Resize {
                            size: css,
                            ratio: 1.0,
                        });
                        next.set_clock(clock.now().0 as f64 / 1000.0);
                        next.update(1.0 / 60.0);
                        room_ui = Some(next);
                    }
                }
                let drawn_ui = room_ui.as_mut().unwrap_or(&mut ui);
                let home = drawn_ui
                    .layers()
                    .last()
                    .is_some_and(|l| l.name == "multiplayer");
                if home {
                    let phase = drawn_ui.layers().last().map(|l| l.phase).unwrap_or("");
                    if phase == "active" {
                        // The room being made moves nothing.
                        assert_eq!(
                            drawn_ui.bounds("Choice.find"),
                            Some(before),
                            "{what} frame {frame}: the column moved"
                        );
                    }
                } else {
                    // The room's song select is never drawn as built:
                    // its search's Clear hidden, its footer filled.
                    assert!(
                        drawn_ui.actor("ClearSearch").is_none_or(|a| !a.visible),
                        "{what} frame {frame}: the search's Clear shows"
                    );
                    let wheel = drawn_ui.actor("Wheel").is_some_and(|w| w.visible);
                    assert!(
                        wheel || !view_seen,
                        "{what} frame {frame}: the view went again"
                    );
                    view_seen |= wheel;
                    let alpha = drawn_ui
                        .actor("RoomJoining")
                        .map_or(0.0, |a| a.current().diffuse()[3]);
                    line_seen |= alpha > 0.01;
                }
                let pixels = gpu.draw_offscreen(drawn_ui, size, format);
                // The bars every frame: the header's wordmark, the
                // footer's hints.
                assert!(
                    room_fake::drawn(&pixels, size, [0.0, 0.0, 0.3, 0.06]),
                    "{what} frame {frame}: no header"
                );
                assert!(
                    room_fake::drawn(&pixels, size, [0.0, 0.955, 0.3, 1.0]),
                    "{what} frame {frame}: an empty footer"
                );
                keep(
                    &format!("entry-{shell:?}-{motion}-{lag}-{frame:03}"),
                    size,
                    "en",
                    &pixels,
                );
            }
            assert!(view_seen, "{what}: the room's view came in");
            // A room that answers at once says nothing of joining; a slow
            // one says it.
            assert_eq!(line_seen, lag > 500, "{what}: the joining line");
        }
    }
}
