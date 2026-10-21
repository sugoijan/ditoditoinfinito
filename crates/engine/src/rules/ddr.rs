//! DDR's own gauges for the DDR A preset (`docs/plans/rulesets.md`): the
//! DANCE GAUGE option's LIFE4, RISKY, FLARE I–EX and FLOATING FLARE, and
//! the FLARE rank and FLARE SKILL points a play earns.

use serde::{Deserialize, Serialize};

use super::{Battery, GaugeRules, Judgement, ScoreCtx};
use crate::judge::{JudgeEvent, JudgeEventKind};

/// The DDR presets' DANCE GAUGE (DDR A3 and WORLD's option, Konami's
/// option list; `docs/research/rules-ddr-itg-dwi.md` 3.2): the preset's
/// own life bar, or one of DDR's other gauges.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DdrGauge {
    /// The preset's own gauge (NORMAL).
    #[default]
    Normal,
    /// Four lives, one lost for each combo break (RemyWiki's general
    /// info: a Miss and an N.G. since DDR (2013)).
    Life4,
    /// One life (RISKY).
    Risky,
    /// FLARE I (1) to IX (9), and EX (10).
    Flare(u8),
    /// FLOATING FLARE: FLARE EX, a level down each time it empties.
    Floating,
}

impl DdrGauge {
    /// In the option's order.
    pub const ALL: [DdrGauge; 14] = [
        DdrGauge::Normal,
        DdrGauge::Life4,
        DdrGauge::Risky,
        DdrGauge::Flare(1),
        DdrGauge::Flare(2),
        DdrGauge::Flare(3),
        DdrGauge::Flare(4),
        DdrGauge::Flare(5),
        DdrGauge::Flare(6),
        DdrGauge::Flare(7),
        DdrGauge::Flare(8),
        DdrGauge::Flare(9),
        DdrGauge::Flare(10),
        DdrGauge::Floating,
    ];

    /// As DDR writes it ("LIFE4", "FLARE VII").
    pub fn name(self) -> String {
        match self {
            DdrGauge::Normal => "Normal".into(),
            DdrGauge::Life4 => "LIFE4".into(),
            DdrGauge::Risky => "RISKY".into(),
            DdrGauge::Flare(level) => format!("FLARE {}", flare_name(level)),
            DdrGauge::Floating => "FLOATING FLARE".into(),
        }
    }

    /// A FLARE level out of range is the nearest one.
    pub fn sanitized(self) -> DdrGauge {
        match self {
            DdrGauge::Flare(level) => DdrGauge::Flare(level.clamp(1, FLARE_EX)),
            other => other,
        }
    }

    /// The gauge, `None` for the preset's own. A life is lost for a tap
    /// worse than `keep` (the worst tier that keeps the combo), a mine or
    /// shock stepped on and a freeze let go: what breaks DDR's combo.
    pub fn gauge(self, keep: Judgement) -> Option<Box<dyn GaugeRules>> {
        Some(match self.sanitized() {
            DdrGauge::Normal => return None,
            DdrGauge::Life4 => Box::new(Battery::new(4, keep, 1, 1)),
            DdrGauge::Risky => Box::new(Battery::new(1, keep, 1, 1)),
            DdrGauge::Flare(level) => Box::new(Flare::new(level, false)),
            DdrGauge::Floating => Box::new(Flare::new(FLARE_EX, true)),
        })
    }
}

/// What a play under DDR A's rules (with a bar, not StepMania's battery)
/// earns beyond its score ([`crate::Results::ddr`]): the gauge it was
/// played with, the FLARE rank it kept, and its FLARE SKILL points.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DdrClear {
    pub gauge: DdrGauge,
    /// Cleared without the gauge ever emptying (a FAILED play keeps no
    /// rank, as DDR WORLD has it, even under fail off): 0 under NORMAL,
    /// LIFE4 and RISKY, 1–10 the FLARE level kept (FLOATING FLARE's at
    /// the end).
    pub flare: Option<u8>,
    /// FLARE SKILL ([`flare_points`]) of such a clear, where no option
    /// assisted ("アシストオプションを設定してプレーした場合、フレアスキルは
    /// 獲得できません") and the chart's level is one of DDR's.
    pub points: Option<u32>,
}

impl DdrClear {
    /// Cleared under LIFE4 (or the stricter RISKY): LIFE4's red lamp,
    /// "Earned when you clear a song with "Life 4" enabled"
    /// (life4ddr.com/about).
    pub fn life4(&self) -> bool {
        self.flare.is_some() && matches!(self.gauge, DdrGauge::Life4 | DdrGauge::Risky)
    }
}

/// FLARE EX's level.
pub const FLARE_EX: u8 = 10;

/// A FLARE level's numeral ("VII", "EX"); none for 0.
pub fn flare_name(level: u8) -> &'static str {
    const NAMES: [&str; 11] = [
        "", "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "EX",
    ];
    NAMES[usize::from(level.min(FLARE_EX))]
}

/// A FLARE gauge's full value: hundredths of a percent, so the drains,
/// given to two decimals, add up exactly.
const FLARE_FULL: i32 = 10_000;

/// What each judgement takes from a FLARE gauge, in hundredths of a
/// percent, level I first: `[Perfect, Great, Good, Miss or N.G.]`.
/// BEMANIWiki's estimates for DDR WORLD (`DanceDanceRevolution WORLD/基礎
/// 知識`, "FLARE GAUGE"): V to EX as in A3, I to IV lowered in WORLD
/// (`docs/plans/rulesets.md`). A Marvelous and an O.K. take nothing; an
/// N.G. (a freeze let go, a shock stepped on) takes a Miss's share.
const FLARE_DRAINS: [[i32; 4]; 10] = [
    [0, 10, 50, 150],
    [0, 10, 63, 300],
    [0, 10, 75, 450],
    [0, 29, 145, 1100],
    [0, 74, 370, 1600],
    [0, 92, 460, 1800],
    [0, 128, 640, 2200],
    [0, 164, 820, 2600],
    [0, 200, 1000, 3000],
    [100, 200, 1000, 3000],
];

/// DDR's FLARE gauge: starts full, never recovers, a fixed share taken
/// for each judgement short of a Marvelous ([`FLARE_DRAINS`]), failed
/// once empty. FLOATING FLARE keeps every level's gauge from the start
/// and plays on the highest still alive, failed when FLARE I empties
/// (BEMANIWiki: "内部的に全レベルのフレアゲージ残量を計算しており、現在の
/// ゲージが尽きると「その時点で生存している一番高いフレアゲージ」に移行する").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Flare {
    /// The level played from, 1 (I) to 10 (EX).
    pub level: u8,
    pub floating: bool,
    /// Each level's gauge, level I first.
    #[serde(skip)]
    left: [i32; 10],
}

impl Flare {
    pub fn new(level: u8, floating: bool) -> Flare {
        Flare {
            level: level.clamp(1, FLARE_EX),
            floating,
            left: [FLARE_FULL; 10],
        }
    }

    /// The level played on now: the gauge's own, or under FLOATING FLARE
    /// the highest still alive; `None` once it has emptied.
    pub fn current(&self) -> Option<u8> {
        let lowest = if self.floating { 1 } else { self.level };
        (lowest..=self.level)
            .rev()
            .find(|&l| self.left[usize::from(l - 1)] > 0)
    }

    /// Which of `[Perfect, Great, Good, Miss]` an event drains, if any.
    fn column(kind: &JudgeEventKind, ticked: bool) -> Option<usize> {
        match kind.scored_as(ticked)? {
            JudgeEventKind::Tap(Judgement::W2, _) => Some(0),
            JudgeEventKind::Tap(Judgement::W3, _) => Some(1),
            JudgeEventKind::Tap(Judgement::W4, _) => Some(2),
            JudgeEventKind::Tap(Judgement::W5 | Judgement::Miss, _)
            | JudgeEventKind::LetGo
            | JudgeEventKind::HitShock
            | JudgeEventKind::HitMine => Some(3),
            _ => None,
        }
    }
}

impl GaugeRules for Flare {
    fn begin(&mut self, _ctx: &ScoreCtx) {
        self.left = [FLARE_FULL; 10];
    }

    fn on_event(&mut self, ev: &JudgeEvent, ctx: &ScoreCtx) {
        let Some(column) = Flare::column(&ev.kind, ctx.ticked) else {
            return;
        };
        for (left, drains) in self.left.iter_mut().zip(FLARE_DRAINS) {
            *left = (*left - drains[column]).max(0);
        }
    }

    fn life(&self) -> f32 {
        self.current().map_or(0.0, |l| {
            self.left[usize::from(l - 1)] as f32 / FLARE_FULL as f32
        })
    }

    fn failed(&self) -> bool {
        self.current().is_none()
    }

    /// DDR draws a low FLARE gauge grey, never in DANGER.
    fn danger(&self) -> bool {
        false
    }

    fn flare_rank(&self) -> Option<u8> {
        self.current()
    }

    fn fresh(&self) -> Box<dyn GaugeRules> {
        Box::new(Flare::new(self.level, self.floating))
    }
}

/// FLARE SKILL's base points of a chart's level, 1 to 19 (BEMANIWiki,
/// `DanceDanceRevolution WORLD/フレアスキル`).
const FLARE_BASE: [u32; 19] = [
    145, 155, 170, 185, 205, 230, 255, 290, 335, 400, 465, 510, 545, 575, 600, 620, 635, 650, 665,
];

/// DDR WORLD's FLARE SKILL points for a chart of level `meter` cleared
/// at FLARE rank `rank` (0 under NORMAL, LIFE4 and RISKY, 1–10 FLARE
/// I–EX): "基準点*(1+フレアランク*0.06)", floored, worked in integers.
/// `None` for a level outside DDR's 1 to 19.
pub fn flare_points(meter: u32, rank: u8) -> Option<u32> {
    let base = *FLARE_BASE.get(usize::try_from(meter).ok()?.checked_sub(1)?)?;
    Some(base * (100 + 6 * u32::from(rank.min(FLARE_EX))) / 100)
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

    fn after(gauge: &mut dyn GaugeRules, events: &[JudgeEvent]) {
        let ctx = ScoreCtx::default();
        gauge.begin(&ctx);
        for e in events {
            gauge.on_event(e, &ctx);
        }
    }

    #[test]
    fn flare_gauges_drain_as_bemaniwiki_has_them() {
        // FLARE IX: the 50th Great fails ("グレ49まで耐えられる").
        let mut ix = Flare::new(9, false);
        after(&mut ix, &[tap(Judgement::W3); 49]);
        assert!(!ix.failed());
        assert!((ix.life() - 0.02).abs() < 1e-6);
        ix.on_event(&tap(Judgement::W3), &ScoreCtx::default());
        assert!(ix.failed());
        assert_eq!(ix.flare_rank(), None);
        // FLARE I (WORLD) lives through 66 misses ("66MISSまで耐えられる").
        let mut i = Flare::new(1, false);
        after(&mut i, &[tap(Judgement::Miss); 66]);
        assert_eq!(i.flare_rank(), Some(1));
        i.on_event(&tap(Judgement::Miss), &ScoreCtx::default());
        assert!(i.failed());
        // Only EX drains on a Perfect; nothing on a Marvelous or an O.K.;
        // an N.G. as a Miss.
        let events = [
            tap(Judgement::W1),
            tap(Judgement::W2),
            ev(JudgeEventKind::Held),
            ev(JudgeEventKind::AvoidShock),
            ev(JudgeEventKind::LetGo),
        ];
        let mut ex = Flare::new(FLARE_EX, false);
        let mut ix = Flare::new(9, false);
        after(&mut ex, &events);
        after(&mut ix, &events);
        assert!((ex.life() - 0.69).abs() < 1e-6, "{}", ex.life());
        assert!((ix.life() - 0.70).abs() < 1e-6);
        assert!(!ex.danger() && ex.danger_line().is_none());
    }

    #[test]
    fn floating_flare_moves_to_the_highest_level_alive() {
        // Four misses empty EX, IX and VIII (30, 30, 26 a miss): VII is
        // left with 12% (BEMANIWiki's example).
        let mut f = Flare::new(FLARE_EX, true);
        after(&mut f, &[tap(Judgement::Miss); 4]);
        assert_eq!(f.flare_rank(), Some(7));
        assert!((f.life() - 0.12).abs() < 1e-6);
        assert!(!f.failed());
        // Every level empty: failed, never NORMAL.
        let ctx = ScoreCtx::default();
        for _ in 0..67 {
            f.on_event(&tap(Judgement::Miss), &ctx);
        }
        assert!(f.failed());
        assert_eq!(f.flare_rank(), None);
        assert_eq!(f.fresh().flare_rank(), Some(FLARE_EX));
    }

    #[test]
    fn flare_skill_points() {
        // BEMANIWiki's and gamerch's tables: level 19 and 12 at EX, IX,
        // VIII and VII; level 17 at EX; level 18 at IX.
        assert_eq!(
            [10, 9, 8, 7].map(|r| flare_points(19, r)),
            [Some(1064), Some(1024), Some(984), Some(944)]
        );
        assert_eq!(
            [10, 9, 8, 7].map(|r| flare_points(12, r)),
            [Some(816), Some(785), Some(754), Some(724)]
        );
        assert_eq!(flare_points(17, 10), Some(1016));
        assert_eq!(flare_points(18, 9), Some(1001));
        // Exact where the product is whole: 600 × 1.06.
        assert_eq!(flare_points(15, 1), Some(636));
        assert_eq!(flare_points(1, 0), Some(145));
        assert_eq!(flare_points(0, 0), None);
        assert_eq!(flare_points(20, 3), None);
    }

    #[test]
    fn life4_and_risky_lose_a_life_a_combo_break() {
        let mut life4 = DdrGauge::Life4.gauge(Judgement::W4).unwrap();
        after(
            &mut *life4,
            &[
                tap(Judgement::W4),
                tap(Judgement::Miss),
                ev(JudgeEventKind::LetGo),
                ev(JudgeEventKind::HitShock),
            ],
        );
        assert!(!life4.failed());
        assert!((life4.life() - 0.25).abs() < 1e-6);
        let mut risky = DdrGauge::Risky.gauge(Judgement::W4).unwrap();
        after(&mut *risky, &[tap(Judgement::W4), tap(Judgement::W3)]);
        assert!(!risky.failed());
        risky.on_event(&tap(Judgement::Miss), &ScoreCtx::default());
        assert!(risky.failed());
        assert!(DdrGauge::Normal.gauge(Judgement::W4).is_none());
        assert_eq!(DdrGauge::Flare(7).name(), "FLARE VII");
        assert_eq!(DdrGauge::Flare(12).sanitized(), DdrGauge::Flare(FLARE_EX));
    }
}
