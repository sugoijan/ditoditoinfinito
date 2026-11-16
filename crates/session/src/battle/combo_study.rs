//! Opt-in study: putting combos into DDI's score
//! (`docs/plans/ddi-ruleset.md`, "DDI's score and combos: a study").
//! NOTHING HERE SCORES A PLAY: it reads plays as the engine judged them
//! and works candidate scores out beside today's.
//!
//! `cargo test --release -p ddi-session --lib combo_score_study --
//! --ignored --nocapture` prints aggregates only, never a song's name:
//!
//! 1. made-up full combos of 100 to 1,200 notes: what one miss costs by
//!    where it falls, and 2, 5 and 10 misses spread evenly and at random,
//!    in *slips* (a W1 that became a W2: 10 points, the same under every
//!    candidate with a combo part);
//! 2. the CPUs at each of the ten absolute levels (`cpu-levels-v1`,
//!    `cpu-play-v1`: the chart a person is given) on every dance-single
//!    chart as written of the bundled songs and of the folders under
//!    `DDI_COMBO_CORPUS` (`:` between, read in place; every
//!    `DDI_COMBO_SAMPLE`th simfile, 1), `DDI_COMBO_SEEDS` seeds a level
//!    (4), DDI Normal, the fail off, plays whose gauge emptied left out:
//!    by level the full combos and each combo part's mean; over every
//!    pair of plays of one chart, how each candidate reorders them
//!    against today's score, how often a full combo loses to a play with
//!    breaks (by the breaks and by how much more accurate the broken play
//!    is), how often the play with fewer breaks beats the more accurate
//!    one, and how often a play with no more breaks and more points
//!    loses (the position's unfairness); and on the full combos, what one
//!    miss put in costs by its place;
//! 3. the attack study's stages (`attack_rates`: eight CPUs of mixed
//!    levels on its six bundled charts, `DDI_COMBO_STAGE_SEEDS` seeds,
//!    8), As picked and Adaptive, with attacks and without: how often
//!    each candidate changes the final order and the winner, what an
//!    attack received costs in score (the same play with its shocks not
//!    stepped on and the notes missed in the lanes they locked hit), and
//!    whether a combo run carries across a step between charts.
//!
//! `DDI_COMBO_ONLY=made-up|plays|stages` runs one part.
//!
//! The candidates: today's score plus a combo part worth, for a full
//! combo, a share S of the proportional million (`M`: 10, 20, 30%) or of
//! the chart's own full score, the million and 100 a point (`F`: 2, 4,
//! 6%, where a full combo is worth that many percent points on every
//! chart; 4% F is 10% M on a chart of 300 notes):
//! A50/A100/A200, each note in combo earning `min(combo, K) / K`; B, the
//! sum of the runs' squares over the notes' square; C, the longest run
//! over the notes; A100+B and A100+C, half each; and, with no combo part,
//! a miss at −50 and −100 points. The percent is today's under all but
//! the last two.
//!
//! MEASUREMENTS OF CPUS, not of people.

use super::*;
use ddi_engine::cpu::{CPU_LEVELS_V1, CPU_PLAY, MINE_RESPONSE_V0, Plan, Skills, Strength};
use ddi_engine::judge::{JudgeEventKind, NoteState};
use ddi_engine::rules::FailPolicy;
use ddi_engine::rules::ddi::{self, DdiOptions};
use ddi_room::{Charts, MatchDifficulty};
use std::path::{Path, PathBuf};

/// A play's judgments in order: a tap's tier (`Judgement::index`, W1 0 to
/// Miss 5), or [`BROKE`], a mine or a shock stepped on.
type Seq = Vec<u8>;
const BROKE: u8 = 6;
const MISS: u8 = 5;
const KS: [u64; 3] = [50, 100, 200];

/// What the candidates read of a play.
#[derive(Clone, Default)]
struct Feat {
    /// The chart's tap-like notes as the play was shown them.
    n: u32,
    most: i64,
    w0: u32,
    /// Its points but its misses'.
    fixed: i64,
    misses: u32,
    breaks: u32,
    /// The combo parts, 0 to 1: A at each of [`KS`], B, C.
    a: [f64; 3],
    b: f64,
    c: f64,
}

/// `sum(min(i, k))` for `i` in `1..=n`: a full combo's ramp.
fn full_ramp(n: u64, k: u64) -> f64 {
    if n <= k {
        (n * (n + 1) / 2) as f64
    } else {
        (k * (k + 1) / 2 + (n - k) * k) as f64
    }
}

/// `seq`'s features on a chart of `n` notes; `other`, its points from
/// what `seq` does not hold (hold units, Boos).
fn feat(seq: &[u8], n: u32, most: i64, w0: u32, other: i64) -> Feat {
    let mut f = Feat {
        n,
        most,
        w0,
        fixed: other,
        ..Feat::default()
    };
    let (mut combo, mut squares, mut longest) = (0u64, 0u64, 0u64);
    let mut ramp = [0u64; 3];
    for &c in seq {
        if c <= 2 {
            combo += 1;
            for (r, k) in ramp.iter_mut().zip(KS) {
                *r += combo.min(k);
            }
            f.fixed += ddi::TAP_POINTS[usize::from(c)];
            continue;
        }
        match c {
            MISS => f.misses += 1,
            BROKE => f.fixed += ddi::MINE_POINTS,
            _ => f.fixed += ddi::TAP_POINTS[usize::from(c)],
        }
        f.breaks += 1;
        squares += combo * combo;
        longest = longest.max(combo);
        combo = 0;
    }
    squares += combo * combo;
    longest = longest.max(combo);
    let notes = u64::from(n.max(1));
    for (i, k) in KS.into_iter().enumerate() {
        f.a[i] = (ramp[i] as f64 / full_ramp(notes, k)).min(1.0);
    }
    f.b = (squares as f64 / (notes * notes) as f64).min(1.0);
    f.c = (longest as f64 / notes as f64).min(1.0);
    f
}

/// The points `seq` gives by itself at today's values.
fn seq_points(seq: &[u8]) -> i64 {
    seq.iter()
        .map(|&c| match c {
            BROKE => ddi::MINE_POINTS,
            c => ddi::TAP_POINTS[usize::from(c)],
        })
        .sum()
}

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Today,
    A(usize),
    B,
    C,
    AB,
    AC,
    Miss(i64),
}

#[derive(Clone, Copy)]
struct Cand {
    shape: Shape,
    /// The share a full combo's part is of the scale.
    s: f64,
    /// The scale: the chart's own full score, else the million.
    full: bool,
}

impl Cand {
    fn name(&self) -> String {
        let shape = match self.shape {
            Shape::Today => return "today".into(),
            Shape::Miss(m) => return format!("miss {m}"),
            Shape::A(i) => format!("A{}", KS[i]),
            Shape::B => "B".into(),
            Shape::C => "C".into(),
            Shape::AB => "A100+B".into(),
            Shape::AC => "A100+C".into(),
        };
        format!(
            "{shape} {:.0}% {}",
            self.s * 100.0,
            if self.full { "F" } else { "M" }
        )
    }

    fn score(&self, f: &Feat) -> i64 {
        let miss = match self.shape {
            Shape::Miss(m) => m,
            _ => ddi::TAP_POINTS[usize::from(MISS)],
        };
        let points = f.fixed + miss * i64::from(f.misses);
        let base = ddi::DdiScoreView::of(points, f.most, f.w0).score;
        let part = match self.shape {
            Shape::Today | Shape::Miss(_) => return base,
            Shape::A(i) => f.a[i],
            Shape::B => f.b,
            Shape::C => f.c,
            Shape::AB => (f.a[1] + f.b) / 2.0,
            Shape::AC => (f.a[1] + f.c) / 2.0,
        };
        let scale = if self.full {
            1e6 + 100.0 * f.most as f64
        } else {
            1e6
        };
        base + (self.s * scale * part).floor() as i64
    }
}

/// Today's score first, then the heavier misses, then every shape at
/// every share on both scales.
fn cands() -> Vec<Cand> {
    let plain = |shape| Cand {
        shape,
        s: 0.0,
        full: false,
    };
    let mut v = vec![plain(Shape::Today), plain(Shape::Miss(-50)), plain(Shape::Miss(-100))];
    for (full, shares) in [(false, [0.1, 0.2, 0.3]), (true, [0.02, 0.04, 0.06])] {
        for s in shares {
            for shape in [
                Shape::A(0),
                Shape::A(1),
                Shape::A(2),
                Shape::B,
                Shape::C,
                Shape::AB,
                Shape::AC,
            ] {
                v.push(Cand { shape, s, full });
            }
        }
    }
    v
}

/// What a W1 that became a W2 costs the score on a chart of `most`.
fn slip(most: i64) -> f64 {
    1000.0 + 1e7 / most.max(1) as f64
}

/// `seq` with the tap at each of `at` a miss.
fn with_misses(seq: &[u8], at: &[usize]) -> Seq {
    let mut s = seq.to_vec();
    for &i in at {
        s[i] = MISS;
    }
    s
}

/// The places of `seq`'s taps.
fn taps_of(seq: &[u8]) -> Vec<usize> {
    (0..seq.len()).filter(|&i| seq[i] != BROKE).collect()
}

const PLACES: [f64; 7] = [0.025, 0.10, 0.25, 0.50, 0.75, 0.90, 0.975];

fn place(taps: &[usize], share: f64) -> usize {
    taps[((taps.len() - 1) as f64 * share).round() as usize]
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Part 1: made-up full combos, every tap a W1.
fn made_up(cands: &[Cand]) {
    for n in [100u32, 300, 600, 1200] {
        let most = 50 * i64::from(n);
        let seq = vec![0u8; n as usize];
        let taps = taps_of(&seq);
        let base = feat(&seq, n, most, 0, 0);
        println!(
            "== a full combo of {n} notes, every tap W1: what misses cost, in slips (a slip {:.0} points \
             of score, {:.3} percent points)",
            slip(most),
            20.0 / f64::from(n)
        );
        println!(
            "{:<16} one miss at 2.5% / 10% / 25% / 50% / 75% / 90% / 97.5% | 2 even, random | 5 even, random \
             | 10 even, random",
            ""
        );
        for c in cands {
            let cost = |at: &[usize]| {
                let f = feat(&with_misses(&seq, at), n, most, 0, 0);
                (c.score(&base) - c.score(&f)) as f64 / slip(most)
            };
            let one: Vec<String> = PLACES
                .iter()
                .map(|&p| format!("{:6.1}", cost(&[place(&taps, p)])))
                .collect();
            let many: Vec<String> = [2usize, 5, 10]
                .iter()
                .map(|&k| {
                    let even: Vec<usize> = (1..=k).map(|j| j * taps.len() / (k + 1)).collect();
                    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ (u64::from(n) << 8) ^ k as u64);
                    let draws = 400;
                    let mut sum = 0.0;
                    for _ in 0..draws {
                        let mut at: Vec<usize> = Vec::new();
                        while at.len() < k {
                            let i = (rng.next() % taps.len() as u64) as usize;
                            if !at.contains(&i) {
                                at.push(i);
                            }
                        }
                        sum += cost(&at);
                    }
                    format!("{:6.1} {:6.1}", cost(&even), sum / f64::from(draws))
                })
                .collect();
            println!("{:<16} {} | {}", c.name(), one.join(" "), many.join(" | "));
        }
    }
}

/// One CPU play of a chart.
struct Played {
    level: usize,
    seq: Seq,
    f: Feat,
    /// Its gauge emptied: not a clear.
    out: bool,
}

/// `p`'s judgments in order and its features.
fn read_play(p: &ddi_engine::player::Player, view: &ScoreView) -> (Seq, Feat) {
    let seq: Seq = p
        .results()
        .timeline
        .iter()
        .filter_map(|j| match j.kind {
            JudgeEventKind::Tap(t, _) => Some(t.index() as u8),
            JudgeEventKind::HitMine | JudgeEventKind::HitShock => Some(BROKE),
            _ => None,
        })
        .collect();
    let d = view.ddi.expect("DDI");
    let f = feat(
        &seq,
        p.score_ctx().steps,
        d.most,
        p.tally().w0,
        d.points - seq_points(&seq),
    );
    // The features read the play as the engine scored it.
    debug_assert_eq!(Cand { shape: Shape::Today, s: 0.0, full: false }.score(&f), d.score);
    (seq, f)
}

fn solo(plan: &Plan, song: &Song, chart: usize, level: usize, seed: u64) -> Option<Played> {
    let mut ruleset = ddi::ruleset(&DdiOptions::default());
    ruleset.fail = FailPolicy::Off;
    let mut o = Opponent::planned_as(
        plan,
        song,
        chart,
        ruleset,
        (
            Strength::Absolute(CPU_LEVELS_V1.levels[level]),
            MINE_RESPONSE_V0.avoidance(level),
        ),
        seed,
        Skills::default(),
        CPU_PLAY,
    )
    .ok()?;
    o.start(0.0);
    o.finish();
    let (seq, f) = read_play(o.player(), &o.score());
    assert_eq!(
        Cand { shape: Shape::Today, s: 0.0, full: false }.score(&f),
        o.score().ddi.unwrap().score,
        "the study's score is the engine's"
    );
    Some(Played {
        level,
        seq,
        f,
        out: o.out_at().is_some(),
    })
}

/// Bins of the broken play's lead in percent points over a full combo.
const LEADS: [f64; 6] = [0.0, 0.25, 0.5, 1.0, 2.0, 4.0];
const LEAD_NAMES: [&str; 7] = [
    "no more accurate",
    "up to 0.25 pt",
    "0.25 to 0.5",
    "0.5 to 1",
    "1 to 2",
    "2 to 4",
    "over 4",
];
/// Bins of the breaks of the broken play.
const BREAK_NAMES: [&str; 5] = ["1 break", "2", "3 to 5", "6 to 10", "11 and more"];

fn bin(v: f64, edges: &[f64]) -> usize {
    edges.iter().position(|&e| v <= e).unwrap_or(edges.len())
}

fn break_bin(b: u32) -> usize {
    match b {
        0 | 1 => 0,
        2 => 1,
        3..=5 => 2,
        6..=10 => 3,
        _ => 4,
    }
}

#[derive(Clone, Default)]
struct CandSum {
    /// Pairs today's score orders, and those the candidate turns around.
    pairs: u64,
    flips: u64,
    /// A full combo against a play with breaks, by the breaks and the
    /// broken play's lead: pairs, and those the full combo loses.
    fc: [[(u64, u64); 7]; 5],
    /// Fewer breaks against more points, by the lead in points: pairs,
    /// and those the one with fewer breaks wins.
    clean: [(u64, u64); 7],
    /// No more breaks and more points (or fewer and no fewer): pairs, and
    /// those it loses.
    dominant: (u64, u64),
    /// On full combos: a miss put in at each of [`PLACES`], slips.
    place: [f64; 7],
    full_combos: u64,
    /// On plays of one to five breaks: one more miss at the middle.
    extra: f64,
    broken: u64,
}

#[derive(Clone, Default)]
struct LevelSum {
    plays: u32,
    out: u32,
    full_combos: u32,
    breaks: u64,
    percent: f64,
    parts: [f64; 5],
    notes: u64,
}

#[derive(Clone, Default)]
struct Sum {
    charts: u32,
    levels: [LevelSum; 10],
    cands: Vec<CandSum>,
}

impl Sum {
    fn new(cands: usize) -> Sum {
        Sum {
            cands: vec![CandSum::default(); cands],
            ..Sum::default()
        }
    }

    fn add(&mut self, o: &Sum) {
        self.charts += o.charts;
        for (a, b) in self.levels.iter_mut().zip(&o.levels) {
            a.plays += b.plays;
            a.out += b.out;
            a.full_combos += b.full_combos;
            a.breaks += b.breaks;
            a.percent += b.percent;
            a.notes += b.notes;
            for k in 0..5 {
                a.parts[k] += b.parts[k];
            }
        }
        for (a, b) in self.cands.iter_mut().zip(&o.cands) {
            a.pairs += b.pairs;
            a.flips += b.flips;
            for k in 0..5 {
                for l in 0..7 {
                    a.fc[k][l].0 += b.fc[k][l].0;
                    a.fc[k][l].1 += b.fc[k][l].1;
                }
            }
            for l in 0..7 {
                a.clean[l].0 += b.clean[l].0;
                a.clean[l].1 += b.clean[l].1;
                a.place[l] += b.place[l];
            }
            a.dominant.0 += b.dominant.0;
            a.dominant.1 += b.dominant.1;
            a.full_combos += b.full_combos;
            a.extra += b.extra;
            a.broken += b.broken;
        }
    }

    /// One chart's plays into the sums.
    fn chart(&mut self, plays: &[Played], cands: &[Cand]) {
        self.charts += 1;
        for p in plays {
            let l = &mut self.levels[p.level];
            l.plays += 1;
            if p.out {
                l.out += 1;
                continue;
            }
            l.full_combos += u32::from(p.f.breaks == 0);
            l.breaks += u64::from(p.f.breaks);
            l.notes += u64::from(p.f.n);
            l.percent += points(&p.f) as f64 / p.f.most.max(1) as f64;
            for (k, v) in [p.f.a[0], p.f.a[1], p.f.a[2], p.f.b, p.f.c].into_iter().enumerate() {
                l.parts[k] += v;
            }
        }
        let cleared: Vec<&Played> = plays.iter().filter(|p| !p.out && p.f.most > 0).collect();
        let scores: Vec<Vec<i64>> = cleared
            .iter()
            .map(|p| cands.iter().map(|c| c.score(&p.f)).collect())
            .collect();
        for i in 0..cleared.len() {
            for j in i + 1..cleared.len() {
                let (x, y) = (&cleared[i].f, &cleared[j].f);
                let (px, py) = (points(x), points(y));
                let lead = |a: i64, b: i64| 100.0 * (a - b) as f64 / x.most as f64;
                for (k, sum) in self.cands.iter_mut().enumerate() {
                    let (sx, sy) = (scores[i][k], scores[j][k]);
                    let today = scores[i][0].cmp(&scores[j][0]);
                    if today.is_ne() {
                        sum.pairs += 1;
                        sum.flips += u64::from(sx.cmp(&sy) == today.reverse());
                    }
                    // A full combo against a play with breaks.
                    if (x.breaks == 0) != (y.breaks == 0) {
                        let (broken, sf, sb, pf, pb) = if x.breaks == 0 {
                            (y, sx, sy, px, py)
                        } else {
                            (x, sy, sx, py, px)
                        };
                        let cell = &mut sum.fc[break_bin(broken.breaks)][bin(lead(pb, pf), &LEADS)];
                        cell.0 += 1;
                        cell.1 += u64::from(sb > sf);
                    }
                    // Fewer breaks against more points.
                    if x.breaks != y.breaks && px != py && (x.breaks < y.breaks) == (px < py) {
                        let (sc, sp, gap) = if x.breaks < y.breaks {
                            (sx, sy, lead(py, px))
                        } else {
                            (sy, sx, lead(px, py))
                        };
                        let cell = &mut sum.clean[bin(gap, &LEADS)];
                        cell.0 += 1;
                        cell.1 += u64::from(sc > sp);
                    }
                    // One no worse in either and better in one.
                    let better = |a: &Feat, pa: i64, b: &Feat, pb: i64| {
                        a.breaks <= b.breaks && pa >= pb && (a.breaks < b.breaks || pa > pb)
                    };
                    if better(x, px, y, py) {
                        sum.dominant.0 += 1;
                        sum.dominant.1 += u64::from(sy > sx);
                    } else if better(y, py, x, px) {
                        sum.dominant.0 += 1;
                        sum.dominant.1 += u64::from(sx > sy);
                    }
                }
            }
        }
        // A miss put into the plays, by its place.
        for p in &cleared {
            let taps = taps_of(&p.seq);
            if taps.len() < 40 || p.f.breaks > 5 {
                continue;
            }
            let other = points(&p.f) - seq_points(&p.seq);
            let after = |share: f64| {
                let at = place(&taps, share);
                // The nearest tap not already a break.
                let at = (0..taps.len())
                    .flat_map(|d| [at.saturating_sub(d), (at + d).min(p.seq.len() - 1)])
                    .find(|&i| p.seq[i] <= 2)
                    .unwrap_or(at);
                feat(&with_misses(&p.seq, &[at]), p.f.n, p.f.most, p.f.w0, other)
            };
            let unit = slip(p.f.most);
            if p.f.breaks == 0 {
                let feats: Vec<Feat> = PLACES.iter().map(|&s| after(s)).collect();
                for (c, sum) in cands.iter().zip(&mut self.cands) {
                    let was = c.score(&p.f);
                    for (l, f) in feats.iter().enumerate() {
                        sum.place[l] += (was - c.score(f)) as f64 / unit;
                    }
                    sum.full_combos += 1;
                }
            } else {
                let f = after(0.5);
                for (c, sum) in cands.iter().zip(&mut self.cands) {
                    sum.extra += (c.score(&p.f) - c.score(&f)) as f64 / unit;
                    sum.broken += 1;
                }
            }
        }
    }
}

/// A play's points at today's values.
fn points(f: &Feat) -> i64 {
    f.fixed + ddi::TAP_POINTS[usize::from(MISS)] * i64::from(f.misses)
}

fn simfiles(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut here: Vec<PathBuf> = Vec::new();
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            simfiles(&p, out);
        } else if let Some(ext) = p.extension() {
            let ext = ext.to_string_lossy().to_ascii_lowercase();
            if ["ssc", "sm", "dwi"].contains(&ext.as_str()) {
                here.push(p);
            }
        }
    }
    // One simfile a folder: `.ssc` before `.sm` before `.dwi`.
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

fn env_number(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Every dance-single chart as written of `files`, each level and seed.
fn play_files(files: &[PathBuf], seeds: u64, cands: &[Cand]) -> Sum {
    let workers = 4;
    let chunk = files.len().div_ceil(workers).max(1);
    let parts: Vec<Sum> = std::thread::scope(|scope| {
        let handles: Vec<_> = files
            .chunks(chunk)
            .map(|files| {
                scope.spawn(move || {
                    let mut sum = Sum::new(cands.len());
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
                            let mut plays = Vec::new();
                            for level in 0..10 {
                                for seed in 1..=seeds {
                                    let seed = seed
                                        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                                        .wrapping_add(level as u64 * 7919 + 1);
                                    plays.extend(solo(&plan, &song, index, level, seed));
                                }
                            }
                            if !plays.is_empty() {
                                sum.chart(&plays, cands);
                            }
                        }
                    }
                    sum
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut sum = Sum::new(cands.len());
    for p in &parts {
        sum.add(p);
    }
    sum
}

fn share(part: u64, of: u64) -> f64 {
    100.0 * part as f64 / of.max(1) as f64
}

fn print_plays(name: &str, sum: &Sum, cands: &[Cand]) {
    println!("== {name}: {} charts", sum.charts);
    println!("by level: plays, out %, of the clears: full combos %, breaks a play, notes a play, percent, the parts A50 A100 A200 B C");
    for (i, l) in sum.levels.iter().enumerate() {
        let clears = f64::from((l.plays - l.out).max(1));
        println!(
            "level {:>2}: {:>6} plays, out {:5.1}% | full combos {:5.1}% | breaks {:6.2} | notes {:5.0} | \
             percent {:5.1} | {:.3} {:.3} {:.3} {:.3} {:.3}",
            i + 1,
            l.plays,
            100.0 * f64::from(l.out) / f64::from(l.plays.max(1)),
            100.0 * f64::from(l.full_combos) / clears,
            l.breaks as f64 / clears,
            l.notes as f64 / clears,
            100.0 * l.percent / clears,
            l.parts[0] / clears,
            l.parts[1] / clears,
            l.parts[2] / clears,
            l.parts[3] / clears,
            l.parts[4] / clears,
        );
    }
    println!(
        "-- pairs of clears of one chart: turned around against today's score (tau = 1 - 2 x that); the \
         play with fewer breaks beats the one with more points, by that one's lead in percent points \
         ({}); a play with no more breaks and no fewer points loses",
        LEAD_NAMES[1..].join(" / ")
    );
    for (c, s) in cands.iter().zip(&sum.cands) {
        let clean: Vec<String> = s.clean[1..]
            .iter()
            .map(|&(n, w)| format!("{:5.1}", share(w, n)))
            .collect();
        println!(
            "{:<16} turned {:5.2}% of {} | fewer breaks wins {} % (of {} pairs) | the better play loses \
             {:6.3}% of {}",
            c.name(),
            share(s.flips, s.pairs),
            s.pairs,
            clean.join(" "),
            s.clean.iter().map(|c| c.0).sum::<u64>(),
            share(s.dominant.1, s.dominant.0),
            s.dominant.0,
        );
    }
    println!(
        "-- a full combo against a play with breaks: the full combo loses, % of the pairs, by the broken \
         play's lead ({}); pairs under today's row",
        LEAD_NAMES.join(" / ")
    );
    for (k, name) in BREAK_NAMES.iter().enumerate() {
        let n: Vec<String> = sum.cands[0].fc[k].iter().map(|c| format!("{:>6}", c.0)).collect();
        let all: u64 = sum.cands[0].fc[k].iter().map(|c| c.0).sum();
        println!("  {name}: pairs {} (all {all})", n.join(" "));
        for (c, s) in cands.iter().zip(&sum.cands) {
            let cells: Vec<String> = s.fc[k]
                .iter()
                .map(|&(n, l)| format!("{:6.1}", share(l, n)))
                .collect();
            let lost: u64 = s.fc[k].iter().map(|c| c.1).sum();
            println!("  {:<16} {} | all {:5.1}", c.name(), cells.join(" "), share(lost, all));
        }
    }
    println!(
        "-- a miss put into a full combo ({} of them), slips, at 2.5% / 10% / 25% / 50% / 75% / 90% / 97.5% \
         of the chart; and one more miss at the middle of a play of 1 to 5 breaks ({})",
        sum.cands[0].full_combos, sum.cands[0].broken
    );
    for (c, s) in cands.iter().zip(&sum.cands) {
        let cells: Vec<String> = s
            .place
            .iter()
            .map(|v| format!("{:6.1}", v / s.full_combos.max(1) as f64))
            .collect();
        println!(
            "{:<16} {} | one more {:6.1}",
            c.name(),
            cells.join(" "),
            s.extra / s.broken.max(1) as f64
        );
    }
}

/// Part 2: the CPUs' plays.
fn plays(cands: &[Cand]) {
    let seeds = env_number("DDI_COMBO_SEEDS", 4);
    let mut bundled = Vec::new();
    simfiles(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs"),
        &mut bundled,
    );
    print_plays(
        &format!("the bundled songs, {seeds} seeds a level"),
        &play_files(&bundled, seeds, cands),
        cands,
    );
    let Some(dirs) = std::env::var_os("DDI_COMBO_CORPUS") else {
        return;
    };
    let mut files = Vec::new();
    for dir in std::env::split_paths(&dirs) {
        simfiles(&dir, &mut files);
    }
    let every = env_number("DDI_COMBO_SAMPLE", 1).max(1) as usize;
    let files: Vec<PathBuf> = files.into_iter().step_by(every).collect();
    print_plays(
        &format!("the local folders ({} simfiles), {seeds} seeds a level", files.len()),
        &play_files(&files, seeds, cands),
        cands,
    );
}

/// A seat of a stage as the candidates read it.
struct Ranked {
    out: bool,
    life: f32,
    f: Feat,
    /// The same play with its shocks' harm taken away.
    healed: Feat,
    received: u32,
}

#[derive(Clone, Default)]
struct StageSum {
    stages: u32,
    seats_in: u32,
    /// Stages whose order of those still in differs from today's, and
    /// whose first place does.
    reordered: u32,
    winner: u32,
    pairs: u64,
    flips: u64,
    /// What the attacks received cost the score, and the attacks.
    cost: f64,
    received: u64,
}

/// The order of those still in: by life then score, or by score then
/// life under Adaptive.
fn order(seats: &[Ranked], c: &Cand, by_score: bool) -> Vec<usize> {
    let mut v: Vec<usize> = (0..seats.len()).filter(|&i| !seats[i].out).collect();
    v.sort_by(|&a, &b| {
        let (sa, sb) = (c.score(&seats[a].f), c.score(&seats[b].f));
        let life = seats[b].life.total_cmp(&seats[a].life);
        if by_score {
            sb.cmp(&sa).then(life)
        } else {
            life.then(sb.cmp(&sa))
        }
        .then(a.cmp(&b))
    });
    v
}

/// Part 3: the attack study's stages.
fn stages(cands: &[Cand]) {
    let seeds = env_number("DDI_COMBO_STAGE_SEEDS", 8);
    let songs: Vec<(Song, usize)> = super::study::CHARTS
        .iter()
        .map(|&(id, which)| {
            let song = crate::test_songs::bundled(id);
            let chart = super::study::chart_of(&song, which);
            (song, chart)
        })
        .collect();
    const FIELD: [u8; 8] = [0, 1, 2, 3, 4, 5, 7, 9];
    for (charts, attacks) in [
        (Charts::AsPicked, true),
        (Charts::AsPicked, false),
        (Charts::Adaptive, true),
        (Charts::Adaptive, false),
    ] {
        let by_score = charts == Charts::Adaptive;
        let mut sums = vec![StageSum::default(); cands.len()];
        // Steps up and down, and those a combo run carried across.
        let (mut ups, mut ups_carried, mut downs, mut downs_carried) = (0u32, 0u32, 0u32, 0u32);
        let (mut life_ties, mut in_pairs) = (0u64, 0u64);
        for (song, chart) in &songs {
            for seed in 1..=seeds {
                let rules = StageRules {
                    difficulty: MatchDifficulty::Normal,
                    charts,
                    ..StageRules::default()
                };
                let ruleset = stage_ruleset(&rules);
                let turn = (seed as usize) % FIELD.len();
                let levels: Vec<u8> = (0..FIELD.len())
                    .map(|i| FIELD[(i + turn) % FIELD.len()])
                    .collect();
                let mut setup =
                    super::study::cpu_practice(seed, &levels, rules, None, Targeting::Spread);
                if !attacks {
                    setup.battle = BattlePreset::GaugeSurvivalV0;
                }
                let b = super::played_out_forfeit(setup, song, *chart, &ruleset).unwrap();
                let table = &ruleset.judge;
                let (lock, reach) = (table.lock.unwrap_or(0.0), table.miss_after());
                let seats: Vec<Ranked> = b
                    .seats
                    .iter()
                    .map(|s| {
                        let p = s.opponent.player();
                        let view = s.opponent.score();
                        let d = view.ddi.expect("DDI");
                        let results = p.results();
                        let hits: Vec<(u8, f64)> = s
                            .feed
                            .moments()
                            .iter()
                            .filter(|m| m.kind == feed::MomentKind::ShockHit)
                            .map(|m| (m.lane, m.at))
                            .collect();
                        // Each lane's missed notes in order: was it missed
                        // in a lane a shock had locked?
                        let lanes = usize::from(p.lanes());
                        let mut locked: Vec<Vec<bool>> = vec![Vec::new(); lanes];
                        for n in p.judge().notes() {
                            if n.state == NoteState::Missed && n.note.kind.is_tap_like() {
                                locked[usize::from(n.note.lane)].push(hits.iter().any(|&(lane, at)| {
                                    lane == n.note.lane
                                        && n.seconds > at - reach
                                        && n.seconds < at + lock + reach
                                }));
                            }
                        }
                        let mut missed = vec![0usize; lanes];
                        let (mut seq, mut healed) = (Seq::new(), Seq::new());
                        let mut runs: Vec<(f64, f64)> = Vec::new();
                        let mut run: Option<(f64, f64)> = None;
                        for j in &results.timeline {
                            let t = f64::from(j.time);
                            let code = match j.kind {
                                JudgeEventKind::Tap(tier, _) => tier.index() as u8,
                                JudgeEventKind::HitMine | JudgeEventKind::HitShock => BROKE,
                                _ => continue,
                            };
                            seq.push(code);
                            if code <= 2 {
                                run = Some(run.map_or((t, t), |(a, _)| (a, t)));
                            } else {
                                runs.extend(run.take());
                            }
                            let lane = usize::from(j.lane);
                            let shock = code == BROKE
                                && hits
                                    .iter()
                                    .any(|&(l, at)| usize::from(l) == lane && (at - t).abs() < 0.002);
                            let in_lock = code == MISS && {
                                let k = missed[lane];
                                missed[lane] += 1;
                                locked[lane].get(k).copied().unwrap_or(false)
                            };
                            if in_lock {
                                healed.push(0);
                            } else if !shock {
                                healed.push(code);
                            }
                        }
                        runs.extend(run.take());
                        for st in p.steps() {
                            let carried =
                                runs.iter().any(|&(a, z)| a < st.boundary.min(st.at) && z > st.boundary);
                            if st.up {
                                ups += 1;
                                ups_carried += u32::from(carried);
                            } else {
                                downs += 1;
                                downs_carried += u32::from(carried);
                            }
                        }
                        let n = p.score_ctx().steps;
                        let w0 = p.tally().w0;
                        let other = d.points - seq_points(&seq);
                        let f = feat(&seq, n, d.most, w0, other);
                        assert_eq!(cands[0].score(&f), d.score, "the study's score is the engine's");
                        Ranked {
                            out: s.out_at().is_some(),
                            life: s.opponent.life(),
                            healed: feat(&healed, n, d.most, w0, other),
                            f,
                            received: b
                                .attacks
                                .as_ref()
                                .map_or(0, |a| a.ledger().stats(s.seat.participant).received),
                        }
                    })
                    .collect();
                let today = order(&seats, &cands[0], by_score);
                for (x, &a) in today.iter().enumerate() {
                    for &b in &today[x + 1..] {
                        in_pairs += 1;
                        life_ties += u64::from(seats[a].life == seats[b].life);
                    }
                }
                for (c, sum) in cands.iter().zip(&mut sums) {
                    let got = order(&seats, c, by_score);
                    sum.stages += 1;
                    sum.seats_in += got.len() as u32;
                    sum.reordered += u32::from(got != today);
                    sum.winner += u32::from(got.first() != today.first());
                    let at = |v: &[usize], i: usize| v.iter().position(|&x| x == i).unwrap();
                    for (x, &a) in today.iter().enumerate() {
                        for &b in &today[x + 1..] {
                            sum.pairs += 1;
                            sum.flips += u64::from(at(&got, a) > at(&got, b));
                        }
                    }
                    for s in seats.iter().filter(|s| !s.out && s.received > 0) {
                        sum.cost += (c.score(&s.healed) - c.score(&s.f)) as f64;
                        sum.received += u64::from(s.received);
                    }
                }
            }
        }
        println!(
            "== stages of eight CPUs (levels 1, 2, 3, 4, 5, 6, 8, 10), DDI Normal, Charts {charts:?}, attacks \
             {}: {} stages, {:.1} still in at the end, {:.1}% of their pairs level on life; steps up {ups} \
             ({ups_carried} with a combo run across the boundary), down {downs} ({downs_carried})",
            if attacks { "on" } else { "off" },
            sums[0].stages,
            f64::from(sums[0].seats_in) / f64::from(sums[0].stages.max(1)),
            share(life_ties, in_pairs),
        );
        let today_cost = sums[0].cost / sums[0].received.max(1) as f64;
        for (c, s) in cands.iter().zip(&sums) {
            let cost = s.cost / s.received.max(1) as f64;
            println!(
                "{:<16} order differs {:5.1}% of stages | first place differs {:5.1}% | pairs turned {:5.2}% \
                 | an attack received costs {:8.0} of score, {:5.2} x today's",
                c.name(),
                100.0 * f64::from(s.reordered) / f64::from(s.stages.max(1)),
                100.0 * f64::from(s.winner) / f64::from(s.stages.max(1)),
                share(s.flips, s.pairs),
                cost,
                cost / today_cost.max(1e-9),
            );
        }
    }
}

#[test]
fn the_combo_parts_are_what_they_say() {
    // One run of the whole chart is 1 everywhere; two halves are half of
    // B and of C; a miss costs A about half its ramp.
    let full = feat(&[0; 400], 400, 20_000, 0, 0);
    assert_eq!((full.a, full.b, full.c), ([1.0; 3], 1.0, 1.0));
    let mut seq = vec![0u8; 401];
    seq[200] = MISS;
    let halves = feat(&seq, 401, 20_050, 0, 0);
    assert!((halves.b - 0.5).abs() < 0.01 && (halves.c - 0.5).abs() < 0.01);
    let lost = (1.0 - halves.a[1]) * full_ramp(401, 100);
    assert!((lost - 5050.0).abs() < 1.0, "{lost}");
    assert_eq!(halves.breaks, 1);
    // Today's score is the engine's sum.
    let today = cands()[0];
    assert_eq!(today.score(&full), ddi::DdiScoreView::of(20_000, 20_000, 0).score);
    assert_eq!(
        today.score(&halves),
        ddi::DdiScoreView::of(20_000 - 10, 20_050, 0).score
    );
}

#[test]
#[ignore = "a study: prints what combo parts would do to DDI's score"]
fn combo_score_study() {
    let cands = cands();
    let only = std::env::var("DDI_COMBO_ONLY").ok();
    let wanted = |part: &str| only.as_deref().is_none_or(|o| o == part);
    if wanted("made-up") {
        made_up(&cands);
    }
    if wanted("plays") {
        plays(&cands);
    }
    if wanted("stages") {
        stages(&cands);
    }
}
