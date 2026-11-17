//! Opt-in study: the CPUs' ten levels beside a person's shape (round V,
//! 2026-11-17, `docs/plans/cpu-player.md`, "Round V").
//!
//! `cargo test --release -p ddi-session --lib cpu_levels_study --
//! --ignored --nocapture` prints aggregates only, never a song's name nor
//! a play's own numbers:
//!
//! 1. `cpus`: the ten levels (`DDI_LEVELS_SCALE`: the levels opponents
//!    play at, or `v1`; candidates through `DDI_LEVELS_ABILITIES`, see
//!    [`levels`]) on every dance-single chart as written of the bundled
//!    songs and of the folders under `DDI_LEVELS_CORPUS` (`:` between,
//!    read in place, one simfile a folder; every `DDI_LEVELS_SAMPLE`th),
//!    `DDI_LEVELS_SEEDS` seeds a level (4), DDI Normal, the fail off (the
//!    whole chart judged; "out" its gauge emptying at any point), the
//!    chart a person is given (`cpu-play-v1`);
//! 2. `reference`: the maintainer's kept plays, only where
//!    `DDI_LEVELS_PLAYS=<the desktop's plays/ folder>` names them (read in
//!    place, read-only; their taps put on DDI Normal's grid by their
//!    offsets, as the combo study does: an estimate for the plays judged
//!    under other rules or difficulties);
//! 3. with both, how a person of the reference's plays would place
//!    against CPUs of each level on charts of the same density.
//!
//! The same measures for both, from a play's judgments in order (its
//! taps' tiers on DDI Normal's grid, W0 to W5 and Miss, and the mines or
//! shocks stepped on): the accuracy and the percent as round U has them
//! (`ddi-percent-v1`), the letters, the share of taps in each tier,
//! misses and breaks (W4, W5, a Miss, a mine) a hundred notes, the full
//! combos and how accurate they are, where along the play breaks fall
//! (by tenths of its notes), a play's lean (its hits' mean signed offset)
//! and spread about it, the lean's drift within the play (its quarters'
//! means about it), how the spread and the breaks grow with the notes in
//! the second before a note (load), and how often a break follows a
//! break. Grouped by the play's notes a second (the chart's density,
//! comparable between a person's plays and the CPUs' charts), by a CPU
//! chart's slot, and by how the play did (its accuracy).
//!
//! MEASUREMENTS: the CPUs are a model; the reference is one player on
//! mostly familiar charts, its shape and not a level.
//!
//! `DDI_LEVELS_OUT=<file>` writes the printout to a file too.

use super::*;
use ddi_engine::cpu::{CPU_LEVELS, LevelScale};
use std::fmt::Write as _;

/// A tap's tier on DDI Normal's grid: W0 to W5, a Miss, a mine or shock
/// stepped on.
const TIERS: usize = 8;
const T_MISS: u8 = 6;
const T_MINE: u8 = 7;
const TIER_NAMES: [&str; TIERS] = ["W0", "W1", "W2", "W3", "W4", "W5", "Miss", "Mine"];

/// The notes a second of a play: its taps over the time from its first
/// note to its last.
const BANDS: [(&str, f64); 5] = [
    ("under 2 notes/s", 2.0),
    ("2 to 3 notes/s", 3.0),
    ("3 to 4.5 notes/s", 4.5),
    ("4.5 to 6 notes/s", 6.0),
    ("6 notes/s and more", f64::INFINITY),
];

/// The notes in the second before a note (its own row's included).
const LOADS: [(&str, f64); 6] = [
    ("<2", 2.0),
    ("2-4", 4.0),
    ("4-6", 6.0),
    ("6-8", 8.0),
    ("8-11", 11.0),
    ("11+", f64::INFINITY),
];

/// A clear's accuracy.
const OUTCOMES: [(&str, f64); 6] = [
    ("acc <70", 70.0),
    ("acc 70-80", 80.0),
    ("acc 80-85", 85.0),
    ("acc 85-90", 90.0),
    ("acc 90-95", 95.0),
    ("acc 95+", f64::INFINITY),
];

/// The letters by index (`percent::grade_index`), and a play out.
const GRADES: [&str; 9] = ["AD", "SSS", "SS", "S", "A", "B", "C", "D", "F"];
const F: usize = 8;

fn bin(v: f64, edges: &[(&str, f64)]) -> usize {
    edges.iter().position(|e| v < e.1).unwrap_or(edges.len() - 1)
}

/// The tier of a tap judged `judgment` (the engine's index, W1 0 to Miss
/// 5) `delta` seconds off: on DDI Normal's grid, W0 split from W1.
fn tier(judgment: u8, delta: f64) -> u8 {
    if judgment >= MISS {
        return T_MISS;
    }
    let d = delta.abs();
    let d_ = ddi::Difficulty::Normal;
    for (t, &units) in ddi::TIER_UNITS.iter().enumerate().take(5) {
        if d <= d_.seconds(units) + 1e-9 {
            return t as u8;
        }
    }
    5
}

/// One play as this study reads it.
struct Read {
    /// Its notes a second ([`BANDS`]), and a CPU chart's slot.
    band: usize,
    slot: Option<usize>,
    out: bool,
    /// Round U's accuracy and percent, in percent.
    accuracy: f64,
    percent: f64,
    /// [`GRADES`]' index.
    grade: usize,
    /// Its moments in order: tier, the note's second, a hit's offset.
    moments: Vec<(u8, f64, Option<f64>)>,
}

impl Read {
    fn new(
        moments: Vec<(u8, f64, Option<f64>)>,
        slot: Option<usize>,
        out: bool,
        accuracy: f64,
        percent: f64,
        grade: usize,
    ) -> Read {
        let taps: Vec<f64> = moments
            .iter()
            .filter(|m| m.0 != T_MINE)
            .map(|m| m.1)
            .collect();
        let (first, last) = taps
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &t| (a.min(t), b.max(t)));
        let nps = taps.len() as f64 / (last - first).max(1.0);
        Read {
            band: bin(nps, &BANDS),
            slot,
            out,
            accuracy,
            percent,
            grade: if out { F } else { grade },
            moments,
        }
    }
}

/// What a group of plays came to.
#[derive(Clone)]
struct Acc {
    plays: u64,
    outs: u64,
    acc_all: f64,
    pct_all: f64,
    clears: u64,
    acc: f64,
    pct: f64,
    grades: [u64; 9],
    /// The clears' percent in half points from -100 to 100, and every
    /// play's (an out below every clear) for the ranks.
    hist: Vec<u32>,
    tiers: [u64; TIERS],
    taps: u64,
    breaks: u64,
    fc: u64,
    fc_acc: f64,
    fc_below90: u64,
    fc_below80: u64,
    tenths: [u64; 10],
    /// Plays with enough hits for a lean: its absolute value, sum and
    /// square, the spread about it, its quarters' drift (RMS of their
    /// means about the play's) and what sampling alone would give.
    leaned: u64,
    lean_abs: f64,
    lean_sum: f64,
    lean_sq: f64,
    spread: f64,
    drift: f64,
    drift_noise: f64,
    /// By the load at the note: taps, tiers, hits and their squared
    /// offset about the play's lean.
    load_taps: [u64; 6],
    load_tiers: [[u64; TIERS]; 6],
    load_hits: [u64; 6],
    load_dev2: [f64; 6],
    /// Taps right after a break, and of them breaks.
    after_break: u64,
    after_break_breaks: u64,
    /// The hits re-placed about the play's lean (calibration taken away).
    centered: [u64; TIERS],
}

impl Acc {
    fn new() -> Acc {
        Acc {
            plays: 0,
            outs: 0,
            acc_all: 0.0,
            pct_all: 0.0,
            clears: 0,
            acc: 0.0,
            pct: 0.0,
            grades: [0; 9],
            hist: vec![0; 400],
            tiers: [0; TIERS],
            taps: 0,
            breaks: 0,
            fc: 0,
            fc_acc: 0.0,
            fc_below90: 0,
            fc_below80: 0,
            tenths: [0; 10],
            leaned: 0,
            lean_abs: 0.0,
            lean_sum: 0.0,
            lean_sq: 0.0,
            spread: 0.0,
            drift: 0.0,
            drift_noise: 0.0,
            load_taps: [0; 6],
            load_tiers: [[0; TIERS]; 6],
            load_hits: [0; 6],
            load_dev2: [0.0; 6],
            after_break: 0,
            after_break_breaks: 0,
            centered: [0; TIERS],
        }
    }

    fn add(&mut self, o: &Acc) {
        self.plays += o.plays;
        self.outs += o.outs;
        self.acc_all += o.acc_all;
        self.pct_all += o.pct_all;
        self.clears += o.clears;
        self.acc += o.acc;
        self.pct += o.pct;
        for i in 0..9 {
            self.grades[i] += o.grades[i];
        }
        for (a, b) in self.hist.iter_mut().zip(&o.hist) {
            *a += b;
        }
        for i in 0..TIERS {
            self.tiers[i] += o.tiers[i];
            self.centered[i] += o.centered[i];
        }
        self.taps += o.taps;
        self.breaks += o.breaks;
        self.fc += o.fc;
        self.fc_acc += o.fc_acc;
        self.fc_below90 += o.fc_below90;
        self.fc_below80 += o.fc_below80;
        for i in 0..10 {
            self.tenths[i] += o.tenths[i];
        }
        self.leaned += o.leaned;
        self.lean_abs += o.lean_abs;
        self.lean_sum += o.lean_sum;
        self.lean_sq += o.lean_sq;
        self.spread += o.spread;
        self.drift += o.drift;
        self.drift_noise += o.drift_noise;
        for i in 0..6 {
            self.load_taps[i] += o.load_taps[i];
            self.load_hits[i] += o.load_hits[i];
            self.load_dev2[i] += o.load_dev2[i];
            for t in 0..TIERS {
                self.load_tiers[i][t] += o.load_tiers[i][t];
            }
        }
        self.after_break += o.after_break;
        self.after_break_breaks += o.after_break_breaks;
    }

    fn play(&mut self, r: &Read) {
        self.plays += 1;
        self.outs += u64::from(r.out);
        self.acc_all += r.accuracy;
        self.pct_all += r.percent;
        let taps: Vec<&(u8, f64, Option<f64>)> = r.moments.iter().filter(|m| m.0 != T_MINE).collect();
        let n = taps.len();
        let broke = |t: u8| t >= 4;
        let breaks = r.moments.iter().filter(|m| broke(m.0)).count() as u64;
        if !r.out {
            self.clears += 1;
            self.acc += r.accuracy;
            self.pct += r.percent;
            self.grades[r.grade] += 1;
            let b = ((r.percent + 100.0) * 2.0).floor().clamp(0.0, 399.0) as usize;
            self.hist[b] += 1;
            if breaks == 0 && n > 0 {
                self.fc += 1;
                self.fc_acc += r.accuracy;
                self.fc_below90 += u64::from(r.accuracy < 90.0);
                self.fc_below80 += u64::from(r.accuracy < 80.0);
            }
        } else {
            self.grades[F] += 1;
        }
        for m in &r.moments {
            self.tiers[usize::from(m.0)] += 1;
        }
        self.taps += n as u64;
        self.breaks += breaks;
        // Where breaks fall, by tenths of the play's notes.
        let mut k = 0usize;
        let mut previous_broke = false;
        for m in &r.moments {
            if m.0 != T_MINE {
                k += 1;
            }
            if broke(m.0) {
                let tenth = ((k.saturating_sub(1)) * 10 / n.max(1)).min(9);
                self.tenths[tenth] += 1;
            }
            if previous_broke {
                self.after_break += 1;
                self.after_break_breaks += u64::from(broke(m.0));
            }
            previous_broke = broke(m.0);
        }
        // Its lean and spread over its hits.
        let hits: Vec<(f64, f64)> = taps.iter().filter_map(|m| m.2.map(|d| (m.1, d))).collect();
        let lean = if hits.len() >= 20 {
            let mean = hits.iter().map(|h| h.1).sum::<f64>() / hits.len() as f64;
            let var = hits.iter().map(|h| (h.1 - mean).powi(2)).sum::<f64>() / (hits.len() as f64 - 1.0);
            self.leaned += 1;
            self.lean_abs += mean.abs();
            self.lean_sum += mean;
            self.lean_sq += mean * mean;
            self.spread += var.sqrt();
            let q = hits.len() / 4;
            let mut rms = 0.0;
            for part in 0..4 {
                let s = &hits[part * q..if part == 3 { hits.len() } else { (part + 1) * q }];
                let m = s.iter().map(|h| h.1).sum::<f64>() / s.len() as f64;
                rms += (m - mean).powi(2);
            }
            self.drift += (rms / 4.0).sqrt();
            // Sampling alone: each quarter's mean wanders by the spread over
            // the square root of its hits.
            self.drift_noise += var.sqrt() / (q.max(1) as f64).sqrt() * (0.75f64).sqrt();
            Some(mean)
        } else {
            None
        };
        // Load at each tap: the notes in the second before it.
        let times: Vec<f64> = taps.iter().map(|m| m.1).collect();
        let mut sorted = times.clone();
        sorted.sort_by(f64::total_cmp);
        for m in &taps {
            let from = sorted.partition_point(|&t| t <= m.1 - 1.0);
            let to = sorted.partition_point(|&t| t <= m.1 + 1e-6);
            let l = bin((to - from) as f64, &LOADS);
            self.load_taps[l] += 1;
            self.load_tiers[l][usize::from(m.0)] += 1;
            if let (Some(d), Some(mean)) = (m.2, lean) {
                self.load_hits[l] += 1;
                self.load_dev2[l] += (d - mean).powi(2);
            }
        }
        // The hits about the play's own lean.
        if let Some(mean) = lean {
            for m in &taps {
                let t = match m.2 {
                    Some(d) => tier(0, d - mean),
                    None => m.0,
                };
                self.centered[usize::from(t)] += 1;
            }
        }
    }

    /// The median of the clears' percent.
    fn median(&self) -> f64 {
        let total: u32 = self.hist.iter().sum();
        let mut seen = 0;
        for (i, &c) in self.hist.iter().enumerate() {
            seen += c;
            if 2 * seen >= total && total > 0 {
                return i as f64 / 2.0 - 100.0 + 0.25;
            }
        }
        f64::NAN
    }
}

fn pc(part: u64, of: u64) -> f64 {
    100.0 * part as f64 / of.max(1) as f64
}

fn header() -> String {
    format!(
        "who | plays out% | all: acc pct | clears: acc pct med | {} (% of plays) | {} (% of moments) | \
         miss/100 break/100 | FC% FCacc FC<90% FC<80% | breaks 1st..10th tenth % | \
         lean|ms| leanSD spread drift (noise) | break after break %",
        GRADES.join(" "),
        TIER_NAMES.join(" ")
    )
}

fn line(out: &mut String, who: &str, a: &Acc) {
    let moments: u64 = a.tiers.iter().sum();
    let tenths: u64 = a.tenths.iter().sum();
    let l = a.leaned.max(1) as f64;
    let lean_mean = a.lean_sum / l;
    let lean_sd = (a.lean_sq / l - lean_mean * lean_mean).max(0.0).sqrt();
    let _ = writeln!(
        out,
        "{who} | {} {:.1} | {:.1} {:.1} | {:.1} {:.1} {:.1} | {} | {} | {:.2} {:.2} | {:.1} {:.1} {:.1} {:.1} | {} | {:.1} {:.1} {:.1} {:.1} ({:.1}) | {:.1}",
        a.plays,
        pc(a.outs, a.plays),
        a.acc_all / a.plays.max(1) as f64,
        a.pct_all / a.plays.max(1) as f64,
        a.acc / a.clears.max(1) as f64,
        a.pct / a.clears.max(1) as f64,
        a.median(),
        a.grades.iter().map(|&g| format!("{:.1}", pc(g, a.plays))).collect::<Vec<_>>().join(" "),
        a.tiers.iter().map(|&t| format!("{:.1}", pc(t, moments))).collect::<Vec<_>>().join(" "),
        100.0 * a.tiers[usize::from(T_MISS)] as f64 / a.taps.max(1) as f64,
        100.0 * a.breaks as f64 / a.taps.max(1) as f64,
        pc(a.fc, a.clears),
        a.fc_acc / a.fc.max(1) as f64,
        pc(a.fc_below90, a.fc),
        pc(a.fc_below80, a.fc),
        a.tenths.iter().map(|&t| format!("{:.0}", pc(t, tenths))).collect::<Vec<_>>().join(" "),
        1000.0 * a.lean_abs / l,
        1000.0 * lean_sd,
        1000.0 * a.spread / l,
        1000.0 * a.drift / l,
        1000.0 * a.drift_noise / l,
        pc(a.after_break_breaks, a.after_break),
    );
}

fn load_lines(out: &mut String, who: &str, a: &Acc) {
    let _ = writeln!(
        out,
        "{who} | {}",
        (0..6)
            .map(|i| {
                let t = a.load_taps[i].max(1);
                let w = &a.load_tiers[i];
                format!(
                    "{}: n{} sd{:.0} W0-1 {:.0} W2 {:.0} W3 {:.0} br/100 {:.1} miss/100 {:.1}",
                    LOADS[i].0,
                    a.load_taps[i],
                    1000.0 * (a.load_dev2[i] / a.load_hits[i].max(1) as f64).sqrt(),
                    pc(w[0] + w[1], t),
                    pc(w[2], t),
                    pc(w[3], t),
                    pc(w[4] + w[5] + w[6], t),
                    pc(w[6], t),
                )
            })
            .collect::<Vec<_>>()
            .join(" || ")
    );
}

/// The levels measured: those opponents play at, `v1`
/// (`DDI_LEVELS_SCALE`), or candidates (`DDI_LEVELS_ABILITIES`).
fn levels() -> (String, Vec<Strength>) {
    let scale = match std::env::var("DDI_LEVELS_SCALE").as_deref() {
        Ok("v1") => LevelScale::AbsoluteV1,
        _ => CPU_LEVELS,
    };
    let Ok(candidates) = std::env::var("DDI_LEVELS_ABILITIES") else {
        return (
            scale.id().to_string(),
            (0..10).map(|i| scale.strength(i).expect("ten levels")).collect(),
        );
    };
    // A candidate model: `cpu-model-v2` with `DDI_LEVELS_MODEL`'s
    // `name=value` pairs (`,` between) put in.
    let mut m = ddi_engine::cpu::ABSOLUTE_V2;
    if let Ok(over) = std::env::var("DDI_LEVELS_MODEL") {
        for pair in over.split(',').filter(|p| !p.is_empty()) {
            let (k, v) = pair.split_once('=').expect("name=value");
            let v: f64 = v.trim().parse().expect("a number");
            match k.trim() {
                "onset" => m.onset = v,
                "spread_onset" => m.spread_onset = v,
                "widen" => m.widen = v,
                "tail_share" => m.tail_share = v,
                "tail_width" => m.tail_width = v,
                "lapse_linear" => m.lapse_linear = v,
                "lapse_square" => m.lapse_square = v,
                "drift_period" => m.drift_period = v,
                "form" => m.form = v,
                "burst" => m.burst = v,
                "burst_seconds" => m.burst_seconds = v,
                "stumble_ms" => m.stumble_ms = v,
                "fatigue_end" => m.fatigue_end = v,
                "fatigue" => m.fatigue = v,
                "jack_share" => m.jack_share = v,
                "stamina_share" => m.stamina_share = v,
                other => panic!("no {other} in the model"),
            }
        }
    }
    let m: &'static ddi_engine::cpu::AbsoluteModel = Box::leak(Box::new(m));
    let levels: Vec<Strength> = candidates
        .split(',')
        .map(|p| {
            let f: Vec<f64> = p.split(':').map(|x| x.trim().parse().expect("a number")).collect();
            Strength::Absolute(ddi_engine::cpu::Ability {
                spread_ms: f[0],
                speed: f[1],
                floor: f[2],
                lean_ms: f[3],
                drift_ms: f[4],
                model: m,
            })
        })
        .collect();
    (format!("candidates ({m:?})"), levels)
}

/// A CPU's play read.
fn cpu_play(plan: &Plan, song: &Song, chart: usize, level: (usize, Strength), seed: u64) -> Option<Read> {
    let mut ruleset = ddi::ruleset(&DdiOptions::default());
    ruleset.fail = FailPolicy::Off;
    let mut o = Opponent::planned_as(
        plan,
        song,
        chart,
        ruleset,
        (level.1, MINE_RESPONSE_V0.avoidance(level.0)),
        seed,
        Skills::default(),
        CPU_PLAY,
    )
    .ok()?;
    o.start(0.0);
    o.finish();
    let view = o.score();
    let d = view.ddi?;
    let moments: Vec<(u8, f64, Option<f64>)> = o
        .player()
        .results()
        .timeline
        .iter()
        .filter_map(|j| match j.kind {
            JudgeEventKind::Tap(t, delta) => {
                let t = tier(t.index() as u8, delta);
                Some((t, f64::from(j.time) - delta, (t != T_MISS).then_some(delta)))
            }
            JudgeEventKind::HitMine | JudgeEventKind::HitShock => Some((T_MINE, f64::from(j.time), None)),
            _ => None,
        })
        .collect();
    if moments.is_empty() {
        return None;
    }
    let grade = percent::grade_index(&o.player().results_brief().grade).unwrap_or(F);
    Some(Read::new(
        moments,
        Some(song.charts[chart].difficulty as usize),
        o.out_at().is_some(),
        100.0 * view.accuracy().unwrap_or(0.0),
        d.percent as f64 / 100.0,
        grade,
    ))
}

/// The groups a CPU's play counts in.
fn cpu_groups(r: &Read) -> Vec<String> {
    let mut g = vec!["all".to_string(), format!("band {}", r.band)];
    if let Some(s) = r.slot {
        g.push(format!("slot {s}"));
    }
    if !r.out {
        g.push(format!("outcome {}", bin(r.accuracy, &OUTCOMES)));
    }
    g
}

type Groups = std::collections::BTreeMap<String, Vec<Acc>>;

/// The CPUs at each level over `files`, by group.
fn cpus(files: &[PathBuf], strengths: &[Strength], seeds: u64) -> Groups {
    let workers = env_number("DDI_LEVELS_THREADS", 4).max(1) as usize;
    let chunk = files.len().div_ceil(workers).max(1);
    let parts: Vec<Groups> = std::thread::scope(|scope| {
        let handles: Vec<_> = files
            .chunks(chunk)
            .map(|files| {
                scope.spawn(move || {
                    let mut groups = Groups::new();
                    for path in files {
                        let Some(ext) = path.extension() else { continue };
                        let ext = ext.to_string_lossy().to_ascii_lowercase();
                        let Ok(bytes) = std::fs::read(path) else { continue };
                        let text = ddi_library::pack::decode_text(&bytes);
                        let Ok(song) = ddi_library::scan::parse_simfile(&text, &ext) else {
                            continue;
                        };
                        for (index, chart) in song.charts.iter().enumerate() {
                            if chart.layout != "dance-single" || chart.derivation.is_some() {
                                continue;
                            }
                            let Ok(plan) = Plan::automatic(&song, index) else {
                                continue;
                            };
                            // Each chart's own seeds, so that a level's leans
                            // (drawn from a CPU's seed) are many over the corpus.
                            let chart_key = {
                                use std::hash::{Hash, Hasher};
                                let mut h = std::collections::hash_map::DefaultHasher::new();
                                format!("{:?}", song.chart_digest(index)).hash(&mut h);
                                h.finish()
                            };
                            for (level, &strength) in strengths.iter().enumerate() {
                                for seed in 1..=seeds {
                                    let seed = (seed ^ chart_key)
                                        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                                        .wrapping_add(level as u64 * 7919 + 1);
                                    let Some(r) = cpu_play(&plan, &song, index, (level, strength), seed) else {
                                        continue;
                                    };
                                    for g in cpu_groups(&r) {
                                        groups.entry(g).or_insert_with(|| vec![Acc::new(); strengths.len()])[level]
                                            .play(&r);
                                    }
                                }
                            }
                        }
                    }
                    groups
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut all = Groups::new();
    for p in parts {
        for (k, v) in p {
            let e = all.entry(k).or_insert_with(|| vec![Acc::new(); strengths.len()]);
            for (a, b) in e.iter_mut().zip(&v) {
                a.add(b);
            }
        }
    }
    all
}

fn group_name(g: &str) -> String {
    let slots = ["Beginner", "Easy", "Medium", "Hard", "Challenge", "Edit"];
    if let Some(b) = g.strip_prefix("band ") {
        return BANDS[b.parse::<usize>().unwrap()].0.to_string();
    }
    if let Some(s) = g.strip_prefix("slot ") {
        return format!("slot {}", slots[s.parse::<usize>().unwrap().min(5)]);
    }
    if let Some(o) = g.strip_prefix("outcome ") {
        return format!("clears at {}", OUTCOMES[o.parse::<usize>().unwrap()].0);
    }
    g.to_string()
}

fn print_cpus(out: &mut String, name: &str, groups: &Groups) {
    let _ = writeln!(out, "== {name}");
    let _ = writeln!(out, "{}", header());
    for (g, levels) in groups {
        let _ = writeln!(out, "-- {}", group_name(g));
        for (i, a) in levels.iter().enumerate() {
            line(out, &format!("L{:>2}", i + 1), a);
        }
    }
    if let Some(levels) = groups.get("all") {
        let _ = writeln!(out, "-- load (all charts): the spread (ms, about the play's lean) and the tiers by the notes in the second before a note");
        for (i, a) in levels.iter().enumerate() {
            load_lines(out, &format!("L{:>2}", i + 1), a);
        }
    }
}

/// A kept play read, and its band.
fn kept_read(k: &Kept) -> Option<Read> {
    let moments: Vec<(u8, f64, Option<f64>)> = k
        .moments
        .iter()
        .zip(&k.times)
        .map(|(&(t, delta), &at)| match (t, delta) {
            (BROKE, _) => (T_MINE, at, None),
            (MISS, _) | (_, None) => (T_MISS, at, None),
            (t, Some(d)) => (tier(t, d), at, Some(d)),
        })
        .collect();
    if moments.len() < 20 {
        return None;
    }
    let mines = k.seq.iter().filter(|&&c| c == BROKE).count() as u32;
    let points = k.pp.points
        + (ddi::TAP_POINTS[5] - TAP_POINTS[5]) * i64::from(k.pp.tiers[5])
        + (ddi::MINE_POINTS - MINE_POINTS) * i64::from(mines);
    let accuracy = ddi::accuracy_of(points, k.pp.most) as f64 / 100.0;
    let percent = percent::built_percent(&k.pp, mines) as f64 / 100.0;
    let grade = percent::built_grade(&k.pp, mines);
    let out = !k.clear;
    Some(Read::new(moments, None, out, accuracy, percent, grade))
}

/// The person's reference and what tells calibration from skill.
fn reference(out: &mut String, dir: &Path) -> Option<Vec<Read>> {
    let (plays, r) = read_kept(dir);
    let kept: Vec<&Kept> = plays
        .iter()
        .filter(|k| !k.autoplay && k.ending != ddi_ui::state::history::Ending::Quit)
        .collect();
    let _ = writeln!(
        out,
        "== the reference: {} kept play files, {} not read, {} without DDI's judgment; {} played or judged again under DDI, not autoplay, not quit",
        r.files,
        r.unread,
        r.no_ddi,
        kept.len()
    );
    let mut difficulties = std::collections::BTreeMap::<String, u32>::new();
    for k in &kept {
        *difficulties.entry(format!("{:?}", k.difficulty)).or_default() += 1;
    }
    let _ = writeln!(out, "DDI difficulty judged on: {difficulties:?}");
    let reads: Vec<(Read, &Kept)> = kept.iter().filter_map(|k| kept_read(k).map(|r| (r, *k))).collect();
    let mut groups = Groups::new();
    for (r, _) in &reads {
        let mut g = vec!["all".to_string(), format!("band {}", r.band)];
        if !r.out {
            g.push(format!("outcome {}", bin(r.accuracy, &OUTCOMES)));
        }
        for g in g {
            groups.entry(g).or_insert_with(|| vec![Acc::new()])[0].play(r);
        }
    }
    let _ = writeln!(out, "{}", header());
    for (g, a) in &groups {
        line(out, &format!("person, {}", group_name(g)), &a[0]);
    }
    if let Some(a) = groups.get("all") {
        let _ = writeln!(out, "-- load");
        load_lines(out, "person", &a[0]);
        let moments: u64 = a[0].centered.iter().sum();
        let _ = writeln!(
            out,
            "-- hits about each play's own lean (calibration taken away), % of the leaned plays' taps: {}",
            a[0].centered
                .iter()
                .zip(TIER_NAMES)
                .map(|(&c, n)| format!("{n} {:.1}", pc(c, moments)))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
    // Calibration against skill: the leans by session and device.
    let mut leans: Vec<(f64, f64, String, f64, f64, u32, f64)> = Vec::new();
    for (r, k) in &reads {
        let hits: Vec<f64> = r.moments.iter().filter_map(|m| m.2).collect();
        if hits.len() < 50 {
            continue;
        }
        let mean = hits.iter().sum::<f64>() / hits.len() as f64;
        let sd = (hits.iter().map(|h| (h - mean).powi(2)).sum::<f64>() / (hits.len() as f64 - 1.0)).sqrt();
        leans.push((k.when, mean, k.audio.clone(), k.audio_offset, sd, hits.len() as u32, r.accuracy));
    }
    leans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let n = leans.len().max(1) as f64;
    let ms = |x: f64| 1000.0 * x;
    let mean = leans.iter().map(|l| l.1).sum::<f64>() / n;
    let sd = (leans.iter().map(|l| (l.1 - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
    let mut sorted: Vec<f64> = leans.iter().map(|l| l.1).collect();
    sorted.sort_by(f64::total_cmp);
    let q = |p: f64| sorted.get(((sorted.len() as f64 - 1.0) * p).round() as usize).copied().unwrap_or(0.0);
    let spread_mean = leans.iter().map(|l| l.4).sum::<f64>() / n;
    let noise = leans.iter().map(|l| l.4 / f64::from(l.5).sqrt()).sum::<f64>() / n;
    let _ = writeln!(
        out,
        "-- the lean (plays with 50 hits or more: {}): mean {:.1} ms, SD between plays {:.1} ms (sampling alone {:.1}), 10/50/90% {:.1} / {:.1} / {:.1} ms, mean |lean| {:.1} ms; spread about it {:.1} ms",
        leans.len(),
        ms(mean),
        ms(sd),
        ms(noise),
        ms(q(0.1)),
        ms(q(0.5)),
        ms(q(0.9)),
        ms(leans.iter().map(|l| l.1.abs()).sum::<f64>() / n),
        ms(spread_mean),
    );
    // Sessions: plays with less than 45 minutes between them.
    let mut sessions: Vec<Vec<f64>> = Vec::new();
    let mut last = f64::NEG_INFINITY;
    for l in &leans {
        if l.0 - last > 45.0 * 60_000.0 {
            sessions.push(Vec::new());
        }
        sessions.last_mut().unwrap().push(l.1);
        last = l.0;
    }
    let within = {
        let (mut ss, mut df) = (0.0, 0.0);
        for s in sessions.iter().filter(|s| s.len() >= 2) {
            let m = s.iter().sum::<f64>() / s.len() as f64;
            ss += s.iter().map(|x| (x - m).powi(2)).sum::<f64>();
            df += s.len() as f64 - 1.0;
        }
        (ss / df.max(1.0)).sqrt()
    };
    let means: Vec<f64> = sessions.iter().filter(|s| s.len() >= 2).map(|s| s.iter().sum::<f64>() / s.len() as f64).collect();
    let mm = means.iter().sum::<f64>() / means.len().max(1) as f64;
    let between = (means.iter().map(|x| (x - mm).powi(2)).sum::<f64>() / (means.len() as f64 - 1.0).max(1.0)).sqrt();
    let _ = writeln!(
        out,
        "-- sessions (45 minutes apart): {} ({} of two plays or more); the lean's SD within a session {:.1} ms, between sessions' means {:.1} ms",
        sessions.len(),
        means.len(),
        ms(within),
        ms(between)
    );
    let mut devices = std::collections::BTreeMap::<&str, Vec<f64>>::new();
    let mut offsets = std::collections::BTreeSet::<i64>::new();
    for l in &leans {
        devices.entry(l.2.as_str()).or_default().push(l.1);
        offsets.insert((l.3 * 1e4).round() as i64);
    }
    let dm: Vec<(usize, f64)> = devices.values().map(|v| (v.len(), v.iter().sum::<f64>() / v.len() as f64)).collect();
    let _ = writeln!(
        out,
        "-- audio devices: {}; their plays and mean lean: {}; audio offsets in effect: {} distinct",
        dm.len(),
        dm.iter().map(|(n, m)| format!("{n} plays {:.1} ms", ms(*m))).collect::<Vec<_>>().join(", "),
        offsets.len()
    );
    let mut by_lean = [(0u32, 0.0f64); 4];
    for l in &leans {
        let b = match ms(l.1.abs()) {
            x if x < 5.0 => 0,
            x if x < 10.0 => 1,
            x if x < 20.0 => 2,
            _ => 3,
        };
        by_lean[b].0 += 1;
        by_lean[b].1 += l.6;
    }
    let _ = writeln!(
        out,
        "-- accuracy by |lean| <5 / 5-10 / 10-20 / 20+ ms: {}",
        by_lean
            .iter()
            .map(|(n, a)| format!("{n} plays {:.1}", a / f64::from((*n).max(1))))
            .collect::<Vec<_>>()
            .join(" / ")
    );
    Some(reads.into_iter().map(|(r, _)| r).collect())
}

/// How the reference's plays would place against the CPUs at each level
/// on charts of the same density: the chance a play beats a CPU's (out
/// below every clear, a clear by its percent; ties half), and the place
/// that makes among eight CPUs of that level.
fn ranks(out: &mut String, person: &[Read], cpus: &Groups) {
    let _ = writeln!(
        out,
        "== the reference against the CPUs (by the play's density; out below every clear, then the percent): the chance a play beats one CPU of each level, and its expected place among eight of them"
    );
    for (b, (name, _)) in BANDS.iter().enumerate() {
        let Some(levels) = cpus.get(&format!("band {b}")) else { continue };
        let mine: Vec<&Read> = person.iter().filter(|r| r.band == b).collect();
        if mine.is_empty() {
            continue;
        }
        let row: Vec<String> = levels
            .iter()
            .map(|a| {
                let total = a.plays.max(1) as f64;
                let outs = a.outs as f64;
                let mut p = 0.0;
                for r in &mine {
                    p += if r.out {
                        0.5 * outs / total
                    } else {
                        let at = ((r.percent + 100.0) * 2.0).floor().clamp(0.0, 399.0) as usize;
                        let below: u32 = a.hist[..at].iter().sum();
                        (outs + f64::from(below) + 0.5 * f64::from(a.hist[at])) / total
                    };
                }
                let p = p / mine.len() as f64;
                format!("{:.0}% ({:.1})", 100.0 * p, 1.0 + 8.0 * (1.0 - p))
            })
            .collect();
        let _ = writeln!(out, "{name} ({} plays): {}", mine.len(), row.join(" | "));
    }
}

#[test]
#[ignore = "a study: prints the CPUs' levels beside a person's shape"]
fn cpu_levels_study() {
    let only = std::env::var("DDI_LEVELS_ONLY").ok();
    let wanted = |part: &str| only.as_deref().is_none_or(|o| o == part);
    let mut out = String::new();
    let started = std::time::Instant::now();
    let mut corpus: Option<Groups> = None;
    if wanted("cpus") {
        let seeds = env_number("DDI_LEVELS_SEEDS", 4);
        let (id, strengths) = levels();
        let mut bundled = Vec::new();
        simfiles(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs"), &mut bundled);
        if std::env::var_os("DDI_LEVELS_NO_BUNDLED").is_none() {
            print_cpus(
                &mut out,
                &format!("{id}: the bundled songs, {seeds} seeds a level"),
                &cpus(&bundled, &strengths, seeds),
            );
        }
        if let Some(dirs) = std::env::var_os("DDI_LEVELS_CORPUS") {
            let mut files = Vec::new();
            for dir in std::env::split_paths(&dirs) {
                simfiles(&dir, &mut files);
            }
            let every = env_number("DDI_LEVELS_SAMPLE", 1).max(1) as usize;
            let files: Vec<PathBuf> = files.into_iter().step_by(every).collect();
            let groups = cpus(&files, &strengths, seeds);
            print_cpus(
                &mut out,
                &format!("{id}: the local folders ({} simfiles), {seeds} seeds a level", files.len()),
                &groups,
            );
            corpus = Some(groups);
        }
        let _ = writeln!(out, "({:.0} s)", started.elapsed().as_secs_f64());
    }
    if wanted("reference")
        && let Some(dir) = std::env::var_os("DDI_LEVELS_PLAYS")
        && let Some(person) = reference(&mut out, Path::new(&dir))
        && let Some(groups) = &corpus
    {
        ranks(&mut out, &person, groups);
    }
    print!("{out}");
    if let Some(path) = std::env::var_os("DDI_LEVELS_OUT") {
        std::fs::write(path, &out).unwrap();
    }
}
