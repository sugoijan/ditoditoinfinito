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
//! - The ten levels opponents play at (`ddi_engine::cpu::CPU_LEVELS`,
//!   `cpu-levels-v1`: abilities the same on every chart);
//!   `DDI_CPU_SCALE=v0` the earlier ones (`CPU_LEVELS_V0`, strength
//!   relative to the chart).
//! - `DDI_CPU_LEVELS=<strength>:<lapse>[,…]`: v0 candidates instead;
//!   `DDI_CPU_ABILITIES=<spread ms>:<notes a second>:<floor>[,…]`: v1
//!   candidates instead.
//! - `DDI_CPU_SAMPLE=<n>`: every n-th simfile alone (a quicker look).
//! - `DDI_CPU_PLAY=v0|v1`: what the CPU plays at an absolute level: the
//!   chart's tap-only copy (`cpu-play-v0`) or the chart itself
//!   (`cpu-play-v1`; `ddi_engine::cpu::CPU_PLAY` by default); the holds
//!   kept and let go and the chart's mines stepped on are counted.
//! - `DDI_CPU_SEEDS=<n>` (3), `DDI_CPU_RULES=<id>[,…]` (`itg-fa+,ddi`;
//!   `ddi-easy`, `ddi-normal` and `ddi-hard` are DDI at a difficulty, the
//!   rooms' match rule).
//!
//! `DDI_CPU_SHOCKS=1` with the corpus runs `local_cpu_shocks` instead (the
//! DDI ruleset's round 4: Evasion, the eighth skill axis): every chart
//! given up to four attacks of eight shocks where `placement-v2` puts
//! them (`ddi_engine::battle::schedule`, sections at least 16 beats
//! apart), under DDI Normal with the fail off, each level at Evasion 0.5,
//! 1 and 2 (`MINE_RESPONSE_V1`): the shocks stepped on, the notes missed
//! in a lane a shock locked, the life those cost an attack (5 a shock, 10
//! a note), the share of plays whose gauge empties while one of the
//! attacks passes, and the attacks a play was given.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ddi_chart::{Difficulty, Song};
use ddi_engine::cpu::{
    Ability, CPU_LEVELS_V0, CPU_LEVELS_V1, CPU_LEVELS_V2, CPU_PLAY, CpuPlay, Level, LevelScale, MINE_RESPONSE_V0,
    Opponent, Plan, Skills, Strength,
};
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
    /// Charts, and the sum of each chart's standard deviation of the
    /// percent across its seeds.
    charts: u32,
    spread: f64,
    /// Holds and rolls kept and let go, the chart's mines stepped on and
    /// passed.
    held: u32,
    let_go: u32,
    mines_hit: u32,
    mines: u32,
    /// DDI's score shown (the sum) and the best the chart gives (every
    /// note W0, every hold kept), thousands.
    score: f64,
    best: f64,
    /// DDI's Percent (`ddi-percent-v1`, the combo in it), and the breaks
    /// (a W4, a W5, a Miss or a mine stepped on) a note.
    ddi_percent: f64,
    breaks: f64,
}

impl Sum {
    fn add(&mut self, o: &Sum) {
        self.plays += o.plays;
        self.percent += o.percent;
        self.life += o.life;
        self.out += o.out;
        self.missed += o.missed;
        self.charts += o.charts;
        self.spread += o.spread;
        self.held += o.held;
        self.let_go += o.let_go;
        self.mines_hit += o.mines_hit;
        self.mines += o.mines;
        self.score += o.score;
        self.best += o.best;
        self.ddi_percent += o.ddi_percent;
        self.breaks += o.breaks;
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

/// What the CPU plays at an absolute level (`DDI_CPU_PLAY`).
fn cpu_play() -> CpuPlay {
    match std::env::var("DDI_CPU_PLAY").as_deref() {
        Ok("v0") => CpuPlay::TapsV0,
        Ok("v1") => CpuPlay::FullV1,
        _ => CPU_PLAY,
    }
}

#[allow(clippy::too_many_arguments)]
fn play(
    plan: &Plan,
    song: &Song,
    chart: usize,
    rules: &str,
    (index, level): (usize, Strength),
    seed: u64,
) -> Sum {
    let mut ruleset = ruleset_of(rules, song, chart);
    ruleset.fail = FailPolicy::Off;
    let how = match level {
        Strength::Absolute(_) => cpu_play(),
        Strength::Relative(_) => CpuPlay::TapsV0,
    };
    let avoidance = MINE_RESPONSE_V0.avoidance(index);
    let Ok(mut o) = Opponent::planned_as(
        plan,
        song,
        chart,
        ruleset,
        (level, avoidance),
        seed,
        Skills::default(),
        how,
    ) else {
        return Sum::default();
    };
    o.start(0.0);
    o.finish();
    let view = o.score();
    // DDI's accuracy, not its percent (`ddi-percent-v1` counts the
    // combo): what the levels are tuned by.
    let percent = view
        .accuracy()
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
        ddi_percent: view.ddi.map_or(0.0, |d| d.percent as f64 / 10_000.0),
        breaks: f64::from(
            tally.taps[Judgement::W4 as usize]
                + tally.taps[Judgement::W5 as usize]
                + tally.taps[Judgement::Miss as usize]
                + tally.mine_hit,
        ) / f64::from(taps.max(1)),
        held: tally.held,
        let_go: tally.let_go,
        mines_hit: tally.mine_hit,
        mines: o.player().judge().mine_count(),
        score: view.ddi.map_or(0.0, |d| d.score as f64 / 1e3),
        best: view.ddi.map_or(0.0, |d| {
            let notes = o
                .player()
                .judge()
                .notes()
                .iter()
                .filter(|n| n.judgeable && n.note.kind.is_tap_like())
                .count() as f64;
            (1e6 + 100.0 * d.most as f64 + notes) / 1e3
        }),
        ..Sum::default()
    }
}

/// Seed `seed` of chart `index` of `song`: the chart's own, so that over
/// the corpus a level's CPUs have as many leans (`cpu-levels-v2`, drawn
/// from the seed) as plays.
fn chart_seed(song: &Song, index: usize, seed: u64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{:?}", song.chart_digest(index)).hash(&mut h);
    (h.finish() ^ seed).wrapping_mul(0x9E37_79B9_7F4A_7C15)
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
    if std::env::var("DDI_CPU_ORDER").is_ok_and(|v| v == "1") {
        return;
    }
    let levels = strengths();
    let seeds: u64 = std::env::var("DDI_CPU_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let rules: Vec<String> = std::env::var("DDI_CPU_RULES")
        .unwrap_or_else(|_| "itg-fa+,ddi".into())
        .split(',')
        .map(str::to_string)
        .collect();
    let (files, table, unparsed, refused) = measure(&dirs, &levels, seeds, &rules);
    println!(
        "{} files, {unparsed} not read, {refused} charts the CPU cannot plan, {seeds} seeds, {}",
        files.len(),
        cpu_play().id()
    );
    println!(
        "levels: {}",
        levels
            .iter()
            .map(|l| match l {
                Strength::Relative(l) => format!("{}:{}", l.strength, l.lapse),
                Strength::Absolute(a) => format!("{}:{}:{}", a.spread_ms, a.speed, a.floor),
            })
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
        println!("  Percent  {}", row(&|s| 100.0 * s.ddi_percent / n(s)));
        println!("  break %  {}", row(&|s| 100.0 * s.breaks / n(s)));
        println!("  out %    {}", row(&|s| 100.0 * f64::from(s.out) / n(s)));
        println!("  life %   {}", row(&|s| 100.0 * s.life / n(s)));
        println!("  missed % {}", row(&|s| 100.0 * s.missed / n(s)));
        println!(
            "  seed sd  {}",
            row(&|s| 100.0 * s.spread / f64::from(s.charts.max(1)))
        );
        println!(
            "  let go % {}",
            row(&|s| 100.0 * f64::from(s.let_go) / f64::from((s.held + s.let_go).max(1)))
        );
        println!(
            "  mines %  {}",
            row(&|s| 100.0 * f64::from(s.mines_hit) / f64::from(s.mines.max(1)))
        );
        println!(
            "  holds/pl {}",
            row(&|s| f64::from(s.held + s.let_go) / n(s))
        );
        println!("  score k  {}", row(&|s| s.score / n(s)));
        println!("  best k   {}", row(&|s| s.best / n(s)));
    }
}

/// The ordering test (round V, `cpu-levels-v2`): `DDI_CPU_ORDER=1` with
/// `DDI_CPU_CORPUS` plays every dance-single chart under the folders at
/// each of the levels opponents play at (`DDI_CPU_SEEDS` seeds a chart,
/// each chart its own, under DDI Normal, the fail off) and asserts that
/// in every group of 30 charts or more (all, each difficulty slot, each
/// meter range, each band of notes a second, by holds) each level is at
/// least as good as the one below on what matters: the accuracy and the
/// Percent of the whole chart (within 0.3 points: seeds' noise), the
/// breaks a note (within 0.2 a hundred) and how often it goes out
/// (within half a point).
#[test]
fn local_cpu_order() {
    let (Some(dirs), true) = (
        std::env::var_os("DDI_CPU_CORPUS"),
        std::env::var("DDI_CPU_ORDER").is_ok_and(|v| v == "1"),
    ) else {
        return;
    };
    let levels = strengths();
    let seeds: u64 = std::env::var("DDI_CPU_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    let (files, table, _, _) = measure(&dirs, &levels, seeds, &["ddi-normal".to_string()]);
    let mut wrong = Vec::new();
    let mut groups = 0;
    for ((_, group), (charts, sums)) in &table {
        if *charts < 30 {
            continue;
        }
        groups += 1;
        let n = |s: &Sum| f64::from(s.plays.max(1));
        for (k, w) in sums.windows(2).enumerate() {
            let (lo, hi) = (&w[0], &w[1]);
            let checks = [
                ("accuracy", hi.percent / n(hi) + 0.003 >= lo.percent / n(lo)),
                ("Percent", hi.ddi_percent / n(hi) + 0.003 >= lo.ddi_percent / n(lo)),
                ("breaks", hi.breaks / n(hi) <= lo.breaks / n(lo) + 0.002),
                ("out", f64::from(hi.out) / n(hi) <= f64::from(lo.out) / n(lo) + 0.005),
            ];
            for (what, ok) in checks {
                if !ok {
                    wrong.push(format!("{group} ({charts} charts): level {} below level {} on {what}", k + 2, k + 1));
                }
            }
        }
    }
    println!(
        "{} files, {groups} groups of 30 charts or more, {seeds} seeds a chart: {} out of order",
        files.len(),
        wrong.len()
    );
    assert!(groups > 0, "no group to compare");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Every dance-single chart as written under `dirs` played at each of
/// `levels` and seeds under each of `rules`, by group: the files read,
/// the table, the files not read and the charts the CPU cannot plan.
fn measure(
    dirs: &std::ffi::OsStr,
    levels: &[Strength],
    seeds: u64,
    rules: &[String],
) -> (Vec<PathBuf>, Table, u32, u32) {
    let mut files = Vec::new();
    for dir in std::env::split_paths(&dirs) {
        collect(&dir, &mut files);
    }
    files.sort();
    sample(&mut files);
    let workers: usize = std::env::var("DDI_CPU_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    let chunk = files.len().div_ceil(workers.max(1)).max(1);
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
                            let holds = chart.notes.iter().filter(|n| n.kind.is_long()).count();
                            let groups = [
                                "all".to_string(),
                                slot(chart.difficulty).to_string(),
                                meter_range(chart.meter).to_string(),
                                nps_range(nps).to_string(),
                                match holds {
                                    0 => "holds 0",
                                    1..=29 => "holds 1-29",
                                    _ => "holds 30+",
                                }
                                .to_string(),
                            ];
                            for r in rules {
                                let sums: Vec<Sum> = levels
                                    .iter()
                                    .enumerate()
                                    .map(|(k, &level)| {
                                        let mut s = Sum::default();
                                        let mut each = Vec::new();
                                        for seed in 1..=seeds {
                                            let one = play(
                                                &plan,
                                                &song,
                                                index,
                                                r,
                                                (k, level),
                                                chart_seed(&song, index, seed),
                                            );
                                            each.push(one.percent);
                                            s.add(&one);
                                        }
                                        // The chart's spread across its seeds.
                                        let n = each.len() as f64;
                                        let mean = each.iter().sum::<f64>() / n;
                                        let var = each.iter().map(|p| (p - mean).powi(2)).sum::<f64>()
                                            / (n - 1.0).max(1.0);
                                        s.charts = 1;
                                        s.spread = var.sqrt();
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
    (files, table, unparsed, refused)
}

/// The levels measured: the opponents' ten, the earlier ones
/// (`DDI_CPU_SCALE=v0`), or candidates (`DDI_CPU_LEVELS`,
/// `DDI_CPU_ABILITIES`).
fn strengths() -> Vec<Strength> {
    if let Ok(v) = std::env::var("DDI_CPU_ABILITIES") {
        return v
            .split(',')
            .map(|p| {
                let f: Vec<f64> = p.split(':').map(|x| x.trim().parse().expect("number")).collect();
                let floor = f.get(2).copied().unwrap_or(0.0);
                Strength::Absolute(match (f.get(3), f.get(4)) {
                    // Five values: a `cpu-levels-v2` candidate (its lean
                    // and drift too); three: a `cpu-levels-v1` one.
                    (Some(&lean_ms), Some(&drift_ms)) => Ability {
                        spread_ms: f[0],
                        speed: f[1],
                        floor,
                        lean_ms,
                        drift_ms,
                        model: &ddi_engine::cpu::ABSOLUTE_V2,
                    },
                    _ => Ability::v1(f[0], f[1], floor),
                })
            })
            .collect();
    }
    if let Ok(v) = std::env::var("DDI_CPU_LEVELS") {
        return v
            .split(',')
            .map(|p| {
                let (s, l) = p.split_once(':').expect("strength:lapse");
                Strength::Relative(Level {
                    strength: s.trim().parse().expect("strength"),
                    lapse: l.trim().parse().expect("lapse"),
                })
            })
            .collect();
    }
    match std::env::var("DDI_CPU_SCALE").as_deref() {
        Ok("v0") => CPU_LEVELS_V0.levels.map(Strength::Relative).to_vec(),
        Ok("v1") => CPU_LEVELS_V1.levels.map(Strength::Absolute).to_vec(),
        _ => CPU_LEVELS_V2.levels.map(Strength::Absolute).to_vec(),
    }
}

/// Every n-th file alone (`DDI_CPU_SAMPLE`).
fn sample(files: &mut Vec<PathBuf>) {
    if let Some(n) = std::env::var("DDI_CPU_SAMPLE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 1)
    {
        *files = files.iter().step_by(n).cloned().collect();
    }
}

/// One chart's attacks against one CPU.
#[derive(Clone, Copy, Default)]
struct Shocked {
    plays: u32,
    attacks: u32,
    shocks: u32,
    stepped: u32,
    locked_missed: u32,
    knocked_out: u32,
}

impl Shocked {
    fn add(&mut self, o: &Shocked) {
        self.plays += o.plays;
        self.attacks += o.attacks;
        self.shocks += o.shocks;
        self.stepped += o.stepped;
        self.locked_missed += o.locked_missed;
        self.knocked_out += o.knocked_out;
    }
}

const EVASIONS: [f64; 3] = [0.5, 1.0, 2.0];

/// The scale the shocks' study runs (`DDI_CPU_SCALE=v0`: the earlier one).
fn scale() -> LevelScale {
    match std::env::var("DDI_CPU_SCALE").as_deref() {
        Ok("v0") => LevelScale::RelativeV0,
        Ok("v1") => LevelScale::AbsoluteV1,
        _ => ddi_engine::cpu::CPU_LEVELS,
    }
}

fn shocked(
    plan: &Plan,
    song: &Song,
    chart: usize,
    sections: &[ddi_engine::battle::Section],
    level: usize,
    evasion: f64,
    seed: u64,
) -> Shocked {
    use ddi_engine::battle::HazardState;
    use ddi_engine::judge::NoteState;
    let mut ruleset = ruleset_of("ddi-normal", song, chart);
    ruleset.fail = FailPolicy::Off;
    let table = ruleset.judge.clone();
    let skills = Skills::default().with_evasion(evasion).unwrap();
    let Ok(mut o) = Opponent::planned(
        plan,
        song,
        chart,
        ruleset,
        (
            scale().strength(level).expect("level"),
            MINE_RESPONSE_V0.avoidance(level),
        ),
        seed,
        skills,
    ) else {
        return Shocked::default();
    };
    o.start(0.0);
    for s in sections {
        if o.receive_hazards(&s.cells).is_err() {
            return Shocked::default();
        }
    }
    o.finish();
    let p = o.player();
    let (lock, window, reach) = (
        table.lock.unwrap_or(0.0),
        table.mine_window_seconds(),
        table.miss_after(),
    );
    let Some(h) = p.hazards() else {
        return Shocked::default();
    };
    let hits: Vec<(u8, f64)> = h
        .mines()
        .iter()
        .filter(|m| m.state == HazardState::Hit)
        .map(|m| (m.lane, m.seconds))
        .collect();
    let locked_missed = p
        .judge()
        .notes()
        .iter()
        .filter(|n| n.state == NoteState::Missed)
        .filter(|n| {
            hits.iter().any(|&(lane, at)| {
                lane == n.note.lane && n.seconds > at - reach && n.seconds < at + lock + reach
            })
        })
        .count() as u32;
    let timing = song.charts[chart].timing(song);
    let knocked_out = o.out_at().is_some_and(|out| {
        sections.iter().any(|s| {
            let first = s.cells.first().map(|c| timing.seconds_at(c.tick));
            let last = s.cells.last().map(|c| timing.seconds_at(c.tick));
            first.is_some_and(|f| out >= f - window)
                && last.is_some_and(|l| out <= l + window + lock + reach)
        })
    });
    Shocked {
        plays: 1,
        attacks: sections.len() as u32,
        shocks: h.mines().len() as u32,
        stepped: hits.len() as u32,
        locked_missed,
        knocked_out: u32::from(knocked_out),
    }
}

type ShockTable = BTreeMap<String, (u32, Vec<Shocked>)>;

#[test]
fn local_cpu_shocks() {
    let (Some(dirs), true) = (
        std::env::var_os("DDI_CPU_CORPUS"),
        std::env::var("DDI_CPU_SHOCKS").is_ok_and(|v| v == "1"),
    ) else {
        return;
    };
    let seeds: u64 = std::env::var("DDI_CPU_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let mut files = Vec::new();
    for dir in std::env::split_paths(&dirs) {
        collect(&dir, &mut files);
    }
    files.sort();
    sample(&mut files);
    let columns = CPU_LEVELS_V0.levels.len() * EVASIONS.len();
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    let chunk = files.len().div_ceil(workers).max(1);
    let parts: Vec<ShockTable> = std::thread::scope(|scope| {
        let handles: Vec<_> = files
            .chunks(chunk)
            .map(|files| {
                scope.spawn(move || {
                    let mut table = ShockTable::new();
                    for path in files {
                        let ext = path
                            .extension()
                            .map(|e| e.to_string_lossy().to_ascii_lowercase())
                            .unwrap_or_default();
                        let Ok(bytes) = std::fs::read(path) else {
                            continue;
                        };
                        let text = ddi_library::pack::decode_text(&bytes);
                        let Ok(song) = parse_simfile(&text, &ext) else {
                            continue;
                        };
                        for (index, chart) in song.charts.iter().enumerate() {
                            if chart.layout != "dance-single" || chart.derivation.is_some() {
                                continue;
                            }
                            let Ok(plan) = Plan::automatic(&song, index) else {
                                continue;
                            };
                            let judge = ruleset_of("ddi-normal", &song, index).judge;
                            let mut sections: Vec<ddi_engine::battle::Section> = Vec::new();
                            for s in ddi_engine::battle::schedule(&song, index, &judge, 11)
                                .into_iter()
                                .flatten()
                            {
                                if sections.len() < 4
                                    && sections
                                        .last()
                                        .is_none_or(|l| s.start_beat >= l.start_beat + 16)
                                {
                                    sections.push(s);
                                }
                            }
                            if sections.is_empty() {
                                continue;
                            }
                            let mut sums = vec![Shocked::default(); columns];
                            for level in 0..CPU_LEVELS_V0.levels.len() {
                                for (k, &e) in EVASIONS.iter().enumerate() {
                                    for seed in 1..=seeds {
                                        let seed = chart_seed(&song, index, seed);
                                        sums[level * EVASIONS.len() + k].add(&shocked(
                                            &plan, &song, index, &sections, level, e, seed,
                                        ));
                                    }
                                }
                            }
                            for g in ["all".to_string(), slot(chart.difficulty).to_string()] {
                                let e = table
                                    .entry(g)
                                    .or_insert_with(|| (0, vec![Shocked::default(); columns]));
                                e.0 += 1;
                                for (a, b) in e.1.iter_mut().zip(&sums) {
                                    a.add(b);
                                }
                            }
                        }
                    }
                    table
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut table = ShockTable::new();
    for t in parts {
        for (k, (n, sums)) in t {
            let e = table
                .entry(k)
                .or_insert_with(|| (0, vec![Shocked::default(); columns]));
            e.0 += n;
            for (a, b) in e.1.iter_mut().zip(&sums) {
                a.add(b);
            }
        }
    }
    println!(
        "{} files, {seeds} seeds, DDI Normal, mine-response-v1, {}",
        files.len(),
        scale().id()
    );
    for (group, (charts, sums)) in &table {
        println!("{group} | {charts} charts | by level, Evasion 0.5 / 1 / 2");
        for (level, cells) in sums.chunks(EVASIONS.len()).enumerate() {
            let cell = |f: &dyn Fn(&Shocked) -> f64| {
                cells
                    .iter()
                    .map(|s| format!("{:5.2}", f(s)))
                    .collect::<Vec<_>>()
                    .join(" /")
            };
            let a = |s: &Shocked| f64::from(s.attacks.max(1));
            println!(
                "  level {:>2}: stepped of 8 {} | life an attack {} | knocked out % of plays {} | attacks a play {}",
                level + 1,
                cell(&|s| 8.0 * f64::from(s.stepped) / f64::from(s.shocks.max(1))),
                cell(&|s| (5.0 * f64::from(s.stepped) + 10.0 * f64::from(s.locked_missed)) / a(s)),
                cell(&|s| 100.0 * f64::from(s.knocked_out) / f64::from(s.plays.max(1))),
                cell(&|s| f64::from(s.attacks) / f64::from(s.plays.max(1))),
            );
        }
    }
}
