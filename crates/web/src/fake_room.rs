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
//! notice over a stage's results, decisions round D4). A second scripted
//! member, bob, readies at once and reports as she does (where the room
//! plays live, he streams the chart's taps instead, every ninth missed, so
//! that the stage goes on to the song's end), so that a Ready
//! window that runs out starts the stage without a player who was not
//! ready (a start needs two: round Y3; before, it was canceled below the
//! minimum and nobody ever sat a stage out here). The player ("you",
//! signed in as the fake room's `dev:you`) is a real member's client over
//! the fake room's socket: the room's song select, the pages over it, the
//! kept room run under them, the notice, the stage's prompt and its play
//! are the real ones; only the server is not, and the song is read as the
//! bundled one instead of from a share. What a real room may do to the
//! player, asked for in the address too (round Y3): `mpfakefail=N`, the
//! player's first N downloads of a stage's song fail (`LoadProblem::Download`:
//! its retries, then the Retry beside the status line); `mpfakeslow=S`,
//! each takes S seconds (still loading at the window's end); and
//! `mpfakeelsewhere=S`, S seconds after a pick the server closes the
//! player's socket 4429, as when a fifth device of theirs opens the room
//! (the notice and Use this device; the room's reconnect grace 5 s here,
//! so that a member gone that long is away and sits a stage out as such);
//! and (round BM) `mpfakeremove=S,kick|mirror|close`, S seconds after the
//! player connected alice kicks them (her own Kick: 4403), the host's
//! Twitch lists remove them (the server's removal: 4406), or the room
//! closes (4410): the notice whose one OK goes to the multiplayer home.

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
    /// Asked for (`mpfakelive=1`): the room plays live.
    static LIVE: Cell<bool> = const { Cell::new(false) };
    /// Asked for (`mpfakefail=N`): the player's downloads of a stage's
    /// song still to fail.
    static FAILS: Cell<u32> = const { Cell::new(0) };
    /// Asked for (`mpfakeslow=S`): how long each of those downloads takes,
    /// seconds.
    static SLOW: Cell<f64> = const { Cell::new(0.0) };
    /// Asked for (`mpfakeelsewhere=S`): how long after a pick the server
    /// closes the player's socket 4429, seconds.
    static ELSEWHERE: Cell<Option<f64>> = const { Cell::new(None) };
    /// Asked for (`mpfakeremove=S,HOW`): how long after the player
    /// connected they are removed, and how.
    static REMOVE: RefCell<Option<(f64, String)>> = const { RefCell::new(None) };
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

/// The fake room plays live (`RoomPolicy::with_live`), as a hosted room
/// does: its stage's song zero on the room's clock, a start missed by more
/// than the late join a forfeit. Alice prepares her stage to play live
/// and never streams (she is a no-show).
pub(crate) fn live() {
    LIVE.set(true);
}

/// The player's first `n` downloads of a stage's song fail
/// (`mpfakefail=N`).
pub(crate) fn fail_downloads(n: u32) {
    FAILS.set(n);
}

/// Each of the player's downloads of a stage's song takes `seconds`
/// (`mpfakeslow=S`).
pub(crate) fn slow_downloads(seconds: f64) {
    SLOW.set(seconds.max(0.0));
}

/// `seconds` after a pick the server closes the player's socket 4429, a
/// fifth device of theirs opening the room (`mpfakeelsewhere=S`).
pub(crate) fn opened_elsewhere(seconds: f64) {
    ELSEWHERE.set(Some(seconds.max(0.0)));
}

/// `seconds` after the player connected they are removed from the room
/// (`mpfakeremove=S,HOW`, round BM): `kick` (alice's Kick), `mirror` (the
/// host's Twitch lists, the server's removal) or `close` (the room
/// closed).
pub(crate) fn remove(seconds: f64, how: &str) {
    REMOVE.with_borrow_mut(|r| *r = Some((seconds.max(0.0), how.to_string())));
}

/// The reconnect grace the fake server gives where it closes the player's
/// socket (the real one's is 30 s): a member gone that long is away.
const GRACE_MS: u64 = 5_000;

/// Whether the fake room was asked for.
pub(crate) fn on() -> bool {
    WANTED.get().is_some()
}

/// The rooms' steady clock (`crate::room::now_ms`).
fn now_ms() -> u64 {
    crate::room::now_ms()
}

struct Fake {
    clock: ManualClock,
    server: SharedFake,
    /// The scripted owner's client, and the other scripted member's.
    alice: SharedRoom,
    bob: SharedRoom,
    /// The player's online state and client.
    you: SharedOnline,
    member: Option<SharedRoom>,
    /// When the player's client was first seen connected.
    connected_at: Option<u64>,
    /// Alice's song added to the room's list (room model revision 17):
    /// her picks are made from it, as a host's are.
    listed: bool,
    picked: bool,
    /// When alice and bob ready for the stage they have (alice:
    /// [`READY_AFTER`]; bob at once).
    ready_at: [Option<u64>; 2],
    /// The stage each saw begin and when, and whether they reported their
    /// run for it.
    playing: [Option<(ddi_room::StageId, u64, bool)>; 2],
    /// When the last pick was seen (alice's), and whether the player's
    /// socket was closed for it ([`ELSEWHERE`]).
    picked_at: Option<(ddi_room::StageId, u64, bool)>,
    /// The player was removed ([`REMOVE`]).
    removed: bool,
    /// The song, once read, and bob's stream of a live stage.
    song: Rc<RefCell<Option<Rc<ddi_chart::Song>>>>,
    stream: Option<(ddi_room::StageId, ddi_room::Millis, crate::fake_seat::Script)>,
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
        if LIVE.get() {
            RoomPolicy::default().with_live()
        } else {
            RoomPolicy::default()
        },
        clock.clone(),
    )
    .shared();
    server.borrow_mut().set_room_name("Fake room (testing)");
    if ELSEWHERE.get().is_some() {
        server.borrow_mut().set_grace(Some(GRACE_MS));
    }
    let _ = server
        .borrow_mut()
        .join(Identity::new("dev:bob"), "bob", Intent::Play);
    let _ = server
        .borrow_mut()
        .join(Identity::new("dev:you"), "you", Intent::Play);
    let scripted = |who: &'static str| {
        Rc::new(RefCell::new(RoomClient::online(
            online(&server, Some(who)),
            ROOM,
            Box::new(FakeSocket::new(server.clone())),
            ClientId::new(format!("{who}-fake")),
            now,
        )))
    };
    let (alice, bob) = (scripted("alice"), scripted("bob"));
    let fake = Rc::new(RefCell::new(Fake {
        clock,
        server: server.clone(),
        alice,
        bob,
        you: online(&server, Some("you")),
        member: None,
        connected_at: None,
        listed: false,
        picked: false,
        removed: false,
        ready_at: [None; 2],
        playing: [None; 2],
        picked_at: None,
        song: Rc::default(),
        stream: None,
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
/// fake room's share is nothing to fetch), after [`SLOW`] seconds, the
/// first [`FAILS`] times failing as a download does.
pub(crate) fn prepare(room: SharedRoom, job: LoadJob) {
    prepare_as(room, job, true);
}

/// [`prepare`], with the player's faults where `player` (a scripted
/// follower has none).
fn prepare_as(room: SharedRoom, job: LoadJob, player: bool) {
    wasm_bindgen_futures::spawn_local(async move {
        let slow = if player { SLOW.get() } else { 0.0 };
        if slow > 0.0 {
            gloo::timers::future::TimeoutFuture::new((slow * 1_000.0) as u32).await;
        }
        if player && FAILS.get() > 0 {
            FAILS.set(FAILS.get() - 1);
            web_sys::console::log_1(&"fake room: the song's download fails (mpfakefail)".into());
            room.borrow_mut()
                .finish_shared_load_live(&job, Err(ddi_ui::state::room::LoadProblem::Download));
            return;
        }
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

/// The song as alice's library has it (every original chart), for the
/// room's list: its content is the bundled song's, which the player's
/// library has too.
fn listed_entry(song: &ddi_chart::Song) -> ddi_ui::screens::data::SongEntry {
    let mut e = entry(song, 0);
    e.charts = (0..song.charts.len())
        .filter(|&i| song.charts[i].derivation.is_none())
        .map(|i| ddi_ui::screens::data::ChartEntry {
            style: song.charts[i].layout.clone(),
            difficulty: song.charts[i].difficulty,
            meter: song.charts[i].meter,
            index: i,
            digest: Some(song.chart_digest(i)),
            ..Default::default()
        })
        .collect();
    e
}

/// Alice's pick of `e`'s chart from the room's list (where the song is
/// listed), else as before revision 17 (a pick shared after it).
fn pick_from_list(
    a: &mut RoomClient,
    e: &ddi_ui::screens::data::SongEntry,
) -> Result<(), ddi_ui::state::room::Refusal> {
    let listed = a.room_songs().iter().find(|l| {
        e.charts[0]
            .digest
            .is_some_and(|d| l.chart(&ddi_room::ChartKey::new(d.to_string())).is_some())
    });
    match listed.map(|l| l.id) {
        Some(id) => {
            let chart = ddi_room::ChartKey::new(e.charts[0].digest.map(|d| d.to_string()).unwrap_or_default());
            a.pick_listed(id, &chart)
        }
        None => a.pick(e, &e.charts[0]),
    }
}

impl Fake {
    /// Scripted member `i`'s (0 alice, 1 bob) side of a stage: their song
    /// loaded (to play live where the room does), Ready ([`READY_AFTER`]
    /// for alice, at once for bob), their run reported three seconds
    /// into the stage's play.
    fn script(&mut self, i: usize, who: &SharedRoom, now: u64) {
        let Ok(mut a) = who.try_borrow_mut() else {
            return;
        };
        for job in a.take_load_jobs() {
            // A follower reads the stage's song from the room's share.
            if job.share.is_some() && i > 0 {
                prepare_as(who.clone(), job, false);
                continue;
            }
            if job.live.is_none() {
                a.finish_load(&job, Ok(()));
                continue;
            }
            // Live: the stage prepared to play live, from the song.
            let who = who.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let result = match crate::songs::load_chart(SONG).await {
                    Ok(song) => prepare_stage(&job, &song, &mut |_| {}).map(|p| p.live),
                    Err(_) => Err(ddi_ui::state::room::LoadProblem::Unreadable),
                };
                who.borrow_mut().finish_load_live(&job, result);
            });
        }
        if a.take_stage_order().is_some() {
            let after = if i == 0 { READY_AFTER.get() } else { 0.0 };
            self.ready_at[i] = Some(now + (after * 1_000.0) as u64);
        }
        if self.ready_at[i].is_some_and(|at| now >= at) {
            self.ready_at[i] = None;
            let _ = a.set_ready(true);
        }
        // Bob in a live stage: his stream, then his run as his device
        // would report it.
        if i == 1 && LIVE.get() {
            drop(a);
            self.stream_bob(who);
            return;
        }
        // Alice in a live stage is a no-show: she reports nothing (round
        // Y3: her finished run three seconds in set the song's end there,
        // so the room ended the song a minute later without the player's
        // own report, and the Standings had no score or life for them).
        if LIVE.get() {
            return;
        }
        if let ddi_room::Phase::Stage { stage } = a.state().phase {
            let (seen, at, reported) = self.playing[i].unwrap_or((stage, now, false));
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
            self.playing[i] = Some((stage, at, reported || report));
        }
    }

    /// Bob's stream of the live stage he is in (the chart's taps, every
    /// ninth missed), and his run reported once it ended.
    fn stream_bob(&mut self, bob: &SharedRoom) {
        let started = {
            let b = bob.borrow();
            let stage = match b.state().phase {
                ddi_room::Phase::Countdown { stage, .. } | ddi_room::Phase::Stage { stage } => Some(stage),
                _ => None,
            };
            stage
                .filter(|s| b.stage_started(*s) && b.in_stage(*s))
                .and_then(|s| Some((s, b.song_zero_at(s)?)))
        };
        if let Some((stage, zero)) = started
            && self.stream.as_ref().is_none_or(|(s, _, _)| *s != stage)
            && let Some(song) = self.song.borrow().clone()
        {
            let chart = (0..song.charts.len())
                .find(|&c| song.charts[c].layout == "dance-single")
                .unwrap_or(0);
            self.stream = Some((stage, zero, crate::fake_seat::Script::new(&song, chart, 9, None)));
        }
        let Some((stage, zero, script)) = self.stream.as_mut() else {
            return;
        };
        if script.ended {
            return;
        }
        let t = bob
            .borrow()
            .room_now()
            .map_or(0, |n| n.0 as i64 - zero.0 as i64);
        script.stream(bob, *stage, t);
        if script.ended {
            let d = script.detail();
            let run = ddi_room::Run::Live {
                life: f64::from(d.life_ppm) / 1e6,
                score: d.score.unwrap_or(0),
                steps: 0,
            };
            let _ = bob.borrow_mut().report_run(*stage, run, Some(d));
        }
    }

    /// [`ELSEWHERE`] seconds after each pick, the player's socket closed
    /// 4429 by the server, as when a fifth device of theirs opens the
    /// room.
    fn elsewhere(&mut self, now: u64) {
        let Some(after) = ELSEWHERE.get() else {
            return;
        };
        let stage = self.alice.try_borrow().ok().and_then(|a| a.state().upcoming_stage());
        let Some(stage) = stage else {
            return;
        };
        let (seen, at, closed) = self.picked_at.unwrap_or((stage, now, false));
        let (at, closed) = if seen == stage { (at, closed) } else { (now, false) };
        let close = !closed && now >= at + (after * 1_000.0) as u64;
        if close {
            let you = self
                .server
                .borrow()
                .state()
                .member_by_identity(&Identity::new("dev:you"))
                .map(|m| m.id);
            if let Some(you) = you {
                web_sys::console::log_1(&"fake room: the player's socket closed 4429 (mpfakeelsewhere)".into());
                self.server
                    .borrow_mut()
                    .fault(you, ddi_room::fake::Fault::Close(4429));
            }
        }
        self.picked_at = Some((stage, at, closed || close));
    }

    /// [`REMOVE`]'s seconds after the player connected, the player removed
    /// as it says (round BM).
    fn remove(&mut self, now: u64) {
        let Some((after, how)) = REMOVE.with_borrow(|r| r.clone()) else {
            return;
        };
        let Some(at) = self.connected_at else {
            return;
        };
        if self.removed || now < at + (after * 1_000.0) as u64 {
            return;
        }
        let you = self
            .server
            .borrow()
            .state()
            .member_by_identity(&Identity::new("dev:you"))
            .map(|m| m.id);
        let Some(you) = you else {
            return;
        };
        self.removed = true;
        web_sys::console::log_1(&format!("fake room: the player removed ({how}, mpfakeremove)").into());
        let close = ddi_room::online::close::ROOM_CLOSED;
        match how.as_str() {
            "kick" => {
                if let Ok(mut a) = self.alice.try_borrow_mut() {
                    let _ = a.kick(you);
                }
            }
            "mirror" => {
                let _ = self.server.borrow_mut().authority(Command::Remove { participant: you });
                self.server
                    .borrow_mut()
                    .fault(you, ddi_room::fake::Fault::Close(ddi_room::online::close::NOT_ADMITTED));
            }
            _ => {
                self.server
                    .borrow_mut()
                    .fault(you, ddi_room::fake::Fault::Close(close));
            }
        }
    }

    fn pump(&mut self) {
        let now = now_ms();
        self.clock.set(now.max(self.clock.now().0));
        for who in [&self.alice, &self.bob] {
            if let Ok(mut a) = who.try_borrow_mut() {
                a.tick(now);
            }
        }
        self.server.borrow_mut().pump();
        let connected = self.member.as_ref().is_some_and(|m| {
            m.try_borrow()
                .is_ok_and(|m| m.link_state() == ddi_room::LinkState::Connected)
        });
        if connected && self.connected_at.is_none() {
            self.connected_at = Some(now);
        }
        // The song in the room's list once the player is in (their Room
        // songs tab shows it before it is picked).
        if connected && !self.listed {
            self.listed = true;
            let alice = self.alice.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let Ok(song) = crate::songs::load_chart(SONG).await else {
                    return;
                };
                let whole = listed_entry(&song);
                let mut a = alice.borrow_mut();
                a.set_library(std::slice::from_ref(&whole));
                if let Err(why) = a.add_song(&whole) {
                    web_sys::console::warn_1(&format!("fake room: adding the song: {why:?}").into());
                }
            });
        }
        let due = self
            .connected_at
            .zip(WANTED.get())
            .is_some_and(|(at, after)| now >= at + (after * 1_000.0) as u64);
        if due && !self.picked {
            self.picked = true;
            let alice = self.alice.clone();
            let kept = self.song.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let Ok(song) = crate::songs::load_chart(SONG).await else {
                    return;
                };
                let chart = (0..song.charts.len())
                    .find(|&c| song.charts[c].layout == "dance-single")
                    .unwrap_or(0);
                let e = entry(&song, chart);
                *kept.borrow_mut() = Some(Rc::new(song));
                let mut a = alice.borrow_mut();
                // A match of two stages.
                let mut rules = a.rules().clone();
                rules.stages = 2;
                let _ = a.save_rules(rules);
                if let Err(why) = pick_from_list(&mut a, &e) {
                    web_sys::console::warn_1(&format!("fake room: the pick: {why:?}").into());
                }
            });
        }
        // Alice's uploads answered as the server would: the listed song's
        // (a bundled song is uploaded as any, round DB) and a stage's.
        if let Ok(mut a) = self.alice.try_borrow_mut() {
            for j in a.take_upload_jobs() {
                match j.entry {
                    Some(song) => {
                        let _ = self.server.borrow_mut().authority(Command::SetSongContent {
                            song,
                            status: ContentStatus::Available,
                            share: Some("abc12345".into()),
                            charts: None,
                        });
                        a.song_upload_finished(song, Ok("abc12345".into()));
                    }
                    None => {
                        let _ = self.server.borrow_mut().authority(Command::SetContentStatus {
                            stage: j.stage,
                            status: ContentStatus::Available,
                            share: Some("abc12345".into()),
                        });
                        a.upload_finished(j.stage, Ok("abc12345".into()));
                    }
                }
            }
        }
        self.remove(now);
        if !self.picked {
            return;
        }
        self.elsewhere(now);
        // Each scripted member's own side of the stage: the song loaded,
        // Ready, the run reported.
        for (i, who) in [self.alice.clone(), self.bob.clone()].into_iter().enumerate() {
            self.script(i, &who, now);
        }
        let Ok(a) = self.alice.try_borrow() else {
            return;
        };
        // The second stage, S seconds after the first was decided.
        let intermission = matches!(a.state().phase, ddi_room::Phase::Intermission { .. });
        drop(a);
        if intermission && !self.picked_again {
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
                    if let Err(why) = pick_from_list(&mut alice.borrow_mut(), &e) {
                        web_sys::console::warn_1(
                            &format!("fake room: the second pick: {why:?}").into(),
                        );
                    }
                });
            }
        }
    }
}
