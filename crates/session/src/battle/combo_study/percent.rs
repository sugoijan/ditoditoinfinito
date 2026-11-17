//! Part 6 of the study (2026-11-17, `docs/plans/ddi-ruleset.md`, "Should
//! the combo score count toward the percent? A study"): what the grades
//! would do if DDI's percent counted the combo part (`ddi-combo-v0`)
//! (and, for reference only, with flatter tiers' points), beside what
//! other rulesets the game plays ([`BASES`]) make of the same plays.
//! Measured only: nothing a play scores or shows changes.
//!
//! A candidate is a points table ([`TIERS`]), a percent rule and grade
//! thresholds. In hundredths of a percent, floored, `points` the play's
//! DDI points under the candidate's table, `most` the chart's (50 a
//! tap-like note and a hold unit, whatever the table), `combo` the play's
//! combo part and `max` a full combo's of the chart's tap-like notes
//! (`ddi::full_combo_points`):
//!
//! - accuracy: `10,000 × points ÷ most` (today's, under today's table);
//! - P1, the score's own weights: `10,000 × (100 × points + combo) ÷ (100
//!   × most + max)`;
//! - P2 at w%: `(100 − w)% × accuracy + w% × combo ÷ max`;
//! - a cap: never more than 2 (3) points below the accuracy;
//! - P1 ≤acc: P1 with the combo counted up to the play's own accuracy
//!   (`min(combo, max × points ÷ most)`): a full combo is its accuracy, a
//!   broken play loses.
//!
//! Grades are read from the candidate's percent as today's are
//! (`ddi::GRADES` or the candidate's thresholds, the floors by the notes'
//! distance from W0, Absolute Dito by the counts); today's is asserted
//! equal to the engine's on every CPU play.
//!
//! MEASUREMENTS OF CPUS, and of one person's kept plays in aggregate.
//!
//! Since round U (2026-11-17) "today" here is the rule before it (a Miss
//! and a mine −10, the percent the accuracy, [`TODAY_GRADES`]): see the
//! parent module's header. [`built_percent`] and [`built_grade`] are
//! round U's rule as the engine plays it, worked out from the same
//! reading and checked against the engine on every CPU play.

use super::*;
use ddi_engine::rules::Judgement;

pub(super) mod distributions;

/// Points tables, W1 (W0 too) to Miss: today's, and one mildly flatter
/// for reference (the maintainer, 2026-11-17: flattening the tiers
/// "seems too much"; kept as one row to see what it would have done).
const TIERS: [(&str, [i64; 6]); 2] = [
    ("", [50, 40, 25, 10, 0, -10]),
    ("T42/32 ", [50, 42, 32, 10, 0, -10]),
];

#[derive(Clone, Copy, PartialEq)]
enum Rule {
    Accuracy,
    P1,
    /// P2 at this weight in percent.
    P2(i64),
    /// P1 with the combo counted up to the play's accuracy.
    P1UpToAccuracy,
}

#[derive(Clone)]
pub(super) struct PCand {
    name: String,
    tiers: usize,
    rule: Rule,
    /// The most below the accuracy the percent may fall, hundredths.
    cap: Option<i64>,
    /// SSS, SS, S, A, B, C in hundredths (today's, [`TODAY_GRADES`], if
    /// none).
    grades: [i64; 6],
}

/// The letters before round U (2026-11-17): what this study calls today.
const TODAY_GRADES: [i64; 6] = [9900, 9600, 8900, 8000, 6800, 5500];

/// Round U's percent (`ddi::PERCENT_RULE`, `ddi-percent-v1`) of a play
/// this study reads on the rule before it: its misses at the engine's
/// −120 and its `mines` (mines and shocks stepped on) at −50, then
/// `ddi::percent_of`. Equal to the engine's on every play judged by it.
pub(super) fn built_percent(p: &Pp, mines: u32) -> i64 {
    let points = p.points
        + (ddi::TAP_POINTS[5] - super::TAP_POINTS[5]) * i64::from(p.tiers[5])
        + (ddi::MINE_POINTS - super::MINE_POINTS) * i64::from(mines);
    ddi::percent_of(points, p.most, p.combo, p.max)
}

/// Round U's letters (`ddi::GRADES`) of [`built_percent`], the floors as
/// ever: the grade index ([`grade_index`]).
pub(super) fn built_grade(p: &Pp, mines: u32) -> usize {
    grade_at(&BUILT_GRADES, built_percent(p, mines), p)
}

/// `ddi::GRADES`' thresholds, SSS to C.
pub(super) const BUILT_GRADES: [i64; 6] = {
    let g = ddi::GRADES;
    [g[0].1, g[1].1, g[2].1, g[3].1, g[4].1, g[5].1]
};

fn cand(
    tiers: usize,
    rule: Rule,
    cap: Option<i64>,
    grades: Option<[i64; 6]>,
    label: &str,
) -> PCand {
    let rule_name = match rule {
        Rule::Accuracy if tiers == 0 => "today".to_string(),
        Rule::Accuracy => "acc".into(),
        Rule::P1 => "P1".into(),
        Rule::P2(w) => format!("P2 {w}%"),
        Rule::P1UpToAccuracy => "P1 <=acc".into(),
    };
    let cap_name = cap.map_or(String::new(), |c| format!(" <={}", c / 100));
    PCand {
        name: format!("{}{rule_name}{cap_name}{label}", TIERS[tiers].0),
        tiers,
        rule,
        cap,
        grades: grades.unwrap_or(TODAY_GRADES),
    }
}

/// The candidates: today's first.
pub(super) fn cands() -> Vec<PCand> {
    let mut v = vec![
        cand(0, Rule::Accuracy, None, None, ""),
        cand(0, Rule::P1, None, None, ""),
        cand(0, Rule::P2(5), None, None, ""),
        cand(0, Rule::P2(10), None, None, ""),
        cand(0, Rule::P2(15), None, None, ""),
        cand(0, Rule::P1, Some(200), None, ""),
        cand(0, Rule::P1, Some(300), None, ""),
        cand(0, Rule::P2(10), Some(200), None, ""),
        cand(0, Rule::P1UpToAccuracy, None, None, ""),
    ];
    v.push(cand(1, Rule::Accuracy, None, None, ""));
    // The packages (`DDI_PCT_PACKAGES=name:rule:cap:SSS,SS,S,A,B,C;…`,
    // rule `acc`, `p1` or a P2 weight, cap in hundredths or `-`), set
    // after a first run.
    if let Ok(spec) = std::env::var("DDI_PCT_PACKAGES") {
        for one in spec.split(';').filter(|s| !s.is_empty()) {
            let parts: Vec<&str> = one.split(':').collect();
            let rule = match parts[1] {
                "acc" => Rule::Accuracy,
                "p1" => Rule::P1,
                w => Rule::P2(w.parse().expect("a weight")),
            };
            let cap = parts[2].parse().ok();
            let g: Vec<i64> = parts[3]
                .split(',')
                .map(|x| x.parse().expect("a threshold"))
                .collect();
            v.push(cand(
                0,
                rule,
                cap,
                Some(g.try_into().expect("six")),
                &format!(" [{}]", parts[0]),
            ));
        }
    }
    v
}

/// What a percent and a grade read of a play.
#[derive(Clone, Copy, Default)]
pub(super) struct Pp {
    pub n: u32,
    /// Today's points.
    pub points: i64,
    pub most: i64,
    pub combo: i64,
    pub max: i64,
    /// Tap-like notes by tier, W1 (W0 too) to Miss.
    pub tiers: [u32; 6],
    /// The floors' distance from W0 (`ddi::distance`).
    pub dist: u64,
    /// Absolute Dito.
    pub ad: bool,
    pub breaks: u32,
    /// What each of [`BASES`] makes of its taps: the percent (DDR A's
    /// money score over 100) in hundredths, and the grade's place in its
    /// list (0 the best).
    pub base: [i64; NB],
    pub base_grade: [u8; NB],
}

/// Another ruleset the game plays, over a play's taps alone (holds and
/// rolls left out; mines where it scores them), as the baseline a player
/// coming from it would expect (`docs/plans/ddi-ruleset.md`, "Should the
/// combo score count toward the percent? A study", "The baselines").
pub(super) struct Base {
    pub name: &'static str,
    /// Its tiers' windows, best first, early and late seconds; a tap
    /// outside them is a miss.
    windows: &'static [(f64, f64)],
    /// Each tier's weight and last the miss's.
    weights: &'static [i64],
    /// A note's best, and what its max combo weighs a note (Dancing☆Onigiri
    /// alone: `2 × maxCombo` beside 8 an Ii, out of 10 a note).
    best: i64,
    combo_weight: i64,
    /// The tiers that keep the combo (`0..keeps`).
    keeps: usize,
    /// A mine stepped on: its weight, and whether it breaks the combo.
    mine: Option<i64>,
    /// DDR's money score instead of a weighted percent.
    money: bool,
    /// The grades, best first, with the least percent (hundredths; DDR A
    /// money over 100) each needs; the last needs nothing.
    grades: &'static [(&'static str, i64)],
    /// The best grade for every tap in the first tier, the next for every
    /// tap in the first two (StepMania 5's AAAA and AAA), before the
    /// percent's grades.
    all_top: bool,
    /// Where a DDI tier (W1 … Miss) falls in it when a play has no offsets
    /// (the made-up plays): by the windows' nearest match.
    by_tier: [usize; 6],
}

pub(super) const NB: usize = 5;

pub(super) const BASES: [Base; NB] = [
    // ITG (Simply Love): dance points 5, 4, 2, 0, −6, −12, a mine −6;
    // its windows with `TimingWindowAdd`.
    Base {
        name: "ITG",
        windows: &[
            (0.023, 0.023),
            (0.0445, 0.0445),
            (0.1035, 0.1035),
            (0.1365, 0.1365),
            (0.1815, 0.1815),
        ],
        weights: &[5, 4, 2, 0, -6, -12],
        best: 5,
        combo_weight: 0,
        keeps: 3,
        mine: Some(-6),
        money: false,
        grades: &[
            ("****", 10000),
            ("***", 9900),
            ("**", 9800),
            ("*", 9600),
            ("S+", 9400),
            ("S", 9200),
            ("S-", 8900),
            ("A+", 8600),
            ("A", 8300),
            ("A-", 8000),
            ("B+", 7600),
            ("B", 7200),
            ("B-", 6800),
            ("C+", 6400),
            ("C", 6000),
            ("C-", 5500),
            ("D", i64::MIN),
        ],
        all_top: false,
        by_tier: [0, 1, 2, 3, 4, 5],
    },
    // StepMania 5's defaults: graded by its grade weights 2, 2, 1, 0, −4,
    // −8 (a mine −8), AAAA every tap W1, AAA every tap W2 or better.
    Base {
        name: "SM5",
        windows: &[
            (0.0225, 0.0225),
            (0.045, 0.045),
            (0.090, 0.090),
            (0.135, 0.135),
            (0.180, 0.180),
        ],
        weights: &[2, 2, 1, 0, -4, -8],
        best: 2,
        combo_weight: 0,
        keeps: 3,
        mine: Some(-8),
        money: false,
        grades: &[
            ("AAAA", 10001),
            ("AAA", 10001),
            ("AA", 9300),
            ("A", 8000),
            ("B", 6500),
            ("C", 4500),
            ("D", i64::MIN),
        ],
        all_top: true,
        by_tier: [0, 1, 2, 3, 4, 5],
    },
    // DDR A: the money score (Marvelous 1, Perfect 1 − 10 points, Great
    // 0.6, Good 0.2 of a step, less 10 each), no W5 (Boo is a miss).
    Base {
        name: "DDR A",
        windows: &[
            (0.016_667, 0.016_667),
            (0.033_333, 0.033_333),
            (0.091_667, 0.091_667),
            (0.141_667, 0.141_667),
        ],
        weights: &[5, 5, 3, 1, 0],
        best: 5,
        combo_weight: 0,
        keeps: 4,
        mine: None,
        money: true,
        grades: &[
            ("AAA", 9900),
            ("AA+", 9500),
            ("AA", 9000),
            ("AA-", 8900),
            ("A+", 8500),
            ("A", 8000),
            ("A-", 7900),
            ("B+", 7500),
            ("B", 7000),
            ("B-", 6900),
            ("C+", 6500),
            ("C", 6000),
            ("C-", 5900),
            ("D+", 5500),
            ("D", i64::MIN),
        ],
        all_top: false,
        by_tier: [0, 1, 2, 3, 4, 4],
    },
    // DDR EXTREME: graded by MAX's dance points, Perfect 2, Great 1, Good
    // 0, Boo −4, Miss −8 (its score weighs later steps more, but grades
    // read the dance points); no Marvelous in normal play.
    Base {
        name: "EXTREME",
        windows: &[
            (0.030, 0.030),
            (0.090, 0.076_667),
            (0.130, 0.116_667),
            (0.156_667, 0.183_333),
        ],
        weights: &[2, 1, 0, -4, -8],
        best: 2,
        combo_weight: 0,
        keeps: 2,
        mine: Some(-8),
        money: false,
        grades: &[
            ("AAA", 10000),
            ("AA", 9300),
            ("A", 8000),
            ("B", 6500),
            ("C", 4500),
            ("D", i64::MIN),
        ],
        all_top: false,
        by_tier: [0, 0, 1, 2, 3, 4],
    },
    // Dancing☆Onigiri: `(8 Ii + 4 Shakin + 2 Matari + 2 maxCombo) ÷ 10`
    // a note (its holds' terms left out with the holds), ranks by it;
    // windows k frames early and k + 1 late at 60 fps.
    Base {
        name: "DanOni",
        windows: &[
            (2.0 / 60.0, 3.0 / 60.0),
            (4.0 / 60.0, 5.0 / 60.0),
            (6.0 / 60.0, 7.0 / 60.0),
            (8.0 / 60.0, 9.0 / 60.0),
        ],
        weights: &[8, 4, 2, 0, 0],
        best: 8,
        combo_weight: 2,
        keeps: 3,
        mine: None,
        money: false,
        grades: &[
            ("AP", 10000),
            ("SS", 9700),
            ("S", 9000),
            ("SA", 8500),
            ("AAA", 8000),
            ("AA", 7500),
            ("A", 7000),
            ("B", 6500),
            ("C", 6000),
            ("D", i64::MIN),
        ],
        all_top: false,
        by_tier: [0, 1, 2, 3, 4, 4],
    },
];

/// One judged moment for the baselines: a DDI tier code (0 W1 … 5 Miss,
/// 6 a mine stepped on) and, for a tap hit, its offset in real seconds
/// (none: placed by [`Base::by_tier`]).
pub(super) type Moment = (u8, Option<f64>);

impl Base {
    fn tier(&self, m: Moment) -> usize {
        let miss = self.weights.len() - 1;
        match m {
            (5, _) => miss,
            (t, None) => self.by_tier[usize::from(t)],
            (_, Some(d)) => self
                .windows
                .iter()
                .position(|&(early, late)| {
                    if d < 0.0 {
                        -d <= early + 1e-9
                    } else {
                        d <= late + 1e-9
                    }
                })
                .unwrap_or(miss),
        }
    }

    /// The percent (hundredths) and the grade's place of `moments`.
    fn judge(&self, moments: &[Moment]) -> (i64, u8) {
        let (pct, place, _) = self.judge_over(moments, None);
        (pct, place)
    }

    /// The same over a chart of `chart_taps` tap-like notes where the
    /// play judged fewer (a fail or a quit: the rest count nothing, as
    /// each of these rulesets counts a play that ended early), and whether
    /// it is a full combo by its own rule (every tap in a tier that keeps
    /// the combo; mines break none of these rulesets' combos, DDR A's
    /// shock rows aside, which are left out with its mines).
    fn judge_over(&self, moments: &[Moment], chart_taps: Option<i64>) -> (i64, u8, bool) {
        let (mut sum, mut taps, mut combo, mut max_combo) = (0i64, 0i64, 0i64, 0i64);
        let mut full_combo = true;
        let (mut all_first, mut all_two) = (true, true);
        let mut counts = [0i64; 6];
        for &m in moments {
            if m.0 == BROKE {
                if let Some(w) = self.mine {
                    sum += w;
                    combo = 0;
                    all_first = false;
                    all_two = false;
                }
                continue;
            }
            let t = self.tier(m);
            taps += 1;
            counts[t] += 1;
            sum += self.weights[t];
            all_first &= t == 0;
            all_two &= t <= 1;
            full_combo &= t < self.keeps;
            if t < self.keeps {
                combo += 1;
                max_combo = max_combo.max(combo);
            } else {
                combo = 0;
            }
        }
        if taps == 0 {
            return (0, (self.grades.len() - 1) as u8, false);
        }
        let of = chart_taps.map_or(taps, |n| n.max(taps));
        let pct = if self.money {
            // DDR A: 1,000,000 × the weighted share, less 10 a Perfect,
            // Great and Good, floored to 10; over 100 as hundredths.
            let money = (1_000_000 * sum).div_euclid(self.best * of)
                - 10 * (counts[1] + counts[2] + counts[3]);
            (money.max(0) / 10 * 10) / 100
        } else {
            (10_000 * (sum + self.combo_weight * max_combo))
                .div_euclid((self.best + self.combo_weight) * of)
        };
        let place = if self.all_top && all_first {
            0
        } else if self.all_top && all_two {
            1
        } else {
            self.grades
                .iter()
                .position(|&(_, min)| pct >= min)
                .unwrap_or(self.grades.len() - 1)
        };
        (pct, place as u8, full_combo)
    }
}

/// What every baseline makes of `moments`.
pub(super) fn baselines(moments: &[Moment]) -> ([i64; NB], [u8; NB]) {
    let mut pct = [0; NB];
    let mut grade = [0; NB];
    for (b, base) in BASES.iter().enumerate() {
        (pct[b], grade[b]) = base.judge(moments);
    }
    (pct, grade)
}

impl Pp {
    pub(super) fn tiers_of(tally: &ddi_engine::rules::Tally) -> [u32; 6] {
        Judgement::ALL.map(|j| tally.count(j))
    }

    fn points_under(&self, table: usize) -> i64 {
        let t = &TIERS[table].1;
        self.points
            + (0..6)
                .map(|i| i64::from(self.tiers[i]) * (t[i] - super::TAP_POINTS[i]))
                .sum::<i64>()
    }
}

fn div(a: i128, b: i128) -> i64 {
    if b > 0 { a.div_euclid(b) as i64 } else { 0 }
}

fn percent(c: &PCand, p: &Pp) -> i64 {
    let pts = i128::from(p.points_under(c.tiers));
    let (most, combo, max) = (i128::from(p.most), i128::from(p.combo), i128::from(p.max));
    let acc = div(10_000 * pts, most);
    let raw = match c.rule {
        Rule::Accuracy => acc,
        Rule::P1 => div(10_000 * (100 * pts + combo), 100 * most + max),
        Rule::P1UpToAccuracy => {
            let up_to = div(max * pts.max(0), most);
            div(
                10_000 * (100 * pts + combo.min(i128::from(up_to))),
                100 * most + max,
            )
        }
        Rule::P2(w) if max > 0 => {
            let w = i128::from(w);
            div(100 * ((100 - w) * pts * max + w * combo * most), most * max)
        }
        Rule::P2(_) => acc,
    };
    match c.cap {
        Some(cap) => raw.max(acc - cap),
        None => raw,
    }
}

const GRADE_NAMES: [&str; 8] = ["AD", "SSS", "SS", "S", "A", "B", "C", "D"];

/// 0 Absolute Dito, 1 SSS … 7 D, as `ddi::grade` reads a clear.
fn grade(c: &PCand, pct: i64, p: &Pp) -> usize {
    grade_at(&c.grades, pct, p)
}

/// The same with `thresholds` (SSS … C, hundredths).
fn grade_at(thresholds: &[i64; 6], pct: i64, p: &Pp) -> usize {
    if p.ad {
        return 0;
    }
    let by = thresholds.iter().position(|&min| pct >= min).unwrap_or(6);
    let floor = p.dist.clamp(1, 7) as usize - 1;
    1 + by.min(floor)
}

/// The grade index of an engine's grade name.
pub(super) fn grade_index(name: &str) -> Option<usize> {
    if name == ddi::ABSOLUTE_DITO || name == ddi::ABSOLUTE_BOT {
        return Some(0);
    }
    ddi::GRADES
        .iter()
        .position(|&(g, _)| g == name)
        .map(|i| i + 1)
}

/// Today's grade by this study's arithmetic (checked against the engine's).
pub(super) fn today_grade(p: &Pp) -> usize {
    let today = cand(0, Rule::Accuracy, None, None, "");
    grade(&today, percent(&today, p), p)
}

fn text(h: i64) -> String {
    let sign = if h < 0 { "-" } else { "" };
    format!(
        "{sign}{}.{:02}",
        h.unsigned_abs() / 100,
        h.unsigned_abs() % 100
    )
}

/// Δ histogram: hundredths in bins of 5, from −40 to +40 points.
const HB: i64 = 5;
const HN: usize = 1601;

#[derive(Clone, Default)]
struct Grades {
    plays: u64,
    at: Vec<[u64; 8]>,
    /// Up 2+, up 1, same, down 1, down 2+.
    moves: Vec<[u64; 5]>,
    delta_sum: Vec<i64>,
    lo: Vec<i64>,
    hi: Vec<i64>,
    hist: Vec<Vec<u32>>,
    /// Full combos by accuracy: under 60, 60 to 70, 70 to 80, 80 to 90, 90
    /// and over.
    fc: [u64; 5],
}

impl Grades {
    fn new(nc: usize) -> Grades {
        Grades {
            at: vec![[0; 8]; nc],
            moves: vec![[0; 5]; nc],
            delta_sum: vec![0; nc],
            lo: vec![i64::MAX; nc],
            hi: vec![i64::MIN; nc],
            hist: vec![vec![0; HN]; nc],
            ..Grades::default()
        }
    }

    fn add_play(&mut self, p: &Pp, pcts: &[i64], grades: &[usize]) {
        self.plays += 1;
        if p.breaks == 0 {
            self.fc[((pcts[0] - 5000).max(0) / 1000).min(4) as usize] += 1;
        }
        for k in 0..pcts.len() {
            self.at[k][grades[k]] += 1;
            let up = grades[0] as i64 - grades[k] as i64;
            self.moves[k][match up {
                i64::MIN..=-2 => 4,
                -1 => 3,
                0 => 2,
                1 => 1,
                _ => 0,
            }] += 1;
            let d = pcts[k] - pcts[0];
            self.delta_sum[k] += d;
            self.lo[k] = self.lo[k].min(d);
            self.hi[k] = self.hi[k].max(d);
            let bin = (d.div_euclid(HB) + (HN as i64 / 2)).clamp(0, HN as i64 - 1) as usize;
            self.hist[k][bin] += 1;
        }
    }

    fn add(&mut self, o: &Grades) {
        self.plays += o.plays;
        for k in 0..self.at.len() {
            for g in 0..8 {
                self.at[k][g] += o.at[k][g];
            }
            for m in 0..5 {
                self.moves[k][m] += o.moves[k][m];
            }
            self.delta_sum[k] += o.delta_sum[k];
            self.lo[k] = self.lo[k].min(o.lo[k]);
            self.hi[k] = self.hi[k].max(o.hi[k]);
            for b in 0..HN {
                self.hist[k][b] += o.hist[k][b];
            }
        }
        for i in 0..5 {
            self.fc[i] += o.fc[i];
        }
    }

    /// The Δ (points) below which `q` of the plays are.
    fn quantile(&self, k: usize, q: f64) -> f64 {
        let want = (q * self.plays as f64).ceil().max(1.0) as u64;
        let mut seen = 0u64;
        for (b, &c) in self.hist[k].iter().enumerate() {
            seen += u64::from(c);
            if seen >= want {
                return ((b as i64 - HN as i64 / 2) * HB) as f64 / 100.0;
            }
        }
        0.0
    }
}

const GAP_NAMES: [&str; 5] = ["<=0.5", "0.5-1", "1-2", "2-4", ">4"];

fn gap_bin(h: i64) -> usize {
    match h {
        ..=50 => 0,
        51..=100 => 1,
        101..=200 => 2,
        201..=400 => 3,
        _ => 4,
    }
}

#[derive(Clone, Default)]
struct Pairs {
    /// One play with fewer breaks, the other more accurate (today's
    /// percent), by its lead: pairs; for each ranker (the candidates, then
    /// [`BASES`]) the cleaner ranked higher and the more accurate ranked
    /// higher, by the percent and by the grade.
    clean: [u64; 5],
    pct_x: Vec<[u64; 5]>,
    pct_y: Vec<[u64; 5]>,
    clean_x: Vec<[u64; 5]>,
    clean_y: Vec<[u64; 5]>,
    /// Each candidate against the baselines (by the percent): it ranks the
    /// cleaner higher where no baseline does; it ranks the more accurate
    /// higher where every baseline ranks the cleaner higher.
    alone: Vec<[u64; 5]>,
    against: Vec<[u64; 5]>,
    /// Any two of different accuracy, by the gap: pairs, and those where
    /// the less accurate is graded higher.
    gap: [u64; 5],
    inverted: Vec<[u64; 5]>,
}

impl Pairs {
    fn new(nc: usize) -> Pairs {
        let nr = nc + NB;
        Pairs {
            pct_x: vec![[0; 5]; nr],
            pct_y: vec![[0; 5]; nr],
            clean_x: vec![[0; 5]; nr],
            clean_y: vec![[0; 5]; nr],
            alone: vec![[0; 5]; nc],
            against: vec![[0; 5]; nc],
            inverted: vec![[0; 5]; nc],
            ..Pairs::default()
        }
    }

    fn chart(&mut self, plays: &[(Pp, Vec<i64>, Vec<usize>)]) {
        let nc = self.inverted.len();
        for i in 0..plays.len() {
            for j in i + 1..plays.len() {
                let (a, b) = (&plays[i], &plays[j]);
                if a.1[0] == b.1[0] {
                    continue;
                }
                // `lo` the less accurate.
                let (lo, hi) = if a.1[0] < b.1[0] { (a, b) } else { (b, a) };
                let g = gap_bin(hi.1[0] - lo.1[0]);
                self.gap[g] += 1;
                for k in 0..nc {
                    self.inverted[k][g] += u64::from(lo.2[k] < hi.2[k]);
                }
                if lo.0.breaks >= hi.0.breaks {
                    continue;
                }
                self.clean[g] += 1;
                for k in 0..nc {
                    self.pct_x[k][g] += u64::from(lo.1[k] > hi.1[k]);
                    self.pct_y[k][g] += u64::from(hi.1[k] > lo.1[k]);
                    self.clean_x[k][g] += u64::from(lo.2[k] < hi.2[k]);
                    self.clean_y[k][g] += u64::from(hi.2[k] < lo.2[k]);
                }
                let mut bases_x = 0;
                for b in 0..NB {
                    let r = nc + b;
                    let x = lo.0.base[b] > hi.0.base[b];
                    bases_x += usize::from(x);
                    self.pct_x[r][g] += u64::from(x);
                    self.pct_y[r][g] += u64::from(hi.0.base[b] > lo.0.base[b]);
                    self.clean_x[r][g] += u64::from(lo.0.base_grade[b] < hi.0.base_grade[b]);
                    self.clean_y[r][g] += u64::from(hi.0.base_grade[b] < lo.0.base_grade[b]);
                }
                for k in 0..nc {
                    self.alone[k][g] += u64::from(lo.1[k] > hi.1[k] && bases_x == 0);
                    self.against[k][g] += u64::from(hi.1[k] > lo.1[k] && bases_x == NB);
                }
            }
        }
    }

    fn add(&mut self, o: &Pairs) {
        for g in 0..5 {
            self.clean[g] += o.clean[g];
            self.gap[g] += o.gap[g];
            for r in 0..self.pct_x.len() {
                self.pct_x[r][g] += o.pct_x[r][g];
                self.pct_y[r][g] += o.pct_y[r][g];
                self.clean_x[r][g] += o.clean_x[r][g];
                self.clean_y[r][g] += o.clean_y[r][g];
            }
            for k in 0..self.inverted.len() {
                self.alone[k][g] += o.alone[k][g];
                self.against[k][g] += o.against[k][g];
                self.inverted[k][g] += o.inverted[k][g];
            }
        }
    }
}

const KIND_NAMES: [&str; 5] = [
    "full combos",
    "1 break",
    "2 to 5 breaks",
    "6 to 20 breaks",
    "over 20 breaks",
];
const LENGTH_NAMES: [&str; 3] = ["under 150 notes", "150 to 400 notes", "over 400 notes"];
const FC_BANDS: [&str; 5] = ["<60", "60-70", "70-80", "80-90", ">=90"];

#[derive(Clone)]
pub(super) struct Study {
    cands: Vec<PCand>,
    charts: u64,
    all: Grades,
    level: Vec<Grades>,
    kind: Vec<Grades>,
    length: Vec<Grades>,
    pairs: Pairs,
    notes: u64,
    breaks: u64,
    /// Each candidate's percents over every clear, 0 to 100.00 (clamped).
    pcts: Vec<Vec<u32>>,
}

impl Study {
    pub(super) fn new() -> Study {
        let cands = cands();
        let nc = cands.len();
        Study {
            cands,
            charts: 0,
            all: Grades::new(nc),
            level: vec![Grades::new(nc); 10],
            kind: vec![Grades::new(nc); 5],
            length: vec![Grades::new(nc); 3],
            pairs: Pairs::new(nc),
            notes: 0,
            breaks: 0,
            pcts: vec![vec![0; 10_001]; nc],
        }
    }

    /// One chart's clears, by level where they are a CPU's.
    pub(super) fn chart(&mut self, plays: &[(Option<usize>, Pp)]) {
        self.charts += 1;
        let mut rows = Vec::with_capacity(plays.len());
        for (level, p) in plays {
            let pcts: Vec<i64> = self.cands.iter().map(|c| percent(c, p)).collect();
            let grades: Vec<usize> = self
                .cands
                .iter()
                .zip(&pcts)
                .map(|(c, &v)| grade(c, v, p))
                .collect();
            self.all.add_play(p, &pcts, &grades);
            if let Some(l) = level {
                self.level[*l].add_play(p, &pcts, &grades);
            }
            let kind = match p.breaks {
                0 => 0,
                1 => 1,
                2..=5 => 2,
                6..=20 => 3,
                _ => 4,
            };
            self.kind[kind].add_play(p, &pcts, &grades);
            let length = match p.n {
                ..150 => 0,
                150..=400 => 1,
                _ => 2,
            };
            self.length[length].add_play(p, &pcts, &grades);
            self.notes += u64::from(p.n);
            self.breaks += u64::from(p.breaks);
            for (h, &v) in self.pcts.iter_mut().zip(&pcts) {
                h[v.clamp(0, 10_000) as usize] += 1;
            }
            rows.push((*p, pcts, grades));
        }
        self.pairs.chart(&rows);
    }

    pub(super) fn add(&mut self, o: &Study) {
        self.charts += o.charts;
        self.all.add(&o.all);
        for i in 0..10 {
            self.level[i].add(&o.level[i]);
        }
        for i in 0..5 {
            self.kind[i].add(&o.kind[i]);
        }
        for i in 0..3 {
            self.length[i].add(&o.length[i]);
        }
        self.pairs.add(&o.pairs);
        self.notes += o.notes;
        self.breaks += o.breaks;
        for (a, b) in self.pcts.iter_mut().zip(&o.pcts) {
            for (x, y) in a.iter_mut().zip(b) {
                *x += y;
            }
        }
    }
}

fn pc(part: u64, of: u64) -> f64 {
    100.0 * part as f64 / of.max(1) as f64
}

fn print_grades(name: &str, g: &Grades, cands: &[PCand]) {
    if g.plays == 0 {
        return;
    }
    let fcs: u64 = g.fc.iter().sum();
    println!(
        "-- {name}: {} clears; full combos {} ({:.1}%), by accuracy {}: {}",
        g.plays,
        fcs,
        pc(fcs, g.plays),
        FC_BANDS.join(" / "),
        g.fc.map(|v| format!("{:.1}%", pc(v, fcs))).join(" / ")
    );
    println!(
        "   {:<18} {} | up2+ up1 same down1 down2+ (%) | Δ points: mean p10 p50 p90 min max",
        "",
        GRADE_NAMES.map(|n| format!("{n:>5}")).join(" ")
    );
    for (k, c) in cands.iter().enumerate() {
        let at: Vec<String> = g.at[k]
            .iter()
            .map(|&v| format!("{:5.1}", pc(v, g.plays)))
            .collect();
        let mv: Vec<String> = g.moves[k]
            .iter()
            .map(|&v| format!("{:5.1}", pc(v, g.plays)))
            .collect();
        println!(
            "   {:<18} {} | {} | {:+.2} {:+.2} {:+.2} {:+.2} {:+.2} {:+.2}",
            c.name,
            at.join(" "),
            mv.join(" "),
            g.delta_sum[k] as f64 / g.plays as f64 / 100.0,
            g.quantile(k, 0.1),
            g.quantile(k, 0.5),
            g.quantile(k, 0.9),
            g.lo[k] as f64 / 100.0,
            g.hi[k] as f64 / 100.0,
        );
    }
}

/// The thresholds that would keep each letter's share of the clears (by
/// the percent alone: Absolute Dito and the floors aside) where today's
/// percent puts it.
fn print_thresholds(s: &Study) {
    let total: u64 = s.pcts[0].iter().map(|&v| u64::from(v)).sum();
    let at_least = |h: &[u32], t: i64| -> u64 {
        h[t.clamp(0, 10_000) as usize..]
            .iter()
            .map(|&v| u64::from(v))
            .sum()
    };
    let shares: Vec<f64> = TODAY_GRADES
        .iter()
        .map(|&t| at_least(&s.pcts[0], t) as f64 / total.max(1) as f64)
        .collect();
    println!(
        "-- thresholds keeping today's shares of the clears at SSS / SS / S / A / B / C or better by the percent \
         ({})",
        shares
            .iter()
            .map(|v| format!("{:.1}%", 100.0 * v))
            .collect::<Vec<_>>()
            .join(" / ")
    );
    for (k, c) in s.cands.iter().enumerate() {
        let t: Vec<String> = shares
            .iter()
            .map(|&share| {
                // Halfway between the play that keeps that share and the
                // next one below it.
                let want = (share * total as f64).round() as u64;
                let nth = |w: u64| {
                    (0..=10_000i64)
                        .rev()
                        .find(|&t| at_least(&s.pcts[k], t) >= w)
                };
                let hi = nth(want).unwrap_or(0);
                let t = nth(want + 1).map_or(hi, |lo| (hi + lo + 1) / 2);
                text(t)
            })
            .collect();
        println!("   {:<18} {}", c.name, t.join(" "));
    }
}

pub(super) fn print_study(name: &str, s: &Study, levels: bool) {
    let cands = &s.cands;
    println!(
        "== {name}: {} charts (groups), {} clears, {:.0} notes and {:.2} breaks a clear",
        s.charts,
        s.all.plays,
        s.notes as f64 / s.all.plays.max(1) as f64,
        s.breaks as f64 / s.all.plays.max(1) as f64
    );
    print_grades("all", &s.all, cands);
    print_thresholds(s);
    if levels {
        for (i, g) in s.level.iter().enumerate() {
            print_grades(&format!("level {}", i + 1), g, cands);
        }
    }
    for (i, g) in s.kind.iter().enumerate() {
        print_grades(KIND_NAMES[i], g, cands);
    }
    for (i, g) in s.length.iter().enumerate() {
        print_grades(LENGTH_NAMES[i], g, cands);
    }
    let p = &s.pairs;
    println!(
        "-- pairs of one chart's clears, by the more accurate one's lead in points ({}): pairs {:?}",
        GAP_NAMES.join(" / "),
        p.gap
    );
    println!("   the less accurate graded higher (%), every pair:");
    for (k, c) in cands.iter().enumerate() {
        let v: Vec<String> = (0..5)
            .map(|g| format!("{:6.2}", pc(p.inverted[k][g], p.gap[g])))
            .collect();
        println!("   {:<18} {}", c.name, v.join(" "));
    }
    println!(
        "   one play with fewer breaks, the other more accurate (pairs {:?}): the cleaner ranked higher / the \
         more accurate ranked higher, by the percent (%) || by the grade (%)",
        p.clean
    );
    let names: Vec<&str> = cands
        .iter()
        .map(|c| c.name.as_str())
        .chain(BASES.iter().map(|b| b.name))
        .collect();
    for (r, name) in names.iter().enumerate() {
        let by_pct: Vec<String> = (0..5)
            .map(|g| {
                format!(
                    "{:5.1}/{:5.1}",
                    pc(p.pct_x[r][g], p.clean[g]),
                    pc(p.pct_y[r][g], p.clean[g])
                )
            })
            .collect();
        let by_grade: Vec<String> = (0..5)
            .map(|g| {
                format!(
                    "{:5.1}/{:5.1}",
                    pc(p.clean_x[r][g], p.clean[g]),
                    pc(p.clean_y[r][g], p.clean[g])
                )
            })
            .collect();
        println!(
            "   {name:<18} {} || {}",
            by_pct.join(" "),
            by_grade.join(" ")
        );
    }
    println!(
        "   each candidate against the baselines, by the percent: the cleaner ranked higher where no baseline \
         does / the more accurate where every baseline ranks the cleaner higher (% of the pairs, by the lead)"
    );
    for (k, c) in cands.iter().enumerate() {
        let v: Vec<String> = (0..5)
            .map(|g| {
                format!(
                    "{:5.1}/{:5.1}",
                    pc(p.alone[k][g], p.clean[g]),
                    pc(p.against[k][g], p.clean[g])
                )
            })
            .collect();
        println!("   {:<18} {}", c.name, v.join(" "));
    }
}

fn even(n: u32, k: usize) -> Vec<usize> {
    (1..=k).map(|j| j * n as usize / (k + 1)).collect()
}

/// The standard normal's distribution (Abramowitz and Stegun 7.1.26,
/// within 1.5e-7).
fn phi(z: f64) -> f64 {
    let x = z.abs() / std::f64::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736
                + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let erf = 1.0 - poly * (-x * x).exp();
    if z >= 0.0 {
        0.5 * (1.0 + erf)
    } else {
        0.5 * (1.0 - erf)
    }
}

/// Its inverse (Acklam's rational approximation, within 1.2e-9).
fn phi_inv(u: f64) -> f64 {
    const A: [f64; 6] = [
        -39.696_830_286_653_76,
        220.946_098_424_520_5,
        -275.928_510_446_968_7,
        138.357_751_867_269,
        -30.664_798_066_147_16,
        2.506_628_277_459_239,
    ];
    const B: [f64; 5] = [
        -54.476_098_798_224_06,
        161.585_836_858_040_9,
        -155.698_979_859_886_6,
        66.801_311_887_719_72,
        -13.280_681_552_885_72,
    ];
    const C: [f64; 6] = [
        -0.007_784_894_002_430_293,
        -0.322_396_458_041_136_5,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    const D: [f64; 4] = [
        0.007_784_695_709_041_462,
        0.322_467_129_070_039_8,
        2.445_134_137_142_996,
        3.754_408_661_907_416,
    ];
    let u = u.clamp(1e-300, 1.0 - 1e-16);
    let low = 0.024_25;
    if u < low {
        let q = (-2.0 * u.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if u <= 1.0 - low {
        let q = u - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - u).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    }
}

/// W3's bound on Normal, seconds: a full combo's offsets are within it.
const W3: f64 = 11.0 / 120.0;

/// `taps` offsets (seconds) of a made-up player, the quantiles of a normal
/// spread cut at W3 (so every tap keeps the combo): `t` from 0 to 1 a
/// spread of up to 20 ms around the note, from 1 to 2 that spread pushed
/// late by up to 75 ms and narrowed to 10 ms (a sloppy full combo is a
/// player off the beat, not a wider spread: a spread alone stays above
/// about 67%).
fn offsets(taps: usize, t: f64) -> Vec<f64> {
    let (mu, sigma) = if t <= 1.0 {
        (0.0, 0.020 * t.max(1e-6))
    } else {
        (0.075 * (t - 1.0), 0.020 * (1.0 - 0.5 * (t - 1.0)))
    };
    let (a, b) = (phi((-W3 - mu) / sigma), phi((W3 - mu) / sigma));
    (0..taps)
        .map(|i| {
            (mu + sigma * phi_inv(a + (b - a) * (i as f64 + 0.5) / taps as f64)).clamp(-W3, W3)
        })
        .collect()
}

/// A DDI tier on Normal's grid of an offset within W3.
fn ddi_tier_of(d: f64) -> u8 {
    let d = d.abs();
    if d <= 2.0 / 120.0 + 1e-9 {
        0
    } else if d <= 5.0 / 120.0 + 1e-9 {
        1
    } else {
        2
    }
}

/// A made-up play of `n` tap notes, no holds, no W0s (the floors do not
/// come in): misses at `at`, every other tap in combo, its offsets those
/// of [`offsets`] whose today's points come nearest above `points` (so
/// its accuracy is a little above the one asked; the tables say it), so
/// that the other rulesets judge the same presses by their own windows.
/// None where the taps cannot reach them.
fn made_up(n: u32, at: &[usize], points: i64) -> Option<Pp> {
    let b = at.len();
    let taps = n as usize - b;
    let want = points + 10 * b as i64;
    let points_of = |offs: &[f64]| -> i64 {
        offs.iter()
            .map(|&d| super::TAP_POINTS[usize::from(ddi_tier_of(d))])
            .sum()
    };
    if want > 50 * taps as i64 {
        return None;
    }
    // The points fall as `t` grows: the largest `t` still reaching them.
    let (mut lo, mut hi) = (0.0f64, 2.0f64);
    if points_of(&offsets(taps, hi)) >= want {
        lo = hi;
    } else {
        for _ in 0..40 {
            let mid = 0.5 * (lo + hi);
            if points_of(&offsets(taps, mid)) >= want {
                lo = mid
            } else {
                hi = mid
            }
        }
    }
    let offs = offsets(taps, lo);
    if points_of(&offs) < want {
        return None;
    }
    let mut tiers = [0u32; 6];
    let mut k = 0u32;
    let mut combo = 0i64;
    let mut hits = offs.iter();
    let mut moments: Vec<Moment> = Vec::with_capacity(n as usize);
    let mut sum = 0i64;
    for i in 0..n as usize {
        if at.contains(&i) {
            k = 0;
            tiers[5] += 1;
            sum += super::TAP_POINTS[5];
            moments.push((MISS, None));
        } else {
            let d = *hits.next().expect("an offset a tap");
            let t = ddi_tier_of(d);
            tiers[usize::from(t)] += 1;
            sum += super::TAP_POINTS[usize::from(t)];
            k += 1;
            combo += ddi::combo_points(k);
            moments.push((t, Some(d)));
        }
    }
    let (base, base_grade) = baselines(&moments);
    Some(Pp {
        n,
        points: sum,
        most: 50 * i64::from(n),
        combo,
        max: ddi::full_combo_points(n),
        tiers,
        dist: 1000,
        ad: false,
        breaks: b as u32,
        base,
        base_grade,
    })
}

fn base_cell(b: usize, p: &Pp) -> String {
    format!(
        "{:>6} {:<4}",
        text(p.base[b]),
        BASES[b].grades[usize::from(p.base_grade[b])].0
    )
}

fn cell(c: &PCand, p: &Pp) -> String {
    let v = percent(c, p);
    format!("{:>6} {:<3}", text(v), GRADE_NAMES[grade(c, v, p)])
}

/// The least full combo accuracy (today's, %) for which `ok` holds,
/// which grows with the points.
fn least_fc(n: u32, ok: impl Fn(&Pp) -> bool) -> Option<f64> {
    let most = 50 * i64::from(n);
    let reaches = |pts: i64| made_up(n, &[], pts).is_some_and(|p| ok(&p));
    if !reaches(most) {
        return None;
    }
    let actual = |pts: i64| made_up(n, &[], pts).map(|p| 100.0 * p.points as f64 / most as f64);
    let (mut lo, mut hi) = (most / 2, most);
    if reaches(lo) {
        return actual(lo);
    }
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if reaches(mid) { hi = mid } else { lo = mid }
    }
    actual(hi)
}

/// The least full combo accuracy (today's) reaching `target` under `c`.
fn fc_for(c: &PCand, n: u32, target: i64) -> Option<f64> {
    least_fc(n, |p| percent(c, p) >= target)
}

/// Part 6a: worked examples a person can check by hand.
pub(super) fn worked() {
    let cs = cands();
    println!(
        "== worked examples: n tap notes, no holds, no W0s (the floors do not come in); a break a miss; the \
         other taps a made-up player's offsets (a normal spread cut at W3, pushed late below about 90%), \
         its accuracy the least at or above the one asked (today's column says it), judged by each other \
         ruleset's own windows; a cell is the percent and the grade"
    );
    for n in [100u32, 300, 1200] {
        let most = 50 * i64::from(n);
        println!(
            "-- {n} notes: a full combo's part {}, its share under P1 {:.2}%",
            ddi::full_combo_points(n),
            100.0 * ddi::full_combo_points(n) as f64
                / (100 * most + ddi::full_combo_points(n)) as f64
        );
        for c in &cs {
            let mut cells = Vec::new();
            for acc in [50i64, 60, 70, 75, 80, 85, 90, 95, 98] {
                if let Some(p) = made_up(n, &[], most * acc / 100) {
                    cells.push(format!("FC{acc} {}", cell(c, &p)));
                }
            }
            println!("   {:<18} {}", c.name, cells.join(" | "));
        }
        for (b, base) in BASES.iter().enumerate() {
            let mut cells = Vec::new();
            for acc in [50i64, 60, 70, 75, 80, 85, 90, 95, 98] {
                if let Some(p) = made_up(n, &[], most * acc / 100) {
                    cells.push(format!("FC{acc} {}", base_cell(b, &p)));
                }
            }
            println!("   {:<18} {}", base.name, cells.join(" | "));
        }
        for acc in [90i64, 95, 98] {
            for (name, at) in [
                ("1 mid", vec![n as usize / 2]),
                (
                    "1 near end",
                    vec![((n - 1) as f64 * 0.975).round() as usize],
                ),
                ("3 even", even(n, 3)),
                ("5 even", even(n, 5)),
                ("10 even", even(n, 10)),
                ("20 even", even(n, 20)),
            ] {
                let Some(p) = made_up(n, &at, most * acc / 100) else {
                    println!("   {acc}% {name}: not reachable");
                    continue;
                };
                let cells: Vec<String> = cs
                    .iter()
                    .map(|c| format!("{}: {}", c.name, cell(c, &p)))
                    .chain((0..NB).map(|b| format!("{}: {}", BASES[b].name, base_cell(b, &p))))
                    .collect();
                println!("   {acc}% {name}: {}", cells.join(" | "));
            }
        }
    }
    // The floors and SSS: every tap W0 but what is said.
    println!("-- the floors and SSS (every other tap W0, in combo)");
    let all_w0 = |n: u32, at: &[usize], minus: i64, dist: u64, tiers: &[(usize, u32)]| {
        let mut p = made_up(
            n,
            at,
            50 * i64::from(n) - 10 * at.len() as i64 - 50 * at.len() as i64,
        )
        .unwrap();
        p.points = 50 * i64::from(n) - minus;
        p.tiers = [n - at.len() as u32, 0, 0, 0, 0, at.len() as u32];
        for &(t, c) in tiers {
            p.tiers[t] += c;
            p.tiers[0] -= c;
        }
        p.dist = dist;
        p
    };
    let mine = |n: u32| {
        let mut p = all_w0(n, &[], 10, 6, &[]);
        p.combo = 2 * ddi::full_combo_points(n / 2);
        p.breaks = 1;
        p
    };
    let rows: Vec<(String, Pp)> = vec![
        ("100: one W1".into(), all_w0(100, &[], 0, 1, &[])),
        ("100: one W2".into(), all_w0(100, &[], 10, 2, &[(1, 1)])),
        ("100: one W3".into(), all_w0(100, &[], 25, 3, &[(2, 1)])),
        (
            "100: one Miss, middle".into(),
            all_w0(100, &[50], 60, 6, &[]),
        ),
        ("100: one Miss, first".into(), all_w0(100, &[0], 60, 6, &[])),
        ("100: one Miss, last".into(), all_w0(100, &[99], 60, 6, &[])),
        (
            "300: one Miss, middle".into(),
            all_w0(300, &[150], 60, 6, &[]),
        ),
        (
            "1200: one Miss, middle".into(),
            all_w0(1200, &[600], 60, 6, &[]),
        ),
        ("100: a mine, middle".into(), mine(100)),
        ("300: a mine, middle".into(), mine(300)),
        (
            "300: FC 98.5%, no W0".into(),
            made_up(300, &[], 15_000 * 985 / 1000).unwrap(),
        ),
        (
            "300: FC 98.9%, no W0".into(),
            made_up(300, &[], 15_000 * 989 / 1000).unwrap(),
        ),
        (
            "300: 99.5% 1 mid, no W0".into(),
            made_up(300, &[150], 15_000 * 995 / 1000).unwrap(),
        ),
    ];
    for (name, p) in rows {
        let cells: Vec<String> = cs
            .iter()
            .map(|c| format!("{}: {}", c.name, cell(c, &p)))
            .collect();
        println!("   {name}: {}", cells.join(" | "));
    }
    println!(
        "-- the least accuracy (today's, %) at which a full combo (no W0s) reaches SSS / SS / S / A / B / C \
         (every full combo is at least 50%: W3 or better on every note)"
    );
    for n in [100u32, 300, 1200] {
        for c in &cs {
            let need: Vec<String> = c
                .grades
                .iter()
                .map(|&min| fc_for(c, n, min).map_or("     -".into(), |a| format!("{a:6.2}")))
                .collect();
            println!("   {n:>5} {:<18} {}", c.name, need.join(" "));
        }
    }
    println!(
        "-- the exchange: the full combo accuracy (today's, %) whose percent equals a play at 95% and at 98% \
         with 1, 3, 5, 10 breaks (spread evenly, misses), and the letters: FC60 FC70 FC80 FC85 | 95%: 1 3 5 10 | \
         98%: 1 3 5 10"
    );
    for n in [100u32, 300, 1200] {
        let most = 50 * i64::from(n);
        for c in &cs {
            let mut need = Vec::new();
            let mut letters = Vec::new();
            for acc in [60i64, 70, 80, 85] {
                let p = made_up(n, &[], most * acc / 100).unwrap();
                letters.push(GRADE_NAMES[grade(c, percent(c, &p), &p)].to_string());
            }
            letters.push("|".into());
            for acc in [95i64, 98] {
                for b in [1usize, 3, 5, 10] {
                    match made_up(n, &even(n, b), most * acc / 100) {
                        Some(p) => {
                            let v = percent(c, &p);
                            letters.push(GRADE_NAMES[grade(c, v, &p)].to_string());
                            need.push(
                                fc_for(c, n, v).map_or("    -".into(), |a| format!("{a:5.1}")),
                            );
                        }
                        None => {
                            letters.push("-".into());
                            need.push("    -".into());
                        }
                    }
                }
                letters.push("|".into());
            }
            println!(
                "   {n:>5} {:<18} FC needed: 95%: {} | 98%: {} || letters {}",
                c.name,
                need[..4].join(" "),
                need[4..].join(" "),
                letters.join(" ")
            );
        }
        for (b, base) in BASES.iter().enumerate() {
            let mut need = Vec::new();
            let mut letters = Vec::new();
            let letter = |p: &Pp| base.grades[usize::from(p.base_grade[b])].0.to_string();
            for acc in [60i64, 70, 80, 85] {
                letters.push(letter(&made_up(n, &[], most * acc / 100).unwrap()));
            }
            letters.push("|".into());
            for acc in [95i64, 98] {
                for k in [1usize, 3, 5, 10] {
                    match made_up(n, &even(n, k), most * acc / 100) {
                        Some(p) => {
                            letters.push(letter(&p));
                            let target = p.base[b];
                            let found = least_fc(n, |f| f.base[b] >= target);
                            need.push(found.map_or("    -".into(), |a| format!("{a:5.1}")));
                        }
                        None => {
                            letters.push("-".into());
                            need.push("    -".into());
                        }
                    }
                }
                letters.push("|".into());
            }
            println!(
                "   {n:>5} {:<18} FC needed: 95%: {} | 98%: {} || letters {}",
                base.name,
                need[..4].join(" "),
                need[4..].join(" "),
                letters.join(" ")
            );
        }
    }
}

/// A play of `n` tap notes, misses at `at`, the other taps of `counts`
/// W1, W2 and W3 in that order, placed by tier for the baselines.
#[cfg(test)]
fn by_counts(n: u32, at: &[usize], counts: [u32; 3]) -> Pp {
    let mut hits = (0..3).flat_map(|t| std::iter::repeat_n(t as u8, counts[t] as usize));
    let moments: Vec<Moment> = (0..n as usize)
        .map(|i| {
            if at.contains(&i) {
                (MISS, None)
            } else {
                (hits.next().expect("a tap"), None)
            }
        })
        .collect();
    let seq: Vec<u8> = moments.iter().map(|m| m.0).collect();
    let (base, base_grade) = baselines(&moments);
    let mut k = 0;
    let combo = seq
        .iter()
        .map(|&c| {
            if c <= 2 {
                k += 1;
                ddi::combo_points(k)
            } else {
                k = 0;
                0
            }
        })
        .sum();
    Pp {
        n,
        points: seq.iter().map(|&c| super::TAP_POINTS[usize::from(c)]).sum(),
        most: 50 * i64::from(n),
        combo,
        max: ddi::full_combo_points(n),
        tiers: [counts[0], counts[1], counts[2], 0, 0, at.len() as u32],
        dist: 1000,
        ad: false,
        breaks: at.len() as u32,
        base,
        base_grade,
    }
}

#[test]
fn the_percent_candidates_are_what_they_say() {
    let cs = cands();
    // P1's share of a full combo: 5.71% at 100 notes, 11.80% at 300,
    // 18.44% at 1,200.
    for (n, share) in [(100u32, 0.0571), (300, 0.1180), (1200, 0.1844)] {
        let max = ddi::full_combo_points(n) as f64;
        assert!(
            (max / (5_000.0 * f64::from(n) + max) - share).abs() < 0.0001,
            "{n}"
        );
    }
    // Today's percent and grade are the engine's arithmetic.
    let fc = by_counts(300, &[], [270, 30, 0]);
    assert_eq!(percent(&cs[0], &fc), 9800);
    assert_eq!(GRADE_NAMES[today_grade(&fc)], "SS");
    // A full combo only gains; under P1 ≤acc it is its accuracy.
    for c in &cs {
        assert!(percent(c, &fc) >= 9799, "{}", c.name);
    }
    // P1 at 300 notes: a full combo is S from about 87.5%, A from about
    // 77.3%, and every full combo at least C (the made-up player's
    // accuracy is a tap's points apart at most).
    let s = fc_for(&cs[1], 300, 8900).unwrap();
    assert!((s - 87.53).abs() < 0.1, "{s}");
    let a = fc_for(&cs[1], 300, 8000).unwrap();
    assert!((a - 77.32).abs() < 0.1, "{a}");
    assert!(fc_for(&cs[1], 300, 5500).unwrap() < 52.0);
    // The made-up player reaches what it is asked, a little above.
    let broken = made_up(1200, &even(1200, 20), 60_000 * 9 / 10).unwrap();
    let acc = percent(&cs[0], &broken);
    assert!((9000..9010).contains(&acc), "{acc}");
    // The loss caps hold.
    assert!(percent(&cs[1], &broken) < acc - 300);
    assert_eq!(percent(&cs[5], &broken), acc - 200);
    // The flatter table: an all-W3 full combo is 64% under 42/32.
    let sloppy = by_counts(100, &[], [0, 0, 100]);
    let flat = cs
        .iter()
        .find(|c| c.tiers == 1 && c.rule == Rule::Accuracy)
        .unwrap();
    assert_eq!(percent(flat, &sloppy), 6400);
    // The baselines: every tap W1 is the best everywhere; one miss in 100
    // is ITG's 96.6% (★), every tap W2 DDR A's 999,000 (AAA).
    let perfect = made_up(100, &[], 5_000).unwrap();
    assert_eq!(perfect.base, [10_000; NB]);
    assert_eq!(perfect.base_grade, [0; NB]);
    let one_miss = by_counts(100, &[50], [99, 0, 0]);
    assert_eq!(
        (
            one_miss.base[0],
            BASES[0].grades[usize::from(one_miss.base_grade[0])].0
        ),
        (9660, "*")
    );
    let all_w2 = by_counts(100, &[], [0, 100, 0]);
    assert_eq!((all_w2.base[2], all_w2.base_grade[2]), (9990, 0));
    // Dancing☆Onigiri counts the max combo: every tap Matari in one run
    // is (2 + 2) ÷ 10, broken in the middle (2 × 99 + 2 × 50) ÷ 1,000.
    let mut matari: Vec<Moment> = vec![(2, None); 100];
    assert_eq!(BASES[4].judge(&matari).0, 4000);
    matari[50] = (MISS, None);
    assert_eq!(BASES[4].judge(&matari).0, 2980);
    // The floors: one W1 is SSS.
    let mut one_w1 = by_counts(100, &[], [100, 0, 0]);
    one_w1.dist = 1;
    assert_eq!(
        GRADE_NAMES[grade(&cs[1], percent(&cs[1], &one_w1), &one_w1)],
        "SSS"
    );
}
