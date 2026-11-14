//! DDI, the game's own ruleset (`docs/plans/ddi-ruleset.md`): windows on
//! a grid of 1/120 s (1/144 on Hard, 1/90 on Easy), an early and a late
//! word for every tier, combo to Great, optional Boos with their
//! exemptions, the lane locked by a mine stepped on, holds earning a unit
//! a second, a life of 0 to 100, one point count shown three ways, grades
//! with floors and DDR's full-combo lamps with FFR-style flags.
//!
//! Values marked *tune* in the plan are the named constants below, to be
//! changed in one place.
//!
//! The judge carries the two rules no other ruleset has:
//! [`EmptyPress::Guarded`] (the Boos' exemptions) and
//! [`JudgeTable::lock`] (the lane lock). Both follow from the chart and
//! the presses and releases alone, so a play's log judges again as the
//! play did with no new log version.

use serde::{Deserialize, Serialize};

use super::{
    ComboRules, EmptyPress, FailPolicy, GaugeRules, GradeBasis, GradeRule, GradeTable, JudgeNames,
    JudgeTable, Judgement, Ruleset, ScoreCtx, ScoreRules, ScoreView, StepManiaOptions, Tally,
    Window,
};
use crate::judge::{JudgeEvent, JudgeEventKind};

/// The ruleset's id.
pub const ID: &str = "ddi";

/// Grid units of each tier's window, W0 first (W1–W5 then follow).
pub const TIER_UNITS: [u32; 6] = [1, 2, 5, 11, 17, 25];
/// Grid units a released hold survives for.
pub const HOLD_GRACE_UNITS: u32 = 38;
/// Grid units a roll survives between taps.
pub const ROLL_GRACE_UNITS: u32 = 42;
/// The mine window, seconds, on every difficulty (StepMania's
/// `TimingWindowSecondsMine`).
pub const MINE_WINDOW: f64 = 0.075;
/// Seconds a lane is locked once a mine is stepped on in it. *Tune.*
pub const LANE_LOCK: f64 = 1.0;
/// A press this close to any mine, in any lane, is never a Boo. *Tune.*
pub const BOO_NEAR_MINE: f64 = 0.5;
/// A press with no note of any lane this close is never a Boo.
pub const BOO_QUIET: f64 = 3.0;
/// A press this long or less before a lift in its lane readies it: never
/// a Boo. *Tune.*
pub const BOO_LIFT_READY: f64 = 1.0;

/// The life at the start, and the most.
pub const LIFE_START: f64 = 50.0;
pub const LIFE_MAX: f64 = 100.0;
/// What a hit adds to the life, W1 (W0 too) to W4; never scaled.
pub const LIFE_GAINS: [f64; 4] = [2.0, 2.0, 1.0, 0.0];
/// What a hold or roll held adds. *Tune.*
pub const LIFE_HELD: f64 = 2.0;
/// What each loss takes on Normal, before the difficulty's factor.
pub const LIFE_W5: f64 = 5.0;
pub const LIFE_MISS: f64 = 10.0;
pub const LIFE_BOO: f64 = 2.0;
pub const LIFE_MINE: f64 = 5.0;
pub const LIFE_DROPPED: f64 = 5.0;
/// Below this life the gauge is in danger (StepMania's
/// `LifeMeterBar` `DangerThreshold`, 0.2). *Tune.*
pub const LIFE_DANGER: f64 = 20.0;

/// Points of a tap-like note by tier, W1 (W0 too) to Miss.
pub const TAP_POINTS: [i64; 6] = [50, 40, 25, 10, 0, -10];
pub const BOO_POINTS: i64 = -5;
pub const MINE_POINTS: i64 = -10;
/// Points of a hold's or roll's unit earned.
pub const UNIT_POINTS: i64 = 50;
/// The most a tap-like note gives.
pub const MOST_TAP_POINTS: i64 = 50;

/// The grade every tap and lift W0, no mine stepped on and every hold
/// and roll held.
pub const ABSOLUTE_DITO: &str = "Absolute Dito";
/// [`ABSOLUTE_DITO`] by an autoplay or a CPU.
pub const ABSOLUTE_BOT: &str = "Absolute Bot";
/// The failed grade.
pub const FAILED: &str = "F";
/// The grades below Absolute Dito, best first, and the percent each
/// needs in hundredths of a percent (D: below C, cleared).
///
/// Softened on 2026-11-12 from 99.5, 98, 95, 90, 80, 70: over the
/// maintainer's kept plays the percent barely moved from ITG's while the
/// letters fell a whole grade, ITG's letters asking less of the same
/// percent (its S- from 89, A- from 80, B- from 68).
pub const GRADES: [(&str, i64); 7] = [
    ("SSS", 9900),
    ("SS", 9600),
    ("S", 8900),
    ("A", 8000),
    ("B", 6800),
    ("C", 5500),
    ("D", i64::MIN),
];

/// Early words by tier, W1 to Miss (W0 is [`W0_WORD`]).
pub const EARLY_WORDS: [&str; 6] = ["Awesome", "Fantastic", "Nice", "OK", "Too early", "Miss"];
/// Late words by tier: each has a G, no early word does.
pub const LATE_WORDS: [&str; 6] = ["Amazing", "Glorious", "Great", "Good", "Too late", "Miss"];
/// W0's one word.
pub const W0_WORD: &str = "Absolute";
/// What stray presses are called, as the results count them.
pub const BOOS_WORD: &str = "Boos";
/// One of them, as a play shows it where the tap judgments show
/// ([`JudgeNames::boo`]).
pub const BOO_WORD: &str = "Boo";
pub const HELD_WORD: &str = "Held";
pub const DROPPED_WORD: &str = "Dropped";

/// Easy, Normal or Hard: the grid every window is on and the life
/// losses' factor.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Difficulty {
    Easy,
    #[default]
    Normal,
    Hard,
}

impl Difficulty {
    pub const ALL: [Difficulty; 3] = [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard];

    /// Grid steps a second: a window of `u` units is `u / hz()` seconds.
    pub fn hz(self) -> f64 {
        match self {
            Difficulty::Easy => 90.0,
            Difficulty::Normal => 120.0,
            Difficulty::Hard => 144.0,
        }
    }

    /// `units` grid steps, seconds.
    pub fn seconds(self, units: u32) -> f64 {
        f64::from(units) / self.hz()
    }

    /// What the life's losses are multiplied by (gains never are). *Tune.*
    pub fn loss_factor(self) -> f64 {
        match self {
            Difficulty::Easy => 0.2,
            Difficulty::Normal => 1.0,
            Difficulty::Hard => 2.0,
        }
    }
}

/// DDI's options: those in a play's [`super::RulesetKey`]. The score's
/// display and the words' language are the screens', not these.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct DdiOptions {
    #[serde(deserialize_with = "crate::lenient")]
    pub difficulty: Difficulty,
    /// Stray presses are Boos ([`EmptyPress::Guarded`]); off by default.
    pub boos: bool,
    /// Reaching 0 life does not end the play: it goes on at 0 to the end,
    /// a failure ([`FailPolicy::ImmediateContinue`]).
    pub practice: bool,
}

impl DdiOptions {
    /// What bests are kept under: the difficulty and Boos, not Practice
    /// (a failure either way).
    pub fn best(self) -> DdiOptions {
        DdiOptions {
            practice: false,
            ..self
        }
    }
}

/// A hold's or roll's units: `ceil(length)` of its length in the chart's
/// seconds (from the chart's timing, a stop's time included; a music rate
/// never changes it), to the microsecond, at least one.
pub fn hold_units(length: f64) -> u32 {
    let micros = (length.max(0.0) * 1e6).round() as u64;
    (micros.div_ceil(1_000_000)).clamp(1, u64::from(u32::MAX)) as u32
}

/// The units a hold from `head` to `end` (song seconds) earned: one at
/// each whole second after its head and one at its tail, each while it
/// was still alive there. `dropped_at`: when it was dropped (alive at a
/// checkpoint strictly before); `None`: held to its tail, every unit.
pub fn units_earned(head: f64, end: f64, dropped_at: Option<f64>) -> u32 {
    let units = hold_units(end - head);
    match dropped_at {
        None => units,
        Some(at) => (1..units).take_while(|&k| head + f64::from(k) < at).count() as u32,
    }
}

/// DDI's point count and its views ([`ScoreView::ddi`]). All may be
/// negative.
///
/// Kept with a play as `points`, `most` and `absolute` alone say it: the
/// rest is worked out again when it is read ([`Stored`]), so a play kept
/// before the score became the sum (its `score` was the proportional
/// part) shows the sum at once.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Stored")]
pub struct DdiScoreView {
    /// Points so far ([`TAP_POINTS`], Boos, mines, hold units).
    pub points: i64,
    /// The most the chart gives: 50 a tap-like note, 50 a hold unit.
    pub most: i64,
    /// The proportional part: `floor(1,000,000 × points ÷ most)`, the
    /// same for every chart played as well. What compares across charts,
    /// and across competitors whose copies of a chart differ.
    pub ratio: i64,
    /// The percent in hundredths, floored (`9950` is 99.50 %).
    pub percent: i64,
    /// `100 × points + W0s`: not divided by the chart's size.
    pub absolute: i64,
    /// The score as shown: `ratio + absolute`, so a longer chart gives
    /// more (a perfect 1,000-note chart about 6,000,000).
    pub score: i64,
}

/// A [`DdiScoreView`] as read from a store: what it is worked out from
/// (the W0s are the absolute score's remainder), whatever else was kept.
#[derive(Default, Deserialize)]
#[serde(default)]
struct Stored {
    points: i64,
    most: i64,
    absolute: i64,
}

impl From<Stored> for DdiScoreView {
    fn from(s: Stored) -> DdiScoreView {
        let w0 = s.absolute.saturating_sub(s.points.saturating_mul(100));
        DdiScoreView::of(s.points, s.most, w0.clamp(0, i64::from(u32::MAX)) as u32)
    }
}

impl DdiScoreView {
    pub fn of(points: i64, most: i64, w0: u32) -> DdiScoreView {
        let ratio = |scale: i64| {
            if most > 0 {
                (i128::from(points) * i128::from(scale)).div_euclid(i128::from(most)) as i64
            } else {
                0
            }
        };
        let absolute = 100 * points + i64::from(w0);
        DdiScoreView {
            points,
            most,
            ratio: ratio(1_000_000),
            percent: ratio(10_000),
            absolute,
            score: ratio(1_000_000) + absolute,
        }
    }

    /// How two plays' scores compare for a best: the higher score (the
    /// sum, which carries the absolute score's W0s as its tie-break),
    /// then the absolute score (the best lamp is kept apart, [`Lamp`]'s
    /// order).
    pub fn best_cmp(&self, other: &DdiScoreView) -> std::cmp::Ordering {
        (self.score, self.absolute).cmp(&(other.score, other.absolute))
    }

    /// The percent as a fraction (`0.995`).
    pub fn fraction(&self) -> f64 {
        self.percent as f64 / 10_000.0
    }

    /// The percent as shown, two decimals ("99.50").
    pub fn percent_text(&self) -> String {
        let sign = if self.percent < 0 { "-" } else { "" };
        let p = self.percent.unsigned_abs();
        format!("{sign}{}.{:02}", p / 100, p % 100)
    }
}

/// How a CPU's score was put beside a person's in a multiplayer stage
/// (round 3b, RETIRED 2026-11-14: `docs/plans/multiplayer-prototype-progress.md`,
/// "CPUs play the full chart"). A CPU played a tap-only copy of the
/// stage's chart, so its score was put on the chart's own scale (its
/// points as the same share of the chart's most, its W0s as counted).
/// Since `cpu-play-v1` a CPU plays the chart a person is given, its
/// score is its own, and nothing puts a score on another chart's scale:
/// the name is kept so that it is not used again for something else.
pub const CPU_SCORE_V0: &str = "cpu-score-v0";

/// DDI's point count ([`DdiScoreView`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DdiScore {
    points: i64,
    most: i64,
    w0: u32,
}

impl DdiScore {
    pub fn new() -> DdiScore {
        DdiScore::default()
    }
}

/// The points an event gives (a hold's by `ctx.units`).
fn points_of(kind: &JudgeEventKind, ctx: &ScoreCtx) -> i64 {
    match kind {
        JudgeEventKind::Tap(j, _) => TAP_POINTS[j.index()],
        JudgeEventKind::Boo => BOO_POINTS,
        JudgeEventKind::HitMine | JudgeEventKind::HitShock => MINE_POINTS,
        JudgeEventKind::Held | JudgeEventKind::LetGo => UNIT_POINTS * i64::from(ctx.units),
        JudgeEventKind::AvoidShock | JudgeEventKind::Checkpoint { .. } => 0,
    }
}

impl ScoreRules for DdiScore {
    fn begin(&mut self, ctx: &ScoreCtx) {
        self.points = 0;
        self.w0 = 0;
        self.most =
            MOST_TAP_POINTS * i64::from(ctx.steps) + UNIT_POINTS * i64::from(ctx.hold_units);
    }

    fn on_event(&mut self, ev: &JudgeEvent, ctx: &ScoreCtx) {
        self.points += points_of(&ev.kind, ctx);
        if ctx.w0 {
            self.w0 += 1;
        }
    }

    fn restep(&mut self, ctx: &ScoreCtx) {
        self.most =
            MOST_TAP_POINTS * i64::from(ctx.steps) + UNIT_POINTS * i64::from(ctx.hold_units);
    }

    fn view(&self) -> ScoreView {
        let ddi = DdiScoreView::of(self.points, self.most, self.w0);
        ScoreView {
            percent: Some(ddi.fraction()),
            ddi: Some(ddi),
            ..ScoreView::default()
        }
    }

    fn fresh(&self) -> Box<dyn ScoreRules> {
        Box::new(DdiScore::new())
    }
}

/// DDI's life: 0 to 100 from 50, kept in tenths so every step is exact;
/// failed at 0.
#[derive(Clone, Debug, PartialEq)]
pub struct DdiLife {
    /// Tenths of a life point.
    life: i64,
    /// The losses in tenths, the difficulty's factor applied: W5, Miss,
    /// Boo, mine, a hold dropped.
    losses: [i64; 5],
}

fn tenths(points: f64) -> i64 {
    (points * 10.0).round() as i64
}

impl DdiLife {
    pub fn new(difficulty: Difficulty) -> DdiLife {
        let f = difficulty.loss_factor();
        DdiLife {
            life: tenths(LIFE_START),
            losses: [LIFE_W5, LIFE_MISS, LIFE_BOO, LIFE_MINE, LIFE_DROPPED].map(|l| tenths(l * f)),
        }
    }

    /// The life, 0 to 100.
    pub fn points(&self) -> f64 {
        self.life as f64 / 10.0
    }
}

impl GaugeRules for DdiLife {
    fn begin(&mut self, _ctx: &ScoreCtx) {
        self.life = tenths(LIFE_START);
    }

    fn on_event(&mut self, ev: &JudgeEvent, _ctx: &ScoreCtx) {
        let [w5, miss, boo, mine, dropped] = self.losses;
        let delta = match ev.kind {
            JudgeEventKind::Tap(j, _) => match j {
                Judgement::W5 => -w5,
                Judgement::Miss => -miss,
                j => tenths(LIFE_GAINS[j.index()]),
            },
            JudgeEventKind::Boo => -boo,
            JudgeEventKind::HitMine | JudgeEventKind::HitShock => -mine,
            JudgeEventKind::LetGo => -dropped,
            JudgeEventKind::Held => tenths(LIFE_HELD),
            JudgeEventKind::AvoidShock | JudgeEventKind::Checkpoint { .. } => 0,
        };
        self.life = (self.life + delta).clamp(0, tenths(LIFE_MAX));
    }

    fn life(&self) -> f32 {
        (self.life as f64 / tenths(LIFE_MAX) as f64) as f32
    }

    fn failed(&self) -> bool {
        self.life <= 0
    }

    fn danger(&self) -> bool {
        self.life > 0 && self.life < tenths(LIFE_DANGER)
    }

    fn danger_line(&self) -> Option<f32> {
        Some((LIFE_DANGER / LIFE_MAX) as f32)
    }

    fn set_life(&mut self, life: f32) -> bool {
        self.life = (f64::from(life.clamp(0.0, 1.0)) * tenths(LIFE_MAX) as f64).round() as i64;
        true
    }

    fn fresh(&self) -> Box<dyn GaugeRules> {
        Box::new(DdiLife {
            life: tenths(LIFE_START),
            losses: self.losses,
        })
    }
}

/// Whether a play is Absolute Dito: a tap or a lift judged, every one
/// inside W0, no hold or roll dropped, no mine stepped on, no Boo (Boos
/// count against perfection).
pub fn absolute(tally: &Tally) -> bool {
    tally.total_taps() > 0
        && tally.w0 == tally.total_taps()
        && tally.let_go == 0
        && tally.mine_hit == 0
        && tally.boo == 0
        && tally.checkpoint_miss == 0
}

/// The sum of the distances from W0 the floors count: a W1 outside W0 1
/// … a W5 5, a Miss, a hold dropped, a mine stepped on and a Boo 6. A
/// missed hold head is its Miss alone (the hold is not also dropped).
pub fn distance(tally: &Tally) -> u64 {
    let w1 = tally.count(Judgement::W1).saturating_sub(tally.w0);
    let tiers = [
        w1,
        tally.count(Judgement::W2),
        tally.count(Judgement::W3),
        tally.count(Judgement::W4),
        tally.count(Judgement::W5),
        tally.count(Judgement::Miss),
    ];
    let taps: u64 = tiers
        .iter()
        .zip(1u64..)
        .map(|(&n, d)| u64::from(n) * d)
        .sum();
    taps + 6 * (u64::from(tally.let_go) + u64::from(tally.mine_hit) + u64::from(tally.boo))
}

/// A play's grade: F when failed; Absolute Dito; else the better of the
/// percent's grade and the floor, Absolute Dito less the notes'
/// [`distance`] (never above SSS, never below D).
pub fn grade(view: &ScoreView, tally: &Tally, failed: bool) -> &'static str {
    if failed {
        return FAILED;
    }
    if absolute(tally) {
        return ABSOLUTE_DITO;
    }
    let percent = view.ddi.map_or(0, |d| d.percent);
    let by_percent = GRADES
        .iter()
        .position(|&(_, min)| percent >= min)
        .unwrap_or(GRADES.len() - 1);
    // Absolute Dito is step 0, SSS step 1, … D step 7.
    let floor = distance(tally).clamp(1, GRADES.len() as u64) as usize - 1;
    GRADES[by_percent.min(floor)].0
}

/// [`ABSOLUTE_BOT`] for an autoplay's or a CPU's Absolute Dito, any other
/// grade as it is: what a play judged again from its log (which does not
/// say who played) is graded when its record says it was autoplayed.
pub fn bot_grade(grade: &str) -> &str {
    if grade == ABSOLUTE_DITO {
        ABSOLUTE_BOT
    } else {
        grade
    }
}

/// A play's lamp, worst first. Absolute Dito is the grade's; the flags
/// are one judgment short of the full combo above them.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Lamp {
    Failed,
    Clear,
    /// Exactly one break ([`breaks`]), the rest a full combo.
    BreakFlag,
    /// No break, the worst a W3.
    FullCombo,
    /// No break, one W3, the rest W2 or better.
    GreatFlag,
    /// No break, the worst a W2.
    GloriousFullCombo,
    /// No break, one W2, the rest W1 or better.
    GloriousFlag,
    /// No break, every tap W1 or better.
    AmazingFullCombo,
    AbsoluteDito,
}

/// The breaks a lamp counts: what breaks the combo in play (a W4, a W5,
/// a Miss, a mine stepped on, `shocks` a battle's shocks stepped on),
/// and a hold or roll dropped and a Boo, which leave the combo alone.
pub fn breaks(tally: &Tally, shocks: u32) -> u64 {
    [
        tally.count(Judgement::W4),
        tally.count(Judgement::W5),
        tally.count(Judgement::Miss),
        tally.mine_hit,
        tally.let_go,
        tally.boo,
        shocks,
    ]
    .iter()
    .map(|&n| u64::from(n))
    .sum()
}

impl Lamp {
    /// The lamp of a play: its counts, whether it failed and the battle's
    /// shocks stepped on.
    pub fn of(tally: &Tally, failed: bool, shocks: u32) -> Lamp {
        if failed {
            return Lamp::Failed;
        }
        if tally.total_taps() == 0 {
            return Lamp::Clear;
        }
        match breaks(tally, shocks) {
            0 => {}
            1 => return Lamp::BreakFlag,
            _ => return Lamp::Clear,
        }
        if shocks == 0 && absolute(tally) {
            return Lamp::AbsoluteDito;
        }
        match (tally.count(Judgement::W2), tally.count(Judgement::W3)) {
            (0, 0) => Lamp::AmazingFullCombo,
            (1, 0) => Lamp::GloriousFlag,
            (_, 0) => Lamp::GloriousFullCombo,
            (_, 1) => Lamp::GreatFlag,
            _ => Lamp::FullCombo,
        }
    }
}

/// What a DDI play comes to beside its score and grade
/// ([`crate::player::Results::ddi`]).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DdiResult {
    pub lamp: Lamp,
    /// Early and Late hits per tier, `[early, late]`, by
    /// [`Judgement::index`] (a W0 is neither; a Miss never counts).
    pub sides: [[u32; 2]; 6],
}

/// `table`, DDI's, for a play at music rate `rate` (song seconds a real
/// second, [`crate::clock::ClockOptions::rate`]): its windows, graces,
/// lock and Boo rules are in real seconds, which the judge's song seconds
/// are `rate` times; hold units stay in the chart's seconds
/// ([`hold_units`]). [`crate::player::Player::new`] applies it.
pub fn at_rate(table: &mut JudgeTable, rate: f64) {
    if !rate.is_finite() || rate <= 0.0 || rate == 1.0 {
        return;
    }
    table.scale *= rate;
    table.add *= rate;
    if let Some(just) = &mut table.just {
        just.early *= rate;
        just.late *= rate;
    }
    if let Some(lock) = &mut table.lock {
        *lock *= rate;
    }
    if let EmptyPress::Guarded {
        near_mine,
        quiet,
        lift_ready,
    } = &mut table.empty_press
    {
        *near_mine *= rate;
        *quiet *= rate;
        *lift_ready *= rate;
    }
}

/// DDI's judgment names: the late words as the tiers' names, the early
/// ones apart, W0's word for a W1 inside W0, and a Boo's.
pub fn names() -> JudgeNames {
    JudgeNames {
        w0: Some((W0_WORD.into(), LATE_WORDS[0].into())),
        early: Some(EARLY_WORDS.map(String::from)),
        boo: Some(BOO_WORD.into()),
        palette: super::colors::Palette::Ddi,
        ..JudgeNames::new(LATE_WORDS, HELD_WORD, DROPPED_WORD)
    }
}

/// The DDI ruleset with `options`.
pub fn ruleset(options: &DdiOptions) -> Ruleset {
    let d = options.difficulty;
    let tier = |i: usize| Some(Window::symmetric(d.seconds(TIER_UNITS[i])));
    Ruleset {
        id: ID.into(),
        name: "DDI".into(),
        approximate: false,
        judge: JudgeTable {
            tiers: [tier(1), tier(2), tier(3), tier(4), tier(5)],
            scale: 1.0,
            add: 0.0,
            hold_window: d.seconds(HOLD_GRACE_UNITS),
            roll_window: d.seconds(ROLL_GRACE_UNITS),
            mine_window: MINE_WINDOW,
            w0: Some(d.seconds(TIER_UNITS[0])),
            empty_press: if options.boos {
                EmptyPress::Guarded {
                    near_mine: BOO_NEAR_MINE,
                    quiet: BOO_QUIET,
                    lift_ready: BOO_LIFT_READY,
                }
            } else {
                EmptyPress::Ignore
            },
            // Early and Late beyond W0 (a W0 is neither).
            just: Some(Window::symmetric(d.seconds(TIER_UNITS[0]))),
            lock: Some(LANE_LOCK),
            ..JudgeTable::DANCE
        },
        combo: ComboRules {
            continue_min: Judgement::W3,
            mine_breaks: true,
            let_go_loses_full_combo: true,
            ..ComboRules::DANCE
        },
        score: Box::new(DdiScore::new()),
        gauge: Box::new(DdiLife::new(d)),
        fail: if options.practice {
            FailPolicy::ImmediateContinue
        } else {
            FailPolicy::Immediate
        },
        grades: GradeTable {
            failed: Some(FAILED.into()),
            rule: Some(GradeRule::Ddi),
            ..super::presets::grades(
                GradeBasis::Percent,
                &GRADES.map(|(name, min)| (name, (min.max(0) as f64) / 10_000.0)),
            )
        },
        names: names(),
        danoni: None,
        stepmania: StepManiaOptions::default(),
        ddi: Some(*options),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::judge::test_events::{ev, tap};

    #[test]
    fn units_are_whole_seconds_up() {
        assert_eq!(hold_units(0.0), 1);
        assert_eq!(hold_units(0.3), 1);
        assert_eq!(hold_units(1.0), 1);
        assert_eq!(hold_units(1.0000004), 1);
        assert_eq!(hold_units(1.000001), 2);
        assert_eq!(hold_units(2.5), 3);
        // Checkpoints at 1 s and 2 s after the head and at the tail.
        assert_eq!(units_earned(10.0, 12.5, None), 3);
        assert_eq!(units_earned(10.0, 12.5, Some(10.9)), 0);
        assert_eq!(units_earned(10.0, 12.5, Some(11.0)), 0);
        assert_eq!(units_earned(10.0, 12.5, Some(11.01)), 1);
        assert_eq!(units_earned(10.0, 12.5, Some(12.4)), 2);
    }

    #[test]
    fn views_floor_and_go_negative() {
        let v = DdiScoreView::of(199, 200, 3);
        assert_eq!((v.ratio, v.percent, v.absolute), (995_000, 9950, 19_903));
        // The score is the two together.
        assert_eq!(v.score, 995_000 + 19_903);
        assert_eq!(v.percent_text(), "99.50");
        let v = DdiScoreView::of(1, 3, 0);
        assert_eq!((v.ratio, v.percent, v.score), (333_333, 3333, 333_433));
        let v = DdiScoreView::of(-10, 100, 0);
        assert_eq!((v.ratio, v.percent, v.absolute), (-100_000, -1000, -1000));
        assert_eq!(v.score, -101_000);
        assert_eq!(v.percent_text(), "-10.00");
        // Floored down, below zero too.
        let v = DdiScoreView::of(-1, 3, 0);
        assert_eq!((v.ratio, v.percent, v.score), (-333_334, -3334, -333_434));
        assert_eq!(v.percent_text(), "-33.34");
    }

    /// A perfect 1,000-note chart with every tap a W0: the whole
    /// proportional part, 100 a point and the W0s.
    #[test]
    fn a_perfect_thousand_notes_score_six_million_and_a_thousand() {
        let v = DdiScoreView::of(50 * 1000, 50 * 1000, 1000);
        assert_eq!((v.ratio, v.absolute), (1_000_000, 5_001_000));
        assert_eq!(v.score, 6_001_000);
        assert_eq!(v.percent, 10_000);
        // Every tap a W1 instead: the same but the W0s.
        assert_eq!(DdiScoreView::of(50 * 1000, 50 * 1000, 0).score, 6_000_000);
    }

    /// A view kept before the score was the sum (its `score` the
    /// proportional part, no `ratio`) reads as the sum; one kept now
    /// reads back equal.
    #[test]
    fn a_stored_view_is_worked_out_again() {
        let old = r#"{"points":199,"most":200,"score":995000,"percent":9950,"absolute":19903}"#;
        let v: DdiScoreView = serde_json::from_str(old).unwrap();
        assert_eq!(v, DdiScoreView::of(199, 200, 3));
        assert_eq!(v.score, 1_014_903);
        let kept = serde_json::to_string(&v).unwrap();
        assert_eq!(serde_json::from_str::<DdiScoreView>(&kept).unwrap(), v);
        let v = DdiScoreView::of(-55, 200, 0);
        let kept = serde_json::to_string(&v).unwrap();
        assert_eq!(serde_json::from_str::<DdiScoreView>(&kept).unwrap(), v);
    }

    #[test]
    fn life_steps_by_difficulty() {
        let ctx = ScoreCtx::default();
        for (d, miss) in [
            (Difficulty::Easy, 2.0),
            (Difficulty::Normal, 10.0),
            (Difficulty::Hard, 20.0),
        ] {
            let mut g = DdiLife::new(d);
            g.begin(&ctx);
            assert_eq!(g.points(), 50.0);
            g.on_event(&tap(Judgement::Miss), &ctx);
            assert_eq!(g.points(), 50.0 - miss, "{d:?}");
            g.on_event(&tap(Judgement::W1), &ctx);
            assert_eq!(g.points(), 52.0 - miss, "{d:?}");
            g.on_event(&ev(JudgeEventKind::Boo), &ctx);
            assert_eq!(g.points(), 52.0 - miss - miss / 5.0, "{d:?}");
        }
    }
}
