//! Part 7 of the study (2026-11-17, `docs/plans/ddi-ruleset.md`, "The
//! percent over every kept play"): how the percent is spread over every
//! kept play under each baseline ruleset ([`BASES`]), under DDI as it is,
//! and under DDI with a heavier miss, with the combo in the percent, and
//! both. Measured only: nothing a play scores or shows changes.
//!
//! Its output is aggregates alone, written as JSON to the path
//! `DDI_PCT_DIST_OUT` names (never into the repository): half-point
//! histograms, counts, means, percentiles, shares and rank agreement;
//! never a play's own row, a song's or chart's name, a date or an id.
//!
//! A DDI series' percent, in hundredths, floored: `points'` the play's DDI
//! points with each Miss worth `miss` instead of −10 (and each mine or
//! shock stepped on `miss` too where the series says so, else −10 as
//! today), `most` the chart's, `combo` the play's combo part and `max` a
//! full combo's of the chart's tap-like notes:
//! `(100 − w)% × 10,000 × points' ÷ most + w% × 10,000 × combo ÷ max`
//! (`w` 0: pure accuracy). Grades read from it as today's (thresholds,
//! then the floors by the notes' distance from W0, Absolute Dito by the
//! counts).
//!
//! A play that ended early (failed or quit) counts what it judged over the
//! whole chart, under every series: the notes after it count nothing.

use super::super::{BROKE, MISS, read_kept};
use super::{BASES, Moment, NB, Pp, TODAY_GRADES, div, grade_at};
use ddi_engine::rules::ddi;
use ddi_ui::state::history::Ending;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

/// Half a point a bin, 0 to 100 (100 itself in the last).
const BINS: usize = 200;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Base(usize),
    /// DDI's points with a Miss worth `miss` (today −10), a W4 `w4`
    /// (today 10) and a W5 `w5` (today 0), a mine or shock stepped on
    /// `miss` too where `mines` (else −10, as today), and the combo part's
    /// share of the percent `combo`, in percent.
    ///
    /// `w2` a W2's worth (today 40); `wide` W1 widened to ITG's Fantastic
    /// window ([`ITG_W1`]): the taps DDI judged W2 within it are W1.
    Ddi {
        miss: i64,
        mines: bool,
        combo: i64,
        w4: i64,
        w5: i64,
        w2: i64,
        wide: bool,
    },
}

struct Series {
    id: String,
    name: String,
    kind: Kind,
}

/// A miss costing `notes` notes against the best tier: the note's 50
/// points lost and `50 × (notes − 1)` more.
fn miss_of(tenths: i64) -> i64 {
    50 - 5 * tenths
}

fn notes_text(tenths: i64) -> String {
    if tenths % 10 == 0 {
        format!("{}", tenths / 10)
    } else {
        format!("{}.{}", tenths / 10, tenths % 10)
    }
}

fn series() -> Vec<Series> {
    let mut v: Vec<Series> = [
        ("itg", "ITG (Simply Love)"),
        ("sm5", "StepMania 5"),
        ("ddr-a", "DDR A"),
        ("ddr-extreme", "DDR EXTREME"),
        ("danoni", "Dancing☆Onigiri"),
    ]
    .iter()
    .enumerate()
    .map(|(b, &(id, name))| Series {
        id: id.into(),
        name: name.into(),
        kind: Kind::Base(b),
    })
    .collect();
    let ddi = |id: String, name: String, miss: i64, mines: bool, combo: i64| Series {
        id,
        name,
        kind: Kind::Ddi {
            miss,
            mines,
            combo,
            w4: ddi::TAP_POINTS[3],
            w5: ddi::TAP_POINTS[4],
            w2: ddi::TAP_POINTS[1],
            wide: false,
        },
    };
    let low = |id: &str, name: &str, w4: i64, w5: i64, miss: i64, combo: i64| Series {
        id: id.into(),
        name: name.into(),
        kind: Kind::Ddi {
            miss,
            mines: false,
            combo,
            w4,
            w5,
            w2: ddi::TAP_POINTS[1],
            wide: false,
        },
    };
    let mid = |id: &str, name: &str, w2: i64, wide: bool, combo: i64| Series {
        id: id.into(),
        name: name.into(),
        kind: Kind::Ddi {
            miss: ddi::TAP_POINTS[5],
            mines: false,
            combo,
            w4: ddi::TAP_POINTS[3],
            w5: ddi::TAP_POINTS[4],
            w2,
            wide,
        },
    };
    v.push(ddi("ddi".into(), "DDI today".into(), -10, false, 0));
    for mines in [false, true] {
        for tenths in [20, 30, 34, 50] {
            let n = notes_text(tenths);
            let (id, name) = if mines {
                (
                    format!("ddi-miss{n}-mines"),
                    format!("DDI, a miss and a mine {n} notes"),
                )
            } else {
                (format!("ddi-miss{n}"), format!("DDI, a miss {n} notes"))
            };
            v.push(ddi(id, name, miss_of(tenths), mines, 0));
        }
    }
    for w in [5, 10] {
        v.push(ddi(
            format!("ddi-combo{w}"),
            format!("DDI, combo {w}%"),
            -10,
            false,
            w,
        ));
    }
    for (tenths, w) in [(30, 5), (30, 10), (34, 10)] {
        let n = notes_text(tenths);
        v.push(ddi(
            format!("ddi-miss{n}-combo{w}"),
            format!("DDI, a miss {n} notes, combo {w}%"),
            miss_of(tenths),
            false,
            w,
        ));
    }
    // The low tiers (W4, W5 and the miss) priced otherwise, W0 to W3 as
    // today. ITG's and StepMania 5's own low tiers are taken as shares of
    // their best tier times DDI's 50: ITG's Decent 0 of 5 is 0, its Way
    // Off −6 of 5 is −60, its Miss −12 of 5 −120 (3.4 notes); StepMania
    // 5's grade weights' W4 0 of 2 is 0, W5 −4 of 2 −100, Miss −8 of 2
    // −200 (5 notes).
    v.push(low("ddi-w4-0", "DDI, W4 worth 0", 0, 0, -10, 0));
    v.push(low("ddi-w5-neg", "DDI, W5 worth −25", 10, -25, -10, 0));
    v.push(low(
        "ddi-low-mid",
        "DDI, low tiers between today and ITG (W4 0, W5 −25, a miss 2 notes)",
        0,
        -25,
        miss_of(20),
        0,
    ));
    v.push(low(
        "ddi-low-itg",
        "DDI, low tiers priced as ITG's",
        0,
        -60,
        miss_of(34),
        0,
    ));
    v.push(low(
        "ddi-low-sm5",
        "DDI, low tiers priced as StepMania 5's",
        0,
        -100,
        miss_of(50),
        0,
    ));
    v.push(low(
        "ddi-low-itg-combo5",
        "DDI, low tiers priced as ITG's, combo 5%",
        0,
        -60,
        miss_of(34),
        5,
    ));
    v.push(low(
        "ddi-low-itg-combo10",
        "DDI, low tiers priced as ITG's, combo 10%",
        0,
        -60,
        miss_of(34),
        10,
    ));
    // Which judgment a tap gets, not what it is worth: W1 as wide as
    // ITG's Fantastic; and, for comparison, a flatter W2.
    v.push(mid(
        "ddi-w1-itg",
        "DDI, W1 as wide as ITG's Fantastic (±23 ms)",
        40,
        true,
        0,
    ));
    v.push(mid("ddi-w2-45", "DDI, W2 worth 45", 45, false, 0));
    v.push(mid(
        "ddi-w1-itg-combo10",
        "DDI, W1 as wide as ITG's Fantastic, combo 10%",
        40,
        true,
        10,
    ));
    v
}

/// ITG's Fantastic window, real seconds (`TimingWindowAdd` in).
const ITG_W1: f64 = 0.023;

/// The taps DDI judged W2 that a W1 as wide as [`ITG_W1`] would take: of
/// `moments` (offsets in real seconds), those past DDI's W1 on
/// `difficulty` and within ITG's Fantastic and DDI's W2, at most `w2`
/// (DDI's own W2 count: a play kept under other rules was put on DDI's
/// grid from the same offsets).
fn moved_to_w1(moments: &[Moment], difficulty: ddi::Difficulty, w2: u32) -> u32 {
    let w1 = difficulty.seconds(ddi::TIER_UNITS[1]);
    let outer = ITG_W1.min(difficulty.seconds(ddi::TIER_UNITS[2]));
    let n = moments
        .iter()
        .filter(|m| m.0 != BROKE && m.0 != MISS)
        .filter_map(|m| m.1)
        .filter(|d| d.abs() > w1 + 1e-9 && d.abs() <= outer + 1e-9)
        .count() as u32;
    n.min(w2)
}

/// A DDI series' percent of a play (hundredths, floored).
fn ddi_percent(miss: i64, mines_follow: bool, w: i64, p: &Pp, mines: u32) -> i64 {
    ddi_percent_low(
        ddi::TAP_POINTS[3],
        ddi::TAP_POINTS[4],
        miss,
        mines_follow,
        w,
        p,
        mines,
    )
}

/// The same with a W4 worth `w4` and a W5 `w5`.
fn ddi_percent_low(
    w4: i64,
    w5: i64,
    miss: i64,
    mines_follow: bool,
    w: i64,
    p: &Pp,
    mines: u32,
) -> i64 {
    ddi_percent_all(ddi::TAP_POINTS[1], w4, w5, miss, mines_follow, w, p, mines)
}

/// The same with a W2 worth `w2` too.
#[allow(clippy::too_many_arguments)]
fn ddi_percent_all(
    w2: i64,
    w4: i64,
    w5: i64,
    miss: i64,
    mines_follow: bool,
    w: i64,
    p: &Pp,
    mines: u32,
) -> i64 {
    let extra = (w2 - ddi::TAP_POINTS[1]) * i64::from(p.tiers[1])
        + (w4 - ddi::TAP_POINTS[3]) * i64::from(p.tiers[3])
        + (w5 - ddi::TAP_POINTS[4]) * i64::from(p.tiers[4])
        + (miss - ddi::TAP_POINTS[5]) * i64::from(p.tiers[5])
        + if mines_follow {
            (miss - ddi::MINE_POINTS) * i64::from(mines)
        } else {
            0
        };
    let pts = i128::from(p.points + extra);
    let (most, combo, max) = (i128::from(p.most), i128::from(p.combo), i128::from(p.max));
    if w == 0 || max == 0 {
        div(10_000 * pts, most)
    } else {
        let w = i128::from(w);
        div(100 * ((100 - w) * pts * max + w * combo * most), most * max)
    }
}

/// A play as this part reads it.
struct Play<'a> {
    clear: bool,
    group: &'a str,
    pp: Pp,
    mines: u32,
    /// DDI's full combo: every tap W3 or better, no mine or shock stepped
    /// on (a dropped hold and a Boo leave the combo alone).
    ddi_fc: bool,
    /// Each baseline's percent, grade place and full combo.
    base: [(i64, u8, bool); NB],
    /// Its taps DDI judged W2 that ITG's Fantastic window would take
    /// ([`moved_to_w1`]).
    moved: u32,
}

impl Play<'_> {
    fn percent(&self, k: Kind) -> i64 {
        match k {
            Kind::Base(b) => self.base[b].0,
            Kind::Ddi {
                miss,
                mines,
                combo,
                w4,
                w5,
                w2,
                ..
            } => ddi_percent_all(w2, w4, w5, miss, mines, combo, &self.pp_for(k), self.mines),
        }
    }

    /// What a DDI series reads of it: under a wider W1 the taps it takes
    /// are W1s (50 points, a distance of 1 for the floors); the combo, the
    /// full combo and Absolute Dito are unchanged (W1 and W2 both continue
    /// the combo; W0 is not widened).
    fn pp_for(&self, k: Kind) -> Pp {
        let mut p = self.pp;
        if let Kind::Ddi { wide: true, .. } = k {
            let m = self.moved;
            p.tiers[0] += m;
            p.tiers[1] -= m;
            p.points += (ddi::TAP_POINTS[0] - ddi::TAP_POINTS[1]) * i64::from(m);
            p.dist = p.dist.saturating_sub(u64::from(m));
        }
        p
    }

    /// DDI's breaks: W4, W5, Misses and mines or shocks stepped on.
    fn breaks(&self) -> u32 {
        self.pp.tiers[3] + self.pp.tiers[4] + self.pp.tiers[5] + self.mines
    }

    fn full_combo(&self, k: Kind) -> bool {
        match k {
            Kind::Base(b) => self.base[b].2,
            Kind::Ddi { .. } => self.ddi_fc,
        }
    }
}

/// The moments up to the `taps`th tap judged (what DDI's judgment of a
/// play that ended early judged: a play kept under other rules may have
/// gone on further there).
fn first_taps(moments: &[Moment], taps: u32) -> Vec<Moment> {
    let mut seen = 0u32;
    let mut out = Vec::with_capacity(moments.len());
    for &m in moments {
        if m.0 != BROKE {
            if seen == taps {
                break;
            }
            seen += 1;
        }
        out.push(m);
    }
    out
}

/// Counts and order statistics of values in hundredths.
fn stats(values: &[i64]) -> Value {
    let mut v = values.to_vec();
    v.sort_unstable();
    let mut hist = vec![0u32; BINS];
    let mut below = 0u32;
    for &x in &v {
        if x < 0 {
            below += 1;
        } else {
            hist[((x / 50) as usize).min(BINS - 1)] += 1;
        }
    }
    let q = |q: f64| -> Value {
        if v.is_empty() {
            return Value::Null;
        }
        let pos = q * (v.len() - 1) as f64;
        let lo = pos.floor() as usize;
        let hi = (lo + 1).min(v.len() - 1);
        let x = v[lo] as f64 + (pos - lo as f64) * (v[hi] - v[lo]) as f64;
        json!(round2(x / 100.0))
    };
    let mean = if v.is_empty() {
        Value::Null
    } else {
        json!(round2(
            v.iter().sum::<i64>() as f64 / v.len() as f64 / 100.0
        ))
    };
    json!({
        "count": v.len(),
        "mean": mean,
        "p5": q(0.05),
        "p25": q(0.25),
        "p50": q(0.5),
        "p75": q(0.75),
        "p95": q(0.95),
        "below_0": below,
        "hist": hist,
    })
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

fn round6(x: f64) -> f64 {
    (x * 1_000_000.0).round() / 1_000_000.0
}

/// The tracked groups of clears, by DDI today: id, a plain name, the
/// definition.
const TRACKED: [(&str, &str, &str); 3] = [
    (
        "s_with_breaks",
        "S or better with breaks",
        "clears DDI today grades S or better (S, SS, SSS, Absolute Dito) that are not full combos under DDI's own \
         rule (a W4, W5, Miss, or a mine or shock stepped on)",
    ),
    (
        "s_full_combo",
        "S or better, full combo",
        "clears DDI today grades S or better that are full combos under DDI's own rule",
    ),
    (
        "fc_below_s",
        "Full combos below S",
        "clears that are full combos under DDI's own rule and that DDI today grades below S (the sloppy full \
         combos)",
    ),
];

/// Which of [`TRACKED`] a clear is in (`today` DDI today's kind).
fn tracked_group(p: &Play, today: Kind) -> Option<usize> {
    if !p.clear {
        return None;
    }
    let s_or_better = grade_at(&TODAY_GRADES, p.percent(today), &p.pp) <= 3;
    match (s_or_better, p.ddi_fc) {
        (true, false) => Some(0),
        (true, true) => Some(1),
        (false, true) => Some(2),
        (false, false) => None,
    }
}

fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

fn share(part: usize, of: usize) -> f64 {
    if of == 0 {
        0.0
    } else {
        round4(part as f64 / of as f64)
    }
}

/// The letter-family classes of the summary: 0 S or better (every letter
/// above the ruleset's A family), 1 A, 2 B, 3 C or below.
const CLASSES: [&str; 4] = ["S_or_better", "A", "B", "C_or_below"];

/// Each baseline's grades as [`BASES`] lists them: the name shown, its
/// family, whether it is a sub-step (a plus, a minus, more stars) of that
/// family, and its class.
const BASE_GRADES: [&[(&str, &str, bool, u8)]; NB] = [
    &[
        ("★★★★", "★", true, 0),
        ("★★★", "★", true, 0),
        ("★★", "★", true, 0),
        ("★", "★", false, 0),
        ("S+", "S", true, 0),
        ("S", "S", false, 0),
        ("S-", "S", true, 0),
        ("A+", "A", true, 1),
        ("A", "A", false, 1),
        ("A-", "A", true, 1),
        ("B+", "B", true, 2),
        ("B", "B", false, 2),
        ("B-", "B", true, 2),
        ("C+", "C", true, 3),
        ("C", "C", false, 3),
        ("C-", "C", true, 3),
        ("D", "D", false, 3),
    ],
    &[
        ("AAAA", "AAAA", false, 0),
        ("AAA", "AAA", false, 0),
        ("AA", "AA", false, 0),
        ("A", "A", false, 1),
        ("B", "B", false, 2),
        ("C", "C", false, 3),
        ("D", "D", false, 3),
    ],
    &[
        ("AAA", "AAA", false, 0),
        ("AA+", "AA", true, 0),
        ("AA", "AA", false, 0),
        ("AA-", "AA", true, 0),
        ("A+", "A", true, 1),
        ("A", "A", false, 1),
        ("A-", "A", true, 1),
        ("B+", "B", true, 2),
        ("B", "B", false, 2),
        ("B-", "B", true, 2),
        ("C+", "C", true, 3),
        ("C", "C", false, 3),
        ("C-", "C", true, 3),
        ("D+", "D", true, 3),
        ("D", "D", false, 3),
    ],
    &[
        ("AAA", "AAA", false, 0),
        ("AA", "AA", false, 0),
        ("A", "A", false, 1),
        ("B", "B", false, 2),
        ("C", "C", false, 3),
        ("D", "D", false, 3),
    ],
    &[
        ("AP", "AP", false, 0),
        ("SS", "S", true, 0),
        ("S", "S", false, 0),
        ("SA", "S", true, 0),
        ("AAA", "A", true, 1),
        ("AA", "A", true, 1),
        ("A", "A", false, 1),
        ("B", "B", false, 2),
        ("C", "C", false, 3),
        ("D", "D", false, 3),
    ],
];

/// DDI's grades by the study's index (0 Absolute Dito … 7 D).
const DDI_GRADES: [&str; 8] = ["Absolute Dito", "SSS", "SS", "S", "A", "B", "C", "D"];

fn ddi_class(g: usize) -> u8 {
    match g {
        0..=3 => 0,
        4 => 1,
        5 => 2,
        _ => 3,
    }
}

fn ddi_grades_json(thresholds: &[i64; 6]) -> Value {
    let mut v = vec![json!({
        "name": "Absolute Dito", "family": "Absolute Dito", "sub_step": false, "class": CLASSES[0],
        "floor": 100.0,
        "rule": "every tap and lift W0, no mine or shock stepped on, every hold and roll held, no Boo",
    })];
    for (i, &t) in thresholds.iter().enumerate() {
        let name = DDI_GRADES[i + 1];
        v.push(json!({
            "name": name, "family": name, "sub_step": false,
            "class": CLASSES[usize::from(ddi_class(i + 1))], "floor": t as f64 / 100.0,
        }));
    }
    v.push(json!({
        "name": "D", "family": "D", "sub_step": false, "class": CLASSES[3], "floor": null,
        "rule": "below C, cleared",
    }));
    Value::Array(v)
}

fn base_grades_json(b: usize) -> Value {
    let rules = |name: &str| match (b, name) {
        (1, "AAAA") => Some("every tap W1 (Flawless)"),
        (1, "AAA") => Some("every tap W2 (Perfect) or better"),
        _ => None,
    };
    Value::Array(
        BASES[b]
            .grades
            .iter()
            .zip(BASE_GRADES[b])
            .map(|(&(_, min), &(name, family, sub, class))| {
                let floor = if min == i64::MIN {
                    Value::Null
                } else {
                    json!(min.min(10_000) as f64 / 100.0)
                };
                let mut g = json!({
                    "name": name, "family": family, "sub_step": sub,
                    "class": CLASSES[usize::from(class)], "floor": floor,
                });
                if let Some(r) = rules(name) {
                    g["rule"] = json!(r);
                }
                g
            })
            .collect(),
    )
}

/// The thresholds (hundredths) keeping each of today's letters' shares of
/// the clears (by the percent alone, Absolute Dito and the floors aside),
/// halfway between the play that keeps the share and the next below, as
/// the study's packages were found.
fn matched(today: &[i64], now: &[i64]) -> [i64; 6] {
    let mut d: Vec<i64> = now.iter().map(|&x| x.clamp(0, 10_000)).collect();
    d.sort_unstable_by(|a, b| b.cmp(a));
    let total = today.len();
    TODAY_GRADES
        .map(|t| {
            let want = today.iter().filter(|&&x| x >= t).count();
            if want == 0 {
                return d.first().map_or(t, |&best| t.max(best + 1).min(10_000));
            }
            let hi = d[want - 1];
            d.get(want).map_or(hi, |&lo| (hi + lo + 1) / 2)
        })
        .map(|t| if total == 0 { 0 } else { t })
}

fn definition(s: &Series) -> (String, Value, String, String) {
    match s.kind {
        Kind::Base(0) => (
            "dance points over the most: Fantastic 5, Excellent 4, Great 2, Decent 0, Way Off −6, Miss −12, a mine \
             stepped on −6, over 5 a tap"
                .into(),
            json!({
                "judgments": [["Fantastic", 1.0], ["Excellent", 0.8], ["Great", 0.4], ["Decent", 0.0], ["Way Off", -1.2], ["Miss", -2.4]],
                "mine": -1.2, "miss_costs_notes": 3.4, "combo_in_percent": false,
            }),
            "every tap Great or better (mines do not break it)".into(),
            "taps only (holds and rolls left out); its windows with TimingWindowAdd, judged from the offsets the \
             original judge recorded: an estimate of how ITG would have matched the presses"
                .into(),
        ),
        Kind::Base(1) => (
            "StepMania 5's grade weights over the most: W1 2, W2 2, W3 1, W4 0, W5 −4, Miss −8, a mine −8, over 2 \
             a tap; AAAA every tap W1, AAA every tap W2 or better"
                .into(),
            json!({
                "judgments": [["Flawless", 1.0], ["Perfect", 1.0], ["Great", 0.5], ["Good", 0.0], ["Boo", -2.0], ["Miss", -4.0]],
                "mine": -4.0, "miss_costs_notes": 5.0, "combo_in_percent": false,
            }),
            "every tap Great (W3) or better (mines do not break it)".into(),
            "taps only (holds and rolls left out); StepMania 5's default windows, judged from the offsets the \
             original judge recorded: an estimate"
                .into(),
        ),
        Kind::Base(2) => (
            "the money score over 10,000: 1,000,000 × (Marvelous 5, Perfect 5, Great 3, Good 1, Miss 0) ÷ (5 a \
             tap), less 10 a Perfect, Great and Good, floored to 10"
                .into(),
            json!({
                "judgments": [["Marvelous", 1.0], ["Perfect", 1.0], ["Great", 0.6], ["Good", 0.2], ["Miss", 0.0]],
                "perfect_less": "10 points of 1,000,000 a Perfect, Great and Good",
                "mine": null, "miss_costs_notes": 1.0, "combo_in_percent": false,
            }),
            "every tap Good or better (DDR A's shock rows, a full row of mines, would break it: left out with the \
             mines)"
                .into(),
            "taps only (holds, rolls and mines left out); the windows are the community's approximation, judged \
             from the offsets the original judge recorded: an estimate"
                .into(),
        ),
        Kind::Base(3) => (
            "MAX's dance points over the most: Perfect 2, Great 1, Good 0, Boo −4, Miss −8, a mine −8, over 2 a tap \
             (its score weighs later steps more; its grades read the dance points)"
                .into(),
            json!({
                "judgments": [["Perfect", 1.0], ["Great", 0.5], ["Good", 0.0], ["Boo", -2.0], ["Miss", -4.0]],
                "mine": -4.0, "miss_costs_notes": 5.0, "combo_in_percent": false,
            }),
            "every tap Great or better (mines do not break it)".into(),
            "taps only (holds and rolls left out); no Marvelous in normal play; the windows are the community's \
             approximation, judged from the offsets the original judge recorded: an estimate"
                .into(),
        ),
        Kind::Base(_) => (
            "(8 Ii + 4 Shakin + 2 Matari + 2 × the max combo) ÷ (10 a tap): the longest run counts a fifth".into(),
            json!({
                "judgments": [["Ii", 0.8], ["Shakin", 0.4], ["Matari", 0.2], ["Shobon", 0.0], ["Uwan (miss)", 0.0]],
                "mine": null, "miss_costs_notes": 0.8,
                "combo_in_percent": true,
                "combo": "the max combo, 0.2 of a note's best for each note of the longest run (a full combo 20%)",
            }),
            "every tap Matari or better (Matari neither adds to nor breaks the combo in Dancing☆Onigiri; the \
             study's max combo counts it as continuing it)"
                .into(),
            "taps only (holds' terms left out with the holds); windows k frames early and k + 1 late at 60 fps, \
             judged from the offsets the original judge recorded: an estimate"
                .into(),
        ),
        Kind::Ddi {
            miss,
            mines,
            combo,
            w4,
            w5,
            w2,
            wide,
        } => {
            let worth = miss as f64 / 50.0;
            let lows = w4 != ddi::TAP_POINTS[3] || w5 != ddi::TAP_POINTS[4];
            let notes = 1.0 - worth;
            let mine = if mines { miss } else { ddi::MINE_POINTS };
            let mut def = format!(
                "10,000 × points ÷ most, floored: W0 and W1 50, W2 {w2}, W3 25, W4 {w4}, W5 {w5}, Miss {miss}, a mine \
                 or shock {mine}, a Boo −5, each hold unit earned 50 (a dropped hold earns no more units, as today)"
            );
            if wide {
                def = format!(
                    "{def}; W1 as wide as ITG's Fantastic (±23 ms in real seconds): a tap DDI judged W2 whose offset \
                     is within ±23 ms is a W1 (W0 and W2's outer edge as today)"
                );
            }
            if combo > 0 {
                def = format!(
                    "{}% × that accuracy + {combo}% × the combo part ÷ a full combo's (ddi-combo-v0)",
                    100 - combo
                );
                def = format!(
                    "{def}; the accuracy with W0 and W1 50, W2 {w2}, W3 25, W4 {w4}, W5 {w5}, Miss {miss}, a mine or \
                     shock {mine}, a Boo −5, hold units 50"
                );
            }
            let tiers = json!({
                "judgments": [["W0", 1.0], ["W1", 1.0], ["W2", round4(w2 as f64 / 50.0)], ["W3", 0.5], ["W4", round4(w4 as f64 / 50.0)], ["W5", round4(w5 as f64 / 50.0)], ["Miss", round4(worth)]],
                "mine_or_shock": round4(mine as f64 / 50.0),
                "boo": -0.1,
                "hold_unit": 1.0,
                "dropped_hold": "earns no more units (no penalty of its own)",
                "miss_costs_notes": round4(notes),
                "mine_costs_notes": round4(-(mine as f64) / 50.0),
                "combo_in_percent": combo > 0,
                "combo_share": combo as f64 / 100.0,
            });
            let note = format!(
                "DDI's own judgment of each play (as played, or judged again under DDI from its log); {}{}the \
                 floors, Absolute Dito and the life as today",
                if wide {
                    "only which judgment a tap gets changes: W1 reaches ±23 ms (real seconds, ITG's Fantastic with \
                     TimingWindowAdd) on every difficulty, so a tap DDI judged W2 within it is a W1 (worth 50, a \
                     distance of 1 for the floors); W0 (±8.33 ms on Normal), W2's outer edge (±41.67 ms on Normal) \
                     and every worth as today; from the offsets the original judge recorded (for plays kept under \
                     other rules, at most DDI's own W2 count: an estimate); holds, the combo, full combos and \
                     Absolute Dito unchanged (W1 and W2 both continue the combo); "
                        .to_string()
                } else if w2 != ddi::TAP_POINTS[1] {
                    format!("only W2's worth changes ({w2}, today 40), for comparison; ")
                } else if lows {
                    format!(
                        "only the low tiers change (W4 {w4}, W5 {w5}, a miss {miss}; W0 to W3 as today): a mine or \
                         shock −10, a Boo −5 and a dropped hold as today; "
                    )
                } else if miss == -10 && !mines {
                    String::new()
                } else if mines {
                    format!("a mine or shock costs what a miss does ({miss}); Boos −5 and dropped holds as today; ")
                } else {
                    "only the miss changes: a mine or shock −10, a Boo −5 and a dropped hold as today; ".to_string()
                },
                if combo > 0 {
                    "the combo part as the judge kept it (or worked out from the taps put on DDI's grid for plays \
                     kept before it); "
                } else {
                    ""
                }
            );
            (
                def,
                tiers,
                "every tap W3 or better, no mine or shock stepped on (a dropped hold and a Boo leave the combo \
                 alone)"
                    .into(),
                note,
            )
        }
    }
}

/// Reads the kept plays in `dir` and writes the distributions to `out`.
pub(in super::super) fn run(dir: &Path, out: &Path) {
    let (kept, read) = read_kept(dir);
    let all = series();
    // The chart's tap-like notes for a play that ended early: from a clear
    // of the same notes (the chart's digest and its most), else estimated
    // from its most with the clears' taps' share of theirs.
    let mut taps_of: BTreeMap<&str, u32> = BTreeMap::new();
    let (mut clear_taps, mut clear_most) = (0u64, 0i64);
    for k in kept
        .iter()
        .filter(|k| !k.autoplay && k.clear && k.tally.total_taps() > 0)
    {
        taps_of.insert(k.notes_key.as_str(), k.tally.total_taps());
        clear_taps += u64::from(k.tally.total_taps());
        clear_most += k.pp.most;
    }
    let tap_share = clear_taps as f64 / (clear_most as f64 / 50.0).max(1.0);
    let (mut autoplay, mut nothing, mut from_clear, mut estimated) = (0u32, 0u32, 0u32, 0u32);
    let (mut fails, mut quits, mut small_clears) = (0u32, 0u32, 0u32);
    let mut played_under: BTreeMap<String, u32> = BTreeMap::new();
    let mut points_agree = 0u32;
    let mut plays: Vec<Play> = Vec::new();
    // The percent a baseline's own ruleset kept (holds in) against the
    // baseline's (taps only), over clears played under it: ITG and ITG
    // FA+ (the same dance points) against ITG's, DDR A's money.
    let mut kept_check: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    // DDI's counts summed over the clears and the rest: W1 (W0 too) to
    // Miss, W0, mines, Boos, holds dropped, plays with a Miss.
    let mut sums = [[0u64; 11]; 2];
    let mut grade_agree = (0u32, 0u32);
    for k in &kept {
        if k.autoplay {
            autoplay += 1;
            continue;
        }
        if k.tally.total_taps() == 0 || k.pp.most <= 0 {
            nothing += 1;
            continue;
        }
        let clear = k.clear;
        let judged = k.tally.total_taps();
        let chart_taps = if clear {
            judged
        } else if let Some(&t) = taps_of.get(k.notes_key.as_str()) {
            from_clear += 1;
            t.max(judged)
        } else {
            estimated += 1;
            ((k.pp.most as f64 / 50.0 * tap_share).round() as u32).max(judged)
        };
        if !clear {
            if k.ending == Ending::Quit {
                quits += 1;
            } else {
                fails += 1;
            }
        } else if chart_taps < 20 {
            small_clears += 1;
        }
        *played_under.entry(k.played.0.clone()).or_default() += 1;
        let t = &k.tally;
        let worked_out = t
            .taps
            .iter()
            .zip(ddi::TAP_POINTS)
            .map(|(&n, w)| i64::from(n) * w)
            .sum::<i64>()
            + ddi::BOO_POINTS * i64::from(t.boo)
            + ddi::MINE_POINTS * i64::from(t.mine_hit)
            + ddi::UNIT_POINTS * i64::from(t.hold_units);
        points_agree += u32::from(worked_out == k.pp.points);
        let mut pp = k.pp;
        pp.max = ddi::full_combo_points(chart_taps);
        let moments = if clear {
            k.moments.clone()
        } else {
            first_taps(&k.moments, judged)
        };
        let mut base = [(0i64, 0u8, false); NB];
        for (b, slot) in base.iter_mut().enumerate() {
            *slot = BASES[b].judge_over(&moments, Some(i64::from(chart_taps)));
        }
        if clear {
            grade_agree.1 += 1;
            grade_agree.0 += u32::from(k.grade == Some(super::today_grade(&pp)));
            let own = match (k.played.0.as_str(), k.played.1) {
                ("itg" | "itg-fa+", Some(v)) => v.percent.map(|p| (100.0 * p, base[0].0)),
                ("ddr-a", Some(v)) => v.money.map(|m| (m as f64 / 10_000.0, base[2].0)),
                _ => None,
            };
            if let Some((kept_pct, ours)) = own {
                kept_check
                    .entry(if k.played.0 == "ddr-a" {
                        "ddr-a"
                    } else {
                        k.played.0.as_str()
                    })
                    .or_default()
                    .push(kept_pct - ours as f64 / 100.0);
            }
        }
        let row = &mut sums[usize::from(!clear)];
        for i in 0..6 {
            row[i] += u64::from(t.taps[i]);
        }
        row[6] += u64::from(t.w0);
        row[7] += u64::from(t.mine_hit);
        row[8] += u64::from(t.boo);
        row[9] += u64::from(t.let_go);
        row[10] += u64::from(t.taps[5] > 0);
        plays.push(Play {
            clear,
            group: k.group.as_str(),
            pp,
            mines: t.mine_hit,
            ddi_fc: clear && t.taps[3] + t.taps[4] + t.taps[5] == 0 && t.mine_hit == 0,
            base,
            moved: moved_to_w1(&moments, k.difficulty, t.taps[1]),
        });
    }
    let clears: Vec<&Play> = plays.iter().filter(|p| p.clear).collect();
    let others: Vec<&Play> = plays.iter().filter(|p| !p.clear).collect();
    let today_kind = Kind::Ddi {
        miss: -10,
        mines: false,
        combo: 0,
        w4: ddi::TAP_POINTS[3],
        w5: ddi::TAP_POINTS[4],
        w2: ddi::TAP_POINTS[1],
        wide: false,
    };
    let today: Vec<i64> = clears.iter().map(|p| p.percent(today_kind)).collect();

    let mut out_series = Vec::new();
    let mut summary = Vec::new();
    for s in &all {
        let (def, tiers, fc_rule, note) = definition(s);
        let pct: Vec<i64> = clears.iter().map(|p| p.percent(s.kind)).collect();
        let rest: Vec<i64> = others.iter().map(|p| p.percent(s.kind)).collect();
        let fc: Vec<i64> = clears
            .iter()
            .filter(|p| p.full_combo(s.kind))
            .map(|p| p.percent(s.kind))
            .collect();
        let fc_stats = stats(&fc);
        let mut entry = json!({
            "id": s.id, "name": s.name,
            "kind": if matches!(s.kind, Kind::Base(_)) { "baseline" } else { "ddi" },
            "definition": def, "tiers": tiers, "note": note,
            "clears": stats(&pct),
            "fails_and_quits": stats(&rest),
            "full_combos": { "rule": fc_rule, "count": fc.len(), "hist": fc_stats["hist"].clone(),
                              "below_0": fc_stats["below_0"].clone() },
        });
        let p50 = entry["clears"]["p50"].clone();
        let mut sum = json!({ "id": s.id, "median_clears": p50 });
        let mut matched_th: Option<[i64; 6]> = None;
        match s.kind {
            Kind::Base(b) => {
                let names = BASE_GRADES[b];
                let mut at = vec![0usize; names.len()];
                let mut classes = [0usize; 4];
                for p in &clears {
                    let g = usize::from(p.base[b].1);
                    at[g] += 1;
                    classes[usize::from(names[g].3)] += 1;
                }
                entry["grades"] = base_grades_json(b);
                entry["by_grade"] = Value::Array(
                    names
                        .iter()
                        .zip(&at)
                        .map(|(g, &n)| json!({ "grade": g.0, "count": n, "share": share(n, clears.len()) }))
                        .collect(),
                );
                sum["families"] = families(&classes, clears.len());
            }
            Kind::Ddi {
                miss,
                mines,
                combo,
                w4,
                w5,
                w2,
                wide,
            } => {
                let thresholds_sets: Vec<(&str, [i64; 6])> = if s.kind == today_kind {
                    vec![("by_grade", TODAY_GRADES)]
                } else {
                    vec![
                        ("by_grade", TODAY_GRADES),
                        ("by_grade_matched", matched(&today, &pct)),
                    ]
                };
                entry["grades"] = ddi_grades_json(&TODAY_GRADES);
                for (key, th) in &thresholds_sets {
                    let mut at = [0usize; 8];
                    let mut classes = [0usize; 4];
                    for (p, &v) in clears.iter().zip(&pct) {
                        let g = grade_at(th, v, &p.pp_for(s.kind));
                        at[g] += 1;
                        classes[usize::from(ddi_class(g))] += 1;
                    }
                    entry[*key] = Value::Array(
                        DDI_GRADES
                            .iter()
                            .zip(at)
                            .map(|(g, n)| json!({ "grade": g, "count": n, "share": share(n, clears.len()) }))
                            .collect(),
                    );
                    if *key == "by_grade" {
                        sum["families"] = families(&classes, clears.len());
                    } else {
                        matched_th = Some(*th);
                        entry["grades_matched"] = ddi_grades_json(th);
                        sum["families_matched"] = families(&classes, clears.len());
                    }
                }
                entry["variant"] = json!({
                    "miss_points": miss,
                    "miss_costs_notes": round4(1.0 - miss as f64 / 50.0),
                    "mine_follows_miss": mines,
                    "combo_share_percent": combo,
                    "w4_points": w4,
                    "w5_points": w5,
                    "w4_worth": round4(w4 as f64 / 50.0),
                    "w5_worth": round4(w5 as f64 / 50.0),
                });
                if w2 != ddi::TAP_POINTS[1] {
                    entry["variant"]["w2_points"] = json!(w2);
                    entry["variant"]["w2_worth"] = json!(round4(w2 as f64 / 50.0));
                }
                if wide {
                    let moved: u64 = clears.iter().map(|p| u64::from(p.moved)).sum();
                    let normal = ddi::Difficulty::Normal;
                    entry["variant"]["w1_wide"] = json!(true);
                    entry["variant"]["windows_seconds"] = json!({
                        "w0": round6(normal.seconds(ddi::TIER_UNITS[0])),
                        "w1": ITG_W1,
                        "w1_today": round6(normal.seconds(ddi::TIER_UNITS[1])),
                        "w2": round6(normal.seconds(ddi::TIER_UNITS[2])),
                        "what": "± real seconds; W0 and W2 as DDI Normal's (Easy and Hard scale them; W1 is ±23 \
                                 ms on every difficulty)",
                    });
                    entry["variant"]["moved_w2_to_w1_per_clear"] =
                        json!(round2(moved as f64 / clears.len().max(1) as f64));
                    entry["variant"]["full_combos_unchanged"] = json!(true);
                }
            }
        }
        // The tracked groups (by DDI today) where this series puts them.
        let mut tracked = serde_json::Map::new();
        for (g, (id, _, _)) in TRACKED.iter().enumerate() {
            let members: Vec<&&Play> = clears
                .iter()
                .filter(|p| tracked_group(p, today_kind) == Some(g))
                .collect();
            let values: Vec<i64> = members.iter().map(|p| p.percent(s.kind)).collect();
            let st = stats(&values);
            let top = |th: Option<&[i64; 6]>| {
                members
                    .iter()
                    .filter(|p| match (s.kind, th) {
                        (Kind::Base(b), _) => BASE_GRADES[b][usize::from(p.base[b].1)].3 == 0,
                        (_, Some(th)) => {
                            ddi_class(grade_at(th, p.percent(s.kind), &p.pp_for(s.kind))) == 0
                        }
                        (_, None) => false,
                    })
                    .count()
            };
            let mut t = json!({
                "count": values.len(),
                "hist": st["hist"].clone(),
                "below_0": st["below_0"].clone(),
                "p25": st["p25"].clone(),
                "p50": st["p50"].clone(),
                "p75": st["p75"].clone(),
                "s_or_better_here": share(top(Some(&TODAY_GRADES)), members.len()),
                "full_combo_here": share(
                    members.iter().filter(|p| p.full_combo(s.kind)).count(),
                    members.len()
                ),
            });
            if let Some(th) = &matched_th {
                t["s_or_better_here_matched"] = json!(share(top(Some(th)), members.len()));
            }
            tracked.insert((*id).to_string(), t);
        }
        entry["tracked"] = Value::Object(tracked);
        out_series.push(entry);
        summary.push(sum);
    }

    // Rank agreement: pairs of clears of one chart (under one set of DDI
    // options), each two series ordering them the same way.
    let mut groups: BTreeMap<&str, Vec<&Play>> = BTreeMap::new();
    for p in &clears {
        groups.entry(p.group).or_default().push(p);
    }
    let ns = all.len();
    let (mut pairs, mut multi) = (0u64, 0u64);
    let mut same = vec![vec![0u64; ns]; ns];
    let mut opposite = vec![vec![0u64; ns]; ns];
    for g in groups.values().filter(|g| g.len() >= 2) {
        multi += 1;
        let v: Vec<Vec<i64>> = g
            .iter()
            .map(|p| all.iter().map(|s| p.percent(s.kind)).collect())
            .collect();
        for i in 0..g.len() {
            for j in i + 1..g.len() {
                pairs += 1;
                for a in 0..ns {
                    let sa = v[i][a].cmp(&v[j][a]);
                    if sa.is_eq() {
                        continue;
                    }
                    for b in 0..ns {
                        let sb = v[i][b].cmp(&v[j][b]);
                        if sb.is_eq() {
                            continue;
                        }
                        if sa == sb {
                            same[a][b] += 1;
                        } else {
                            opposite[a][b] += 1;
                        }
                    }
                }
            }
        }
    }
    let agreement: Vec<Vec<Value>> = (0..ns)
        .map(|a| {
            (0..ns)
                .map(|b| {
                    let n = same[a][b] + opposite[a][b];
                    if n == 0 {
                        Value::Null
                    } else {
                        json!(round4(same[a][b] as f64 / n as f64))
                    }
                })
                .collect()
        })
        .collect();
    let untied: Vec<Vec<u64>> = (0..ns)
        .map(|a| (0..ns).map(|b| same[a][b] + opposite[a][b]).collect())
        .collect();

    let mut against = serde_json::Map::new();
    for (id, d) in &kept_check {
        let n = d.len().max(1) as f64;
        let within = |x: f64| share(d.iter().filter(|v| v.abs() <= x).count(), d.len());
        against.insert(
            (*id).to_string(),
            json!({
                "baseline": if *id == "ddr-a" { "ddr-a" } else { "itg" },
                "clears": d.len(),
                "mean_kept_minus_baseline": round2(d.iter().sum::<f64>() / n),
                "mean_abs_difference": round2(d.iter().map(|v| v.abs()).sum::<f64>() / n),
                "within_0_5": within(0.5), "within_1": within(1.0), "within_2": within(2.0),
            }),
        );
    }
    let per_play = |row: &[u64; 11], n: usize| {
        let m = |x: u64| round2(x as f64 / n.max(1) as f64);
        json!({
            "plays": n,
            "taps": m(row[..6].iter().sum()),
            "w1_and_w0": m(row[0]), "w0": m(row[6]), "w2": m(row[1]), "w3": m(row[2]),
            "w4": m(row[3]), "w5": m(row[4]), "miss": m(row[5]),
            "mines_stepped_on": m(row[7]), "boos": m(row[8]), "holds_dropped": m(row[9]),
            "share_with_a_miss": share(row[10] as usize, n),
        })
    };
    let per_clear = |x: u64| round2(x as f64 / clears.len().max(1) as f64);
    let tracked_groups: Vec<Value> = TRACKED
        .iter()
        .enumerate()
        .map(|(g, &(id, name, definition))| {
            let members: Vec<&&Play> = clears.iter().filter(|p| tracked_group(p, today_kind) == Some(g)).collect();
            let n = members.len().max(1) as f64;
            let mut description = definition.to_string();
            if members.len() < 5 {
                description.push_str(
                    "; a very small group (under 5 plays): its bins come close to showing single plays, which \
                     nothing here identifies (no names, dates or charts)",
                );
            }
            json!({
                "id": id,
                "name": name,
                "definition": description,
                "count": members.len(),
                "mean_breaks_ddi": round2(members.iter().map(|p| f64::from(p.breaks())).sum::<f64>() / n),
                "mean_percent_ddi_today": round2(
                    members.iter().map(|p| p.percent(today_kind) as f64).sum::<f64>() / n / 100.0
                ),
            })
        })
        .collect();
    let doc = json!({
        "about": "The percent each ruleset gives the maintainer's kept plays (desktop), aggregates only: \
                  half-point histograms, counts, means and percentiles; no play's own values. Measured, \
                  nothing a play scores changed. From the ddi-session study combo_score_study \
                  (DDI_COMBO_ONLY=distributions), docs/plans/ddi-ruleset.md.",
        "units": "percent, 0 to 100; hist[i] counts the values in [i/2, (i+1)/2), the last bin includes 100; \
                  values below 0 (possible under a heavy miss) are counted in below_0 and in no bin; \
                  percentiles interpolate linearly between the sorted values",
        "bin_width": 0.5,
        "bins": BINS,
        "counts": {
            "files": read.files,
            "unread": read.unread,
            "no_ddi_judgment": read.no_ddi,
            "autoplay": autoplay,
            "nothing_judged": nothing,
            "judged": plays.len(),
            "clears": clears.len(),
            "clears_under_20_notes": small_clears,
            "fails": fails,
            "quits": quits,
            "fails_and_quits": others.len(),
            "ended_early_chart_notes_from_a_clear": from_clear,
            "ended_early_chart_notes_estimated": estimated,
            "played_under": played_under,
            "charts_with_two_clears_or_more": multi,
        },
        "how": {
            "judged": "every kept play with a DDI judgment (as played under DDI, or judged again under DDI from its \
                       log by the game's reworker) and at least one tap judged; autoplays left out",
            "clears": "plays DDI cleared (finished, not failed under DDI's own judgment); the other rulesets' \
                       life bars are not applied, so every series has the same clears",
            "fails_and_quits": "a play that ended early counts what it judged over the whole chart under every \
                                series (the notes after it count nothing); the baselines judge the same taps \
                                DDI's judgment judged; the chart's tap-like notes from a clear of the same notes, \
                                else estimated from its most points with the clears' share of taps",
            "baselines": "over a play's taps alone (holds and rolls left out, mines where the ruleset scores \
                          them), each tap judged by the ruleset's own windows from the offset the original judge \
                          recorded, a miss a miss",
            "grades_matched": "for the DDI variants: thresholds keeping each of today's letters' shares of the \
                               clears, by the percent alone (Absolute Dito and the floors aside), halfway between \
                               the play that keeps the share and the next below; where no clear reaches a letter \
                               today, today's threshold (raised just above the variant's best clear should that \
                               reach it)",
            "families": "S_or_better is every letter above the ruleset's A family (ITG ★ and S; StepMania 5 \
                         AAAA, AAA, AA; DDR A AAA and AA; EXTREME AAA, AA; Dancing☆Onigiri AP, SS, S, SA; DDI \
                         Absolute Dito, SSS, SS, S); C_or_below every letter below B",
        },
        "checks": {
            "ddi_points_from_counts": { "agree": points_agree, "of": plays.len(),
                "what": "DDI's kept points equal to its counts worked out (taps, Boos, mines, hold units)" },
            "today_grade_as_kept": { "agree": grade_agree.0, "of": grade_agree.1 },
            "baselines_against_kept": {
                "by_ruleset_played": against,
                "what": "clears played under a baseline's ruleset: the percent that ruleset's judge kept \
                         (holds and rolls in; ITG FA+ keeps ITG's dance points) against this baseline's (taps \
                         only, from the same offsets), in points",
            },
        },
        "ddi_judgments_per_play": {
            "what": "DDI's counts, a play's mean; share_with_a_miss the share of plays with one Miss or more",
            "clears": per_play(&sums[0], clears.len()),
            "fails_and_quits": per_play(&sums[1], others.len()),
        },
        "ddi_judgments_per_clear": {
            "what": "DDI's own counts, a clear's mean over the clears (W1 those outside W0; taps every tap-like note)",
            "clears": clears.len(),
            "w0": per_clear(sums[0][6]),
            "w1": per_clear(sums[0][0].saturating_sub(sums[0][6])),
            "w2": per_clear(sums[0][1]),
            "w3": per_clear(sums[0][2]),
            "w4": per_clear(sums[0][3]),
            "w5": per_clear(sums[0][4]),
            "miss": per_clear(sums[0][5]),
            "boo": per_clear(sums[0][8]),
            "mines": per_clear(sums[0][7]),
            "taps": per_clear(sums[0][..6].iter().sum()),
        },
        "tracked_groups": tracked_groups,
        "series": out_series,
        "summary": {
            "series": summary,
            "rank_agreement": {
                "what": "pairs of clears of one chart (under one set of DDI options): of the pairs both series \
                         order (neither ties), the share they order the same way",
                "series": all.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
                "pairs": pairs,
                "agreement": agreement,
                "untied_pairs": untied,
            },
        },
    });
    std::fs::write(out, serde_json::to_string_pretty(&doc).expect("JSON"))
        .expect("the output written");
    println!(
        "== distributions: {} files, {} judged ({} clears, {} fails, {} quits), written",
        read.files,
        plays.len(),
        clears.len(),
        fails,
        quits
    );
}

fn families(classes: &[usize; 4], of: usize) -> Value {
    let mut m = serde_json::Map::new();
    for (c, &n) in CLASSES.iter().zip(classes) {
        m.insert((*c).into(), json!(share(n, of)));
    }
    Value::Object(m)
}

#[test]
fn the_distributions_series_are_what_they_say() {
    // By hand: 100 taps, 90 W1, 8 W2, 2 Misses, a mine stepped on, no
    // holds: 4,500 + 320 − 20 − 10 = 4,790 of 5,000.
    let p = Pp {
        n: 100,
        points: 4_790,
        most: 5_000,
        combo: 0,
        max: ddi::full_combo_points(100),
        tiers: [90, 8, 0, 0, 0, 2],
        dist: 1000,
        ad: false,
        breaks: 3,
        base: [0; NB],
        base_grade: [0; NB],
    };
    let pct = |miss_tenths: i64, mines: bool| ddi_percent(miss_of(miss_tenths), mines, 0, &p, 1);
    assert_eq!(miss_of(12), -10);
    assert_eq!(pct(12, false), 9580);
    // A miss 2 notes (−50): 4,500 + 320 − 100 − 10 = 4,710.
    assert_eq!(pct(20, false), 9420);
    // 3 notes (−100): 4,610; with the mine too: 4,520.
    assert_eq!(pct(30, false), 9220);
    assert_eq!(pct(30, true), 9040);
    // 3.4 (−120): 4,570; 5 (−200): 4,410.
    assert_eq!(pct(34, false), 9140);
    assert_eq!(pct(50, false), 8820);
    // The low tiers, by hand: 100 taps, 80 W1, 5 W2, 4 W3, 6 W4, 3 W5, 2
    // Misses, a mine: 4,000 + 200 + 100 + 60 + 0 − 20 − 10 = 4,330.
    let mut low = p;
    low.tiers = [80, 5, 4, 6, 3, 2];
    low.points = 4_330;
    let lows =
        |w4: i64, w5: i64, miss: i64, w: i64| ddi_percent_low(w4, w5, miss, false, w, &low, 1);
    assert_eq!(lows(10, 0, -10, 0), 8660);
    // W4 0: −60 → 4,270. W5 −25: −75 → 4,255.
    assert_eq!(lows(0, 0, -10, 0), 8540);
    assert_eq!(lows(10, -25, -10, 0), 8510);
    // Between (W4 0, W5 −25, Miss −50): 4,330 − 60 − 75 − 80 = 4,115.
    assert_eq!(lows(0, -25, miss_of(20), 0), 8230);
    // ITG's (W4 0, W5 −60, Miss −120): 4,330 − 60 − 180 − 220 = 3,870.
    assert_eq!(lows(0, -60, miss_of(34), 0), 7740);
    // StepMania 5's (W4 0, W5 −100, Miss −200): 4,330 − 60 − 300 − 380 =
    // 3,590.
    assert_eq!(lows(0, -100, miss_of(50), 0), 7180);
    // ITG's with a 10% combo share, a full combo's half: 90% × 77.40 + 5.
    let mut half = low;
    half.combo = half.max / 2;
    let halves = |w: i64| ddi_percent_low(0, -60, miss_of(34), false, w, &half, 1);
    assert_eq!(halves(10), (9 * 7740 + 5_000) / 10);
    assert_eq!(halves(5), (19 * 7740 + 5_000) / 20);
    // The series say those prices.
    let all = series();
    let kind = |id: &str| all.iter().find(|s| s.id == id).unwrap().kind;
    assert_eq!(
        kind("ddi-low-itg"),
        Kind::Ddi {
            miss: -120,
            mines: false,
            combo: 0,
            w4: 0,
            w5: -60,
            w2: 40,
            wide: false,
        }
    );
    assert_eq!(
        kind("ddi-low-sm5"),
        Kind::Ddi {
            miss: -200,
            mines: false,
            combo: 0,
            w4: 0,
            w5: -100,
            w2: 40,
            wide: false,
        }
    );
    assert_eq!(
        kind("ddi-low-mid"),
        Kind::Ddi {
            miss: -50,
            mines: false,
            combo: 0,
            w4: 0,
            w5: -25,
            w2: 40,
            wide: false,
        }
    );
    assert_eq!(
        kind("ddi-w4-0"),
        Kind::Ddi {
            miss: -10,
            mines: false,
            combo: 0,
            w4: 0,
            w5: 0,
            w2: 40,
            wide: false,
        }
    );
    assert_eq!(
        kind("ddi-w5-neg"),
        Kind::Ddi {
            miss: -10,
            mines: false,
            combo: 0,
            w4: 10,
            w5: -25,
            w2: 40,
            wide: false,
        }
    );
    assert_eq!(
        kind("ddi-low-itg-combo10"),
        Kind::Ddi {
            miss: -120,
            mines: false,
            combo: 10,
            w4: 0,
            w5: -60,
            w2: 40,
            wide: false,
        }
    );
    // The combo's 10%: 90% × 95.80 + 10% × combo ÷ max.
    let mut q = p;
    q.combo = q.max / 2;
    assert_eq!(ddi_percent(-10, false, 10, &q, 1), (9 * 9580 + 5_000) / 10);
    // A play ended early: 40 of a chart's 100 taps judged, every one a
    // Fantastic, is 40% under ITG; Dancing☆Onigiri counts the run over the
    // chart too: (8 + 2) × 40 ÷ 1,000.
    let moments: Vec<Moment> = vec![(0, Some(0.0)); 40];
    assert_eq!(BASES[0].judge_over(&moments, Some(100)).0, 4000);
    assert_eq!(BASES[4].judge_over(&moments, Some(100)).0, 4000);
    assert!(BASES[0].judge_over(&moments, None).2);
    // A Great keeps ITG's combo, not EXTREME's; a miss breaks every one.
    let mut great = moments.clone();
    great[3] = (2, Some(0.08));
    assert!(BASES[0].judge_over(&great, None).2);
    assert!(!BASES[3].judge_over(&great, None).2);
    great[5] = (MISS, None);
    assert!((0..NB).all(|b| !BASES[b].judge_over(&great, None).2));
    // The moments up to DDI's taps keep the mines between.
    let mixed: Vec<Moment> = vec![(0, None), (BROKE, None), (0, None), (0, None)];
    assert_eq!(first_taps(&mixed, 2).len(), 3);
    // The thresholds that keep today's shares: the same values keep them.
    let v: Vec<i64> = (0..100).map(|i| 5_000 + 50 * i).collect();
    assert_eq!(
        matched(&v, &v).map(|t| v.iter().filter(|&&x| x >= t).count()),
        TODAY_GRADES.map(|t| v.iter().filter(|&&x| x >= t).count())
    );
    // The histogram's bins: 99.99 and 100 in the last, −0.01 below.
    let s = stats(&[9_999, 10_000, -1, 0, 49, 50]);
    assert_eq!(s["below_0"], 1);
    assert_eq!(s["hist"][199], 2);
    assert_eq!(s["hist"][0], 2);
    assert_eq!(s["hist"][1], 1);

    // A tracked play by hand: 100 taps, 60 at 0 ms (W1, none in W0 for
    // the sake of the floors), 20 at 20 ms and 19 at 30 ms (DDI's W2 on
    // Normal), a Miss: 3,000 + 1,560 − 10 = 4,550 of 5,000, 91.00: an S
    // with a break, so in `s_with_breaks`.
    let mut moments: Vec<Moment> = vec![(0, Some(0.0)); 60];
    moments.extend(vec![(1, Some(0.020)); 20]);
    moments.extend(vec![(1, Some(-0.030)); 19]);
    moments.push((MISS, None));
    let mut base = [(0i64, 0u8, false); NB];
    for (b, slot) in base.iter_mut().enumerate() {
        *slot = BASES[b].judge_over(&moments, Some(100));
    }
    let play = Play {
        clear: true,
        group: "",
        pp: Pp {
            n: 100,
            points: 4_550,
            most: 5_000,
            combo: ddi::full_combo_points(50),
            max: ddi::full_combo_points(100),
            tiers: [60, 39, 0, 0, 0, 1],
            dist: 60 + 2 * 39 + 6,
            ad: false,
            breaks: 1,
            base: [0; NB],
            base_grade: [0; NB],
        },
        mines: 0,
        ddi_fc: false,
        base,
        moved: moved_to_w1(&moments, ddi::Difficulty::Normal, 39),
    };
    let today = all.iter().find(|s| s.id == "ddi").unwrap().kind;
    assert_eq!(play.percent(today), 9100);
    assert_eq!(tracked_group(&play, today), Some(0));
    assert_eq!(play.breaks(), 1);
    // ITG: 60 + 20 Fantastics (23 ms), 19 Excellents, a Miss: (400 + 76 −
    // 12) ÷ 500 = 92.80, bin 185, its S (S from 92).
    assert_eq!(play.percent(Kind::Base(0)), 9280);
    assert_eq!(stats(&[play.percent(Kind::Base(0))])["hist"][185], 1);
    assert_eq!(BASES[0].grades[usize::from(play.base[0].1)].0, "S");
    // DDR A: 60 Marvelous, 39 Perfects, a Miss: 1,000,000 × 495 ÷ 500 −
    // 390 = 989,610, 98.96, bin 197.
    assert_eq!(play.percent(Kind::Base(2)), 9896);
    assert_eq!(stats(&[play.percent(Kind::Base(2))])["hist"][197], 1);
    // The wider W1 takes the 20 at 20 ms (past 16.67, within 23), not the
    // 30 ms ones: 4,550 + 200 = 4,750, 95.00; its floors' distance 20
    // less; the full combo unchanged.
    assert_eq!(play.moved, 20);
    let wide = kind("ddi-w1-itg");
    assert_eq!(play.percent(wide), 9500);
    assert_eq!(play.pp_for(wide).dist, play.pp.dist - 20);
    assert_eq!(play.pp_for(wide).tiers, [80, 19, 0, 0, 0, 1]);
    assert_eq!(play.full_combo(wide), play.full_combo(today));
    // W2 at 45: 4,550 + 39 × 5 = 4,745, 94.90.
    assert_eq!(play.percent(kind("ddi-w2-45")), 9490);
    // The wider W1 with the 10% combo share, the combo half a full
    // combo's: 90% × 95.00 + 10% × (combo ÷ max).
    let c = kind("ddi-w1-itg-combo10");
    let share_of = 10_000 * ddi::full_combo_points(50) / ddi::full_combo_points(100);
    assert!(
        (play.percent(c) - (9 * 9500 + share_of) / 10).abs() <= 1,
        "{}",
        play.percent(c)
    );
    // Hard's W1 (13.9 ms) loses the 20 ms taps to the wider W1 too; a
    // 22 ms tap on Easy (W1 22.2 ms) is W1 already.
    assert_eq!(moved_to_w1(&moments, ddi::Difficulty::Hard, 39), 20);
    assert_eq!(
        moved_to_w1(&[(1, Some(0.022))], ddi::Difficulty::Easy, 1),
        0
    );
}
