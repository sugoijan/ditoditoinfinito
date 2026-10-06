//! Life gauges: StepMania bar, battery and the DDR fixed-percent bar.

use serde::{Deserialize, Serialize};

use super::{GaugeRules, Judgement, ScoreCtx};
use crate::judge::{JudgeEvent, JudgeEventKind};

/// Life this close to zero snaps to zero, so deltas that exactly cancel the
/// initial value (five −0.1 misses from 0.5) fail despite `f32` rounding.
const LIFE_EPSILON: f32 = 1e-6;

/// Clamps to `0..=1`, snapping near-zero values to zero.
fn clamp_life(life: f32) -> f32 {
    if life < LIFE_EPSILON {
        0.0
    } else {
        life.min(1.0)
    }
}

/// Life change per event kind, as fractions of the bar.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LifeDeltas {
    /// Per tier, indexed by [`Judgement::index`].
    pub tiers: [f32; 6],
    pub held: f32,
    pub let_go: f32,
    pub mine: f32,
    /// FFR-style empty press.
    pub boo: f32,
}

impl LifeDeltas {
    pub const fn new(tiers: [f32; 6], held: f32, let_go: f32, mine: f32) -> LifeDeltas {
        LifeDeltas {
            tiers,
            held,
            let_go,
            mine,
            boo: 0.0,
        }
    }

    pub fn of(&self, kind: &JudgeEventKind) -> f32 {
        match kind {
            JudgeEventKind::Tap(j, _) => self.tiers[j.index()],
            JudgeEventKind::Held => self.held,
            JudgeEventKind::LetGo => self.let_go,
            JudgeEventKind::HitMine => self.mine,
            JudgeEventKind::Boo => self.boo,
        }
    }
}

/// StepMania `LifeMeterBar`.
///
/// Positive deltas are multiplied by `difficulty`, negative ones divided by
/// it. After any loss, `regen_combo_after_miss` is added to a lockout
/// counter (capped at `max_regen_combo`); every non-negative event then
/// decrements the counter and only the event that brings it to zero (and
/// later ones) regains life — exactly `LifeMeterBar::ChangeLife`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LifeBar {
    pub deltas: LifeDeltas,
    pub initial: f32,
    pub danger_threshold: f32,
    pub difficulty: f64,
    pub regen_combo_after_miss: u32,
    pub max_regen_combo: u32,
    /// StepMania `MercifulDrain`: scale losses by `0.5 + 0.5 × life`.
    pub merciful_drain: bool,
    #[serde(skip)]
    life: f32,
    #[serde(skip)]
    lockout: u32,
}

impl LifeBar {
    pub fn new(
        deltas: LifeDeltas,
        initial: f32,
        danger_threshold: f32,
        regen_combo_after_miss: u32,
        max_regen_combo: u32,
    ) -> LifeBar {
        LifeBar {
            deltas,
            initial,
            danger_threshold,
            difficulty: 1.0,
            regen_combo_after_miss,
            max_regen_combo,
            merciful_drain: false,
            life: initial,
            lockout: 0,
        }
    }

    fn change(&mut self, mut delta: f32) {
        if delta > 0.0 {
            delta = (f64::from(delta) * self.difficulty) as f32;
        } else if delta < 0.0 {
            delta = (f64::from(delta) / self.difficulty) as f32;
            if self.merciful_drain {
                delta *= 0.5 + 0.5 * self.life.clamp(0.0, 1.0);
            }
        }
        if delta >= 0.0 {
            self.lockout = self.lockout.saturating_sub(1);
            if self.lockout > 0 {
                delta = 0.0;
            }
        } else {
            let new = (self.lockout + self.regen_combo_after_miss).min(self.max_regen_combo);
            self.lockout = self.lockout.max(new);
        }
        self.life = clamp_life(self.life + delta);
    }
}

impl GaugeRules for LifeBar {
    fn begin(&mut self, _ctx: &ScoreCtx) {
        self.life = self.initial;
        self.lockout = 0;
    }

    fn on_event(&mut self, ev: &JudgeEvent, _ctx: &ScoreCtx) {
        self.change(self.deltas.of(&ev.kind));
    }

    fn life(&self) -> f32 {
        self.life
    }

    fn failed(&self) -> bool {
        self.life <= 0.0
    }

    fn danger(&self) -> bool {
        self.life < self.danger_threshold
    }

    fn fresh(&self) -> Box<dyn GaugeRules> {
        Box::new(LifeBar {
            life: self.initial,
            lockout: 0,
            ..self.clone()
        })
    }
}

/// Lives (StepMania battery, DDR LIFE4).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Battery {
    pub lives: u32,
    /// Worst tap judgement that keeps a life.
    pub min_keep: Judgement,
    pub mine_costs: u32,
    pub let_go_costs: u32,
    #[serde(skip)]
    remaining: u32,
}

impl Battery {
    pub fn new(lives: u32, min_keep: Judgement, mine_costs: u32, let_go_costs: u32) -> Battery {
        Battery {
            lives,
            min_keep,
            mine_costs,
            let_go_costs,
            remaining: lives,
        }
    }
}

impl GaugeRules for Battery {
    fn begin(&mut self, _ctx: &ScoreCtx) {
        self.remaining = self.lives;
    }

    fn on_event(&mut self, ev: &JudgeEvent, _ctx: &ScoreCtx) {
        let cost = match ev.kind {
            JudgeEventKind::Tap(j, _) if j > self.min_keep => 1,
            JudgeEventKind::HitMine => self.mine_costs,
            JudgeEventKind::LetGo => self.let_go_costs,
            _ => 0,
        };
        self.remaining = self.remaining.saturating_sub(cost);
    }

    fn life(&self) -> f32 {
        if self.lives == 0 {
            0.0
        } else {
            self.remaining as f32 / self.lives as f32
        }
    }

    fn failed(&self) -> bool {
        self.remaining == 0
    }

    fn danger(&self) -> bool {
        self.remaining <= 1
    }

    fn fresh(&self) -> Box<dyn GaugeRules> {
        Box::new(Battery::new(
            self.lives,
            self.min_keep,
            self.mine_costs,
            self.let_go_costs,
        ))
    }
}

/// DDR-style fixed-percent gauge (A20 community figures).
///
/// Gains are fixed per tier; the first miss costs `miss_first`, each
/// directly following miss `miss_consecutive`. Below `danger_threshold`
/// losses are multiplied by `danger_loss_mul` and gains by
/// `danger_gain_mul`. `LetGo` and `HitMine` count as misses.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FixedPercent {
    /// Per tier, indexed by [`Judgement::index`]; the `Miss` entry is unused.
    pub gains: [f32; 6],
    pub held: f32,
    pub miss_first: f32,
    pub miss_consecutive: f32,
    pub initial: f32,
    pub danger_threshold: f32,
    pub danger_loss_mul: f32,
    pub danger_gain_mul: f32,
    #[serde(skip)]
    life: f32,
    #[serde(skip)]
    miss_streak: u32,
}

impl FixedPercent {
    /// iamkenzen's DDR A20 figures: +0.4% Marvelous/Perfect, +0.2% Great,
    /// 0 Good, −4.8% first miss, −3.6% consecutive; starts at 50%; DANGER
    /// below 30% halves losses and raises gains to +0.5%/+0.25%.
    pub fn ddr_a20() -> FixedPercent {
        FixedPercent {
            gains: [0.004, 0.004, 0.002, 0.0, 0.0, 0.0],
            held: 0.004,
            miss_first: 0.048,
            miss_consecutive: 0.036,
            initial: 0.5,
            danger_threshold: 0.3,
            danger_loss_mul: 0.5,
            danger_gain_mul: 1.25,
            life: 0.5,
            miss_streak: 0,
        }
    }

    fn miss(&mut self) {
        let mut loss = if self.miss_streak == 0 {
            self.miss_first
        } else {
            self.miss_consecutive
        };
        if self.danger() {
            loss *= self.danger_loss_mul;
        }
        self.miss_streak += 1;
        self.life = clamp_life(self.life - loss);
    }

    fn gain(&mut self, mut amount: f32) {
        if self.danger() {
            amount *= self.danger_gain_mul;
        }
        self.miss_streak = 0;
        self.life = clamp_life(self.life + amount);
    }
}

impl GaugeRules for FixedPercent {
    fn begin(&mut self, _ctx: &ScoreCtx) {
        self.life = self.initial;
        self.miss_streak = 0;
    }

    fn on_event(&mut self, ev: &JudgeEvent, _ctx: &ScoreCtx) {
        match ev.kind {
            JudgeEventKind::Tap(Judgement::Miss, _)
            | JudgeEventKind::LetGo
            | JudgeEventKind::HitMine => self.miss(),
            JudgeEventKind::Tap(j, _) => self.gain(self.gains[j.index()]),
            JudgeEventKind::Held => self.gain(self.held),
            JudgeEventKind::Boo => {}
        }
    }

    fn life(&self) -> f32 {
        self.life
    }

    fn failed(&self) -> bool {
        self.life <= 0.0
    }

    fn danger(&self) -> bool {
        self.life < self.danger_threshold
    }

    fn fresh(&self) -> Box<dyn GaugeRules> {
        Box::new(FixedPercent {
            life: self.initial,
            miss_streak: 0,
            ..self.clone()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_chart::Tick;

    fn tap(j: Judgement) -> JudgeEvent {
        JudgeEvent {
            note_index: None,
            lane: 0,
            tick: Tick::ZERO,
            kind: JudgeEventKind::Tap(j, 0.0),
            song_time: 0.0,
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn life_bar_regen_lockout() {
        let ctx = ScoreCtx::default();
        let mut bar = LifeBar::new(
            LifeDeltas::new(
                [0.008, 0.008, 0.004, 0.0, -0.04, -0.08],
                0.008,
                -0.08,
                -0.16,
            ),
            0.5,
            0.2,
            5,
            10,
        );
        bar.begin(&ctx);
        bar.on_event(&tap(Judgement::Miss), &ctx);
        assert!(close(bar.life(), 0.42));
        // Four W1s are swallowed by the lockout, the fifth regains life.
        for _ in 0..4 {
            bar.on_event(&tap(Judgement::W1), &ctx);
            assert!(close(bar.life(), 0.42));
        }
        bar.on_event(&tap(Judgement::W1), &ctx);
        assert!(close(bar.life(), 0.428));
        // Two misses stack the lockout to 10.
        bar.on_event(&tap(Judgement::Miss), &ctx);
        bar.on_event(&tap(Judgement::Miss), &ctx);
        assert_eq!(bar.lockout, 10);
        assert!(!bar.failed());
    }

    #[test]
    fn life_bar_difficulty_scales_asymmetrically() {
        let ctx = ScoreCtx::default();
        let mut bar = LifeBar::new(
            LifeDeltas::new([0.01, 0.0, 0.0, 0.0, 0.0, -0.1], 0.0, 0.0, 0.0),
            0.5,
            0.2,
            0,
            0,
        );
        bar.difficulty = 0.5;
        bar.begin(&ctx);
        bar.on_event(&tap(Judgement::W1), &ctx);
        assert!(close(bar.life(), 0.505));
        bar.on_event(&tap(Judgement::Miss), &ctx);
        assert!(close(bar.life(), 0.305));
    }

    #[test]
    fn battery_counts_lives() {
        let ctx = ScoreCtx::default();
        let mut b = Battery::new(4, Judgement::W3, 1, 1);
        b.begin(&ctx);
        b.on_event(&tap(Judgement::W3), &ctx);
        assert!(close(b.life(), 1.0));
        b.on_event(&tap(Judgement::W4), &ctx);
        b.on_event(&tap(Judgement::Miss), &ctx);
        assert!(close(b.life(), 0.5));
        assert!(!b.danger());
        b.on_event(&tap(Judgement::W5), &ctx);
        assert!(b.danger());
        b.on_event(&tap(Judgement::Miss), &ctx);
        assert!(b.failed());
    }

    #[test]
    fn fixed_percent_consecutive_misses_and_danger() {
        let ctx = ScoreCtx::default();
        let mut g = FixedPercent::ddr_a20();
        g.begin(&ctx);
        g.on_event(&tap(Judgement::Miss), &ctx);
        assert!(close(g.life(), 0.452));
        g.on_event(&tap(Judgement::Miss), &ctx);
        assert!(close(g.life(), 0.416));
        g.on_event(&tap(Judgement::W3), &ctx);
        assert!(close(g.life(), 0.418));
        g.on_event(&tap(Judgement::Miss), &ctx);
        assert!(close(g.life(), 0.370));
        g.on_event(&tap(Judgement::Miss), &ctx);
        g.on_event(&tap(Judgement::Miss), &ctx);
        assert!(close(g.life(), 0.298));
        assert!(g.danger());
        // In danger losses halve and gains grow.
        g.on_event(&tap(Judgement::Miss), &ctx);
        assert!(close(g.life(), 0.280));
        g.on_event(&tap(Judgement::W1), &ctx);
        assert!(close(g.life(), 0.285));
    }
}
