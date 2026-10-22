//! Opt-in: the autoplayer through every chart of a local song folder.
//!
//! `DDI_AUTOPLAY_CORPUS=<dir>[:<dir>…] cargo test --release -p ddi-engine
//! --test local_autoplay -- --nocapture` imports every `.sm`, `.ssc` and
//! `.dwi` under the folders (recursively) and plays each chart's script
//! ([`ddi_engine::autoplay::script`], on time) under ITG, StepMania 5,
//! DDR A, DDR X and FFR, holds ticked on pump charts as the game's
//! default does. Each play must end with every step at the top
//! judgement, 100 %, the full EX score, DDR A's and DDR X's 1,000,000
//! and FFR's AAA (its raw score 50 a
//! note, no Boo): whatever is left over (a mine under a hold, a
//! note the judge never reaches, a roll the taps cannot keep) is a bug in
//! the importer, the judge or the script. Each play is also judged again
//! from its input log ([`ddi_engine::history::replay`]) to the same
//! results. `DDI_AUTOPLAY_FILTER=<text>`
//! plays only files whose path contains it; `DDI_AUTOPLAY_VERBOSE=1`
//! prints every judgement short of the top one, with the lane's notes. Third-party songs used this
//! way stay local.

use std::path::{Path, PathBuf};

use ddi_chart::Song;
use ddi_engine::InputEvent;
use ddi_engine::autoplay;
use ddi_engine::judge::JudgeEventKind;
use ddi_engine::player::{PlayOptions, Player, Results};
use ddi_engine::rules::danoni::DanoniOptions;
use ddi_engine::rules::ffr::Flag;
use ddi_engine::rules::presets::{self, HoldStyle, RulesetMode};
use ddi_engine::rules::{FullCombo, Judgement};
use ddi_library::scan::parse_simfile;
use ddi_platform::{ClockSample, HostTime};

/// The shell's frame, 60 Hz.
const FRAME: f64 = 1.0 / 60.0;

const PRESETS: [&str; 5] = ["itg", "sm5", "ddr-a", "ddr-x", "ffr"];

#[test]
fn local_autoplay() {
    let Some(dirs) = std::env::var_os("DDI_AUTOPLAY_CORPUS") else {
        return;
    };
    let filter = std::env::var("DDI_AUTOPLAY_FILTER").unwrap_or_default();
    let mut files = Vec::new();
    for dir in std::env::split_paths(&dirs) {
        collect(&dir, &mut files);
    }
    files.retain(|f| f.to_string_lossy().contains(&filter));
    files.sort();
    let (mut charts, mut plays, mut unparsed, mut shocks) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    for path in &files {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let bytes = std::fs::read(path).unwrap();
        let text = ddi_library::pack::decode_text(&bytes);
        let song = match parse_simfile(&text, &ext) {
            Ok(song) => song,
            Err(e) => {
                unparsed += 1;
                eprintln!("{}: not imported: {e}", path.display());
                continue;
            }
        };
        for (index, chart) in song.charts.iter().enumerate() {
            charts += 1;
            for preset in PRESETS {
                plays += 1;
                let played = std::panic::catch_unwind(|| play(&song, index, preset));
                let problems = match played {
                    Ok(Some((r, n))) => {
                        assert_eq!(r.assist, chart.derivation.is_some());
                        shocks += n;
                        check(&r)
                    }
                    Ok(None) => vec!["did not finish".to_string()],
                    Err(_) => vec!["panicked".to_string()],
                };
                if !problems.is_empty() {
                    failures.push(format!(
                        "{} chart {index} ({} {:?} {}) {preset}: {}",
                        path.display(),
                        chart.layout,
                        chart.difficulty,
                        chart.meter,
                        problems.join("; ")
                    ));
                }
            }
        }
    }
    eprintln!(
        "{} files ({unparsed} not imported), {charts} charts, {plays} plays ({shocks} shocks), {} failed",
        files.len(),
        failures.len()
    );
    for f in &failures {
        eprintln!("{f}");
    }
    assert!(failures.is_empty(), "{} plays fell short", failures.len());
}

/// One autoplayed play of `song`'s chart `index` and the shocks it had;
/// `None` when it did not finish.
fn play(song: &Song, index: usize, preset: &str) -> Option<(Results, u32)> {
    let ruleset = presets::for_chart(
        RulesetMode::AllStepMania,
        preset,
        &DanoniOptions::default(),
        HoldStyle::ByGame,
        &song.charts[index],
    );
    let mut player = Player::new(song, index, ruleset, PlayOptions::default());
    // Presses and releases in time order, a release before a press at
    // the same time (a roll's or a jack's next tap).
    let mut events: Vec<(f64, bool, u8)> = autoplay::script(player.judge().notes(), 0.0)
        .iter()
        .flat_map(|p| [(p.press, true, p.lane), (p.release, false, p.lane)])
        .collect();
    events.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    player.clock_sample(ClockSample {
        context_time: 0.0,
        host_time: HostTime(0.0),
        output_latency: 0.0,
    });
    player.start(0.0, 0.0);
    let end = player.song_end_time().max(0.0) + 10.0;
    let mut next = 0;
    let mut frame = 0u32;
    let verbose = std::env::var_os("DDI_AUTOPLAY_VERBOSE").is_some();
    while !player.finished() {
        let now = f64::from(frame) * FRAME;
        if now > end {
            return None;
        }
        let mut judged = Vec::new();
        while next < events.len() && events[next].0 <= now {
            let (t, pressed, lane) = events[next];
            judged.extend(player.input(InputEvent {
                lane,
                pressed,
                host_time: HostTime(t),
            }));
            next += 1;
        }
        judged.extend(player.update(HostTime(now)));
        if verbose {
            for e in judged.iter().filter(|e| {
                matches!(
                    e.kind,
                    JudgeEventKind::HitMine | JudgeEventKind::LetGo | JudgeEventKind::Boo
                ) || matches!(e.kind, JudgeEventKind::Checkpoint { missed, .. } if missed > 0)
                    || matches!(e.kind, JudgeEventKind::Tap(j, _) if j != Judgement::W1)
            }) {
                let lane: Vec<_> = player
                    .judge()
                    .notes()
                    .iter()
                    .filter(|n| {
                        n.note.lane == e.lane
                            && n.seconds < e.song_time + 0.5
                            && n.end_seconds > e.song_time - 0.5
                    })
                    .map(|n| (n.seconds, n.end_seconds, n.note.kind, n.judgeable))
                    .collect();
                eprintln!(
                    "  {preset} {:.4} lane {} beat {:.4} {:?}; lane's notes {lane:?}",
                    e.song_time,
                    e.lane,
                    e.tick.beat(),
                    e.kind
                );
            }
        }
        frame += 1;
    }
    // Judged again from its log, the same results
    // (`docs/plans/play-history.md`).
    let results = player.results();
    let again = ddi_engine::history::replay(
        song,
        index,
        &player.ruleset().key(),
        player.options(),
        player.log(),
    );
    assert_eq!(again, results, "{preset}: the replay differs");
    Some((results, player.score_ctx().shocks))
}

/// What falls short of a perfect play.
fn check(r: &Results) -> Vec<String> {
    let mut out = Vec::new();
    let t = &r.tally;
    if r.failed {
        out.push("failed".into());
    }
    if r.full_combo != FullCombo::MarvelousFC {
        out.push(format!("{:?}", r.full_combo));
    }
    if t.let_go > 0 || t.mine_hit > 0 || t.checkpoint_miss > 0 {
        out.push(format!(
            "let go {}, mines hit {}, checkpoints missed {}",
            t.let_go, t.mine_hit, t.checkpoint_miss
        ));
    }
    if let Some(p) = r.score.percent
        && p < 1.0 - 1e-12
    {
        out.push(format!("{:.4} %", p * 100.0));
    }
    if r.score.ex != r.score.max_ex {
        out.push(format!("EX {:?} of {:?}", r.score.ex, r.score.max_ex));
    }
    match &r.ffr {
        Some(ffr) => {
            if ffr.flag != Flag::Aaa || t.boo > 0 {
                out.push(format!("{:?}, {} stray presses", ffr.flag, t.boo));
            }
            if r.score.money != Some(50 * u64::from(t.total_taps())) {
                out.push(format!("raw score {:?}", r.score.money));
            }
        }
        None => {
            if let Some(m) = r.score.money
                && m != 1_000_000
            {
                out.push(format!("{m} points"));
            }
        }
    }
    out
}

/// Simfiles under `dir`, recursively.
fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| {
            ["sm", "ssc", "dwi"]
                .iter()
                .any(|x| e.eq_ignore_ascii_case(x))
        }) {
            out.push(path);
        }
    }
}
