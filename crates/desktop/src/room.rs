//! The multiplayer prototype on desktop
//! (`docs/plans/multiplayer-prototype-progress.md`, M2; behind no switch
//! since 2026-11-14): the multiplayer home, the practice room over it and song select choosing a stage's
//! song over the room, all on the UI's stack. The room lives here for the
//! run, so leaving it and coming back finds it as it was; Leave room
//! forgets it. The screens are the UI core's (`ddi_ui::screens::room`),
//! bound to the shared practice state; nothing of the room is kept here.
//!
//! An online room's (M5d) stage songs: the host's pick uploaded as the
//! room's share under its grant ([`upload`], on a thread), each member's
//! preparation from the library or from the room's share (read into the
//! cache on a thread, `crate::share::load`), both reporting through the
//! room client's inbox ([`inbox`]); its stages played over the room as
//! practice's are, the player's own run reported to the room ([`report`]).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};

use ddi_library::manifest::ManifestEntry;
use ddi_library::share::ShareError;
use ddi_room::StageId;
use ddi_ui::Cx;
use ddi_ui::screens::Env;
use ddi_ui::screens::online::{OnlineCall, OnlinePage, OnlineScreen};
use ddi_ui::screens::room::{RoomCall, RoomShell};
use ddi_ui::screens::select::{SelectCall, SelectScreen};
use ddi_ui::state::online::Redirect;
use ddi_ui::state::place::MenuPlace;
use ddi_ui::state::settings::Settings;
use ddi_ui::state::room::{
    Inbox, LivePrep, LoadJob, LoadProblem, SharedRoom, ShellNews, UploadJob, picked_of,
    prepare_stage,
};

use crate::preview::SharedPlayer;
use crate::songs::Songs;

thread_local! {
    static ROOM: RefCell<Option<SharedRoom>> = const { RefCell::new(None) };
    /// The room of the stage handed to the play host last.
    static PLAYING: RefCell<Option<SharedRoom>> = const { RefCell::new(None) };
    /// Where the online room's threads send their news ([`inbox`]).
    static NEWS: RefCell<Option<Sender<News>>> = const { RefCell::new(None) };
    /// The stage handed to the play host to watch with the room's
    /// audience, and its seat's client ([`audience_stage`]).
    static WATCHING: RefCell<Option<(StageId, ddi_ui::state::audience::SharedAudience)>> =
        const { RefCell::new(None) };
    /// Where the audience's threads send theirs ([`audience_inbox`]).
    static AUDIENCE_NEWS: RefCell<Option<NewsTx>> = const { RefCell::new(None) };
    /// TESTS ONLY: the wall clock a live stage's play reads the room's
    /// clock through ([`live_handle`]), where a test runs on a fake one.
    static TEST_WALL: RefCell<Option<Rc<dyn Fn() -> u64>>> = const { RefCell::new(None) };
}

/// TESTS ONLY (an offscreen live stage on a fake clock): the wall clock
/// (ms since the epoch) a live stage's play reads, and the room its stage
/// is played in (as a pick's prompt hands it over), in place of the
/// system's and the room screen's.
pub fn set_live_for_tests(room: SharedRoom, wall_ms: Rc<dyn Fn() -> u64>) {
    PLAYING.with_borrow_mut(|p| *p = Some(room));
    TEST_WALL.with_borrow_mut(|w| *w = Some(wall_ms));
}

/// TESTS ONLY (an offscreen audience's stage on a fake clock): stage
/// `stage` watched with the room's audience through `audience`, the wall
/// clock its play reads `wall_ms` (as the audience's page hands it over).
pub fn set_audience_for_tests(
    stage: StageId,
    audience: ddi_ui::state::audience::SharedAudience,
    wall_ms: Rc<dyn Fn() -> u64>,
) {
    WATCHING.with_borrow_mut(|w| *w = Some((stage, audience)));
    TEST_WALL.with_borrow_mut(|w| *w = Some(wall_ms));
}

/// A thread's news for the online room (`Send`; turned into the room
/// client's [`ShellNews`] on the UI's thread).
enum News {
    Progress {
        stage: StageId,
        done: f64,
        total: f64,
    },
    Uploaded {
        stage: StageId,
        result: Result<String, ShareError>,
    },
    PreparedShared {
        job: Box<LoadJob>,
        result: Result<Box<SharedSong>, LoadProblem>,
    },
}

/// A stage's song read from the room's share: its entry, the chart's index,
/// its pack and its banner's source.
struct SharedSong {
    entry: ManifestEntry,
    index: usize,
    pack: String,
    banner: Option<String>,
    /// What live play needs of its chart (`prepare_stage`).
    live: Option<LivePrep>,
}

/// Where an online room's threads send their news: one room client's
/// [`Inbox`] (the run's room's, [`inbox`], or a bot's, `crate::bots`).
#[derive(Clone)]
pub struct NewsTx(Sender<News>);

/// The online room client's inbox: what this module's threads report.
pub fn inbox() -> Inbox {
    let (tx, inbox) = news_inbox();
    NEWS.with_borrow_mut(|n| *n = Some(tx.0));
    inbox
}

/// A room client's inbox of its own, and where its threads' news goes
/// ([`upload_with`], [`prepare_online_with`]).
pub fn news_inbox() -> (NewsTx, Inbox) {
    let (tx, rx) = channel::<News>();
    let inbox: Inbox = Box::new(move || {
        rx.try_iter()
            .map(|n| match n {
                News::Progress { stage, done, total } => ShellNews::Progress { stage, done, total },
                News::Uploaded { stage, result } => ShellNews::Uploaded { stage, result },
                News::PreparedShared { job, result } => {
                    let live = result.as_ref().ok().and_then(|f| f.live.clone());
                    ShellNews::PreparedShared {
                        result: result.and_then(|found| {
                            picked_of(&found.entry, found.index, &found.pack, found.banner)
                                .map(Box::new)
                                .ok_or(LoadProblem::Changed)
                        }),
                        job,
                        live,
                    }
                }
            })
            .collect()
    });
    (NewsTx(tx), inbox)
}

fn news() -> Option<NewsTx> {
    NEWS.with_borrow(|n| n.clone().map(NewsTx))
}

/// An online room's stage song uploaded as the room's share under its
/// grant, on a thread (`crate::share::upload_room`), its news to the
/// online room's [`inbox`].
pub fn upload(job: UploadJob) {
    if let Some(tx) = news() {
        upload_with(&tx, job);
    }
}

/// [`upload`], its news to `tx`.
pub fn upload_with(tx: &NewsTx, job: UploadJob) {
    let tx = tx.0.clone();
    let song = job.song.id.clone();
    let stage = job.stage;
    let grant = job.grant;
    let spawned = std::thread::Builder::new()
        .name("ddi-room-upload".into())
        .spawn(move || {
            let progress_tx = tx.clone();
            let mut progress = move |done: f64, total: f64| {
                let _ = progress_tx.send(News::Progress { stage, done, total });
            };
            let result = crate::share::upload_room(&song, &grant, &mut progress);
            if let Err(e) = &result {
                eprintln!("room: sharing stage {stage}: {e}");
            }
            let _ = tx.send(News::Uploaded { stage, result });
        });
    if let Err(e) = spawned {
        eprintln!("room: sharing stage {stage}: {e}");
    }
}

/// A check of a stage's song as its preparation read it (a bot's: its
/// CPU plans the chart), on the thread that read it: the job, the song
/// and its chart's index. Refused, the stage fails to load.
pub type SongCheck =
    Arc<dyn Fn(&LoadJob, ddi_chart::Song, usize) -> Result<(), LoadProblem> + Send + Sync>;

/// An online room's stage song prepared on this device: the host's from
/// the library at once; a member's from the room's share, read into the
/// cache on a thread and found by the digest chosen (its news to the
/// online room's [`inbox`]).
pub fn prepare_online(room: &SharedRoom, job: LoadJob) {
    prepare_online_with(news().as_ref(), room, job, None);
}

/// [`prepare_online`], a share's news to `tx`, the song as read checked
/// by `check` where there is one.
pub fn prepare_online_with(
    tx: Option<&NewsTx>,
    room: &SharedRoom,
    job: LoadJob,
    check: Option<SongCheck>,
) {
    let Some(share) = job.share.clone() else {
        prepare_library(room, &job, true, check.as_ref());
        return;
    };
    let Some(tx) = tx.map(|t| t.0.clone()) else {
        return;
    };
    let spawned = std::thread::Builder::new()
        .name("ddi-room-share".into())
        .spawn(move || {
            let result = (|| {
                let loaded = crate::share::load(&share, &|| {}).map_err(|e| {
                    eprintln!("room: reading share {share}: {e}");
                    LoadProblem::Download
                })?;
                crate::library::register_room_songs(&loaded.songs);
                let song = crate::play::load::read_chart(&job.song).map_err(|e| {
                    eprintln!("room: stage song {} unreadable: {e:?}", job.song);
                    LoadProblem::Unreadable
                })?;
                let prepared = prepare_stage(&job, &song, &mut |e| eprintln!("room: {e}"))?;
                if let Some(check) = &check {
                    check(&job, song, prepared.index)?;
                }
                let first = loaded.songs.first().ok_or(LoadProblem::Download)?;
                Ok(Box::new(SharedSong {
                    entry: first.entry.clone(),
                    index: prepared.index,
                    pack: first.pack.clone(),
                    banner: first.entry.banner.as_deref().map(|b| first.source(b)),
                    live: prepared.live,
                }))
            })();
            let _ = tx.send(News::PreparedShared {
                job: Box::new(job),
                result,
            });
        });
    if let Err(e) = spawned {
        eprintln!("room: preparing a stage song: {e}");
    }
}

/// The audience's client watching `order`, where it was handed over to
/// watch with the room's audience ([`audience_screen`]).
pub fn audience_stage(
    order: &ddi_ui::state::room::StageOrder,
) -> Option<ddi_ui::state::audience::SharedAudience> {
    if !order.for_audience() {
        return None;
    }
    WATCHING.with_borrow(|w| {
        w.as_ref()
            .filter(|(s, _)| *s == order.stage)
            .map(|(_, a)| a.clone())
    })
}

/// The room watched with its audience's inbox: its stages' songs read
/// from the room's share on threads ([`prepare_audience`]).
pub fn audience_inbox() -> ddi_ui::state::audience::AudienceInbox {
    let (tx, mut inbox) = news_inbox();
    AUDIENCE_NEWS.with_borrow_mut(|n| *n = Some(tx));
    Box::new(move || {
        inbox()
            .into_iter()
            .filter_map(|n| match n {
                ShellNews::PreparedShared { job, result, live } => {
                    Some((*job, result.map(|p| (*p, live))))
                }
                _ => None,
            })
            .collect()
    })
}

/// A stage's song of the room watched with its audience, read from the
/// room's share on a thread (as a member's, [`prepare_online_with`]).
pub fn prepare_audience(job: LoadJob) {
    if job.share.is_none() {
        return;
    }
    let tx = AUDIENCE_NEWS.with_borrow(Clone::clone);
    // A share's preparation never touches the room it is given.
    prepare_online_with(tx.as_ref(), &current(), job, None);
}

/// The room of the stage played last (the practice room otherwise): its
/// outcome goes there, its results ask it how the stage went.
pub fn stage_room() -> SharedRoom {
    PLAYING.with_borrow(Clone::clone).unwrap_or_else(current)
}

/// The local player's attack policy as stage `order`'s play starts: the
/// one they have in its room now where it is the room's stage (it may
/// have changed on the stage's prompt), else the order's own.
pub fn stage_targeting(order: &ddi_ui::state::room::StageOrder) -> ddi_room::attacks::Targeting {
    let room = stage_room();
    let r = room.borrow();
    if r.me() == order.me && r.stage_started(order.stage) {
        r.my_targeting()
    } else {
        order.targeting
    }
}

/// Stage `order`'s room client for its play, where the room plays it live
/// (`ddi_session::battle::live`): the play starts at the room's instant
/// and streams through it.
pub fn live_handle(
    order: &ddi_ui::state::room::StageOrder,
) -> Option<ddi_session::battle::live::LiveHandle> {
    order.live.as_ref()?;
    let wall_ms = TEST_WALL
        .with_borrow(Clone::clone)
        .unwrap_or_else(|| Rc::new(crate::clock::now_ms));
    // Watched with the room's audience: its seat's client feeds it.
    if let Some(a) = audience_stage(order) {
        return Some(ddi_session::battle::live::LiveHandle { room: a, wall_ms });
    }
    Some(ddi_session::battle::live::LiveHandle {
        room: stage_room(),
        wall_ms,
    })
}

/// A stage's prompt left (its Back, M7b;
/// `ddi_session::battle::leave_prompt`): before the room started it, the
/// player withdraws; once on, a forfeit, an online room's as the player's
/// own report, a practice's CPUs played to their end on `song` (as read)
/// and the stage reported; without a song to play them on, the match
/// stops. A stage watched with the room's audience leaves nothing (it is
/// no member).
pub fn leave_prompt(order: &ddi_ui::state::room::StageOrder, song: Option<&ddi_chart::Song>) {
    use ddi_session::battle::LeftPrompt;
    if WATCHING.with_borrow(|w| w.as_ref().is_some_and(|(s, _)| *s == order.stage)) {
        return;
    }
    match ddi_session::battle::leave_prompt(&mut stage_room().borrow_mut(), order, song) {
        LeftPrompt::Forfeited(Err(why)) => eprintln!("room: the forfeit was refused: {why:?}"),
        LeftPrompt::Lost(Some(e)) => eprintln!("room: stage {}: {e}", order.number),
        _ => {}
    }
}

/// The run's practice room, made at first.
pub fn current() -> SharedRoom {
    let room = ROOM.with(|r| {
        r.borrow_mut()
            .get_or_insert_with(|| ddi_ui::state::room::practice_room(crate::clock::now_ms()))
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
pub fn develop(cpus: u16) {
    ddi_ui::state::room::set_development(ddi_ui::state::room::Development {
        cpus,
        ..ddi_ui::state::room::development()
    });
    discard();
}

/// Leave room: forgotten; the next visit makes a new one.
pub fn discard() {
    if let Some(room) = ROOM.with(|r| r.borrow_mut().take()) {
        forget_playing(&room);
    }
}

/// A room left (practice's, or an online one, `crate::online::discard`):
/// where its stage was the one handed to the play host last, that is
/// forgotten too, so neither the room's client nor its socket's thread
/// lives on.
pub(crate) fn forget_playing(room: &SharedRoom) {
    PLAYING.with_borrow_mut(|p| {
        if p.as_ref().is_some_and(|r| Rc::ptr_eq(r, room)) {
            *p = None;
        }
    });
}

/// The multiplayer home (`crate::online`): CPU practice opens the
/// practice room; Private room signs in (the system browser) and makes an
/// online room; the invite field opens an invite's page. Its songs come
/// from `songs` (the library), else from the sample songs.
pub fn home(env: &Env, songs: Option<Rc<Songs>>, sound: Option<SharedPlayer>) -> OnlineScreen {
    online_page(env, OnlinePage::Home, songs, sound)
}

/// An invite's page (`--join`, or the home's invite field).
pub fn invite(
    env: &Env,
    invite: String,
    songs: Option<Rc<Songs>>,
    sound: Option<SharedPlayer>,
) -> OnlineScreen {
    online_page(env, OnlinePage::Invite(invite), songs, sound)
}

fn online_page(
    env: &Env,
    which: OnlinePage,
    songs: Option<Rc<Songs>>,
    sound: Option<SharedPlayer>,
) -> OnlineScreen {
    let e = env.clone();
    let invite_page = matches!(which, OnlinePage::Invite(_));
    OnlineScreen::new(
        env.clone(),
        crate::online::current(),
        which,
        Rc::new(move |call, cx| match call {
            OnlineCall::Practice => cx.push(room_screen(&e, songs.clone(), sound.clone())),
            OnlineCall::EnterRoom(id) => {
                // A joined invite's page gives its place to the room.
                if invite_page {
                    cx.close(false);
                }
                cx.push(online_room_screen(&e, &id, songs.clone(), sound.clone()));
            }
            OnlineCall::OpenInvite(token) => {
                cx.push(invite(&e, token, songs.clone(), sound.clone()))
            }
            OnlineCall::Watch { link, ticket } => {
                // The invite's page gives its place to the audience's.
                if invite_page {
                    cx.close(false);
                }
                let play = songs.as_ref().map(|s| s.play.clone());
                cx.push(audience_screen(&e, &link, Some(ticket), play));
            }
            // Desktop's sound needs no gesture.
            OnlineCall::WatchPressed => {}
            OnlineCall::Redirect(r) => redirect(&r),
            OnlineCall::Leave => cx.close(true),
        }),
    )
}

/// A room watched with its audience (`docs/plans/multiplayer-audience.md`):
/// its page between stages, its stages' songs read from the room's share.
pub fn audience_screen(
    env: &Env,
    link: &str,
    ticket: Option<String>,
    play: Option<Rc<RefCell<Option<crate::play::Request>>>>,
) -> ddi_ui::screens::audience::AudienceScreen {
    use ddi_ui::screens::audience::{AudienceCall, AudienceScreen};
    let audience = crate::online::audience(link, ticket);
    let a = audience.clone();
    AudienceScreen::new(
        env.clone(),
        audience,
        Rc::new(move |call, cx| match call {
            AudienceCall::Prepare(job) => prepare_audience(job),
            // Watched over the audience's page through the play host: its
            // prompt counting down, the spectator view, then back to the
            // page (no results of its own: the page shows the stage's).
            AudienceCall::Watch(order) => {
                let stage = order.stage;
                if hand_to_play(&play, *order, "audience") {
                    WATCHING.with_borrow_mut(|w| *w = Some((stage, a.clone())));
                }
            }
            // Desktop's sound needs no press.
            AudienceCall::WatchPressed => {}
            AudienceCall::Redirect(r) => redirect(&r),
            AudienceCall::Leave => {
                crate::online::discard_audience();
                cx.close(true);
            }
        }),
    )
}

/// Desktop signs in in the system browser.
fn redirect(r: &Redirect) {
    if let Redirect::Browser(url) = r
        && let Err(e) = crate::os::open_url(url)
    {
        eprintln!("online: opening the browser: {e}");
    }
}

/// Leave room's own: the room forgotten, the screen closed.
type CloseFn = Rc<dyn Fn(bool, &mut Cx)>;

/// The room's shell calls every room screen makes (song select, the
/// stage's prompt): its songs prepared (practice: read from the library;
/// online: the library or the room's share), uploaded (online), and a
/// stage's prompt opened over the room through `play` (the play host's
/// request). `songs`: whether there is a library (the sample songs have
/// no charts to read).
fn room_shell(
    room: SharedRoom,
    library: bool,
    play: Option<Rc<RefCell<Option<crate::play::Request>>>>,
    close: CloseFn,
) -> RoomShell {
    Rc::new(move |call, cx| match call {
        RoomCall::Leave => cx.close(true),
        RoomCall::Discard => close(true, cx),
        RoomCall::Redirect(r) => redirect(&r),
        RoomCall::Prepare(job) if room.borrow().is_online() => prepare_online(&room, job),
        RoomCall::Prepare(job) => prepare_library(&room, &job, library, None),
        RoomCall::Upload(job) => upload(*job),
        // Its prompt over the room (`crate::play::host`): Ready, the
        // play, its results; its outcome reported before them. The sample
        // songs (no library) have no play.
        RoomCall::PlayStage(order) => {
            if hand_to_play(&play, *order, "room") {
                PLAYING.with_borrow_mut(|p| *p = Some(room.clone()));
            }
        }
    })
}

/// Stage `order` handed to the play host through `play` (its prompt over
/// the screens), where there is a library to play it from; whether it
/// was (`what` names the caller in the log).
fn hand_to_play(
    play: &Option<Rc<RefCell<Option<crate::play::Request>>>>,
    order: ddi_ui::state::room::StageOrder,
    what: &str,
) -> bool {
    match play {
        Some(slot) => {
            *slot.borrow_mut() = Some(crate::play::Request::stage(order));
            true
        }
        None => {
            eprintln!("{what}: stage {}: no library to play it from", order.number);
            false
        }
    }
}

/// The shell a stage's prompt runs the room through meanwhile (its songs
/// prepared and shared; it opens no other prompt).
pub fn prompt_shell(room: &SharedRoom) -> RoomShell {
    room_shell(room.clone(), true, None, Rc::new(|_, _| {}))
}

/// An online room (`crate::online::room`): its song select (M7b). Back
/// and Leave room both leave it (asked first where leaving costs
/// something, `ddi_ui::screens::room::ask_leave`): the room is told and
/// forgotten, then the home. Sign in again opens the browser. The host's picks and resting song are uploaded as
/// the room's shares, each member prepares the stage's song from its
/// library or the share, and a stage's prompt opens over the room.
pub fn online_room_screen(
    env: &Env,
    id: &str,
    songs: Option<Rc<Songs>>,
    sound: Option<SharedPlayer>,
) -> SelectScreen {
    let room = crate::online::room(id);
    let play = songs.as_ref().map(|s| s.play.clone());
    let close: CloseFn = Rc::new(|cancel, cx| {
        crate::online::discard();
        cx.close(cancel);
    });
    let shell = room_shell(room.clone(), songs.is_some(), play, close);
    select(env, room, songs.as_ref(), sound, shell)
}

/// The practice room's song select (M7b: the room's main view): Back to
/// the home (the room kept between matches; with a match running,
/// forgotten with its match, never asked), Leave room forgets it first, a
/// stage played through the library's play call.
pub fn room_screen(
    env: &Env,
    songs: Option<Rc<Songs>>,
    sound: Option<SharedPlayer>,
) -> SelectScreen {
    let play = songs.as_ref().map(|s| s.play.clone());
    room_screen_with(env, songs, sound, play)
}

/// [`room_screen`], its stages played through `play` (the play host's
/// request; tests give their own).
pub fn room_screen_with(
    env: &Env,
    songs: Option<Rc<Songs>>,
    sound: Option<SharedPlayer>,
    play: Option<Rc<RefCell<Option<crate::play::Request>>>>,
) -> SelectScreen {
    let close: CloseFn = Rc::new(|cancel, cx| {
        discard();
        cx.close(cancel);
    });
    let shell = room_shell(current(), songs.is_some(), play, close);
    select(env, current(), songs.as_ref(), sound, shell)
}

/// A stage's outcome (`ddi_session::battle`), reported to the room that
/// handed it over (`ddi_session::battle::report_outcome`: the policy the
/// play left the player with kept for the next stage wherever the stage
/// had attacks, online too). How the match ended for the player, where
/// that was its last stage (an online room's is not known yet: `None`).
pub fn report(outcome: ddi_session::battle::StageOutcome) -> Option<ddi_ui::state::room::MatchEnd> {
    // Watched with the audience: nothing is reported (it is no member).
    if WATCHING.with_borrow(|w| w.as_ref().is_some_and(|(s, _)| *s == outcome.stage)) {
        return None;
    }
    let reported = ddi_session::battle::report_outcome(&mut stage_room().borrow_mut(), outcome);
    if let Err(why) = reported.taken {
        eprintln!("room: the stage's outcome was refused: {why:?}");
    }
    reported.ending
}

/// Prepares a stage's song from the library (practice's, and an online
/// room's without a share): its chart read (as a play reads it), the
/// CPUs' plan checked and what live play needs where the room plays live,
/// then `check` (a bot's); the room told it loaded or why not. Without a
/// library (`library`: the sample songs) a song not found has no chart:
/// prepared at once.
fn prepare_library(room: &SharedRoom, job: &LoadJob, library: bool, check: Option<&SongCheck>) {
    let result = match crate::play::load::read_chart(&job.song) {
        Ok(song) => prepare_stage(job, &song, &mut |e| {
            eprintln!("room: stage song {}: {e}", job.song)
        })
        .and_then(|prepared| {
            if let Some(check) = check {
                check(job, song, prepared.index)?;
            }
            Ok(prepared.live)
        }),
        // A sample song (no library): nothing to read.
        Err(ddi_ui::screens::failures::LoadFailure::NoSong) if !library => Ok(None),
        Err(e) => {
            eprintln!("room: stage song {} unreadable: {e:?}", job.song);
            Err(LoadProblem::Unreadable)
        }
    };
    room.borrow_mut().finish_load_live(job, result);
}

/// The room's song select over the library (`songs`; the sample songs
/// without one): previews as song select's, Back to the home; it moves no
/// singleplayer place.
fn select(
    env: &Env,
    room: SharedRoom,
    songs: Option<&Rc<Songs>>,
    sound: Option<SharedPlayer>,
    shell: RoomShell,
) -> SelectScreen {
    let Some(songs) = songs else {
        return ddi_ui::screens::sample::room_screen(env, room.clone()).with_room(room, shell);
    };
    // The room's song select has a place of its own, for the run
    // (`ddi_ui::state::place::ROOM_KEY`): a new room starts where the last
    // one was, the first where singleplayer's is.
    let place = Rc::new(RefCell::new(MenuPlace::load_room()));
    let data = songs.data_at(&place.borrow());
    // Options and Calibrate (the footer's links) over the room, as from
    // singleplayer's song select: back on it where it was.
    let open = crate::songs::open_fn(songs, sound.clone());
    let songs = songs.clone();
    let shown = env.clone();
    let calls: ddi_ui::screens::select::ShellFn = Rc::new(move |call, cx| match call {
        SelectCall::Open(what) if what == "calibrate" => {
            cx.push(crate::calibrate::menu(&shown, &songs, sound.clone()));
        }
        SelectCall::Leave => {
            if let Some(sound) = &sound {
                sound.borrow_mut().pause();
            }
            cx.close(true);
        }
        call @ (SelectCall::Selected { .. } | SelectCall::Volume(_) | SelectCall::Shown(_)) => {
            songs.call(call, sound.as_ref(), cx)
        }
        // Moving no singleplayer place or style.
        SelectCall::Place(p) => {
            ddi_ui::state::place::apply_room(&p, &mut place.borrow_mut());
            songs.warm_shown(&p, &Settings::cached());
        }
        _ => {}
    });
    // The pick is the room's: the room's place keeps it selected.
    SelectScreen::new(
        env.clone(),
        data,
        None,
        Rc::new(|song, chart, _, _| ddi_ui::state::place::room_picked(&song.id, chart.index)),
        open,
    )
    .with_shell(calls, Default::default())
    .with_room(room, shell)
}
