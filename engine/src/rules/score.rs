//! Scoring systems: StepMania dance points and the DDR money score.

use serde::{Deserialize, Serialize};

use super::{ScoreCtx, ScoreRules, ScoreView};
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

    /// Weight of an event (`Boo` is worth nothing).
    pub fn of(&self, kind: &JudgeEventKind) -> i32 {
        match kind {
            JudgeEventKind::Tap(j, _) => self.tiers[j.index()],
            JudgeEventKind::Held => self.held,
            JudgeEventKind::LetGo => self.let_go,
            JudgeEventKind::HitMine => self.mine,
            JudgeEventKind::Boo => 0,
        }
    }

    /// Maximum total for a chart: `steps × W1 + holds × Held`.
    pub fn possible(&self, ctx: &ScoreCtx) -> i64 {
        i64::from(ctx.steps) * i64::from(self.tiers[0])
            + i64::from(ctx.holds) * i64::from(self.held)
    }
}

/// StepMania / ITG dance points: `percent = points / possible`.
///
/// `weights` is the displayed percentage (`PercentScoreWeight*`),
/// `grade_weights` the optional second set grades are computed from
/// (`GradeWeight*`), `ex_weights` the optional EX score (3/2/1 style).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DancePoints {
    pub weights: Weights,
    pub grade_weights: Option<Weights>,
    pub ex_weights: Option<Weights>,
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
            points: 0,
            grade_points: 0,
            ex: 0,
            ctx: ScoreCtx::default(),
        }
    }

    fn percent(points: i64, possible: i64) -> f64 {
        if possible <= 0 {
            return 0.0;
        }
        (points as f64 / possible as f64).max(0.0)
    }
}

impl ScoreRules for DancePoints {
    fn begin(&mut self, ctx: &ScoreCtx) {
        self.ctx = *ctx;
    }

    fn on_event(&mut self, ev: &JudgeEvent, ctx: &ScoreCtx) {
        self.ctx = *ctx;
        self.points += i64::from(self.weights.of(&ev.kind));
        if let Some(g) = &self.grade_weights {
            self.grade_points += i64::from(g.of(&ev.kind));
        }
        if let Some(x) = &self.ex_weights {
            self.ex += i64::from(x.of(&ev.kind));
        }
    }

    fn view(&self) -> ScoreView {
        ScoreView {
            money: None,
            percent: Some(Self::percent(self.points, self.weights.possible(&self.ctx))),
            grade_percent: self
                .grade_weights
                .map(|g| Self::percent(self.grade_points, g.possible(&self.ctx))),
            ex: self.ex_weights.map(|_| self.ex.max(0) as u32),
            max_ex: self.ex_weights.map(|x| x.possible(&self.ctx).max(0) as u32),
        }
    }

    fn finish(&mut self) {}

    fn fresh(&self) -> Box<dyn ScoreRules> {
        Box::new(DancePoints::new(
            self.weights,
            self.grade_weights,
            self.ex_weights,
        ))
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

    fn count(&self) -> i64 {
        i64::from(self.ctx.steps) + i64::from(self.ctx.holds)
    }
}

impl ScoreRules for DdrMoney {
    fn begin(&mut self, ctx: &ScoreCtx) {
        self.ctx = *ctx;
    }

    fn on_event(&mut self, ev: &JudgeEvent, ctx: &ScoreCtx) {
        self.ctx = *ctx;
        let (w, penalised) = self.weights();
        self.sum += i64::from(w.of(&ev.kind));
        if let JudgeEventKind::Tap(j, _) = ev.kind
            && penalised[j.index()]
        {
            self.penalties += 1;
        }
        self.ex += i64::from(Self::EX.of(&ev.kind));
    }

    fn view(&self) -> ScoreView {
        let n = self.count();
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
        }
    }

    fn finish(&mut self) {}

    fn fresh(&self) -> Box<dyn ScoreRules> {
        Box::new(DdrMoney::new(self.variant))
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
