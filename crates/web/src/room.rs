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
    LoadJob, LoadProblem, SharedFound, SharedRoom, StageOrder, UploadJob, prepare_stage,
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

/// Which of a room's pages the address is ([`sync_unload_guard`] reads it
/// each UI frame, parsing the address only when it changes).
#[derive(Clone, PartialEq)]
enum RoomPage {
    /// The practice room's song select (`#/room`).
    Practice,
    /// Online room `id`'s song select.
    Online(String),
    /// A stage's prompt, play or results.
    Stage,
    /// A page that opens over a room's song select and comes back to it
    /// (Options and what it opens: Calibrate, a calibration's run, the
    /// announcer studio and its scenes): the room is kept as it was, an
    /// online one run meanwhile ([`run_kept`]), as desktop keeps the
    /// room's screen under them (review3 web-2, web-3).
    Over,
    /// The room watched with its audience by `link`: its page, or a stage
    /// of it watched.
    Audience(String),
    /// Any other page.
    Other,
}

thread_local! {
    /// The address last read and which page it is.
    static PAGE: RefCell<(String, RoomPage)> = const { RefCell::new((String::new(), RoomPage::Other)) };
}

/// Which room page the address is, parsed again only when it changed.
fn room_page() -> RoomPage {
    let hash = web_sys::window()
        .and_then(|w| w.location().hash().ok())
        .unwrap_or_default();
    PAGE.with_borrow_mut(|(seen, page)| {
        if *seen != hash {
            *page = match Route::parse(&hash) {
                Route::Room => RoomPage::Practice,
                Route::MpRoom { id } => RoomPage::Online(id),
                Route::Play(p) if p.stage.is_some() => RoomPage::Stage,
                Route::Play(p) if p.scene.is_some() => RoomPage::Over,
                Route::Options | Route::Calibrate | Route::CalibrateRun { .. } | Route::Studio => {
                    RoomPage::Over
                }
                Route::MpWatch { link } | Route::MpWatchStage { link } => RoomPage::Audience(link),
                _ => RoomPage::Other,
            };
            *seen = hash;
        }
        page.clone()
    })
}

/// Whether the page is a room's (its song select, or a stage's prompt,
/// play or results, or a page over it) and leaving that room would cost
/// something now (`RoomClient::leave_cost`: the cases its screens ask
/// before leaving in).
fn leaving_costs(page: &RoomPage) -> bool {
    let practice = || ROOM.with(|r| r.borrow().clone());
    let room = match page {
        RoomPage::Practice => practice(),
        RoomPage::Online(_) => crate::online::kept_room(),
        // The stage handed to the play route, or (on its results) the
        // one handed over last.
        RoomPage::Stage => PLAYING
            .with(|p| p.borrow().as_ref().map(|p| p.room.clone()))
            .or_else(|| LAST.with(|l| l.borrow().as_ref().map(|(_, room)| room.clone()))),
        RoomPage::Over => crate::online::kept_room().or_else(practice),
        RoomPage::Audience(_) | RoomPage::Other => None,
    };
    room.is_some_and(|r| r.try_borrow().is_ok_and(|r| r.leave_cost().is_some()))
}

/// CPU practice left with a match running is forgotten, silently (the
/// maintainer, 2026-11-12): the page is no longer one of the practice
/// room's (its song select, or a stage's prompt, play or results, or a
/// page over it: Options, Calibrate), by whatever way (the game's Back,
/// the browser's, an address typed), and the session's practice has a
/// match running, so the room and its unfinished match go; coming back
/// to CPU practice makes a fresh room. Between matches the practice is
/// kept for the session, as it was.
fn forget_left_practice(page: &RoomPage) {
    if matches!(page, RoomPage::Practice | RoomPage::Stage | RoomPage::Over) {
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

/// An online room left by whatever way (the browser's Back, an address
/// typed, a link followed; review3 web-1) is left as the game's Back
/// leaves it (AGENTS.md: Back on an online room's song select leaves the
/// room): told, its socket closed, forgotten. Kept on its own song
/// select, a stage's pages and the pages over it.
fn leave_left_online(page: &RoomPage) {
    let Some(kept) = crate::online::kept_room_id() else {
        return;
    };
    let stays = match page {
        RoomPage::Online(id) => *id == kept,
        RoomPage::Stage | RoomPage::Over => true,
        _ => false,
    };
    if stays {
        return;
    }
    if let Some(room) = crate::online::kept_room()
        && let Ok(mut r) = room.try_borrow_mut()
    {
        r.set_rules_seen(false);
        r.leave();
    }
    crate::online::discard();
}

/// The room's seat in its audience, left by whatever way: stopped
/// (its socket closed, its audio context let go; review3 web-1's sibling,
/// web-7). Kept on its pages alone.
fn leave_left_audience(page: &RoomPage) {
    let Some((link, _)) = crate::online::kept_audience_link() else {
        return;
    };
    if !matches!(page, RoomPage::Audience(l) if *l == link) {
        crate::online::discard_audience();
    }
}

thread_local! {
    /// The timer that runs the kept online room where no frame does
    /// ([`run_kept`]), and the listener that hears the page shown again.
    static KEPT_PUMP: RefCell<Option<(gloo::timers::callback::Interval, gloo::events::EventListener)>> =
        const { RefCell::new(None) };
    /// A run of the kept room asked for from an event while the page is
    /// hidden ([`wake_hidden`]), not yet made.
    static WAKING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Such a run was asked for by the socket's close: its reconnect is
    /// tried at once ([`wake_hidden`]); and when one last was (ms).
    static CLOSED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static HURRIED_AT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

thread_local! {
    /// The unseen step of a room's stage's prompt on the page, while it
    /// shows (`components::game_canvas`'s `prompt_unseen`).
    static UNSEEN_PROMPT: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

/// The unseen step of the stage's prompt the page shows (`None` once it
/// goes): run after the kept room's own run when its socket wakes the
/// hidden page ([`wake_hidden`]), so that a stage the room starts while
/// the page is hidden starts its play as the start comes (the maintainer,
/// 2026-11-17), not at the hidden page's next timer.
pub(crate) fn set_unseen_prompt(step: Option<Rc<dyn Fn()>>) {
    UNSEEN_PROMPT.with_borrow_mut(|s| *s = step);
}

/// A hidden page's reconnect is tried at once no more often than this
/// (ms): a socket that closes again and again (a server that is down)
/// backs off as ever in between.
const HURRY_EVERY_MS: u64 = 5_000;

thread_local! {
    /// The page's own title, while a hidden tab's says a stage is starting
    /// ([`tell_hidden`]).
    static TAB_TITLE: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// A hidden tab cannot show its member the stage's Ready screen: while
/// the room's stage gathers readiness and waits for theirs
/// (`RoomClient::ready_wanted`), the tab's title says the stage is
/// starting (the browser shows it on the tab); the page's own title is
/// back once the page is shown, the wait is over or the room is gone
/// (`room`: none).
fn tell_hidden(room: Option<&SharedRoom>) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let number = room
        .filter(|_| page_hidden())
        .and_then(|room| room.try_borrow().ok()?.ready_wanted());
    TAB_TITLE.with_borrow_mut(|own| match (number, own.is_some()) {
        (Some(number), false) => {
            *own = Some(document.title());
            let title: String = ddi_l10n::msg::room::due_title(number).into();
            document.set_title(&title);
        }
        (None, true) => {
            if let Some(title) = own.take() {
                document.set_title(&title);
            }
        }
        _ => {}
    });
}

/// How often the kept online room is run from its timer (ms; a hidden
/// page's timers come at the browser's own pace, a second or more apart).
const KEPT_PUMP_MS: u32 = 100;

/// Whether the page is hidden (a tab in the background, a minimized
/// window): the browser draws no frame for it and slows its timers.
fn page_hidden() -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .is_some_and(|d| d.hidden())
}

/// An online room kept is run from a timer of its own wherever no frame
/// runs it, so that its picks are prepared and uploaded and the room hears
/// from it:
///
/// - while its member is on a page over it (Options, Calibrate: review3
///   web-3), which draws only when it changes; a stage due meanwhile is
///   told over the page ([`tell_due`]), and its prompt opens once the
///   member is back on its song select (as desktop's, whose room screen
///   runs under Options);
/// - on a stage's results ([`on_results`]): the next stage's song is
///   prepared while its member is still there, and the results tell them
///   it is due themselves (`ddi_ui`'s `results_stage.rs`);
/// - on any of its pages while the page is hidden (feedback round N1): a
///   browser stops a hidden page's frames, so nothing ran the room there:
///   a pick was never loaded (the room waited its whole loading grace for
///   the member and started without them) and a socket that dropped was
///   never opened again (the room counted the member away). A hidden
///   page's timers still come, a second apart or more, and what the
///   socket and the server's answers bring wakes it at once
///   ([`wake_hidden`]).
fn run_kept(page: &RoomPage) {
    let wanted = matches!(page, RoomPage::Online(_) | RoomPage::Over | RoomPage::Stage)
        && crate::online::kept_room().is_some();
    if !wanted {
        tell_hidden(None);
    }
    KEPT_PUMP.with_borrow_mut(|pump| {
        if !wanted {
            *pump = None;
        } else if pump.is_none()
            && let Some(document) = web_sys::window().and_then(|w| w.document())
        {
            // (Stopped by the next page's first frame, never from inside
            // its own call.)
            let timer = gloo::timers::callback::Interval::new(KEPT_PUMP_MS, || match room_page() {
                RoomPage::Over => pump_kept(true),
                RoomPage::Stage if on_results() => pump_kept(false),
                RoomPage::Online(_) | RoomPage::Stage if page_hidden() => pump_kept(false),
                _ => {}
            });
            let shown = gloo::events::EventListener::new(&document, "visibilitychange", |_| {
                if !page_hidden() {
                    shown_again();
                }
            });
            *pump = Some((timer, shown));
        }
    });
}

/// Something came for the kept online room (a message on its socket, its
/// socket's close, a request's answer) while the page is hidden: the room
/// is run now, not at the hidden page's next timer, which may be a minute
/// away (Chrome, after five minutes hidden). A socket that `closed` is
/// opened again at once ([`RoomClient::hurry`]: the reconnect's first wait
/// would be a timer's), at most once in [`HURRY_EVERY_MS`].
pub(crate) fn wake_hidden(closed: bool) {
    if !page_hidden()
        || (crate::online::kept_room().is_none() && crate::online::kept_audience().is_none())
    {
        return;
    }
    if closed {
        CLOSED.set(true);
    }
    if WAKING.replace(true) {
        return;
    }
    wasm_bindgen_futures::spawn_local(async {
        WAKING.set(false);
        let closed = CLOSED.replace(false);
        if !page_hidden() {
            return;
        }
        // The audience's page (round Y3): its seat's socket woke it; a
        // lost one replaced at once, then the page stepped unseen.
        if matches!(room_page(), RoomPage::Audience(_)) {
            let now = now_ms();
            if closed && now.saturating_sub(HURRIED_AT.get()) >= HURRY_EVERY_MS {
                HURRIED_AT.set(now);
                hurry_audience();
            }
            if let Some(step) = UNSEEN_PROMPT.with_borrow(Clone::clone) {
                step();
            }
            return;
        }
        if crate::online::kept_room().is_none()
            || !matches!(
                room_page(),
                RoomPage::Online(_) | RoomPage::Over | RoomPage::Stage
            )
        {
            return;
        }
        pump_kept(false);
        // A stage's prompt on the page: stepped now too, so that the
        // start the socket brought starts its play now.
        if let Some(step) = UNSEEN_PROMPT.with_borrow(Clone::clone) {
            step();
        }
        let now = now_ms();
        if closed && now.saturating_sub(HURRIED_AT.get()) >= HURRY_EVERY_MS {
            HURRIED_AT.set(now);
            hurry_kept();
        }
    });
}

/// The kept audience seat's reconnect tried at its next run where one
/// waits (`AudienceClient::hurry`).
fn hurry_audience() {
    if let Some(a) = crate::online::kept_audience()
        && let Ok(mut a) = a.try_borrow_mut()
    {
        a.hurry(now_ms());
    }
}

/// The audience's page while it is hidden (round Y3): a browser draws no
/// frame for it, so nothing ran its seat there (its socket unread, a lost
/// one never replaced, a stage's song never readied). Stepped unseen from
/// a timer of its own (`ddi_ui`'s `Screen::unseen`: the seat run as a
/// frame runs it), and at once when its socket brings something or closes
/// ([`wake_hidden`], a lost socket's reconnect hurried); shown again, a
/// reconnect that waits is tried at once. As a member's room is run on a
/// hidden page ([`run_kept`]). Dropped with the page.
pub(crate) struct AudienceUnseen {
    _timer: gloo::timers::callback::Interval,
    _shown: Option<gloo::events::EventListener>,
}

impl Drop for AudienceUnseen {
    fn drop(&mut self) {
        set_unseen_prompt(None);
    }
}

/// [`AudienceUnseen`] for the audience's page run by `host`.
pub(crate) fn audience_unseen(host: &Rc<RefCell<crate::ui_host::Host>>) -> AudienceUnseen {
    let weak = Rc::downgrade(host);
    let step: Rc<dyn Fn()> = Rc::new(move || {
        if page_hidden()
            && let Some(host) = weak.upgrade()
        {
            crate::ui_host::step_unseen(&host);
        }
    });
    set_unseen_prompt(Some(step.clone()));
    let shown = web_sys::window().and_then(|w| w.document()).map(|d| {
        gloo::events::EventListener::new(&d, "visibilitychange", |_| {
            if !page_hidden() {
                hurry_audience();
            }
        })
    });
    AudienceUnseen {
        _timer: gloo::timers::callback::Interval::new(KEPT_PUMP_MS, move || step()),
        _shown: shown,
    }
}

/// The page is shown again: the kept room run at once, and a reconnect
/// that waits (a wait reckoned while the page stood still) tried now.
fn shown_again() {
    if !matches!(room_page(), RoomPage::Online(_) | RoomPage::Over | RoomPage::Stage) {
        return;
    }
    pump_kept(false);
    hurry_kept();
}

/// The kept room's reconnect tried now where one waits, and the room run
/// once more so that its ticket is asked for.
fn hurry_kept() {
    let Some(room) = crate::online::kept_room() else {
        return;
    };
    let hurried = room.try_borrow_mut().is_ok_and(|mut r| r.hurry());
    if hurried {
        pump_kept(false);
    }
}

/// On a stage's page with its play over (its outcome reported, or its
/// prompt left): its results, where the page itself no longer runs the
/// room.
fn on_results() -> bool {
    PLAYING.with_borrow(Option::is_none)
}

/// The kept online room's one run: ticked, its songs to prepare and
/// upload handed on, and with `tell` a stage due told over the page.
fn pump_kept(tell: bool) {
    let Some(room) = crate::online::kept_room() else {
        return;
    };
    let (jobs, uploads, ahead) = {
        let Ok(mut r) = room.try_borrow_mut() else {
            return;
        };
        r.tick(now_ms());
        (r.take_load_jobs(), r.take_upload_jobs(), r.take_preload_jobs())
    };
    for job in jobs {
        prepare_for(room.clone(), job);
    }
    for job in uploads {
        upload(room.clone(), job);
    }
    for job in ahead {
        preload(room.clone(), job);
    }
    if tell {
        tell_due(&room);
    }
    tell_hidden(Some(&room));
}

/// The kept online room's stage is due for its member while they are on
/// a page over it (the maintainer, 2026-11-15: "Tell them"): a notice
/// over that page (`ddi_ui::screens::room::DueDialog`), whose button goes
/// to the room's song select, where the stage's prompt opens. The page is
/// never left for them; on a page without the UI's screens (a
/// calibration's run, a studio scene's play) they are told once back on
/// one.
fn tell_due(room: &SharedRoom) {
    use ddi_ui::screens::room::{DueDialog, DueGo};
    let Some(id) = crate::online::kept_room_id() else {
        return;
    };
    // Cheap while nothing is due or it is told already.
    let due = room.try_borrow().is_ok_and(|r| {
        r.stage_due()
            .is_some_and(|d| r.telling_due().get() != Some(d.stage))
    });
    if !due {
        return;
    }
    let go: DueGo = Rc::new(move |cx| {
        cx.request(ddi_ui::UiRequest::Navigate(
            Route::MpRoom { id: id.clone() }.to_hash(),
        ))
    });
    crate::ui_host::push_dialog(|env| DueDialog::due(env, room, go));
}

/// Registers the browser's `beforeunload` question (closing or reloading
/// the tab: the browser shows its own words, and only once the person
/// has used the page) while leaving a room costs something or the chart
/// editor has unsaved changes on its page, and removes it otherwise:
/// never there for the rest of singleplayer, nor for a room that can be
/// left at no cost. Called each frame of the UI's pages; cheap.
///
/// The page cannot tell the room it goes (nothing sent from an unload
/// arrives reliably, and a reload must not make its player leave): the
/// socket closes with the page, and the room gives the player its
/// reconnect grace before it counts them away.
pub(crate) fn sync_unload_guard() {
    let page = room_page();
    forget_left_practice(&page);
    leave_left_online(&page);
    leave_left_audience(&page);
    run_kept(&page);
    // And while the chart editor has unsaved changes on its page.
    let want = leaving_costs(&page) || crate::editor::unsaved_here();
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

/// The online room kept, for `__DDI_DEBUG.ui.keptRoom`: its id, its
/// phase, its upcoming stage and whether the room still waits for this
/// player to load its song (null without one).
pub(crate) fn kept_debug() -> wasm_bindgen::JsValue {
    let (Some(id), Some(room)) = (crate::online::kept_room_id(), crate::online::kept_room()) else {
        return wasm_bindgen::JsValue::NULL;
    };
    let Ok(r) = room.try_borrow() else {
        return wasm_bindgen::JsValue::NULL;
    };
    let stage = r.state().upcoming_stage();
    let o = js_sys::Object::new();
    let set = |k: &str, v: wasm_bindgen::JsValue| {
        let _ = js_sys::Reflect::set(&o, &k.into(), &v);
    };
    set("id", id.into());
    set("phase", format!("{:?}", r.state().phase.kind()).into());
    set("stage", stage.map_or(wasm_bindgen::JsValue::NULL, |s| (s.0 as f64).into()));
    set("loading", stage.is_some_and(|s| r.loading(s)).into());
    o.into()
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
            // A room gone for its member (round BM: removed, kept out,
            // closed): never reached again by the browser's Back.
            if room.try_borrow().is_ok_and(|r| r.room_gone()) {
                crate::router::returning_to(&Route::Multiplayer.to_hash());
            }
            match &which {
                RoomRoute::Practice => discard(),
                RoomRoute::Online(_) => crate::online::discard(),
            }
            cx.request(UiRequest::Navigate(Route::Multiplayer.to_hash()));
        }
        RoomCall::Redirect(r) => crate::components::page_routes::redirect(r),
        RoomCall::Prepare(job) => prepare_for(room.clone(), job),
        RoomCall::Upload(job) => upload(room.clone(), *job),
        RoomCall::Preload(job) => preload(room.clone(), job),
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
                RoomCall::Preload(job) => preload(room.clone(), job),
                RoomCall::Redirect(r) => crate::components::page_routes::redirect(r),
                _ => {}
            }
        })
    };
    (room, shell)
}

/// The rooms' clock (round P1b, `ddi_platform::steady`), which the
/// screens tick them on too (`ddi_ui::Ui::set_wall_ms`, set each frame by
/// `crate::ui_host`) and a live play reads: the page's time origin plus
/// `performance.now()`, ms on the wall clock's scale that never go
/// backwards, so a device's clock set back or forward (a network time
/// correction, a clock set by hand) never stalls nor skips a countdown.
/// A browser may stop it while the computer sleeps: a practice room's
/// countdown then goes on where it stood, and an online room's clock
/// estimate takes the step at its next ping (`ddi_room::live::clock`).
/// Dates (a play's, an import's) stay `Date.now()`'s.
pub(crate) fn now_ms() -> u64 {
    thread_local! {
        static STEADY: std::cell::RefCell<ddi_platform::steady::SteadyClock> =
            std::cell::RefCell::default();
    }
    let p = web_sys::window().and_then(|w| w.performance());
    let (origin, running) = p.map_or((js_sys::Date::now(), 0.0), |p| (p.time_origin(), p.now()));
    // (Anchored at the page's time origin: its wall time at the start.)
    STEADY.with_borrow_mut(|s| s.read(origin + running, running))
}

/// The session's practice room, made at first.
pub(crate) fn current() -> SharedRoom {
    let room = ROOM.with(|r| {
        r.borrow_mut()
            .get_or_insert_with(|| ddi_ui::state::room::practice_room(now_ms()))
            .clone()
    });
    // The player's own row: their signed-in name where they are (not
    // while the room is borrowed: the next hand-over sets it).
    if let Ok(mut r) = room.try_borrow_mut() {
        r.set_own_name(crate::online::signed_in_name());
    }
    room
}

/// DEVELOPMENT ONLY: practice against `cpus` CPUs, beyond the cap, from
/// the next practice room on (the one there is left): for trying the
/// miniature fields with far more opponents. Never from a menu.
pub(crate) fn develop(cpus: u16) {
    ddi_ui::state::room::set_development(ddi_ui::state::room::Development {
        cpus,
        ..ddi_ui::state::room::development()
    });
    discard();
}

/// Leave room: the practice forgotten; the next visit makes a new one.
pub(crate) fn discard() {
    ROOM.with(|r| r.borrow_mut().take());
    PLAYING.with(|p| p.borrow_mut().take());
}

/// A room left (an online one, `crate::online::discard`): where the stage
/// handed over last was of it, that is forgotten too (the stage being
/// played and the room its results go back to), so that its client does
/// not live on.
pub(crate) fn forget_playing(room: &SharedRoom) {
    PLAYING.with_borrow_mut(|p| {
        if p.as_ref().is_some_and(|p| Rc::ptr_eq(&p.room, room)) {
            *p = None;
        }
    });
    LAST.with_borrow_mut(|l| {
        if l.as_ref().is_some_and(|(_, r)| Rc::ptr_eq(r, room)) {
            *l = None;
        }
    });
}

/// Prepares a stage's song on this device for `room`: its chart read (as a
/// play reads it) and checked (`ddi_ui::state::room::prepare_stage`: the
/// chart chosen, the CPUs' plan in practice); an online member's from the
/// room's share (`crate::share`, held for the session), its chart found
/// by the digest chosen. The room told it loaded or why not.
pub(crate) fn prepare_for(room: SharedRoom, job: LoadJob) {
    // TESTING ONLY: the fake room's song is the bundled one.
    if crate::fake_room::on() && job.share.is_some() && room.borrow().is_online() {
        crate::fake_room::prepare(room, job);
        return;
    }
    wasm_bindgen_futures::spawn_local(async move {
        let warn = |e: &str| {
            web_sys::console::warn_1(&format!("room: stage song {}: {e}", job.song).into())
        };
        if job.share.is_some() {
            // How far its music is, told the room as it comes.
            let progress = |done: f64, total: f64| {
                if let Ok(mut r) = room.try_borrow_mut() {
                    r.load_progress(&job, done, total);
                }
            };
            let result = read_shared(&job, &progress).await;
            room.borrow_mut().finish_shared_found(&job, result);
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

/// A stage's song read from the room's share `job` names, its chart found
/// by the digest chosen, with what live play needs of it, and, for a
/// stage (not the song the picker rests on), its music fetched to the end
/// (`progress`: its bytes so far and in all): a stage's song is loaded
/// once it can be played. (Feedback round N1: the music was fetched only
/// on the stage's Ready screen, after the room had been told the song was
/// loaded; on a slow connection the stage started without it, and its
/// player, more than the late join behind, forfeited and watched.)
async fn read_shared(
    job: &LoadJob,
    progress: &dyn Fn(f64, f64),
) -> Result<SharedFound, LoadProblem> {
    let warn =
        |e: &str| web_sys::console::warn_1(&format!("room: stage song {}: {e}", job.song).into());
    let share = job.share.as_deref().ok_or(LoadProblem::Download)?;
    // A member's stage song (stage 2): into the room's area first, so
    // that what follows is read on this device and the song is here for
    // the room's list after (out of room: down the link as before).
    let mut stored = false;
    if let Some(store) = job.store.as_deref()
        && job.stage != ddi_ui::state::room::SPOTLIGHT
    {
        let stop = ddi_ui::state::room::Stop::default();
        match crate::room_store::song(store, share, progress, &stop).await {
            Ok(()) => stored = true,
            Err(ddi_ui::state::room::PreloadProblem::Quota) => {}
            Err(_) => return Err(LoadProblem::Download),
        }
    }
    let loaded = crate::share::load(share).await.map_err(|e| {
        warn(&e.to_string());
        LoadProblem::Download
    })?;
    let song = crate::songs::load_chart(&job.song).await.map_err(|e| {
        warn(&format!("{e:?}"));
        LoadProblem::Unreadable
    })?;
    let p = prepare_stage(job, &song, &mut |e| warn(e))?;
    let record = &loaded.songs.first().ok_or(LoadProblem::Download)?.record;
    let entry = &record.entry;
    if job.stage != ddi_ui::state::room::SPOTLIGHT
        && !stored
        && let Some(url) = crate::share::file_url(&entry.id, entry.music_for(p.index).as_str())
    {
        crate::songs::prefetch(&url, progress).await.map_err(|e| {
            warn(&e);
            LoadProblem::Download
        })?;
    }
    Ok(SharedFound {
        banner: entry
            .banner
            .as_ref()
            .map(|b| crate::songs::picture_source(&entry.id, b.as_str())),
        background: entry
            .background
            .as_ref()
            .map(|b| crate::songs::picture_source(&entry.id, b.as_str())),
        entry: entry.clone(),
        index: p.index,
        pack: record.pack.clone(),
        live: p.live,
    })
}

/// Stage 2 of the room's song list: a listed song downloaded ahead into
/// the room's area (`crate::room_store`), or deleted from it; the room
/// told how it goes. (Testing's fake rooms have no share to read: their
/// songs are the bundled ones, here at once.)
pub(crate) fn preload(room: SharedRoom, job: ddi_ui::state::room::PreloadJob) {
    use ddi_ui::state::room::PreloadJob;
    wasm_bindgen_futures::spawn_local(async move {
        match job {
            PreloadJob::Listing { song, share, store } => {
                let result = crate::room_store::listing(&store, &share).await;
                room.borrow_mut().preload_listed(song, result);
            }
            PreloadJob::Song {
                song,
                share,
                store,
                stop,
            } => {
                let progress = |done: f64, total: f64| {
                    if let Ok(mut r) = room.try_borrow_mut() {
                        r.preload_progress(song, done, total);
                    }
                };
                let result = crate::room_store::song(&store, &share, &progress, &stop).await;
                room.borrow_mut().preload_done(song, result);
            }
            PreloadJob::Forget { share, store } => crate::room_store::forget(&store, &share).await,
        }
    });
}

/// A stage's song of the room watched with its audience
/// (`crate::online::audience`), read from the room's share.
pub(crate) fn prepare_audience(job: LoadJob) {
    if crate::fake_seat::on() {
        crate::fake_seat::prepare(job);
        return;
    }
    wasm_bindgen_futures::spawn_local(async move {
        let result = read_shared(&job, &|_, _| {}).await;
        if let Some(a) = crate::online::kept_audience() {
            a.borrow_mut().finish_found(&job, result);
        }
    });
}

/// An online room's stage song uploaded as the room's share under its
/// grant (`crate::share::upload_room`), the room told how it goes and how
/// it ended.
pub(crate) fn upload(room: SharedRoom, job: UploadJob) {
    wasm_bindgen_futures::spawn_local(async move {
        let progress = {
            let room = room.clone();
            move |done: f64, total: f64| match job.entry {
                // A song added to the room's list (revision 17).
                Some(song) => room.borrow_mut().song_upload_progress(song, done, total),
                None => room.borrow_mut().upload_progress(job.stage, done, total),
            }
        };
        let result = crate::share::upload_room(&job.song.id, &job.grant, &progress).await;
        if let Err(e) = &result {
            let what = match job.entry {
                Some(song) => format!("the room's song {song}"),
                None => format!("stage {}", job.stage),
            };
            web_sys::console::warn_1(&format!("room: sharing {what}: {e}").into());
        }
        match job.entry {
            Some(song) => room.borrow_mut().song_upload_finished(song, result),
            None => room.borrow_mut().upload_finished(job.stage, result),
        }
    });
}

/// A stage of `room` to play: kept for the play route, whose address it
/// gives (`#/play?song=…&chart=…&stage=<id>`, with the session's testing
/// parameters); its results go back to `back`.
pub(crate) fn play_in(room: SharedRoom, back: Route, order: StageOrder) -> String {
    let online = match &back {
        Route::MpRoom { id } => Some(id.clone()),
        _ => None,
    };
    let route = Route::Play(crate::router::PlayRoute {
        stage: Some(order.stage.0),
        room: online.clone(),
        ..crate::router::PlayRoute::song(order.song.id.clone(), order.chart.index)
    });
    LAST.with(|l| *l.borrow_mut() = Some((back, room.clone())));
    PLAYING.with(|p| {
        *p.borrow_mut() = Some(Playing {
            order: Rc::new(order),
            room,
        })
    });
    // The session's testing parameters for a practice room's stages
    // alone: never an online room's, whose run is reported to the others
    // as the person's own (review3 web-6: `auto=1` autoplayed it).
    if online.is_some() {
        return route.to_hash();
    }
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
/// and streams through it. Watched with the room's audience (`watched`),
/// its seat's client feeds it instead.
pub(crate) fn live_handle(
    order: &StageOrder,
    watched: bool,
) -> Option<ddi_session::battle::live::LiveHandle> {
    let room: Rc<RefCell<dyn ddi_ui::state::room::LiveRoom>> = if watched {
        crate::online::audience_stage(order)?
    } else {
        order.live.as_ref()?;
        playing_room()
    };
    Some(ddi_session::battle::live::LiveHandle {
        room,
        wall_ms: Rc::new(now_ms),
        // The clock a play's frames are timed on (`performance.now()`).
        host_s: Rc::new(crate::web::clock::PerformanceClock::seconds()),
    })
}

/// The local player's attack policy as stage `order`'s play starts: the
/// one they have now in the room that handed the stage over, where it is
/// that room's stage (it may have changed on the stage's prompt), else
/// the order's own (as desktop's `stage_targeting`).
pub(crate) fn stage_targeting(order: &StageOrder) -> ddi_room::attacks::Targeting {
    let room = PLAYING.with(|p| {
        p.borrow()
            .as_ref()
            .filter(|p| p.order.stage == order.stage)
            .map(|p| p.room.clone())
    });
    room.and_then(|room| {
        let r = room.try_borrow().ok()?;
        (r.me() == order.me && r.stage_started(order.stage)).then(|| r.my_targeting())
    })
    .unwrap_or(order.targeting)
}

/// Stage `order`'s `of` as its play starts (round AC): the room's, where
/// the Ready card may have made it the match's last song since the order
/// was made ([`ddi_ui::state::room::RoomClient::stage_of`]).
pub(crate) fn stage_of(order: &StageOrder) -> u16 {
    let room = PLAYING.with(|p| {
        p.borrow()
            .as_ref()
            .filter(|p| p.order.stage == order.stage)
            .map(|p| p.room.clone())
    });
    room.and_then(|room| {
        let r = room.try_borrow().ok()?;
        (r.me() == order.me && r.stage_started(order.stage)).then(|| r.stage_of(order.stage))
    })
    .unwrap_or(order.of)
}

/// Stage `order`'s own starting chart as its play starts (room model
/// revision 23): the room's, where it may have been told or chosen on the
/// Ready card since the order was made
/// ([`ddi_ui::state::room::RoomClient::start_for`]).
pub(crate) fn stage_start(order: &StageOrder) -> Option<ddi_room::ChartKey> {
    let room = PLAYING.with(|p| {
        p.borrow()
            .as_ref()
            .filter(|p| p.order.stage == order.stage)
            .map(|p| p.room.clone())
    });
    room.and_then(|room| Some(room.try_borrow().ok()?.start_for(order)))
        .unwrap_or_else(|| order.start.clone())
}

/// Where a stage's results go back to: the room it was played in.
pub(crate) fn back_route() -> Route {
    LAST.with(|l| l.borrow().as_ref().map(|(r, _)| r.clone()))
        .unwrap_or(Route::Room)
}

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
            if last_room().try_borrow().is_ok_and(|r| r.room_gone()) {
                crate::router::returning_to(&Route::Multiplayer.to_hash());
            }
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

/// The room the last stage handed over is of (its results ask it how the
/// stage went).
pub(crate) fn last_room() -> SharedRoom {
    kept_last_room().unwrap_or_else(current)
}

/// [`last_room`] while it is kept (never the practice room made for the
/// asking: the debug state's).
pub(crate) fn kept_last_room() -> Option<SharedRoom> {
    LAST.with(|l| l.borrow().as_ref().map(|(_, r)| r.clone()))
}

/// The room of the stage being played (the practice room otherwise).
fn playing_room() -> SharedRoom {
    PLAYING
        .with(|p| p.borrow().as_ref().map(|p| p.room.clone()))
        .unwrap_or_else(current)
}

/// A stage's outcome (`ddi_session::battle`), reported to the room that
/// handed it over (`ddi_session::battle::report_outcome`: the policy the
/// play left the player with kept for the next stage wherever the stage
/// had attacks, online too). How the match ended for the player, where
/// that was its last stage (an online room's is not known yet: `None`).
pub(crate) fn report(
    outcome: ddi_session::battle::StageOutcome,
) -> Option<ddi_ui::state::room::MatchEnd> {
    let handed = PLAYING.with(|p| p.borrow_mut().take_if(|p| p.order.stage == outcome.stage));
    // A stage watched with a room's audience reports nothing (the seat is
    // no member).
    if handed.is_none() && crate::online::watching_stage() == Some(outcome.stage) {
        return None;
    }
    let room = handed.map_or_else(playing_room, |p| p.room);
    let reported = ddi_session::battle::report_outcome(&mut room.borrow_mut(), outcome);
    if let Err(why) = reported.taken {
        web_sys::console::warn_1(&format!("room: the stage's outcome was refused: {why:?}").into());
    }
    reported.ending
}

/// A stage's prompt left (its Back, M7b;
/// `ddi_session::battle::leave_prompt`): before the room started it, the
/// player withdraws; once on, a forfeit, an online room's as the player's
/// own report, a practice's CPUs played to their end on `song` (as read)
/// and the stage reported; without a song to play them on, the match
/// stops. The stage handed over is forgotten.
pub(crate) fn leave_prompt(order: &StageOrder, song: Option<&ddi_chart::Song>) {
    use ddi_session::battle::LeftPrompt;
    let room = playing_room();
    let left = ddi_session::battle::leave_prompt(&mut room.borrow_mut(), order, song);
    PLAYING.with(|p| p.borrow_mut().take());
    match left {
        LeftPrompt::Forfeited(Err(why)) => {
            web_sys::console::warn_1(&format!("room: the forfeit was refused: {why:?}").into())
        }
        LeftPrompt::Lost(Some(e)) => {
            web_sys::console::warn_1(&format!("room: stage {}: {e}", order.number).into())
        }
        _ => {}
    }
}
