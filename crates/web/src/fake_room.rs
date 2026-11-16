//! TESTING ONLY: an online room without a server, with the page's player
//! a member who follows, for the headless browsers' look at what a member
//! sees when the room's host picks a stage (decisions round D2: the
//! notice over Options that the stage is due). Asked for in the address
//! the page loads on (`#/mp/room/Room000001?mpfakeroom=S`), never from a
//! menu: online room `Room000001` is then a fake room in the page
//! (`ddi_room::fake`), whose owner, alice, is scripted: S seconds after
//! the player's client connected she picks a bundled song, her upload of
//! it is answered as the server would, and she readies (`mpfakeready=R`:
//! R seconds after she has the stage, so that the room waits for her with
//! the player ready meanwhile, feedback round O). Her match has two
//! stages: three seconds into a stage's play she reports her run (so the
//! stage is decided once the player's own play ended or was left), and S
//! seconds after the first stage's results she picks the second (the
//! notice over a stage's results, decisions round D4). The player
//! ("you", signed in as the fake room's `dev:you`) is a real member's
//! client over the fake room's socket: the room's song select, the pages
//! over it, the kept room run under them, the notice, the stage's prompt
//! and its play are the real ones; only the server is not, and the song
//! is read as the bundled one instead of from a share.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ddi_room::fake::{FakeServer, FakeSocket, SharedFake};
use ddi_room::{ClientId, Clock, Command, ContentStatus, Identity, Intent, ManualClock, RoomPolicy};
use ddi_ui::state::online::SharedOnline;
use ddi_ui::state::room::{LoadJob, Picked, RoomClient, SharedRoom, prepare_stage};

use crate::fake_seat::{ROOM, SONG, entry, online};

thread_local! {
    /// Asked for (`mpfakeroom=S`): how long after the player connected
    /// alice picks, seconds.
    static WANTED: Cell<Option<f64>> = const { Cell::new(None) };
    /// Asked for (`mpfakeready=R`): how long after she has a stage alice
    /// readies, seconds.
    static READY_AFTER: Cell<f64> = const { Cell::new(0.0) };
    static FAKE: RefCell<Option<Rc<RefCell<Fake>>>> = const { RefCell::new(None) };
}

/// Online room [`ROOM`] is the fake one from here on, its owner picking
/// `after` seconds after the player connected.
pub(crate) fn want(after: f64) {
    WANTED.set(Some(after.max(0.0)));
}

/// Alice readies `after` seconds after she has a stage (at once by
/// default).
pub(crate) fn ready_after(after: f64) {
    READY_AFTER.set(after.max(0.0));
}

/// Whether the fake room was asked for.
pub(crate) fn on() -> bool {
    WANTED.get().is_some()
}

fn now_ms() -> u64 {
    js_sys::Date::now().max(0.0) as u64
}

struct Fake {
    clock: ManualClock,
    server: SharedFake,
    /// The scripted owner's client.
    alice: SharedRoom,
    /// The player's online state and client.
    you: SharedOnline,
    member: Option<SharedRoom>,
    /// When the player's client was first seen connected.
    connected_at: Option<u64>,
    picked: bool,
    /// When alice readies for the stage she has ([`READY_AFTER`]).
    ready_at: Option<u64>,
    /// The stage alice saw begin and when, and whether she reported her
    /// run for it.
    playing: Option<(ddi_room::StageId, u64, bool)>,
    /// When the first stage was seen decided, and whether the second was
    /// picked.
    decided_at: Option<u64>,
    picked_again: bool,
    _pump: Option<gloo::timers::callback::Interval>,
}

fn fake() -> Rc<RefCell<Fake>> {
    if let Some(f) = FAKE.with_borrow(|f| f.clone()) {
        return f;
    }
    let now = now_ms();
    let clock = ManualClock::new(now);
    let server = FakeServer::hosted(
        ROOM,
        Identity::new("dev:alice"),
        "alice",
        RoomPolicy::default(),
        clock.clone(),
    )
    .shared();
    server.borrow_mut().set_room_name("Fake room (testing)");
    let _ = server
        .borrow_mut()
        .join(Identity::new("dev:you"), "you", Intent::Play);
    let alice = Rc::new(RefCell::new(RoomClient::online(
        online(&server, Some("alice")),
        ROOM,
        Box::new(FakeSocket::new(server.clone())),
        ClientId::new("alice-fake"),
        now,
    )));
    let fake = Rc::new(RefCell::new(Fake {
        clock,
        server: server.clone(),
        alice,
        you: online(&server, Some("you")),
        member: None,
        connected_at: None,
        picked: false,
        ready_at: None,
        playing: None,
        decided_at: None,
        picked_again: false,
        _pump: None,
    }));
    let pumped = fake.clone();
    fake.borrow_mut()._pump = Some(gloo::timers::callback::Interval::new(50, move || {
        if let Ok(mut f) = pumped.try_borrow_mut() {
            f.pump();
        }
    }));
    FAKE.with_borrow_mut(|f| *f = Some(fake.clone()));
    fake
}

/// The player's online state over the fake room (signed in as "you").
pub(crate) fn you() -> SharedOnline {
    fake().borrow().you.clone()
}

/// The player's client of the fake room, made at first.
pub(crate) fn member() -> SharedRoom {
    let f = fake();
    let mut f = f.borrow_mut();
    if let Some(m) = &f.member {
        return m.clone();
    }
    let m = Rc::new(RefCell::new(RoomClient::online(
        f.you.clone(),
        ROOM,
        Box::new(FakeSocket::new(f.server.clone())),
        ClientId::new("you-fake"),
        now_ms(),
    )));
    f.member = Some(m.clone());
    m
}

/// A stage's song for the player's client: read as the bundled song (the
/// fake room's share is nothing to fetch).
pub(crate) fn prepare(room: SharedRoom, job: LoadJob) {
    wasm_bindgen_futures::spawn_local(async move {
        let result = match crate::songs::load_chart(SONG).await {
            Ok(song) => prepare_stage(&job, &song, &mut |e| {
                web_sys::console::warn_1(&format!("fake room: {e}").into())
            })
            .map(|p| {
                let e = entry(&song, p.index);
                (
                    Picked {
                        song: e.clone(),
                        chart: e.charts[0].clone(),
                    },
                    p.live,
                )
            }),
            Err(_) => Err(ddi_ui::state::room::LoadProblem::Unreadable),
        };
        room.borrow_mut().finish_shared_load_live(&job, result);
    });
}

impl Fake {
    fn pump(&mut self) {
        let now = now_ms();
        self.clock.set(now.max(self.clock.now().0));
        if let Ok(mut a) = self.alice.try_borrow_mut() {
            a.tick(now);
        }
        self.server.borrow_mut().pump();
        let connected = self.member.as_ref().is_some_and(|m| {
            m.try_borrow()
                .is_ok_and(|m| m.link_state() == ddi_room::LinkState::Connected)
        });
        if connected && self.connected_at.is_none() {
            self.connected_at = Some(now);
        }
        let due = self
            .connected_at
            .zip(WANTED.get())
            .is_some_and(|(at, after)| now >= at + (after * 1_000.0) as u64);
        if due && !self.picked {
            self.picked = true;
            let alice = self.alice.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let Ok(song) = crate::songs::load_chart(SONG).await else {
                    return;
                };
                let chart = (0..song.charts.len())
                    .find(|&c| song.charts[c].layout == "dance-single")
                    .unwrap_or(0);
                let e = entry(&song, chart);
                let mut a = alice.borrow_mut();
                // A match of two stages.
                let mut rules = a.rules().clone();
                rules.stages = 2;
                let _ = a.save_rules(rules);
                if let Err(why) = a.pick(&e, &e.charts[0]) {
                    web_sys::console::warn_1(&format!("fake room: the pick: {why:?}").into());
                }
            });
        }
        if !self.picked {
            return;
        }
        // Alice's own side of the stage: her upload answered as the
        // server would, her song loaded, her Ready.
        let Ok(mut a) = self.alice.try_borrow_mut() else {
            return;
        };
        for j in a.take_upload_jobs() {
            let _ = self.server.borrow_mut().authority(Command::SetContentStatus {
                stage: j.stage,
                status: ContentStatus::Available,
                share: Some("abc12345".into()),
            });
            a.upload_finished(j.stage, Ok("abc12345".into()));
        }
        for job in a.take_load_jobs() {
            a.finish_load(&job, Ok(()));
        }
        if a.take_stage_order().is_some() {
            self.ready_at = Some(now + (READY_AFTER.get() * 1_000.0) as u64);
        }
        if self.ready_at.is_some_and(|at| now >= at) {
            self.ready_at = None;
            let _ = a.set_ready(true);
        }
        // Her run, three seconds into the stage's play.
        if let ddi_room::Phase::Stage { stage } = a.state().phase {
            let (seen, at, reported) = self.playing.unwrap_or((stage, now, false));
            let (at, reported) = if seen == stage { (at, reported) } else { (now, false) };
            let report = !reported && now >= at + 3_000;
            if report {
                let run = ddi_room::Run::Live {
                    life: 0.8,
                    score: 0,
                    steps: 0,
                };
                let _ = a.report_run(stage, run, None);
            }
            self.playing = Some((stage, at, reported || report));
        }
        // The second stage, S seconds after the first was decided.
        if matches!(a.state().phase, ddi_room::Phase::Intermission { .. }) && !self.picked_again {
            let decided = *self.decided_at.get_or_insert(now);
            let after = (WANTED.get().unwrap_or(0.0) * 1_000.0) as u64;
            if now >= decided + after {
                self.picked_again = true;
                let alice = self.alice.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let Ok(song) = crate::songs::load_chart(SONG).await else {
                        return;
                    };
                    let chart = (0..song.charts.len())
                        .find(|&c| song.charts[c].layout == "dance-single")
                        .unwrap_or(0);
                    let e = entry(&song, chart);
                    if let Err(why) = alice.borrow_mut().pick(&e, &e.charts[0]) {
                        web_sys::console::warn_1(
                            &format!("fake room: the second pick: {why:?}").into(),
                        );
                    }
                });
            }
        }
    }
}
