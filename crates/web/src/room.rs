//! The multiplayer prototype's practice room for the session
//! (`docs/plans/multiplayer-prototype-progress.md`, M2). Every route mounts
//! a fresh page, so the room lives here, beside the pages, as the share
//! does (`crate::share`): the room page (`#/room`), song select choosing a
//! stage's song (`#/room/pick`) and later the stage's play read the same
//! one. A reload starts a new practice; nothing is stored.
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

use ddi_room::StageId;
use ddi_ui::state::room::{
    LoadJob, LoadProblem, RoomClient, SharedRoom, StageOrder, UploadJob, picked_of, prepare_chart,
};

use crate::router::Route;

/// A stage handed to the play route: the room it is of, and the route the
/// results go back to.
struct Playing {
    order: Rc<StageOrder>,
    room: SharedRoom,
}

thread_local! {
    static ROOM: RefCell<Option<SharedRoom>> = const { RefCell::new(None) };
    /// The stage handed to the play route.
    static PLAYING: RefCell<Option<Playing>> = const { RefCell::new(None) };
    /// The room the last stage handed over is of, and the route its
    /// results go back to.
    static LAST: RefCell<Option<(Route, SharedRoom)>> = const { RefCell::new(None) };
    /// The stage whose song was just chosen: the room focuses it on return.
    static CHOSEN: RefCell<Option<StageId>> = const { RefCell::new(None) };
    /// The online room song select's pick mode chooses for (none: the
    /// practice room).
    static PICK: RefCell<Option<SharedRoom>> = const { RefCell::new(None) };
}

/// Song select's pick mode chooses for `room` (an online room), else for
/// the practice room.
pub(crate) fn pick_for(room: Option<SharedRoom>) {
    PICK.with(|p| *p.borrow_mut() = room);
}

/// The room song select's pick mode chooses a stage's song for.
pub(crate) fn picking() -> SharedRoom {
    PICK.with(|p| p.borrow().clone()).unwrap_or_else(current)
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
                RoomClient::shared(now, now)
            })
            .clone()
    })
}

/// Leave room: the practice forgotten; the next visit makes a new one.
pub(crate) fn discard() {
    ROOM.with(|r| r.borrow_mut().take());
    PLAYING.with(|p| p.borrow_mut().take());
    CHOSEN.with(|c| c.borrow_mut().take());
}

/// A stage's song was chosen (song select's pick mode).
pub(crate) fn chosen(stage: StageId) {
    CHOSEN.with(|c| *c.borrow_mut() = Some(stage));
}

/// The stage chosen last, once.
pub(crate) fn take_chosen() -> Option<StageId> {
    CHOSEN.with(|c| c.borrow_mut().take())
}

/// Prepares a practice stage's song on this device ([`prepare_for`] the
/// practice room).
pub(crate) fn prepare(job: LoadJob) {
    prepare_for(current(), job);
}

/// Prepares a stage's song on this device for `room`: its chart read (as a
/// play reads it) and checked (`ddi_ui::state::room::prepare_chart`: the
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
                    Ok(song) => prepare_chart(&job, &song, &mut |e| warn(e)).and_then(|index| {
                        let record = &loaded.songs.first().ok_or(LoadProblem::Download)?.record;
                        let entry = &record.entry;
                        let banner = entry
                            .banner
                            .as_ref()
                            .and_then(|b| crate::share::file_url(&entry.id, b.as_str()));
                        picked_of(entry, index, &record.pack, banner).ok_or(LoadProblem::Changed)
                    }),
                },
            };
            room.borrow_mut().finish_shared_load(&job, result);
            return;
        }
        let result = match crate::songs::load_chart(&job.song).await {
            Ok(song) => prepare_chart(&job, &song, &mut |e| warn(e)).map(|_| ()),
            Err(e) => {
                warn(&format!("unreadable: {e:?}"));
                Err(LoadProblem::Unreadable)
            }
        };
        room.borrow_mut().finish_load(&job, result);
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

/// A practice stage to play ([`play_in`] the practice room).
pub(crate) fn play(order: StageOrder) -> String {
    play_in(current(), Route::Room, order)
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

/// Where a stage's results go back to: the room it was played in.
pub(crate) fn back_route() -> Route {
    LAST.with(|l| l.borrow().as_ref().map(|(r, _)| r.clone()))
        .unwrap_or(Route::Room)
}

/// The room the last stage handed over is of (its results ask it how the
/// stage went).
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
    PLAYING.with(|p| {
        p.borrow_mut().take_if(|p| p.order.stage == outcome.stage);
    });
    let online = room.borrow().is_online();
    let reported = if online {
        room.borrow_mut().report_run(outcome.stage, outcome.mine)
    } else {
        room.borrow_mut()
            .report_stage(outcome.stage, outcome.ranking, outcome.confirmed_early)
    };
    if let Err(why) = reported {
        web_sys::console::warn_1(&format!("room: the stage's outcome was refused: {why:?}").into());
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
            .report_run(order.stage, ddi_room::Run::Forfeit);
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
