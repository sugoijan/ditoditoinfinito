//! What a finished play's results say as the window shows them
//! (`play::host`), on this machine's GPU offscreen and on a fake clock
//! with no sound device: a play nobody presses in, failed, whose keeping
//! failed too; an autoplay quit. Without a GPU adapter (CI) the plays
//! run undrawn (`common::rig::Draws`) and all of it is checked all the
//! same.

mod common;
use std::rc::Rc;
use std::sync::atomic::Ordering;

use ddi_desktop::play::Request;
use ddi_desktop::play::host::Host;
use ddi_platform::history::MemoryPlayStore;
use ddi_platform::{DeviceId, HostTime, RawInput};
use ddi_ui::state::history::Plays;
use common::ui::{ResultsShown, key, results_shown, top};

const SIZE: [u32; 2] = [960, 540];

#[test]
fn a_failed_play_unkept_and_a_quit_say_so_on_their_results() {
    common::indexes();
    let (mut ui, env) = common::ui::theme("select", common::ui::lowercase(), SIZE, false);
    ui.update(0.5);
    let store = Rc::new(MemoryPlayStore::default());
    store.failing.set(true);
    let plays = Plays::new(store.clone());
    let mut host = Host::new(Rc::default(), Some(plays.clone()));
    let clock = common::rig::fake_clock(&mut host);
    let draws = common::rig::Draws::here(&mut host, SIZE);
    let run = |host: &mut Host, ui: &mut ddi_ui::Ui, frames: usize| {
        let mut n = 0;
        while host.playing() && n < frames {
            let now = clock.fetch_add(1_000_000_000 / 60, Ordering::SeqCst) + 1_000_000_000 / 60;
            draws.frame(host, ui, &env, now);
            n += 1;
            assert!(n < 60 * 300, "the play never ended");
        }
        draws.wait_gpu();
        ui.update(0.05);
    };
    let start = |host: &mut Host, ui: &mut ddi_ui::Ui, request: Request| {
        *host.requested.borrow_mut() = Some(request);
        draws.wait_prompt(host, ui, &env);
        ui.update(0.5);
        key(ui, "Enter", "Enter");
        ui.update(0.05);
        draws.poll(host, ui, &env);
        assert!(host.playing());
    };
    let leave = |host: &mut Host, ui: &mut ddi_ui::Ui| {
        ui.update(3.0);
        key(ui, "Escape", "Escape");
        ui.update(0.5);
        draws.poll(host, ui, &env);
        ui.update(1.0);
        assert!(host.idle());
    };

    // Nobody presses: failed, and the store refuses it.
    start(&mut host, &mut ui, Request::song("beatucada", 2, false));
    run(&mut host, &mut ui, usize::MAX);
    assert_eq!(top(&ui), "results");
    assert_eq!(store.len(), 0);
    let shown = results_shown(&ui);
    assert!(host.log.last().unwrap().ends_with("failed true"), "{:?}", host.log);
    assert_eq!(
        shown,
        ResultsShown {
            notes: "This play could not be saved: the store is full.".into(),
            retry: true,
            offset: true,
        }
    );
    leave(&mut host, &mut ui);

    // Autoplayed, then quit (Escape twice).
    start(&mut host, &mut ui, Request::song("beatucada", 2, true));
    run(&mut host, &mut ui, 120);
    let at = clock.load(Ordering::SeqCst) as f64 / 1e9;
    for (dt, pressed) in [(0.0, true), (0.05, false), (0.1, true)] {
        host.feed.borrow_mut().edges.push(RawInput {
            device: DeviceId::Keyboard,
            control: "Escape".into(),
            pressed,
            host_time: HostTime(at + dt),
            slot: 0,
        });
    }
    clock.fetch_add(200_000_000, Ordering::SeqCst);
    run(&mut host, &mut ui, 30);
    assert_eq!(top(&ui), "results");
    let shown = results_shown(&ui);
    assert!(host.log.last().unwrap().starts_with("quit"), "{:?}", host.log);
    assert_eq!(
        shown,
        ResultsShown {
            notes: "Autoplay: this play does not count toward your bests. \
                    · This play could not be saved: the store is full."
                .into(),
            retry: true,
            offset: true,
        }
    );
}

/// Round FX: a song this device has only from an online room's store
/// (`rooms/<store>/<share>/<n>/song/` in the cache folder, as
/// `share::load_with` leaves it and registers it, `shared:<share>:<n>`)
/// shows its banner on its results, read from there (the web's took such
/// a song for a bundled one and showed none). Played alone here: a
/// stage's results build the song's banner the same way.
#[test]
fn a_room_song_downloaded_ahead_shows_its_banner_on_its_results() {
    common::indexes();
    let (mut ui, env) = common::ui::theme("select", common::ui::lowercase(), SIZE, false);
    ui.update(0.5);
    // The song's files as the room's store keeps them.
    let share = "fxbanner1";
    let root = std::env::temp_dir()
        .join(format!("ddi-room-store-{}", std::process::id()))
        .join("rooms/Room000001")
        .join(share)
        .join("0");
    let _ = std::fs::remove_dir_all(&root);
    let song_dir = root.join("song");
    std::fs::create_dir_all(&song_dir).unwrap();
    let bundled = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs/beatucada");
    for f in ["Beatucada.ssc", "Beatucada.opus", "Banner.png", "Background.png"] {
        std::fs::copy(bundled.join(f), song_dir.join(f)).unwrap();
    }
    let (mut songs, report) = ddi_desktop::library::scan_folders(std::slice::from_ref(&song_dir));
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    let mut song = songs.pop().unwrap();
    let id = ddi_library::share::song_id(share, 0);
    song.entry.id = id.clone();
    song.root = root.clone();
    song.dir = ddi_library::path::RelPathBuf::from("song");
    // The share as read (its list), every file of it on this device.
    let files = ["Beatucada.ssc", "Beatucada.opus", "Banner.png", "Background.png"]
        .iter()
        .map(|f| ddi_library::share::ShareFileDoc {
            path: (*f).into(),
            bytes: std::fs::metadata(song_dir.join(f)).unwrap().len() as f64,
            url: String::new(),
        })
        .collect();
    let doc = ddi_library::share::ShareDoc {
        pack: String::new(),
        expires_at: None,
        created_at: 0.0,
        songs: vec![ddi_library::share::ShareSongDoc {
            record: serde_json::json!({ "entry": song.entry, "pack": "" }),
            files,
        }],
        pack_art: serde_json::Value::Null,
        id: share.into(),
    };
    ddi_desktop::share::keep_loaded_for_tests(doc, root.parent().unwrap().to_path_buf());
    ddi_desktop::library::register_room_songs(&[song]);

    let store = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store.clone());
    let mut host = Host::new(Rc::default(), Some(plays.clone()));
    let clock = common::rig::fake_clock(&mut host);
    let draws = common::rig::Draws::here(&mut host, SIZE);
    *host.requested.borrow_mut() = Some(Request::song(&id, 2, true));
    draws.wait_prompt(&mut host, &mut ui, &env);
    ui.update(0.5);
    key(&mut ui, "Enter", "Enter");
    ui.update(0.05);
    draws.poll(&mut host, &mut ui, &env);
    assert!(host.playing(), "{:?}", host.log);
    let mut frames = 0;
    let mut run = |host: &mut Host, ui: &mut ddi_ui::Ui, n: usize| {
        for _ in 0..n {
            if !host.playing() {
                break;
            }
            let now = clock.fetch_add(1_000_000_000 / 60, Ordering::SeqCst) + 1_000_000_000 / 60;
            draws.frame(host, ui, &env, now);
            frames += 1;
        }
        draws.wait_gpu();
        ui.update(0.05);
    };
    run(&mut host, &mut ui, 120);
    let at = clock.load(Ordering::SeqCst) as f64 / 1e9;
    for (dt, pressed) in [(0.0, true), (0.05, false), (0.1, true)] {
        host.feed.borrow_mut().edges.push(RawInput {
            device: DeviceId::Keyboard,
            control: "Escape".into(),
            pressed,
            host_time: HostTime(at + dt),
            slot: 0,
        });
    }
    clock.fetch_add(200_000_000, Ordering::SeqCst);
    run(&mut host, &mut ui, 30);
    assert_eq!(top(&ui), "results");
    let banner = match ui.actor("Banner").map(|a| &a.kind) {
        Some(ddi_actor::Kind::Sprite(s)) => ui.texture_source(s.texture),
        _ => None,
    };
    let source = banner.expect("the results show a banner").source;
    assert_eq!(
        std::path::Path::new(&*source),
        song_dir.join("Banner.png"),
        "the banner read from the room's store"
    );
    let _ = std::fs::remove_dir_all(root.parent().unwrap().parent().unwrap().parent().unwrap());
}
