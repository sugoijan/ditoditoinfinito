//! A new song from a music file on desktop, as the window does it but
//! headless (`docs/plans/editor.md`, "New songs"): song select's Add
//! songs → "New song" with the picked file injected (cancelled first:
//! nothing changes), the song's folder made in the song folder's
//! `Dito Edits`, named from the file's tags, the editor over song select
//! on it; notes placed by keys, a playtest of it autoplayed on a fake
//! clock (never kept), saved, listed by song select, opened again equal.
//! A second one left unsaved is taken away again. `--edit`'s request
//! waits for the library.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_chart::Tick;
use ddi_platform::HostTime;
use ddi_platform::history::MemoryPlayStore;
use ddi_ui::screens::DESIGN;
use ddi_ui::state::history::Plays;
use ddi_ui::state::settings::Settings;
use ddi_ui::testing::Harness;
use ddi_ui::{Modifiers, PointerKind, TextMode, Ui};

mod common;

fn key(h: &mut Harness, code: &str, k: &str) {
    h.key(code, k);
    h.run(0.05);
}

fn tap(h: &mut Harness, name: &str) {
    assert!(
        h.ui.bounds(name).is_some(),
        "no {name}: {:?}",
        h.ui.layers()
    );
    h.tap(PointerKind::Mouse, name);
    h.run(0.4);
}

fn top(ui: &Ui) -> String {
    ui.layers()
        .last()
        .map(|l| l.name.clone())
        .unwrap_or_default()
}

/// A mono 16-bit WAV of a tone, `seconds` long, titled `title` in its
/// `LIST`/`INFO`.
fn wav(seconds: f64, title: &str) -> Vec<u8> {
    let rate = 44_100u32;
    let n = (seconds * rate as f64) as usize;
    let data: Vec<u8> = (0..n)
        .flat_map(|i| {
            let t = i as f64 / rate as f64;
            (((t * 440.0 * std::f64::consts::TAU).sin() * 8000.0) as i16).to_le_bytes()
        })
        .collect();
    let mut name = title.as_bytes().to_vec();
    name.push(0);
    if name.len() % 2 == 1 {
        name.push(0);
    }
    let mut info = b"INFO".to_vec();
    info.extend(b"INAM");
    info.extend((name.len() as u32).to_le_bytes());
    info.extend(&name);
    let mut body = b"WAVE".to_vec();
    body.extend(b"fmt ");
    body.extend(16u32.to_le_bytes());
    body.extend(1u16.to_le_bytes());
    body.extend(1u16.to_le_bytes());
    body.extend(rate.to_le_bytes());
    body.extend((rate * 2).to_le_bytes());
    body.extend(2u16.to_le_bytes());
    body.extend(16u16.to_le_bytes());
    body.extend(b"LIST");
    body.extend((info.len() as u32).to_le_bytes());
    body.extend(&info);
    body.extend(b"data");
    body.extend((data.len() as u32).to_le_bytes());
    body.extend(&data);
    let mut out = b"RIFF".to_vec();
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    out
}

fn scratch() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/scratch/editor-new-song");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("songs")).unwrap();
    std::fs::create_dir_all(dir.join("music")).unwrap();
    dir
}

#[test]
fn a_new_song_made_charted_played_saved_and_opened_again() {
    common::indexes();
    let dir = scratch();
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("DDI_HOME", dir.join("home")) };
    ddi_desktop::audio::set_headless(true);
    let music = dir.join("music/tone_01.wav");
    std::fs::write(&music, wav(10.0, "Test Tone")).unwrap();
    let folder = dir.join("songs");
    let songs = ddi_desktop::songs::Songs::new(vec![folder.clone()]);
    songs.skip_offer();
    songs.start(Arc::new(|| {}));
    let wait = |songs: &ddi_desktop::songs::Songs| {
        if let Some(l) = songs.library.borrow_mut().as_mut() {
            l.wait();
        }
    };
    wait(&songs);
    let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_lowercase());
    let sound = Rc::new(RefCell::new(ddi_desktop::preview::Player::with(
        ddi_desktop::audio::Audio::silent(0.0),
        ddi_desktop::preview::PreviewSong::bundled,
    )));
    let (ui, env, _) = ddi_desktop::shell::library_ui("select", fold, sound, &songs);
    env.reduced_motion.set(true);
    let mut h = Harness::with_gestures(DESIGN, DESIGN, 1.0, TextMode::Own, false);
    h.ui = ui;
    h.run(0.5);
    assert_eq!(top(&h.ui), "select");
    let pack = folder.join("Dito Edits");

    // Cancelled: nothing changes.
    ddi_desktop::os::inject_picked_music(None);
    tap(&mut h, "Link.Add");
    tap(&mut h, "NewSong");
    assert_eq!(top(&h.ui), "select", "{:?}", h.ui.layers());
    assert!(!pack.exists());

    // Picked: the folder, the editor on the song, unsaved.
    ddi_desktop::os::inject_picked_music(Some(music.clone()));
    tap(&mut h, "Link.Add");
    tap(&mut h, "NewSong");
    assert_eq!(top(&h.ui), "editor", "{:?}", h.ui.layers());
    let song_dir = pack.join("Test Tone");
    assert!(song_dir.join("tone_01.wav").is_file());
    assert!(song_dir.join("Test Tone.ssc").is_file());
    let id = ddi_library::pack::song_id("Dito Edits", "Test Tone");
    assert!(ddi_desktop::library::song(&id).is_some(), "in the library");
    let view = env.state.borrow().editor.clone().expect("the editor");
    assert_eq!((view.song.as_str(), view.notes), ("Test Tone", 0));
    assert!(view.dirty, "a new song is not saved");

    // Listened to: the music, decoded on its thread since the editor
    // opened, plays on the host clock and the field follows it.
    key(&mut h, "Space", " ");
    let view = || env.state.borrow().editor.clone().unwrap();
    assert!(view().listening);
    let started = std::time::Instant::now();
    while view().shown < 96.0 {
        assert!(
            started.elapsed().as_secs() < 30,
            "never heard: {:?}",
            view()
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
        h.run(0.02);
    }
    key(&mut h, "Space", " ");
    assert!(!view().listening);
    key(&mut h, "Home", "Home");
    assert_eq!(view().cursor, 0);

    // Charted: the snap to 4ths (from 16ths), a note a beat from beat 4
    // to beat 11.
    for _ in 0..3 {
        key(&mut h, "ArrowLeft", "ArrowLeft");
    }
    for _ in 0..4 {
        key(&mut h, "ArrowDown", "ArrowDown");
    }
    for beat in 4..12 {
        let lane = (beat % 4) as u8;
        let code = format!("Digit{}", lane + 1);
        key(&mut h, &code, &(lane + 1).to_string());
        key(&mut h, "ArrowDown", "ArrowDown");
    }
    assert_eq!(env.state.borrow().editor.as_ref().unwrap().notes, 8);

    // Played: Shift P, autoplayed from 2 s before the cursor's beat 12.
    for _ in 0..4 {
        key(&mut h, "ArrowUp", "ArrowUp");
    }
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    h.key_with("KeyP", "P", shift);
    h.run(0.05);
    let request = songs.play.borrow_mut().take().expect("a playtest");
    assert_eq!(request.song, id);
    let edited = request.edited.clone().expect("the song as edited");
    assert_eq!(request.start, Some(2.0), "beat 8 at 120 BPM, less 2 s");
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let loaded =
        ddi_desktop::play::load::load_with(&id, Some(0), 48_000, false, Some(&edited), &cancel)
            .expect("loaded");
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    let audio =
        ddi_desktop::audio::Audio::headless(1.0, Some(Arc::new(move || c.load(Ordering::SeqCst))));
    let store = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store.clone());
    let mut play = ddi_desktop::play::Play::start(
        request,
        Rc::new(loaded),
        &Settings::default(),
        audio,
        Rc::new(RefCell::new(ddi_desktop::play::Feed::default())),
        None,
        60.0,
    )
    .expect("the playtest");
    loop {
        let now = clock.fetch_add(16_666_667, Ordering::SeqCst) + 16_666_667;
        match play.tick(HostTime(now as f64 / 1e9), Some(&plays)) {
            Some(ddi_desktop::play::Ended::Finished { results, .. }) => {
                assert!(!results.failed);
                break;
            }
            Some(_) => panic!("the playtest did not finish"),
            None => {}
        }
        assert!(now < 100_000_000_000, "never ends");
    }
    // From beat 4 (2 s before beat 8): all eight notes, top tier.
    assert_eq!(play.session.player.tally().taps[0], 8);
    assert_eq!(store.len(), 0, "a playtest is never kept");

    // Saved, without a question or an `.old`; listed by song select.
    tap(&mut h, "Button.Save");
    assert_eq!(top(&h.ui), "editor", "no question: {:?}", h.ui.layers());
    assert!(!env.state.borrow().editor.as_ref().unwrap().dirty);
    assert!(!song_dir.join("Test Tone.ssc.old").exists());
    wait(&songs);
    songs.poll(&mut h.ui);
    let listed = songs.listed();
    let entry = listed
        .iter()
        .find(|l| l.entry.id == id)
        .expect("in song select");
    assert_eq!(entry.pack.as_deref(), Some("Dito Edits"));
    assert_eq!(entry.entry.charts[0].notes, 8);

    // Left (nothing unsaved: no question), opened again equal.
    key(&mut h, "Escape", "Escape");
    h.run(0.5);
    assert_eq!(top(&h.ui), "select", "{:?}", h.ui.layers());
    assert!(song_dir.is_dir(), "kept");
    let text = std::fs::read_to_string(song_dir.join("Test Tone.ssc")).unwrap();
    let saved = ddi_chart::formats::sm::parse_ssc(&text).unwrap();
    let again = ddi_desktop::editor::read(&id, 0).unwrap();
    assert_eq!(again.song().charts, saved.charts);
    assert_eq!(again.song().timing, saved.timing);
    assert_eq!(again.song().title, "Test Tone");
    assert_eq!(again.song().music.as_deref(), Some("tone_01.wav"));
    assert_eq!(again.song().charts[0].notes, edited.charts[0].notes);
    let ticks: Vec<(u8, Tick)> = again.song().charts[0]
        .notes
        .iter()
        .map(|n| (n.lane, n.tick))
        .collect();
    assert_eq!(
        ticks,
        (4..12)
            .map(|b| ((b % 4) as u8, Tick::from_beats(b)))
            .collect::<Vec<_>>()
    );

    // A second one, left unsaved: asked, then taken away.
    ddi_desktop::os::inject_picked_music(Some(music));
    tap(&mut h, "Link.Add");
    tap(&mut h, "NewSong");
    assert_eq!(top(&h.ui), "editor");
    let second = pack.join("Test Tone 2");
    assert!(second.is_dir());
    let id2 = ddi_library::pack::song_id("Dito Edits", "Test Tone 2");
    key(&mut h, "Escape", "Escape");
    h.run(0.4);
    tap(&mut h, "Confirm");
    h.run(0.5);
    assert_eq!(top(&h.ui), "select", "{:?}", h.ui.layers());
    assert!(!second.exists(), "its folder removed");
    assert!(ddi_desktop::library::song(&id2).is_none());
    wait(&songs);
    songs.poll(&mut h.ui);
    assert!(!songs.listed().iter().any(|l| l.entry.id == id2));
    assert!(songs.listed().iter().any(|l| l.entry.id == id));

    // `--edit SONG`: once the library has the song (or has read every
    // folder without it, for the editor to say why).
    *songs.edit.borrow_mut() = Some((id.clone(), 0));
    assert_eq!(songs.edit_ready(), Some((id, 0)));
    assert_eq!(songs.edit_ready(), None);
    *songs.edit.borrow_mut() = Some(("u-none".into(), 0));
    assert_eq!(songs.edit_ready(), Some(("u-none".into(), 0)));
}
