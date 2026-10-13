//! Passive native capture projections. Work code executes only in the worker.
mod graph;
mod job;
pub(crate) use job::checked_capture;
mod movie;
mod observed;
mod scheduled;
use crate::{Error, Result, model::Mode};
use graph::*;
pub use graph::{Graph, numeric, text};
pub use job::{
    CaptureBindings, OptionalColorEvidence, ProjectionRequest, prepare_projection,
    review_optional_colors,
};
pub use observed::{
    MotionSelection, movie_audio_origin, movie_observation, relative_chart,
    validate_clock_agreement,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn invalid(message: impl Into<String>) -> Error {
    Error::new("invalid-native-projection", message)
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Family {
    #[serde(rename = "child-timeline-5")]
    ChildTimeline5,
    #[serde(rename = "dac-7")]
    Dac7,
    #[serde(rename = "dac-5")]
    Dac5,
    #[serde(rename = "dac-fixed-5")]
    DacFixed5,
    ScheduledCounter,
    ScheduledConstant,
    ParaflaCounter,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectionOptions {
    pub family: Family,
    pub mode: Mode,
    #[serde(default)]
    pub source_prefix: String,
    pub source_suffix: Option<String>,
    #[serde(default)]
    pub review_missing_decorative_sprites: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Projection {
    pub prepared_iteration: u64,
    pub chart: Value,
}
const FIVE: [(&str, &str); 5] = [
    ("left", "left"),
    ("down", "down"),
    ("up", "up"),
    ("right", "right"),
    ("space", "space"),
];
const SEVEN: [(&str, &str); 7] = [
    ("left", "le"),
    ("leftdia", "led"),
    ("down", "do"),
    ("space", "oni"),
    ("up", "up"),
    ("rightdia", "rid"),
    ("right", "ri"),
];
type LaneDefinitions = &'static [(&'static str, &'static str)];
fn definitions(mode: Mode) -> Result<(LaneDefinitions, Vec<usize>)> {
    match mode {
        Mode::Five => Ok((&FIVE, vec![0, 2, 4, 6, 3])),
        Mode::Seven => Ok((&SEVEN, (0..7).collect())),
        _ => Err(invalid("Native family requires explicit 5 or 7 keys")),
    }
}
fn items(capture: &Value) -> Result<&Vec<Value>> {
    let items = array(&capture["captures"])?;
    if items.is_empty() || items.len() > 4096 {
        return Err(invalid("Need bounded nonempty native captures"));
    }
    let mut last = None;
    for item in items {
        let frame = iteration(item)?;
        if last.is_some_and(|l| frame <= l) {
            return Err(invalid("Capture iterations must increase"));
        }
        last = Some(frame);
    }
    Ok(items)
}
fn iteration(item: &Value) -> Result<u64> {
    item["frame"]
        .as_u64()
        .filter(|v| *v <= 1000000)
        .ok_or_else(|| invalid("Invalid worker iteration"))
}
pub(crate) fn audit_ok(capture: &Value, reviewed: &[Value]) -> Result<()> {
    let audit = &capture["audit"];
    if capture["avm1Halted"] != false
        || audit["truncated"] != false
        || audit["droppedEntries"] != 0
        || audit["truncatedDetails"] != 0
    {
        return Err(invalid("Incomplete or halted native audit"));
    }
    let entries = array(&audit["entries"])?;
    if entries.len() > 100000 {
        return Err(invalid("Native audit limit"));
    }
    let reviewed_errors = entries
        .iter()
        .filter(|e| reviewed.contains(e) && e["kind"] == "runtime-error")
        .count();
    if audit["runtimeErrors"].as_u64() != Some(reviewed_errors as u64) {
        return Err(invalid("Unreviewed native runtime errors"));
    }
    let resources = capture["source"]["config"]
        .get("resources")
        .map(array)
        .transpose()?;
    let mut urls = std::collections::BTreeSet::new();
    if let Some(resources) = resources {
        if resources.len() > 4096 {
            return Err(invalid("Native resource declaration limit"));
        }
        for resource in resources {
            urls.insert(
                resource["url"]
                    .as_str()
                    .ok_or_else(|| invalid("Invalid native resource URL"))?,
            );
        }
    }
    for entry in entries {
        if reviewed.contains(entry) || entry["kind"] == "runtime-warning" {
            continue;
        }
        if entry["kind"] == "cache-hit"
            && entry["detail"]
                .as_str()
                .is_some_and(|url| urls.contains(url))
        {
            continue;
        }
        return Err(invalid("Unexpected native request or audit entry"));
    }
    Ok(())
}
fn holds(flat: &[f64], strict: bool) -> Result<Vec<[f64; 2]>> {
    if !flat.len().is_multiple_of(2) {
        return Err(invalid("Unpaired hold endpoints"));
    }
    flat.as_chunks::<2>()
        .0
        .iter()
        .map(|p| {
            if p[1] < p[0] || (strict && p[1] == p[0]) {
                Err(invalid("Invalid hold pair"))
            } else {
                Ok([p[0], p[1]])
            }
        })
        .collect()
}
fn events(flat: &[f64]) -> Result<Value> {
    if !flat.len().is_multiple_of(2) {
        return Err(invalid("Unpaired native events"));
    }
    Ok(json!(
        flat.as_chunks::<2>()
            .0
            .iter()
            .map(|p| json!({"nativeFrame":p[0],"multiplier":p[1]}))
            .collect::<Vec<_>>()
    ))
}
fn lane(name: &str, taps: Vec<f64>, holds: Vec<[f64; 2]>, order: bool) -> Value {
    let issues = taps
        .windows(2)
        .enumerate()
        .filter(|(_, p)| p[1] <= p[0])
        .map(|(j, p)| json!({"index":j+1,"frame":p[1],"previousFrame":p[0]}))
        .collect::<Vec<_>>();
    let mut value = json!({"name":name,"nativeFrames":taps,"nativeHoldFrames":holds});
    if order {
        value["sourceOrderIssues"] = json!(issues);
    }
    value
}
fn equal_fields(a: &Value, b: &Value, fields: &[&str]) -> Result<()> {
    if fields.iter().any(|f| a[*f] != b[*f]) {
        return Err(invalid(
            "Initialized native data changed before consumption",
        ));
    }
    Ok(())
}
/// Structural selection, never a SWF hash guess. Projection alone does not prove
/// run provenance, movement or decoded audio alignment; prepare_projection binds
/// saved run/config bytes and the later extraction stage must add those proofs.
pub fn project_capture(
    capture: &Value,
    options: &ProjectionOptions,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Projection> {
    checkpoint()?;
    if capture["swfHeaderFps"].as_f64() != Some(60.0) {
        return Err(invalid("Need a verified 60 Hz movie clock"));
    }
    items(capture)?;
    definitions(options.mode)?;
    if !matches!(options.source_prefix.as_str(), "" | "n") {
        return Err(invalid("Unknown source prefix"));
    }
    if options.source_suffix.as_ref().is_some_and(|s| {
        s.chars().count() > 32 || s.chars().any(|c| !c.is_alphanumeric() && c != '_')
    }) || (options.source_suffix.is_some() && options.family != Family::ChildTimeline5)
    {
        return Err(invalid("Unsupported source selection"));
    }
    if options.family != Family::ChildTimeline5 && !options.source_prefix.is_empty() {
        return Err(invalid("Prefix only applies to child timelines"));
    }
    if options.review_missing_decorative_sprites
        && !matches!(
            options.family,
            Family::ScheduledCounter | Family::ParaflaCounter
        )
    {
        return Err(invalid(
            "Decorative exception does not apply to this family",
        ));
    }
    match options.family {
        Family::ChildTimeline5 | Family::Dac7 => movie::movie_arrays(capture, options, checkpoint),
        Family::Dac5 | Family::DacFixed5 => movie::dac_five(capture, options, checkpoint),
        Family::ScheduledConstant => scheduled::constant(capture, options.mode, checkpoint),
        Family::ScheduledCounter | Family::ParaflaCounter => {
            scheduled::scheduled(capture, options, checkpoint)
        }
    }
}
