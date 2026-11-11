//! Opt-in: the CPU opponent's levels measured over local song folders
//! (`docs/plans/cpu-player.md`, "Ten levels").
//!
//! `DDI_CPU_CORPUS=<dir>[:<dir>…] cargo test --release -p ddi-engine --test
//! local_cpu -- --nocapture` reads every `.sm`, `.ssc` and `.dwi` under
//! the folders (in place, recursively), and plays every dance-single
//! chart as written (no generated versions) with an opponent at each
//! level and seed, on its tap-only copy, under the rulesets named, to the
//! song's end with the fail off (so the percent is of the whole chart;
//! "out" is its gauge emptying at any point, as a stage would end it).
//! It prints aggregates only, by difficulty slot, by meter range and by
//! notes a second: never a song's name. Third-party songs used this way
//! stay local.
//!
//! - `DDI_CPU_LEVELS=<strength>:<lapse>[,…]`: candidates instead of the
//!   ten levels (`ddi_engine::cpu::CPU_LEVELS_V0`).
//! - `DDI_CPU_SEEDS=<n>` (3), `DDI_CPU_RULES=<id>[,…]` (`itg-fa+,ddi`;
//!   `ddi-easy`, `ddi-normal` and `ddi-hard` are DDI at a difficulty, the
//!   rooms' match rule).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ddi_chart::{Difficulty, Song};
use ddi_engine::cpu::{CPU_LEVELS_V0, Level, Opponent, Plan, Skills};
use ddi_engine::rules::presets::{self, HoldStyle};
use ddi_engine::rules::{FailPolicy, Judgement};
use ddi_library::scan::parse_simfile;

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|e| {
            ["sm", "ssc", "dwi"].contains(&e.to_string_lossy().to_ascii_lowercase().as_str())
        }) {
            out.push(p);
        }
    }
}

/// One play's outcome.
#[derive(Clone, Copy, Default)]
struct Sum {
    plays: u32,
    percent: f64,
    life: f64,
    out: u32,
    missed: f64,
}

impl Sum {
    fn add(&mut self, o: &Sum) {
        self.plays += o.plays;
        self.percent += o.percent;
        self.life += o.life;
        self.out += o.out;
        self.missed += o.missed;
    }
}

/// The ruleset a rules id names: a preset's, or DDI at a difficulty
/// (`ddi-easy`, `ddi-normal`, `ddi-hard`; `ddi` is Normal), the rooms'
/// match rules.
fn ruleset_of(rules: &str, song: &Song, chart: usize) -> ddi_engine::rules::Ruleset {
    use ddi_engine::rules::ddi::{self, DdiOptions, Difficulty as D};
    let difficulty = match rules {
        "ddi-easy" => D::Easy,
        "ddi-normal" => D::Normal,
        "ddi-hard" => D::Hard,
        _ => return presets::stepmania(rules, HoldStyle::ByGame, &song.charts[chart]),
    };
    ddi::ruleset(&DdiOptions {
        difficulty,
        ..Default::default()
    })
}

fn play(plan: &Plan, song: &Song, chart: usize, rules: &str, level: Level, seed: u64) -> Sum {
    let mut ruleset = ruleset_of(rules, song, chart);
    ruleset.fail = FailPolicy::Off;
    let Ok(mut o) = Opponent::planned(
        plan,
        song,
        chart,
        ruleset,
        (level, 0.0),
        seed,
        Skills::default(),
    ) else {
        return Sum::default();
    };
    o.start(0.0);
    o.finish();
    let view = o.score();
    let percent = view
        .percent
        .or(view.money.map(|m| m as f64 / 1_000_000.0))
        .or(view.ex_percent)
        .unwrap_or(0.0);
    let tally = o.player().tally();
    let taps: u32 = tally.taps.iter().sum();
    let out = o.out_at().is_some();
    Sum {
        plays: 1,
        percent,
        life: if out { 0.0 } else { f64::from(o.life()) },
        out: u32::from(out),
        missed: f64::from(tally.taps[Judgement::Miss as usize]) / f64::from(taps.max(1)),
    }
}

fn slot(d: Difficulty) -> &'static str {
    match d {
        Difficulty::Beginner => "1 Beginner",
        Difficulty::Easy => "2 Easy",
        Difficulty::Medium => "3 Medium",
        Difficulty::Hard => "4 Hard",
        Difficulty::Challenge => "5 Challenge",
        Difficulty::Edit => "6 Edit",
    }
}

fn meter_range(m: u32) -> &'static str {
    match m {
        0..=3 => "meter 01-03",
        4..=6 => "meter 04-06",
        7..=9 => "meter 07-09",
        10..=12 => "meter 10-12",
        13..=15 => "meter 13-15",
        _ => "meter 16+",
    }
}

fn nps_range(nps: f64) -> &'static str {
    match nps {
        n if n < 1.0 => "notes/s 0-1",
        n if n < 2.0 => "notes/s 1-2",
        n if n < 3.0 => "notes/s 2-3",
        n if n < 4.5 => "notes/s 3-4.5",
        n if n < 6.0 => "notes/s 4.5-6",
        n if n < 8.0 => "notes/s 6-8",
        _ => "notes/s 8+",
    }
}

type Table = BTreeMap<(String, String), (u32, Vec<Sum>)>;

#[test]
fn local_cpu_levels() {
    let Some(dirs) = std::env::var_os("DDI_CPU_CORPUS") else {
        return;
    };
    let levels: Vec<Level> = match std::env::var("DDI_CPU_LEVELS") {
        Ok(v) => v
            .split(',')
            .map(|p| {
                let (s, l) = p.split_once(':').expect("strength:lapse");
                Level {
                    strength: s.trim().parse().expect("strength"),
                    lapse: l.trim().parse().expect("lapse"),
                }
            })
            .collect(),
        Err(_) => CPU_LEVELS_V0.levels.to_vec(),
    };
    let seeds: u64 = std::env::var("DDI_CPU_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let rules: Vec<String> = std::env::var("DDI_CPU_RULES")
        .unwrap_or_else(|_| "itg-fa+,ddi".into())
        .split(',')
        .map(str::to_string)
        .collect();
    let mut files = Vec::new();
    for dir in std::env::split_paths(&dirs) {
        collect(&dir, &mut files);
    }
    files.sort();
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    let chunk = files.len().div_ceil(workers).max(1);
    let (levels, rules) = (&levels, &rules);
    let parts: Vec<(Table, u32, u32)> = std::thread::scope(|scope| {
        let handles: Vec<_> = files
            .chunks(chunk)
            .map(|files| {
                scope.spawn(move || {
                    let mut table = Table::new();
                    let (mut unparsed, mut refused) = (0, 0);
                    for path in files {
                        let ext = path
                            .extension()
                            .map(|e| e.to_string_lossy().to_ascii_lowercase())
                            .unwrap_or_default();
                        let Ok(bytes) = std::fs::read(path) else {
                            unparsed += 1;
                            continue;
                        };
                        let text = ddi_library::pack::decode_text(&bytes);
                        let Ok(song) = parse_simfile(&text, &ext) else {
                            unparsed += 1;
                            continue;
                        };
                        for (index, chart) in song.charts.iter().enumerate() {
                            if chart.layout != "dance-single" || chart.derivation.is_some() {
                                continue;
                            }
                            let Ok(plan) = Plan::automatic(&song, index) else {
                                refused += 1;
                                continue;
                            };
                            let timing = chart.timing(&song);
                            let (mut first, mut last, mut notes) = (f64::MAX, f64::MIN, 0u32);
                            for n in &chart.notes {
                                if matches!(
                                    n.kind,
                                    ddi_chart::NoteKind::Tap
                                        | ddi_chart::NoteKind::HoldHead { .. }
                                        | ddi_chart::NoteKind::RollHead { .. }
                                ) {
                                    let t = timing.seconds_at(n.tick);
                                    first = first.min(t);
                                    last = last.max(t);
                                    notes += 1;
                                }
                            }
                            let nps = f64::from(notes) / (last - first).max(1.0);
                            let groups = [
                                "all".to_string(),
                                slot(chart.difficulty).to_string(),
                                meter_range(chart.meter).to_string(),
                                nps_range(nps).to_string(),
                            ];
                            for r in rules {
                                let sums: Vec<Sum> = levels
                                    .iter()
                                    .map(|&level| {
                                        let mut s = Sum::default();
                                        for seed in 1..=seeds {
                                            s.add(&play(&plan, &song, index, r, level, seed));
                                        }
                                        s
                                    })
                                    .collect();
                                for g in &groups {
                                    let e = table
                                        .entry((r.clone(), g.clone()))
                                        .or_insert_with(|| (0, vec![Sum::default(); levels.len()]));
                                    e.0 += 1;
                                    for (a, b) in e.1.iter_mut().zip(&sums) {
                                        a.add(b);
                                    }
                                }
                            }
                        }
                    }
                    (table, unparsed, refused)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut table = Table::new();
    let (mut unparsed, mut refused) = (0, 0);
    for (t, u, r) in parts {
        unparsed += u;
        refused += r;
        for (k, (n, sums)) in t {
            let e = table
                .entry(k)
                .or_insert_with(|| (0, vec![Sum::default(); levels.len()]));
            e.0 += n;
            for (a, b) in e.1.iter_mut().zip(&sums) {
                a.add(b);
            }
        }
    }
    println!(
        "{} files, {unparsed} not read, {refused} charts the CPU cannot plan, {seeds} seeds",
        files.len()
    );
    println!(
        "levels (strength:lapse): {}",
        levels
            .iter()
            .map(|l| format!("{}:{}", l.strength, l.lapse))
            .collect::<Vec<_>>()
            .join(" ")
    );
    for ((rules, group), (charts, sums)) in &table {
        let row = |f: &dyn Fn(&Sum) -> f64| {
            sums.iter()
                .map(|s| format!("{:5.1}", f(s)))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let n = |s: &Sum| f64::from(s.plays.max(1));
        println!("{rules} | {group} | {charts} charts");
        println!("  percent  {}", row(&|s| 100.0 * s.percent / n(s)));
        println!("  out %    {}", row(&|s| 100.0 * f64::from(s.out) / n(s)));
        println!("  life %   {}", row(&|s| 100.0 * s.life / n(s)));
        println!("  missed % {}", row(&|s| 100.0 * s.missed / n(s)));
    }
}
