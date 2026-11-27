//! Find songs with "This computer" on (`docs/plans/source-plugins.md`,
//! stage 2), as the window does it but headless: song select's More →
//! Find songs, a folder of generated songs (in the build's scratch
//! folder, nothing of anyone's) searched, a song played from it to its
//! results with autoplay on a fake clock without importing it (kept by
//! its found song's id, with where it came from; its item held from the
//! prompt until its results close), then imported (in the library after)
//! and played again from the page (kept as the library's song; both plays
//! its chart's). The page's pictures where
//! `DDI_OFFSCREEN_PPM` names a folder and a GPU is there.

mod common;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use ddi_desktop::play::host::Host;
use ddi_platform::history::MemoryPlayStore;
use ddi_ui::screens::DESIGN;
use ddi_ui::state::history::Plays;
use ddi_ui::testing::Harness;
use ddi_ui::{PointerKind, TextMode, Ui};

const SIZE: [u32; 2] = [1280, 720];

fn top(ui: &Ui) -> String {
    ui.layers()
        .last()
        .map(|l| l.name.clone())
        .unwrap_or_default()
}

fn simfile(title: &str, meter: u32) -> String {
    format!(
        "#VERSION:0.83;\n#TITLE:{title};\n#ARTIST:Generated;\n#MUSIC:song.wav;\n#OFFSET:0;\n#BPMS:0=120;\n#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Hard;\n#METER:{meter};\n#NOTES:\n1000\n0100\n0010\n0001\n;\n"
    )
}

/// Two seconds of silence: 8 kHz, mono, 16-bit.
fn silence() -> Vec<u8> {
    let data = 32_000u32;
    let mut w = Vec::new();
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&8000u32.to_le_bytes());
    w.extend_from_slice(&16000u32.to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data.to_le_bytes());
    w.resize(44 + data as usize, 0);
    w
}

fn song_at(dir: &Path, title: &str, meter: u32) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("song.ssc"), simfile(title, meter)).unwrap();
    std::fs::write(dir.join("song.wav"), silence()).unwrap();
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/scratch")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// The page's item `key` (`button:play:0`) tapped.
fn press(h: &mut Harness, env: &ddi_ui::screens::Env, key: &str) {
    let items = env.state.borrow().page_items.clone();
    let i = items
        .iter()
        .position(|k| k == key)
        .unwrap_or_else(|| panic!("no {key} in {items:?}"));
    h.tap(PointerKind::Mouse, &format!("Item{i}"));
    h.run(0.1);
}

/// Frames (real time between, for the sources' threads) until `done`.
fn until(h: &mut Harness, what: &str, done: impl Fn(&Harness) -> bool) {
    let started = std::time::Instant::now();
    while !done(h) {
        assert!(
            started.elapsed().as_secs() < 60,
            "never {what}: {:?} {:?}",
            h.ui.layers(),
            h.texts()
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
        h.run(0.05);
    }
}

/// The song folders the settings list (`--songs` folders aside).
fn crate_folders() -> Vec<PathBuf> {
    ddi_desktop::library::folders()
}

fn shows(h: &Harness, text: &str) -> bool {
    h.texts().iter().any(|t| t.contains(text))
}

#[test]
fn this_computer_finds_a_song_plays_it_without_importing_and_imports_it() {
    common::indexes();
    let dir = scratch("find-songs");
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("DDI_HOME", dir.join("home")) };
    ddi_desktop::audio::set_headless(true);
    let found = dir.join("found");
    song_at(&found.join("Pack A/Song One"), "Song One", 4);
    song_at(&found.join("Pack A/Song Two"), "Song Two", 7);
    let library = dir.join("library");
    std::fs::create_dir_all(&library).unwrap();

    // Off by default: no source, no Find songs.
    let off = ddi_desktop::songs::Songs::new(vec![library.clone()]).with_sources();
    assert!(!off.finding());
    drop(off);

    // This computer on (`--sources on`), its folder named.
    ddi_desktop::sources::save_switch(true);
    let mut setting = ddi_desktop::sources::Setting::load();
    setting.folders = vec![found.clone()];
    setting.save();
    let songs = ddi_desktop::songs::Songs::new(vec![library.clone()]).with_sources();
    assert!(songs.finding());
    songs.skip_offer();
    songs.start(Arc::new(|| {}));
    let wait = |songs: &ddi_desktop::songs::Songs| {
        if let Some(l) = songs.library.borrow_mut().as_mut() {
            l.wait();
        }
    };
    wait(&songs);
    songs.autoplay.set(true);
    let fold: ddi_ui::dev_widgets::FoldFn = common::ui::nfkd();
    let sound = Rc::new(RefCell::new(ddi_desktop::preview::Player::with(
        ddi_desktop::audio::Audio::silent(0.0),
        ddi_desktop::library::preview_song,
    )));
    let (ui, env, _) = ddi_desktop::shell::library_ui("select", fold, sound, &songs);
    env.reduced_motion.set(true);
    let mut h = Harness::with_gestures(DESIGN, DESIGN, 1.0, TextMode::Own, false);
    h.ui = ui;
    h.run(0.5);
    assert_eq!(top(&h.ui), "select");
    let mut pictures = common::gpu::Pictures::new();

    // More → Find songs: the folder searched, both songs listed.
    h.tap(PointerKind::Mouse, "More");
    h.run(0.4);
    pictures.shot(&mut h.ui, SIZE, "find-songs-more.png");
    h.tap(PointerKind::Mouse, "Item.Link.Find");
    h.run(0.4);
    assert_eq!(top(&h.ui), "find-songs", "{:?}", h.ui.layers());
    until(&mut h, "found", |h| shows(h, "Song Two") && shows(h, "Song One"));
    assert!(shows(&h, "This computer"), "{:?}", h.texts());
    assert!(shows(&h, "2 results."), "{:?}", h.texts());
    pictures.shot(&mut h.ui, SIZE, "find-songs.png");

    // Typed: searched once the field rests.
    let field = h
        .ui
        .bounds("Field.query")
        .map(|_| "Field.query")
        .expect("the search field");
    h.tap(PointerKind::Mouse, field);
    h.run(0.1);
    h.send(ddi_ui::UiEvent::Text(ddi_ui::event::TextEvent::Commit(
        "two".into(),
    )));
    h.key("Enter", "Enter");
    until(&mut h, "searched", |h| shows(h, "1 result") || shows(h, "One result"));
    assert!(!shows(&h, "Song One"), "{:?}", h.texts());

    // Play: its songs alone, from memory; played to its results.
    press(&mut h, &env, "button:play:0");
    until(&mut h, "listed", |h| top(&h.ui) == "select");
    h.run(0.6);
    assert!(shows(&h, "FOUND SONGS"), "{:?}", h.texts());
    assert!(shows(&h, "Song Two"), "{:?}", h.texts());
    pictures.shot(&mut h.ui, SIZE, "find-songs-found.png");
    h.key("Enter", "Enter");
    h.run(0.2);
    let key = {
        let request = songs.play.borrow();
        let request = request.as_ref().expect("a play asked for");
        assert!(request.song.starts_with("src:"), "{}", request.song);
        ddi_source_host::ItemKey(request.held.as_ref().expect("its item held").key())
    };
    let store = ddi_desktop::sources::fetched::store().unwrap();
    let store_plays = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store_plays.clone());
    let mut host = Host::new(songs.play.clone(), Some(plays.clone()));
    let clock = common::rig::fake_clock(&mut host);
    let draws = common::rig::Draws::here(&mut host, SIZE);
    let play_to_results = |host: &mut Host, ui: &mut Ui| {
        draws.wait_prompt(host, ui, &env);
        ui.update(0.5);
        common::ui::key(ui, "Enter", "Enter");
        ui.update(0.05);
        draws.poll(host, ui, &env);
        assert!(host.playing());
        let mut n = 0;
        while host.playing() {
            let now = clock.fetch_add(1_000_000_000 / 60, Ordering::SeqCst) + 1_000_000_000 / 60;
            draws.frame(host, ui, &env, now);
            n += 1;
            assert!(n < 60 * 120, "the play never ended");
        }
        draws.wait_gpu();
        ui.update(0.05);
        assert_eq!(top(ui), "results");
    };
    play_to_results(&mut host, &mut h.ui);
    assert!(store.lock().unwrap().held(key), "held while its results show");
    // Kept by its found song's id (the library has no such song), with
    // where it came from.
    assert_eq!(store_plays.len(), 1, "kept");
    {
        let history = plays.history();
        let (_, s) = history.last().expect("the play");
        assert!(s.chart.song.starts_with("src:"), "{}", s.chart.song);
        let o = s.origin.as_ref().expect("its origin");
        assert_eq!(o.source, "this-computer");
        assert_eq!((o.title.as_str(), o.chart.meter), ("Song Two", 7));
    }
    // The results closed: the item let go.
    h.ui.update(3.0);
    common::ui::key(&mut h.ui, "Escape", "Escape");
    h.ui.update(0.5);
    draws.poll(&mut host, &mut h.ui, &env);
    h.ui.update(1.0);
    assert!(host.idle());
    assert_eq!(top(&h.ui), "select");
    // Back on Find songs: its list gone, the item let go.
    h.key("Escape", "Escape");
    h.run(0.5);
    assert_eq!(top(&h.ui), "find-songs", "{:?}", h.ui.layers());
    assert!(!store.lock().unwrap().held(key), "let go once nothing needs it");

    // Import: its pack's folder added to the song folders where it is,
    // nothing copied; in the library after.
    press(&mut h, &env, "button:import:0");
    until(&mut h, "imported", |h| {
        shows(h, "Added “Pack A” to your song folders.")
    });
    assert_eq!(crate_folders(), [found.join("Pack A")], "the settings' folders");
    pictures.shot(&mut h.ui, SIZE, "find-songs-imported.png");
    wait(&songs);
    songs.poll(&mut h.ui);
    let imported = songs
        .library
        .borrow()
        .as_ref()
        .unwrap()
        .songs
        .iter()
        .find(|s| s.entry.title == "Song Two")
        .cloned()
        .expect("in the library");
    assert!(imported.entry.id.starts_with("u-"));
    assert_eq!(imported.root, found.join("Pack A"), "read where it is");
    assert!(
        ddi_desktop::sources::import::catalog().is_empty()
            && ddi_desktop::sources::import::roots().is_empty(),
        "nothing copied or recorded"
    );
    until(&mut h, "marked", |h| shows(h, "In your library"));
    pictures.shot(&mut h.ui, SIZE, "find-songs-in-library.png");
    // The other song of the pack: in that folder already, nothing added.
    let search_for = |h: &mut Harness, text: &str| {
        h.tap(PointerKind::Mouse, "Field.query");
        for _ in 0..8 {
            h.key("Backspace", "Backspace");
        }
        h.send(ddi_ui::UiEvent::Text(ddi_ui::event::TextEvent::Commit(
            text.into(),
        )));
        h.key("Enter", "Enter");
    };
    search_for(&mut h, "one");
    until(&mut h, "song one", |h| shows(h, "Song One") && !shows(h, "Song Two"));
    assert!(!shows(&h, "Added “Pack A”"), "a new search, the note gone");
    // In the library with its pack: its Import waits (the rule that adds
    // nothing for a folder listed already is `this_computer`'s tests').
    until(&mut h, "song one marked", |h| shows(h, "Pack A · Single 4 · In your library"));
    assert_eq!(crate_folders().len(), 1, "nothing added twice");
    search_for(&mut h, "two");
    until(&mut h, "song two again", |h| shows(h, "Song Two") && !shows(h, "Song One"));

    // Played again from the page: kept now, as the library's song.
    press(&mut h, &env, "button:play:0");
    until(&mut h, "listed again", |h| top(&h.ui) == "select");
    h.key("Enter", "Enter");
    h.run(0.2);
    play_to_results(&mut host, &mut h.ui);
    let history = plays.history();
    let kept: Vec<_> = history.iter().collect();
    assert_eq!(kept.len(), 2, "each play kept once");
    assert_eq!(kept[1].1.chart.song, imported.entry.id, "as the library's song");
    assert_eq!(store_plays.len(), 2);
    // Both are the library song's chart's, each once.
    let digest = kept[1].1.chart.digest;
    assert_eq!(kept[0].1.chart.digest, digest);
    assert_eq!(history.ids_of_chart(digest).len(), 2);
}
