//! Plays of found songs kept, and found again (`docs/plans/source-plugins.md`,
//! "Plays of found songs"): over a scratch home, with "This computer" on
//! (a folder of generated songs) and the example plugin installed and
//! switched on as a real process, headless, on a fake clock (nothing pressed: plays that count).
//!
//! - A song found on this computer and one of the plugin's played without
//!   importing them: each kept by its found song's id, with where it came
//!   from.
//! - The run ended and another started (the store of fetched songs
//!   emptied): both listed under "Played before" on Find songs, from the
//!   plays alone; Play fetches each again, by the same id, its plays (P)
//!   and its best there, its replay watched.
//! - The plugin switched off: its row stays, with its line, no button.
//! - Judged by an older judge with the chart absent: kept as played,
//!   passed over quietly; once its song is fetched again, judged again.
//! - Imported afterwards (in place): the play the library song's chart's,
//!   counted once; a play of the library song beside it.
//! - A new revision whose chart is unchanged carries its plays; one whose
//!   notes changed starts its chart with none, the old plays kept.
//!
//! The pictures (`plays-*.png`) where `DDI_OFFSCREEN_PPM` names a folder
//! and a GPU is there. The plugin's process is this test's own
//! executable, linked under the plugin's name (`main`), so no harness.

mod common;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::play::host::Host;
use ddi_desktop::songs::Songs;
use ddi_ui::screens::DESIGN;
use ddi_ui::state::history::{JUDGE_VERSION, PlaySummary};
use ddi_ui::testing::Harness;
use ddi_ui::{PointerKind, TextMode, Ui};

const SIZE: [u32; 2] = [1280, 720];

fn main() {
    let me = std::env::current_exe().expect("this executable");
    match me.file_stem().and_then(|s| s.to_str()) {
        Some("example") => ddi_source_sdk::main(ddi_source_test_plugins::example::Example),
        _ => {
            found_songs_plays_are_kept_and_found_again();
            println!("found_plays: ok");
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

/// A simfile: `title`, a Single chart at `meter` of `rows` (four a beat).
fn simfile(title: &str, meter: u32, rows: &str) -> String {
    format!(
        "#VERSION:0.83;\n#TITLE:{title};\n#ARTIST:Generated;\n#MUSIC:song.wav;\n#OFFSET:0;\n#BPMS:0=120;\n#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Hard;\n#METER:{meter};\n#NOTES:\n{rows};\n"
    )
}

/// Notes of its own (the example plugin's songs have others: a chart of
/// the same content would share its plays).
const ROWS: &str = "1001\n0110\n1001\n0110\n";
const OTHER_ROWS: &str = "1001\n0110\n1001\n0110\n,\n1100\n0000\n0011\n0000\n";

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

/// This executable installed as plugin `id` in `plugins`, with its
/// manifest.
fn install(plugins: &Path, id: &str) {
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
            "id = \"{id}\"\nname = {{ en = \"Test {id}\" }}\npublisher = \"Tests\"\nversion = \"1.0.0\"\nprotocol = [1]\nexecutable = \"{id}\"\ncapabilities = [\"input\", \"search\", \"filters\", \"browse\", \"metadata\", \"fetch\"]\ninputs = [\"url\"]\n"
        ),
    )
    .unwrap();
}

fn shows(h: &Harness, text: &str) -> bool {
    h.texts().iter().any(|t| t.contains(text))
}

/// One run of the window, headless: its songs and sources, its UI on
/// song select, the play host over its history.
struct Run {
    songs: Rc<Songs>,
    h: Harness,
    env: ddi_ui::screens::Env,
    host: Host,
    clock: Arc<AtomicU64>,
    draws: common::rig::Draws,
    pictures: common::gpu::Pictures,
}

impl Run {
    fn start(library: &Path) -> Run {
        // What a window's start does: the fetched songs a run left gone.
        ddi_desktop::sources::clear_store();
        let songs = Songs::new(vec![library.to_path_buf()]).with_sources();
        assert!(songs.finding());
        songs.skip_offer();
        songs.start(Arc::new(|| {}));
        if let Some(l) = songs.library.borrow_mut().as_mut() {
            l.wait();
        }
        // Played by nobody (every note missed): a player's own play, which
        // counts for the bests, as an autoplayed one would not.
        songs.autoplay.set(false);
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
        let mut host = Host::new(songs.play.clone(), songs.plays.clone());
        let clock = common::rig::fake_clock(&mut host);
        let draws = common::rig::Draws::here(&mut host, SIZE);
        Run {
            songs,
            h,
            env,
            host,
            clock,
            draws,
            pictures: common::gpu::Pictures::new(),
        }
    }

    /// Frames (real time between, for the sources' threads; the library
    /// and the history's re-working polled as the window's loop does)
    /// until `done`.
    fn until(&mut self, what: &str, done: impl Fn(&Run) -> bool) {
        let started = std::time::Instant::now();
        while !done(self) {
            assert!(
                started.elapsed().as_secs() < 60,
                "never {what}: {:?} {:?}",
                self.h.ui.layers(),
                self.h.texts()
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
            self.h.run(0.05);
            self.songs.rework(false, false);
            self.songs.poll(&mut self.h.ui);
        }
    }

    fn shows(&self, text: &str) -> bool {
        shows(&self.h, text)
    }

    fn shot(&mut self, name: &str) {
        self.pictures.shot(&mut self.h.ui, SIZE, name);
    }

    /// The page's item `key` (`button:play:0`) tapped.
    fn press(&mut self, key: &str) {
        let items = self.env.state.borrow().page_items.clone();
        let i = items
            .iter()
            .position(|k| k == key)
            .unwrap_or_else(|| panic!("no {key} in {items:?}"));
        self.h.tap(PointerKind::Mouse, &format!("Item{i}"));
        self.h.run(0.1);
    }

    fn open_find(&mut self) {
        self.h.tap(PointerKind::Mouse, "More");
        self.h.run(0.4);
        self.h.tap(PointerKind::Mouse, "Item.Link.Find");
        self.h.run(0.4);
        assert_eq!(top(&self.h.ui), "find-songs", "{:?}", self.h.ui.layers());
    }

    /// Back to Find songs from a found song's list.
    fn back_to_find(&mut self) {
        self.h.key("Escape", "Escape");
        self.h.run(0.5);
        assert_eq!(top(&self.h.ui), "find-songs", "{:?}", self.h.ui.layers());
    }

    /// The search field set to `text`, searched.
    fn search(&mut self, text: &str) {
        self.h.tap(PointerKind::Mouse, "Field.query");
        for _ in 0..24 {
            self.h.key("Backspace", "Backspace");
        }
        if !text.is_empty() {
            self.h
                .send(ddi_ui::UiEvent::Text(ddi_ui::event::TextEvent::Commit(
                    text.into(),
                )));
        }
        self.h.key("Enter", "Enter");
    }

    /// The song asked for (song select's Enter, or a replay) played from
    /// its prompt to its results, and back to the list.
    fn play_to_results(&mut self) {
        let (host, ui, env) = (&mut self.host, &mut self.h.ui, &self.env);
        self.draws.wait_prompt(host, ui, env);
        ui.update(0.5);
        common::ui::key(ui, "Enter", "Enter");
        ui.update(0.05);
        self.draws.poll(host, ui, env);
        assert!(host.playing());
        let mut n = 0;
        while host.playing() {
            let now =
                self.clock.fetch_add(1_000_000_000 / 60, Ordering::SeqCst) + 1_000_000_000 / 60;
            self.draws.frame(host, ui, env, now);
            n += 1;
            assert!(n < 60 * 120, "the play never ended");
        }
        self.draws.wait_gpu();
        ui.update(0.05);
        assert_eq!(top(ui), "results");
        ui.update(3.0);
        common::ui::key(ui, "Escape", "Escape");
        ui.update(0.5);
        self.draws.poll(host, ui, env);
        ui.update(1.0);
        assert!(host.idle());
        assert_eq!(top(ui), "select");
    }

    /// Song select's first song played (Enter), from its prompt to its
    /// results: its id.
    fn play_listed(&mut self) -> String {
        self.h.key("Enter", "Enter");
        self.h.run(0.2);
        let id = self
            .songs
            .play
            .borrow()
            .as_ref()
            .expect("a play asked for")
            .song
            .clone();
        self.play_to_results();
        id
    }

    /// The history's plays, oldest first.
    fn kept(&self) -> Vec<(String, PlaySummary)> {
        let plays = self.songs.plays.as_ref().expect("the history");
        plays
            .history()
            .iter()
            .map(|(id, s)| (id.clone(), s.clone()))
            .collect()
    }

    /// The found songs in the store titled `title`, by id.
    fn fetched(&self, title: &str) -> Vec<String> {
        let store = ddi_desktop::sources::fetched::store().expect("the store");
        let every = store.lock().unwrap().every_song();
        every
            .into_iter()
            .filter(|(s, _)| s.entry.title == title)
            .map(|(s, _)| s.id)
            .collect()
    }

    fn end(self) {
        if let Some(s) = self.songs.sources.borrow_mut().as_mut() {
            s.host.shutdown();
        }
        drop(self.h);
        let _ = ddi_platform::storage::flush();
    }
}

fn found_songs_plays_are_kept_and_found_again() {
    common::indexes();
    let dir = scratch("found-plays");
    // SAFETY: set before any thread of this run reads the environment.
    unsafe { std::env::set_var("DDI_HOME", dir.join("home")) };
    ddi_desktop::audio::set_headless(true);
    let plugins = ddi_desktop::sources::plugins_dir().unwrap();
    install(&plugins, "example");
    let switches = ddi_desktop::sources::switches_path().unwrap();
    std::fs::create_dir_all(switches.parent().unwrap()).unwrap();
    ddi_source_host::switches::Switches::set(&switches, "example", true).unwrap();
    let found = dir.join("found");
    let local = found.join("Pack A/Local One");
    std::fs::create_dir_all(&local).unwrap();
    std::fs::write(local.join("song.ssc"), simfile("Local One", 4, ROWS)).unwrap();
    std::fs::write(local.join("song.wav"), silence()).unwrap();
    let library = dir.join("library");
    std::fs::create_dir_all(&library).unwrap();
    ddi_desktop::sources::save_switch(true);
    let mut setting = ddi_desktop::sources::Setting::load();
    setting.folders = vec![found.clone()];
    setting.save();

    // Run 1: a song of this computer's and one of the plugin's played
    // without importing them, each kept by its found id with its origin.
    let mut r = Run::start(&library);
    r.open_find();
    r.until("found", |r| r.shows("Local One"));
    r.press("button:play:0");
    r.until("listed", |r| top(&r.h.ui) == "select");
    let local_id = r.play_listed();
    assert!(local_id.starts_with("src:"), "{local_id}");
    r.back_to_find();
    r.until("played before", |r| {
        r.shows("Played before") && r.shows("Last played")
    });
    r.shot("plays-run-1.png");
    r.press("button:source:1");
    r.until("the plugin's", |r| r.shows("24 results."));
    r.search("song 3");
    r.until("song 3", |r| {
        r.shows("One result") && r.shows("Generated Song 3")
    });
    r.press("button:play:0");
    r.until("song 3 listed", |r| top(&r.h.ui) == "select");
    let example_id = r.play_listed();
    let kept = r.kept();
    assert_eq!(kept.len(), 2, "both kept");
    assert_eq!(kept[0].1.chart.song, local_id);
    assert_eq!(kept[1].1.chart.song, example_id);
    let o = kept[1].1.origin.clone().expect("its origin");
    assert_eq!(
        (
            o.source.as_str(),
            o.item.as_str(),
            o.revision.as_deref(),
            o.song
        ),
        ("example", "song-3", Some("r1"), 0)
    );
    assert_eq!(o.title, "Generated Song 3");
    let local_origin = kept[0].1.origin.clone().expect("its origin");
    assert_eq!(local_origin.source, "this-computer");
    let local_digest = kept[0].1.chart.digest;
    r.end();

    // Run 2: the store emptied; both listed under Played before from the
    // plays alone, fetched again by the same ids.
    let mut r = Run::start(&library);
    assert!(
        ddi_desktop::sources::fetched::store()
            .unwrap()
            .lock()
            .unwrap()
            .every_song()
            .is_empty(),
        "nothing fetched yet"
    );
    r.open_find();
    r.until("played before", |r| {
        r.shows("Played before") && r.shows("Local One")
    });
    r.shot("plays-played-before.png");
    r.press("button:played:0");
    r.until("found again", |r| top(&r.h.ui) == "select");
    r.h.run(0.6);
    assert_eq!(
        r.fetched("Local One"),
        [local_id.as_str()],
        "the same id as before"
    );
    assert!(r.shows("FOUND SONGS"));
    r.shot("plays-found-again.png");
    // Its best and its plays, by its chart.
    {
        let plays = r.songs.plays.as_ref().unwrap();
        let best = plays
            .history()
            .bests_of_charts(&[local_digest])
            .into_values()
            .next()
            .expect("its best");
        assert_eq!(best.plays, 1);
    }
    r.h.key("KeyP", "p");
    r.h.run(0.5);
    assert_eq!(top(&r.h.ui), "plays", "{:?}", r.h.ui.layers());
    r.shot("plays-plays-list.png");
    // Its replay watched: the play's results, not kept again.
    r.h.key("Enter", "Enter");
    r.h.run(0.3);
    assert!(
        r.songs
            .play
            .borrow()
            .as_ref()
            .is_some_and(|p| p.replay.is_some() && p.refused.is_none()),
        "a replay asked for"
    );
    r.play_to_results();
    let play_id = &kept[0].0;
    assert!(
        r.host
            .log
            .iter()
            .any(|l| l == &format!("replay {play_id}: the play's results")),
        "{:?}",
        r.host.log
    );
    assert_eq!(r.kept().len(), 2, "a replay is not kept");
    r.back_to_find();
    r.press("button:source:1");
    r.until("the plugin's played before", |r| {
        r.shows("Played before") && r.shows("Generated Song 3") && r.shows("24 results.")
    });
    r.press("button:played:0");
    r.until("song 3 again", |r| top(&r.h.ui) == "select");
    assert_eq!(
        r.fetched("Generated Song 3"),
        [example_id.as_str()],
        "the same id as before"
    );
    r.back_to_find();
    r.end();

    // Both judged by an older judge now (as after an update of the game).
    let plays_dir = ddi_desktop::history::plays_dir().unwrap();
    let mut staled = 0;
    for e in std::fs::read_dir(&plays_dir).unwrap().flatten() {
        let path = e.path();
        if !path.to_string_lossy().ends_with(".summary.json") {
            continue;
        }
        let mut v: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        v["worked"]["judge_version"] = serde_json::json!(JUDGE_VERSION - 1);
        std::fs::write(&path, serde_json::to_vec(&v).unwrap()).unwrap();
        staled += 1;
    }
    assert_eq!(staled, 2);

    // Run 3: the plugin switched off: its row stays, with its line.
    ddi_source_host::switches::Switches::set(&switches, "example", false).unwrap();
    let mut r = Run::start(&library);
    r.open_find();
    r.until("its source off", |r| {
        r.shows("Played before from Test example") && r.shows("Its source is not on")
    });
    assert!(r.shows("Generated Song 3"));
    r.shot("plays-source-off.png");
    // The charts absent: kept as played, stale, passed over quietly (a
    // background pass and more).
    let started = std::time::Instant::now();
    r.until("a pass over them", |_| {
        started.elapsed().as_secs_f64() > 4.0
    });
    let stale = |r: &Run, song: &str| {
        r.kept()
            .into_iter()
            .find(|(_, s)| s.chart.song == song)
            .map(|(_, s)| {
                assert!(s.worked.is_some(), "what it came to as played kept");
                s.is_stale()
            })
            .expect("still kept")
    };
    assert!(stale(&r, &local_id) && stale(&r, &example_id));
    // This computer's fetched again: its play judged again.
    r.press("button:played:0");
    r.until("found again", |r| top(&r.h.ui) == "select");
    r.until("judged again", |r| !stale(r, &local_id));
    assert!(stale(&r, &example_id), "its source still off");
    r.back_to_find();

    // Imported (in place): the play is the library song's chart's, once.
    r.search("local");
    r.until("local", |r| r.shows("One result") && r.shows("Local One"));
    r.press("button:import:0");
    r.until("imported", |r| {
        r.shows("Added “Pack A” to your song folders.")
    });
    r.until("in the library", |r| r.shows("In your library"));
    r.shot("plays-imported.png");
    let library_id = r
        .songs
        .library
        .borrow()
        .as_ref()
        .unwrap()
        .songs
        .iter()
        .find(|s| s.entry.title == "Local One")
        .map(|s| s.entry.id.clone())
        .expect("in the library");
    let best_plays = |r: &Run, digest| {
        r.songs
            .plays
            .as_ref()
            .unwrap()
            .history()
            .bests_of_charts(&[digest])
            .into_values()
            .map(|b| b.plays)
            .sum::<u32>()
    };
    assert_eq!(best_plays(&r, local_digest), 1, "counted once");
    // Played again: kept as the library's song, beside it.
    r.press("button:play:0");
    r.until("listed", |r| top(&r.h.ui) == "select");
    r.play_listed();
    let kept = r.kept();
    assert_eq!(kept.len(), 3);
    assert_eq!(kept[2].1.chart.song, library_id);
    assert_eq!(best_plays(&r, local_digest), 2);
    r.back_to_find();

    // A new revision, its chart unchanged (the title longer): fetched
    // again as its current revision, said so; its chart's plays there.
    std::fs::write(
        local.join("song.ssc"),
        simfile("Local One (Longer Title)", 4, ROWS),
    )
    .unwrap();
    r.h.key("Escape", "Escape");
    r.h.run(0.5);
    r.open_find();
    r.until("played before", |r| {
        r.shows("Played before") && r.shows("Local One")
    });
    r.press("button:played:0");
    r.until("found, changed", |r| top(&r.h.ui) == "select");
    let renewed = r
        .fetched("Local One (Longer Title)")
        .pop()
        .expect("fetched");
    assert_ne!(renewed, local_id, "another revision, another id");
    {
        let store = ddi_desktop::sources::fetched::store().unwrap();
        let mut st = store.lock().unwrap();
        let (song, _) = st.song(&renewed).unwrap();
        assert_eq!(
            song.entry.charts[0].digest,
            Some(local_digest),
            "the same chart"
        );
    }
    assert_eq!(best_plays(&r, local_digest), 2, "its plays carried over");
    r.back_to_find();
    assert!(r.shows("This song changed at its source since you last played it."));
    r.shot("plays-changed.png");
    // Another, its notes changed: its chart starts with no plays; the old
    // chart's are kept.
    std::fs::write(local.join("song.ssc"), simfile("Local One", 4, OTHER_ROWS)).unwrap();
    r.h.key("Escape", "Escape");
    r.h.run(0.5);
    r.open_find();
    r.until("played before", |r| {
        r.shows("Played before") && r.shows("Local One")
    });
    r.press("button:played:0");
    r.until("found, its notes changed", |r| top(&r.h.ui) == "select");
    let changed = r
        .fetched("Local One")
        .into_iter()
        .find(|id| *id != local_id)
        .expect("fetched as another revision");
    let new_digest = {
        let store = ddi_desktop::sources::fetched::store().unwrap();
        let mut st = store.lock().unwrap();
        st.song(&changed).unwrap().0.entry.charts[0].digest.unwrap()
    };
    assert_ne!(new_digest, local_digest);
    assert_eq!(best_plays(&r, new_digest), 0, "no plays of the new chart");
    assert_eq!(best_plays(&r, local_digest), 2, "the old chart's kept");
    assert_eq!(r.kept().len(), 3, "nothing removed");
    r.back_to_find();
    r.end();
}
