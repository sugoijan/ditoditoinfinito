//! Dancing☆Onigiri works and FFR levels found by "This computer"
//! (`docs/plans/source-plugins.md`, "This computer reads what the library
//! reads"), over a scratch home and a folder of a generated work and a
//! generated level, headless, on a fake clock (nothing pressed: plays that
//! count):
//!
//! - Both found on Find songs, played without importing them to their
//!   results, each play kept by its found song's id with where it came
//!   from.
//! - The run ended and another started (the store of fetched songs
//!   emptied): both listed under "Played before" from the plays alone,
//!   fetched again by the same ids.
//! - Imported in place: the work's folder and the level's added to the
//!   library's song folders, nothing copied; the library then lists them
//!   and the page says they are in it.
//!
//! Every wait is for a state (a screen, a text, a play's end), never for a
//! time. The pictures (`found-works-*.png`) where `DDI_OFFSCREEN_PPM`
//! names a folder and a GPU is there.

mod common;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::play::host::Host;
use ddi_desktop::songs::Songs;
use ddi_ui::screens::DESIGN;
use ddi_ui::state::history::PlaySummary;
use ddi_ui::testing::Harness;
use ddi_ui::{PointerKind, TextMode, Ui};

const SIZE: [u32; 2] = [1280, 720];

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

/// A work in `pack`: its page in `danoni/`, its music in `music/` (where
/// danoniplus looks for it), one song of eight notes.
fn work_at(pack: &Path, title: &str) {
    std::fs::create_dir_all(pack.join("danoni")).unwrap();
    std::fs::create_dir_all(pack.join("music")).unwrap();
    std::fs::write(pack.join("music/work.wav"), silence()).unwrap();
    let left: Vec<String> = (230..470).step_by(60).map(|f| f.to_string()).collect();
    let down: Vec<String> = (260..500).step_by(60).map(|f| f.to_string()).collect();
    let html = format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>{title}</title></head><body>\n\
         <input type=\"hidden\" name=\"dos\" id=\"dos\" value='\n\
         |musicTitle={title},Generated,,,120|\n|difData=5,Easy,3.5|\n|musicUrl=work.wav|\n\
         |blankFrame=200|\n|left_data={}|\n|down_data={}|\n'>\n</body></html>\n",
        left.join(","),
        down.join(",")
    );
    std::fs::write(pack.join("danoni/work.html"), html).unwrap();
}

/// An FFR level of eight notes, its music the platform's MP3 fixture.
fn level() -> Vec<u8> {
    let mp3 = include_bytes!("../../platform/tests/fixtures/sine-441-bare.mp3").repeat(8);
    let notes: Vec<(f64, &str, &str)> = (0..8)
        .map(|i| (30.0 + 5.0 * i as f64, ["L", "D", "U", "R"][i % 4], "blue"))
        .collect();
    ddi_library::swf::write_level(&notes, &mp3, true)
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
        // Played by nobody: a player's own play, as an autoplayed one
        // would not be.
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

    /// Frames until `done` (the sources' threads and the library polled
    /// as the window's loop polls them). Waits for the state alone: the
    /// bound is far past any machine's time, only so a test that can
    /// never pass ends.
    fn until(&mut self, what: &str, done: impl Fn(&Run) -> bool) {
        let mut frames = 0u32;
        while !done(self) {
            frames += 1;
            assert!(
                frames < 30_000,
                "never {what}: {:?} {:?}",
                self.h.ui.layers(),
                self.h.texts()
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
            self.h.run(0.05);
            self.songs.rework(false, false);
            self.songs.poll(&mut self.h.ui);
        }
    }

    fn shows(&self, text: &str) -> bool {
        self.h.texts().iter().any(|t| t.contains(text))
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

    /// The results for `query`, once they are its: a search that finds
    /// nothing first, so no earlier search's results are taken for them;
    /// then one result.
    fn find_one(&mut self, query: &str) {
        self.search("no such song anywhere");
        self.until("nothing", |r| r.shows("Nothing found."));
        self.search(query);
        self.until(query, |r| r.shows("One result."));
    }

    /// The only result for `query` played from its prompt to its results
    /// (song select's first song), and back to Find songs: its song's id.
    fn play_found(&mut self, query: &str) -> String {
        self.find_one(query);
        self.press("button:play:0");
        self.until("its list", |r| top(&r.h.ui) == "select");
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
        self.back_to_find();
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

    /// The library's song titled `title`.
    fn in_library(&self, title: &str) -> Option<ddi_desktop::library::LibSong> {
        self.songs
            .library
            .borrow()
            .as_ref()?
            .songs
            .iter()
            .find(|s| s.entry.title == title)
            .cloned()
    }

    fn end(self) {
        if let Some(s) = self.songs.sources.borrow_mut().as_mut() {
            s.host.shutdown();
        }
        drop(self.h);
        let _ = ddi_platform::storage::flush();
    }
}

#[test]
fn found_works_and_levels_play_keep_their_plays_and_import_in_place() {
    common::indexes();
    let dir = scratch("found-works");
    // SAFETY: set before any thread of this run reads the environment
    // (this file's one test).
    unsafe { std::env::set_var("DDI_HOME", dir.join("home")) };
    ddi_desktop::audio::set_headless(true);
    let found = dir.join("found");
    work_at(&found.join("WorkPack"), "Work Test");
    std::fs::create_dir_all(found.join("FFR Levels")).unwrap();
    std::fs::write(found.join("FFR Levels/level_1.swf"), level()).unwrap();
    let library = dir.join("library");
    std::fs::create_dir_all(&library).unwrap();
    ddi_desktop::sources::save_switch(true);
    let mut setting = ddi_desktop::sources::Setting::load();
    setting.folders = vec![found.clone()];
    setting.save();

    // Run 1: both found, played without importing them, each kept by its
    // found id with its origin.
    let mut r = Run::start(&library);
    r.open_find();
    r.until("found", |r| r.shows("Work Test") && r.shows("level_1"));
    r.shot("found-works-found.png");
    let work_id = r.play_found("Work Test");
    assert!(work_id.starts_with("src:"), "{work_id}");
    let level_id = r.play_found("level_1");
    assert!(level_id.starts_with("src:"), "{level_id}");
    assert_ne!(work_id, level_id);
    let kept = r.kept();
    assert_eq!(kept.len(), 2, "both kept");
    assert_eq!(kept[0].1.chart.song, work_id);
    assert_eq!(kept[1].1.chart.song, level_id);
    for (_, s) in &kept {
        let o = s.origin.clone().expect("its origin");
        assert_eq!(o.source, "this-computer");
    }
    assert_eq!(kept[0].1.origin.as_ref().unwrap().title, "Work Test");
    assert_eq!(kept[1].1.origin.as_ref().unwrap().title, "level_1");
    r.end();

    // Run 2: the store emptied; both listed under Played before from the
    // plays alone, fetched again by the same ids.
    let mut r = Run::start(&library);
    r.open_find();
    r.search("");
    r.until("played before", |r| {
        r.shows("Played before") && r.shows("Work Test") && r.shows("level_1")
    });
    r.shot("found-works-played-before.png");
    for row in ["button:played:0", "button:played:1"] {
        r.press(row);
        r.until("found again", |r| top(&r.h.ui) == "select");
        r.back_to_find();
    }
    assert_eq!(r.fetched("Work Test"), [work_id.as_str()], "the same id");
    assert_eq!(r.fetched("level_1"), [level_id.as_str()], "the same id");

    // Imported in place: the work's folder, the level's; the library
    // lists them where they are.
    r.find_one("Work Test");
    assert!(r.shows("Work Test"));
    r.press("button:import:0");
    r.until("the work added", |r| {
        r.shows("Added “WorkPack” to your song folders.")
    });
    r.until("the work in the library", |r| {
        r.in_library("Work Test").is_some() && r.shows("In your library")
    });
    let work = r.in_library("Work Test").unwrap();
    assert!(work.work.is_some(), "a work's song");
    assert_eq!(work.root, found.join("WorkPack"), "where it is");
    r.shot("found-works-imported.png");
    r.find_one("level_1");
    assert!(r.shows("level_1"));
    r.press("button:import:0");
    r.until("the level added", |r| {
        r.shows("Added “FFR Levels” to your song folders.")
    });
    r.until("the level in the library", |r| {
        r.in_library("level_1").is_some() && r.shows("In your library")
    });
    let level = r.in_library("level_1").unwrap();
    assert!(ddi_library::ffr::is_level_chart(&level.entry.chart));
    assert_eq!(level.root, found.join("FFR Levels"), "where it is");
    // Nothing copied: the folder found holds what it held.
    let mut files: Vec<String> = Vec::new();
    for e in walk(&found) {
        files.push(e.strip_prefix(&found).unwrap().display().to_string());
    }
    files.sort();
    assert_eq!(files.len(), 3, "{files:?}");
    // The plays of the found songs count for the library's charts.
    let digest = kept[0].1.chart.digest;
    let plays = r
        .songs
        .plays
        .as_ref()
        .unwrap()
        .history()
        .bests_of_charts(&[digest])
        .into_values()
        .map(|b| b.plays)
        .sum::<u32>();
    assert_eq!(plays, 1, "the work's play, once");
    r.end();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every file under `dir`.
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            if e.file_type().unwrap().is_dir() {
                stack.push(e.path());
            } else {
                out.push(e.path());
            }
        }
    }
    out
}
