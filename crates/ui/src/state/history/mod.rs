//! The play history (`docs/plans/play-history.md`, step 4): what a play
//! keeps, what is worked out from it, and the bests, platform-free.
//!
//! A play is stored as a [`PlayEntry`] (`ddi_platform::history`): the raw
//! [`PlayRecord`] (never rewritten), the engine's encoded input log and
//! timing as played, and a [`PlaySummary`] worked out from them, which is
//! worked out again ([`rework`]) when the judge has changed since
//! ([`JUDGE_VERSION`]). [`History`] holds the summaries in memory and
//! answers for the bests; [`Plays`] puts it over a [`PlayStore`] for the
//! shells, and [`Reworker`] says when stale plays are worked out again.
//! An export is read back by [`PlayImport`].

mod import;
#[cfg(test)]
mod import_tests;
mod plays;
mod replay;
mod reworker;
#[cfg(test)]
mod tests;
pub mod timing;

use std::collections::{BTreeMap, HashMap};

use ddi_chart::Song;
use ddi_engine::player::{PlayOptions, Results};
use ddi_engine::rules::ddr::DdrClear;
use ddi_engine::rules::ffr::Flag;
use ddi_engine::rules::{FullCombo, Judgement, ScoreView, Tally};
pub use ddi_platform::history::{PlayEntry, PlayStore, Reply, is_play_id, play_id};
use serde::{Deserialize, Serialize};

// The engine's and the chart's history API (steps 1–3).
pub use ddi_chart::ChartDigest;
pub use ddi_engine::JUDGE_VERSION;
pub use ddi_engine::history::{LogError, LoggedInput, PlayLog, PlayTiming, replay};
pub use ddi_engine::rules::RulesetKey;
pub use import::{
    ImportReport, Imported, LinePart, LineProblem, MAX_LINE, MAX_PLAYS, MAX_PROBLEMS, PlayImport,
    Refused, read_line,
};
pub use plays::{Plays, Reworked, export_lines, rework_entries};
pub use replay::{
    Ghost, GhostDelta, GhostRun, Refusal, Replay, ReplayCheck, ReplayProblem, Replaying,
    local_time, play_rows, plays_list,
};
pub use reworker::{FOCUS_STEP_MS, PASS_STEP_MS, REWORK_BATCH, ReworkStep, Reworker};

/// The version of [`PlayRecord`]'s and [`PlaySummary`]'s format. The
/// reader keeps reading every past one.
pub const RECORD_VERSION: u32 = 1;

/// Which chart a play was of: the song, its chart's content (a chart
/// whose notes or timing change is another chart), and where it was in
/// the song (to find it again).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChartKey {
    pub song: String,
    pub digest: ChartDigest,
    pub index: u32,
}

/// How a play ended. A quit is kept too (its results graded QUIT, the
/// notes left unjudged, as after a fail) but never counts toward the bests
/// ([`PlaySummary::counts`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ending {
    Finished,
    Failed,
    Quit,
}

/// A play the game played: kept, never counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Autoplay {
    /// Through the tests' fake pad (`auto=pad`).
    #[serde(default)]
    pub pad: bool,
    /// Presses late by this many seconds (negative: early).
    #[serde(default)]
    pub bias: f64,
}

/// Where an offset in effect came from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OffsetSource {
    /// The device's profile, calibrated.
    Calibrated,
    /// The device's profile, not calibrated (copied from another).
    Profile,
    /// No profile: the settings' default.
    #[default]
    Default,
}

/// An audio path or a display, as the offsets are kept by.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub label: String,
}

/// The clock and the offsets a play ran with.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClockSetup {
    /// Seconds.
    pub audio_offset: f64,
    pub audio_source: OffsetSource,
    pub visual_offset: f64,
    pub visual_source: OffsetSource,
    pub audio: Option<Device>,
    pub display: Option<Device>,
    /// The output latency as the play started, seconds.
    pub output_latency: f64,
    /// Hz.
    pub sample_rate: f64,
    /// The display's refresh rate as the play started, Hz.
    pub refresh_hz: f64,
    /// The output latency's range over the play, seconds.
    pub latency_min: f64,
    pub latency_max: f64,
}

/// How a play was drawn. Settings by their stored names.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DrawSetup {
    /// `None`: the default skin, Infinito.
    pub noteskin: Option<String>,
    /// The note colors and whether a chart's own colors were drawn (the
    /// built-in arrows' fallback palette).
    pub note_colors: String,
    pub chart_colors: bool,
    /// The values set for the drawn skin's parameters
    /// (`docs/plans/skin-parameters.md`; Infinito's when none was chosen).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub skin_params: crate::state::settings::Values,
    pub field_filter: f32,
    pub bg_brightness: f32,
    pub background_frame: String,
    pub video: String,
    /// The timing-error display.
    pub show_deltas: bool,
    /// The graphics backend (`WebGPU`, `WebGL2`, …).
    pub graphics: String,
}

/// An input device a play's lanes were pressed with.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum InputDevice {
    Keyboard,
    Touch,
    /// A controller, by its model (`Gamepad.id`).
    Pad {
        model: String,
    },
    Other {
        name: String,
    },
}

/// Every parameter of a play beyond what judging reads.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlaySetup {
    pub clock: ClockSetup,
    pub drawn: DrawSetup,
    pub inputs: Vec<InputDevice>,
}

/// A play as played: never rewritten. The input log and the timing as
/// played are kept beside it, encoded ([`PlayEntry::log`],
/// [`PlayEntry::timing`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayRecord {
    pub version: u32,
    pub chart: ChartKey,
    pub ruleset: RulesetKey,
    pub options: PlayOptions,
    /// The song offset in effect (seconds; the song was played shifted
    /// by it, `Song::shift_notes`).
    pub song_offset: f64,
    #[serde(default)]
    pub setup: PlaySetup,
    /// The settings as stored when it was played.
    #[serde(default)]
    pub settings: serde_json::Value,
    pub ended: Ending,
    #[serde(default)]
    pub autoplay: Option<Autoplay>,
    /// Milliseconds since the Unix epoch.
    pub time: f64,
    #[serde(default)]
    pub app_version: String,
    /// The judge that judged it ([`JUDGE_VERSION`] then; 1 in records
    /// kept before it was written down, when there was no other): its
    /// timing is that judge's.
    #[serde(default = "first_judge")]
    pub judge_version: u32,
}

fn first_judge() -> u32 {
    1
}

/// The lamp a play earns: failed, cleared, or a full combo's kind,
/// better to the end. Under DDR A's rules, LIFE4's clear types too
/// (`docs/plans/rulesets.md`): a clear under the LIFE4 gauge (or RISKY)
/// and single digit perfects, a perfect full combo short of a marvelous
/// one by 1 to 9 Perfects. Under FFR's rules, its flags instead of the
/// full combos' kinds (`ddi_engine::rules::ffr::Flag`, R^3's song icons
/// in its order): a clear, a full combo, then single digit goods to AAA,
/// which only FFR's rules earn and compare among themselves (bests are
/// kept by ruleset).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lamp {
    /// The player quit ([`Results::quit`]): no lamp, not a fail.
    Quit,
    Failed,
    Clear,
    /// DDI's: one break short of a full combo. DDI's lamps
    /// (`ddi_engine::rules::ddi::Lamp`) are this one, [`Lamp::FullCombo`]
    /// and the five at the end, in its ladder's order; like FFR's flags
    /// they compare only among themselves (bests are kept by ruleset).
    BreakFlag,
    Life4Clear,
    FullCombo,
    GreatFullCombo,
    PerfectFullCombo,
    SingleDigitPerfects,
    MarvelousFullCombo,
    SingleDigitGoods,
    Omniflag,
    Missflag,
    Averageflag,
    Blackflag,
    Booflag,
    Aaa,
    /// DDI's: a full combo with one W3, the rest W2 or better.
    GreatFlag,
    /// DDI's: a full combo, the worst a W2.
    GloriousFullCombo,
    /// DDI's: a full combo with one W2, the rest W1 or better.
    GloriousFlag,
    /// DDI's: a full combo, every tap W1 or better.
    AmazingFullCombo,
    /// DDI's: every tap W0 (the grade's).
    AbsoluteDito,
}

impl Lamp {
    pub fn of(r: &Results) -> Lamp {
        if r.quit {
            Lamp::Quit
        } else if r.failed {
            Lamp::Failed
        } else {
            Lamp::played(r)
        }
    }

    /// The lamp the play's judgements earn, failed or not (the results
    /// show a full combo's kind beside FAILED).
    pub fn played(r: &Results) -> Lamp {
        use ddi_engine::rules::ddi::Lamp as Ddi;
        if let Some(ddi) = &r.ddi {
            return match ddi.lamp {
                Ddi::Failed | Ddi::Clear => Lamp::Clear,
                Ddi::BreakFlag => Lamp::BreakFlag,
                Ddi::FullCombo => Lamp::FullCombo,
                Ddi::GreatFlag => Lamp::GreatFlag,
                Ddi::GloriousFullCombo => Lamp::GloriousFullCombo,
                Ddi::GloriousFlag => Lamp::GloriousFlag,
                Ddi::AmazingFullCombo => Lamp::AmazingFullCombo,
                Ddi::AbsoluteDito => Lamp::AbsoluteDito,
            };
        }
        if let Some(ffr) = &r.ffr {
            return match ffr.flag {
                Flag::Pass => Lamp::Clear,
                Flag::FullCombo => Lamp::FullCombo,
                Flag::SingleDigitGoods => Lamp::SingleDigitGoods,
                Flag::Omniflag => Lamp::Omniflag,
                Flag::Missflag => Lamp::Missflag,
                Flag::Averageflag => Lamp::Averageflag,
                Flag::Blackflag => Lamp::Blackflag,
                Flag::Booflag => Lamp::Booflag,
                Flag::Aaa => Lamp::Aaa,
            };
        }
        let ddr = r.ddr.as_ref();
        match r.full_combo {
            FullCombo::None if ddr.is_some_and(DdrClear::life4) => Lamp::Life4Clear,
            FullCombo::None => Lamp::Clear,
            FullCombo::FC => Lamp::FullCombo,
            FullCombo::GreatFC => Lamp::GreatFullCombo,
            FullCombo::PerfectFC if ddr.is_some() && r.tally.count(Judgement::W2) < 10 => {
                Lamp::SingleDigitPerfects
            }
            FullCombo::PerfectFC => Lamp::PerfectFullCombo,
            FullCombo::MarvelousFC => Lamp::MarvelousFullCombo,
        }
    }

    /// The lamp as colors and commands name it, and written short.
    pub fn names(self) -> (&'static str, &'static str) {
        match self {
            Lamp::Quit => ("Quit", "QUIT"),
            Lamp::Failed => ("Failed", "FAILED"),
            Lamp::Clear => ("Clear", "CLEAR"),
            Lamp::Life4Clear => ("Life4Clear", "LIFE4"),
            Lamp::FullCombo => ("FullCombo", "FC"),
            Lamp::GreatFullCombo => ("GreatFullCombo", "GFC"),
            Lamp::PerfectFullCombo => ("PerfectFullCombo", "PFC"),
            Lamp::SingleDigitPerfects => ("SingleDigitPerfects", "SDP"),
            Lamp::MarvelousFullCombo => ("MarvelousFullCombo", "MFC"),
            Lamp::SingleDigitGoods => ("SingleDigitGoods", "SDG"),
            Lamp::Omniflag => ("Omniflag", "OMNIFLAG"),
            Lamp::Missflag => ("Missflag", "MISSFLAG"),
            Lamp::Averageflag => ("Averageflag", "AVFLAG"),
            Lamp::Blackflag => ("Blackflag", "BLACKFLAG"),
            Lamp::Booflag => ("Booflag", "BOOFLAG"),
            Lamp::Aaa => ("Aaa", "AAA"),
            // DDI's, short enough for a chart's row (as FFR's flags).
            Lamp::BreakFlag => ("BreakFlag", "BREAKFLAG"),
            Lamp::GreatFlag => ("GreatFlag", "GREATFLAG"),
            Lamp::GloriousFullCombo => ("GloriousFullCombo", "GFC"),
            Lamp::GloriousFlag => ("GloriousFlag", "GLORYFLAG"),
            Lamp::AmazingFullCombo => ("AmazingFullCombo", "AFC"),
            Lamp::AbsoluteDito => ("AbsoluteDito", "AD"),
        }
    }

    /// The lamp written out as the results show it; a quit, failed and a
    /// plain clear show none.
    pub fn label(self) -> Option<&'static str> {
        match self {
            Lamp::Quit | Lamp::Failed | Lamp::Clear => None,
            Lamp::Life4Clear => Some("LIFE4 CLEAR"),
            Lamp::FullCombo => Some("FULL COMBO"),
            Lamp::GreatFullCombo => Some("GREAT FULL COMBO"),
            Lamp::PerfectFullCombo => Some("PERFECT FULL COMBO"),
            Lamp::SingleDigitPerfects => Some("SINGLE DIGIT PERFECTS"),
            Lamp::MarvelousFullCombo => Some("MARVELOUS FULL COMBO"),
            Lamp::SingleDigitGoods => Some("SINGLE DIGIT GOODS"),
            Lamp::Omniflag => Some("OMNIFLAG"),
            Lamp::Missflag => Some("MISSFLAG"),
            Lamp::Averageflag => Some("AVERAGEFLAG"),
            Lamp::Blackflag => Some("BLACKFLAG"),
            Lamp::Booflag => Some("BOOFLAG"),
            Lamp::Aaa => Some("AAA"),
            Lamp::BreakFlag => Some("BREAK FLAG"),
            Lamp::GreatFlag => Some("GREAT FLAG"),
            Lamp::GloriousFullCombo => Some("GLORIOUS FULL COMBO"),
            Lamp::GloriousFlag => Some("GLORIOUS FLAG"),
            Lamp::AmazingFullCombo => Some("AMAZING FULL COMBO"),
            Lamp::AbsoluteDito => Some("ABSOLUTE DITO"),
        }
    }
}

/// What is worked out from a play's raw data, by the judge of
/// `judge_version`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Worked {
    pub judge_version: u32,
    pub score: ScoreView,
    pub grade: String,
    pub lamp: Lamp,
    pub full_combo: FullCombo,
    pub assist: bool,
    pub failed: bool,
    pub tally: Tally,
    pub max_combo: u32,
    pub fast: u32,
    pub slow: u32,
    #[serde(default)]
    pub hits: u32,
    /// Seconds.
    pub mean: f64,
    pub stddev: f64,
    /// Presses that reached the play later than it judged by time
    /// (`Results::late_edges`; not shown yet).
    #[serde(default)]
    pub late_edges: u32,
}

impl Worked {
    pub fn from_results(r: &Results) -> Worked {
        Worked {
            judge_version: JUDGE_VERSION,
            score: r.score,
            grade: r.grade.clone(),
            lamp: Lamp::of(r),
            full_combo: r.full_combo,
            assist: r.assist,
            failed: r.failed,
            tally: r.tally,
            max_combo: r.max_combo,
            fast: r.fast,
            slow: r.slow,
            hits: r.hits,
            mean: r.mean_delta,
            stddev: r.stddev_delta,
            late_edges: r.late_edges,
        }
    }

    /// Worked out by an older judge.
    pub fn is_stale(&self) -> bool {
        self.judge_version < JUDGE_VERSION
    }

    /// What a play judged again from its log comes to, `autoplay` when
    /// its record says the game played it: the log does not say who
    /// played, so DDI's Absolute Dito is then Absolute Bot, as the play
    /// itself graded it (`ddi_engine::rules::ddi::bot_grade`).
    pub fn from_replay(r: &Results, autoplay: bool) -> Worked {
        let mut w = Worked::from_results(r);
        if autoplay && r.ddi.is_some() {
            w.grade = ddi_engine::rules::ddi::bot_grade(&w.grade).to_string();
        }
        w
    }
}

/// A play judged again under rules other than those it was played by
/// (`docs/plans/ddi-ruleset.md`, "Re-judging"): which, and what it came
/// to. Kept beside what it came to as played ([`PlaySummary::worked`],
/// never replaced), and counted for the bests under these rules too. The
/// play did not react to these rules (a lane locked by a mine, Boos).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rejudged {
    pub ruleset: RulesetKey,
    /// `None`: it did not read, and is judged again.
    #[serde(default, deserialize_with = "lenient_some")]
    pub worked: Option<Worked>,
}

impl Rejudged {
    fn is_stale(&self) -> bool {
        self.worked.as_ref().is_none_or(Worked::is_stale)
    }
}

/// What judging a play again takes from its summary
/// ([`Plays::asks`]): the rules it waits to be judged under, and the
/// rules it was judged again under before.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ask {
    pub rules: Option<crate::state::settings::RuleChoice>,
    pub rejudged: Option<RulesetKey>,
}

/// [`Ask`]s by play id.
pub type Asks = HashMap<String, Ask>;

/// What a play judged again comes to ([`rework`]): as played, and under
/// the other rules asked for (none where they are the play's own).
#[derive(Clone, Debug, PartialEq)]
pub struct Judged {
    pub worked: Worked,
    pub rejudged: Option<Rejudged>,
}

/// The score a best is chosen by: money where the ruleset counts it,
/// else the percentage, else the EX score; DDI's as its score (the
/// proportional part plus the absolute score, [`better_score`]'s order).
pub fn score_value(v: &ScoreView) -> f64 {
    if let Some(d) = &v.ddi {
        return d.score as f64;
    }
    v.money
        .map(|m| m as f64)
        .or(v.percent)
        .or(v.ex.map(f64::from))
        .unwrap_or(0.0)
}

/// A grade as the lists show it, where there is room for a few letters
/// (a chart's row, a play's): DDI's two long ones by their initials, AD
/// and AB; the others as they are. The results write them out.
pub fn list_grade(grade: &str) -> &str {
    use ddi_engine::rules::ddi::{ABSOLUTE_BOT, ABSOLUTE_DITO};
    match grade {
        ABSOLUTE_DITO => "AD",
        ABSOLUTE_BOT => "AB",
        other => other,
    }
}

/// Whether score `a` is a better best than `b`: DDI's by its score (the
/// sum), then its absolute score (`DdiScoreView::best_cmp`); the others
/// by [`score_value`].
pub fn better_score(a: &ScoreView, b: &ScoreView) -> bool {
    match (&a.ddi, &b.ddi) {
        (Some(a), Some(b)) => a.best_cmp(b).is_gt(),
        _ => score_value(a) > score_value(b),
    }
}

/// A best's score as the ruleset scores: money, a percentage, EX; DDI's
/// as its score, the proportional part plus the absolute score
/// ([`score_text_as`] for its percent).
pub fn score_text(v: &ScoreView) -> String {
    score_text_as(v, false)
}

/// [`score_text`], a DDI score as its percent when `percent`
/// (`Settings::ddi_percent`).
pub fn score_text_as(v: &ScoreView, percent: bool) -> String {
    if let Some(d) = &v.ddi {
        return if percent {
            format!("{}%", d.percent_text())
        } else {
            d.score.to_string()
        };
    }
    match (v.money, v.percent, v.ex) {
        (Some(m), _, _) => m.to_string(),
        (None, Some(p), _) => format!("{:.2}%", p * 100.0),
        (None, None, Some(ex)) => format!("EX {ex}"),
        _ => format!("{:.0}", score_value(v)),
    }
}

/// A field that does not read (a later format, a damaged store) is none:
/// a `worked` then is stale and is worked out again from its record.
fn lenient_some<'de, D: serde::Deserializer<'de>, T: serde::de::DeserializeOwned>(
    d: D,
) -> Result<Option<T>, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(v).ok())
}

/// What is loaded at start for every play: what it was of, and what it
/// came to (`None` until it is worked out again, when it did not read).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlaySummary {
    pub version: u32,
    pub chart: ChartKey,
    pub ruleset: RulesetKey,
    /// Milliseconds since the Unix epoch.
    pub time: f64,
    #[serde(default)]
    pub autoplay: bool,
    pub ended: Ending,
    #[serde(default, deserialize_with = "lenient_some")]
    pub worked: Option<Worked>,
    /// The options as played (the plays list shows them; none in
    /// summaries kept before they were, or that do not read).
    #[serde(default, deserialize_with = "lenient_some")]
    pub options: Option<PlayOptions>,
    /// The devices it ran on and the audio offset in effect
    /// ([`timing::timing_verdict`]); none in summaries kept before they
    /// were, which the verdict leaves out.
    #[serde(default, deserialize_with = "lenient_some")]
    pub clock: Option<SummaryClock>,
    /// The play judged again under other rules ([`Rejudged`]).
    #[serde(
        default,
        deserialize_with = "lenient_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub rejudged: Option<Rejudged>,
    /// The rules it waits to be judged again under ([`Plays::rejudge`]);
    /// stale until it is.
    #[serde(
        default,
        deserialize_with = "lenient_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub rejudge: Option<crate::state::settings::RuleChoice>,
}

/// A play's devices and audio offset, as its summary keeps them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SummaryClock {
    pub audio: String,
    pub display: String,
    /// Seconds.
    pub audio_offset: f64,
}

impl SummaryClock {
    fn of(clock: &ClockSetup) -> Option<SummaryClock> {
        Some(SummaryClock {
            audio: clock.audio.as_ref()?.id.clone(),
            display: clock.display.as_ref()?.id.clone(),
            audio_offset: clock.audio_offset,
        })
    }
}

impl PlaySummary {
    /// Whether the play counts toward the bests: the player's own, played
    /// to the end or failed. Failed plays count, as StepMania keeps them
    /// among the high scores (graded Failed).
    pub fn counts(&self) -> bool {
        !self.autoplay && self.ended != Ending::Quit
    }

    /// Worked out by an older judge, or not readable, or waiting to be
    /// judged again under other rules.
    pub fn is_stale(&self) -> bool {
        self.worked.as_ref().is_none_or(Worked::is_stale)
            || self.rejudge.is_some()
            || self.rejudged.as_ref().is_some_and(Rejudged::is_stale)
    }

    /// What the play came to under the rules `now` keeps bests by (a
    /// [`RulesetKey::best_key`]): judged again under them where it was
    /// (`true`), else as played. With no `now`, as it was judged last.
    pub fn view(&self, now: Option<&RulesetKey>) -> (&RulesetKey, Option<&Worked>, bool) {
        match &self.rejudged {
            Some(r) if now.is_none_or(|k| r.ruleset.best_key() == *k) => {
                (&r.ruleset, r.worked.as_ref(), true)
            }
            _ => (&self.ruleset, self.worked.as_ref(), false),
        }
    }

    /// What judging it again takes from it.
    pub fn ask(&self) -> Ask {
        Ask {
            rules: self.rejudge.clone(),
            rejudged: self.rejudged.as_ref().map(|r| r.ruleset.clone()),
        }
    }

    /// The summary of a play from its record, to be worked out.
    pub fn of(record: &PlayRecord) -> PlaySummary {
        PlaySummary {
            version: RECORD_VERSION,
            chart: record.chart.clone(),
            ruleset: record.ruleset.clone(),
            time: record.time,
            autoplay: record.autoplay.is_some(),
            ended: record.ended,
            worked: None,
            options: Some(record.options),
            clock: SummaryClock::of(&record.setup.clock),
            rejudged: None,
            rejudge: None,
        }
    }

    /// The summary of a play from its record alone, to be worked out
    /// (a summary that was lost or does not read).
    pub fn from_record(record: &str) -> Result<PlaySummary, String> {
        #[derive(Deserialize)]
        struct Keys {
            chart: ChartKey,
            ruleset: RulesetKey,
            time: f64,
            ended: Ending,
            #[serde(default)]
            autoplay: Option<serde_json::Value>,
            #[serde(default)]
            options: Option<serde_json::Value>,
            #[serde(default)]
            setup: Option<serde_json::Value>,
        }
        let k: Keys = serde_json::from_str(record).map_err(|e| e.to_string())?;
        let clock = k
            .setup
            .and_then(|s| serde_json::from_value::<PlaySetup>(s).ok())
            .and_then(|s| SummaryClock::of(&s.clock));
        Ok(PlaySummary {
            version: RECORD_VERSION,
            chart: k.chart,
            ruleset: k.ruleset,
            time: k.time,
            autoplay: k.autoplay.is_some_and(|a| !a.is_null()),
            ended: k.ended,
            worked: None,
            options: k.options.and_then(|o| serde_json::from_value(o).ok()),
            clock,
            rejudged: None,
            rejudge: None,
        })
    }
}

/// A new play: its entry to store and its summary.
pub fn new_entry(
    id: String,
    record: &PlayRecord,
    log: &PlayLog,
    results: &Results,
) -> Result<(PlayEntry, PlaySummary), String> {
    let summary = PlaySummary {
        worked: Some(Worked::from_results(results)),
        ..PlaySummary::of(record)
    };
    let entry = PlayEntry {
        id,
        summary: Some(serde_json::to_string(&summary).map_err(|e| e.to_string())?),
        record: serde_json::to_string(record).map_err(|e| e.to_string())?,
        log: log.encode(),
        timing: PlayTiming::from_results(results).encode(),
    };
    Ok((entry, summary))
}

/// What judging a play again reads from its record (the rest may be of a
/// later format and is left alone).
#[derive(Deserialize)]
struct JudgedBy {
    chart: ChartKey,
    ruleset: RulesetKey,
    #[serde(default)]
    options: PlayOptions,
    #[serde(default)]
    song_offset: f64,
    #[serde(default)]
    autoplay: Option<serde_json::Value>,
}

/// The chart a play was of in `song` (the song as stored, unshifted):
/// where it was, or where a chart of the same content is now.
pub fn find_chart(song: &Song, chart: &ChartKey) -> Option<usize> {
    let at = chart.index as usize;
    if at < song.charts.len() && song.chart_digest(at) == chart.digest {
        return Some(at);
    }
    (0..song.charts.len()).find(|&i| song.chart_digest(i) == chart.digest)
}

/// Judges a stored play again from its log, on `song` (as stored, before
/// any song offset): what it comes to under today's judge by the rules
/// it was played by, and by the other rules `ask` names: those it waits
/// for (the chart's, by the rules chosen then), else those it was judged
/// again under before. Rules that are the play's own leave no second
/// judgment.
pub fn rework(entry: &PlayEntry, song: &Song, ask: &Ask) -> Result<Judged, String> {
    let by: JudgedBy = serde_json::from_str(&entry.record).map_err(|e| e.to_string())?;
    let index = find_chart(song, &by.chart).ok_or("the chart is no longer there")?;
    let log = PlayLog::decode(&entry.log).map_err(|e| e.to_string())?;
    let autoplay = by.autoplay.as_ref().is_some_and(|a| !a.is_null());
    let other = match &ask.rules {
        Some(rules) => Some(rules.ruleset_for(&song.charts[index]).key()),
        None => ask.rejudged.clone(),
    }
    .filter(|key| *key != by.ruleset);
    let song = song.shifted(by.song_offset);
    let judge = |rules: &RulesetKey| {
        Worked::from_replay(&replay(&song, index, rules, &by.options, &log), autoplay)
    };
    Ok(Judged {
        worked: judge(&by.ruleset),
        rejudged: other.map(|ruleset| Rejudged {
            worked: Some(judge(&ruleset)),
            ruleset,
        }),
    })
}

/// A best's key: a chart's content, the rules it was played by (without
/// the gauge chosen) and whether notes were removed (assist plays are
/// apart, as StepMania marks them). Not the song: a pack imported again
/// from another folder gets new song ids, and its charts keep their
/// bests.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BestKey {
    pub digest: ChartDigest,
    pub ruleset: RulesetKey,
    pub assist: bool,
}

/// The best of a chart under one ruleset: the best score and the best
/// lamp (from different plays, maybe), how often it was played and when
/// last.
#[derive(Clone, Debug, PartialEq)]
pub struct Best {
    pub score: ScoreView,
    pub grade: String,
    pub lamp: Lamp,
    pub plays: u32,
    /// Milliseconds since the Unix epoch.
    pub last_played: f64,
    /// A play under this key waits to be worked out again: the values
    /// are the old judge's (or none yet) until then.
    pub pending: bool,
    /// The plays the score and the lamp are from.
    pub score_play: String,
    pub lamp_play: String,
}

/// The summaries of every play kept, in memory.
#[derive(Clone, Debug, Default)]
pub struct History {
    plays: BTreeMap<String, PlaySummary>,
    /// Plays whose summary is missing or does not read, until rebuilt from
    /// their records ([`Plays::load`]); listed, never dropped.
    unreadable: Vec<String>,
}

impl History {
    /// From the store's `(id, summary)` pairs (`None`: a play with no
    /// summary); summaries that do not read are listed apart
    /// ([`History::unreadable`]), never dropped.
    pub fn from_summaries(summaries: Vec<(String, Option<String>)>) -> History {
        let mut h = History::default();
        for (id, json) in summaries {
            match json.and_then(|j| serde_json::from_str::<PlaySummary>(&j).ok()) {
                Some(s) => {
                    h.plays.insert(id, s);
                }
                None => h.unreadable.push(id),
            }
        }
        h
    }

    pub fn len(&self) -> usize {
        self.plays.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plays.is_empty() && self.unreadable.is_empty()
    }

    pub fn unreadable(&self) -> &[String] {
        &self.unreadable
    }

    pub fn get(&self, id: &str) -> Option<&PlaySummary> {
        self.plays.get(id)
    }

    /// Whether play `id` is kept (its summary read or not).
    pub fn contains(&self, id: &str) -> bool {
        self.plays.contains_key(id) || self.unreadable.iter().any(|u| u == id)
    }

    /// Every play, oldest first.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &PlaySummary)> {
        self.plays.iter()
    }

    /// The latest play.
    pub fn last(&self) -> Option<(&String, &PlaySummary)> {
        self.plays.iter().next_back()
    }

    pub fn add(&mut self, id: String, summary: PlaySummary) {
        self.unreadable.retain(|u| *u != id);
        self.plays.insert(id, summary);
    }

    fn mark_unreadable(&mut self, id: String) {
        if !self.plays.contains_key(&id) && !self.unreadable.contains(&id) {
            self.unreadable.push(id);
        }
    }

    /// What was read from the store, added to what is here.
    fn merge(&mut self, read: History) {
        for (id, s) in read.plays {
            self.add(id, s);
        }
        for id in read.unreadable {
            self.mark_unreadable(id);
        }
    }

    /// A play worked out again: as played, and under the other rules
    /// asked for (its wait for them is over).
    pub fn set_worked(&mut self, id: &str, judged: Judged) -> Option<&PlaySummary> {
        let s = self.plays.get_mut(id)?;
        s.worked = Some(judged.worked);
        s.rejudged = judged.rejudged;
        s.rejudge = None;
        Some(s)
    }

    /// Every play (those under ruleset `only`, a preset's id, when
    /// given) waits to be judged again under `rules`; the summaries that
    /// changed.
    pub fn ask_rejudge(
        &mut self,
        rules: &crate::state::settings::RuleChoice,
        only: Option<&str>,
    ) -> Vec<(String, &PlaySummary)> {
        let mut changed = Vec::new();
        for (id, s) in self.plays.iter_mut() {
            if only.is_some_and(|o| s.ruleset.id != o) || s.rejudge.as_ref() == Some(rules) {
                continue;
            }
            s.rejudge = Some(rules.clone());
            changed.push((id.clone(), &*s));
        }
        changed
    }

    pub fn remove(&mut self, ids: &[String]) {
        for id in ids {
            self.plays.remove(id);
        }
        self.unreadable.retain(|u| !ids.contains(u));
    }

    /// Every play's id (the unreadable ones too).
    pub fn ids(&self) -> Vec<String> {
        self.plays.keys().chain(&self.unreadable).cloned().collect()
    }

    /// The plays of a chart (by content, under any song id).
    pub fn ids_of_chart(&self, digest: ChartDigest) -> Vec<String> {
        self.select(|s| s.chart.digest == digest)
    }

    /// The plays of these songs (a pack's).
    pub fn ids_of_songs(&self, songs: &[String]) -> Vec<String> {
        self.select(|s| songs.contains(&s.chart.song))
    }

    /// The plays of `song` worked out by an older judge (or not
    /// readable), oldest first.
    pub fn stale_of(&self, song: &str) -> Vec<String> {
        self.select(|s| s.is_stale() && s.chart.song == song)
    }

    /// Whether a play of `song` waits to be worked out again.
    pub fn has_stale(&self, song: &str) -> bool {
        self.plays
            .values()
            .any(|s| s.is_stale() && s.chart.song == song)
    }

    /// How many plays wait to be worked out again.
    pub fn stale_count(&self) -> usize {
        self.plays.values().filter(|s| s.is_stale()).count()
    }

    fn select(&self, f: impl Fn(&PlaySummary) -> bool) -> Vec<String> {
        self.plays
            .iter()
            .filter(|(_, s)| f(s))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Every best.
    pub fn bests(&self) -> HashMap<BestKey, Best> {
        self.bests_where(|_| true)
    }

    /// The bests of these charts (a song's, by their digests), whatever
    /// song id they were played under.
    pub fn bests_of_charts(&self, digests: &[ChartDigest]) -> HashMap<BestKey, Best> {
        self.bests_where(|s| digests.contains(&s.chart.digest))
    }

    /// The best of a chart under a ruleset (the gauge chosen aside).
    pub fn best(&self, digest: ChartDigest, ruleset: &RulesetKey, assist: bool) -> Option<Best> {
        let key = BestKey {
            digest,
            ruleset: ruleset.best_key(),
            assist,
        };
        self.bests_where(|s| s.chart.digest == digest).remove(&key)
    }

    /// The best of `song`'s chart `chart` (as stored) under the rules and
    /// the assist the settings play it with: a play's ghost's.
    pub fn best_of_play(
        &self,
        song: &Song,
        chart: usize,
        settings: &crate::state::settings::Settings,
    ) -> Option<Best> {
        let c = song.charts.get(chart)?;
        self.best(
            song.chart_digest(chart),
            &settings.ruleset_for(c).key(),
            c.derivation
                .as_ref()
                .is_some_and(ddi_chart::simplify::Derivation::assisted)
                || settings.transform.is_assist(),
        )
    }

    fn bests_where(&self, f: impl Fn(&PlaySummary) -> bool) -> HashMap<BestKey, Best> {
        let mut out: HashMap<BestKey, Best> = HashMap::new();
        // Plays not readable yet: their charts' bests are pending, with
        // the others' values.
        let mut unread = Vec::new();
        for (id, s) in self.plays.iter().filter(|(_, s)| s.counts() && f(s)) {
            // As played, and under the rules it was judged again by.
            let again = s.rejudged.as_ref().map(|r| (&r.ruleset, r.worked.as_ref()));
            let waits = s.rejudge.is_some();
            for (ruleset, worked) in std::iter::once((&s.ruleset, s.worked.as_ref())).chain(again) {
                let Some(w) = worked else {
                    unread.push((s.chart.digest, ruleset.best_key()));
                    continue;
                };
                let key = BestKey {
                    digest: s.chart.digest,
                    ruleset: ruleset.best_key(),
                    assist: w.assist,
                };
                let b = out.entry(key).or_insert_with(|| Best {
                    score: w.score,
                    grade: w.grade.clone(),
                    lamp: w.lamp,
                    plays: 0,
                    last_played: s.time,
                    pending: false,
                    score_play: id.clone(),
                    lamp_play: id.clone(),
                });
                b.plays += 1;
                b.last_played = b.last_played.max(s.time);
                b.pending |= w.is_stale() || waits;
                if better_score(&w.score, &b.score) {
                    b.score = w.score;
                    b.grade = w.grade.clone();
                    b.score_play = id.clone();
                }
                if w.lamp > b.lamp {
                    b.lamp = w.lamp;
                    b.lamp_play = id.clone();
                }
            }
        }
        for (k, b) in out.iter_mut() {
            b.pending |= unread
                .iter()
                .any(|(d, r)| *d == k.digest && *r == k.ruleset);
        }
        out
    }
}
