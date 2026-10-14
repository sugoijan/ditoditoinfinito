//! Reviewed native engine candidates plus observed menu/selector/motion plans.
//! A fingerprint proposes controls; source-bound runtime proofs authorize data.
mod driver;
mod identity;
mod play;
pub use crate::dependencies::{
    ProfileDependencyJob, execute_profile as execute_with_dependencies,
    execute_profile_job as execute_dependencies_job,
};
use crate::{
    Error, Result,
    extraction::{self, Family, Graph, MotionSelection, MotionSource, ProjectionOptions},
    model::Mode,
};
pub use driver::*;
pub use identity::{EngineProfile, identify};
pub use play::{early_hold_inputs, has_early_holds};
use serde::Serialize;
use serde_json::{Value, json};
fn invalid(message: impl Into<String>) -> Error {
    Error::new("invalid-native-profile", message)
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MenuRow {
    pub index: usize,
    pub label: String,
    pub key_label: String,
    pub mode: Option<Mode>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Menu {
    pub rows: Vec<MenuRow>,
    pub selected: usize,
}
fn prop<'a>(
    props: &std::collections::BTreeMap<String, &'a Value>,
    name: &str,
) -> Result<&'a Value> {
    props
        .get(name)
        .copied()
        .ok_or_else(|| invalid(format!("Missing menu property {name}")))
}
fn integer(v: &Value, max: usize) -> Result<usize> {
    let n = extraction::numeric(v)?;
    if n.fract() != 0. || !(0.0..=max as f64).contains(&n) {
        return Err(invalid("Invalid menu index/count"));
    }
    Ok(n as usize)
}
fn menu_at(profile: EngineProfile, item: &Value) -> Result<Menu> {
    let graph = Graph::new(&item["state"])?;
    if graph.root_frame()? != profile.menu_frame() {
        return Err(invalid(
            "Expected the identified profile's options-menu frame",
        ));
    }
    let root = graph.root()?;
    let count = integer(prop(&root, "difficulty")?, 16)?;
    let labels = graph.array(prop(&root, "diff_label")?)?;
    let keys = graph.array(prop(&root, "key_label")?)?;
    if count == 0 || labels.len() != count || keys.len() != count {
        return Err(invalid("Menu key/difficulty counts disagree"));
    }
    let mut rows = Vec::new();
    for (i, (label, key)) in labels.iter().zip(keys).enumerate() {
        let label = extraction::text(label)?;
        let key_label = extraction::text(key)?;
        if label.len() > 4096 || key_label.len() > 128 {
            return Err(invalid("Unbounded menu text"));
        }
        let mode = profile.mode(&key_label);
        rows.push(MenuRow {
            index: i,
            label,
            key_label,
            mode,
        });
    }
    let selected = integer(prop(&root, "dif_shift")?, count - 1)?;
    if extraction::text(prop(&root, "diffi")?)? != rows[selected].label
        || extraction::text(prop(&root, "keyLabel")?)? != rows[selected].key_label
    {
        return Err(invalid("Selected label/key does not match menu table"));
    }
    let suffix = prop(&root, "diffin")?;
    if (selected == 0 && extraction::text(suffix).as_deref() != Ok(""))
        || (selected > 0 && extraction::numeric(suffix)? != (selected + 1) as f64)
    {
        return Err(invalid("Menu suffix does not match selector"));
    }
    Ok(Menu { rows, selected })
}
/// Compatibility entry point for the original Higan-build menu contract.
pub fn read_menu(capture: &Value) -> Result<Menu> {
    EngineProfile::RekuConstantMenu.read_menu(capture)
}
impl EngineProfile {
    pub fn read_menu(self, capture: &Value) -> Result<Menu> {
        extraction::audit_ok(capture, &[])?;
        if capture["swfHeaderFps"].as_f64() != Some(60.) || capture["avm1Halted"] != false {
            return Err(invalid("Menu requires nonhalted 60 Hz capture"));
        }
        let captures = capture["captures"]
            .as_array()
            .filter(|a| a.len() >= 2 && a.len() <= 16)
            .ok_or_else(|| invalid("Need repeated bounded menu observations"))?;
        let first = menu_at(self, &captures[0])?;
        let mut last = None;
        for c in captures {
            let f = c["frame"]
                .as_u64()
                .ok_or_else(|| invalid("Missing menu iteration"))?;
            if !(1..=1200).contains(&f) || last.is_some_and(|n| n >= f) {
                return Err(invalid("Menu iterations must increase"));
            }
            last = Some(f);
            if menu_at(self, c)? != first {
                return Err(invalid("Menu changed across observations"));
            }
        }
        Ok(first)
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectorControls {
    pub events: Vec<Value>,
    pub start: u64,
    pub initialization_frames: Vec<u64>,
}
/// Compatibility entry point for the original Higan-build controls.
pub fn selector_controls(menu: &Menu, index: usize) -> Result<SelectorControls> {
    EngineProfile::RekuConstantMenu.selector_controls(menu, index)
}
impl EngineProfile {
    pub fn selector_controls(self, menu: &Menu, index: usize) -> Result<SelectorControls> {
        if menu.rows.is_empty()
            || menu.rows.len() > 16
            || menu.selected >= menu.rows.len()
            || index >= menu.rows.len()
        {
            return Err(invalid("Selector outside observed table"));
        }
        let clicks = (index + menu.rows.len() - menu.selected) % menu.rows.len();
        let mut events = vec![json!({"frame":40,"type":"click","x":160,"y":self.title_y()})];
        for step in 0..clicks {
            events.push(json!({"frame":43+2*step,"type":"click","x":40,"y":self.next_y()}));
        }
        let start = 50u64.max(46 + 2 * clicks as u64);
        events.push(json!({"frame":start,"type":"click","x":50,"y":230}));
        Ok(SelectorControls {
            events,
            start,
            initialization_frames: (start - 2..=start + 8).collect(),
        })
    }
}
pub fn options(mode: Mode) -> Result<ProjectionOptions> {
    if !matches!(mode, Mode::Five | Mode::Seven) {
        return Err(invalid("Profile supports explicit 5/7 labels only"));
    }
    Ok(ProjectionOptions {
        family: Family::ScheduledConstant,
        mode,
        source_prefix: String::new(),
        source_suffix: None,
        pinned_selection: None,
        review_missing_decorative_sprites: false,
    })
}
/// Propose tap/hold-head witnesses using observed travel and audio-clock origin.
/// Every proposal must subsequently pass movie_observation; these are no proof.
pub fn motion_candidates(
    chart: &Value,
    origin: &Value,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Vec<MotionSelection>> {
    let lanes = chart["lanes"]
        .as_array()
        .ok_or_else(|| invalid("Missing projected lanes"))?;
    let clips: &[&str] = match chart["mode"].as_str() {
        Some("5") => &["le", "do", "up", "ri", "oni"],
        Some("7") => &["le", "led", "do", "oni", "up", "rid", "ri"],
        _ => return Err(invalid("Unknown projected mode")),
    };
    if lanes.len() != clips.len() || chart["clockField"] != "frame_num" {
        return Err(invalid("Profile clock/lane layout differs"));
    }
    let travel = extraction::numeric(&chart["constantTravelFrames"])?;
    let offset =
        extraction::numeric(&origin["logicalFrame"])? - extraction::numeric(&origin["iteration"])?;
    if !(6.0..=12000.0).contains(&travel) || !offset.is_finite() {
        return Err(invalid("Unbounded travel/origin"));
    }
    let speeds = chart["speed"]
        .as_array()
        .ok_or_else(|| invalid("Missing speed events"))?;
    let mut candidates = Vec::new();
    let mut holds = Vec::new();
    let mut first_tap = f64::INFINITY;
    for lane in lanes {
        if let Some(raw) = lane["nativeFrames"].as_array().and_then(|a| a.first()) {
            first_tap = first_tap.min(extraction::numeric(raw)?);
        }
    }
    for fraction in [0.5, 0.25, 0.75] {
        for (index, (lane, clip)) in lanes.iter().zip(clips).enumerate() {
            checkpoint()?;
            for source in [MotionSource::Tap, MotionSource::HoldHead] {
                let raw = match source {
                    MotionSource::Tap => lane["nativeFrames"].as_array().and_then(|a| a.first()),
                    MotionSource::HoldHead => lane["nativeHoldFrames"]
                        .as_array()
                        .and_then(|a| a.first())
                        .and_then(|p| p.as_array())
                        .and_then(|p| p.first()),
                };
                let Some(raw) = raw else { continue };
                let raw = extraction::numeric(raw)?;
                let first = (raw - travel * fraction - offset).ceil();
                if !(1.0..=1195.0).contains(&first) {
                    continue;
                }
                let logical = first + offset;
                if logical <= raw - travel || logical + 5. >= raw {
                    continue;
                }
                let ratio = |at: f64| -> Result<f64> {
                    let mut ratio = 1.;
                    for e in speeds {
                        if extraction::numeric(&e["nativeFrame"])? <= at {
                            ratio = extraction::numeric(&e["multiplier"])?;
                        }
                    }
                    Ok(ratio)
                };
                let a = ratio(logical)?;
                if a <= 0.
                    || ratio(logical + 5.)? != a
                    || speeds.iter().any(|e| {
                        e["nativeFrame"]
                            .as_f64()
                            .is_some_and(|f| f > logical && f <= logical + 5.)
                    })
                {
                    continue;
                }
                let proposal = MotionSelection {
                    source,
                    note: format!(
                        "{}{}_s0",
                        if source == MotionSource::HoldHead {
                            "f"
                        } else {
                            ""
                        },
                        clip
                    ),
                    receptor: format!("{}_step", clip),
                    lane: index,
                    iterations: (first as u64..=first as u64 + 5).collect(),
                    target_y: None,
                    geometry_reference: Some("le_step".into()),
                };
                if source == MotionSource::Tap {
                    candidates.push(proposal);
                } else {
                    holds.push((raw, proposal));
                }
            }
        }
    }
    // Early holds can end a no-input run before the first tap appears. Prefer
    // those witnesses; otherwise preserve the established tap proposal order.
    holds.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut early = Vec::new();
    let mut late = Vec::new();
    for (raw, proposal) in holds {
        if raw < first_tap {
            early.push(proposal)
        } else {
            late.push(proposal)
        }
    }
    early.extend(candidates);
    early.extend(late);
    Ok(early)
}
