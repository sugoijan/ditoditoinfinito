//! Shipped rulesets.

use ddi_chart::{Chart, Difficulty};
use serde::{Deserialize, Serialize};

use super::danoni::DanoniOptions;
use super::{
    ComboRules, DancePoints, DdrMoney, DdrVariant, EmptyPress, FailPolicy, FixedPercent,
    GradeBasis, GradeTable, HoldRules, HoldStart, JudgeNames, JudgeTable, Judgement, LifeBar,
    LifeDeltas, Max2, Ruleset, StepManiaOptions, Weights, Window,
};

fn symmetric(tiers: [f64; 5]) -> [Option<Window>; 5] {
    tiers.map(|s| Some(Window::symmetric(s)))
}

fn grades(basis: GradeBasis, tiers: &[(&str, f64)]) -> GradeTable {
    GradeTable {
        basis,
        tiers: tiers.iter().map(|(n, m)| (n.to_string(), *m)).collect(),
        all_w1: None,
        all_w2: None,
        failed: None,
    }
}

/// ITG's dance points (`PercentScoreWeight*`), [`itg`]'s and
/// [`itg_fa_plus`]'s percentage.
const ITG_WEIGHTS: Weights = Weights::new([5, 4, 2, 0, -6, -12], 5, 0, -6);

/// In The Groove as reproduced by Simply Love's "ITG" mode.
pub fn itg() -> Ruleset {
    Ruleset {
        id: "itg".into(),
        name: "ITG (Simply Love)".into(),
        approximate: false,
        judge: JudgeTable {
            tiers: symmetric([0.0215, 0.043, 0.102, 0.135, 0.180]),
            scale: 1.0,
            add: 0.0015,
            hold_window: 0.32,
            roll_window: 0.35,
            mine_window: 0.07,
            // The FA+ split is ITG FA+'s ([`itg_fa_plus`]).
            w0: None,
            empty_press: EmptyPress::Ignore,
            judge_offset: 0.0,
            hold: HoldRules::Decay,
            hold_start: HoldStart::JUDGED,
            supersede: None,
            just: None,
            shock_rows: false,
            miss_at: None,
            taps_only: false,
        },
        combo: ComboRules {
            continue_min: Judgement::W3,
            per_row: false,
            held_increments: false,
            let_go_breaks: false,
            mine_breaks: false,
            neutral: None,
            let_go_loses_full_combo: false,
            jumps_add_each: false,
        },
        score: Box::new(DancePoints::new(
            ITG_WEIGHTS,
            None,
            Some(Weights::new([3, 2, 1, 0, 0, 0], 1, 0, -1)),
        )),
        gauge: Box::new(LifeBar::new(
            LifeDeltas::new(
                [0.008, 0.008, 0.004, 0.0, -0.050, -0.100],
                0.008,
                -0.080,
                -0.050,
            ),
            0.5,
            0.2,
            5,
            10,
        )),
        fail: FailPolicy::Immediate,
        grades: GradeTable {
            failed: Some("F".into()),
            ..grades(
                GradeBasis::Percent,
                &[
                    ("★★★★", 1.00),
                    ("★★★", 0.99),
                    ("★★", 0.98),
                    ("★", 0.96),
                    ("S+", 0.94),
                    ("S", 0.92),
                    ("S-", 0.89),
                    ("A+", 0.86),
                    ("A", 0.83),
                    ("A-", 0.80),
                    ("B+", 0.76),
                    ("B", 0.72),
                    ("B-", 0.68),
                    ("C+", 0.64),
                    ("C", 0.60),
                    ("C-", 0.55),
                    ("D", 0.0),
                ],
            )
        },
        names: JudgeNames::new(
            [
                "Fantastic",
                "Excellent",
                "Great",
                "Decent",
                "Way Off",
                "Miss",
            ],
            "Held",
            "Let Go",
        ),
        danoni: None,
        stepmania: StepManiaOptions::default(),
    }
}

/// ITG with Simply Love's FA+ window: judged, scored and graded as
/// [`itg`], every Fantastic within 15 ms (13.5 ms raw, plus
/// `TimingWindowAdd`; Simply Love's FA+ `W1`, `SL_Init.lua`) a blue one,
/// the rest of the Fantastic window white ones (`IsW0Judgment`,
/// `SL-Helpers.lua`), and the EX score as Simply Love scores it
/// (`CalculateExScore`): blue Fantastic 3.5, white 3, Excellent 2, Great
/// 1, held 1, a mine hit −1, out of 3.5 a step and 1 a hold or roll
/// (`docs/research/rules-ddr-itg-dwi.md` 5.2). Kept in halves here.
pub fn itg_fa_plus() -> Ruleset {
    let itg = itg();
    Ruleset {
        id: "itg-fa+".into(),
        name: "ITG FA+ (Simply Love)".into(),
        judge: JudgeTable {
            w0: Some(0.0135),
            ..itg.judge
        },
        score: Box::new(
            DancePoints::new(
                ITG_WEIGHTS,
                None,
                Some(Weights::new([6, 4, 2, 0, 0, 0], 2, 0, -2)),
            )
            .with_ex_w0(7),
        ),
        names: JudgeNames {
            w0: Some(("Blue Fantastic".into(), "White Fantastic".into())),
            ..itg.names
        },
        ..itg
    }
}

/// Calibration: one wide symmetric window so every tap is recorded with its
/// signed offset; no combo, score, gauge or failure. The window is
/// [`crate::calibration::MAX_OFFSET`] wide on each side; the calibration
/// chart must space notes further apart than twice that so windows never
/// overlap and a press can only match one note.
pub fn calibration() -> Ruleset {
    Ruleset {
        id: "calibration".into(),
        name: "Calibration".into(),
        approximate: false,
        judge: JudgeTable {
            tiers: [
                Some(Window::symmetric(crate::calibration::MAX_OFFSET)),
                None,
                None,
                None,
                None,
            ],
            scale: 1.0,
            add: 0.0,
            hold_window: 0.32,
            roll_window: 0.35,
            mine_window: 0.07,
            w0: None,
            empty_press: EmptyPress::Ignore,
            judge_offset: 0.0,
            hold: HoldRules::Decay,
            hold_start: HoldStart::JUDGED,
            supersede: None,
            just: None,
            shock_rows: false,
            miss_at: None,
            taps_only: false,
        },
        combo: ComboRules {
            continue_min: Judgement::W1,
            per_row: false,
            held_increments: false,
            let_go_breaks: false,
            mine_breaks: false,
            neutral: None,
            let_go_loses_full_combo: false,
            jumps_add_each: false,
        },
        score: Box::new(DancePoints::new(
            Weights::new([1, 0, 0, 0, 0, 0], 0, 0, 0),
            None,
            None,
        )),
        gauge: Box::new(LifeBar::new(
            LifeDeltas::new([0.0; 6], 0.0, 0.0, 0.0),
            1.0,
            0.0,
            0,
            0,
        )),
        fail: FailPolicy::Off,
        grades: grades(GradeBasis::Percent, &[("", 0.0)]),
        names: JudgeNames::new(["hit", "", "", "", "", "miss"], "", ""),
        danoni: None,
        stepmania: StepManiaOptions::default(),
    }
}

/// StepMania 5 engine defaults (dance).
pub fn sm5() -> Ruleset {
    Ruleset {
        id: "sm5".into(),
        name: "StepMania 5".into(),
        approximate: false,
        judge: JudgeTable {
            tiers: symmetric([0.0225, 0.045, 0.090, 0.135, 0.180]),
            scale: 1.0,
            add: 0.0,
            hold_window: 0.25,
            roll_window: 0.5,
            mine_window: 0.09,
            w0: None,
            empty_press: EmptyPress::Ignore,
            judge_offset: 0.0,
            hold: HoldRules::Decay,
            hold_start: HoldStart::JUDGED,
            supersede: None,
            just: None,
            shock_rows: false,
            miss_at: None,
            taps_only: false,
        },
        combo: ComboRules {
            continue_min: Judgement::W3,
            per_row: false,
            held_increments: false,
            let_go_breaks: false,
            mine_breaks: false,
            neutral: None,
            let_go_loses_full_combo: false,
            jumps_add_each: false,
        },
        score: Box::new(DancePoints::new(
            Weights::new([3, 2, 1, 0, 0, 0], 3, 0, -2),
            Some(Weights::new([2, 2, 1, 0, -4, -8], 6, 0, -8)),
            None,
        )),
        gauge: Box::new(LifeBar::new(
            LifeDeltas::new(
                [0.008, 0.008, 0.004, 0.0, -0.040, -0.080],
                0.008,
                -0.080,
                -0.160,
            ),
            0.5,
            0.2,
            5,
            5,
        )),
        fail: FailPolicy::Immediate,
        grades: GradeTable {
            all_w1: Some("AAAA".into()),
            all_w2: Some("AAA".into()),
            failed: Some("F".into()),
            ..grades(
                GradeBasis::Percent,
                &[
                    ("AA", 0.93),
                    ("A", 0.80),
                    ("B", 0.65),
                    ("C", 0.45),
                    ("D", 0.0),
                ],
            )
        },
        names: JudgeNames::new(
            ["Flawless", "Perfect", "Great", "Good", "Boo", "Miss"],
            "OK",
            "NG",
        ),
        danoni: None,
        stepmania: StepManiaOptions::default(),
    }
}

/// DDR A → WORLD, with the community frame-table windows (approximate).
///
/// `W5` is disabled (X2 merged Boo into Miss). DDR has no release
/// judgement but tolerates brief releases, modelled with a 0.25 s hold
/// window. A full row of mines is a shock arrow (`shock_rows`), judged
/// in a 0.07 s window.
pub fn ddr_a() -> Ruleset {
    Ruleset {
        id: "ddr-a".into(),
        name: "DDR A".into(),
        approximate: true,
        judge: JudgeTable {
            tiers: [
                Some(Window::symmetric(0.016667)),
                Some(Window::symmetric(0.033333)),
                Some(Window::symmetric(0.091667)),
                Some(Window::symmetric(0.141667)),
                None,
            ],
            scale: 1.0,
            add: 0.0,
            hold_window: 0.25,
            roll_window: 0.25,
            mine_window: 0.07,
            w0: None,
            empty_press: EmptyPress::Ignore,
            judge_offset: 0.0,
            hold: HoldRules::Decay,
            hold_start: HoldStart::JUDGED,
            supersede: None,
            just: None,
            shock_rows: true,
            miss_at: None,
            taps_only: false,
        },
        combo: ComboRules {
            continue_min: Judgement::W4,
            per_row: true,
            held_increments: false,
            let_go_breaks: true,
            mine_breaks: true,
            neutral: None,
            let_go_loses_full_combo: false,
            jumps_add_each: false,
        },
        score: Box::new(DdrMoney::new(DdrVariant::A)),
        gauge: Box::new(FixedPercent::ddr_a20()),
        fail: FailPolicy::Immediate,
        grades: GradeTable {
            failed: Some("E".into()),
            ..grades(
                GradeBasis::Money,
                &[
                    ("AAA", 990_000.0),
                    ("AA+", 950_000.0),
                    ("AA", 900_000.0),
                    ("AA-", 890_000.0),
                    ("A+", 850_000.0),
                    ("A", 800_000.0),
                    ("A-", 790_000.0),
                    ("B+", 750_000.0),
                    ("B", 700_000.0),
                    ("B-", 690_000.0),
                    ("C+", 650_000.0),
                    ("C", 600_000.0),
                    ("C-", 590_000.0),
                    ("D+", 550_000.0),
                    ("D", 0.0),
                ],
            )
        },
        names: JudgeNames::new(
            ["Marvelous", "Perfect", "Great", "Good", "—", "Miss"],
            "O.K.",
            "N.G.",
        ),
        danoni: None,
        stepmania: StepManiaOptions::default(),
    }
}

/// DDRMAX2 and DDR EXTREME in normal play (`docs/plans/rulesets.md`):
/// their score and dance-point grades ([`Max2`]), and their windows as
/// measured from EXTREME's `GAME.DAT` (Sesse, then PlasmaVenom and
/// Arcorann, Zenius-I-vanisher thread 7564; 1/150 s ticks, each bound
/// taken half a tick out): Perfect ±30 ms, Great −90/+76.667, Good
/// −130/+116.667, Boo −156.667/+183.333. Marvelous judges only in
/// courses ("It is only used in courses", RemyWiki's AC DDR EXTREME), so
/// its window falls to Perfect. Great keeps the combo, a jump adds two to
/// it (per-row judging, [`ComboRules::jumps_add_each`]), an N.G. neither
/// breaks it nor costs dance points. The life bar is StepMania 3.9's,
/// which modeled DDR's (its numbers are not public): `LifeMeterBar`'s
/// defaults with `MercifulDrain`, a regen lockout up to 10 and the harsh
/// penalty from a full bar.
pub fn ddr_extreme() -> Ruleset {
    let mut bar = LifeBar::new(
        LifeDeltas::new(
            [0.008, 0.008, 0.004, 0.0, -0.040, -0.080],
            0.008,
            -0.080,
            -0.160,
        ),
        0.5,
        0.2,
        5,
        10,
    );
    bar.merciful_drain = true;
    bar.hot_penalty = true;
    Ruleset {
        id: "ddr-extreme".into(),
        name: "DDR EXTREME".into(),
        approximate: true,
        judge: JudgeTable {
            tiers: [
                None,
                Some(Window::symmetric(0.030)),
                Some(Window {
                    early: 0.090,
                    late: 0.076_667,
                }),
                Some(Window {
                    early: 0.130,
                    late: 0.116_667,
                }),
                Some(Window {
                    early: 0.156_667,
                    late: 0.183_333,
                }),
            ],
            scale: 1.0,
            add: 0.0,
            hold_window: 0.25,
            roll_window: 0.5,
            mine_window: 0.09,
            w0: None,
            empty_press: EmptyPress::Ignore,
            judge_offset: 0.0,
            hold: HoldRules::Decay,
            hold_start: HoldStart::JUDGED,
            supersede: None,
            just: None,
            shock_rows: false,
            miss_at: None,
            taps_only: false,
        },
        combo: ComboRules {
            continue_min: Judgement::W3,
            per_row: true,
            held_increments: false,
            let_go_breaks: false,
            mine_breaks: false,
            neutral: None,
            let_go_loses_full_combo: false,
            jumps_add_each: true,
        },
        score: Box::new(Max2::new()),
        gauge: Box::new(bar),
        fail: FailPolicy::Immediate,
        grades: GradeTable {
            failed: Some("E".into()),
            ..grades(
                GradeBasis::Percent,
                &[
                    ("AAA", 1.00),
                    ("AA", 0.93),
                    ("A", 0.80),
                    ("B", 0.65),
                    ("C", 0.45),
                    ("D", 0.0),
                ],
            )
        },
        names: JudgeNames::new(
            ["Marvelous", "Perfect", "Great", "Good", "Boo", "Miss"],
            "O.K.",
            "N.G.",
        ),
        danoni: None,
        stepmania: StepManiaOptions::default(),
    }
}

/// DDR X in normal play, its score and grades SuperNOVA2's
/// (`docs/plans/rulesets.md`, "DDR X"; approximate: neither its windows
/// nor its gauge are public).
///
/// The windows are the 60 fps frame table quoted for DDR before A
/// (Marvelous ±1 frame, Perfect ±2, Great ±5.5, Good ±8.5 and the
/// Almost, SuperNOVA to X's name for the old Boo, ±13.5; "the Marvelous
/// window remains at +/-1 frame on DDR A, as it had been in previous
/// games. All other windows have remained intact", Zenius-I-vanisher
/// thread 9479); a note let pass is a Boo, X's name for a miss.
/// Marvelous judges in every mode from SuperNOVA2. Judged a row at a
/// time, a jump one step and one to the combo; Great keeps the combo,
/// Good and worse break it ("The lowest judgment that would not break
/// the player's combo until DanceDanceRevolution (2013)", RemyWiki's
/// glossary), as does an N.G., a freeze let go or a shock stepped on
/// (new in X); a shock avoided adds one, a freeze held none (as DDR A).
/// The score is [`DdrVariant::Sn2`], with no EX score (X had none). The
/// life bar is StepMania 3.9's, as [`ddr_extreme`]'s: X's amounts are
/// not public, only their pattern (Marvelous and Perfect a large gain,
/// Great a small one, Good nothing, Almost a small loss, Boo a large
/// one), which it follows; X showed no DANGER on it. The grades depend
/// on the chart's difficulty ([`ddr_x_grades`], [`ddr_x_for`]); for no
/// chart, Expert's.
pub fn ddr_x() -> Ruleset {
    let mut bar = LifeBar::new(
        LifeDeltas::new(
            [0.008, 0.008, 0.004, 0.0, -0.040, -0.080],
            0.008,
            -0.080,
            -0.160,
        ),
        0.5,
        0.0,
        5,
        10,
    );
    bar.merciful_drain = true;
    bar.hot_penalty = true;
    Ruleset {
        id: "ddr-x".into(),
        name: "DDR X".into(),
        approximate: true,
        judge: JudgeTable {
            tiers: symmetric([0.016_667, 0.033_333, 0.091_667, 0.141_667, 0.225]),
            scale: 1.0,
            add: 0.0,
            hold_window: 0.25,
            roll_window: 0.25,
            mine_window: 0.07,
            w0: None,
            empty_press: EmptyPress::Ignore,
            judge_offset: 0.0,
            hold: HoldRules::Decay,
            hold_start: HoldStart::JUDGED,
            supersede: None,
            just: None,
            shock_rows: true,
            miss_at: None,
            taps_only: false,
        },
        combo: ComboRules {
            continue_min: Judgement::W3,
            per_row: true,
            held_increments: false,
            let_go_breaks: true,
            mine_breaks: true,
            neutral: None,
            let_go_loses_full_combo: false,
            jumps_add_each: false,
        },
        score: Box::new(DdrMoney::new(DdrVariant::Sn2)),
        gauge: Box::new(bar),
        fail: FailPolicy::Immediate,
        grades: ddr_x_grades(Difficulty::Hard),
        names: JudgeNames::new(
            ["Marvelous", "Perfect", "Great", "Good", "Almost", "Boo"],
            "O.K.",
            "N.G.",
        ),
        danoni: None,
        stepmania: StepManiaOptions::default(),
    }
}

/// DDR X's rules ([`ddr_x`]) for `chart`: its grades are its
/// difficulty's ([`ddr_x_grades`]).
pub fn ddr_x_for(chart: &Chart) -> Ruleset {
    Ruleset {
        grades: ddr_x_grades(chart.difficulty),
        ..ddr_x()
    }
}

/// DDR SuperNOVA2 and X's grades, by the chart's difficulty (RemyWiki's
/// "DanceDanceRevolution SuperNOVA2 Scoring System", the table's
/// "SuperNOVA2 to (2014)" columns): AAA from 990,000 and AA from 950,000
/// on every difficulty; A, B and C from 800,000, 700,000 and 500,000 on
/// Beginner and Basic, 850,000, 750,000 and 600,000 on Difficult, and
/// 900,000, 800,000 and 700,000 on Expert and Challenge; D below; E a
/// failed play. No source gives an edit's: it takes Expert and
/// Challenge's (DDR (2013) shows an edit as a Challenge chart,
/// BEMANIWiki's "リンク・EDIT関連"; `docs/plans/rulesets.md`).
pub fn ddr_x_grades(difficulty: Difficulty) -> GradeTable {
    let [a, b, c] = match difficulty {
        Difficulty::Beginner | Difficulty::Easy => [800_000.0, 700_000.0, 500_000.0],
        Difficulty::Medium => [850_000.0, 750_000.0, 600_000.0],
        Difficulty::Hard | Difficulty::Challenge | Difficulty::Edit => {
            [900_000.0, 800_000.0, 700_000.0]
        }
    };
    GradeTable {
        failed: Some("E".into()),
        ..grades(
            GradeBasis::Money,
            &[
                ("AAA", 990_000.0),
                ("AA", 950_000.0),
                ("A", a),
                ("B", b),
                ("C", c),
                ("D", 0.0),
            ],
        )
    }
}

/// One of FFR's 30 frames a second.
const FFR_FRAME: f64 = 1.0 / 30.0;

/// FlashFlashRevolution as its R^3 client plays it
/// (`docs/plans/rulesets.md`, "FFR"; rCubed read for its rules, its code
/// being AGPL: [`super::ffr`]).
///
/// The windows are R^3's default judge (`Constant.JUDGE_WINDOW`), around
/// the instant it judges a note: Amazing −18/+17 ms, Perfect −51/+50,
/// Good −85/+117, Average early only, −118 ms; R^3 takes a bound's early
/// side as outside (`judgeAccuracy > t`), which only an exact instant can
/// tell apart. Hits end 117 ms late, but a note is only missed once the
/// game reaches its sixth frame after it (`GAME_FRAME − FRAME ≥ 6`), 5.5
/// frames after that instant (half a frame into the note's frame,
/// `NoteBox.spawnArrow`): [`JudgeTable::miss_at`]. A press that hits no
/// note is a Boo from the chart's first note on
/// ([`EmptyPress::BooFromFirstNote`]). Judged a note at a time; every hit
/// keeps the combo and a Miss breaks it, a Boo does neither. The raw
/// score and the life are [`super::ffr::FfrRaw`] and
/// [`super::ffr::FfrLife`]; the life empty ends the play at once. No
/// grades: what a play comes to is its flag (AAA, the flags, single digit
/// goods, a full combo), the results' lamp. Only taps are played
/// ([`JudgeTable::taps_only`]): FFR's charts have nothing else, and R^3
/// plays a StepMania chart's holds and rolls as taps and its mines not at
/// all (`ChartStepmania.as` reads `1`, `2` and `4` as notes). The
/// StepMania presets' options (fail type, life and judge levels, the
/// battery) do not apply, as FFR has none of them.
pub fn ffr() -> Ruleset {
    Ruleset {
        id: "ffr".into(),
        name: "FFR (R^3)".into(),
        approximate: false,
        judge: JudgeTable {
            tiers: [
                Some(Window {
                    early: 0.018,
                    late: 0.017,
                }),
                Some(Window {
                    early: 0.051,
                    late: 0.050,
                }),
                Some(Window {
                    early: 0.085,
                    late: 0.117,
                }),
                // No Average on the late side.
                Some(Window {
                    early: 0.118,
                    late: 0.0,
                }),
                None,
            ],
            scale: 1.0,
            add: 0.0,
            // Holds, rolls and mines are not played.
            hold_window: 0.0,
            roll_window: 0.0,
            mine_window: 0.0,
            w0: None,
            empty_press: EmptyPress::BooFromFirstNote,
            judge_offset: 0.0,
            hold: HoldRules::Decay,
            hold_start: HoldStart::JUDGED,
            supersede: None,
            just: None,
            shock_rows: false,
            miss_at: Some(5.5 * FFR_FRAME),
            taps_only: true,
        },
        combo: ComboRules {
            continue_min: Judgement::W4,
            per_row: false,
            held_increments: false,
            let_go_breaks: false,
            mine_breaks: false,
            neutral: None,
            let_go_loses_full_combo: false,
            jumps_add_each: false,
        },
        score: Box::new(super::ffr::FfrRaw::new()),
        gauge: Box::new(super::ffr::FfrLife::new()),
        fail: FailPolicy::Immediate,
        grades: grades(GradeBasis::Money, &[("", 0.0)]),
        names: JudgeNames::new(
            ["Amazing", "Perfect", "Good", "Average", "—", "Miss"],
            "",
            "",
        ),
        danoni: None,
        stepmania: StepManiaOptions::default(),
    }
}

/// Presets selectable by players, in menu order (excludes `calibration`
/// and the chart-dependent [`danoni`] rules).
pub fn all() -> Vec<Ruleset> {
    vec![
        itg(),
        itg_fa_plus(),
        sm5(),
        ddr_a(),
        ddr_x(),
        ddr_extreme(),
        ffr(),
    ]
}

/// Dancing☆Onigiri rules with danoniplus's defaults, as for a chart from
/// another format (see [`super::danoni::ruleset`] for a work's own).
pub fn danoni() -> Ruleset {
    super::danoni::ruleset(None, &DanoniOptions::default())
}

/// Which rules a chart is played by (decision 23).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RulesetMode {
    /// The chosen preset for every chart.
    #[default]
    AllStepMania,
    /// Each chart by the rules of the game it was written for.
    Original,
    /// Dancing☆Onigiri rules for every chart.
    AllDanoni,
}

impl RulesetMode {
    pub const ALL: [RulesetMode; 3] = [
        RulesetMode::AllStepMania,
        RulesetMode::Original,
        RulesetMode::AllDanoni,
    ];
}

/// How the StepMania presets keep holds (`docs/plans/pump.md`).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HoldStyle {
    /// Ticked on pump charts (StepMania's pump game) and their versions
    /// without centers ([`Chart::pump_game`]), dance-style on the rest.
    #[default]
    ByGame,
    /// Held to the end, one O.K. or N.G. (StepMania's dance game).
    Dance,
    /// Judged at checkpoints ([`HoldRules::Ticked`]).
    Ticked,
}

impl HoldStyle {
    pub const ALL: [HoldStyle; 3] = [HoldStyle::ByGame, HoldStyle::Dance, HoldStyle::Ticked];

    /// Whether `chart`'s holds are ticked.
    pub fn ticks(self, chart: &Chart) -> bool {
        match self {
            HoldStyle::ByGame => chart.pump_game(),
            HoldStyle::Dance => false,
            HoldStyle::Ticked => true,
        }
    }
}

/// The ruleset to play `chart` with: the preset `preset` (ITG when
/// unknown), its holds as `holds` says, or the Dancing☆Onigiri rules
/// (which keep their own holds), as `mode` says. Leaving each chart to
/// its game ([`RulesetMode::Original`]) plays a Dancing☆Onigiri work's
/// charts by Dancing☆Onigiri's rules and an FFR level's
/// ([`Chart::ffr`]) by FFR's.
pub fn for_chart(
    mode: RulesetMode,
    preset: &str,
    danoni: &DanoniOptions,
    holds: HoldStyle,
    chart: &Chart,
) -> Ruleset {
    let use_danoni = match mode {
        RulesetMode::AllStepMania => false,
        RulesetMode::Original => chart.danoni.is_some(),
        RulesetMode::AllDanoni => true,
    };
    if use_danoni {
        super::danoni::ruleset(Some(chart), danoni)
    } else if mode == RulesetMode::Original && chart.ffr {
        ffr()
    } else {
        stepmania(preset, holds, chart)
    }
}

/// The preset `id` (ITG when unknown) for `chart`, its holds as `holds`
/// says (FFR's rules play holds as taps, so keep none), DDR X's grades
/// those of the chart's difficulty ([`ddr_x_for`]).
pub fn stepmania(id: &str, holds: HoldStyle, chart: &Chart) -> Ruleset {
    let mut ruleset = match id {
        "ddr-x" => ddr_x_for(chart),
        id => by_id(id).unwrap_or_else(itg),
    };
    if holds.ticks(chart) && !ruleset.judge.taps_only {
        ruleset.judge.hold = HoldRules::Ticked {
            checkpoint_window: HoldRules::CHECKPOINT_WINDOW,
        };
    }
    ruleset
}

/// Preset by id.
pub fn by_id(id: &str) -> Option<Ruleset> {
    match id {
        "itg" => Some(itg()),
        "itg-fa+" => Some(itg_fa_plus()),
        "sm5" => Some(sm5()),
        "ddr-a" => Some(ddr_a()),
        "ddr-x" => Some(ddr_x()),
        "ddr-extreme" => Some(ddr_extreme()),
        "ffr" => Some(ffr()),
        _ => None,
    }
}
