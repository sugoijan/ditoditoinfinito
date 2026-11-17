//! Opt-in study: putting combos into DDI's score
//! (`docs/plans/ddi-ruleset.md`, "DDI's score and combos: a study" and
//! "Combos count"). It reads plays as the engine judged them and works
//! candidate combo parts out beside the score without one; the engine's
//! own part (`ddi-combo-v0`, [`ENGINE`]) is asserted equal to its
//! candidate's on every CPU play.
//!
//! `cargo test --release -p ddi-session --lib combo_score_study --
//! --ignored --nocapture` prints aggregates only, never a song's name:
//!
//! 1. made-up full combos of 100 to 2,400 notes: each candidate's part,
//!    its share of the score, what broken in two loses of it, what one
//!    miss costs by where it falls (the first note, 2.5%, a quarter, the
//!    middle, the last) and 2, 5 and 10 misses spread evenly and at
//!    random, in *slips* (a W1 that became a W2: 10 points);
//! 2. the CPUs at each of the ten absolute levels (`cpu-levels-v1`,
//!    `cpu-play-v1`: the chart a person is given) on every dance-single
//!    chart as written of the bundled songs and of the folders under
//!    `DDI_COMBO_CORPUS` (`:` between, read in place; every
//!    `DDI_COMBO_SAMPLE`th simfile, 1), `DDI_COMBO_SEEDS` seeds a level
//!    (4), DDI Normal, the fail off, plays whose gauge emptied left out:
//!    by level the full combos and each per-note candidate's part over a
//!    full combo's; over every pair of plays of one chart, how each
//!    candidate reorders them, how often a full combo loses to a play
//!    with breaks (by the breaks and by how much more accurate the broken
//!    play is), how often the play with fewer breaks beats the more
//!    accurate one, and how often a play with no more breaks and more
//!    points loses (the position's unfairness); and on the full combos,
//!    what one miss put in costs by its place;
//! 3. the attack study's stages (`attack_rates`: eight CPUs of mixed
//!    levels on its six bundled charts, `DDI_COMBO_STAGE_SEEDS` seeds,
//!    8), As picked and Adaptive, with attacks and without: how often
//!    each candidate changes the final order and the winner, what an
//!    attack received costs in score, and whether a combo run carries
//!    across a step between charts;
//! 4. `ladder`: playing the next easier chart on purpose under Adaptive,
//!    a perfect play of it against the stage's chart at 95% and 85%, in
//!    one run or broken;
//! 5. `kept`: the maintainer's kept plays, only where
//!    `DDI_COMBO_PLAYS=<the desktop's plays/ folder>` names them (read in
//!    place, read-only; aggregates only, never a play's own numbers):
//!    where breaks fall, the part's share, pairs of one chart's plays
//!    reordered and bests changing hands. Plays kept under other rules
//!    are put on DDI's grid by their taps' offsets (the charts are not
//!    read): an estimate, said as such.
//!
//! 6. `percent` (2026-11-17, [`percent`]): what the grades would do if
//!    the percent counted the combo part, or the tiers' points were
//!    flatter: worked examples, then the CPUs over the same charts as
//!    part 2 and the kept plays (where `DDI_COMBO_PLAYS` names them), the
//!    grades' shares and moves by level, by kind of play and by length.
//!    `DDI_COMBO_THREADS` (4) threads.
//!
//! 7. `distributions` (2026-11-17, [`percent::distributions`]): the
//!    percent each baseline ruleset and DDI (as it is, with a heavier
//!    miss, with the combo in it, both) gives every kept play, written as
//!    aggregates to the JSON file `DDI_PCT_DIST_OUT` names, from the plays
//!    `DDI_COMBO_PLAYS` names.
//!
//! 8. `built` (round U, 2026-11-17, [`percent::distributions::built`]):
//!    round U's rule as the engine plays it over the kept plays' clears
//!    (`DDI_COMBO_PLAYS`), beside the series it was chosen from;
//!    aggregates only.
//!
//! `DDI_COMBO_ONLY=made-up|plays|stages|ladder|kept|percent|distributions|built`
//! runs one part; every part but `percent`, `distributions` and `built`
//! runs when it is not set.
//!
//! The candidates: the score without a combo part ("today"), the first
//! study's pick at 20% of the million ("A100+B": half a ramp to 100, half
//! the runs' squares, 200,000 for a full combo on every chart), and the
//! per-note ones of round P1a ([`PER`]), each note in combo adding
//! `(a × min(k, 100) + b × grow(k)) / d`, weighted so a full combo of 300
//! notes is about 200,000. The percent is today's under all of them.
//!
//! MEASUREMENTS OF CPUS, not of people.
//!
//! **Which rule each part measures (since round U, 2026-11-17, room model
//! revision 13).** The study was made before round U and its "today" is
//! the rule before it: a Miss −10 points, a mine or shock stepped on −10
//! ([`TAP_POINTS`], [`MINE_POINTS`], kept here), and the percent the
//! accuracy alone with the letters SSS 99, SS 96, S 89, A 80, B 68, C 55
//! (`percent::TODAY_GRADES`). The engine now plays round U's rule
//! (`ddi::TAP_POINTS`'s Miss −120, `ddi::MINE_POINTS` −50,
//! `ddi::PERCENT_RULE`, `ddi::GRADES`): every play the engine judges is
//! read back on the old rule ([`before_u`]: its misses and its mines or
//! shocks stepped on priced as before; nothing else differs, life never
//! reading points), so parts 1 to 7 and their candidates and series
//! measure against the rule before round U, as when they were written,
//! and their checks against the engine hold on that reading. Part 8,
//! `built` ([`percent::distributions::built`]), measures round U's rule
//! itself: the engine's own percent and letters over the kept plays
//! beside the series it was chosen from (`ddi-miss3.4-combo10`).

use super::*;
use ddi_engine::cpu::{CPU_LEVELS_V1, CPU_PLAY, MINE_RESPONSE_V0, Plan, Skills, Strength};
use ddi_engine::judge::{JudgeEventKind, NoteState};
use ddi_engine::rules::FailPolicy;
use ddi_engine::rules::ddi::{self, DdiOptions};
use ddi_room::{Charts, MatchDifficulty};
use std::path::{Path, PathBuf};

mod levels;
mod percent;

/// A play's judgments in order: a tap's tier (`Judgement::index`, W1 0 to
/// Miss 5), or [`BROKE`], a mine or a shock stepped on.
type Seq = Vec<u8>;
const BROKE: u8 = 6;
const MISS: u8 = 5;

/// The points of a tap-like note by tier, W1 to Miss, under the rule this
/// study measures as "today": DDI's before round U (2026-11-17), a Miss
/// −10. The engine's are `ddi::TAP_POINTS` (a Miss −120).
pub(super) const TAP_POINTS: [i64; 6] = [50, 40, 25, 10, 0, -10];
/// A mine or a shock stepped on before round U: −10 (the engine's
/// `ddi::MINE_POINTS`, −50, since).
pub(super) const MINE_POINTS: i64 = -10;

/// An engine's DDI view as the rule before round U has it: the misses and
/// the mines or shocks stepped on of `seq` (the play's judgments) priced
/// as [`TAP_POINTS`] and [`MINE_POINTS`] price them, the rest as it is
/// (the combo part and the most do not read points).
fn before_u(d: ddi::DdiScoreView, seq: &[u8]) -> ddi::DdiScoreView {
    let count = |c: u8| seq.iter().filter(|&&x| x == c).count() as i64;
    let points = d.points
        - (ddi::TAP_POINTS[5] - TAP_POINTS[5]) * count(MISS)
        - (ddi::MINE_POINTS - MINE_POINTS) * count(BROKE);
    let w0 = (d.absolute - 100 * d.points).clamp(0, i64::from(u32::MAX)) as u32;
    ddi::DdiScoreView::of(points, d.most, w0).with_combo(d.combo)
}
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
    /// The per-note candidates' sums ([`PER`]), before their divisor.
    per: [i64; NP],
}

/// How a per-note candidate's second term grows with the combo `k`.
#[derive(Clone, Copy, PartialEq)]
enum Grow {
    /// `isqrt(k)`.
    Sqrt,
    /// `ilog2(k)`.
    Log2,
    /// `min(k, 1000)`.
    Lin1000,
    /// `k`, without a cap.
    Lin,
    /// `isqrt(k - 100)` past combo 100, else 0.
    SqrtOver100,
}

fn grow(g: Grow, k: u64) -> i64 {
    let k = k as i64;
    match g {
        Grow::Sqrt => k.isqrt(),
        Grow::Log2 => i64::from(k.max(1).ilog2()),
        Grow::Lin1000 => k.min(1000),
        Grow::Lin => k,
        Grow::SqrtOver100 => (k - 100).max(0).isqrt(),
    }
}

/// A per-note candidate (round P1a): each note judged in combo adds
/// `(a × min(k, 100) + b × grow(k)) / d`, `k` the combo after it, summed
/// in whole numbers and divided once at the end; nothing divided by the
/// chart's size. Each is weighted so a full combo of 300 notes is about
/// 200,000, the study's "A100+B at 20% M".
struct Per {
    name: &'static str,
    a: i64,
    b: i64,
    g: Grow,
    d: i64,
}

const NP: usize = 6;
const PER: [Per; NP] = [
    Per { name: "R+sqrt", a: 4, b: 30, g: Grow::Sqrt, d: 1 },
    Per { name: "R+log2", a: 4, b: 53, g: Grow::Log2, d: 1 },
    Per { name: "R+lin<=1000", a: 39, b: 22, g: Grow::Lin1000, d: 10 },
    Per { name: "R+lin", a: 39, b: 22, g: Grow::Lin, d: 10 },
    Per { name: "R+sqrt>100", a: 6, b: 28, g: Grow::SqrtOver100, d: 1 },
    Per { name: "R+sqrt>100 x2", a: 4, b: 56, g: Grow::SqrtOver100, d: 1 },
];

/// The candidate the engine plays (`ddi::COMBO_SCORE`, `ddi-combo-v0`):
/// its part is asserted equal to the engine's on every play.
const ENGINE: usize = 4;

fn per_note(p: &Per, k: u64) -> i64 {
    p.a * (k.min(100) as i64) + p.b * grow(p.g, k)
}

/// A per-note candidate's part for a full combo of `n` notes.
fn full_per(p: &Per, n: u64) -> i64 {
    (1..=n).map(|k| per_note(p, k)).sum::<i64>() / p.d
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
            for (sum, p) in f.per.iter_mut().zip(&PER) {
                *sum += per_note(p, combo);
            }
            f.fixed += TAP_POINTS[usize::from(c)];
            continue;
        }
        match c {
            MISS => f.misses += 1,
            BROKE => f.fixed += MINE_POINTS,
            _ => f.fixed += TAP_POINTS[usize::from(c)],
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
            BROKE => MINE_POINTS,
            c => TAP_POINTS[usize::from(c)],
        })
        .sum()
}

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Today,
    /// The first study's "A100+B": half the ramp to 100, half the runs'
    /// squares, a share of a scale.
    AB,
    /// One of [`PER`].
    Per(usize),
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
            Shape::AB => String::from("A100+B"),
            Shape::Per(i) => return PER[i].name.into(),
        };
        format!(
            "{shape} {:.0}% {}",
            self.s * 100.0,
            if self.full { "F" } else { "M" }
        )
    }

    fn score(&self, f: &Feat) -> i64 {
        let base = ddi::DdiScoreView::of(points(f), f.most, f.w0).score;
        let part = match self.shape {
            Shape::Today => return base,
            Shape::Per(i) => return base + f.per[i] / PER[i].d,
            Shape::AB => (f.a[1] + f.b) / 2.0,
        };
        let scale = if self.full {
            1e6 + 100.0 * f.most as f64
        } else {
            1e6
        };
        base + (self.s * scale * part).floor() as i64
    }
}

/// Today's score first, then the study's pick ("A100+B" at 20% of the
/// million, what the maintainer chose the shape and weight of), then the
/// per-note candidates of round P1a ([`PER`]).
fn cands() -> Vec<Cand> {
    let mut v = vec![
        Cand {
            shape: Shape::Today,
            s: 0.0,
            full: false,
        },
        Cand {
            shape: Shape::AB,
            s: 0.2,
            full: false,
        },
    ];
    v.extend((0..NP).map(|i| Cand {
        shape: Shape::Per(i),
        s: 0.0,
        full: false,
    }));
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

/// The places one miss is put at in a made-up full combo: its first
/// note, 2.5%, a quarter, the middle and its last note.
const MADE_UP_PLACES: [f64; 5] = [0.0, 0.025, 0.25, 0.5, 1.0];

/// Part 1: made-up full combos, every tap a W1.
fn made_up(cands: &[Cand]) {
    for n in [100u32, 300, 600, 1200, 2400] {
        let most = 50 * i64::from(n);
        let seq = vec![0u8; n as usize];
        let taps = taps_of(&seq);
        let base = feat(&seq, n, most, 0, 0);
        // What a percent point of accuracy is worth in score here.
        let point = 10_000.0 + 50.0 * f64::from(n);
        println!(
            "== a full combo of {n} notes, every tap W1: the combo part, its share of the score, what \
             broken in two loses of it; what misses cost, in slips (a slip {:.0} points of score, {:.3} \
             percent points); a percent point is {point:.0} of score",
            slip(most),
            20.0 / f64::from(n)
        );
        println!(
            "{:<16} part, share of score | halves lose | one miss at first / 2.5% / 25% / 50% / last \
             (middle in percent points) | 2 even, random | 5 even, random | 10 even, random",
            ""
        );
        let today = cands[0].score(&base);
        for c in cands {
            let full = c.score(&base);
            let part = full - today;
            let cost = |at: &[usize]| {
                let f = feat(&with_misses(&seq, at), n, most, 0, 0);
                (full - c.score(&f)) as f64 / slip(most)
            };
            let middle = place(&taps, 0.5);
            let halves = {
                let f = feat(&with_misses(&seq, &[middle]), n, most, 0, 0);
                let lost = part - (c.score(&f) - cands[0].score(&f));
                if part > 0 { 100.0 * lost as f64 / part as f64 } else { 0.0 }
            };
            let one: Vec<String> = MADE_UP_PLACES
                .iter()
                .map(|&p| format!("{:6.1}", cost(&[place(&taps, p)])))
                .collect();
            let middle_points = cost(&[middle]) * slip(most) / point;
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
            println!(
                "{:<16} {:>9} {:5.1}% | {:5.1}% | {} ({:4.2} pt) | {}",
                c.name(),
                part,
                100.0 * part as f64 / full.max(1) as f64,
                halves,
                one.join(" "),
                middle_points,
                many.join(" | ")
            );
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
    /// What part 6 reads of it.
    pp: percent::Pp,
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
    let d = before_u(view.ddi.expect("DDI"), &seq);
    let f = feat(
        &seq,
        p.score_ctx().steps,
        d.most,
        p.tally().w0,
        d.points - seq_points(&seq),
    );
    // The features read the play as the engine scored it.
    debug_assert_eq!(Cand { shape: Shape::Today, s: 0.0, full: false }.score(&f), d.score - d.combo);
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
    let d = before_u(o.score().ddi.unwrap(), &seq);
    assert_eq!(
        Cand { shape: Shape::Today, s: 0.0, full: false }.score(&f),
        d.score - d.combo,
        "the study's score is the engine's"
    );
    assert_eq!(f.per[ENGINE] / PER[ENGINE].d, d.combo, "the engine's combo part is the candidate's");
    let tally = o.player().tally();
    let moments: Vec<percent::Moment> = o
        .player()
        .results()
        .timeline
        .iter()
        .filter_map(|j| match j.kind {
            JudgeEventKind::Tap(t, _) if t.index() as u8 == MISS => Some((MISS, None)),
            JudgeEventKind::Tap(t, delta) => Some((t.index() as u8, Some(delta))),
            JudgeEventKind::HitMine | JudgeEventKind::HitShock => Some((BROKE, None)),
            _ => None,
        })
        .collect();
    let (base, base_grade) = percent::baselines(&moments);
    let pp = percent::Pp {
        n: f.n,
        points: d.points,
        most: d.most,
        combo: d.combo,
        max: ddi::full_combo_points(f.n),
        tiers: percent::Pp::tiers_of(tally),
        dist: ddi::distance(tally),
        ad: ddi::absolute(tally),
        breaks: f.breaks,
        base,
        base_grade,
    };
    let out = o.out_at().is_some();
    if !out {
        // Round U's rule worked out here from the play read on the old
        // one is the engine's own percent and grade.
        let mines = seq.iter().filter(|&&c| c == BROKE).count() as u32;
        let built = o.score().ddi.unwrap();
        assert_eq!(percent::built_percent(&pp, mines), built.percent, "the built percent is the engine's");
        assert_eq!(
            percent::grade_index(&o.player().results_brief().grade),
            Some(percent::built_grade(&pp, mines)),
            "the built grade is the engine's"
        );
    }
    Some(Played { level, seq, f, out, pp })
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
    /// Each per-note candidate's part over a full combo's of the notes.
    parts: [f64; NP],
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
            for k in 0..NP {
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
            for (k, per) in PER.iter().enumerate() {
                let full = full_per(per, u64::from(p.f.n));
                l.parts[k] += if full > 0 { (p.f.per[k] / per.d) as f64 / full as f64 } else { 0.0 };
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
    f.fixed + TAP_POINTS[usize::from(MISS)] * i64::from(f.misses)
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
    let parts = per_chart(files, seeds, || Sum::new(cands.len()), |sum, plays| sum.chart(plays, cands));
    let mut sum = Sum::new(cands.len());
    for p in &parts {
        sum.add(p);
    }
    sum
}

/// Each dance-single chart as written of `files` played by the CPUs at
/// each level and seed, handed to `each` with a sum of its thread's.
fn per_chart<S: Send>(
    files: &[PathBuf],
    seeds: u64,
    new: impl Fn() -> S + Sync,
    each: impl Fn(&mut S, &[Played]) + Sync,
) -> Vec<S> {
    let workers = env_number("DDI_COMBO_THREADS", 4).max(1) as usize;
    let chunk = files.len().div_ceil(workers).max(1);
    let (new, each) = (&new, &each);
    std::thread::scope(|scope| {
        let handles: Vec<_> = files
            .chunks(chunk)
            .map(|files| {
                scope.spawn(move || {
                    let mut sum = new();
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
                                each(&mut sum, &plays);
                            }
                        }
                    }
                    sum
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    })
}

fn share(part: u64, of: u64) -> f64 {
    100.0 * part as f64 / of.max(1) as f64
}

fn print_plays(name: &str, sum: &Sum, cands: &[Cand]) {
    println!("== {name}: {} charts", sum.charts);
    println!(
        "by level: plays, out %, of the clears: full combos %, breaks a play, notes a play, percent, each \
         per-note candidate's part over a full combo's ({})",
        PER.map(|p| p.name).join(" ")
    );
    for (i, l) in sum.levels.iter().enumerate() {
        let clears = f64::from((l.plays - l.out).max(1));
        println!(
            "level {:>2}: {:>6} plays, out {:5.1}% | full combos {:5.1}% | breaks {:6.2} | notes {:5.0} | \
             percent {:5.1} | {}",
            i + 1,
            l.plays,
            100.0 * f64::from(l.out) / f64::from(l.plays.max(1)),
            100.0 * f64::from(l.full_combos) / clears,
            l.breaks as f64 / clears,
            l.notes as f64 / clears,
            100.0 * l.percent / clears,
            l.parts.map(|v| format!("{:.3}", v / clears)).join(" "),
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
                        // Read on the rule before round U once its
                        // judgments are known (`seq`, below).
                        let new = d;
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
                        let d = before_u(new, &seq);
                        let other = d.points - seq_points(&seq);
                        let f = feat(&seq, n, d.most, w0, other);
                        assert_eq!(cands[0].score(&f), d.score - d.combo, "the study's score is the engine's");
                        assert_eq!(f.per[ENGINE] / PER[ENGINE].d, d.combo, "the engine's combo part");
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

/// What one chart is to the "easier chart on purpose" part: its notes
/// and its most.
fn notes_and_most(song: &Song, chart: usize) -> Option<(u32, i64)> {
    let ruleset = stage_ruleset(&StageRules::default());
    song.charts.get(chart)?;
    let p = ddi_engine::Player::new(song, chart, ruleset.fresh(), Default::default());
    let most = p.ruleset().score.view().ddi?.most;
    Some((p.score_ctx().steps, most))
}

/// A made-up play of `n` notes worth `share` of `most`, every tap in
/// combo but `breaks` misses spread evenly (its points `share` of the
/// most whatever they are).
fn made_up_play(n: u32, most: i64, share: f64, breaks: usize) -> Feat {
    let seq = vec![0u8; n as usize];
    let at: Vec<usize> = (1..=breaks).map(|j| j * seq.len() / (breaks + 1)).collect();
    let mut f = feat(&with_misses(&seq, &at), n, most, 0, 0);
    let target = (most as f64 * share).round() as i64;
    f.fixed = target - TAP_POINTS[usize::from(MISS)] * i64::from(f.misses);
    f
}

/// The harder plays the easier rung's perfect play is set against:
/// a share of the harder chart's most and its breaks.
const AGAINST: [(f64, usize); 6] = [(0.95, 0), (0.95, 1), (0.95, 3), (0.85, 0), (0.85, 1), (0.85, 3)];

#[derive(Clone, Default)]
struct UnderSum {
    charts: u32,
    notes: (u64, u64),
    /// By candidate and [`AGAINST`]: the easier perfect play wins; the
    /// ratio of its score to the harder play's.
    wins: Vec<[u32; 6]>,
    ratio: Vec<[f64; 6]>,
}

/// Part 4: playing an easier chart on purpose under Adaptive (a stage
/// ranked by score): the next easier rung played perfectly, every tap a
/// W1 in one run, against the stage's chart at 95% and 85% of its most,
/// in one run, broken once, broken three times.
fn under(cands: &[Cand], files: &[PathBuf]) -> UnderSum {
    let mut sum = UnderSum {
        wins: vec![[0; 6]; cands.len()],
        ratio: vec![[0.0; 6]; cands.len()],
        ..UnderSum::default()
    };
    for path in files {
        let Some(ext) = path.extension() else { continue };
        let ext = ext.to_string_lossy().to_ascii_lowercase();
        let Ok(bytes) = std::fs::read(path) else { continue };
        let text = ddi_library::pack::decode_text(&bytes);
        let Ok(song) = ddi_library::scan::parse_simfile(&text, &ext) else {
            continue;
        };
        for (index, chart) in song.charts.iter().enumerate() {
            if chart.layout != "dance-single" || chart.derivation.is_some() || chart.notes.is_empty() {
                continue;
            }
            let ladder = ddi_engine::ladder::Ladder::of(&song, index);
            let Some(&easier) = ladder.below.first() else { continue };
            let (Some((ne, me)), Some((nh, mh))) =
                (notes_and_most(&song, easier), notes_and_most(&song, index))
            else {
                continue;
            };
            if ne == 0 || nh == 0 {
                continue;
            }
            sum.charts += 1;
            sum.notes.0 += u64::from(ne);
            sum.notes.1 += u64::from(nh);
            let perfect = made_up_play(ne, me, 1.0, 0);
            for (k, &(share, breaks)) in AGAINST.iter().enumerate() {
                let harder = made_up_play(nh, mh, share, breaks);
                for (c, cand) in cands.iter().enumerate() {
                    let (p, h) = (cand.score(&perfect), cand.score(&harder));
                    sum.wins[c][k] += u32::from(p > h);
                    sum.ratio[c][k] += p as f64 / h.max(1) as f64;
                }
            }
        }
    }
    sum
}

fn print_under(name: &str, sum: &UnderSum, cands: &[Cand]) {
    let n = f64::from(sum.charts.max(1));
    println!(
        "== playing the next easier chart on purpose ({name}): {} charts with one below, {:.0} notes on \
         the easier, {:.0} on the stage's; how often a perfect play of the easier (one run, every tap W1) \
         beats the stage's chart at 95% (one run / one break / three) and at 85% (the same), and the \
         mean ratio of the scores",
        sum.charts,
        sum.notes.0 as f64 / n,
        sum.notes.1 as f64 / n,
    );
    for (c, cand) in cands.iter().enumerate() {
        let cells: Vec<String> = (0..6)
            .map(|k| format!("{:5.1}% {:.3}", 100.0 * f64::from(sum.wins[c][k]) / n, sum.ratio[c][k] / n))
            .collect();
        println!("{:<16} {}", cand.name(), cells.join(" | "));
    }
}

/// The DDI tier (W1 0 … W5 4, Miss 5) of a tap `delta` song seconds off
/// at music rate `rate` on `difficulty`'s grid: what the maintainer's
/// plays kept under other rules are put on (an estimate: the chart is
/// not read, so a press DDI would have matched otherwise, a locked lane
/// and Boos are not known).
fn ddi_tier(delta: f64, rate: f64, difficulty: ddi::Difficulty) -> u8 {
    let real = (delta / rate.max(1e-6)).abs();
    for (t, &units) in ddi::TIER_UNITS[1..].iter().enumerate() {
        if real <= difficulty.seconds(units) + 1e-9 {
            return t as u8;
        }
    }
    MISS
}

/// One of the maintainer's kept plays as the study reads it.
struct Kept {
    /// The chart and the DDI options it was judged under, as one key.
    group: String,
    seq: Seq,
    f: Feat,
    clear: bool,
    counts: bool,
    /// What part 6 reads of it: the combo part the judge kept where it
    /// kept one (`JUDGE_VERSION` 5), else worked out from `seq`.
    pp: percent::Pp,
    /// Its grade as kept, by part 6's index.
    grade: Option<usize>,
    combo_kept: bool,
    /// What part 7 ([`percent::distributions`]) reads too: who played it,
    /// how it ended, DDI's counts, every judged tap and mine in order for
    /// the baselines, the chart's notes as one key, and the percent its
    /// own ruleset kept where that was not DDI.
    autoplay: bool,
    ending: ddi_ui::state::history::Ending,
    tally: ddi_engine::rules::Tally,
    moments: Vec<percent::Moment>,
    notes_key: String,
    /// The ruleset it was played under, and what that one kept where it
    /// was not DDI.
    played: (String, Option<ddi_engine::rules::ScoreView>),
    /// The DDI difficulty it was judged on.
    difficulty: ddi::Difficulty,
    /// What the levels' study ([`levels`]) reads too: each moment's
    /// note's second (a tap's press less its offset, in real seconds at
    /// the play's rate), when it was
    /// played (ms since the epoch), the audio device it was heard on (a
    /// key to group by, never shown) and the audio offset in effect.
    times: Vec<f64>,
    when: f64,
    audio: String,
    audio_offset: f64,
}

#[derive(Default)]
struct KeptRead {
    files: u32,
    unread: u32,
    no_ddi: u32,
    as_played: u32,
    rejudged: u32,
    /// Of the plays judged again: their tiers put on DDI's grid matched
    /// the second judgment's counts exactly; the mean absolute difference
    /// of the six counts summed, per play.
    exact: u32,
    diff: u64,
}

/// The kept plays in `dir` (the desktop's `plays/`, read in place).
fn read_kept(dir: &Path) -> (Vec<Kept>, KeptRead) {
    use ddi_engine::rules::Judgement;
    use ddi_ui::state::history::{Ending, PlayRecord, PlaySummary};
    let mut r = KeptRead::default();
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (out, r);
    };
    let mut names: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    names.sort();
    for path in names {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if !name.ends_with(".json") || name.ends_with(".summary.json") {
            continue;
        }
        r.files += 1;
        let read = || -> Option<(PlayRecord, PlaySummary, ddi_engine::history::PlayTiming)> {
            let entry = ddi_platform::history::PlayEntry::from_json(&std::fs::read_to_string(&path).ok()?).ok()?;
            let record: PlayRecord = serde_json::from_str(&entry.record).ok()?;
            let summary_path = path.with_file_name(name.replace(".json", ".summary.json"));
            let summary: PlaySummary = serde_json::from_str(&std::fs::read_to_string(summary_path).ok()?).ok()?;
            let timing = ddi_engine::history::PlayTiming::decode(&entry.timing).ok()?;
            Some((record, summary, timing))
        };
        let Some((record, summary, timing)) = read() else {
            r.unread += 1;
            continue;
        };
        // Its DDI judgment: as played, or judged again under DDI.
        let played = record.ruleset.id == ddi::ID;
        let (key, worked) = if played {
            (record.ruleset.clone(), summary.worked.clone())
        } else {
            match &summary.rejudged {
                Some(j) if j.ruleset.id == ddi::ID => (j.ruleset.clone(), j.worked.clone()),
                _ => (record.ruleset.clone(), None),
            }
        };
        let Some(worked) = worked else {
            r.no_ddi += 1;
            continue;
        };
        let Some(new) = worked.score.ddi else {
            r.no_ddi += 1;
            continue;
        };
        let difficulty = key.ddi.unwrap_or_default().difficulty;
        let rate = record.options.clock.rate;
        let seq: Seq = timing
            .timeline
            .iter()
            .filter_map(|j| match j.kind {
                JudgeEventKind::Tap(t, _) if played => Some(t.index() as u8),
                JudgeEventKind::Tap(Judgement::Miss, _) => Some(MISS),
                JudgeEventKind::Tap(_, delta) => Some(ddi_tier(delta, rate, difficulty)),
                JudgeEventKind::HitMine | JudgeEventKind::HitShock => Some(BROKE),
                _ => None,
            })
            .collect();
        if played {
            r.as_played += 1;
        } else {
            r.rejudged += 1;
            let tiers = [
                Judgement::W1,
                Judgement::W2,
                Judgement::W3,
                Judgement::W4,
                Judgement::W5,
                Judgement::Miss,
            ];
            let diff: u64 = tiers
                .iter()
                .enumerate()
                .map(|(i, &t)| {
                    let mine = seq.iter().filter(|&&c| usize::from(c) == i).count() as i64;
                    (mine - i64::from(worked.tally.count(t))).unsigned_abs()
                })
                .sum();
            r.exact += u32::from(diff == 0);
            r.diff += diff;
        }
        // Judged by round U's rule (judge 6 on): read on the old one (its
        // misses and mines are `seq`'s however its taps were put on DDI's
        // grid).
        let d = if worked.judge_version >= 6 { before_u(new, &seq) } else { new };
        let n = seq.iter().filter(|&&c| c != BROKE).count() as u32;
        let w0 = (d.absolute - 100 * d.points).clamp(0, i64::from(u32::MAX)) as u32;
        let f = feat(&seq, n, d.most, w0, d.points - seq_points(&seq));
        // Kept before the combo part, or judged again with it.
        debug_assert_eq!(cands()[0].score(&f), d.score - d.combo);
        let best = key.best_key();
        let combo_kept = worked.judge_version >= 5;
        let moments: Vec<percent::Moment> = timing
            .timeline
            .iter()
            .filter_map(|j| match j.kind {
                JudgeEventKind::Tap(Judgement::Miss, _) => Some((MISS, None)),
                JudgeEventKind::Tap(t, delta) => {
                    Some((t.index() as u8, Some(delta / rate.max(1e-6))))
                }
                JudgeEventKind::HitMine | JudgeEventKind::HitShock => Some((BROKE, None)),
                _ => None,
            })
            .collect();
        let times: Vec<f64> = timing
            .timeline
            .iter()
            .filter_map(|j| match j.kind {
                JudgeEventKind::Tap(_, delta) => Some((f64::from(j.time) - delta) / rate.max(1e-6)),
                JudgeEventKind::HitMine | JudgeEventKind::HitShock => {
                    Some(f64::from(j.time) / rate.max(1e-6))
                }
                _ => None,
            })
            .collect();
        let (base, base_grade) = percent::baselines(&moments);
        let original = (
            record.ruleset.id.clone(),
            (!played).then(|| summary.worked.as_ref().map(|w| w.score)).flatten(),
        );
        let pp = percent::Pp {
            n,
            points: d.points,
            most: d.most,
            combo: if combo_kept { d.combo } else { f.per[ENGINE] / PER[ENGINE].d },
            max: ddi::full_combo_points(n),
            tiers: percent::Pp::tiers_of(&worked.tally),
            dist: ddi::distance(&worked.tally),
            ad: ddi::absolute(&worked.tally),
            breaks: f.breaks,
            base,
            base_grade,
        };
        out.push(Kept {
            group: format!("{:?}/{:?}", summary.chart, best.ddi),
            seq,
            f,
            clear: record.ended == Ending::Finished && !worked.failed,
            counts: summary.counts(),
            pp,
            grade: percent::grade_index(&worked.grade),
            combo_kept,
            autoplay: summary.autoplay,
            ending: record.ended,
            tally: worked.tally,
            moments,
            notes_key: format!("{:?}/{}", summary.chart.digest, d.most),
            played: original,
            difficulty,
            times,
            when: record.time,
            audio: record.setup.clock.audio.as_ref().map_or(String::new(), |d| d.id.clone()),
            audio_offset: record.setup.clock.audio_offset,
        });
    }
    (out, r)
}

/// Part 5: the maintainer's kept plays, aggregates only (never a play's
/// or a chart's own numbers or names).
fn kept(cands: &[Cand], dir: &Path) {
    let (plays, r) = read_kept(dir);
    println!(
        "== the kept plays: {} files, {} unread, {} with no DDI judgment; {} played under DDI, {} judged \
         again under DDI (their taps put on DDI's grid: {:.1}% match the second judgment's counts \
         exactly, {:.2} judgments a play off on average)",
        r.files,
        r.unread,
        r.no_ddi,
        r.as_played,
        r.rejudged,
        share(u64::from(r.exact), u64::from(r.rejudged)),
        r.diff as f64 / f64::from(r.rejudged.max(1)),
    );
    let clears: Vec<&Kept> = plays.iter().filter(|p| p.clear && p.counts && p.f.n >= 20).collect();
    // Runs: the breaks a play, where breaks fall, how long runs are.
    let mut by_breaks = [0u32; 6];
    let mut deciles = [0u64; 10];
    let (mut notes, mut in_long, mut longest) = (0u64, 0u64, 0.0f64);
    for p in &clears {
        by_breaks[match p.f.breaks {
            0 => 0,
            1 => 1,
            2 => 2,
            3..=5 => 3,
            6..=10 => 4,
            _ => 5,
        }] += 1;
        let total = p.seq.len().max(1);
        let mut run = 0u64;
        let mut best = 0u64;
        for (i, &c) in p.seq.iter().enumerate() {
            if c <= 2 {
                run += 1;
                continue;
            }
            deciles[(10 * i / total).min(9)] += 1;
            if run >= 100 {
                in_long += run;
            }
            best = best.max(run);
            run = 0;
        }
        if run >= 100 {
            in_long += run;
        }
        best = best.max(run);
        notes += u64::from(p.f.n);
        longest += best as f64 / f64::from(p.f.n.max(1));
    }
    let all_breaks: u64 = deciles.iter().sum();
    println!(
        "clears counted ({}; {:.0} notes a play): breaks 0 / 1 / 2 / 3-5 / 6-10 / 11+: {}; breaks by tenth of \
         the play: {}; notes in runs of 100 or more {:.1}%; the longest run {:.1}% of a play on average",
        clears.len(),
        notes as f64 / clears.len().max(1) as f64,
        by_breaks
            .iter()
            .map(|&b| format!("{:.1}%", share(u64::from(b), clears.len() as u64)))
            .collect::<Vec<_>>()
            .join(" / "),
        deciles
            .iter()
            .map(|&d| format!("{:.1}", share(d, all_breaks)))
            .collect::<Vec<_>>()
            .join(" "),
        share(in_long, notes),
        100.0 * longest / clears.len().max(1) as f64,
    );
    // By chart: pairs reordered, bests changing hands.
    let mut groups: std::collections::BTreeMap<&str, Vec<&Kept>> = Default::default();
    for p in plays.iter().filter(|p| p.counts) {
        groups.entry(p.group.as_str()).or_default().push(p);
    }
    let multi = groups.values().filter(|g| g.len() >= 2).count();
    println!(
        "charts played (under one set of DDI options): {}, {multi} with two plays or more; per candidate: \
         the part's mean share of a clear's score and of a full combo's part; pairs of plays of one chart \
         turned around; bests changing hands",
        groups.len()
    );
    for c in cands {
        let (mut part_share, mut of_full) = (0.0, 0.0);
        for p in &clears {
            let score = c.score(&p.f);
            let part = score - cands[0].score(&p.f);
            part_share += part as f64 / score.max(1) as f64;
            if let Shape::Per(i) = c.shape {
                of_full += part as f64 / full_per(&PER[i], u64::from(p.f.n)).max(1) as f64;
            }
        }
        let (mut pairs, mut turned, mut changed) = (0u64, 0u64, 0u64);
        for g in groups.values().filter(|g| g.len() >= 2) {
            let today: Vec<i64> = g.iter().map(|p| cands[0].score(&p.f)).collect();
            let now: Vec<i64> = g.iter().map(|p| c.score(&p.f)).collect();
            for i in 0..g.len() {
                for j in i + 1..g.len() {
                    if today[i] != today[j] {
                        pairs += 1;
                        turned += u64::from(now[i].cmp(&now[j]) == today[j].cmp(&today[i]));
                    }
                }
            }
            let best = |v: &[i64]| (0..v.len()).max_by_key(|&i| (v[i], today[i], std::cmp::Reverse(i))).unwrap();
            changed += u64::from(best(&today) != best(&now));
        }
        let n = clears.len().max(1) as f64;
        println!(
            "{:<16} part {:5.1}% of the score, {:5.1}% of a full combo's | turned {:5.2}% of {pairs} pairs | \
             bests changing hands {:5.1}% of {multi}",
            c.name(),
            100.0 * part_share / n,
            100.0 * of_full / n,
            share(turned, pairs),
            share(changed, multi as u64),
        );
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
    if wanted("ladder") {
        let mut bundled = Vec::new();
        simfiles(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs"), &mut bundled);
        print_under("the bundled songs", &under(&cands, &bundled), &cands);
        if let Some(dirs) = std::env::var_os("DDI_COMBO_CORPUS") {
            let mut files = Vec::new();
            for dir in std::env::split_paths(&dirs) {
                simfiles(&dir, &mut files);
            }
            print_under("the local folders", &under(&cands, &files), &cands);
        }
    }
    if wanted("kept")
        && let Some(dir) = std::env::var_os("DDI_COMBO_PLAYS")
    {
        kept(&cands, Path::new(&dir));
    }
    if only.as_deref() == Some("percent") {
        percent_part();
    }
    if only.as_deref() == Some("built") {
        match std::env::var_os("DDI_COMBO_PLAYS") {
            Some(dir) => percent::distributions::built(Path::new(&dir)),
            None => println!("built: DDI_COMBO_PLAYS names the plays"),
        }
    }
    if only.as_deref() == Some("distributions") {
        match (std::env::var_os("DDI_COMBO_PLAYS"), std::env::var_os("DDI_PCT_DIST_OUT")) {
            (Some(dir), Some(out)) => percent::distributions::run(Path::new(&dir), Path::new(&out)),
            _ => println!("distributions: DDI_COMBO_PLAYS and DDI_PCT_DIST_OUT name the plays and the output"),
        }
    }
}

/// Part 6: the percent and the grades with the combo in them
/// ([`percent`]).
fn percent_part() {
    percent::worked();
    let seeds = env_number("DDI_COMBO_SEEDS", 4);
    let study = |files: &[PathBuf]| {
        let parts = per_chart(files, seeds, percent::Study::new, |s, plays| {
            let clears: Vec<(Option<usize>, percent::Pp)> = plays
                .iter()
                .filter(|p| !p.out && p.pp.most > 0 && p.pp.n > 0)
                .map(|p| (Some(p.level), p.pp))
                .collect();
            if !clears.is_empty() {
                s.chart(&clears);
            }
        });
        let mut all = percent::Study::new();
        for p in &parts {
            all.add(p);
        }
        all
    };
    let mut bundled = Vec::new();
    simfiles(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/songs"), &mut bundled);
    percent::print_study(&format!("the bundled songs, {seeds} seeds a level"), &study(&bundled), true);
    if let Some(dirs) = std::env::var_os("DDI_COMBO_CORPUS") {
        let mut files = Vec::new();
        for dir in std::env::split_paths(&dirs) {
            simfiles(&dir, &mut files);
        }
        let every = env_number("DDI_COMBO_SAMPLE", 1).max(1) as usize;
        let files: Vec<PathBuf> = files.into_iter().step_by(every).collect();
        percent::print_study(
            &format!("the local folders ({} simfiles), {seeds} seeds a level", files.len()),
            &study(&files),
            true,
        );
    }
    if let Some(dir) = std::env::var_os("DDI_COMBO_PLAYS") {
        let (plays, _) = read_kept(Path::new(&dir));
        let clears: Vec<&Kept> = plays.iter().filter(|p| p.clear && p.counts && p.f.n >= 20).collect();
        let agree = clears.iter().filter(|p| p.grade == Some(percent::today_grade(&p.pp))).count();
        let kept_combo = clears.iter().filter(|p| p.combo_kept).count();
        println!(
            "== the kept plays: {} clears; today's grade worked out here agrees with the kept one on {:.1}%; \
             the combo part as the judge kept it on {:.1}% (the rest worked out from the taps put on DDI's grid)",
            clears.len(),
            share(agree as u64, clears.len() as u64),
            share(kept_combo as u64, clears.len() as u64),
        );
        let mut groups: std::collections::BTreeMap<&str, Vec<(Option<usize>, percent::Pp)>> = Default::default();
        for p in &clears {
            groups.entry(p.group.as_str()).or_default().push((None, p.pp));
        }
        let mut s = percent::Study::new();
        for g in groups.values() {
            s.chart(g);
        }
        percent::print_study("the kept plays (aggregates only)", &s, false);
    }
}
