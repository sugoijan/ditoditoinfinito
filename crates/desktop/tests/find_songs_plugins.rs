//! Find songs with plugins (`docs/plans/source-plugins.md`, stage 2): the
//! well-behaved example plugin (`ddi_source_test_plugins::example`, every
//! song generated, no network) installed in a scratch home's plugins
//! folder and switched on, run as a real process, searched from the
//! page, a song played from it to its results without importing it, one
//! imported; and a plugin that exits at once switched on beside it: the
//! page says the source stopped working, and the game goes on with the
//! other.
//!
//! The plugins' processes are this test's own executable, hard linked
//! (or copied) under the plugins' names: run so named, it is that plugin
//! (`main`), so no other build is needed. Hence no test harness: `main`
//! runs the test itself.

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

/// The plugin that exits at once, before any Hello.
const CRASHING: &str = "crashing";

fn main() {
    let me = std::env::current_exe().expect("this executable");
    match me.file_stem().and_then(|s| s.to_str()) {
        Some("example") => ddi_source_sdk::main(ddi_source_test_plugins::example::Example),
        Some(CRASHING) => {
            eprintln!("crashing on purpose");
            std::process::exit(3);
        }
        _ => {
            the_example_plugin_from_find_songs_and_a_crashing_one_beside_it();
            println!("find_songs_plugins: ok");
        }
    }
}

fn top(ui: &Ui) -> String {
    ui.layers()
        .last()
        .map(|l| l.name.clone())
        .unwrap_or_default()
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/scratch")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// This executable installed as plugin `id` in `plugins`, with its
/// manifest.
fn install(plugins: &Path, id: &str, capabilities: &str, inputs: &str) {
    let dir = plugins.join(id);
    let bin = dir.join("bin").join(ddi_source_host::installed::platform());
    std::fs::create_dir_all(&bin).unwrap();
    let exe = bin.join(format!("{id}{}", std::env::consts::EXE_SUFFIX));
    let me = std::env::current_exe().unwrap();
    if std::fs::hard_link(&me, &exe).is_err() {
        std::fs::copy(&me, &exe).unwrap();
    }
    std::fs::write(
        dir.join("plugin.toml"),
        format!(
            "id = \"{id}\"\nname = {{ en = \"Test {id}\" }}\npublisher = \"Tests\"\nversion = \"1.0.0\"\nprotocol = [1]\nexecutable = \"{id}\"\ncapabilities = {capabilities}\ninputs = {inputs}\n"
        ),
    )
    .unwrap();
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

fn shows(h: &Harness, text: &str) -> bool {
    h.texts().iter().any(|t| t.contains(text))
}

fn the_example_plugin_from_find_songs_and_a_crashing_one_beside_it() {
    common::indexes();
    let dir = scratch("find-songs-plugins");
    // SAFETY: set before any thread of this run reads the environment.
    unsafe { std::env::set_var("DDI_HOME", dir.join("home")) };
    ddi_desktop::audio::set_headless(true);
    let plugins = ddi_desktop::sources::plugins_dir().unwrap();
    install(
        &plugins,
        "example",
        r#"["input", "search", "filters", "browse", "metadata", "fetch"]"#,
        r#"["url"]"#,
    );
    install(&plugins, CRASHING, r#"["search", "fetch"]"#, "[]");
    let switches = ddi_desktop::sources::switches_path().unwrap();
    std::fs::create_dir_all(switches.parent().unwrap()).unwrap();
    for id in ["example", CRASHING] {
        ddi_source_host::switches::Switches::set(&switches, id, true).unwrap();
    }
    let library = dir.join("library");
    std::fs::create_dir_all(&library).unwrap();
    // This computer off: the plugins alone.
    let songs = ddi_desktop::songs::Songs::new(vec![library]).with_sources();
    assert!(songs.finding(), "a plugin is switched on");
    songs.skip_offer();
    songs.start(Arc::new(|| {}));
    let wait = |songs: &ddi_desktop::songs::Songs| {
        if let Some(l) = songs.library.borrow_mut().as_mut() {
            l.wait();
        }
    };
    wait(&songs);
    songs.autoplay.set(true);
    let sound = Rc::new(RefCell::new(ddi_desktop::preview::Player::with(
        ddi_desktop::audio::Audio::silent(0.0),
        ddi_desktop::library::preview_song,
    )));
    let (ui, env, _) =
        ddi_desktop::shell::library_ui("select", common::ui::nfkd(), sound, &songs);
    env.reduced_motion.set(true);
    let mut h = Harness::with_gestures(DESIGN, DESIGN, 1.0, TextMode::Own, false);
    h.ui = ui;
    h.run(0.5);
    let mut pictures = common::gpu::Pictures::new();

    // Find songs: the sources in their ids' order, the one that crashes
    // first, asked at once: said plainly, and the game goes on.
    h.tap(PointerKind::Mouse, "More");
    h.run(0.4);
    h.tap(PointerKind::Mouse, "Item.Link.Find");
    h.run(0.4);
    assert_eq!(top(&h.ui), "find-songs", "{:?}", h.ui.layers());
    until(&mut h, "failed", |h| shows(h, "The source stopped working."));
    pictures.shot(&mut h.ui, SIZE, "find-songs-plugin-failed.png");

    // The example's 24 songs, listed as it answers.
    press(&mut h, &env, "button:source:1");
    until(&mut h, "searched", |h| shows(h, "24 results."));
    assert!(shows(&h, "Generated Song 1"), "{:?}", h.texts());
    assert!(shows(&h, "Test example"), "its own name: {:?}", h.texts());
    pictures.shot(&mut h.ui, SIZE, "find-songs-plugins.png");

    // Played from it, without importing: fetched over its pipe, its songs
    // listed alone, played to its results, not kept.
    press(&mut h, &env, "button:play:0");
    until(&mut h, "listed", |h| top(&h.ui) == "select");
    h.run(0.6);
    assert!(shows(&h, "Generated Song 1"), "{:?}", h.texts());
    h.key("Enter", "Enter");
    h.run(0.2);
    assert!(
        songs
            .play
            .borrow()
            .as_ref()
            .is_some_and(|r| r.song.starts_with("src:") && r.held.is_some())
    );
    let store_plays = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store_plays.clone());
    let mut host = Host::new(songs.play.clone(), Some(plays));
    let clock = common::rig::fake_clock(&mut host);
    let draws = common::rig::Draws::here(&mut host, SIZE);
    draws.wait_prompt(&mut host, &mut h.ui, &env);
    h.ui.update(0.5);
    common::ui::key(&mut h.ui, "Enter", "Enter");
    h.ui.update(0.05);
    draws.poll(&mut host, &mut h.ui, &env);
    assert!(host.playing());
    let mut n = 0;
    while host.playing() {
        let now = clock.fetch_add(1_000_000_000 / 60, Ordering::SeqCst) + 1_000_000_000 / 60;
        draws.frame(&mut host, &mut h.ui, &env, now);
        n += 1;
        assert!(n < 60 * 120, "the play never ended");
    }
    draws.wait_gpu();
    h.ui.update(0.05);
    assert_eq!(top(&h.ui), "results");
    assert_eq!(store_plays.len(), 0, "not kept");
    h.ui.update(3.0);
    common::ui::key(&mut h.ui, "Escape", "Escape");
    h.ui.update(0.5);
    draws.poll(&mut host, &mut h.ui, &env);
    h.ui.update(1.0);
    assert!(host.idle());
    h.key("Escape", "Escape");
    h.run(0.5);
    assert_eq!(top(&h.ui), "find-songs", "{:?}", h.ui.layers());

    // Imported: written into the library from memory.
    press(&mut h, &env, "button:import:1");
    until(&mut h, "imported", |h| shows(h, "Imported one song"));
    wait(&songs);
    songs.poll(&mut h.ui);
    assert!(
        songs
            .library
            .borrow()
            .as_ref()
            .unwrap()
            .songs
            .iter()
            .any(|s| s.entry.title == "Generated Song 2" && s.entry.id.starts_with("u-")),
        "in the library"
    );
    until(&mut h, "marked", |h| shows(h, "In your library"));

    // The plugin that crashes asked again: said again; the example still
    // answers after it.
    press(&mut h, &env, "button:source:0");
    until(&mut h, "failed again", |h| shows(h, "The source stopped working."));
    press(&mut h, &env, "button:source:1");
    until(&mut h, "searched again", |h| shows(h, "24 results."));
    assert_eq!(top(&h.ui), "find-songs");
    if let Some(s) = songs.sources.borrow_mut().as_mut() {
        s.host.shutdown();
    }
}
