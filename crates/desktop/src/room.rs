//! The multiplayer prototype on desktop
//! (`docs/plans/multiplayer-prototype-progress.md`, M2), behind its hidden
//! switch (`--multiplayer on`, `Settings::multiplayer`): the multiplayer
//! home, the practice room over it and song select choosing a stage's
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
use std::sync::mpsc::{Sender, channel};

use ddi_library::manifest::ManifestEntry;
use ddi_library::share::ShareError;
use ddi_room::StageId;
use ddi_ui::Cx;
use ddi_ui::screens::Env;
use ddi_ui::screens::online::{OnlineCall, OnlinePage, OnlineScreen};
use ddi_ui::screens::room::{RoomCall, RoomScreen};
use ddi_ui::screens::room_dialogs::NoticeDialog;
use ddi_ui::screens::select::{SelectCall, SelectScreen};
use ddi_ui::state::online::Redirect;
use ddi_ui::state::room::{
    Inbox, LoadJob, LoadProblem, RoomClient, SharedRoom, ShellNews, UploadJob, picked_of,
    prepare_chart,
};

use crate::preview::SharedPlayer;
use crate::songs::Songs;

thread_local! {
    static ROOM: RefCell<Option<SharedRoom>> = const { RefCell::new(None) };
    /// The room of the stage handed to the play host last.
    static PLAYING: RefCell<Option<SharedRoom>> = const { RefCell::new(None) };
    /// Where the online room's threads send their news ([`inbox`]).
    static NEWS: RefCell<Option<Sender<News>>> = const { RefCell::new(None) };
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
}

/// The online room client's inbox: what this module's threads report.
pub fn inbox() -> Inbox {
    let (tx, rx) = channel::<News>();
    NEWS.with_borrow_mut(|n| *n = Some(tx));
    Box::new(move || {
        rx.try_iter()
            .map(|n| match n {
                News::Progress { stage, done, total } => ShellNews::Progress { stage, done, total },
                News::Uploaded { stage, result } => ShellNews::Uploaded { stage, result },
                News::PreparedShared { job, result } => ShellNews::PreparedShared {
                    result: result.and_then(|found| {
                        picked_of(&found.entry, found.index, &found.pack, found.banner)
                            .map(Box::new)
                            .ok_or(LoadProblem::Changed)
                    }),
                    job,
                },
            })
            .collect()
    })
}

fn news() -> Option<Sender<News>> {
    NEWS.with_borrow(Clone::clone)
}

/// An online room's stage song uploaded as the room's share under its
/// grant, on a thread (`crate::share::upload_room`), its news to the
/// online room's [`inbox`].
pub fn upload(job: UploadJob) {
    let Some(tx) = news() else {
        return;
    };
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

/// An online room's stage song prepared on this device: the host's from
/// the library at once; a member's from the room's share, read into the
/// cache on a thread and found by the digest chosen (its news to the
/// online room's [`inbox`]).
pub fn prepare_online(room: &SharedRoom, job: LoadJob) {
    let Some(share) = job.share.clone() else {
        let result = crate::play::load::read_chart(&job.song)
            .map_err(|e| {
                eprintln!("room: stage song {} unreadable: {e:?}", job.song);
                LoadProblem::Unreadable
            })
            .and_then(|song| {
                prepare_chart(&job, &song, &mut |e| {
                    eprintln!("room: stage song {}: {e}", job.song)
                })
            });
        room.borrow_mut().finish_load(&job, result.map(|_| ()));
        return;
    };
    let Some(tx) = news() else {
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
                let index = prepare_chart(&job, &song, &mut |e| eprintln!("room: {e}"))?;
                let first = loaded.songs.first().ok_or(LoadProblem::Download)?;
                Ok(Box::new(SharedSong {
                    entry: first.entry.clone(),
                    index,
                    pack: first.pack.clone(),
                    banner: first.entry.banner.as_deref().map(|b| first.source(b)),
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

/// The room of the stage played last (the practice room otherwise): its
/// outcome goes there, its results ask it how the stage went.
pub fn stage_room() -> SharedRoom {
    PLAYING.with_borrow(Clone::clone).unwrap_or_else(current)
}

/// A stage left from its prompt in an online room: the player's forfeit
/// reported as its own run. `false` for practice (its CPUs are played to
/// their end by the play host).
pub fn left_online_stage(stage: StageId) -> bool {
    let room = stage_room();
    if !room.borrow().is_online() {
        return false;
    }
    if let Err(why) = room.borrow_mut().report_run(stage, ddi_room::Run::Forfeit) {
        eprintln!("room: the forfeit was refused: {why:?}");
    }
    true
}

/// The run's practice room, made at first.
pub fn current() -> SharedRoom {
    ROOM.with(|r| {
        r.borrow_mut()
            .get_or_insert_with(|| {
                let now = crate::clock::epoch_ms().max(0.0) as u64;
                RoomClient::shared(now, now)
            })
            .clone()
    })
}

/// Leave room: forgotten; the next visit makes a new one.
pub fn discard() {
    ROOM.with(|r| r.borrow_mut().take());
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
            OnlineCall::Redirect(r) => redirect(&r),
            OnlineCall::Leave => cx.close(true),
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

/// An online room (`crate::online::room`): Back to the home, Leave room
/// tells the room and forgets it, Choose song opens song select over it
/// (its pick for this room), Sign in again opens the browser. The host's
/// pick is uploaded as the room's share, each member prepares the stage's
/// song from its library or the share, and a stage is played over the
/// room as practice's are, its run reported to the room.
pub fn online_room_screen(
    env: &Env,
    id: &str,
    songs: Option<Rc<Songs>>,
    sound: Option<SharedPlayer>,
) -> RoomScreen {
    let e = env.clone();
    let room = crate::online::room(id);
    let shared = room.clone();
    let play = songs.as_ref().map(|s| s.play.clone());
    RoomScreen::new(
        env.clone(),
        room,
        Rc::new(move |call, cx| match call {
            RoomCall::Leave => cx.close(true),
            RoomCall::Discard => {
                crate::online::discard();
                cx.close(true);
            }
            RoomCall::ChooseSong { stage, number } => cx.push(pick(
                &e,
                shared.clone(),
                songs.as_ref(),
                sound.clone(),
                stage,
                number,
            )),
            RoomCall::Redirect(r) => redirect(&r),
            RoomCall::Prepare(job) => prepare_online(&shared, job),
            RoomCall::Upload(job) => upload(*job),
            RoomCall::PlayStage(order) => match &play {
                Some(slot) => {
                    PLAYING.with_borrow_mut(|p| *p = Some(shared.clone()));
                    *slot.borrow_mut() = Some(crate::play::Request::stage(*order));
                }
                None => eprintln!("room: stage {}: no library to play it from", order.number),
            },
        }),
    )
}

/// The practice room: Back to the home, Leave room forgets it first,
/// Choose song opens song select over it, a stage is played through the
/// library's play call.
pub fn room_screen(env: &Env, songs: Option<Rc<Songs>>, sound: Option<SharedPlayer>) -> RoomScreen {
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
) -> RoomScreen {
    let e = env.clone();
    RoomScreen::new(
        env.clone(),
        current(),
        Rc::new(move |call, cx| match call {
            RoomCall::Leave => cx.close(true),
            RoomCall::Discard => {
                discard();
                cx.close(true);
            }
            RoomCall::ChooseSong { stage, number } => {
                cx.push(pick(
                    &e,
                    current(),
                    songs.as_ref(),
                    sound.clone(),
                    stage,
                    number,
                ));
            }
            RoomCall::Prepare(job) => prepare(&job, songs.is_some()),
            // Played over the room (`crate::play::host`): its prompt, the
            // play, its results; its outcome reported before them. The
            // sample songs (no library) have no play.
            RoomCall::PlayStage(order) => match &play {
                Some(slot) => {
                    PLAYING.with_borrow_mut(|p| *p = Some(current()));
                    *slot.borrow_mut() = Some(crate::play::Request::stage(*order));
                }
                None => eprintln!("room: stage {}: no library to play it from", order.number),
            },
            // Practice never signs in, and shares nothing.
            RoomCall::Redirect(_) | RoomCall::Upload(_) => {}
        }),
    )
}

/// A stage's outcome (`ddi_session::battle`), reported to its room: in
/// practice as its authority (its ranking confirmed, the song ended); in
/// an online room the player's own run, which the room ranks with the
/// others'. How the match ended for the player, where that was its last
/// stage (an online room's is not known yet: `None`).
pub fn report(outcome: ddi_session::battle::StageOutcome) -> Option<ddi_ui::state::room::MatchEnd> {
    let room = stage_room();
    let online = room.borrow().is_online();
    let reported = if online {
        room.borrow_mut().report_run(outcome.stage, outcome.mine)
    } else {
        room.borrow_mut()
            .report_stage(outcome.stage, outcome.ranking, outcome.confirmed_early)
    };
    if let Err(why) = reported {
        eprintln!("room: the stage's outcome was refused: {why:?}");
    }
    room.borrow().match_end()
}

/// Prepares a stage's song: its chart read from the library (as a play
/// reads it) and the CPUs' plan checked; the room told it loaded or why
/// not. The sample songs (no library) have no chart: prepared at once.
fn prepare(job: &ddi_ui::state::room::LoadJob, library: bool) {
    let result = match crate::play::load::read_chart(&job.song) {
        Ok(song) => prepare_chart(job, &song, &mut |e| {
            eprintln!("room: stage song {}: {e}", job.song)
        })
        .map(|_| ()),
        // A sample song (no library): nothing to read.
        Err(ddi_ui::screens::failures::LoadFailure::NoSong) if !library => Ok(()),
        Err(e) => {
            eprintln!("room: stage song {} unreadable: {e:?}", job.song);
            Err(LoadProblem::Unreadable)
        }
    };
    current().borrow_mut().finish_load(job, result);
}

/// Song select over the library choosing stage `stage`'s song: a chart
/// the CPUs cannot play is refused in a pop-up; one they can goes to the
/// room, and song select closes. Its previews play as song select's do;
/// it moves no singleplayer place.
fn pick(
    env: &Env,
    room: SharedRoom,
    songs: Option<&Rc<Songs>>,
    sound: Option<SharedPlayer>,
    stage: StageId,
    number: u16,
) -> SelectScreen {
    let picked = room.clone();
    let play = Rc::new(
        move |song: &ddi_ui::screens::data::SongEntry,
              chart: &ddi_ui::screens::data::ChartEntry,
              env: &Env,
              cx: &mut Cx| {
            let chosen = picked.borrow_mut().choose(stage, song, chart);
            match chosen {
                Ok(()) => cx.close(false),
                Err(why) => cx.overlay(NoticeDialog::refusal(env.clone(), &why), true),
            }
        },
    );
    let Some(songs) = songs else {
        return ddi_ui::screens::sample::pick_screen(
            env,
            room,
            stage,
            number,
            ddi_ui::screens::sample::select(),
        );
    };
    let mut data = songs.data();
    data.pick = Some(ddi_ui::screens::data::PickView { stage: number });
    let songs = songs.clone();
    let calls: ddi_ui::screens::select::ShellFn = Rc::new(move |call, cx| match call {
        SelectCall::Leave => {
            if let Some(sound) = &sound {
                sound.borrow_mut().pause();
            }
            cx.close(true);
        }
        call @ (SelectCall::Selected { .. } | SelectCall::Volume(_) | SelectCall::Shown(_)) => {
            songs.call(call, sound.as_ref(), cx)
        }
        _ => {}
    });
    SelectScreen::new(env.clone(), data, None, play, Rc::new(|_, _| {}))
        .with_shell(calls, Default::default())
}
