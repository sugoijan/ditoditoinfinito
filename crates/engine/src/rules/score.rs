//! Scoring systems: StepMania dance points and the DDR money score.

use serde::{Deserialize, Serialize};

use super::{Judgement, ScoreCtx, ScoreRules, ScoreView};
use crate::judge::{JudgeEvent, JudgeEventKind};

/// Points per event kind.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Weights {
    /// Per tier, indexed by [`Judgement::index`](super::Judgement::index).
    pub tiers: [i32; 6],
    pub held: i32,
    pub let_go: i32,
    pub mine: i32,
}

impl Weights {
    pub const fn new(tiers: [i32; 6], held: i32, let_go: i32, mine: i32) -> Weights {
        Weights {
            tiers,
            held,
            let_go,
            mine,
        }
    }

    /// Weight of an event (`Boo` is worth nothing; a checkpoint and a
    /// ticked hold's outcome as [`JudgeEventKind::scored_as`] says).
    pub fn of(&self, kind: &JudgeEventKind, ctx: &ScoreCtx) -> i32 {
        match kind.scored_as(ctx.ticked) {
            Some(JudgeEventKind::Tap(j, _)) => self.tiers[j.index()],
            Some(JudgeEventKind::Held) => self.held,
            Some(JudgeEventKind::LetGo) => self.let_go,
            Some(JudgeEventKind::HitMine) => self.mine,
            _ => 0,
        }
    }

    /// Maximum total for a chart: `steps × W1 + holds × Held`, or with
    /// ticked holds `(steps + checkpoints) × W1`; shocks weigh as holds.
    pub fn possible(&self, ctx: &ScoreCtx) -> i64 {
        let w1 = i64::from(self.tiers[0]);
        let shocks = i64::from(ctx.shocks) * i64::from(self.held);
        if ctx.ticked {
            (i64::from(ctx.steps) + i64::from(ctx.checkpoints)) * w1 + shocks
        } else {
            i64::from(ctx.steps) * w1 + i64::from(ctx.holds) * i64::from(self.held) + shocks
        }
    }
}

/// `points` of `possible` as a fraction, never below 0 (0 of nothing).
fn percent(points: i64, possible: i64) -> f64 {
    if possible <= 0 {
        return 0.0;
    }
    (points as f64 / possible as f64).max(0.0)
}

/// StepMania / ITG dance points: `percent = points / possible`.
///
/// `weights` is the displayed percentage (`PercentScoreWeight*`),
/// `grade_weights` the optional second set grades are computed from
/// (`GradeWeight*`), `ex_weights` the optional EX score (3/2/1 style).
/// With `ex_w0`, a hit inside the ruleset's `W0` window
/// ([`ScoreCtx::w0`]) weighs that much in the EX score instead, and the
/// EX score is Simply Love's percentage ([`ScoreView::ex_percent`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DancePoints {
    pub weights: Weights,
    pub grade_weights: Option<Weights>,
    pub ex_weights: Option<Weights>,
    #[serde(default)]
    pub ex_w0: Option<i32>,
    #[serde(skip)]
    points: i64,
    #[serde(skip)]
    grade_points: i64,
    #[serde(skip)]
    ex: i64,
    #[serde(skip)]
    ctx: ScoreCtx,
}

impl DancePoints {
    pub fn new(
        weights: Weights,
        grade_weights: Option<Weights>,
        ex_weights: Option<Weights>,
    ) -> DancePoints {
        DancePoints {
            weights,
            grade_weights,
            ex_weights,
            ex_w0: None,
            points: 0,
            grade_points: 0,
            ex: 0,
            ctx: ScoreCtx::default(),
        }
    }

    /// With a `W0` weight: `ex_weights` as an EX percentage whose `W0`
    /// hits weigh `w0` (Simply Love's EX score in halves: 7, 6, 4, 2,
    /// held 2, mine −2).
    pub fn with_ex_w0(mut self, w0: i32) -> DancePoints {
        self.ex_w0 = Some(w0);
        self
    }

    /// The most EX points the chart has: every step a `W0` where one is
    /// weighed apart.
    fn possible_ex(&self, x: &Weights) -> i64 {
        let w0_bonus = self.ex_w0.map_or(0, |w| i64::from(w - x.tiers[0]));
        x.possible(&self.ctx) + i64::from(self.ctx.steps) * w0_bonus
    }
}

impl ScoreRules for DancePoints {
    fn begin(&mut self, ctx: &ScoreCtx) {
        self.ctx = *ctx;
    }

    fn on_event(&mut self, ev: &JudgeEvent, ctx: &ScoreCtx) {
        self.ctx = *ctx;
        self.points += i64::from(self.weights.of(&ev.kind, ctx));
        if let Some(g) = &self.grade_weights {
            self.grade_points += i64::from(g.of(&ev.kind, ctx));
        }
        if let Some(x) = &self.ex_weights {
            self.ex += i64::from(match self.ex_w0 {
                Some(w0) if ctx.w0 => w0,
                _ => x.of(&ev.kind, ctx),
            });
        }
    }

    fn view(&self) -> ScoreView {
        ScoreView {
            money: None,
            percent: Some(percent(self.points, self.weights.possible(&self.ctx))),
            grade_percent: self
                .grade_weights
                .map(|g| percent(self.grade_points, g.possible(&self.ctx))),
            ex: self
                .ex_weights
                .filter(|_| self.ex_w0.is_none())
                .map(|_| self.ex.max(0) as u32),
            max_ex: self
                .ex_weights
                .filter(|_| self.ex_w0.is_none())
                .map(|x| x.possible(&self.ctx).max(0) as u32),
            // `floor(points / possible × 10000) / 100` percent, never
            // below 0 (`CalculateExScore`).
            ex_percent: self.ex_weights.filter(|_| self.ex_w0.is_some()).map(|x| {
                match self.possible_ex(&x) {
                    p if p <= 0 => 0.0,
                    p => (self.ex.max(0) as f64 * 10_000.0 / p as f64).floor() / 10_000.0,
                }
            }),
        }
    }

    fn finish(&mut self) {}

    fn fresh(&self) -> Box<dyn ScoreRules> {
        Box::new(DancePoints {
            ex_w0: self.ex_w0,
            ..DancePoints::new(self.weights, self.grade_weights, self.ex_weights)
        })
    }
}

/// Which DDR money-score formula.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DdrVariant {
    /// SuperNOVA2 → DDR (2014): Great = SC/2 − 10, Good = 0.
    Sn2,
    /// DDR A → WORLD: Great = SC·3/5 − 10, Good = SC/5 − 10.
    A,
}

/// DDR money score, max 1,000,000.
///
/// `SC = 1,000,000 / (steps + holds)`; Marvelous and O.K. earn `SC`,
/// Perfect `SC − 10`, Great and Good a fraction of `SC` minus 10 (see
/// [`DdrVariant`]). Computed as `1,000,000 × weighted%` in integer
/// arithmetic, minus 10 per penalised judgement, floored to 10. EX score
/// 3/2/1, O.K. 3.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DdrMoney {
    pub variant: DdrVariant,
    #[serde(skip)]
    sum: i64,
    #[serde(skip)]
    penalties: i64,
    #[serde(skip)]
    ex: i64,
    #[serde(skip)]
    ctx: ScoreCtx,
}

impl DdrMoney {
    pub fn new(variant: DdrVariant) -> DdrMoney {
        DdrMoney {
            variant,
            sum: 0,
            penalties: 0,
            ex: 0,
            ctx: ScoreCtx::default(),
        }
    }

    /// Relative weights (Marvelous = max) and whether the tier takes the −10.
    fn weights(&self) -> (Weights, [bool; 6]) {
        match self.variant {
            DdrVariant::Sn2 => (
                Weights::new([2, 2, 1, 0, 0, 0], 2, 0, 0),
                [false, true, true, false, false, false],
            ),
            DdrVariant::A => (
                Weights::new([5, 5, 3, 1, 0, 0], 5, 0, 0),
                [false, true, true, true, false, false],
            ),
        }
    }

    const EX: Weights = Weights::new([3, 2, 1, 0, 0, 0], 3, 0, 0);
}

impl ScoreRules for DdrMoney {
    fn begin(&mut self, ctx: &ScoreCtx) {
        self.ctx = *ctx;
    }

    fn on_event(&mut self, ev: &JudgeEvent, ctx: &ScoreCtx) {
        self.ctx = *ctx;
        let (w, penalised) = self.weights();
        self.sum += i64::from(w.of(&ev.kind, ctx));
        if let JudgeEventKind::Tap(j, _) = ev.kind
            && penalised[j.index()]
        {
            self.penalties += 1;
        }
        self.ex += i64::from(Self::EX.of(&ev.kind, ctx));
    }

    fn view(&self) -> ScoreView {
        // `SC = 1,000,000 / (steps + freezes + shock arrows)`.
        let n = self.ctx.units();
        let (w, _) = self.weights();
        let money = if n == 0 {
            0
        } else {
            let ideal = 1_000_000 * self.sum / (i64::from(w.tiers[0]) * n);
            ((ideal - 10 * self.penalties).max(0) / 10 * 10) as u64
        };
        ScoreView {
            money: Some(money),
            percent: None,
            grade_percent: None,
            ex: Some(self.ex.max(0) as u32),
            max_ex: Some((Self::EX.possible(&self.ctx)).max(0) as u32),
            ex_percent: None,
        }
    }

    fn finish(&mut self) {}

    fn fresh(&self) -> Box<dyn ScoreRules> {
        Box::new(DdrMoney::new(self.variant))
    }
}

/// DDRMAX2 and DDR EXTREME's score in normal play (RemyWiki's
/// "DanceDanceRevolution Scoring System", after aaroninjapan.com), with
/// their dance points for the grade.
///
/// The song's base `B` is 1,000,000 a foot of its rating (StepMania 3.9's
/// `ScoreKeeperMAX2` clamps the meter to 1–10). Of `N` steps and freezes
/// (a jump one step; [`ScoreCtx::steps`] under per-row rules), the `n`th
/// scores `p × floor(B / S) × n` with `S = N(N + 1)/2` and `p` 10 for a
/// Perfect (or better) and an O.K., 5 for a Great, 0 otherwise; a freeze
/// scores its O.K. as the step after the last one scored (an N.G. takes
/// its place for nothing). The last of them, a Perfect or an O.K., adds
/// what `floor` left out, `10 × (B − floor(B / S) × S)`, so all Perfect is
/// `10 × B` (RemyWiki writes `× N` there, which cannot give it).
///
/// The dance points are MAX's: Perfect 2, Great 1, Good 0, Boo −4, Miss
/// −8, O.K. 6, N.G. 0, a mine stepped on −8 (StepMania's
/// `GradeWeightHitMine`), out of 2 a step and 6 a freeze; the percentage
/// is [`ScoreView::percent`], the grades read it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Max2 {
    #[serde(skip)]
    score: i64,
    /// Steps and freezes scored so far.
    #[serde(skip)]
    n: i64,
    #[serde(skip)]
    points: i64,
    #[serde(skip)]
    ctx: ScoreCtx,
}

impl Default for Max2 {
    fn default() -> Max2 {
        Max2::new()
    }
}

impl Max2 {
    const DANCE_POINTS: Weights = Weights::new([2, 2, 1, 0, -4, -8], 6, 0, -8);

    pub fn new() -> Max2 {
        Max2 {
            score: 0,
            n: 0,
            points: 0,
            ctx: ScoreCtx::default(),
        }
    }

    /// `B`: a million a foot, the rating clamped to 1–10.
    fn base(&self) -> i64 {
        1_000_000 * i64::from(self.ctx.meter.clamp(1, 10))
    }

    /// `p` of a scored unit, `None` for what is not a step or a freeze.
    fn multiplier(kind: &JudgeEventKind, ticked: bool) -> Option<i64> {
        Some(match kind.scored_as(ticked)? {
            JudgeEventKind::Tap(j, _) => match j {
                Judgement::W1 | Judgement::W2 => 10,
                Judgement::W3 => 5,
                _ => 0,
            },
            JudgeEventKind::Held => 10,
            JudgeEventKind::LetGo => 0,
            _ => return None,
        })
    }
}

impl ScoreRules for Max2 {
    fn begin(&mut self, ctx: &ScoreCtx) {
        self.ctx = *ctx;
    }

    fn on_event(&mut self, ev: &JudgeEvent, ctx: &ScoreCtx) {
        self.ctx = *ctx;
        self.points += i64::from(Self::DANCE_POINTS.of(&ev.kind, ctx));
        let Some(p) = Max2::multiplier(&ev.kind, ctx.ticked) else {
            return;
        };
        // `N`: steps and freezes, shocks counted as freezes as later DDR
        // counts them.
        let units = self.ctx.units();
        let s = units * (units + 1) / 2;
        if s == 0 {
            return;
        }
        let per = self.base() / s;
        self.n += 1;
        self.score += p * per * self.n;
        if self.n == units && p == 10 {
            self.score += 10 * (self.base() - per * s);
        }
    }

    fn view(&self) -> ScoreView {
        ScoreView {
            money: Some(self.score.max(0) as u64),
            percent: Some(percent(self.points, Self::DANCE_POINTS.possible(&self.ctx))),
            ..ScoreView::default()
        }
    }

    fn finish(&mut self) {}

    fn fresh(&self) -> Box<dyn ScoreRules> {
        Box::new(Max2::new())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::Judgement;
    use ddi_chart::Tick;

    fn ev(kind: JudgeEventKind) -> JudgeEvent {
        JudgeEvent {
            note_index: None,
            lane: 0,
            tick: Tick::ZERO,
            kind,
            song_time: 0.0,
        }
    }

    fn tap(j: Judgement) -> JudgeEvent {
        ev(JudgeEventKind::Tap(j, 0.0))
    }

    #[test]
    fn ddr_a_matches_remywiki_formula() {
        // 20 steps: SC = 50,000. 17 Marvelous, 1 Perfect, 1 Great, 1 Good.
        // 17×50000 + 49990 + (30000−10) + (10000−10) = 850,000 + 49,990 + 29,990 + 9,990 = 939,970.
        let ctx = ScoreCtx {
            steps: 20,
            holds: 0,
            mines: 0,
            combo: 0,
            ..ScoreCtx::default()
        };
        let mut s = DdrMoney::new(DdrVariant::A);
        s.begin(&ctx);
        for _ in 0..17 {
            s.on_event(&tap(Judgement::W1), &ctx);
        }
        s.on_event(&tap(Judgement::W2), &ctx);
        s.on_event(&tap(Judgement::W3), &ctx);
        s.on_event(&tap(Judgement::W4), &ctx);
        let v = s.view();
        assert_eq!(v.money, Some(939_970));
        // EX: 17×3 + 2 + 1 = 54 of 60.
        assert_eq!(v.ex, Some(54));
        assert_eq!(v.max_ex, Some(60));
    }

    #[test]
    fn ddr_all_marvelous_is_a_million() {
        let ctx = ScoreCtx {
            steps: 7,
            holds: 3,
            mines: 0,
            combo: 0,
            ..ScoreCtx::default()
        };
        let mut s = DdrMoney::new(DdrVariant::Sn2);
        s.begin(&ctx);
        for _ in 0..7 {
            s.on_event(&tap(Judgement::W1), &ctx);
        }
        for _ in 0..3 {
            s.on_event(&ev(JudgeEventKind::Held), &ctx);
        }
        assert_eq!(s.view().money, Some(1_000_000));
        // SN2: Good is worth 0 and takes no penalty.
        let mut s = DdrMoney::new(DdrVariant::Sn2);
        s.begin(&ctx);
        s.on_event(&tap(Judgement::W4), &ctx);
        assert_eq!(s.view().money, Some(0));
    }

    #[test]
    fn dance_points_percent_and_ex() {
        let w = Weights::new([5, 4, 2, 0, -6, -12], 5, 0, -6);
        let ex = Weights::new([3, 2, 1, 0, 0, 0], 1, 0, -1);
        let ctx = ScoreCtx {
            steps: 4,
            holds: 1,
            mines: 1,
            combo: 0,
            ..ScoreCtx::default()
        };
        let mut s = DancePoints::new(w, None, Some(ex));
        s.begin(&ctx);
        s.on_event(&tap(Judgement::W1), &ctx);
        s.on_event(&tap(Judgement::W2), &ctx);
        s.on_event(&tap(Judgement::W5), &ctx);
        s.on_event(&tap(Judgement::Miss), &ctx);
        s.on_event(&ev(JudgeEventKind::Held), &ctx);
        s.on_event(&ev(JudgeEventKind::HitMine), &ctx);
        let v = s.view();
        // (5 + 4 − 6 − 12 + 5 − 6) / (4×5 + 1×5) = −10/25 → clamped to 0.
        assert_eq!(v.percent, Some(0.0));
        // EX: 3 + 2 + 0 + 0 + 1 − 1 = 5 of 4×3 + 1 = 13.
        assert_eq!(v.ex, Some(5));
        assert_eq!(v.max_ex, Some(13));
    }
}
