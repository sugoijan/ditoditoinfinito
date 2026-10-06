//! Rules: judgement windows, combo, scoring, gauges, grades and presets.
//!
//! Everything here is data plus two small strategy traits ([`ScoreRules`]
//! and [`GaugeRules`]) for the parts that carry state and formulas. A
//! [`Ruleset`] bundles one of each; [`presets`] builds the shipped ones.

pub mod gauge;
pub mod presets;
pub mod score;

use serde::{Deserialize, Serialize};

pub use gauge::{Battery, FixedPercent, LifeBar, LifeDeltas};
pub use score::{DancePoints, DdrMoney, DdrVariant, Weights};

use crate::judge::{JudgeEvent, JudgeEventKind};

/// Tap judgement tiers, best first. Better judgements compare *lower*
/// (`W1 < W2 < … < Miss`), so "at least as good as `j`" is `x <= j`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Judgement {
    /// Best tier (Marvelous / Fantastic / Flawless).
    W1,
    W2,
    W3,
    W4,
    /// Worst hit tier (Boo / Way Off); disabled in modern DDR.
    W5,
    /// The note passed without being hit.
    Miss,
}

impl Judgement {
    /// All tiers, best first.
    pub const ALL: [Judgement; 6] = [
        Judgement::W1,
        Judgement::W2,
        Judgement::W3,
        Judgement::W4,
        Judgement::W5,
        Judgement::Miss,
    ];

    /// Position in [`Judgement::ALL`] (0 = `W1`, 5 = `Miss`).
    pub fn index(self) -> usize {
        self as usize
    }

    /// Inverse of [`Judgement::index`].
    pub fn from_index(i: usize) -> Option<Judgement> {
        Judgement::ALL.get(i).copied()
    }

    pub fn is_miss(self) -> bool {
        self == Judgement::Miss
    }
}

/// Timing window of one tier in seconds, as positive magnitudes.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Window {
    /// How early a press may be.
    pub early: f64,
    /// How late a press may be.
    pub late: f64,
}

impl Window {
    /// Same bound on both sides.
    pub const fn symmetric(seconds: f64) -> Window {
        Window {
            early: seconds,
            late: seconds,
        }
    }
}

/// What a press that hits no note does.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum EmptyPress {
    /// DDR / StepMania: nothing happens.
    Ignore,
    /// FFR: every stray press is a `Boo` event.
    Boo,
    /// Dancing☆Onigiri "Excessive": a press that is early by `lo..=hi`
    /// seconds of a pending note in the same lane is a `Boo`. `factor` is
    /// the intended life damage as a fraction of a miss; gauges decide
    /// what to do with a `Boo`, so it is informational here.
    ExcessiveEarly { lo: f64, hi: f64, factor: f64 },
}

/// Timing windows and how presses map to tiers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JudgeTable {
    /// Raw windows for `W1..=W5`; `None` disables a tier (its deltas fall
    /// into the next enabled, wider tier).
    pub tiers: [Option<Window>; 5],
    /// StepMania `TimingWindowScale`: every bound is `scale * raw + add`.
    pub scale: f64,
    /// StepMania `TimingWindowAdd`, seconds.
    pub add: f64,
    /// Seconds a hold may be released before it is dropped.
    pub hold_window: f64,
    /// Seconds between roll taps before the roll is dropped.
    pub roll_window: f64,
    /// A press within this many seconds of a mine hits it.
    pub mine_window: f64,
    /// FA+ style sub-window inside `W1` (raw, scaled like the tiers), for
    /// display and EX score only. `None` = not used.
    pub w0: Option<f64>,
    pub empty_press: EmptyPress,
    /// DDR "JUDGMENT TIMING", seconds. Positive means the game expects
    /// presses *later*: a press at a given instant is judged as that much
    /// **earlier** (its delta shrinks), the same direction as
    /// [`ClockOptions::audio_offset`](crate::clock::ClockOptions::audio_offset).
    /// Raise it when you keep getting "Fast".
    pub judge_offset: f64,
}

impl JudgeTable {
    /// `scale * raw + add`.
    pub fn scaled(&self, raw: f64) -> f64 {
        self.scale * raw + self.add
    }

    /// Hold window after `scale`/`add` (StepMania applies them to
    /// `TW_Hold`/`TW_Roll`/`TW_Mine` as well).
    pub fn hold_window_seconds(&self) -> f64 {
        self.scaled(self.hold_window)
    }

    pub fn roll_window_seconds(&self) -> f64 {
        self.scaled(self.roll_window)
    }

    pub fn mine_window_seconds(&self) -> f64 {
        self.scaled(self.mine_window)
    }

    /// Effective window of a tier, or `None` for `Miss` and disabled tiers.
    pub fn window(&self, tier: Judgement) -> Option<Window> {
        let raw = self.tiers.get(tier.index()).copied().flatten()?;
        Some(Window {
            early: self.scaled(raw.early),
            late: self.scaled(raw.late),
        })
    }

    /// Tier for a press `delta_seconds` = press − note (positive = late),
    /// or `None` when the press is outside every window.
    pub fn judge(&self, delta_seconds: f64) -> Option<Judgement> {
        for tier in &Judgement::ALL[..5] {
            if let Some(w) = self.window(*tier) {
                let bound = if delta_seconds < 0.0 { w.early } else { w.late };
                if delta_seconds.abs() <= bound {
                    return Some(*tier);
                }
            }
        }
        None
    }

    /// Whether a delta falls inside the FA+ `w0` sub-window.
    pub fn is_w0(&self, delta_seconds: f64) -> bool {
        self.w0
            .is_some_and(|w| delta_seconds.abs() <= self.scaled(w))
    }

    /// Widest late bound: a note older than this is a `Miss`.
    pub fn miss_after(&self) -> f64 {
        Judgement::ALL[..5]
            .iter()
            .filter_map(|t| self.window(*t))
            .map(|w| w.late)
            .fold(0.0, f64::max)
    }

    /// Widest early bound: a note further ahead than this cannot be hit.
    pub fn max_early(&self) -> f64 {
        Judgement::ALL[..5]
            .iter()
            .filter_map(|t| self.window(*t))
            .map(|w| w.early)
            .fold(0.0, f64::max)
    }
}

/// How combo grows and breaks.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComboRules {
    /// Lowest tier that still continues the combo (SM dance: `W3`; DDR 2013+: `W4`).
    pub continue_min: Judgement,
    /// Group simultaneous notes into one row judgement (DDR) instead of one
    /// per note (StepMania). The row gets the worst of its notes.
    pub per_row: bool,
    /// Whether a completed hold (`Held`) adds one to the combo.
    pub held_increments: bool,
    /// Whether a dropped hold (`LetGo`) breaks the combo.
    pub let_go_breaks: bool,
    /// Whether hitting a mine breaks the combo.
    pub mine_breaks: bool,
}

/// What happens when the gauge says "failed".
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FailPolicy {
    /// Arcade: the song ends at once.
    Immediate,
    /// The play is marked failed but continues to the end.
    ImmediateContinue,
    /// Only checked at the end of the song against `min_life`.
    EndOfSong { min_life: f32 },
    /// Never fail.
    Off,
}

/// Full-combo lamp.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum FullCombo {
    None,
    /// Combo never broke (worst judgement still continued combo).
    FC,
    /// Nothing worse than `W3`.
    GreatFC,
    /// Nothing worse than `W2`.
    PerfectFC,
    /// Only `W1`.
    MarvelousFC,
}

/// Counts of every judged event in a play.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tally {
    /// Per tier, indexed by [`Judgement::index`].
    pub taps: [u32; 6],
    pub held: u32,
    pub let_go: u32,
    pub mine_hit: u32,
    pub boo: u32,
}

impl Tally {
    pub fn record(&mut self, kind: &JudgeEventKind) {
        match kind {
            JudgeEventKind::Tap(j, _) => self.taps[j.index()] += 1,
            JudgeEventKind::Held => self.held += 1,
            JudgeEventKind::LetGo => self.let_go += 1,
            JudgeEventKind::HitMine => self.mine_hit += 1,
            JudgeEventKind::Boo => self.boo += 1,
        }
    }

    pub fn count(&self, j: Judgement) -> u32 {
        self.taps[j.index()]
    }

    pub fn total_taps(&self) -> u32 {
        self.taps.iter().sum()
    }

    /// Worst tap judgement recorded, `None` if nothing was judged yet.
    pub fn worst_tap(&self) -> Option<Judgement> {
        Judgement::ALL
            .iter()
            .rev()
            .find(|j| self.taps[j.index()] > 0)
            .copied()
    }

    /// Whether every tap so far is at least as good as `j`.
    pub fn all_within(&self, j: Judgement) -> bool {
        self.taps[j.index() + 1..].iter().all(|&c| c == 0)
    }
}

impl FullCombo {
    /// Lamp for a finished play.
    pub fn evaluate(tally: &Tally, combo: &ComboRules) -> FullCombo {
        if tally.count(Judgement::Miss) > 0
            || (combo.let_go_breaks && tally.let_go > 0)
            || (combo.mine_breaks && tally.mine_hit > 0)
        {
            return FullCombo::None;
        }
        let Some(worst) = tally.worst_tap() else {
            return FullCombo::None;
        };
        if worst > combo.continue_min {
            return FullCombo::None;
        }
        match worst {
            Judgement::W1 => FullCombo::MarvelousFC,
            Judgement::W2 => FullCombo::PerfectFC,
            Judgement::W3 => FullCombo::GreatFC,
            _ => FullCombo::FC,
        }
    }
}

/// Which number of a [`ScoreView`] grades are read from.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GradeBasis {
    /// `ScoreView::money`.
    Money,
    /// `ScoreView::grade_percent`, falling back to `ScoreView::percent`.
    Percent,
}

/// Grade thresholds, best first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GradeTable {
    pub basis: GradeBasis,
    /// `(name, minimum)`, sorted from best to worst; the last entry should
    /// have minimum `0.0` so every play gets a grade.
    pub tiers: Vec<(String, f64)>,
    /// Grade for a play where every tap is `W1` and no hold was dropped or
    /// mine hit, checked before `all_w2` and the thresholds.
    pub all_w1: Option<String>,
    /// Grade for a play where every tap is `W1`/`W2` and no hold was
    /// dropped or mine hit (StepMania `GradeTier02IsAllW2s`).
    pub all_w2: Option<String>,
    /// Grade for a failed play (DDR "E"), checked first.
    pub failed: Option<String>,
}

impl GradeTable {
    /// Grade string for a play.
    pub fn grade(&self, view: &ScoreView, tally: &Tally, failed: bool) -> String {
        if failed && let Some(f) = &self.failed {
            return f.clone();
        }
        let clean = tally.let_go == 0 && tally.mine_hit == 0 && tally.total_taps() > 0;
        if clean
            && let Some(g) = &self.all_w1
            && tally.all_within(Judgement::W1)
        {
            return g.clone();
        }
        if clean
            && let Some(g) = &self.all_w2
            && tally.all_within(Judgement::W2)
        {
            return g.clone();
        }
        let value = match self.basis {
            GradeBasis::Money => view.money.unwrap_or(0) as f64,
            GradeBasis::Percent => view.grade_percent.or(view.percent).unwrap_or(0.0),
        };
        self.tiers
            .iter()
            .find(|(_, min)| value >= *min)
            .or(self.tiers.last())
            .map(|(name, _)| name.clone())
            .unwrap_or_default()
    }
}

/// Display names for the six tiers plus the hold outcomes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JudgeNames {
    /// Indexed by [`Judgement::index`].
    pub tiers: [String; 6],
    /// Completed hold ("O.K." / "Held").
    pub held: String,
    /// Dropped hold ("N.G." / "Let Go").
    pub let_go: String,
}

impl JudgeNames {
    pub fn new(tiers: [&str; 6], held: &str, let_go: &str) -> JudgeNames {
        JudgeNames {
            tiers: tiers.map(String::from),
            held: held.to_string(),
            let_go: let_go.to_string(),
        }
    }

    pub fn name(&self, j: Judgement) -> &str {
        &self.tiers[j.index()]
    }
}

/// Totals a score or gauge needs, fixed for the play except `combo`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoreCtx {
    /// Judged steps: one per judged tap-like note, or one per row when the
    /// ruleset's [`ComboRules::per_row`] is set (a DDR jump = 1 step).
    pub steps: u32,
    /// Judged holds and rolls.
    pub holds: u32,
    /// Judged mines and shocks.
    pub mines: u32,
    /// Combo after the current event.
    pub combo: u32,
}

/// Score as the HUD shows it. Fields a scoring system does not produce are `None`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ScoreView {
    /// DDR-style money score (max 1,000,000).
    pub money: Option<u64>,
    /// Dance-point percentage in `0..=1`.
    pub percent: Option<f64>,
    /// Percentage computed with the ruleset's grade weights when they differ
    /// from the displayed ones (StepMania `GradeWeight*`).
    pub grade_percent: Option<f64>,
    /// EX score.
    pub ex: Option<u32>,
    /// Maximum EX score for the chart.
    pub max_ex: Option<u32>,
}

/// A scoring system: consumes judge events and produces a [`ScoreView`].
pub trait ScoreRules {
    /// Called once before the first event with the chart totals.
    fn begin(&mut self, ctx: &ScoreCtx);
    fn on_event(&mut self, ev: &JudgeEvent, ctx: &ScoreCtx);
    fn view(&self) -> ScoreView;
    /// Called once when the song ends.
    fn finish(&mut self);
    /// A new instance with the same parameters and no state.
    fn fresh(&self) -> Box<dyn ScoreRules>;
}

/// A life gauge: consumes judge events and reports life, danger and failure.
pub trait GaugeRules {
    /// Called once before the first event with the chart totals.
    fn begin(&mut self, ctx: &ScoreCtx);
    fn on_event(&mut self, ev: &JudgeEvent, ctx: &ScoreCtx);
    /// Life in `0..=1`.
    fn life(&self) -> f32;
    fn failed(&self) -> bool;
    fn danger(&self) -> bool;
    /// A new instance with the same parameters and no state.
    fn fresh(&self) -> Box<dyn GaugeRules>;
}

/// Everything that decides how a play is judged and scored.
pub struct Ruleset {
    /// Stable identifier (`itg`, `sm5`, `ddr-a`).
    pub id: String,
    pub name: String,
    /// The preset reproduces a game whose exact numbers are not public.
    pub approximate: bool,
    pub judge: JudgeTable,
    pub combo: ComboRules,
    pub score: Box<dyn ScoreRules>,
    pub gauge: Box<dyn GaugeRules>,
    pub fail: FailPolicy,
    pub grades: GradeTable,
    pub names: JudgeNames,
}

impl Ruleset {
    /// A copy with fresh score and gauge state, for a new play.
    pub fn fresh(&self) -> Ruleset {
        Ruleset {
            id: self.id.clone(),
            name: self.name.clone(),
            approximate: self.approximate,
            judge: self.judge.clone(),
            combo: self.combo,
            score: self.score.fresh(),
            gauge: self.gauge.fresh(),
            fail: self.fail,
            grades: self.grades.clone(),
            names: self.names.clone(),
        }
    }
}

impl core::fmt::Debug for Ruleset {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Ruleset")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("approximate", &self.approximate)
            .field("judge", &self.judge)
            .field("combo", &self.combo)
            .field("fail", &self.fail)
            .field("grades", &self.grades)
            .field("names", &self.names)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(tiers: [f64; 5]) -> JudgeTable {
        JudgeTable {
            tiers: tiers.map(|s| Some(Window::symmetric(s))),
            scale: 1.0,
            add: 0.0,
            hold_window: 0.25,
            roll_window: 0.5,
            mine_window: 0.09,
            w0: None,
            empty_press: EmptyPress::Ignore,
            judge_offset: 0.0,
        }
    }

    #[test]
    fn judge_picks_tiers_and_add() {
        let mut t = table([0.0215, 0.043, 0.102, 0.135, 0.180]);
        t.add = 0.0015;
        assert_eq!(t.judge(0.0), Some(Judgement::W1));
        assert_eq!(t.judge(0.022), Some(Judgement::W1));
        assert_eq!(t.judge(-0.024), Some(Judgement::W2));
        assert_eq!(t.judge(0.1), Some(Judgement::W3));
        assert_eq!(t.judge(0.1364), Some(Judgement::W4));
        assert_eq!(t.judge(0.181), Some(Judgement::W5));
        assert_eq!(t.judge(0.182), None);
        assert!((t.miss_after() - 0.1815).abs() < 1e-12);
    }

    #[test]
    fn disabled_tier_folds_into_wider_one() {
        let mut t = table([0.016667, 0.033333, 0.091667, 0.141667, 0.225]);
        t.tiers[4] = None;
        assert_eq!(t.judge(0.15), None);
        assert_eq!(t.judge(0.14), Some(Judgement::W4));
        assert!((t.miss_after() - 0.141667).abs() < 1e-12);
    }

    #[test]
    fn asymmetric_windows() {
        let mut t = table([0.02, 0.04, 0.08, 0.12, 0.18]);
        t.tiers[0] = Some(Window {
            early: 0.010,
            late: 0.030,
        });
        assert_eq!(t.judge(-0.015), Some(Judgement::W2));
        assert_eq!(t.judge(0.025), Some(Judgement::W1));
    }

    #[test]
    fn full_combo_lamps() {
        let combo = ComboRules {
            continue_min: Judgement::W3,
            per_row: false,
            held_increments: false,
            let_go_breaks: false,
            mine_breaks: false,
        };
        let mut tally = Tally::default();
        tally.taps[0] = 10;
        assert_eq!(FullCombo::evaluate(&tally, &combo), FullCombo::MarvelousFC);
        tally.taps[2] = 1;
        assert_eq!(FullCombo::evaluate(&tally, &combo), FullCombo::GreatFC);
        tally.taps[3] = 1;
        assert_eq!(FullCombo::evaluate(&tally, &combo), FullCombo::None);
        let ddr = ComboRules {
            continue_min: Judgement::W4,
            let_go_breaks: true,
            ..combo
        };
        assert_eq!(FullCombo::evaluate(&tally, &ddr), FullCombo::FC);
        tally.let_go = 1;
        assert_eq!(FullCombo::evaluate(&tally, &ddr), FullCombo::None);
    }

    #[test]
    fn grade_predicates_and_thresholds() {
        let g = GradeTable {
            basis: GradeBasis::Percent,
            tiers: vec![("AA".into(), 0.93), ("A".into(), 0.80), ("D".into(), 0.0)],
            all_w1: Some("AAAA".into()),
            all_w2: Some("AAA".into()),
            failed: Some("F".into()),
        };
        let mut tally = Tally::default();
        tally.taps[0] = 5;
        let view = ScoreView {
            percent: Some(0.5),
            ..Default::default()
        };
        assert_eq!(g.grade(&view, &tally, false), "AAAA");
        tally.taps[1] = 1;
        assert_eq!(g.grade(&view, &tally, false), "AAA");
        tally.taps[2] = 1;
        assert_eq!(g.grade(&view, &tally, false), "D");
        let view = ScoreView {
            percent: Some(0.95),
            grade_percent: Some(0.85),
            ..Default::default()
        };
        assert_eq!(g.grade(&view, &tally, false), "A");
        assert_eq!(g.grade(&view, &tally, true), "F");
    }
}
