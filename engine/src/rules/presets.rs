//! Shipped rulesets.

use super::{
    ComboRules, DancePoints, DdrMoney, DdrVariant, EmptyPress, FailPolicy, FixedPercent,
    GradeBasis, GradeTable, JudgeNames, JudgeTable, Judgement, LifeBar, LifeDeltas, Ruleset,
    Weights, Window,
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
            w0: Some(0.0135),
            empty_press: EmptyPress::Ignore,
            judge_offset: 0.0,
        },
        combo: ComboRules {
            continue_min: Judgement::W3,
            per_row: false,
            held_increments: false,
            let_go_breaks: false,
            mine_breaks: false,
        },
        score: Box::new(DancePoints::new(
            Weights::new([5, 4, 2, 0, -6, -12], 5, 0, -6),
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
        },
        combo: ComboRules {
            continue_min: Judgement::W1,
            per_row: false,
            held_increments: false,
            let_go_breaks: false,
            mine_breaks: false,
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
        },
        combo: ComboRules {
            continue_min: Judgement::W3,
            per_row: false,
            held_increments: false,
            let_go_breaks: false,
            mine_breaks: false,
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
    }
}

/// DDR A → WORLD, with the community frame-table windows (approximate).
///
/// `W5` is disabled (X2 merged Boo into Miss). DDR has no release
/// judgement but tolerates brief releases, modelled with a 0.25 s hold
/// window. Mines use a 0.07 s window for shock arrows later.
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
        },
        combo: ComboRules {
            continue_min: Judgement::W4,
            per_row: true,
            held_increments: false,
            let_go_breaks: true,
            mine_breaks: true,
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
    }
}

/// All presets, in menu order.
/// Presets selectable by players (excludes `calibration`).
pub fn all() -> Vec<Ruleset> {
    vec![itg(), sm5(), ddr_a()]
}

/// Preset by id.
pub fn by_id(id: &str) -> Option<Ruleset> {
    match id {
        "itg" => Some(itg()),
        "sm5" => Some(sm5()),
        "ddr-a" => Some(ddr_a()),
        _ => None,
    }
}
