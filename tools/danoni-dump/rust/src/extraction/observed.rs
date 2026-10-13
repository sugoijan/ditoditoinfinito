use super::*;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MotionSelection {
    pub note: String,
    pub receptor: String,
    pub lane: usize,
    pub iterations: Vec<u64>,
    pub target_y: Option<f64>,
    pub geometry_reference: Option<String>,
}
pub(super) fn display_map(item: &Value) -> Result<BTreeMap<String, Option<&Value>>> {
    let mut result = BTreeMap::new();
    let displays = array(&item["state"]["displayTree"])?;
    if displays.len() > 512 {
        return Err(invalid("Native display limit"));
    }
    for d in displays {
        if !d["name"].is_null() && d["kind"] == "movie-clip" && d["visible"] == true {
            let name = text(&d["name"])?;
            if result.insert(name.clone(), Some(d)).is_some() {
                result.insert(name, None);
            }
        }
    }
    Ok(result)
}
fn coordinate(value: &Value) -> Result<f64> {
    let v = numeric(value)?;
    if v.abs() > 1e9 {
        return Err(invalid("Unbounded stage coordinate"));
    }
    Ok(v)
}
fn clock(graph: &Graph<'_>, field: Option<&str>) -> Result<f64> {
    match field {
        Some("frame_num" | "frameNum") => number(&graph.root()?, field.unwrap()),
        Some(_) => Err(invalid("Unknown native clock")),
        None => graph.root_frame(),
    }
}
pub fn movie_observation(
    capture: &Value,
    chart: &Value,
    selection: &MotionSelection,
    clock_field: Option<&str>,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Value> {
    if capture["swfHeaderFps"].as_f64() != Some(60.0) {
        return Err(invalid("Motion requires a verified 60 Hz clock"));
    }
    let diagnostics = chart
        .get("presentationDiagnostics")
        .map(array)
        .transpose()?
        .cloned()
        .unwrap_or_default();
    audit_ok(capture, &diagnostics)?;
    if selection.iterations.len() > 4096
        || selection.iterations.is_empty()
        || selection
            .target_y
            .is_some_and(|v| !v.is_finite() || v.abs() > 1e9)
    {
        return Err(invalid("Invalid motion selection"));
    }
    let lanes = array(&chart["lanes"])?;
    let lane = lanes
        .get(selection.lane)
        .ok_or_else(|| invalid("Motion lane outside selected mode"))?;
    let raw = numeric(
        array(&lane["nativeFrames"])?
            .first()
            .ok_or_else(|| invalid("Motion lane has no tap"))?,
    )?;
    let (mut frames, mut widths) = (Vec::new(), Vec::new());
    for item in items(capture)?.iter().filter(|i| {
        i["frame"]
            .as_u64()
            .is_some_and(|f| selection.iterations.contains(&f))
    }) {
        checkpoint()?;
        let graph = Graph::new(&item["state"])?;
        let display = display_map(item)?;
        let Some(note) = display.get(&selection.note).copied().flatten() else {
            continue;
        };
        let Some(receptor) = display.get(&selection.receptor).copied().flatten() else {
            continue;
        };
        let geometry = if let Some(name) = &selection.geometry_reference {
            display
                .get(name)
                .copied()
                .flatten()
                .ok_or_else(|| invalid("Missing geometry reference"))?
        } else {
            receptor
        };
        let point = array(&note["stageRegistrationPixels"])?;
        let target = array(&receptor["stageRegistrationPixels"])?;
        let bounds = array(&geometry["stageBoundsPixels"])?;
        if point.len() != 2 || target.len() != 2 || bounds.len() != 4 {
            return Err(invalid("Invalid stage geometry"));
        }
        frames.push(json!({"iteration":iteration(item)?,"movieFrame":clock(&graph,clock_field)?,"x":coordinate(&point[0])?,"y":coordinate(&point[1])?,"targetY":selection.target_y.unwrap_or(coordinate(&target[1])?)}));
        widths.push(coordinate(&bounds[2])? - coordinate(&bounds[0])?);
    }
    if frames.len() < 3
        || widths.iter().any(|v| *v <= 0.0)
        || widths.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - widths.iter().copied().fold(f64::INFINITY, f64::min)
            > 1e-6
    {
        return Err(invalid("Need repeated stable arrow/reference observations"));
    }
    let (a, b) = (&frames[0], frames.last().unwrap());
    let n = |v: &Value, k: &str| numeric(&v[k]);
    let delta = n(b, "movieFrame")? - n(a, "movieFrame")?;
    if delta <= 0.0 {
        return Err(invalid("Native motion clock does not progress"));
    }
    let velocity = (n(b, "y")? - n(a, "y")?) / delta;
    if velocity >= 0.0 || !velocity.is_finite() {
        return Err(invalid("Expected upward finite linear motion"));
    }
    let tolerance = 0.05 + 1e-9;
    for f in &frames {
        let ticks = n(f, "movieFrame")? - n(a, "movieFrame")?;
        if ticks != n(f, "iteration")? - n(a, "iteration")?
            || (n(f, "x")? - n(a, "x")?).abs() > 1e-6
            || (n(f, "y")? - (n(a, "y")? + velocity * ticks)).abs() > tolerance
        {
            return Err(invalid(
                "Nonlinear/reused arrow or inconsistent native clock",
            ));
        }
    }
    let arrival = n(a, "movieFrame")? + (n(a, "targetY")? - n(a, "y")?) / velocity;
    let mut base_velocity = velocity.abs();
    let mut author_ratio = 1.0;
    if chart.get("initialPixelSpeed").is_some() {
        let initial = numeric(&chart["initialPixelSpeed"])?;
        let events = array(&chart["speed"])?;
        let active = |frame: f64| -> Result<f64> {
            let mut ratio = 1.0;
            for event in events {
                if numeric(&event["nativeFrame"])? <= frame {
                    ratio = numeric(&event["multiplier"])?;
                }
            }
            Ok(ratio)
        };
        let ratios = if chart["motionSpeedPolicy"] == "per-arrow-at-spawn" {
            let ratio =
                if selection.lane == 4 && chart.get("independentOnigiriPixelSpeed").is_some() {
                    numeric(&chart["independentOnigiriPixelSpeed"])? / initial
                } else {
                    active(raw)?
                };
            for item in items(capture)?.iter().filter(|i| {
                i["frame"]
                    .as_u64()
                    .is_some_and(|f| selection.iterations.contains(&f))
            }) {
                if let Some(note) = display_map(item)?.get(&selection.note).copied().flatten() {
                    let graph = Graph::new(&item["state"])?;
                    if (number(&graph.props(&note["object"])?, "spd")? - initial * ratio).abs()
                        > 1e-9
                    {
                        return Err(invalid("Stored per-arrow speed differs"));
                    }
                }
            }
            vec![ratio; frames.len()]
        } else if chart.get("motionSpeedPolicy").is_some() {
            return Err(invalid("Unknown native motion speed policy"));
        } else {
            frames
                .iter()
                .map(|f| active(numeric(&f["movieFrame"])?))
                .collect::<Result<Vec<_>>>()?
        };
        if ratios[0] <= 0.0 || ratios.iter().any(|r| *r != ratios[0]) {
            return Err(Error::new(
                "native-motion-segment-unproved",
                "Motion crosses author speed segments",
            ));
        }
        author_ratio = ratios[0];
        base_velocity = initial;
        if (velocity.abs() - initial * author_ratio).abs() > tolerance {
            return Err(invalid("Motion disagrees with stored pixel speed"));
        }
        for p in frames.windows(2) {
            let ticks = n(&p[1], "movieFrame")? - n(&p[0], "movieFrame")?;
            if (n(&p[0], "y")? - n(&p[1], "y")? - initial * author_ratio * ticks).abs()
                > tolerance * ticks
            {
                return Err(invalid("Stored increment cannot explain quantized motion"));
            }
        }
    }
    if (arrival - raw).abs() > 1.5 {
        return Err(invalid("Arrival differs from source judgment frame"));
    }
    if !(base_velocity * 60.0 / widths[0]).is_finite() {
        return Err(invalid("Native base scroll overflows"));
    }
    Ok(
        json!({"schema":"ddi-native-motion-measurement/experimental-1","sourceSwfSha256":capture["source"]["movieSha256"],"fps":60,"pixelsPerSecond":velocity.abs()*60.0,"referencePixels":widths[0],"baseArrowHeightsPerSecond":base_velocity*60.0/widths[0],"observedAuthorMultiplier":author_ratio,"noteName":selection.note,"receptorName":selection.receptor,"geometryReferenceName":selection.geometry_reference.as_ref().unwrap_or(&selection.receptor),"clockField":clock_field,"stagePositionTolerancePixels":0.05,"arrivalDifferenceFrames":arrival-raw,"rawFirstFrame":raw,"observedArrivalMovieFrame":arrival,"samples":frames,"scope":"Observed native default movement, measured source geometry and verified raw movie-frame arrival"}),
    )
}
pub fn movie_audio_origin(
    capture: &Value,
    clock_field: Option<&str>,
    stream_index: usize,
) -> Result<Value> {
    if capture["swfHeaderFps"].as_f64() != Some(60.0) || capture["avm1Halted"] != false {
        return Err(invalid("Audio origin requires a nonhalted 60 Hz capture"));
    }
    let starts = array(&capture["audio"]["streamStarts"])?;
    let start = starts
        .get(stream_index)
        .ok_or_else(|| invalid("Missing native stream start"))?;
    if start["compression"] != "Mp3" {
        return Err(invalid("Need an observed MP3 song stream start"));
    }
    let matching = items(capture)?
        .iter()
        .filter(|i| i["frame"] == start["iteration"])
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err(invalid("Need exact audio-start snapshot"));
    }
    let mut result = start.clone();
    if !result.is_object() {
        return Err(invalid("Malformed stream start"));
    }
    result["streamStartIndex"] = json!(stream_index);
    if let Some(next) = starts.get(stream_index + 1) {
        result["nextStreamOutputSampleFrame"] = next["outputSampleFrame"].clone();
    }
    result["logicalFrame"] = json!(clock(&Graph::new(&matching[0]["state"])?, clock_field)?);
    result["clock"] = json!(clock_field.unwrap_or("root movie frame"));
    result["scope"] = json!(
        "Observed restricted Ruffle stream start; original Flash/device latency not measured"
    );
    Ok(result)
}
pub fn relative_chart(chart: &Value, origin: f64) -> Result<Value> {
    if !origin.is_finite() || origin.abs() > 720000.0 {
        return Err(invalid("Invalid logical audio origin"));
    }
    let mut result = chart.clone();
    for lane in result["lanes"]
        .as_array_mut()
        .ok_or_else(|| invalid("Missing projected lanes"))?
    {
        let taps = array(&lane["nativeFrames"])?
            .iter()
            .map(|v| Ok((numeric(v)? - origin) / 60.0))
            .collect::<Result<Vec<_>>>()?;
        let holds = array(&lane["nativeHoldFrames"])?
            .iter()
            .map(|p| {
                Ok([
                    (numeric(&p[0])? - origin) / 60.0,
                    (numeric(&p[1])? - origin) / 60.0,
                ])
            })
            .collect::<Result<Vec<_>>>()?;
        lane["tapsSeconds"] = json!(taps);
        lane["holdsSeconds"] = json!(holds);
    }
    for key in ["speed", "boost", "stops"] {
        if result.get(key).is_none() {
            result[key] = json!([]);
        }
        for event in result[key]
            .as_array_mut()
            .ok_or_else(|| invalid("Invalid projected events"))?
        {
            event["seconds"] = json!((numeric(&event["nativeFrame"])? - origin) / 60.0);
        }
    }
    Ok(result)
}

/// Bind observations made in different runs to the same logical counter origin.
pub fn validate_clock_agreement(origin: &Value, motion: &Value) -> Result<()> {
    let offset = numeric(&origin["logicalFrame"])? - numeric(&origin["iteration"])?;
    let clock = motion["clockField"].as_str().unwrap_or("root movie frame");
    if origin["clock"] != clock || origin["clockPath"] != motion["clockPath"] {
        return Err(Error::new(
            "native-clock-mismatch",
            "Audio and movement use different clock fields",
        ));
    }
    let samples = array(&motion["samples"])?;
    if samples.len() < 3 || samples.len() > 4096 {
        return Err(invalid("Need bounded movement samples"));
    }
    for sample in samples {
        if numeric(&sample["movieFrame"])? - numeric(&sample["iteration"])? != offset {
            return Err(Error::new(
                "native-clock-mismatch",
                "Audio and movement counter offsets differ",
            ));
        }
    }
    Ok(())
}
