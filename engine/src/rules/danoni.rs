//! Dancing☆Onigiri rules as danoniplus plays them: frame windows, the
//! hold release budget, gauges resolved from a chart's headers, and the
//! score and ranks.
//!
//! References are to danoniplus `js/lib/*.js` (v51.2.2, `develop` @
//! `80c3c47`); `docs/research/formats-ffr-danoni.md` B3–B5 and B8 sum
//! them up.
//!
//! danoniplus counts in 60 fps frames: a press at continuous time `t`
//! (frames) of a note arriving at frame `A` is judged by `A − floor(t)`
//! against `|diff| ≤ k`, so a tier of `k` frames covers `[A − k, A + k + 1)`.
//! The windows here are `k` frames early and `k + 1` frames late.

use ddi_chart::formats::danoni::{dos, expr};
use ddi_chart::{Chart, DanoniChart, NoteKind};
use serde::{Deserialize, Serialize};

use super::{
    ComboRules, EmptyPress, FailPolicy, GaugeRules, GradeBasis, GradeTable, HoldRules, HoldStart,
    JudgeNames, JudgeTable, Judgement, Ruleset, ScoreCtx, ScoreRules, ScoreView, Supersede, Window,
};
use crate::judge::{JudgeEvent, JudgeEventKind};

/// One danoniplus frame, seconds.
pub const FRAME: f64 = 1.0 / 60.0;

/// `C_VAL_MAXLIFE`.
const DEFAULT_MAX_LIFE: f64 = 1000.0;

/// `C_FRM_FRZATTEMPT`.
const DEFAULT_FRZ_ATTEMPT: u32 = 5;

/// Excessive: a press 9–16 frames early of a note in its lane, a quarter
/// of a miss's damage (`mainWindow.js` 2491–2493, `lifeDamage`).
const EXCESSIVE_FACTOR: f64 = 0.25;

/// danoniplus's selectable judge ranges (`g_judgRanges`,
/// `danoni_constants.js` 1149–1154).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum JudgeRange {
    #[default]
    Normal,
    Narrow,
    Hard,
    ExHard,
}

impl JudgeRange {
    pub const ALL: [JudgeRange; 4] = [
        JudgeRange::Normal,
        JudgeRange::Narrow,
        JudgeRange::Hard,
        JudgeRange::ExHard,
    ];

    pub fn name(self) -> &'static str {
        match self {
            JudgeRange::Normal => "Normal",
            JudgeRange::Narrow => "Narrow",
            JudgeRange::Hard => "Hard",
            JudgeRange::ExHard => "ExHard",
        }
    }

    /// Arrow tiers in frames: Ii, Shakin, Matari, Shobon, Uwan (the last is
    /// the Excessive bound).
    pub fn arrow(self) -> [u32; 5] {
        match self {
            JudgeRange::Normal => [2, 4, 6, 8, 16],
            JudgeRange::Narrow => [2, 3, 4, 8, 16],
            JudgeRange::Hard => [1, 3, 5, 8, 16],
            JudgeRange::ExHard => [1, 2, 3, 8, 16],
        }
    }

    /// Hold tiers in frames: Kita, the start bound (`sfsf`), Iknai.
    pub fn freeze(self) -> [u32; 3] {
        match self {
            JudgeRange::Normal | JudgeRange::Narrow => [2, 4, 8],
            JudgeRange::Hard => [1, 3, 8],
            JudgeRange::ExHard => [1, 2, 8],
        }
    }
}

/// `k` frames early, `k + 1` late.
fn frames(k: u32) -> Window {
    Window {
        early: f64::from(k) * FRAME,
        late: f64::from(k + 1) * FRAME,
    }
}

/// The player's Dancing☆Onigiri options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DanoniOptions {
    /// Gauge name (`Original`, `Normal`, a chart's custom one); `None`, or
    /// a name the chart does not offer, plays the chart's first gauge.
    pub gauge: Option<String>,
    pub range: JudgeRange,
    /// Excessive (early stray presses cost life); `None` follows the chart
    /// (`excessiveJdgUse`, off by default).
    pub excessive: Option<bool>,
}

/// A gauge as danoniplus resolves it for one chart (`resolveGaugeValues`,
/// `dosConverter.js` 2147–2205).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GaugeDef {
    pub name: String,
    /// Percent of the maximum life the play must end at or above; `None`
    /// for a survival gauge (`x`), which only fails at zero, like border 0.
    pub border: Option<f64>,
    /// Life units per recovery, damage; per arrow count when `variable`.
    pub recovery: f64,
    pub damage: f64,
    /// Percent of the maximum life the play starts with.
    pub init: f64,
    /// Recovery and damage scale with the chart's note count
    /// (`value × maxLifeVal / allArrows`).
    pub variable: bool,
}

/// `(name, border, recovery, damage, init, variable, header overridable,
/// recovery derived ×2 from)`: `g_gaugeDefObj`, `danoni_constants.js`
/// 1330–1341. `None` damage is the maximum life (SuddenDeath).
type Def = (
    &'static str,
    Option<f64>,
    f64,
    Option<f64>,
    f64,
    bool,
    bool,
    Option<&'static str>,
);

const DEFS: [Def; 9] = [
    ("Original", None, 6.0, Some(40.0), 25.0, false, true, None),
    ("Heavy", None, 2.0, Some(50.0), 50.0, false, false, None),
    (
        "NoRecovery",
        None,
        0.0,
        Some(50.0),
        100.0,
        false,
        false,
        None,
    ),
    ("SuddenDeath", None, 0.0, None, 100.0, false, false, None),
    ("Practice", None, 0.0, Some(0.0), 50.0, false, false, None),
    (
        "Light",
        None,
        12.0,
        Some(40.0),
        25.0,
        false,
        true,
        Some("Original"),
    ),
    ("Normal", Some(70.0), 2.0, Some(7.0), 25.0, true, true, None),
    ("Hard", Some(0.0), 1.0, Some(50.0), 100.0, true, false, None),
    (
        "Easy",
        Some(70.0),
        4.0,
        Some(7.0),
        25.0,
        true,
        true,
        Some("Normal"),
    ),
];

/// `g_gaugeOptionObj`.
const SURVIVAL: [&str; 6] = [
    "Original",
    "Heavy",
    "NoRecovery",
    "SuddenDeath",
    "Practice",
    "Light",
];
const BORDER: [&str; 4] = ["Normal", "Hard", "SuddenDeath", "Easy"];

fn def(name: &str) -> Option<&'static Def> {
    DEFS.iter().find(|d| d.0 == name)
}

/// What a chart says about its rules, resolved for play.
#[derive(Clone, Debug, PartialEq)]
pub struct ChartRules {
    /// Gauges in menu order; the first is the default.
    pub gauges: Vec<GaugeDef>,
    pub max_life: f64,
    /// `frzStartjdgUse`: hold starts are judged like arrows.
    pub frz_start_judged: bool,
    /// `frzAttempt`: frames a hold may be released in all.
    pub frz_attempt: u32,
    /// `excessiveJdgUse`: Excessive starts on.
    pub excessive: bool,
}

/// Note counts the gauge expressions may use (`arrow[]`, `frz[]`).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub arrows: u32,
    pub holds: u32,
}

impl Counts {
    pub fn of(chart: &Chart) -> Counts {
        let mut c = Counts::default();
        for n in &chart.notes {
            match n.kind {
                NoteKind::Tap | NoteKind::Lift => c.arrows += 1,
                NoteKind::HoldHead { .. } | NoteKind::RollHead { .. } => c.holds += 1,
                _ => {}
            }
        }
        c
    }
}

struct Headers<'a> {
    chart: Option<&'a DanoniChart>,
    /// The chart whose entries are read (`scoreId`).
    index: usize,
}

impl<'a> Headers<'a> {
    fn get(&self, key: &str) -> Option<&'a str> {
        self.chart?
            .headers
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .filter(|v| !v.trim().is_empty())
    }

    fn index(&self) -> usize {
        self.index
    }

    /// `gauge{name}{n}` (this chart's own list) or `gauge{name}`, one
    /// `$`-separated entry per chart, falling back to the first
    /// (`getGaugeSetting`, `dosConverter.js` 2105–2129).
    fn gauge(&self, name: &str) -> Option<Vec<String>> {
        let i = self.index();
        let list = self
            .get(&format!("gauge{name}{}", i + 1))
            .or_else(|| self.get(&format!("gauge{name}")))?;
        let entries = dos::split_lf2(list);
        let entry = entries.get(i).or(entries.first())?;
        Some(entry.split(',').map(|s| s.trim().to_string()).collect())
    }

    /// `customGauge{n}` (`customGauge1` for the first chart) or `customGauge`.
    fn custom(&self) -> Option<&'a str> {
        self.get(&format!("customGauge{}", self.index() + 1))
            .or_else(|| self.get("customGauge"))
    }

    /// Whether any `gauge…` header gives this chart gauge settings
    /// (danoniplus then keeps a selection of its own for the chart).
    fn any_gauge(&self) -> bool {
        let own = format!("{}", self.index() + 1);
        self.chart.is_some_and(|c| {
            c.headers.iter().any(|(k, v)| {
                !v.trim().is_empty()
                    && k.strip_prefix("gauge").is_some_and(|name| {
                        !name.ends_with(|ch: char| ch.is_ascii_digit()) || name.ends_with(&own)
                    })
            })
        })
    }

    fn flag(&self, key: &str) -> Option<bool> {
        self.get(key).map(|v| v.trim().eq_ignore_ascii_case("true"))
    }
}

/// A gauge value: a number or an arithmetic expression over `arrow[]`,
/// `frz[]`, `all[]` and `maxlife[]` (`g_escapeStr.gaugeParamName`), braces
/// allowed; `None` when empty or not a number.
fn calc(value: &str, counts: Counts, max_life: f64) -> Option<f64> {
    let s = value
        .replace("arrow[]", &counts.arrows.to_string())
        .replace("frz[]", &counts.holds.to_string())
        .replace("all[]", &(counts.arrows + counts.holds).to_string())
        .replace("maxlife[]", &max_life.to_string())
        .replace(['{', '}'], "");
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    s.parse::<f64>()
        .ok()
        .or_else(|| expr::eval(s))
        .filter(|v| v.is_finite())
}

/// `x` (survival) or a percentage.
fn border(value: &str, counts: Counts, max_life: f64) -> Option<Option<f64>> {
    if value.trim() == "x" {
        Some(None)
    } else {
        calc(value, counts, max_life).map(Some)
    }
}

impl ChartRules {
    /// Resolves the rules of a Dancing☆Onigiri chart; any other chart gets
    /// danoniplus's defaults (difData `x,6,40,25`).
    pub fn of(chart: Option<&DanoniChart>, counts: Counts) -> ChartRules {
        let own = Headers {
            chart,
            index: chart.map_or(0, |c| c.index),
        };
        // A chart without a gauge selection of its own uses the first
        // chart's (`g_gaugeSelObj[scoreId] || g_gaugeSelObj[0]`,
        // `resolveGaugeValues`).
        let h = if own.index > 0 && own.custom().is_none() && !own.any_gauge() {
            Headers { chart, index: 0 }
        } else {
            Headers {
                chart,
                index: own.index,
            }
        };
        let max_life = h
            .get("maxLifeVal")
            .and_then(dos::parse_float)
            .filter(|v| *v > 0.0 && v.is_finite())
            .unwrap_or(DEFAULT_MAX_LIFE);
        let dif = chart.map_or_else(
            || ["x", "6", "40", "25"].map(String::from),
            |c| c.gauge.clone(),
        );

        // Menu order: the chart's custom list, else by its difData border.
        let mut variable_override: Vec<(String, bool)> = Vec::new();
        let custom = h.custom().map(str::trim);
        let order: Vec<String> = match custom {
            Some("survival") => SURVIVAL.map(String::from).to_vec(),
            Some("border") => BORDER.map(String::from).to_vec(),
            Some(c) if c != "customDefault" => c
                .split(',')
                .filter_map(|g| {
                    let mut parts = g.split("::");
                    let name = parts.next()?.trim();
                    if name.is_empty() {
                        return None;
                    }
                    if let Some(flag) = parts.next() {
                        variable_override.push((name.to_string(), flag.trim() == "V"));
                    }
                    Some(name.to_string())
                })
                .collect(),
            _ => Vec::new(),
        };
        let is_custom = !order.is_empty();
        let order = if is_custom {
            order
        } else if dif[0].trim() == "x" {
            SURVIVAL.map(String::from).to_vec()
        } else {
            BORDER.map(String::from).to_vec()
        };
        // difData applies to the first overridable family only when the
        // list is custom (Light and Easy count as Original and Normal).
        let root = |name: &str| def(name).and_then(|d| d.7).unwrap_or(name).to_string();
        let first_root = order
            .iter()
            .find(|n| def(n).is_some_and(|d| d.6))
            .map(|n| root(n));

        let gauges = order
            .iter()
            .filter_map(|name| {
                let d = def(name);
                let mut g = d.map(|d| GaugeDef {
                    name: name.clone(),
                    border: d.1,
                    recovery: d.2,
                    damage: d.3.unwrap_or(max_life),
                    init: d.4,
                    variable: d.5,
                });
                if let Some(d) = d
                    && d.6
                    && (!is_custom || first_root.as_deref() == Some(root(name).as_str()))
                    && let Some(g) = g.as_mut()
                {
                    let mul = if d.7.is_some() { 2.0 } else { 1.0 };
                    g.border = border(&dif[0], counts, max_life).unwrap_or(g.border);
                    g.recovery = calc(&dif[1], counts, max_life).unwrap_or(g.recovery) * mul;
                    g.damage = calc(&dif[2], counts, max_life).unwrap_or(g.damage);
                    g.init = calc(&dif[3], counts, max_life).unwrap_or(g.init);
                }
                // gauge{Name} headers win; a gauge without a definition
                // needs one.
                if let Some(o) = h.gauge(name)
                    && o.len() >= 2
                {
                    let field = |k: usize| o.get(k).map(String::as_str).unwrap_or("");
                    let base = g.clone().unwrap_or(GaugeDef {
                        name: name.clone(),
                        border: None,
                        recovery: 0.0,
                        damage: 0.0,
                        init: 100.0,
                        variable: false,
                    });
                    let mut v = base.clone();
                    if !field(0).is_empty() {
                        v.border = border(field(0), counts, max_life).unwrap_or(base.border);
                    }
                    v.recovery = calc(field(1), counts, max_life).unwrap_or(base.recovery);
                    v.damage = calc(field(2), counts, max_life).unwrap_or(base.damage);
                    v.init = calc(field(3), counts, max_life).unwrap_or(base.init);
                    g = Some(v);
                }
                let mut g = g?;
                if let Some((_, v)) = variable_override.iter().find(|(n, _)| n == name) {
                    g.variable = *v;
                }
                Some(g)
            })
            .collect();

        ChartRules {
            gauges,
            max_life,
            frz_start_judged: h.flag("frzStartjdgUse").unwrap_or(false),
            frz_attempt: h
                .get("frzAttempt")
                .and_then(dos::parse_int)
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(DEFAULT_FRZ_ATTEMPT),
            excessive: excessive_default(&own),
        }
    }

    /// The gauge to play: the chosen one when the chart offers it, else the
    /// chart's first.
    pub fn gauge(&self, chosen: Option<&str>) -> GaugeDef {
        chosen
            .and_then(|c| self.gauges.iter().find(|g| g.name == c))
            .or(self.gauges.first())
            .cloned()
            .unwrap_or_else(|| GaugeDef {
                name: "Original".into(),
                border: None,
                recovery: 6.0,
                damage: 40.0,
                init: 25.0,
                variable: false,
            })
    }
}

/// `excessiveJdgUse` forces Excessive on for every chart; otherwise the
/// chart's entry of `excessiveUse` (`enabled,startsOn` per chart),
/// falling back to the first (`dosConverter.js` 1630–1655).
fn excessive_default(h: &Headers) -> bool {
    if h.flag("excessiveJdgUse") == Some(true) {
        return true;
    }
    let Some(list) = h.get("excessiveUse") else {
        return false;
    };
    let entries = dos::split_lf2(list);
    let entry = entries.get(h.index()).or(entries.first());
    entry
        .and_then(|e| e.split(',').nth(1))
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("on"))
}

/// danoniplus's life gauge.
///
/// Recovery on Ii, Shakin and Kita; damage on Shobon, Uwan and Iknai,
/// a quarter of it on Excessive; Matari does neither (`judgeRecovery`,
/// `judgeDamage`, `judgeMatari`). Life runs from 0 to `max_life`; with a
/// border the play is checked at the end, without one (or with border 0)
/// it stops at zero (`mainWindow.js` 1617–1634). Mines, which only other
/// formats have, cost a miss's damage.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DanoniGauge {
    pub def: GaugeDef,
    pub max_life: f64,
    /// Whether hold starts are judged (they then count in `allArrows`).
    pub frz_start_judged: bool,
    #[serde(skip)]
    life: f64,
    #[serde(skip)]
    recovery: f64,
    #[serde(skip)]
    damage: f64,
}

impl DanoniGauge {
    pub fn new(def: GaugeDef, max_life: f64, frz_start_judged: bool) -> DanoniGauge {
        let mut g = DanoniGauge {
            def,
            max_life,
            frz_start_judged,
            life: 0.0,
            recovery: 0.0,
            damage: 0.0,
        };
        g.begin(&ScoreCtx::default());
        g
    }

    fn border_life(&self) -> f64 {
        self.max_life * self.def.border.unwrap_or(0.0) / 100.0
    }

    fn change(&mut self, delta: f64) {
        self.life = (self.life + delta).clamp(0.0, self.max_life);
    }
}

impl GaugeRules for DanoniGauge {
    fn begin(&mut self, ctx: &ScoreCtx) {
        // `floor(init × 100) / 100` (dataLoader.js 2446).
        self.life = ((self.max_life * self.def.init / 100.0) * 100.0).floor() / 100.0;
        self.life = self.life.clamp(0.0, self.max_life);
        let all = full_arrows(ctx);
        let scale = |v: f64| {
            if self.def.variable {
                if all == 0 {
                    0.0
                } else {
                    (v * self.max_life / f64::from(all)).min(self.max_life)
                }
            } else {
                v
            }
        };
        self.recovery = scale(self.def.recovery);
        self.damage = scale(self.def.damage);
    }

    fn on_event(&mut self, ev: &JudgeEvent, _ctx: &ScoreCtx) {
        match ev.kind {
            JudgeEventKind::Tap(Judgement::W1 | Judgement::W2, _) | JudgeEventKind::Held => {
                self.change(self.recovery)
            }
            JudgeEventKind::Tap(Judgement::W3, _) => {}
            JudgeEventKind::Tap(_, _) | JudgeEventKind::LetGo | JudgeEventKind::HitMine => {
                self.change(-self.damage)
            }
            JudgeEventKind::Boo => self.change(-self.damage * EXCESSIVE_FACTOR),
        }
    }

    fn life(&self) -> f32 {
        if self.max_life <= 0.0 {
            0.0
        } else {
            (self.life / self.max_life) as f32
        }
    }

    fn failed(&self) -> bool {
        if self.border_life() <= 0.0 {
            self.life <= 0.0
        } else {
            self.life < self.border_life()
        }
    }

    fn fails_at_end(&self, _min_life: f32) -> bool {
        // In life units, as danoniplus compares (`lifeVal < lifeBorder`).
        self.life < self.border_life()
    }

    fn danger(&self) -> bool {
        if self.border_life() > 0.0 {
            self.life < self.border_life()
        } else {
            self.life < self.max_life * 0.25
        }
    }

    fn fresh(&self) -> Box<dyn GaugeRules> {
        Box::new(DanoniGauge::new(
            self.def.clone(),
            self.max_life,
            self.frz_start_judged,
        ))
    }
}

/// `g_fullArrows`: every judged tap event plus one per hold outcome
/// (`dataLoader.js` 446–456). The player's `steps` already count hold
/// starts only when they are judged.
fn full_arrows(ctx: &ScoreCtx) -> u32 {
    ctx.steps + ctx.holds
}

/// danoniplus's score, `(8·Ii + 4·Shakin + 2·Matari + 8·Kita +
/// 2·maxCombo + 2·freezeMaxCombo) / (10·fullArrows) × 1,000,000` rounded
/// (`g_pointAllocation`, `result.js` 89–93). Hold outcomes keep their own
/// combo: Kita adds to it, Iknai resets it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DanoniScore {
    #[serde(skip)]
    points: u64,
    #[serde(skip)]
    max_combo: u32,
    #[serde(skip)]
    freeze_combo: u32,
    #[serde(skip)]
    freeze_max: u32,
    #[serde(skip)]
    ctx: ScoreCtx,
}

impl DanoniScore {
    pub fn new() -> DanoniScore {
        DanoniScore::default()
    }
}

impl ScoreRules for DanoniScore {
    fn begin(&mut self, ctx: &ScoreCtx) {
        *self = DanoniScore {
            ctx: *ctx,
            ..DanoniScore::default()
        };
    }

    fn on_event(&mut self, ev: &JudgeEvent, ctx: &ScoreCtx) {
        self.ctx = *ctx;
        self.max_combo = self.max_combo.max(ctx.combo);
        match ev.kind {
            JudgeEventKind::Tap(Judgement::W1, _) => self.points += 8,
            JudgeEventKind::Tap(Judgement::W2, _) => self.points += 4,
            JudgeEventKind::Tap(Judgement::W3, _) => self.points += 2,
            JudgeEventKind::Held => {
                self.points += 8;
                self.freeze_combo += 1;
                self.freeze_max = self.freeze_max.max(self.freeze_combo);
            }
            JudgeEventKind::LetGo => self.freeze_combo = 0,
            _ => {}
        }
    }

    fn view(&self) -> ScoreView {
        let all = u64::from(full_arrows(&self.ctx)) * 10;
        let total = self.points + 2 * u64::from(self.max_combo) + 2 * u64::from(self.freeze_max);
        let money = if all == 0 {
            0
        } else {
            // round(total / all × 1e6), half away from zero like Math.round
            // for positive values.
            (total * 2_000_000 + all) / (2 * all)
        };
        ScoreView {
            money: Some(money.min(1_000_000)),
            percent: None,
            grade_percent: None,
            ex: None,
            max_ex: None,
        }
    }

    fn finish(&mut self) {}

    fn fresh(&self) -> Box<dyn ScoreRules> {
        Box::new(DanoniScore::new())
    }
}

/// Ranks by score (`g_rankObj`, `danoni_constants.js` 1194–1209): AP for
/// only Ii and Kita, PF for nothing below Shakin, F when failed.
fn grades() -> GradeTable {
    GradeTable {
        basis: GradeBasis::Money,
        tiers: [
            ("SS", 970_000.0),
            ("S", 900_000.0),
            ("SA", 850_000.0),
            ("AAA", 800_000.0),
            ("AA", 750_000.0),
            ("A", 700_000.0),
            ("B", 650_000.0),
            ("C", 600_000.0),
            ("D", 0.0),
        ]
        .iter()
        .map(|(n, m)| (n.to_string(), *m))
        .collect(),
        all_w1: Some("AP".into()),
        all_w2: Some("PF".into()),
        failed: Some("F".into()),
    }
}

/// The Dancing☆Onigiri ruleset for `chart` (any chart: one from another
/// format gets danoniplus's defaults).
pub fn ruleset(chart: Option<&Chart>, options: &DanoniOptions) -> Ruleset {
    let counts = chart.map(Counts::of).unwrap_or_default();
    let rules = ChartRules::of(chart.and_then(|c| c.danoni.as_ref()), counts);
    let range = options.range;
    let [ii, shakin, matari, shobon, uwan] = range.arrow();
    let [kita, sfsf, _] = range.freeze();
    let gauge = rules.gauge(options.gauge.as_deref());
    let fail = match gauge.border {
        Some(b) if b > 0.0 => FailPolicy::EndOfSong {
            min_life: (b / 100.0) as f32,
        },
        _ => FailPolicy::Immediate,
    };
    let excessive = options.excessive.unwrap_or(rules.excessive);
    Ruleset {
        id: "danoni".into(),
        name: "Dancing☆Onigiri".into(),
        approximate: false,
        judge: JudgeTable {
            tiers: [
                Some(frames(ii)),
                Some(frames(shakin)),
                Some(frames(matari)),
                Some(frames(shobon)),
                None,
            ],
            scale: 1.0,
            add: 0.0,
            // Rolls and mines come only from other formats; they keep
            // StepMania's windows.
            hold_window: 0.25,
            roll_window: 0.5,
            mine_window: 0.09,
            w0: None,
            empty_press: if excessive {
                EmptyPress::ExcessiveEarly {
                    lo: f64::from(shobon) * FRAME,
                    hi: f64::from(uwan) * FRAME,
                    factor: EXCESSIVE_FACTOR,
                }
            } else {
                EmptyPress::Ignore
            },
            judge_offset: 0.0,
            hold: HoldRules::ReleaseBudget {
                budget: f64::from(rules.frz_attempt) * FRAME,
            },
            hold_start: HoldStart {
                judged: rules.frz_start_judged,
                accept: Some(frames(sfsf)),
            },
            // `judgeNextFunc` (mainWindow.js 797–847): the next arrow
            // within Shakin, the previous more than Ii late; for a hold,
            // the next note within `sfsf`, the hold more than Kita late.
            supersede: Some(Supersede {
                next_within: f64::from(shakin) * FRAME,
                prev_late: f64::from(ii + 1) * FRAME,
                hold_next_within: f64::from(sfsf) * FRAME,
                hold_prev_late: f64::from(kita + 1) * FRAME,
            }),
            // `justFrames` (default 1): fast or slow beyond one frame
            // (`displayDiff`, mainWindow.js).
            just: Some(frames(1)),
        },
        combo: ComboRules {
            continue_min: Judgement::W2,
            per_row: false,
            held_increments: false,
            let_go_breaks: false,
            mine_breaks: false,
            neutral: Some(Judgement::W3),
            let_go_loses_full_combo: true,
        },
        score: Box::new(DanoniScore::new()),
        gauge: Box::new(DanoniGauge::new(
            gauge,
            rules.max_life,
            rules.frz_start_judged,
        )),
        fail,
        grades: grades(),
        names: JudgeNames::new(
            ["Perfect", "Great", "Good", "Bad", "—", "Miss"],
            "O.K.",
            "N.G.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_chart::Tick;

    fn chart(gauge: [&str; 4], headers: &[(&str, &str)], index: usize) -> DanoniChart {
        DanoniChart {
            init_speed: 3.5,
            gauge: gauge.map(String::from),
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            index,
            ..DanoniChart::default()
        }
    }

    const COUNTS: Counts = Counts {
        arrows: 90,
        holds: 10,
    };

    fn names(r: &ChartRules) -> Vec<&str> {
        r.gauges.iter().map(|g| g.name.as_str()).collect()
    }

    #[test]
    fn default_survival_list_and_light_doubles_recovery() {
        let c = chart(["x", "3", "30", "50"], &[], 0);
        let r = ChartRules::of(Some(&c), COUNTS);
        assert_eq!(names(&r), SURVIVAL.to_vec());
        let original = r.gauge(None);
        assert_eq!(
            (original.recovery, original.damage, original.init),
            (3.0, 30.0, 50.0)
        );
        let light = r.gauge(Some("Light"));
        assert_eq!(light.recovery, 6.0);
        // Not overridable: keeps its definition.
        assert_eq!(r.gauge(Some("Heavy")).recovery, 2.0);
        assert_eq!(r.gauge(Some("SuddenDeath")).damage, 1000.0);
        // A gauge the chart does not offer plays the first.
        assert_eq!(r.gauge(Some("Normal")).name, "Original");
    }

    #[test]
    fn border_charts_and_numbered_headers() {
        let c = chart(
            ["80", "2", "7", "25"],
            &[
                ("gaugeHard", "0,1,50,100$0,2,arrow[]/10,100"),
                ("gaugeNormal2", "75,3,7,25"),
                ("maxLifeVal", "500"),
            ],
            1,
        );
        let r = ChartRules::of(Some(&c), COUNTS);
        assert_eq!(names(&r), BORDER.to_vec());
        assert_eq!(r.max_life, 500.0);
        let normal = r.gauge(None);
        assert_eq!(normal.border, Some(75.0));
        assert_eq!(normal.recovery, 3.0);
        let hard = r.gauge(Some("Hard"));
        assert_eq!((hard.recovery, hard.damage), (2.0, 9.0));
        // Easy takes difData with doubled recovery.
        let easy = r.gauge(Some("Easy"));
        assert_eq!((easy.border, easy.recovery), (Some(80.0), 4.0));
        assert_eq!(r.gauge(Some("SuddenDeath")).damage, 500.0);
    }

    #[test]
    fn custom_lists() {
        let c = chart(
            ["x", "6", "40", "25"],
            &[
                ("customGauge", "Escape::V::Escape!,Normal::F"),
                ("gaugeEscape", "0,1,100,100"),
            ],
            0,
        );
        let r = ChartRules::of(Some(&c), COUNTS);
        assert_eq!(names(&r), vec!["Escape", "Normal"]);
        let esc = r.gauge(None);
        assert!(esc.variable);
        assert_eq!((esc.border, esc.damage), (Some(0.0), 100.0));
        // Normal is the first overridable family: difData applies, and
        // `::F` makes it fixed.
        let normal = r.gauge(Some("Normal"));
        assert_eq!(normal.border, None);
        assert_eq!(normal.recovery, 6.0);
        assert!(!normal.variable);

        let c = chart(["x", "6", "40", "25"], &[("customGauge2", "border")], 1);
        assert_eq!(names(&ChartRules::of(Some(&c), COUNTS)), BORDER.to_vec());
        // A chart with no selection of its own takes the first chart's.
        let c = chart(
            ["x", "6", "40", "25"],
            &[("customGauge1", "Hard::V,Easy")],
            2,
        );
        assert_eq!(
            names(&ChartRules::of(Some(&c), COUNTS)),
            vec!["Hard", "Easy"]
        );
        let c = chart(
            ["x", "6", "40", "25"],
            &[
                ("customGauge1", "Hard::V,Easy"),
                ("gaugeHard3", "0,1,9,100"),
            ],
            2,
        );
        assert_eq!(names(&ChartRules::of(Some(&c), COUNTS)), SURVIVAL.to_vec());
        // A name with no definition and no header is left out.
        let c = chart(
            ["x", "6", "40", "25"],
            &[("customGauge", "Mystery,Original")],
            0,
        );
        assert_eq!(names(&ChartRules::of(Some(&c), COUNTS)), vec!["Original"]);
    }

    #[test]
    fn hold_and_excessive_headers() {
        let c = chart(
            ["x", "6", "40", "25"],
            &[
                ("frzStartjdgUse", "true"),
                ("frzAttempt", "8"),
                ("excessiveUse", "true,OFF$true,ON"),
            ],
            1,
        );
        let r = ChartRules::of(Some(&c), COUNTS);
        assert!(r.frz_start_judged && r.excessive);
        assert_eq!(r.frz_attempt, 8);
        let c = chart(["x", "6", "40", "25"], &[("excessiveJdgUse", "true")], 0);
        assert!(ChartRules::of(Some(&c), COUNTS).excessive);
        assert!(!ChartRules::of(None, COUNTS).excessive);
    }

    fn ev(kind: JudgeEventKind) -> JudgeEvent {
        JudgeEvent {
            note_index: None,
            lane: 0,
            tick: Tick::ZERO,
            kind,
            song_time: 0.0,
        }
    }

    #[test]
    fn variable_gauge_scales_by_note_count() {
        let def = GaugeDef {
            name: "Normal".into(),
            border: Some(70.0),
            recovery: 2.0,
            damage: 7.0,
            init: 25.0,
            variable: true,
        };
        let ctx = ScoreCtx {
            steps: 90,
            holds: 10,
            mines: 0,
            combo: 0,
        };
        let mut g = DanoniGauge::new(def, 1000.0, false);
        g.begin(&ctx);
        assert!((g.life() - 0.25).abs() < 1e-6);
        // Recovery 2 × 1000 / 100 = 20.
        g.on_event(&ev(JudgeEventKind::Tap(Judgement::W1, 0.0)), &ctx);
        assert!((g.life() - 0.27).abs() < 1e-6);
        g.on_event(&ev(JudgeEventKind::Tap(Judgement::W3, 0.0)), &ctx);
        assert!((g.life() - 0.27).abs() < 1e-6);
        // Damage 70; Excessive a quarter of it.
        g.on_event(&ev(JudgeEventKind::LetGo), &ctx);
        assert!((g.life() - 0.20).abs() < 1e-6);
        g.on_event(&ev(JudgeEventKind::Boo), &ctx);
        assert!((g.life() - 0.1825).abs() < 1e-6);
        assert!(g.failed() && g.danger());
    }

    #[test]
    fn survival_gauge_fails_at_zero() {
        let def = GaugeDef {
            name: "SuddenDeath".into(),
            border: None,
            recovery: 0.0,
            damage: 1000.0,
            init: 100.0,
            variable: false,
        };
        let mut g = DanoniGauge::new(def, 1000.0, false);
        let ctx = ScoreCtx::default();
        g.begin(&ctx);
        g.on_event(&ev(JudgeEventKind::Tap(Judgement::W3, 0.0)), &ctx);
        assert!(!g.failed());
        g.on_event(&ev(JudgeEventKind::Tap(Judgement::W4, 0.0)), &ctx);
        assert!(g.failed());
    }

    #[test]
    fn score_formula() {
        // 8 arrows, 2 holds, starts unjudged: fullArrows 10.
        let mut s = DanoniScore::new();
        let mut ctx = ScoreCtx {
            steps: 8,
            holds: 2,
            mines: 0,
            combo: 0,
        };
        s.begin(&ctx);
        for _ in 0..8 {
            ctx.combo += 1;
            s.on_event(&ev(JudgeEventKind::Tap(Judgement::W1, 0.0)), &ctx);
        }
        s.on_event(&ev(JudgeEventKind::Held), &ctx);
        s.on_event(&ev(JudgeEventKind::Held), &ctx);
        assert_eq!(s.view().money, Some(1_000_000));

        // 6 Ii, 1 Shakin, 1 Matari (combo 7 at most), 1 Kita, 1 Iknai:
        // (48 + 4 + 2 + 8 + 14 + 2) / 100 = 0.78.
        let mut s = DanoniScore::new();
        ctx.combo = 0;
        s.begin(&ctx);
        for j in [
            Judgement::W1,
            Judgement::W1,
            Judgement::W1,
            Judgement::W2,
            Judgement::W1,
            Judgement::W1,
            Judgement::W1,
        ] {
            ctx.combo += 1;
            s.on_event(&ev(JudgeEventKind::Tap(j, 0.0)), &ctx);
        }
        s.on_event(&ev(JudgeEventKind::Tap(Judgement::W3, 0.0)), &ctx);
        s.on_event(&ev(JudgeEventKind::Held), &ctx);
        s.on_event(&ev(JudgeEventKind::LetGo), &ctx);
        assert_eq!(s.view().money, Some(780_000));
    }

    #[test]
    fn windows_are_frame_buckets() {
        let r = ruleset(None, &DanoniOptions::default());
        let t = &r.judge;
        // Ii: [−2, +3) frames.
        assert_eq!(t.judge(-2.0 * FRAME), Some(Judgement::W1));
        assert_eq!(t.judge(-2.01 * FRAME), Some(Judgement::W2));
        assert_eq!(t.judge(2.99 * FRAME), Some(Judgement::W1));
        assert_eq!(t.judge(4.99 * FRAME), Some(Judgement::W2));
        assert_eq!(t.judge(8.99 * FRAME), Some(Judgement::W4));
        assert_eq!(t.judge(-8.01 * FRAME), None);
        assert!((t.miss_after() - 9.0 * FRAME).abs() < 1e-12);
        let hard = ruleset(
            None,
            &DanoniOptions {
                range: JudgeRange::ExHard,
                ..Default::default()
            },
        );
        assert_eq!(hard.judge.judge(1.5 * FRAME), Some(Judgement::W1));
        assert_eq!(hard.judge.judge(2.5 * FRAME), Some(Judgement::W2));
        assert_eq!(hard.judge.hold_start.accept, Some(frames(2)));
    }
}
