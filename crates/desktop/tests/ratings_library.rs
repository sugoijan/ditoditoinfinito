//! Chart ratings of a library read in place (`docs/plans/msd-ratings.md`,
//! round Z1): the background thread (`ddi_desktop::ratings`) rates the
//! songs of a scratch pack of synthetic charts while song select shows
//! them, the focused song first, and the skill radar comes for its chart
//! without anything else being done: with the option On from the start
//! (a fresh profile), and turned On while song select is underneath (as
//! Options does it). No GPU, no sound device.

mod common;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ddi_desktop::songs::Songs;
use ddi_platform::storage::{self, MemoryStore};
use ddi_ui::UiEvent;
use ddi_ui::state::place::MenuPlace;
use ddi_ui::state::ratings;
use ddi_ui::state::settings::Settings;

/// A mono 16-bit WAV of silence, `seconds` long, at 8 kHz.
fn wav(seconds: u32) -> Vec<u8> {
    let rate = 8_000u32;
    let n = seconds * rate;
    let mut out = b"RIFF".to_vec();
    out.extend((36 + n * 2).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(rate.to_le_bytes());
    out.extend((rate * 2).to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend((n * 2).to_le_bytes());
    out.extend(std::iter::repeat_n(0u8, (n * 2) as usize));
    out
}

/// `beats` of sixteenths over `lanes` lanes, a stream stepping along.
fn stream(beats: usize, lanes: usize) -> String {
    let rows: Vec<String> = (0..beats * 4)
        .map(|i| {
            let mut r = vec!['0'; lanes];
            r[(i * 3 + i / 5) % lanes] = '1';
            r.into_iter().collect()
        })
        .collect();
    rows.chunks(16)
        .map(|m| m.join("\n"))
        .collect::<Vec<_>>()
        .join("\n,\n")
}

/// `songs` songs in `dir/MSD Pack/`, each a Single Medium and Hard and a
/// Double Hard (which is not rated: eight lanes).
fn pack(dir: &Path, songs: usize) {
    for s in 0..songs {
        let song = dir.join(format!("MSD Pack/Song {s:02}"));
        std::fs::create_dir_all(&song).unwrap();
        std::fs::write(song.join("music.wav"), wav(40)).unwrap();
        let mut sm = format!(
            "#TITLE:Song {s:02};#ARTIST:Synthetic;#MUSIC:music.wav;#OFFSET:0;#BPMS:0={};\n",
            130 + 10 * s
        );
        for (d, m, lanes, style) in [
            ("Medium", 6, 4, "dance-single"),
            ("Hard", 9, 4, "dance-single"),
            ("Hard", 10, 8, "dance-double"),
        ] {
            sm.push_str(&format!(
                "#NOTES:{style}:x:{d}:{m}:0,0,0,0,0:\n{};\n",
                stream(if d == "Medium" { 48 } else { 96 }, lanes)
            ));
        }
        std::fs::write(song.join("song.sm"), sm).unwrap();
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/scratch/ratings-library")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("songs")).unwrap();
    pack(&dir.join("songs"), 4);
    dir
}

/// The library scanned: the id of `title`'s song.
fn scanned(songs: &Rc<Songs>, title: &str) -> String {
    let started = Instant::now();
    loop {
        let id = songs.library.borrow().as_ref().and_then(|l| {
            (!l.scanning)
                .then(|| l.songs.iter().find(|s| s.entry.title == title))
                .flatten()
                .map(|s| s.entry.id.clone())
        });
        if let Some(id) = id {
            return id;
        }
        let mut ui = ddi_ui::Ui::new(ddi_ui::screens::DESIGN, ddi_ui::TextMode::Own, true);
        songs.poll(&mut ui);
        assert!(started.elapsed().as_secs() < 60, "the scan never ended");
        std::thread::sleep(Duration::from_millis(20));
    }
}

struct Run {
    ui: ddi_ui::Ui,
    env: ddi_ui::screens::Env,
    songs: Rc<Songs>,
    rater: ddi_desktop::ratings::Ratings,
}

impl Run {
    /// Song select on the library, `id` shown.
    fn open(dir: &Path, id: &str) -> Run {
        MenuPlace {
            known: true,
            song: Some(id.to_string()),
            chart: Some((id.to_string(), 1)),
            ..MenuPlace::default()
        }
        .save();
        let songs = Songs::new(vec![dir.join("songs")]);
        songs.skip_offer();
        songs.start(Arc::new(|| {}));
        // Its songs read (from the cache the first scan left).
        assert!(!scanned(&songs, "Song 03").is_empty());
        let fold: ddi_ui::dev_widgets::FoldFn = Rc::new(|s: &str| s.to_lowercase());
        let sound = Rc::new(std::cell::RefCell::new(ddi_desktop::preview::Player::with(
            ddi_desktop::audio::Audio::silent(0.0),
            ddi_desktop::preview::PreviewSong::bundled,
        )));
        let (mut ui, env, _) = ddi_desktop::shell::library_ui("select", fold, sound, &songs);
        ui.handle(UiEvent::Resize {
            size: [1280.0, 720.0],
            ratio: 1.0,
        });
        Run {
            ui,
            env,
            songs,
            rater: Default::default(),
        }
    }

    /// The loop's turn: the library's news, the ratings' thread, a frame.
    fn step(&mut self) {
        self.songs.poll(&mut self.ui);
        self.rater.poll(false, None);
        self.ui.update(1.0 / 30.0);
        std::thread::sleep(Duration::from_millis(10));
    }

    /// Steps until `done`, at most `limit`: how long it took.
    fn until(&mut self, what: &str, limit: f64, mut done: impl FnMut(&Run) -> bool) -> f64 {
        let began = Instant::now();
        while !done(self) {
            self.step();
            assert!(
                began.elapsed().as_secs_f64() < limit,
                "{what}: not within {limit} s; ratings {:?}",
                ratings::status()
            );
        }
        began.elapsed().as_secs_f64()
    }

    fn radar(&self) -> bool {
        self.ui.bounds("Radar").is_some()
    }

    /// The song shown.
    fn shown(&self) -> String {
        self.env.state.borrow().song_id.clone()
    }
}

fn settings(radar: bool) {
    let mut s = Settings::load();
    s.skill_radar = radar;
    s.save();
    s.show_ratings();
}

#[test]
fn a_library_songs_radar_comes_once_its_chart_is_rated() {
    common::indexes();
    ddi_desktop::audio::set_headless(true);
    let dir = scratch("fresh");
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("DDI_HOME", dir.join("home")) };
    storage::install(Rc::new(MemoryStore::default()));
    let id = {
        let songs = Songs::new(vec![dir.join("songs")]);
        songs.skip_offer();
        songs.start(Arc::new(|| {}));
        scanned(&songs, "Song 03")
    };
    // A fresh profile, the option On, song select opened at once.
    settings(true);
    let mut run = Run::open(&dir, &id);
    run.step();
    assert_eq!(run.shown(), id, "the library song shown");
    assert!(ratings::status().pending > 0, "offered: {:?}", ratings::status());
    let took = run.until("the radar", 30.0, Run::radar);
    eprintln!("radar on a fresh profile after {took:.2} s; {:?}", ratings::status());
    // Every song rated in the end.
    run.until("every song rated", 60.0, |_| {
        let s = ratings::status();
        s.pending == 0 && s.rating.is_none()
    });
    assert!(ratings::status().rated >= 8, "{:?}", ratings::status());
}

#[test]
fn a_radar_turned_on_under_song_select_comes_without_a_restart() {
    common::indexes();
    ddi_desktop::audio::set_headless(true);
    let dir = scratch("turned");
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("DDI_HOME", dir.join("home")) };
    storage::install(Rc::new(MemoryStore::default()));
    let id = {
        let songs = Songs::new(vec![dir.join("songs")]);
        songs.skip_offer();
        songs.start(Arc::new(|| {}));
        scanned(&songs, "Song 02")
    };
    settings(false);
    let mut run = Run::open(&dir, &id);
    run.until("song select at rest", 10.0, |r| r.ui.bounds("Info").is_some());
    assert_eq!(run.shown(), id, "the library song shown");
    for _ in 0..30 {
        run.step();
    }
    assert!(!run.radar(), "Off: none");
    assert_eq!(ratings::status().pending, 0, "Off: nothing offered");
    // Turned On as Options turns it (song select underneath).
    settings(true);
    let took = run.until("the radar", 30.0, Run::radar);
    eprintln!("radar turned on after {took:.2} s; {:?}", ratings::status());
}
