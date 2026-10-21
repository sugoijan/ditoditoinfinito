//! FFR levels in the library on disk (`docs/plans/phase-10.md`, step 2):
//! a folder of level SWFs and a `beatBox` text beside its music, scanned
//! in place, each level a song; played headless on a fake clock, the
//! music taken out of the SWF, every note autoplayed to the top tier.
//!
//! `DDI_FFR_FIXTURES=<dir>` with `--ignored` writes the same levels to a
//! folder (the web's checks import them).

mod common;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ddi_desktop::audio::Audio;
use ddi_desktop::play::{Ended, Feed, Play, Request, load};
use ddi_platform::HostTime;
use ddi_ui::state::settings::Settings;

/// About 12 s of a quiet 441 Hz tone, MP3: the platform's fixture (a
/// bare stream, no tags) over and over.
fn tone() -> Vec<u8> {
    include_bytes!("../../platform/tests/fixtures/sine-441-bare.mp3").repeat(40)
}

/// `n` notes from 1 s, a sixth of a second apart, around the lanes.
fn notes(n: usize) -> Vec<(f64, &'static str, &'static str)> {
    (0..n)
        .map(|i| {
            (
                30.0 + 5.0 * i as f64,
                ["L", "D", "U", "R"][i % 4],
                ["red", "blue", "purple", "yellow"][i % 4],
            )
        })
        .collect()
}

/// The levels: two SWFs (one compressed) in `FFR Levels/`, and a
/// `beatBox` as a decompiler exports it beside its music in `Decompiled/`.
fn write_levels(dir: &Path) {
    let mp3 = tone();
    let ffr = dir.join("FFR Levels");
    let text = dir.join("Decompiled");
    for d in [&ffr, &text] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(
        ffr.join("level_1.swf"),
        ddi_library::swf::write_level(&notes(40), &mp3, true),
    )
    .unwrap();
    std::fs::write(
        ffr.join("level_2.swf"),
        ddi_library::swf::write_level(&notes(24), &mp3, false),
    )
    .unwrap();
    let beatbox: Vec<String> = notes(16)
        .iter()
        .map(|(f, d, c)| format!("[{f}, \"{d}\", \"{c}\"]"))
        .collect();
    std::fs::write(
        text.join("DoAction.as"),
        format!("_root.beatBox = [{}];\n", beatbox.join(", ")),
    )
    .unwrap();
    std::fs::write(text.join("DoAction.mp3"), &mp3).unwrap();
}

/// Autoplays chart 0 of library song `id` on a fake clock: its tally
/// (taps by tier, then holds let go).
fn autoplay(id: &str) -> Result<Vec<u32>, String> {
    let loaded = Rc::new(load::load(id, 48_000)?);
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    let audio = Audio::headless(1.0, Some(Arc::new(move || c.load(Ordering::SeqCst))));
    let mut p = Play::start(
        Request::song(id, 0, true),
        loaded,
        &Settings::default(),
        audio,
        Rc::new(RefCell::new(Feed::default())),
        None,
        60.0,
    )?;
    let step = 1_000_000_000 / 60;
    loop {
        let now = clock.fetch_add(step, Ordering::SeqCst) + step;
        match p.tick(HostTime(now as f64 / 1e9), None) {
            Some(Ended::Finished { results, .. }) => {
                let mut t = results.tally.taps.to_vec();
                t.push(results.tally.let_go);
                return Ok(t);
            }
            Some(_) => return Err("the play did not finish".into()),
            None => {}
        }
        if now > 1_000_000_000 * 120 {
            return Err("never ends".into());
        }
    }
}

#[test]
fn levels_in_a_folder_are_songs_that_play() {
    common::indexes();
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("ffr-levels-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    write_levels(&dir);
    let (songs, report) = ddi_desktop::library::scan_folders(std::slice::from_ref(&dir));
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    let mut titles: Vec<(String, String, usize)> = songs
        .iter()
        .map(|s| {
            (
                s.pack.clone(),
                s.entry.title.clone(),
                s.entry.charts[0].notes,
            )
        })
        .collect();
    titles.sort();
    assert_eq!(
        titles,
        vec![
            ("Decompiled".into(), "DoAction".into(), 16),
            ("FFR Levels".into(), "level_1".into(), 40),
            ("FFR Levels".into(), "level_2".into(), 24),
        ]
    );
    ddi_desktop::library::register_for_tests(&songs);
    for s in &songs {
        // The music out of the level (or beside it), decoded.
        let loaded = load::load(&s.entry.id, 48_000).unwrap();
        assert!(loaded.music.duration() > 10.0, "{}", s.entry.title);
        let t = autoplay(&s.entry.id).unwrap();
        let n = s.entry.charts[0].notes as u32;
        assert_eq!(t[0], n, "{}: {t:?}", s.entry.title);
        assert_eq!(t[1..].iter().sum::<u32>(), 0, "{}: {t:?}", s.entry.title);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "writes the levels to DDI_FFR_FIXTURES"]
fn write_levels_for_the_web() {
    let Some(dir) = std::env::var_os("DDI_FFR_FIXTURES") else {
        eprintln!("DDI_FFR_FIXTURES not set");
        return;
    };
    write_levels(Path::new(&dir));
}
