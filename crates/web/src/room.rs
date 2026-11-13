//! The multiplayer prototype's practice room for the session
//! (`docs/plans/multiplayer-prototype-progress.md`, M2, M7b). Every route
//! mounts a fresh page, so the room lives here, beside the pages, as the
//! share does (`crate::share`): the room's song select (`#/room`, its main
//! view) and the stage's prompt and play read the same one. A reload
//! starts a new practice; nothing is stored. The room screens' calls go
//! through [`shell`] (song select's) and [`prompt_room`] (a stage's
//! prompt's).
//!
//! A chosen stage's song is prepared here ([`prepare`]: its chart read
//! from the library and the CPUs' plan checked), and a stage whose
//! countdown is over is played on the play route ([`play`]).
//!
//! An online room's (M5d, `crate::online`) go through here too: the
//! host's pick uploaded as the room's share under its grant ([`upload`]),
//! each member's preparation from the library or from that share
//! ([`prepare_for`]), and its stage played on the play route the same way
//! ([`play_in`]), its run reported to the room ([`report`]).

use std::cell::RefCell;
use std::rc::Rc;

use ddi_ui::state::room::{
    LoadJob, LoadProblem, RoomClient, SharedRoom, StageOrder, UploadJob, picked_of, prepare_stage,
};

use crate::router::Route;

/// A stage handed to the play route: the room it is of, and the route the
/// results go back to.
struct Playing {
    order: Rc<StageOrder>,
    room: SharedRoom,
}

thread_local! {
    /// DEVELOPMENT ONLY: the CPUs of the next practice room beyond the cap
    /// (0: an ordinary one), the web's `mpcpus=N` beside `mp=1`
    /// (`docs/plans/multiplayer-prototype-progress.md`, M6b).
    static DEVELOPMENT_CPUS: std::cell::Cell<u16> = const { std::cell::Cell::new(0) };
    static ROOM: RefCell<Option<SharedRoom>> = const { RefCell::new(None) };
    /// The stage handed to the play route.
    static PLAYING: RefCell<Option<Playing>> = const { RefCell::new(None) };
    /// The room the last stage handed over is of, and the route its
    /// results go back to.
    static LAST: RefCell<Option<(Route, SharedRoom)>> = const { RefCell::new(None) };
}

/// Which room song select is the main view of (M7b): the session's
/// practice room, or online room `id` (`crate::online::room`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RoomRoute {
    Practice,
    Online(String),
}

impl RoomRoute {
    pub(crate) fn room(&self) -> SharedRoom {
        match self {
            RoomRoute::Practice => current(),
            RoomRoute::Online(id) => crate::online::room(id),
        }
    }

    /// Its address: where its stages' results come back to.
    pub(crate) fn route(&self) -> Route {
        match self {
            RoomRoute::Practice => Route::Room,
            RoomRoute::Online(id) => Route::MpRoom { id: id.clone() },
        }
    }
}

thread_local! {
    /// The `beforeunload` listener, registered exactly while leaving a
    /// room costs something ([`sync_unload_guard`]).
    static UNLOAD_GUARD: RefCell<Option<gloo::events::EventListener>> =
        const { RefCell::new(None) };
}

/// Whether the page is a room's (its song select, or a stage's prompt,
/// play or results) and leaving that room would cost something now
/// (`RoomClient::leave_cost`: the cases its screens ask before leaving
/// in).
fn leaving_costs() -> bool {
    let hash = web_sys::window()
        .and_then(|w| w.location().hash().ok())
        .unwrap_or_default();
    let room = match Route::parse(&hash) {
        Route::Room => ROOM.with(|r| r.borrow().clone()),
        Route::MpRoom { .. } => crate::online::kept_room(),
        // The stage handed to the play route, or (on its results) the
        // one handed over last.
        Route::Play(p) if p.stage.is_some() => PLAYING
            .with(|p| p.borrow().as_ref().map(|p| p.room.clone()))
            .or_else(|| LAST.with(|l| l.borrow().as_ref().map(|(_, room)| room.clone()))),
        _ => None,
    };
    room.is_some_and(|r| r.try_borrow().is_ok_and(|r| r.leave_cost().is_some()))
}

/// CPU practice left with a match running is forgotten, silently (the
/// maintainer, 2026-11-12): the page is no longer one of the practice
/// room's (its song select, or a stage's prompt, play or results), by
/// whatever way (the game's Back, the browser's, an address typed), and
/// the session's practice has a match running, so the room and its
/// unfinished match go; coming back to CPU practice makes a fresh room.
/// Between matches the practice is kept for the session, as it was.
fn forget_left_practice() {
    let hash = web_sys::window()
        .and_then(|w| w.location().hash().ok())
        .unwrap_or_default();
    if matches!(Route::parse(&hash), Route::Room)
        || matches!(Route::parse(&hash), Route::Play(p) if p.stage.is_some())
    {
        return;
    }
    let running = ROOM.with(|r| {
        r.borrow().as_ref().is_some_and(|room| {
            room.try_borrow()
                .is_ok_and(|r| r.state().phase.kind().match_running())
        })
    });
    if running {
        discard();
    }
}

/// Registers the browser's `beforeunload` question (closing or reloading
/// the tab: the browser shows its own words, and only once the person
/// has used the page) while leaving a room costs something, and removes
/// it otherwise: never there for singleplayer, nor for a room that can be
/// left at no cost. Called each frame of the UI's pages; cheap.
///
/// The page cannot tell the room it goes (nothing sent from an unload
/// arrives reliably, and a reload must not make its player leave): the
/// socket closes with the page, and the room gives the player its
/// reconnect grace before it counts them away.
pub(crate) fn sync_unload_guard() {
    forget_left_practice();
    let want = leaving_costs();
    UNLOAD_GUARD.with_borrow_mut(|guard| {
        if !want {
            *guard = None;
        } else if guard.is_none()
            && let Some(window) = web_sys::window()
        {
            let options = gloo::events::EventListenerOptions::enable_prevent_default();
            *guard = Some(gloo::events::EventListener::new_with_options(
                &window,
                "beforeunload",
                options,
                |e| {
                    e.prevent_default();
                    // What browsers took before `preventDefault` counted.
                    let _ = js_sys::Reflect::set(e, &"returnValue".into(), &true.into());
                },
            ));
        }
    });
}

/// Whether the `beforeunload` question is registered now
/// (`__DDI_DEBUG.ui.unloadGuard`: headless browsers show no prompt).
pub(crate) fn unload_guarded() -> bool {
    UNLOAD_GUARD.with_borrow(Option::is_some)
}

/// The room's calls from its screens (song select, a stage's prompt):
/// Back to the multiplayer home (practice between matches, the room
/// kept), Leave room forgets the room first (an online one told; Back on
/// an online room's song select comes here too, asked first where
/// leaving costs something, and Back on a practice with a match running,
/// never asked), Sign in again
/// leaves for the server's sign-in, its
/// songs prepared (`prepare_for`) and uploaded (`upload`), and a stage's
/// prompt opened on the play route (`play_in`), song select's running
/// preview context handed over to it so its audio starts without another
/// press (`AGENTS.md`'s gesture rule: never a context made outside one).
pub(crate) fn shell(which: RoomRoute) -> ddi_ui::screens::room::RoomShell {
    use ddi_ui::UiRequest;
    use ddi_ui::screens::room::RoomCall;
    let room = which.room();
    Rc::new(move |call, cx| match call {
        RoomCall::Leave => cx.request(UiRequest::Navigate(Route::Multiplayer.to_hash())),
        RoomCall::Discard => {
            match &which {
                RoomRoute::Practice => discard(),
                RoomRoute::Online(_) => crate::online::discard(),
            }
            cx.request(UiRequest::Navigate(Route::Multiplayer.to_hash()));
        }
        RoomCall::Redirect(r) => crate::components::page_routes::redirect(r),
        RoomCall::Prepare(job) => prepare_for(room.clone(), job),
        RoomCall::Upload(job) => upload(room.clone(), *job),
        RoomCall::PlayStage(order) => {
            if let Some(audio) = crate::select_preview::hand_over(cx.in_gesture()) {
                crate::web::audio::keep_for_play(&order.song.id, order.chart.index, audio);
            }
            cx.request(UiRequest::Navigate(play_in(
                room.clone(),
                which.route(),
                *order,
            )));
        }
    })
}

/// A stage's prompt left before the room started it (M7b): the player
/// withdraws (not ready; an operator's pick reopened), whether so.
pub(crate) fn withdraw(order: &StageOrder) -> bool {
    let room = playing_room();
    let exit = room.borrow_mut().leave_prompt(order.stage);
    if exit == ddi_ui::state::room::PromptExit::Withdraw {
        PLAYING.with(|p| p.borrow_mut().take());
        return true;
    }
    false
}

/// The room of the stage being played, and its shell's calls, for the
/// stage's prompt.
pub(crate) fn prompt_room() -> (SharedRoom, ddi_ui::screens::room::RoomShell) {
    let room = playing_room();
    let shell: ddi_ui::screens::room::RoomShell = {
        let room = room.clone();
        Rc::new(move |call, _cx| {
            use ddi_ui::screens::room::RoomCall;
            match call {
                RoomCall::Prepare(job) => prepare_for(room.clone(), job),
                RoomCall::Upload(job) => upload(room.clone(), *job),
                RoomCall::Redirect(r) => crate::components::page_routes::redirect(r),
                _ => {}
            }
        })
    };
    (room, shell)
}

/// Milliseconds since the epoch: the clock the screens share
/// (`ddi_ui::Ui::set_clock`) is on it too.
fn now_ms() -> u64 {
    js_sys::Date::now().max(0.0) as u64
}

/// The session's practice room, made at first.
pub(crate) fn current() -> SharedRoom {
    ROOM.with(|r| {
        r.borrow_mut()
            .get_or_insert_with(|| {
                let now = now_ms();
                match DEVELOPMENT_CPUS.get() {
                    0 => RoomClient::shared(now, now),
                    cpus => std::rc::Rc::new(std::cell::RefCell::new(RoomClient::development(
                        now, now, cpus,
                    ))),
                }
            })
            .clone()
    })
}

/// DEVELOPMENT ONLY: practice against `cpus` CPUs, beyond the cap, from
/// the next practice room on (the one there is left): for trying the
/// miniature fields with far more opponents. Never from a menu.
pub(crate) fn develop(cpus: u16) {
    DEVELOPMENT_CPUS.set(cpus);
    discard();
}

/// Leave room: the practice forgotten; the next visit makes a new one.
pub(crate) fn discard() {
    ROOM.with(|r| r.borrow_mut().take());
    PLAYING.with(|p| p.borrow_mut().take());
}

/// Prepares a stage's song on this device for `room`: its chart read (as a
/// play reads it) and checked (`ddi_ui::state::room::prepare_stage`: the
/// chart chosen, the CPUs' plan in practice); an online member's from the
/// room's share (`crate::share`, held for the session), its chart found
/// by the digest chosen. The room told it loaded or why not.
pub(crate) fn prepare_for(room: SharedRoom, job: LoadJob) {
    wasm_bindgen_futures::spawn_local(async move {
        let warn = |e: &str| {
            web_sys::console::warn_1(&format!("room: stage song {}: {e}", job.song).into())
        };
        if let Some(share) = &job.share {
            let result = match crate::share::load(share).await {
                Err(e) => {
                    warn(&e.to_string());
                    Err(LoadProblem::Download)
                }
                Ok(loaded) => match crate::songs::load_chart(&job.song).await {
                    Err(e) => {
                        warn(&format!("{e:?}"));
                        Err(LoadProblem::Unreadable)
                    }
                    Ok(song) => prepare_stage(&job, &song, &mut |e| warn(e)).and_then(|p| {
                        let record = &loaded.songs.first().ok_or(LoadProblem::Download)?.record;
                        let entry = &record.entry;
                        let banner = entry
                            .banner
                            .as_ref()
                            .and_then(|b| crate::share::file_url(&entry.id, b.as_str()));
                        picked_of(entry, p.index, &record.pack, banner)
                            .map(|picked| (picked, p.live))
                            .ok_or(LoadProblem::Changed)
                    }),
                },
            };
            room.borrow_mut().finish_shared_load_live(&job, result);
            return;
        }
        let result = match crate::songs::load_chart(&job.song).await {
            Ok(song) => prepare_stage(&job, &song, &mut |e| warn(e)).map(|p| p.live),
            Err(e) => {
                warn(&format!("unreadable: {e:?}"));
                Err(LoadProblem::Unreadable)
            }
        };
        room.borrow_mut().finish_load_live(&job, result);
    });
}

/// An online room's stage song uploaded as the room's share under its
/// grant (`crate::share::upload_room`), the room told how it goes and how
/// it ended.
pub(crate) fn upload(room: SharedRoom, job: UploadJob) {
    wasm_bindgen_futures::spawn_local(async move {
        let progress = {
            let room = room.clone();
            move |done: f64, total: f64| room.borrow_mut().upload_progress(job.stage, done, total)
        };
        let result = crate::share::upload_room(&job.song.id, &job.grant, &progress).await;
        if let Err(e) = &result {
            web_sys::console::warn_1(&format!("room: sharing stage {}: {e}", job.stage).into());
        }
        room.borrow_mut().upload_finished(job.stage, result);
    });
}

/// A stage of `room` to play: kept for the play route, whose address it
/// gives (`#/play?song=…&chart=…&stage=<id>`, with the session's testing
/// parameters); its results go back to `back`.
pub(crate) fn play_in(room: SharedRoom, back: Route, order: StageOrder) -> String {
    let route = Route::Play(crate::router::PlayRoute {
        stage: Some(order.stage.0),
        ..crate::router::PlayRoute::song(order.song.id.clone(), order.chart.index)
    });
    LAST.with(|l| *l.borrow_mut() = Some((back, room.clone())));
    PLAYING.with(|p| {
        *p.borrow_mut() = Some(Playing {
            order: Rc::new(order),
            room,
        })
    });
    crate::router::with_testing(route.to_hash(), &crate::router::TESTING_KEYS)
}

/// The stage handed to the play route with this id, while it is played.
pub(crate) fn playing(id: u64) -> Option<Rc<StageOrder>> {
    PLAYING.with(|p| {
        p.borrow()
            .as_ref()
            .filter(|p| p.order.stage.0 == id)
            .map(|p| p.order.clone())
    })
}

/// Stage `order`'s room client for its play, where the room plays it live
/// (`ddi_session::battle::live`): the play starts at the room's instant
/// and streams through it.
pub(crate) fn live_handle(order: &StageOrder) -> Option<ddi_session::battle::live::LiveHandle> {
    order.live.as_ref()?;
    Some(ddi_session::battle::live::LiveHandle {
        room: playing_room(),
        wall_ms: Rc::new(now_ms),
    })
}

/// The local player's attack policy in the room of the stage being
/// played, as its play starts.
pub(crate) fn my_targeting() -> ddi_room::attacks::Targeting {
    playing_room().borrow().my_targeting()
}

/// Where a stage's results go back to: the room it was played in.
pub(crate) fn back_route() -> Route {
    LAST.with(|l| l.borrow().as_ref().map(|(r, _)| r.clone()))
        .unwrap_or(Route::Room)
}

/// The room the last stage handed over is of (its results ask it how the
/// stage went).
/// The room's calls from a stage's results the player reached by the
/// browser's Back during its play (`ResultsScreen::after_back`): the
/// room's question's Leave leaves the room (an online one told and
/// forgotten, a practice forgotten) for the multiplayer home.
pub(crate) fn results_shell() -> ddi_ui::screens::room::RoomShell {
    use ddi_ui::UiRequest;
    use ddi_ui::screens::room::RoomCall;
    Rc::new(move |call, cx| match call {
        RoomCall::Leave => cx.request(UiRequest::Navigate(Route::Multiplayer.to_hash())),
        RoomCall::Discard => {
            if last_room().try_borrow().is_ok_and(|r| r.is_online()) {
                crate::online::discard();
            } else {
                discard();
            }
            cx.request(UiRequest::Navigate(Route::Multiplayer.to_hash()));
        }
        _ => {}
    })
}

pub(crate) fn last_room() -> SharedRoom {
    LAST.with(|l| l.borrow().as_ref().map(|(_, r)| r.clone()))
        .unwrap_or_else(current)
}

/// The room of the stage being played (the practice room otherwise).
fn playing_room() -> SharedRoom {
    PLAYING
        .with(|p| p.borrow().as_ref().map(|p| p.room.clone()))
        .unwrap_or_else(current)
}

/// A stage's outcome (`ddi_session::battle`), reported to its room: in
/// practice as its authority (its ranking confirmed, the song ended); in
/// an online room the player's own run, which the room ranks with the
/// others'. How the match ended for the player, where that was its last
/// stage (an online room's is not known yet: `None`).
pub(crate) fn report(
    outcome: ddi_session::battle::StageOutcome,
) -> Option<ddi_ui::state::room::MatchEnd> {
    let room = playing_room();
    let has_attacks = PLAYING.with(|p| {
        p.borrow()
            .as_ref()
            .is_some_and(|p| p.order.stage == outcome.stage && p.order.battle.attacks())
    });
    PLAYING.with(|p| {
        p.borrow_mut().take_if(|p| p.order.stage == outcome.stage);
    });
    room.borrow_mut()
        .set_stage_summary(outcome.stage, outcome.summary);
    let reported = room.borrow_mut().report_played(
        outcome.stage,
        outcome.mine,
        outcome.ranking,
        outcome.confirmed_early,
        outcome.details,
    );
    if let Err(why) = reported {
        web_sys::console::warn_1(&format!("room: the stage's outcome was refused: {why:?}").into());
    }
    // The policy the play left them with is theirs for the next stage
    // (where the stage had attacks: an online room's has none yet).
    if has_attacks {
        room.borrow_mut().played_targeting(outcome.targeting);
    }
    room.borrow().match_end()
}

/// A stage left before its play began (its prompt's Back): a forfeit, the
/// CPUs played to their end on `song` (as read) under `ruleset`; without
/// a song to play them on, the match stops. An online room hears the
/// forfeit as the player's own report.
pub(crate) fn leave_before_play(
    order: &StageOrder,
    song: Option<&ddi_chart::Song>,
    ruleset: Option<&ddi_engine::rules::Ruleset>,
) {
    let room = playing_room();
    if room.borrow().is_online() {
        PLAYING.with(|p| p.borrow_mut().take());
        let _ = room
            .borrow_mut()
            .report_run(order.stage, ddi_room::Run::Forfeit, None);
        return;
    }
    let outcome = song.zip(ruleset).map(|(song, rules)| {
        ddi_session::battle::forfeit(
            ddi_session::battle::StageSetup::of(order),
            song,
            order.chart.index,
            rules,
        )
    });
    match outcome {
        Some(Ok(o)) => {
            report(o);
        }
        other => {
            if let Some(Err(e)) = other {
                web_sys::console::warn_1(&format!("room: stage {}: {e}", order.number).into());
            }
            PLAYING.with(|p| p.borrow_mut().take());
            current().borrow_mut().stage_lost();
        }
    }
}
