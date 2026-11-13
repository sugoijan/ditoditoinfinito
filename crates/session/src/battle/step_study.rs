//! Opt-in study: the match rule "At zero life: Step down" (`step-down-v0`,
//! `ddi_engine::ladder`; `docs/plans/multiplayer-prototype-progress.md`,
//! "Step down").
//!
//! `cargo test --release -p ddi-session --lib step_down_study -- --ignored
//! --nocapture` reads the bundled songs and, with `DDI_STEP_CORPUS=<dir>[:<dir>…]`,
//! every song folder under the folders (in place; one simfile a folder,
//! `.ssc` before `.sm` before `.dwi`), and prints aggregates only, never a
//! song's name:
//!
//! 1. the ladders: by the stage chart's slot, how many dance-single
//!    charts have one and how long;
//! 2. what placing a stage's battle plan over every rung at once costs:
//!    the sections a stage chart alone has against those its ladder
//!    merged leaves (`placement-v2`);
//! 3. stages of ten CPUs, one at each level, attacks on, on every Hard and
//!    Challenge chart with a ladder (every `DDI_STEP_EVERY`th, 1), under
//!    DDI Normal and Hard, each with the rule Out and Step down, seeded
//!    (`DDI_STEP_SEEDS`, 2): by level, how often it goes out, how often it
//!    steps down and how far, and its mean place, against Out.
//!
//! MEASUREMENTS OF CPUS AGAINST CPUS, not of people.

use super::*;
use ddi_room::{Charts, MatchDifficulty};
use ddi_ui::state::room::StageRules;
use std::path::{Path, PathBuf};

fn song_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut here: Vec<PathBuf> = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            song_files(&p, out);
        } else if let Some(ext) = p.extension() {
            let ext = ext.to_string_lossy().to_ascii_lowercase();
            if ["ssc", "sm", "dwi"].contains(&ext.as_str()) {
                here.push(p);
            }
        }
    }
    let rank = |p: &PathBuf| match p
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .as_deref()
    {
        Some("ssc") => 0,
        Some("sm") => 1,
        _ => 2,
    };
    if let Some(best) = here.iter().min_by_key(|p| rank(p)) {
        out.push(best.clone());
    }
}

fn read(path: &Path) -> Option<Song> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    let bytes = std::fs::read(path).ok()?;
    let text = ddi_library::pack::decode_text(&bytes);
    ddi_library::scan::parse_simfile(&text, &ext).ok()
}

const SLOTS: [ddi_chart::Difficulty; 6] = ddi_chart::Difficulty::ALL;

#[derive(Clone, Default)]
struct Tally {
    /// By the stage chart's slot: charts, by ladder length (0 to 4+).
    ladders: [[u32; 5]; 6],
    /// By slot: the plan's sections alone and merged (charts with a
    /// ladder only).
    sections: [(u64, u64); 6],
    songs: u32,
    songs_with_ladder: u32,
    /// By difficulty (Normal, Hard), rule (Out, Step down) and level:
    /// stages, out, places summed, steps by depth (0 to 4+).
    levels: [[[LevelSum; 10]; 2]; 2],
    stages: [u32; 2],
}

#[derive(Clone, Copy, Default)]
struct LevelSum {
    stages: u32,
    out: u32,
    place: u64,
    depth: [u32; 5],
}

impl Tally {
    fn add(&mut self, o: &Tally) {
        for s in 0..6 {
            for d in 0..5 {
                self.ladders[s][d] += o.ladders[s][d];
            }
            self.sections[s].0 += o.sections[s].0;
            self.sections[s].1 += o.sections[s].1;
        }
        self.songs += o.songs;
        self.songs_with_ladder += o.songs_with_ladder;
        for d in 0..2 {
            self.stages[d] += o.stages[d];
            for r in 0..2 {
                for l in 0..10 {
                    let (a, b) = (&mut self.levels[d][r][l], &o.levels[d][r][l]);
                    a.stages += b.stages;
                    a.out += b.out;
                    a.place += b.place;
                    for k in 0..5 {
                        a.depth[k] += b.depth[k];
                    }
                }
            }
        }
    }
}

/// A stage of ten CPUs, one at each level (seated in turn by the seed),
/// on chart `chart` of `song` under `difficulty` and `charts`, attacks
/// on: each level's (steps, out, place from 1).
fn stage(
    song: &Song,
    chart: usize,
    difficulty: MatchDifficulty,
    charts: Charts,
    seed: u64,
) -> Option<Vec<(usize, usize, bool, usize)>> {
    let rules = StageRules {
        difficulty,
        charts,
        ..StageRules::default()
    };
    let ruleset = stage_ruleset(&rules);
    let turn = seed as usize % 10;
    let levels: Vec<u8> = (0..10).map(|i| ((i + turn) % 10) as u8).collect();
    let setup = StageSetup {
        stage: StageId(seed),
        number: 1,
        of: 1,
        battle: BattlePreset::GaugeAttacksV0,
        rules,
        me: ParticipantId(1),
        targeting: Targeting::Spread,
        cpus: levels
            .iter()
            .enumerate()
            .map(|(i, &level)| CpuSeat {
                participant: ParticipantId(i as u64 + 2),
                ordinal: i as u16 + 1,
                level,
                seed: seed
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    .wrapping_add(i as u64 * 7919 + 1),
                name: None,
                skills: None,
                targeting: Targeting::Spread,
            })
            .collect(),
        dev_mines_at_beat: None,
        others: 0,
        watching: false,
        live: None,
    };
    let ladder = stage_ladder(song, chart, &setup);
    let mut b = Battle::new(setup, song, chart, &ruleset, 0.0, FORFEIT_HORIZON, &ladder).ok()?;
    if let Some(a) = b.attacks.as_mut() {
        a.out_at(b.setup.me, f64::NEG_INFINITY);
    }
    b.play_out_as(Some(Run::Forfeit));
    let ranking = match &b.decided {
        Some(r) => r.clone(),
        None => {
            let checkpoints = b
                .attacks
                .as_ref()
                .map_or(&[][..], |a| a.ledger().checkpoints_us());
            SETTLEMENT
                .rank(&b.entrants_as(None), checkpoints, b.settling, b.setup.rules.rank_by())
                .0
        }
    };
    let place_of = |p: ParticipantId| {
        let mut place = 1;
        for g in &ranking {
            if g.iter().any(|f| f.participant == p) {
                return place;
            }
            place += g.len();
        }
        place
    };
    Some(
        b.seats
            .iter()
            .map(|s| {
                (
                    usize::from(s.seat.level),
                    s.opponent.player().steps().len(),
                    s.out_at().is_some(),
                    place_of(s.seat.participant),
                )
            })
            .collect(),
    )
}

/// Everything of one song.
fn study_song(song: &Song, bundled: bool, every: usize, seeds: u64, counter: &mut usize) -> Tally {
    let mut t = Tally {
        songs: 1,
        ..Tally::default()
    };
    let judge = stage_ruleset(&StageRules::default()).judge;
    let mut any = false;
    for (index, chart) in song.charts.iter().enumerate() {
        if chart.layout != "dance-single" || chart.derivation.is_some() || chart.notes.is_empty() {
            continue;
        }
        let slot = SLOTS.iter().position(|d| *d == chart.difficulty).unwrap_or(5);
        let ladder = ddi_engine::ladder::below(song, index);
        t.ladders[slot][ladder.len().min(4)] += 1;
        if ladder.is_empty() {
            continue;
        }
        any = true;
        let alone = ddi_engine::battle::schedule(song, index, &judge, 11)
            .iter()
            .filter(|s| s.is_some())
            .count() as u64;
        let merged_song = ddi_engine::ladder::merged(song, index, &ladder);
        let merged = ddi_engine::battle::schedule(&merged_song, index, &judge, 11)
            .iter()
            .filter(|s| s.is_some())
            .count() as u64;
        t.sections[slot].0 += alone;
        t.sections[slot].1 += merged;
        if !matches!(
            chart.difficulty,
            ddi_chart::Difficulty::Hard | ddi_chart::Difficulty::Challenge
        ) {
            continue;
        }
        *counter += 1;
        if !bundled && !(*counter - 1).is_multiple_of(every) {
            continue;
        }
        for (d, difficulty) in [MatchDifficulty::Normal, MatchDifficulty::Hard]
            .into_iter()
            .enumerate()
        {
            for seed in 1..=seeds {
                for (r, charts) in [Charts::AsPicked, Charts::Adaptive].into_iter().enumerate() {
                    let Some(seats) = stage(song, index, difficulty, charts, seed) else {
                        continue;
                    };
                    if r == 0 {
                        t.stages[d] += 1;
                    }
                    for (level, steps, out, place) in seats {
                        let l = &mut t.levels[d][r][level.min(9)];
                        l.stages += 1;
                        l.out += u32::from(out);
                        l.place += place as u64;
                        l.depth[steps.min(4)] += 1;
                    }
                }
            }
        }
    }
    t.songs_with_ladder = u32::from(any);
    t
}

#[test]
#[ignore = "a study: prints what Step down does over the bundled songs and a local corpus"]
fn step_down_study() {
    let every: usize = std::env::var("DDI_STEP_EVERY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
        .max(1);
    let seeds: u64 = std::env::var("DDI_STEP_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2);
    let started = std::time::Instant::now();
    let report = |name: &str, t: &Tally| {
        println!("== {name}: {} songs, {} with a ladder", t.songs, t.songs_with_ladder);
        println!("ladders by the stage chart's slot (charts; length 0, 1, 2, 3, 4+; mean)");
        for (s, d) in SLOTS.iter().enumerate() {
            let row = t.ladders[s];
            let n: u32 = row.iter().sum();
            if n == 0 {
                continue;
            }
            let mean = row
                .iter()
                .enumerate()
                .map(|(k, c)| k as f64 * f64::from(*c))
                .sum::<f64>()
                / f64::from(n);
            let (alone, merged) = t.sections[s];
            println!(
                "  {:<10} {n:>5} | {:>5} {:>5} {:>5} {:>5} {:>5} | {mean:4.2} | sections alone {alone:>6}, merged {merged:>6} ({:5.1}% kept)",
                d.name(),
                row[0],
                row[1],
                row[2],
                row[3],
                row[4],
                if alone > 0 { 100.0 * merged as f64 / alone as f64 } else { 0.0 },
            );
        }
        for (d, name) in ["Normal", "Hard"].iter().enumerate() {
            println!(
                "stages of ten CPUs (one a level), DDI {name}, attacks on: {} stages a rule",
                t.stages[d]
            );
            println!(
                "  level | Out: out%  place | Step down: out%  stepped%  depth 1/2/3/4+ %  mean steps  place"
            );
            for l in 0..10 {
                let (o, s) = (t.levels[d][0][l], t.levels[d][1][l]);
                let (on, sn) = (f64::from(o.stages.max(1)), f64::from(s.stages.max(1)));
                let stepped = s.stages - s.depth[0];
                let mean = s
                    .depth
                    .iter()
                    .enumerate()
                    .map(|(k, c)| k as f64 * f64::from(*c))
                    .sum::<f64>()
                    / sn;
                println!(
                    "  {:>5} | {:5.1} {:6.2} | {:5.1} {:8.1}  {:4.1}/{:4.1}/{:4.1}/{:4.1}  {:5.2}  {:6.2}",
                    l + 1,
                    100.0 * f64::from(o.out) / on,
                    o.place as f64 / on,
                    100.0 * f64::from(s.out) / sn,
                    100.0 * f64::from(stepped) / sn,
                    100.0 * f64::from(s.depth[1]) / sn,
                    100.0 * f64::from(s.depth[2]) / sn,
                    100.0 * f64::from(s.depth[3]) / sn,
                    100.0 * f64::from(s.depth[4]) / sn,
                    mean,
                    s.place as f64 / sn,
                );
            }
        }
        println!("  ({:.0} s so far)", started.elapsed().as_secs_f64());
    };
    // The bundled songs, every chart.
    let mut bundled_tally = Tally::default();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs");
    let mut files = Vec::new();
    song_files(&dir, &mut files);
    let mut counter = 0;
    for f in &files {
        if let Some(song) = read(f) {
            bundled_tally.add(&study_song(&song, true, every, seeds, &mut counter));
        }
    }
    report("the bundled songs", &bundled_tally);
    // A local corpus, in place: aggregates only.
    let Some(dirs) = std::env::var_os("DDI_STEP_CORPUS") else {
        return;
    };
    let mut files = Vec::new();
    for dir in std::env::split_paths(&dirs) {
        song_files(&dir, &mut files);
    }
    files.sort();
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    let chunk = files.len().div_ceil(workers).max(1);
    let parts: Vec<(Tally, u32)> = std::thread::scope(|scope| {
        let handles: Vec<_> = files
            .chunks(chunk)
            .map(|files| {
                scope.spawn(move || {
                    let mut t = Tally::default();
                    let mut unread = 0;
                    let mut counter = 0;
                    for f in files {
                        match read(f) {
                            Some(song) => t.add(&study_song(&song, false, every, seeds, &mut counter)),
                            None => unread += 1,
                        }
                    }
                    (t, unread)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut total = Tally::default();
    let mut unread = 0;
    for (t, u) in &parts {
        total.add(t);
        unread += u;
    }
    println!("({} song files, {unread} unread)", files.len());
    report("the local corpus", &total);
}

/// What a stage of 72 CPUs pays for a ladder when it loads: the ladder
/// planned once (`planned_ladder`), each CPU's script for every rung, the
/// plan placed over every rung; against the same stage under Out. Then a
/// stage played out (judging every rung a CPU reaches). Prints times.
#[test]
#[ignore = "a study: prints what a ladder costs a stage of 72 CPUs"]
fn step_down_cost_of_72_cpus() {
    let song = super::study::bundled_song("beatucada");
    let chart = 5;
    for charts in [Charts::AsPicked, Charts::Adaptive] {
        let rules = StageRules {
            charts,
            ..StageRules::default()
        };
        let ruleset = stage_ruleset(&rules);
        let setup = StageSetup {
            stage: StageId(3),
            number: 1,
            of: 1,
            battle: BattlePreset::GaugeAttacksV0,
            rules,
            me: ParticipantId(1),
            targeting: Targeting::Spread,
            cpus: (0..72u64)
                .map(|i| CpuSeat {
                    participant: ParticipantId(i + 2),
                    ordinal: i as u16 + 1,
                    level: (i % 10) as u8,
                    seed: i * 7919 + 1,
                    name: None,
                    skills: None,
                    targeting: Targeting::Spread,
                })
                .collect(),
            dev_mines_at_beat: None,
            others: 0,
            watching: false,
            live: None,
        };
        let t = std::time::Instant::now();
        let ladder = stage_ladder(&song, chart, &setup);
        let planned = t.elapsed().as_secs_f64();
        let mut b = Battle::new(setup, &song, chart, &ruleset, 0.0, FORFEIT_HORIZON, &ladder).unwrap();
        let loaded = t.elapsed().as_secs_f64();
        if let Some(a) = b.attacks.as_mut() {
            a.out_at(b.setup.me, f64::NEG_INFINITY);
        }
        b.play_out_as(Some(Run::Forfeit));
        let played = t.elapsed().as_secs_f64();
        let stepped = b.seats.iter().filter(|s| !s.opponent.player().steps().is_empty()).count();
        println!(
            "{charts:?}: {} rungs; ladder planned {:.0} ms, stage loaded {:.0} ms, played out {:.0} ms; {stepped} of 72 stepped down",
            ladder.len(),
            planned * 1e3,
            loaded * 1e3,
            played * 1e3,
        );
    }
}
