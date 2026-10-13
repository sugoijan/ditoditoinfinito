use super::*;
use std::cmp::Ordering;
#[derive(Clone, Copy, Debug)]
struct Frame(f64);
impl Frame {
    fn new(v: f64) -> Result<Self> {
        if !v.is_finite() || v.abs() > 720000.0 {
            return Err(invalid("Schedule frame outside bounds"));
        }
        Ok(Self(if v == 0.0 { 0.0 } else { v }))
    }
}
impl PartialEq for Frame {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Eq for Frame {}
impl PartialOrd for Frame {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Frame {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}
type Schedule = BTreeMap<(Frame, usize), usize>;
fn add(schedule: &mut Schedule, frame: f64, lane: usize, count: usize) -> Result<()> {
    *schedule.entry((Frame::new(frame)?, lane)).or_default() += count;
    Ok(())
}
fn schedule(graph: &Graph<'_>, reference: &Value, indices: &[usize]) -> Result<Schedule> {
    let mut result = Schedule::new();
    for (key, value) in graph.props(reference)? {
        let digits = key.strip_prefix('-').unwrap_or(&key);
        if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
            if !["length", "__proto__", "__constructor__", "constructor"].contains(&key.as_str()) {
                return Err(invalid("Unknown schedule property"));
            }
            continue;
        }
        let frame = key
            .parse::<i64>()
            .map_err(|_| invalid("Invalid schedule frame"))?;
        for value in graph.array(value)? {
            let lane = numeric(value)?;
            if lane.fract() != 0.0 || !indices.iter().any(|i| *i as f64 == lane) {
                return Err(invalid("Unsupported scheduled lane"));
            }
            add(&mut result, frame as f64, lane as usize, 1)?;
        }
    }
    Ok(result)
}
fn rows(schedule: &Schedule) -> Value {
    json!(
        schedule
            .iter()
            .map(|((f, l), n)| json!({"frame":f.0,"lane":l,"count":n}))
            .collect::<Vec<_>>()
    )
}
fn expected(lanes: &[Value], indices: &[usize], travel: f64) -> Result<(Schedule, Schedule)> {
    let (mut taps, mut freezes) = (Schedule::new(), Schedule::new());
    for (lane, index) in lanes.iter().zip(indices) {
        for f in array(&lane["nativeFrames"])? {
            add(&mut taps, numeric(f)? - travel, *index, 1)?;
        }
        for pair in array(&lane["nativeHoldFrames"])? {
            add(&mut freezes, numeric(&pair[0])? - travel, *index, 1)?;
        }
    }
    Ok((taps, freezes))
}
fn recovered(schedule: &Schedule, travel: &Props<'_>) -> Result<Schedule> {
    let mut result = Schedule::new();
    for ((f, l), n) in schedule {
        add(
            &mut result,
            f.0 + number(travel, &format!("{:.0}", f.0))?,
            *l,
            *n,
        )?;
    }
    Ok(result)
}
fn raw_lanes(graph: &Graph<'_>, root: &Props<'_>, mode: Mode) -> Result<(Vec<Value>, String)> {
    let (defs, indices) = definitions(mode)?;
    let mut lanes = Vec::new();
    let mut selected = String::new();
    if root.contains_key("timeline") {
        let taps = graph.array(get(root, "timeline")?)?;
        let flat = graph.array(get(root, "ftimeline")?)?;
        for ((name, _), i) in defs.iter().zip(indices) {
            let taps = numbers(
                graph,
                taps.get(i).ok_or_else(|| invalid("Missing raw tap lane"))?,
            )?;
            let holds_flat = numbers(
                graph,
                flat.get(i)
                    .ok_or_else(|| invalid("Missing raw hold lane"))?,
            )?;
            lanes.push(lane(name, taps, holds(&holds_flat, false)?, false));
        }
    } else {
        selected = suffix(get(root, "diffin")?)?;
        if optional(root, "DOS_DIVIDE") == &Value::Bool(true) {
            selected.clear();
        }
        for (name, _) in defs {
            let field = source_field(root, name, &selected, "")?
                .ok_or_else(|| invalid("Missing selected source lane"))?;
            let taps = split_numbers(get(root, &field)?)?;
            let field = source_field(root, &hold_stem(name), &selected, "")?;
            let flat = field
                .map(|f| split_numbers(get(root, &f)?))
                .transpose()?
                .unwrap_or_default();
            lanes.push(lane(name, taps, holds(&flat, false)?, false));
        }
    }
    Ok((lanes, selected))
}
pub(super) fn scheduled(
    capture: &Value,
    options: &ProjectionOptions,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Projection> {
    let parafla = options.family == Family::ParaflaCounter;
    let mut diagnostics = Vec::new();
    if options.review_missing_decorative_sprites {
        diagnostics = array(&capture["audit"]["entries"])?
            .iter()
            .filter(|e| e["kind"] == "runtime-error")
            .cloned()
            .collect();
        if diagnostics.is_empty()
            || diagnostics.iter().any(|e| {
                e["detail"] != "message=Tried to instantiate a non-registered character undefined "
                    || e["iteration"].as_u64().is_none()
            })
        {
            return Err(invalid("Unknown presentation failure"));
        }
    }
    audit_ok(capture, &diagnostics)?;
    let (_, indices) = definitions(options.mode)?;
    for item in items(capture)? {
        checkpoint()?;
        let graph = Graph::new(&item["state"])?;
        let root = graph.root()?;
        if !root.contains_key("mkArrow") || !root.contains_key("judgeY") {
            continue;
        }
        let clock = if root.contains_key("frameNum") {
            "frameNum"
        } else {
            "frame_num"
        };
        if number(&root, clock)? != 0.0 {
            continue;
        }
        let (lanes, selected) = raw_lanes(&graph, &root, options.mode)?;
        if !lanes
            .iter()
            .any(|l| l["nativeFrames"].as_array().is_some_and(|v| !v.is_empty()))
        {
            continue;
        }
        let mut inactive = Vec::new();
        if parafla {
            let codes = numbers(&graph, get(&root, "keyCtrl")?)?;
            if !root.contains_key("frameNum")
                || !root.contains_key("arrowSpd")
                || codes.len() != 7
                || number(&root, "setSpd")? != 2.0 * number(&root, "difSpd")?
                || number(&root, "setSpd")? <= 0.0
            {
                return Err(invalid("Unproved ParaFla clock, keys or speed units"));
            }
            inactive = codes
                .iter()
                .enumerate()
                .filter(|(i, c)| **c == 0.0 && !indices.contains(i))
                .map(|(i, _)| i)
                .collect();
        }
        let all_indices = indices.iter().chain(&inactive).copied().collect::<Vec<_>>();
        let all_taps = schedule(&graph, get(&root, "mkArrow")?, &all_indices)?;
        let all_freezes = schedule(&graph, get(&root, "mkFArrow")?, &all_indices)?;
        let taps = all_taps
            .iter()
            .filter(|((_, l), _)| indices.contains(l))
            .map(|(k, v)| (*k, *v))
            .collect::<Schedule>();
        let freezes = all_freezes
            .iter()
            .filter(|((_, l), _)| indices.contains(l))
            .map(|(k, v)| (*k, *v))
            .collect::<Schedule>();
        if taps.is_empty() {
            continue;
        }
        let travel = graph.props(get(&root, "judgeY")?)?;
        let (expected_taps, expected_holds) = expected(&lanes, &indices, 0.0)?;
        if recovered(&taps, &travel)? != expected_taps
            || recovered(&freezes, &travel)? != expected_holds
        {
            return Err(invalid("Spawn plus travel differs from raw heads"));
        }
        if !diagnostics.is_empty() {
            if diagnostics
                .iter()
                .any(|e| e["iteration"].as_u64().unwrap() <= iteration(item).unwrap())
            {
                return Err(invalid("Presentation failure preceded compilation"));
            }
            let required = SEVEN
                .iter()
                .enumerate()
                .filter(|(i, _)| indices.contains(i))
                .map(|(_, (_, c))| format!("{c}_step"))
                .collect::<Vec<_>>();
            let mut present = false;
            for i in items(capture)? {
                let display = observed::display_map(i)?;
                if required
                    .iter()
                    .all(|k| display.get(k).is_some_and(|v| v.is_some()))
                {
                    present = true;
                    break;
                }
            }
            if !present {
                return Err(invalid("Required selected-mode sprites missing"));
            }
        }
        let meta = metadata(
            &root,
            &[
                "musicTitle",
                "GAME_TITLE",
                "TUNING_NAME",
                "TUNING_NAMES",
                "tuning",
                "diffi",
                "VERSION",
                "keyLabel",
                "ARTIST_NAME",
                "ARTIST_URL",
                "CREDIT",
                "CREATOR_NAME",
            ],
        )?;
        if meta
            .get("keyLabel")
            .is_some_and(|v| v != &json!(options.mode))
        {
            return Err(invalid("Selected key mode differs"));
        }
        let mut speeds = if let Some(v) = root.get("speed_dac") {
            numbers(&graph, v)?
        } else {
            split_numbers(
                root.get("speed_change")
                    .copied()
                    .unwrap_or(optional(&root, "speed_data")),
            )?
        };
        let boosts = numbers(
            &graph,
            root.get("boost_dac")
                .copied()
                .unwrap_or(optional(&root, "boostDac")),
        )?;
        let mut chart = json!({"mode":options.mode,"lanes":lanes,"speed":[],"boost":events(&boosts)?,"colors":[],"metadata":meta,"nativeLaneIndices":indices,"nativeKeyCodes":if optional(&root,"keyCtrl")["ref"].is_number(){json!(numbers(&graph,get(&root,"keyCtrl")?)?)}else{Value::Null},"clockField":clock,"scheduleAgreement":true,"preparedIteration":iteration(item)?,"nativeSchedules":{"taps":rows(&taps),"holds":rows(&freezes)},"presentationDiagnostics":diagnostics});
        if !parafla && root.contains_key("set_spi") && root.contains_key("dif_spi") {
            let initial = number(&root, "set_spi")?;
            if initial <= 0.0 || initial != 2.0 * number(&root, "dif_spi")? {
                return Err(invalid("Unproved initial scheduled speed"));
            }
            chart["initialPixelSpeed"] = json!(initial);
        }
        if parafla {
            let initial = number(&root, "setSpd")?;
            let pixels = numbers(&graph, get(&root, "arrowSpd")?)?;
            if pixels.len() < 2
                || !pixels.len().is_multiple_of(2)
                || pixels[..2] != [0.0, initial]
                || pixels
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|p| p[0])
                    .collect::<Vec<_>>()
                    .windows(2)
                    .any(|p| p[0] >= p[1])
            {
                return Err(invalid("Invalid parsed ParaFla pixel speeds"));
            }
            speeds = pixels[2..]
                .iter()
                .enumerate()
                .map(|(i, v)| if i % 2 == 0 { *v } else { v / initial })
                .collect();
            chart["initialPixelSpeed"] = json!(initial);
            chart["parsedPixelSpeeds"] = json!(pixels);
            chart["sourceSelection"] = json!({"runtimeSuffix":suffix(get(&root,"diffin")?)?,"splitDos":optional(&root,"DOS_DIVIDE")==&Value::Bool(true),"loadedFieldSuffix":selected});
            chart["inactiveSchedules"] = json!({"nativeKeyCodes":numbers(&graph,get(&root,"keyCtrl")?)?,"taps":rows(&all_taps.iter().filter(|((_,l),_)|inactive.contains(l)).map(|(k,v)|(*k,*v)).collect()),"holds":rows(&all_freezes.iter().filter(|((_,l),_)|inactive.contains(l)).map(|(k,v)|(*k,*v)).collect())});
        }
        chart["speed"] = events(&speeds)?;
        return Ok(Projection {
            prepared_iteration: iteration(item)?,
            chart,
        });
    }
    Err(invalid(
        "Need complete raw lanes and compiled schedule before consumption",
    ))
}
fn settings(
    graph: &Graph<'_>,
    root: &Props<'_>,
    raw: &[Value],
    indices: &[usize],
    travel: f64,
    mode: Mode,
) -> Result<Value> {
    if number(root, "set_cnt")? != travel {
        return Err(invalid("Unstable constant travel"));
    }
    let lengths = graph.array(get(root, "frzLength")?)?;
    for (index, lane) in indices.iter().zip(raw) {
        let pairs = array(&lane["nativeHoldFrames"])?;
        if !pairs.is_empty() {
            let duration = graph.props(
                lengths
                    .get(*index)
                    .ok_or_else(|| invalid("Missing hold duration lane"))?,
            )?;
            for (i, p) in pairs.iter().enumerate() {
                if number(&duration, &(i * 2).to_string())? != numeric(&p[1])? - numeric(&p[0])? {
                    return Err(invalid("Hold duration cache differs"));
                }
            }
        }
    }
    let meta = metadata(root, &["diffi", "keyLabel", "musicTitle", "tuning"])?;
    if meta["keyLabel"] != json!(mode) {
        return Err(invalid("Constant key mode changed"));
    }
    let speeds = numbers(graph, get(root, "speed_dac")?)?;
    let speed = events(&speeds)?;
    if speeds
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| p[0])
        .collect::<Vec<_>>()
        .windows(2)
        .any(|p| p[0] > p[1])
    {
        return Err(invalid("Unordered constant speeds"));
    }
    let initial = number(root, "set_spi")?;
    if initial <= 0.0 || initial != 2.0 * number(root, "dif_spi")? {
        return Err(invalid("Unproved initial constant speed"));
    }
    Ok(
        json!({"metadata":meta,"initialPixelSpeed":initial,"nativeKeyCodes":numbers(graph,get(root,"keyCtrl")?)?,"speed":speed}),
    )
}
pub(super) fn constant(
    capture: &Value,
    mode: Mode,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Projection> {
    audit_ok(capture, &[])?;
    let (defs, indices) = definitions(mode)?;
    let mut before = None::<(u64, Vec<Value>, f64)>;
    let mut compiled = None::<(Value, Schedule, Schedule)>;
    for item in items(capture)? {
        checkpoint()?;
        let graph = Graph::new(&item["state"])?;
        let root = graph.root()?;
        if root.contains_key("frame_num") && number(&root, "frame_num")? != 0.0 {
            continue;
        }
        if let Some((result, expected, expected_h)) = &compiled
            && root.contains_key("frame_num")
        {
            let (_, raw, travel) = before.as_ref().unwrap();
            if ["mkArrow", "mkFArrow", "frzLength", "set_cnt"]
                .iter()
                .any(|k| !root.contains_key(*k))
            {
                continue;
            }
            if schedule(&graph, get(&root, "mkArrow")?, &indices)? != *expected
                || schedule(&graph, get(&root, "mkFArrow")?, &indices)? != *expected_h
            {
                return Err(invalid("Schedule changed before counter initialization"));
            }
            let initialized = settings(&graph, &root, raw, &indices, *travel, mode)?;
            equal_fields(
                &initialized,
                result,
                &["initialPixelSpeed", "nativeKeyCodes", "speed"],
            )?;
            for (k, v) in initialized["metadata"].as_object().unwrap() {
                if result["metadata"].get(k) != Some(v) {
                    return Err(invalid("Metadata changed before counter initialization"));
                }
            }
            let mut result = result.clone();
            result["preparedIteration"] = json!(iteration(item)?);
            result["clockInitializationIteration"] = json!(iteration(item)?);
            return Ok(Projection {
                prepared_iteration: iteration(item)?,
                chart: result,
            });
        }
        if ["timeline", "ftimeline", "mkArrow", "mkFArrow", "set_cnt"]
            .iter()
            .any(|k| !root.contains_key(*k))
        {
            continue;
        }
        let taps = graph.array(get(&root, "timeline")?)?;
        let holds_flat = graph.array(get(&root, "ftimeline")?)?;
        let mut lanes = Vec::new();
        for ((name, _), index) in defs.iter().zip(&indices) {
            lanes.push(json!({"name":name,"nativeFrames":numbers(&graph,taps.get(*index).ok_or_else(||invalid("Missing tap lane"))?)?,"nativeHoldFrames":numbers(&graph,holds_flat.get(*index).ok_or_else(||invalid("Missing hold lane"))?)?}));
        }
        if before.is_none() {
            let mut values = Vec::new();
            for index in &indices {
                values.extend(graph.array(taps[*index])?);
            }
            if values.is_empty() || values.iter().any(|v| v["type"] != "string") {
                continue;
            }
            let selected = root
                .get("diffin")
                .map(|v| suffix(v))
                .transpose()?
                .unwrap_or_default();
            for l in &mut lanes {
                let name = l["name"].as_str().unwrap().to_string();
                let flat = array(&l["nativeHoldFrames"])?
                    .iter()
                    .map(numeric)
                    .collect::<Result<Vec<_>>>()?;
                let pairs = holds(&flat, true)?;
                for (stem, expected) in [
                    (name.clone(), l["nativeFrames"].clone()),
                    (hold_stem(&name), json!(flat)),
                ] {
                    if let Some(field) = source_field(&root, &stem, &selected, "")?
                        && json!(split_numbers(get(&root, &field)?)?) != expected
                    {
                        return Err(invalid("Constant selected-source disagreement"));
                    }
                }
                l["nativeHoldFrames"] = json!(pairs);
            }
            before = Some((iteration(item)?, lanes, number(&root, "set_cnt")?));
            continue;
        }
        let (original, raw, travel) = before.as_ref().unwrap();
        if !(0.0 < *travel && *travel <= 12000.0) {
            return Err(invalid("Invalid constant travel"));
        }
        let compiled_taps = schedule(&graph, get(&root, "mkArrow")?, &indices)?;
        let freezes = schedule(&graph, get(&root, "mkFArrow")?, &indices)?;
        if compiled_taps.is_empty() {
            continue;
        }
        let (expected_taps, expected_h) = expected(raw, &indices, *travel)?;
        if compiled_taps != expected_taps || freezes != expected_h {
            return Err(invalid("Constant travel/compiled schedule disagreement"));
        }
        for (source, prepared) in raw.iter().zip(&lanes) {
            let taps = array(&source["nativeFrames"])?
                .iter()
                .map(|v| Ok(numeric(v)? - travel))
                .collect::<Result<Vec<_>>>()?;
            let holds = array(&source["nativeHoldFrames"])?
                .iter()
                .map(|p| Ok([numeric(&p[0])? - travel, numeric(&p[1])?]))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            if prepared["nativeFrames"] != json!(taps)
                || prepared["nativeHoldFrames"] != json!(holds)
            {
                return Err(invalid("Constant array mutation disagreement"));
            }
        }
        let mut result = json!({"mode":mode,"lanes":raw,"boost":[],"colors":[],"nativeLaneIndices":indices,"clockField":"frame_num","scheduleAgreement":true,"preparedIteration":iteration(item)?,"splitIteration":original,"compiledIteration":iteration(item)?,"constantTravelFrames":travel,"nativeSchedules":{"taps":rows(&compiled_taps),"holds":rows(&freezes)},"nativePresentation":{"iro_dac":if root.contains_key("iro_dac")&&!undefined(get(&root,"iro_dac")?){json!(graph.array(get(&root,"iro_dac")?)?)}else{json!([])}}});
        result.as_object_mut().unwrap().extend(
            settings(&graph, &root, raw, &indices, *travel, mode)?
                .as_object()
                .unwrap()
                .clone(),
        );
        if root.contains_key("frame_num") {
            return Ok(Projection {
                prepared_iteration: iteration(item)?,
                chart: result,
            });
        }
        compiled = Some((result, expected_taps, expected_h));
    }
    Err(invalid("Need split, compiled and zero-counter snapshots"))
}
