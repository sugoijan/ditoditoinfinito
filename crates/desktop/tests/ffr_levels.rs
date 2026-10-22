//! FFR levels in the library on disk (`docs/plans/phase-10.md`, step 2):
//! a folder of level SWFs and a `beatBox` text beside its music, scanned
//! in place, each level a song; played headless on a fake clock, the
//! music taken out of the SWF, every note autoplayed to the top tier,
//! under the ruleset chosen and, each chart left to its game's rules,
//! under FFR's (`docs/plans/rulesets.md`, "FFR"); each note judged where
//! R^3 judges it in the music (half a frame into its frame, as many frames
//! earlier as the SWF's sound stream starts after its frame 2), and each
//! play's log replayed to the same results.
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
use ddi_engine::judge::JudgeEventKind;
use ddi_engine::player::Results;
use ddi_engine::rules::ffr::Flag;
use ddi_engine::rules::presets::RulesetMode;
use ddi_platform::HostTime;
use ddi_platform::history::{MemoryPlayStore, PlayStore};
use ddi_ui::state::history::{PlayLog, PlayRecord, Plays};
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

/// Each level's title, notes and R^3's `mp3Frame` for it: the game frame
/// its music starts at (a SWF's stream's first frame less 2; a text
/// level's music from the game's start).
const LEVELS: [(&str, usize, i64); 3] =
    [("DoAction", 16, 0), ("level_1", 40, -2), ("level_2", 24, 5)];

/// The levels: two SWFs in `FFR Levels/` (one compressed, its sound
/// stream from its first frame; one from its frame 7), and a `beatBox` as
/// a decompiler exports it beside its music in `Decompiled/`.
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
        ddi_library::swf::write_level_at(&notes(24), &mp3, false, 7),
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

/// Autoplays chart 0 of library song `id` on a fake clock under
/// `settings`: its results, checked against its kept log replayed.
fn autoplay(id: &str, settings: &Settings) -> Result<Results, String> {
    let loaded = Rc::new(load::load(id, 48_000)?);
    let clock = Arc::new(AtomicU64::new(1_000_000_000));
    let c = clock.clone();
    let audio = Audio::headless(1.0, Some(Arc::new(move || c.load(Ordering::SeqCst))));
    let store = Rc::new(MemoryPlayStore::default());
    let plays = Plays::new(store.clone());
    let mut p = Play::start(
        Request::song(id, 0, true),
        loaded.clone(),
        settings,
        audio,
        Rc::new(RefCell::new(Feed::default())),
        None,
        60.0,
    )?;
    let step = 1_000_000_000 / 60;
    let results = loop {
        let now = clock.fetch_add(step, Ordering::SeqCst) + step;
        match p.tick(HostTime(now as f64 / 1e9), Some(&plays)) {
            Some(Ended::Finished { results, .. }) => break *results,
            Some(_) => return Err("the play did not finish".into()),
            None => {}
        }
        if now > 1_000_000_000 * 120 {
            return Err("never ends".into());
        }
    };
    // The play kept, its log replayed under its record: the same results.
    let kept = Rc::new(RefCell::new(Vec::new()));
    let k = kept.clone();
    store.export(
        16,
        Box::new(move |entries| k.borrow_mut().extend(entries)),
        Box::new(|r| assert_eq!(r, Ok(1))),
    );
    let entry = kept.borrow_mut().pop().ok_or("the play was not kept")?;
    let record: PlayRecord = serde_json::from_str(&entry.record).map_err(|e| e.to_string())?;
    let log = PlayLog::decode(&entry.log).map_err(|e| format!("{e:?}"))?;
    assert_eq!(&log, p.session.player.log());
    let again =
        ddi_ui::state::history::replay(&loaded.song, 0, &record.ruleset, &record.options, &log);
    assert_eq!(again.tally, results.tally, "{id}");
    assert_eq!(again.score, results.score, "{id}");
    assert_eq!(again.ffr, results.ffr, "{id}");
    assert_eq!(again.timeline, results.timeline, "{id}");
    Ok(results)
}

/// Where R^3 judges note frame `frame` of a level whose music starts at
/// game frame `music_frame`: seconds into the music.
fn r3_seconds(frame: f64, music_frame: i64) -> f64 {
    (frame + 0.5 - music_frame as f64) / 30.0
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
    let original = Settings {
        ruleset_mode: RulesetMode::Original,
        ..Settings::default()
    };
    for s in &songs {
        let title = s.entry.title.as_str();
        let &(_, count, music_frame) = LEVELS.iter().find(|l| l.0 == title).unwrap();
        // The music out of the level (or beside it), decoded.
        let loaded = load::load(&s.entry.id, 48_000).unwrap();
        assert!(loaded.music.duration() > 10.0, "{title}");
        // Each note where R^3 has it in the music.
        let want: Vec<f64> = notes(count)
            .iter()
            .map(|n| r3_seconds(n.0, music_frame))
            .collect();
        let p = ddi_engine::player::Player::new(
            &loaded.song,
            0,
            ddi_engine::rules::presets::ffr(),
            ddi_engine::player::PlayOptions::default(),
        );
        let judged: Vec<f64> = p.judge().notes().iter().map(|n| n.seconds).collect();
        assert_eq!(judged.len(), want.len(), "{title}");
        for (t, w) in judged.iter().zip(&want) {
            assert!((t - w).abs() < 1e-9, "{title}: {t} against R^3's {w}");
        }
        let r = autoplay(&s.entry.id, &Settings::default()).unwrap();
        let t = r.tally;
        let n = s.entry.charts[0].notes as u32;
        assert_eq!(t.taps[0], n, "{}: {t:?}", s.entry.title);
        assert_eq!(
            t.taps[1..].iter().sum::<u32>() + t.let_go,
            0,
            "{}",
            s.entry.title
        );
        assert_eq!(r.ffr, None);
        // By the level's own game's rules: FFR's, AAA, every press at the
        // time R^3 judges its note, on the music's clock.
        let r = autoplay(&s.entry.id, &original).unwrap();
        let mut taps: Vec<f64> = r
            .timeline
            .iter()
            .filter(|j| matches!(j.kind, JudgeEventKind::Tap(..)))
            .map(|j| f64::from(j.time))
            .collect();
        taps.sort_by(f64::total_cmp);
        assert_eq!(taps.len(), want.len(), "{title}");
        for (t, w) in taps.iter().zip(&want) {
            assert!((t - w).abs() < 1e-4, "{title}: pressed {t}, R^3's {w}");
        }
        let ffr = r.ffr.expect("played by FFR's rules");
        assert_eq!(ffr.flag, Flag::Aaa, "{}", s.entry.title);
        assert_eq!(r.tally.taps[0], n);
        assert_eq!(r.score.money, Some(50 * u64::from(n)));
        assert_eq!(ffr.total, 1550 * u64::from(n));
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
