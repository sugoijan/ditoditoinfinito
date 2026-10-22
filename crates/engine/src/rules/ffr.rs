//! FlashFlashRevolution's rules as its R^3 client (rCubed) plays them
//! (`docs/plans/rulesets.md`, "FFR"; `docs/research/formats-ffr-danoni.md`
//! A2). rCubed is AGPL-3.0: its code was read to learn the rules
//! (`src/Constant.as`, `src/game/GameplayDisplay.as`,
//! `src/game/GameScoreResult.as`, `src/GlobalVariables.as`, at
//! `e57179c`), nothing is transcribed. The windows and the preset are
//! [`super::presets::ffr`]; here are what carries state or a formula: the
//! raw score ([`FfrRaw`]), the life ([`FfrLife`]) and what a play comes to
//! ([`FfrResult`]: the total score, the raw goods and the flag).

use serde::{Deserialize, Serialize};

use super::{GaugeRules, Judgement, ScoreCtx, ScoreRules, ScoreView, Tally};
use crate::judge::{JudgeEvent, JudgeEventKind};

/// The raw score a judgement adds: an Amazing or a Perfect 50, a Good
/// 25, an Average 5, a Miss −10, a Boo −5 (`commitJudge`).
fn raw_of(kind: &JudgeEventKind) -> i64 {
    match kind {
        JudgeEventKind::Tap(j, _) => match j {
            Judgement::W1 | Judgement::W2 => 50,
            Judgement::W3 => 25,
            Judgement::W4 => 5,
            Judgement::W5 => 0,
            Judgement::Miss => -10,
        },
        JudgeEventKind::Boo => -5,
        _ => 0,
    }
}

/// FFR's raw score (rCubed's `gameScore`), what the game shows during
/// play and keeps as a play's score (a level's best and rank are by it).
/// Shown as [`ScoreView::money`], never below 0 (R^3 lets it go
/// negative, which only a play missing nearly everything reaches).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FfrRaw {
    #[serde(skip)]
    raw: i64,
}

impl FfrRaw {
    pub fn new() -> FfrRaw {
        FfrRaw::default()
    }
}

impl ScoreRules for FfrRaw {
    fn begin(&mut self, _ctx: &ScoreCtx) {
        self.raw = 0;
    }

    fn on_event(&mut self, ev: &JudgeEvent, _ctx: &ScoreCtx) {
        self.raw += raw_of(&ev.kind);
    }

    fn view(&self) -> ScoreView {
        ScoreView {
            money: Some(self.raw.max(0) as u64),
            ..ScoreView::default()
        }
    }

    fn finish(&mut self) {}

    fn fresh(&self) -> Box<dyn ScoreRules> {
        Box::new(FfrRaw::new())
    }
}

/// FFR's life (`gameLife`, `updateHealth`): 50 of 100 at the start, 5
/// more for every hit (an Average too), 5 less for a Miss and for a Boo,
/// never above 100; at 0 or below the play is over. Whole numbers, as
/// R^3 keeps them, so it fails exactly where it does. FFR's bar has no
/// danger state (it shades from red to green), so this has none.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FfrLife {
    #[serde(skip)]
    life: i32,
}

impl FfrLife {
    /// Life at the start, and the most.
    pub const START: i32 = 50;
    pub const MAX: i32 = 100;
    /// What a hit adds and a Miss or a Boo takes
    /// (`HEALTH_JUDGE_ADD`/`REMOVE`).
    pub const STEP: i32 = 5;

    pub fn new() -> FfrLife {
        FfrLife { life: Self::START }
    }

    /// Life as R^3 counts it, 0 to 100 (below 0 once failed).
    pub fn points(&self) -> i32 {
        self.life
    }
}

impl Default for FfrLife {
    fn default() -> FfrLife {
        FfrLife::new()
    }
}

impl GaugeRules for FfrLife {
    fn begin(&mut self, _ctx: &ScoreCtx) {
        self.life = Self::START;
    }

    fn on_event(&mut self, ev: &JudgeEvent, _ctx: &ScoreCtx) {
        let delta = match ev.kind {
            JudgeEventKind::Tap(Judgement::Miss, _) | JudgeEventKind::Boo => -Self::STEP,
            JudgeEventKind::Tap(..) => Self::STEP,
            _ => return,
        };
        self.life = (self.life + delta).min(Self::MAX);
    }

    fn life(&self) -> f32 {
        (self.life.max(0) as f32) / (Self::MAX as f32)
    }

    fn failed(&self) -> bool {
        self.life <= 0
    }

    fn danger(&self) -> bool {
        false
    }

    fn fresh(&self) -> Box<dyn GaugeRules> {
        Box::new(FfrLife::new())
    }
}

/// What R^3's song list shows of a play (`getSongIconIndex`, its icons in
/// order: each later one that applies replaces the one before), worst
/// first. AAA is every note an Amazing or a Perfect and no Boo; the flags
/// are AAA but for one judgement (a Boo, a Good, an Average, a Miss; the
/// omniflag one each of a Good, an Average, a Miss and a Boo); single
/// digit goods fewer than 10 raw goods; a full combo every note hit.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Flag {
    /// Played to the end.
    Pass,
    FullCombo,
    SingleDigitGoods,
    Omniflag,
    Missflag,
    Averageflag,
    Blackflag,
    Booflag,
    Aaa,
}

/// What an FFR play comes to besides its raw score
/// ([`crate::player::Results::ffr`]).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FfrResult {
    /// The play's flag ([`Flag`]).
    pub flag: Flag,
    /// R^3's total score (`score_total`): 500 an Amazing or a Perfect,
    /// 250 a Good, 50 an Average and 1,000 a note of the longest combo,
    /// less 300 a Miss and 15 a Boo, plus the raw score; never below 0.
    pub total: u64,
    /// Raw goods in tenths (`raw_goods`): 10 a Good, 18 an Average, 24 a
    /// Miss and 2 a Boo, the raw score short of every note a Perfect over
    /// 25 points.
    pub raw_goods_tenths: u32,
}

impl FfrResult {
    /// A play's result from its counts, its longest combo and the
    /// chart's judged notes.
    pub fn of(tally: &Tally, max_combo: u32, notes: u32) -> FfrResult {
        let perfect = i64::from(tally.count(Judgement::W1) + tally.count(Judgement::W2));
        let good = i64::from(tally.count(Judgement::W3));
        let average = i64::from(tally.count(Judgement::W4));
        let miss = i64::from(tally.count(Judgement::Miss));
        let boo = i64::from(tally.boo);
        let notes = i64::from(notes);
        let combo = i64::from(max_combo);
        let raw = 50 * perfect + 25 * good + 5 * average - 10 * miss - 5 * boo;
        let max_raw = 50 * notes;
        let total =
            500 * perfect + 250 * good + 50 * average + 1000 * combo - 300 * miss - 15 * boo + raw;
        let mut flag = Flag::Pass;
        if notes > 0 {
            let full = combo == notes;
            let only =
                |g: i64, a: i64, m: i64, b: i64| good == g && average == a && miss == m && boo == b;
            if perfect + good + average == notes && miss == 0 && full {
                flag = Flag::FullCombo;
            }
            if max_raw - raw < 250 {
                flag = Flag::SingleDigitGoods;
            }
            if perfect == notes - 3 && only(1, 1, 1, 1) {
                flag = Flag::Omniflag;
            }
            if perfect == notes - 1 && only(0, 0, 1, 0) {
                flag = Flag::Missflag;
            }
            if perfect == notes - 1 && only(0, 1, 0, 0) && full {
                flag = Flag::Averageflag;
            }
            if perfect == notes - 1 && only(1, 0, 0, 0) && full {
                flag = Flag::Blackflag;
            }
            if perfect == notes && only(0, 0, 0, 1) && full {
                flag = Flag::Booflag;
            }
            if raw == max_raw {
                flag = Flag::Aaa;
            }
        }
        FfrResult {
            flag,
            total: total.max(0) as u64,
            raw_goods_tenths: (10 * good + 18 * average + 24 * miss + 2 * boo) as u32,
        }
    }

    /// The raw goods as R^3 writes them, one decimal ("2.4").
    pub fn raw_goods_text(&self) -> String {
        format!(
            "{}.{}",
            self.raw_goods_tenths / 10,
            self.raw_goods_tenths % 10
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn tally(perfect: u32, good: u32, average: u32, miss: u32, boo: u32) -> Tally {
        let mut t = Tally::default();
        t.taps[0] = perfect;
        t.taps[2] = good;
        t.taps[3] = average;
        t.taps[5] = miss;
        t.boo = boo;
        t
    }

    #[test]
    fn raw_score_by_judgement() {
        let ctx = ScoreCtx::default();
        let mut s = FfrRaw::new();
        s.begin(&ctx);
        for j in [Judgement::W1, Judgement::W2, Judgement::W3, Judgement::W4] {
            s.on_event(&tap(j), &ctx);
        }
        assert_eq!(s.view().money, Some(130));
        s.on_event(&tap(Judgement::Miss), &ctx);
        s.on_event(&ev(JudgeEventKind::Boo), &ctx);
        assert_eq!(s.view().money, Some(115));
        assert_eq!(s.view().percent, None);
        // Never shown below 0.
        for _ in 0..20 {
            s.on_event(&tap(Judgement::Miss), &ctx);
        }
        assert_eq!(s.view().money, Some(0));
    }

    #[test]
    fn life_is_whole_fives_from_fifty() {
        let ctx = ScoreCtx::default();
        let mut g = FfrLife::new();
        g.begin(&ctx);
        assert_eq!(g.life(), 0.5);
        for _ in 0..20 {
            g.on_event(&tap(Judgement::W4), &ctx);
        }
        assert_eq!(g.points(), 100);
        assert_eq!(g.life(), 1.0);
        // Ten Misses and ten Boos from full: 0, failed at the last.
        for i in 0..20 {
            assert!(!g.failed(), "{i}");
            let e = if i % 2 == 0 {
                tap(Judgement::Miss)
            } else {
                ev(JudgeEventKind::Boo)
            };
            g.on_event(&e, &ctx);
        }
        assert!(g.failed());
        assert_eq!(g.life(), 0.0);
        assert!(!g.danger());
    }

    #[test]
    fn totals_and_raw_goods() {
        // 100 notes: 97 Perfect, a Good, an Average, a Miss, two Boos,
        // the longest combo 60.
        let r = FfrResult::of(&tally(97, 1, 1, 1, 2), 60, 100);
        let raw = 97 * 50 + 25 + 5 - 10 - 10;
        assert_eq!(
            r.total,
            (97 * 500 + 250 + 50 + 60 * 1000 - 300 - 30 + raw) as u64
        );
        assert_eq!(r.raw_goods_tenths, 10 + 18 + 24 + 4);
        assert_eq!(r.raw_goods_text(), "5.6");
        assert_eq!(r.flag, Flag::SingleDigitGoods);
        // Nothing but misses: 0, not below.
        assert_eq!(FfrResult::of(&tally(0, 0, 0, 10, 0), 0, 10).total, 0);
    }

    #[test]
    fn flags_in_r3s_order() {
        let flag = |p, g, a, m, b, combo| FfrResult::of(&tally(p, g, a, m, b), combo, 20).flag;
        assert_eq!(flag(20, 0, 0, 0, 0, 20), Flag::Aaa);
        assert_eq!(flag(20, 0, 0, 0, 1, 20), Flag::Booflag);
        assert_eq!(flag(19, 1, 0, 0, 0, 20), Flag::Blackflag);
        assert_eq!(flag(19, 0, 1, 0, 0, 20), Flag::Averageflag);
        assert_eq!(flag(19, 0, 0, 1, 0, 12), Flag::Missflag);
        assert_eq!(flag(17, 1, 1, 1, 1, 12), Flag::Omniflag);
        // Two Goods and a Miss: 4.4 raw goods, no full combo.
        assert_eq!(flag(17, 2, 0, 1, 0, 10), Flag::SingleDigitGoods);
        // Ten Goods: 10 raw goods, a full combo.
        assert_eq!(flag(10, 10, 0, 0, 0, 20), Flag::FullCombo);
        assert_eq!(flag(10, 9, 0, 1, 0, 10), Flag::Pass);
        // Boos keep the combo: two of them and a Good are still a full
        // combo and single digit goods, not a flag.
        assert_eq!(flag(19, 1, 0, 0, 2, 20), Flag::SingleDigitGoods);
        // No notes: played to the end.
        assert_eq!(FfrResult::of(&Tally::default(), 0, 0).flag, Flag::Pass);
    }
}
