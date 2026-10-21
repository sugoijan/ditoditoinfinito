//! Rules: judgement windows, combo, scoring, gauges, grades and presets.
//!
//! Everything here is data plus two small strategy traits ([`ScoreRules`]
//! and [`GaugeRules`]) for the parts that carry state and formulas. A
//! [`Ruleset`] bundles one of each; [`presets`] builds the shipped ones.

pub mod danoni;
pub mod ddr;
pub mod gauge;
pub mod presets;
pub mod score;

use serde::{Deserialize, Serialize};

pub use gauge::{Battery, FixedPercent, LifeBar, LifeDeltas};
pub use score::{DancePoints, DdrMoney, DdrVariant, Max2, Weights};

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
    /// How a hold is kept once its head is hit.
    #[serde(default)]
    pub hold: HoldRules,
    /// How a hold's head is judged.
    #[serde(default)]
    pub hold_start: HoldStart,
    /// danoniplus's rule that an unjudged late note gives way to the next
    /// one in its lane; `None` = notes wait until they pass every window.
    #[serde(default)]
    pub supersede: Option<Supersede>,
    /// Hits this close to the note count as neither fast nor slow
    /// (danoniplus `justFrames`); `None` = [`JudgeTable::DEFAULT_JUST`].
    #[serde(default)]
    pub just: Option<Window>,
    /// DDR "JUDGMENT TIMING", seconds. Positive means the game expects
    /// presses *later*: a press at a given instant is judged as that much
    /// **earlier** (its delta shrinks), the same direction as
    /// [`ClockOptions::audio_offset`](crate::clock::ClockOptions::audio_offset).
    /// Raise it when you keep getting "Fast".
    pub judge_offset: f64,
    /// DDR's shock arrows: a row with a mine in every lane is one shock,
    /// avoided as a whole (O.K.) or stepped on in any lane (N.G.); else
    /// such a row is mines, as StepMania plays it.
    #[serde(default)]
    pub shock_rows: bool,
}

impl JudgeTable {
    /// Fast/slow dead zone when a ruleset names none: 1 ms, so presses on
    /// time (autoplay) count as neither.
    pub const DEFAULT_JUST: Window = Window::symmetric(0.001);

    /// Whether a hit `delta` seconds off (positive = late) is fast, slow or
    /// neither (`None`).
    pub fn fast_slow(&self, delta: f64) -> Option<bool> {
        let just = self.just.unwrap_or(Self::DEFAULT_JUST);
        if delta < -just.early {
            Some(true)
        } else if delta > just.late {
            Some(false)
        } else {
            None
        }
    }

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

    /// Whether a hit of tier `tier`, `delta` seconds off, is inside the
    /// [`JudgeTable::w0`] sub-window (scaled as the tiers are; Simply
    /// Love's `IsW0Judgment` compares the offset's magnitude the same
    /// way): `None` where the table has no such window.
    pub fn is_w0(&self, tier: Judgement, delta: f64) -> Option<bool> {
        let w0 = self.w0?;
        Some(tier == Judgement::W1 && delta.abs() <= self.scaled(w0))
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

/// How a hit hold is kept to its end.
#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum HoldRules {
    /// StepMania: released, a hold's life decays over `hold_window`
    /// (rolls over `roll_window`) and comes back when pressed again.
    #[default]
    Decay,
    /// danoniplus (`frzAttempt`): a hold is dropped once it has been
    /// released for more than `budget` seconds in all; pressing again stops
    /// the count but never resets it.
    ReleaseBudget { budget: f64 },
    /// StepMania's pump game (`docs/plans/pump.md`): holds and rolls are
    /// judged at checkpoints (the chart's `#TICKCOUNTS`); released, a
    /// hold's life runs out over `checkpoint_window` seconds (scaled) and
    /// comes back full when pressed again; nothing is dropped before its
    /// end, and a hold starts at its head whether or not the head was hit.
    Ticked { checkpoint_window: f64 },
}

impl HoldRules {
    /// StepMania's `TimingWindowSecondsCheckpoint`.
    pub const CHECKPOINT_WINDOW: f64 = 0.1664;

    pub fn is_ticked(self) -> bool {
        matches!(self, HoldRules::Ticked { .. })
    }
}

/// How the head of a hold is judged.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HoldStart {
    /// The head gets a tap judgement (StepMania; danoniplus with
    /// `frzStartjdgUse`). Otherwise a hold only counts by its outcome.
    pub judged: bool,
    /// danoniplus: a press within the tap windows starts the hold only
    /// inside this window; outside it the hold is dropped. `None`: any
    /// judged press starts it.
    pub accept: Option<Window>,
}

impl Default for HoldStart {
    fn default() -> HoldStart {
        HoldStart::JUDGED
    }
}

impl HoldStart {
    /// StepMania: the head is judged like a tap and any hit starts the hold.
    pub const JUDGED: HoldStart = HoldStart {
        judged: true,
        accept: None,
    };
}

/// danoniplus `judgeNextFunc.arrowOFF`/`frzOFF` (`js/lib/mainWindow.js`
/// 797–847): a note still unjudged gives way to the next one in its lane.
/// A tap gives way to the next tap once that is `next_within` seconds away
/// and the tap `prev_late` seconds late; a hold's start gives way to the
/// next tap or hold by `hold_next_within` and `hold_prev_late`. A hold
/// never takes over from an earlier tap.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Supersede {
    pub next_within: f64,
    pub prev_late: f64,
    pub hold_next_within: f64,
    pub hold_prev_late: f64,
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
    /// A tier that neither adds to nor breaks the combo (danoniplus's
    /// Good, `judgeMatari`); `None` when every tier does one or the other.
    #[serde(default)]
    pub neutral: Option<Judgement>,
    /// A dropped hold loses the full-combo lamp even when it does not break
    /// the combo (danoniplus keeps a separate hold combo).
    #[serde(default)]
    pub let_go_loses_full_combo: bool,
    /// With [`ComboRules::per_row`], a row that continues the combo adds
    /// one for each of its notes (DDRMAX to EXTREME: "Jumps now adding two
    /// to your combo (but count as only one step for all other
    /// purposes)", RemyWiki's AC DDRMAX).
    #[serde(default)]
    pub jumps_add_each: bool,
}

/// What happens when the gauge says "failed".
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FailPolicy {
    /// Arcade: the song ends at once.
    Immediate,
    /// The play is marked failed but continues to the end; the gauge
    /// takes nothing more (StepMania's `LifeMeterBar::ChangeLife` returns
    /// once the player has failed).
    ImmediateContinue,
    /// Only checked at the end of the song: against `min_life`
    /// ([`GaugeRules::fails_at_end`]), or with `None` whether the gauge
    /// says failed then (StepMania's `FailType_EndOfSong` marks a player
    /// failed whose life meter `IsFailing` as the song ends,
    /// `ScreenGameplay.cpp`).
    EndOfSong { min_life: Option<f32> },
    /// Never fail.
    Off,
}

/// StepMania's fail types (`FailType`, `PlayerOptions.cpp`; the player
/// options' "Fail" row: Immediate, Delayed, Fail at end, Off), which the
/// player chooses for the StepMania presets.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FailType {
    /// The song ends when the gauge empties (the presets' own).
    #[default]
    Immediate,
    /// Failed when the gauge empties, played on to the end.
    ImmediateContinue,
    /// Failed when the gauge is empty as the song ends.
    EndOfSong,
    /// Never failed.
    Off,
}

impl FailType {
    pub const ALL: [FailType; 4] = [
        FailType::Immediate,
        FailType::ImmediateContinue,
        FailType::EndOfSong,
        FailType::Off,
    ];

    pub fn policy(self) -> FailPolicy {
        match self {
            FailType::Immediate => FailPolicy::Immediate,
            FailType::ImmediateContinue => FailPolicy::ImmediateContinue,
            FailType::EndOfSong => FailPolicy::EndOfSong { min_life: None },
            FailType::Off => FailPolicy::Off,
        }
    }
}

/// StepMania 5.1's `LifeDifficulty` levels' scales, level 1 first
/// (`ScreenOptionsMasterPrefs.cpp`, "StepMania 3.9 and 4.0 values"): the
/// `LifeDifficultyScale` preference a life bar multiplies its gains by
/// and divides its losses by (`LifeMeterBar::ChangeLife`).
pub const LIFE_SCALES: [f64; 7] = [1.60, 1.40, 1.20, 1.00, 0.80, 0.60, 0.40];

/// StepMania 5.1's judge (`TimingWindowScale`) levels' scales, level 1
/// first, the ninth "Justice" (`ScreenOptionsMasterPrefs.cpp`): every
/// window is `scale × seconds + add` (`Player::GetWindowSeconds`).
pub const JUDGE_SCALES: [f64; 9] = [1.50, 1.33, 1.16, 1.00, 0.84, 0.66, 0.50, 0.33, 0.20];

/// StepMania's life meters the player may choose instead of a preset's
/// own (`LifeType`, the player options' "Life" row; its third, Lifetime,
/// is for courses).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LifeType {
    /// The preset's own gauge.
    #[default]
    Bar,
    /// Lives ([`gauge::Battery`]): one lost for each tap worse than
    /// `W3`, mine stepped on and hold let go, as `LifeMeterBattery` with
    /// `_fallback`'s metrics (`MinScoreToKeepLife` W3, `SubtractLives`,
    /// `MinesSubtractLives` and `LetGoSubtractLives` 1); none gained back
    /// within a song.
    Battery,
}

impl LifeType {
    pub const ALL: [LifeType; 2] = [LifeType::Bar, LifeType::Battery];
}

/// What the player chooses for the StepMania presets beyond the preset:
/// the fail type, StepMania's life and judge difficulty as its levels (4
/// leaves a preset as it is), and the life meter. Not under
/// Dancing☆Onigiri's rules, whose gauges and judge range are its own
/// ([`danoni::DanoniOptions`]).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct StepManiaOptions {
    #[serde(deserialize_with = "crate::lenient")]
    pub fail: FailType,
    /// 1 (easiest) to 7, [`LIFE_SCALES`].
    pub life: u8,
    /// 1 (widest) to 9 (Justice), [`JUDGE_SCALES`].
    pub judge: u8,
    #[serde(deserialize_with = "crate::lenient")]
    pub life_type: LifeType,
    /// The battery's lives, 1 to [`StepManiaOptions::MAX_LIVES`]
    /// (`BatteryLives`, 4 by default).
    pub lives: u8,
    /// DDR's DANCE GAUGE, for the DDR presets (with the life meter a
    /// bar).
    #[serde(deserialize_with = "crate::lenient")]
    pub gauge: ddr::DdrGauge,
}

impl Default for StepManiaOptions {
    fn default() -> StepManiaOptions {
        StepManiaOptions {
            fail: FailType::Immediate,
            life: StepManiaOptions::DEFAULT_LEVEL,
            judge: StepManiaOptions::DEFAULT_LEVEL,
            life_type: LifeType::Bar,
            lives: StepManiaOptions::DEFAULT_LIVES,
            gauge: ddr::DdrGauge::Normal,
        }
    }
}

impl StepManiaOptions {
    /// The level that leaves a preset as it is (StepMania's default, 1.0).
    pub const DEFAULT_LEVEL: u8 = 4;
    /// StepMania's `PlayerOptions::m_BatteryLives` as it starts.
    pub const DEFAULT_LIVES: u8 = 4;
    /// The most lives the options offer (StepMania's battery rows go to
    /// 10).
    pub const MAX_LIVES: u8 = 10;

    pub fn is_default(&self) -> bool {
        *self == StepManiaOptions::default()
    }

    /// The levels and lives clamped to those StepMania has.
    pub fn sanitized(self) -> StepManiaOptions {
        StepManiaOptions {
            life: self.life.clamp(1, LIFE_SCALES.len() as u8),
            judge: self.judge.clamp(1, JUDGE_SCALES.len() as u8),
            lives: self.lives.clamp(1, Self::MAX_LIVES),
            gauge: self.gauge.sanitized(),
            ..self
        }
    }

    pub fn life_scale(&self) -> f64 {
        LIFE_SCALES[usize::from(self.sanitized().life - 1)]
    }

    pub fn judge_scale(&self) -> f64 {
        JUDGE_SCALES[usize::from(self.sanitized().judge - 1)]
    }

    /// A judge level's name: its number, or "Justice".
    pub fn judge_name(level: u8) -> String {
        if usize::from(level) == JUDGE_SCALES.len() {
            "Justice".into()
        } else {
            level.to_string()
        }
    }

    /// What differs from the defaults, as the mods line says it ("fail
    /// off", "life 6", "judge 5").
    pub fn summary(&self) -> Vec<String> {
        let mut out = Vec::new();
        match self.fail {
            FailType::Immediate => {}
            FailType::ImmediateContinue => out.push("fail delayed".into()),
            FailType::EndOfSong => out.push("fail at end".into()),
            FailType::Off => out.push("fail off".into()),
        }
        let s = self.sanitized();
        if s.gauge != ddr::DdrGauge::Normal {
            out.push(s.gauge.name());
        }
        if s.life_type == LifeType::Battery {
            out.push(format!(
                "battery {} {}",
                s.lives,
                if s.lives == 1 { "life" } else { "lives" }
            ));
        }
        if s.life != Self::DEFAULT_LEVEL {
            out.push(format!("life {}", s.life));
        }
        if s.judge != Self::DEFAULT_LEVEL {
            out.push(format!("judge {}", Self::judge_name(s.judge)));
        }
        out
    }
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
#[serde(default)]
pub struct Tally {
    /// Per tier, indexed by [`Judgement::index`].
    pub taps: [u32; 6],
    pub held: u32,
    pub let_go: u32,
    pub mine_hit: u32,
    pub boo: u32,
    /// Checkpoint rows of ticked holds hit and missed.
    #[serde(default)]
    pub checkpoint_hit: u32,
    #[serde(default)]
    pub checkpoint_miss: u32,
    /// Of the `W1` taps, those inside the ruleset's [`JudgeTable::w0`]
    /// (ITG FA+'s blue Fantastics); 0 where it has none.
    #[serde(default)]
    pub w0: u32,
}

impl Tally {
    pub fn record(&mut self, kind: &JudgeEventKind) {
        self.record_times(kind, 1);
    }

    /// Records `kind` as `n` events (a `#COMBOS` segment's count).
    /// A shock avoided counts as an O.K. and one stepped on as an N.G.,
    /// with the holds', as DDR counts them.
    pub fn record_times(&mut self, kind: &JudgeEventKind, n: u32) {
        let count = match kind {
            JudgeEventKind::Tap(j, _) => &mut self.taps[j.index()],
            JudgeEventKind::Held | JudgeEventKind::AvoidShock => &mut self.held,
            JudgeEventKind::LetGo | JudgeEventKind::HitShock => &mut self.let_go,
            JudgeEventKind::HitMine => &mut self.mine_hit,
            JudgeEventKind::Boo => &mut self.boo,
            JudgeEventKind::Checkpoint { missed: 0, .. } => &mut self.checkpoint_hit,
            JudgeEventKind::Checkpoint { .. } => &mut self.checkpoint_miss,
        };
        *count += n;
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
            || tally.checkpoint_miss > 0
            || ((combo.let_go_breaks || combo.let_go_loses_full_combo) && tally.let_go > 0)
            || (combo.mine_breaks && tally.mine_hit > 0)
        {
            return FullCombo::None;
        }
        let Some(worst) = tally.worst_tap() else {
            // Only holds, whose starts were not judged: all held is the
            // best lamp.
            return if tally.held > 0 {
                FullCombo::MarvelousFC
            } else {
                FullCombo::None
            };
        };
        if worst
            > combo
                .continue_min
                .max(combo.neutral.unwrap_or(combo.continue_min))
        {
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
        let clean = tally.let_go == 0
            && tally.mine_hit == 0
            && tally.checkpoint_miss == 0
            && tally.total_taps() > 0;
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
    /// Under a [`JudgeTable::w0`] window, the results' names of a `W1`
    /// inside it and outside it ("Blue Fantastic", "White Fantastic");
    /// play shows both as `W1`'s name, in two colors.
    #[serde(default)]
    pub w0: Option<(String, String)>,
}

impl JudgeNames {
    pub fn new(tiers: [&str; 6], held: &str, let_go: &str) -> JudgeNames {
        JudgeNames {
            tiers: tiers.map(String::from),
            held: held.to_string(),
            let_go: let_go.to_string(),
            w0: None,
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
    /// Judged mines.
    pub mines: u32,
    /// Judged shocks (rows, [`JudgeTable::shock_rows`]), weighed as holds.
    #[serde(default)]
    pub shocks: u32,
    /// Combo after the current event.
    pub combo: u32,
    /// Holds are ticked ([`HoldRules::Ticked`]): their outcomes weigh
    /// nothing, their judged checkpoint rows weigh as steps.
    pub ticked: bool,
    /// Judged checkpoint rows of ticked holds.
    pub checkpoints: u32,
    /// The current event is a hit inside the ruleset's
    /// [`JudgeTable::w0`] (an EX score weighs it apart).
    #[serde(default)]
    pub w0: bool,
    /// The chart's meter (its foot rating), which DDR's older money
    /// scores and FLARE SKILL weigh by.
    #[serde(default)]
    pub meter: u32,
}

/// Score as the HUD shows it. Fields a scoring system does not produce are `None`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
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
    /// Simply Love's EX score, in `0..=1`: its points over the chart's
    /// most, floored to a hundredth of a percent (ITG FA+; `ex` and
    /// `max_ex` are then `None`).
    pub ex_percent: Option<f64>,
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
    /// The life (`0..=1`, as [`GaugeRules::life`]) below which the gauge
    /// is in danger, where it is one (the results' life graph draws it).
    fn danger_line(&self) -> Option<f32> {
        None
    }
    /// Whether a play ending now fails [`FailPolicy::EndOfSong`] with
    /// `min_life` (a gauge may compare more exactly than through
    /// [`GaugeRules::life`]).
    fn fails_at_end(&self, min_life: f32) -> bool {
        self.life() < min_life
    }
    /// Takes StepMania's `LifeDifficultyScale` (gains multiplied by it,
    /// losses divided), where the gauge is a life bar; `false` where it
    /// has none (StepMania scales only `LifeMeterBar`).
    fn set_difficulty(&mut self, _scale: f64) -> bool {
        false
    }
    /// The FLARE level the gauge is on, where it is a FLARE gauge
    /// ([`ddr::Flare`]) still alive.
    fn flare_rank(&self) -> Option<u8> {
        None
    }
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
    /// The player's Dancing☆Onigiri options it was built with for a chart
    /// ([`danoni::ruleset`]); `None` for the StepMania presets and for
    /// Dancing☆Onigiri's rules built for no chart.
    pub danoni: Option<danoni::DanoniOptions>,
    /// The player's StepMania options it was built with
    /// ([`Ruleset::with_stepmania`]): the defaults for the rest, and the
    /// default life level where the gauge takes none.
    pub stepmania: StepManiaOptions,
}

/// What builds the same [`Ruleset`] again for a chart (a play's record,
/// `docs/plans/play-history.md`): its id, and what varies under one id
/// as the player chose it. Tunings a ruleset can be given beyond the
/// players' choices (its windows, a judge offset) are not in it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RulesetKey {
    /// `itg`, `sm5`, `ddr-a`, `danoni`, `calibration`.
    pub id: String,
    /// The StepMania presets' holds as played: [`presets::HoldStyle::Dance`]
    /// or [`presets::HoldStyle::Ticked`] (never `ByGame`: resolved for
    /// the chart). `Dance` under Dancing☆Onigiri, which keeps its own.
    #[serde(default)]
    pub holds: presets::HoldStyle,
    /// Dancing☆Onigiri's options: the gauge chosen, the judge range and
    /// the excessive setting. `None` under `danoni`: the rules built for
    /// no chart ([`presets::danoni`]), whatever the chart.
    #[serde(default)]
    pub danoni: Option<danoni::DanoniOptions>,
    /// The StepMania presets' fail type and difficulty levels (the
    /// defaults under the other rules, and in records kept before them).
    #[serde(default, skip_serializing_if = "StepManiaOptions::is_default")]
    pub stepmania: StepManiaOptions,
}

impl RulesetKey {
    /// The ruleset this key was taken from, for `chart`.
    pub fn build(&self, chart: &ddi_chart::Chart) -> Ruleset {
        match self.id.as_str() {
            "danoni" => match &self.danoni {
                Some(options) => danoni::ruleset(Some(chart), options),
                None => presets::danoni(),
            },
            "calibration" => presets::calibration(),
            id => presets::stepmania(id, self.holds, chart).with_stepmania(&self.stepmania),
        }
    }

    /// The key bests are kept under: without Dancing☆Onigiri's gauge
    /// choice, nor the StepMania presets' fail type and life difficulty,
    /// which change the life bar and whether the play fails but not how
    /// notes are judged and scored (a best from any gauge counts; the
    /// judge difficulty, which changes the windows, stays).
    pub fn best_key(&self) -> RulesetKey {
        RulesetKey {
            danoni: self
                .danoni
                .clone()
                .map(|d| danoni::DanoniOptions { gauge: None, ..d }),
            // The judge level only.
            stepmania: StepManiaOptions {
                judge: self.stepmania.judge,
                ..StepManiaOptions::default()
            },
            ..self.clone()
        }
    }
}

impl Ruleset {
    /// What builds this ruleset again ([`RulesetKey::build`]). The
    /// Dancing☆Onigiri excessive setting is the one in effect (the
    /// chart's when the player left it to the chart), so one rule has one
    /// key.
    pub fn key(&self) -> RulesetKey {
        RulesetKey {
            id: self.id.clone(),
            holds: if self.judge.hold.is_ticked() {
                presets::HoldStyle::Ticked
            } else {
                presets::HoldStyle::Dance
            },
            danoni: self.danoni.clone().map(|d| danoni::DanoniOptions {
                excessive: Some(matches!(
                    self.judge.empty_press,
                    EmptyPress::ExcessiveEarly { .. }
                )),
                ..d
            }),
            stepmania: self.stepmania,
        }
    }

    /// Whether this is one of the StepMania presets (not
    /// Dancing☆Onigiri's rules, not calibration's).
    pub fn is_stepmania(&self) -> bool {
        !matches!(self.id.as_str(), "danoni" | "calibration")
    }

    /// Whether the player's DDR gauge ([`StepManiaOptions::gauge`])
    /// applies: DDR A's (A3 and WORLD's DANCE GAUGE option).
    pub fn takes_ddr_gauge(&self) -> bool {
        self.id == "ddr-a"
    }

    /// The StepMania preset with the player's fail type, life and judge
    /// difficulty and life meter (`options`); other rules as they are.
    /// The judge level scales every window, holds', rolls' and mines' too
    /// (StepMania's `GetWindowSeconds`); the life level only a life bar's
    /// changes (`LifeMeterBar`), so under a gauge without one (a battery,
    /// DDR A's) the default level is kept, as are the default lives under
    /// a bar. DDR's gauge replaces DDR A's own under a bar (a battery
    /// replaces either), and is kept as the default elsewhere.
    pub fn with_stepmania(mut self, options: &StepManiaOptions) -> Ruleset {
        if !self.is_stepmania() {
            return self;
        }
        let mut options = options.sanitized();
        self.judge.scale *= options.judge_scale();
        match options.life_type {
            LifeType::Bar => {
                options.lives = StepManiaOptions::DEFAULT_LIVES;
                match options.gauge.gauge(self.combo.continue_min) {
                    Some(gauge) if self.takes_ddr_gauge() => self.gauge = gauge,
                    _ => options.gauge = ddr::DdrGauge::Normal,
                }
            }
            LifeType::Battery => {
                options.gauge = ddr::DdrGauge::Normal;
                self.gauge = Box::new(Battery::new(u32::from(options.lives), Judgement::W3, 1, 1))
            }
        }
        if !self.gauge.set_difficulty(options.life_scale()) {
            options.life = StepManiaOptions::DEFAULT_LEVEL;
        }
        self.fail = options.fail.policy();
        self.stepmania = options;
        self
    }

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
            danoni: self.danoni.clone(),
            stepmania: self.stepmania,
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
            hold: HoldRules::Decay,
            hold_start: HoldStart::JUDGED,
            supersede: None,
            just: None,
            shock_rows: false,
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
            neutral: None,
            let_go_loses_full_combo: false,
            jumps_add_each: false,
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
