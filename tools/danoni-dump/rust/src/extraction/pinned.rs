//! Exact reviewed movie contracts, distinct from structural-family discovery.
use super::*;
use std::collections::BTreeSet;
pub(super) const HIGAN_HASH: &str =
    "6ce53b126e90b25a9f307595bf226ed4e6ce8f70c74d816a1d372c7f20de6e85";
pub(super) const KOUHAKU_HASH: &str =
    "e14e472911b6603ddcc58bd40049d0b05cfa986a2240209dd848a516cf40fed2";
// Reviewed selector-controlled speed initialization; no source ActionScript is
// copied here. The exact movie identity binds the static control-flow review.
pub(super) const RERAVE_HASH: &str =
    "a6fec2d1ac860d35d5481274fbed04b35c608049e30d8d2d30e01e3a5bcbbbae";
pub(super) fn inactive_child_speed(
    capture: &Value,
    options: &ProjectionOptions,
    bank: &Props<'_>,
    field: &str,
    values: &[f64],
) -> Result<Option<Value>> {
    if capture["source"]["movieSha256"] != RERAVE_HASH
        || capture["swfHeaderFps"].as_f64() != Some(60.0)
        || !options.source_prefix.is_empty()
        || options.source_suffix.is_some()
        || options.pinned_selection.is_some()
        || field != "speed_data"
    {
        return Ok(None);
    }
    // Hard selects 1. This movie creates speed_timeline only for selector 2
    // (LVUP), while retaining the shared speed_data on the root for both.
    // Absence alone never proves inactivity for an unreviewed movie.
    for item in items(capture)? {
        if number(&globals(&item["state"])?, "humen")? != 1.0 {
            return Err(invalid("Reviewed inactive speed selector differs"));
        }
    }
    if bank.contains_key("speed_timeline") || number(bank, "s")? != 0.0 {
        return Err(invalid("Reviewed inactive speed bank/cursor differs"));
    }
    events(values)?;
    Ok(Some(json!({
        "contract":"rerave-selector-speed/1", "movieSha256":RERAVE_HASH,
        "field":field, "values":values, "selector":{"scope":"swf6-global","field":"humen","value":1},
        "reason":"Reviewed movie initializes the speed bank only for selector 2; selector 1 retains shared source data without executing it"
    })))
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PinnedSelection {
    Tougenkyo,
    TougenkyoHard,
    Run,
    Kitakaze,
    Korokke,
    Emy,
}
impl PinnedSelection {
    fn details(self) -> (&'static str, &'static str, &'static str, u16, u16) {
        match self {
            Self::Tougenkyo => ("tougenkyo", "桃源郷", "公", 229, 6),
            Self::TougenkyoHard => ("tougenkyo-hard", "桃源郷 hard", "公", 229, 6),
            Self::Run => ("run", "RUN", "tio", 232, 7),
            Self::Kitakaze => ("kitakaze", "北風のメロディー", "八乙女葦菜", 234, 8),
            Self::Korokke => ("korokke", "ころっけわっしょい", "ゆうかり", 236, 9),
            Self::Emy => ("emy", "emy", "onoken", 238, 10),
        }
    }
    fn stars(self) -> usize {
        match self {
            Self::Tougenkyo => 3,
            Self::TougenkyoHard => 8,
            Self::Run => 6,
            Self::Kitakaze | Self::Emy => 5,
            Self::Korokke => 4,
        }
    }
}
fn check(capture: &Value, hash: &str) -> Result<()> {
    if capture["source"]["movieSha256"] != hash || capture["swfHeaderFps"].as_f64() != Some(60.0) {
        return Err(invalid("Unreviewed pinned movie/timebase"));
    }
    items(capture)?;
    audit_ok(capture, &[])
}
fn strings_equal(root: &Props<'_>, key: &str, expected: &str) -> Result<()> {
    if text(get(root, key)?)? != expected {
        return Err(invalid(format!("Unreviewed pinned {key}")));
    }
    Ok(())
}
fn cell<'a>(bank: &[&'a Value], index: usize) -> Result<&'a Value> {
    bank.get(index)
        .copied()
        .ok_or_else(|| invalid("Missing pinned lane array"))
}
fn nums(graph: &Graph<'_>, v: &Value) -> Result<Vec<f64>> {
    graph.array(v)?.iter().map(|v| numeric(v)).collect()
}
type Schedule = BTreeMap<(u64, usize), usize>;
fn schedule(graph: &Graph<'_>, value: &Value) -> Result<Schedule> {
    let mut result = Schedule::new();
    for (key, v) in graph.props(value)? {
        if key.is_empty() || !key.bytes().all(|c| c.is_ascii_digit()) {
            if !["length", "__proto__", "__constructor__"].contains(&key.as_str()) {
                return Err(invalid("Unexpected pinned schedule property"));
            }
            continue;
        }
        let frame = key
            .parse::<u64>()
            .ok()
            .filter(|n| *n <= 720000)
            .ok_or_else(|| invalid("Invalid pinned schedule frame"))?;
        for lane in nums(graph, v)? {
            if lane.fract() != 0.0 || ![0.0, 2.0, 4.0, 6.0, 3.0].contains(&lane) {
                return Err(invalid("Unreviewed pinned schedule lane"));
            }
            *result.entry((frame, lane as usize)).or_default() += 1;
        }
    }
    Ok(result)
}
fn counted(result: &mut Schedule, frame: f64, lane: usize) -> Result<()> {
    if !(0.0..=720000.0).contains(&frame) || frame.fract() != 0.0 {
        return Err(invalid("Invalid pinned spawn frame"));
    }
    *result.entry((frame as u64, lane)).or_default() += 1;
    Ok(())
}
fn one_start(capture: &Value) -> Result<&Value> {
    let starts = array(&capture["audio"]["streamStarts"])?;
    if starts.len() != 1 || starts[0]["compression"] != "Mp3" {
        return Err(invalid("Need one pinned MP3 start"));
    }
    Ok(&starts[0])
}
fn at(capture: &Value, frame: u64) -> Result<&Value> {
    items(capture)?
        .iter()
        .find(|i| iteration(i) == Ok(frame))
        .ok_or_else(|| invalid("Missing exact pinned snapshot iteration"))
}
fn display_tree(item: &Value) -> Result<&Vec<Value>> {
    let tree = array(&item["state"]["displayTree"])?;
    if tree.len() > 512 {
        return Err(invalid("Pinned display limit"));
    }
    for (i, d) in tree.iter().enumerate() {
        if d["id"].as_u64() != Some(i as u64) {
            return Err(invalid("Pinned display ids must match local indices"));
        }
    }
    Ok(tree)
}
fn path(tree: &[Value], index: usize) -> Result<Vec<String>> {
    let mut seen = BTreeSet::new();
    let mut parts = vec![];
    let mut current = Some(index);
    while let Some(index) = current {
        if !seen.insert(index) || seen.len() > 64 {
            return Err(invalid("Cyclic/deep pinned display ancestry"));
        }
        let d = tree
            .get(index)
            .ok_or_else(|| invalid("Invalid display parent index"))?;
        parts.push(text(&d["name"])?);
        current = if d["parent"].is_null() {
            None
        } else {
            Some(
                d["parent"]
                    .as_u64()
                    .filter(|v| *v < tree.len() as u64)
                    .ok_or_else(|| invalid("Invalid display parent"))? as usize,
            )
        };
    }
    parts.reverse();
    Ok(parts)
}
fn find_path<'a>(tree: &'a [Value], wanted: &[String]) -> Result<&'a Value> {
    let mut found = None;
    for (i, d) in tree.iter().enumerate() {
        if d["name"].is_null() {
            continue;
        }
        if path(tree, i)? == wanted {
            if found.is_some() {
                return Err(invalid("Ambiguous pinned display path"));
            }
            found = Some(d);
        }
    }
    found.ok_or_else(|| invalid("Missing pinned display path"))
}
fn globals(state: &Value) -> Result<Props<'_>> {
    let scopes = array(&state["globalScopes"])?;
    if scopes.len() > 64 {
        return Err(invalid("Global scope limit"));
    }
    let selected = scopes
        .iter()
        .filter(|s| s["caseSensitive"] == false)
        .collect::<Vec<_>>();
    if selected.len() != 1 {
        return Err(invalid("Need one SWF6 global scope"));
    }
    let properties = array(&selected[0]["properties"])?;
    if properties.len() > 640000 {
        return Err(invalid("Global property limit"));
    }
    let mut result = Props::new();
    for p in properties {
        if p["accessor"] != false {
            return Err(invalid("Pinned global accessor is not stored data"));
        }
        if result
            .insert(
                text(&p["name"])?,
                p.get("value")
                    .ok_or_else(|| invalid("Missing global value"))?,
            )
            .is_some()
        {
            return Err(invalid("Duplicate global property"));
        }
    }
    Ok(result)
}
pub(super) fn higan_reference(
    capture: &Value,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Value> {
    check(capture, HIGAN_HASH)?;
    let (mut split, mut prepared, mut cleanup) = (None, None, None);
    for item in items(capture)? {
        checkpoint()?;
        let graph = Graph::new(&item["state"])?;
        let root = graph.root()?;
        if let Some(t) = root.get("timeline") {
            let outer = graph.array(t)?;
            let first = graph.array(
                outer
                    .first()
                    .ok_or_else(|| invalid("Empty pinned timeline"))?,
            )?;
            let head = first
                .first()
                .ok_or_else(|| invalid("Empty pinned first lane"))?;
            if head["type"] == "string" {
                split = Some(item);
            } else if head.is_number() {
                prepared = Some(item);
            } else {
                return Err(invalid("Invalid pinned split/prepared value"));
            }
        } else if root.contains_key("mkArrow") && graph.root_frame()? == 4.0 {
            cleanup = Some(item);
        }
    }
    let (s, p, f) = (
        split.ok_or_else(|| invalid("Missing Higan split"))?,
        prepared.ok_or_else(|| invalid("Missing Higan prepared arrays"))?,
        cleanup.ok_or_else(|| invalid("Missing Higan cleanup"))?,
    );
    if !(iteration(s)? < iteration(p)? && iteration(p)? < iteration(f)?) {
        return Err(invalid("Unordered Higan initialization stages"));
    }
    let (before, after, clean) = (
        Graph::new(&s["state"])?,
        Graph::new(&p["state"])?,
        Graph::new(&f["state"])?,
    );
    let (b, p_root, f_root) = (before.root()?, after.root()?, clean.root()?);
    for root in [&b, &p_root, &f_root] {
        strings_equal(root, "keyLabel", "5")?;
        strings_equal(root, "diffin", "")?;
        strings_equal(root, "motion", "OFF")?;
        for (key, expected) in [
            ("difficulty", 1.0),
            ("correction", 0.0),
            ("set_cnt", 100.0),
            ("startFrame", 0.0),
            ("frame_num", 0.0),
        ] {
            if number(root, key)? != expected {
                return Err(invalid("Unreviewed Higan timing/settings"));
            }
        }
    }
    let start = one_start(capture)?;
    if start["latencySeek"] != 0 {
        return Err(invalid("Unreviewed Higan stream latency"));
    }
    let audio_item = at(
        capture,
        start["iteration"]
            .as_u64()
            .ok_or_else(|| invalid("Missing audio iteration"))?,
    )?;
    let audio_graph = Graph::new(&audio_item["state"])?;
    let origin = number(&audio_graph.root()?, "frame_num")?;
    let music = display_tree(audio_item)?
        .iter()
        .filter(|d| !d["name"].is_null() && text(&d["name"]).is_ok_and(|s| s == "music_act"))
        .collect::<Vec<_>>();
    if music.len() != 1 || music[0]["movieFrame"] != 2 || origin != 145.0 {
        return Err(invalid("Unreviewed Higan music origin"));
    }
    let indices = [0usize, 2, 4, 6, 3];
    let codes = nums(&after, get(&p_root, "keyCtrl")?)?;
    let mut key_codes = vec![];
    for i in indices {
        let n = *codes
            .get(i)
            .ok_or_else(|| invalid("Missing Higan key code"))?;
        if !(0.0..=65535.0).contains(&n) || n.fract() != 0.0 {
            return Err(invalid("Invalid Higan key code"));
        }
        key_codes.push(n as u16);
    }
    let (bt, pt, bh, ph, lengths) = (
        before.array(get(&b, "timeline")?)?,
        after.array(get(&p_root, "timeline")?)?,
        before.array(get(&b, "ftimeline")?)?,
        after.array(get(&p_root, "ftimeline")?)?,
        after.array(get(&p_root, "frzLength")?)?,
    );
    let mut lanes = vec![];
    let (mut tap_schedule, mut hold_schedule) = (Schedule::new(), Schedule::new());
    for (i, (name, _)) in indices.into_iter().zip(FIVE) {
        checkpoint()?;
        let taps = nums(&before, cell(&bt, i)?)?;
        let spawns = nums(&after, cell(&pt, i)?)?;
        if spawns != taps.iter().map(|n| n - 100.0).collect::<Vec<_>>() {
            return Err(invalid("Higan note transformation differs"));
        }
        for n in &spawns {
            counted(&mut tap_schedule, *n, i)?;
        }
        let mut holds = vec![];
        if undefined(cell(&bh, i)?) {
            if i != 3 || !undefined(cell(&ph, i)?) {
                return Err(invalid("Unreviewed missing Higan hold lane"));
            }
        } else {
            let raw = nums(&before, cell(&bh, i)?)?;
            let transformed = nums(&after, cell(&ph, i)?)?;
            let durations = after.props(cell(&lengths, i)?)?;
            if !raw.len().is_multiple_of(2) || raw.len() != transformed.len() {
                return Err(invalid("Unpaired Higan holds"));
            }
            for (offset, pair) in raw.as_chunks::<2>().0.iter().enumerate() {
                let [head, tail] = *pair;
                let key = (offset * 2).to_string();
                if tail < head
                    || transformed[offset * 2..offset * 2 + 2] != [head - 100.0, tail]
                    || number(&durations, &key)? != tail - head
                {
                    return Err(invalid("Higan hold transformation/duration differs"));
                }
                holds.push([head, tail]);
                counted(&mut hold_schedule, head - 100.0, i)?;
            }
        }
        lanes.push(json!({"name":name,"nativeLaneIndex":i,"judgementFrames":taps,"spawnFrames":spawns,"holdFrames":holds,"tapsSeconds":taps.iter().map(|n|(n-origin)/60.0).collect::<Vec<_>>(),"holdsSeconds":holds.iter().map(|[a,b]|[(a-origin)/60.0,(b-origin)/60.0]).collect::<Vec<_>>()}));
    }
    for (g, r) in [(&after, &p_root), (&clean, &f_root)] {
        if schedule(g, get(r, "mkArrow")?)? != tap_schedule
            || schedule(g, get(r, "mkFArrow")?)? != hold_schedule
        {
            return Err(invalid("Higan compiled schedule differs"));
        }
    }
    let speeds = nums(&clean, get(&f_root, "speed_dac")?)?;
    if !speeds.len().is_multiple_of(2) {
        return Err(invalid("Unpaired Higan speed"));
    }
    let speed = speeds
        .as_chunks::<2>()
        .0
        .iter()
        .map(|[n, m]| json!({"nativeFrame":n,"seconds":(n-origin)/60.0,"multiplier":m}))
        .collect::<Vec<_>>();
    let cv = clean.array(get(&f_root, "iro_dac")?)?;
    if !cv.len().is_multiple_of(3) {
        return Err(invalid("Unpaired Higan color"));
    }
    let (mut colors, mut native_colors) = (vec![], vec![]);
    for row in cv.as_chunks::<3>().0 {
        let frame = numeric(row[0])?;
        let code = text(row[1])?;
        let target = text(row[2])?;
        if !["al", "oni"].contains(&target.as_str())
            || code.len() != 8
            || !code.starts_with("0x")
            || !code[2..].bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(invalid("Unreviewed Higan color target/value"));
        }
        let seconds = (frame - 100.0 - origin) / 60.0;
        native_colors.push(json!({"frame":frame,"target":target,"color":code,"seconds":seconds}));
        for lane in if target == "al" {
            vec![0, 1, 2, 3, 4]
        } else {
            vec![4]
        } {
            colors.push(json!({"seconds":seconds,"target":lane,"color":format!("#{}",&code[2..]),"all":true}));
        }
    }
    let stop = clean.array(get(&f_root, "stop_dac")?)?;
    if stop.len() != 1 || !text(stop[0])?.is_empty() {
        return Err(invalid("Unreviewed Higan stops"));
    }
    Ok(
        json!({"schema":"ddi-danoni-avm1-reference/experimental-1","adapter":"reku-2.16-higan/1","sourceSwfSha256":HIGAN_HASH,"ruffle":capture["build"],"fps":60,"stages":{"split":iteration(s)?,"prepared":iteration(p)?,"cleanup":iteration(f)?},"audioOrigin":{"logicalFrame":origin,"iteration":iteration(audio_item)?,"streamMovieFrame":2,"outputSampleFrame":start["outputSampleFrame"],"scope":"Observed Ruffle post-frame counter and continuous zero-latency stream; original Flash latency unmeasured"},"metadata":{"keyMode":"5","label":text(get(&f_root,"diffi")?)?,"authorSpeed":number(&f_root,"dif_spi")?,"initialPixelSpeed":number(&p_root,"set_spi")?,"musicTitle":text(get(&b,"musicTitle")?)?,"tuning":text(get(&b,"tuning")?)?},"lanes":lanes,"nativeKeyCodes":key_codes,"speed":speed,"colors":colors,"nativeColors":native_colors,"boost":[],"stops":[],"scheduleAgreement":true,"limits":["Pinned work and default options only","Original Flash playback latency unmeasured","Cosmetics and judgement/gauges are not reproduced","Diagonal placeholder lanes excluded after compiled-schedule check"]}),
    )
}
fn kouhaku_settings(item: &Value, selection: PinnedSelection) -> Result<()> {
    let (_, title, artist, _, scene) = selection.details();
    let graph = Graph::new(&item["state"])?;
    let root = graph.root()?;
    let global = globals(&item["state"])?;
    strings_equal(&global, "kyoku", title)?;
    strings_equal(&root, "artist", artist)?;
    strings_equal(&root, "difficulty", &"★".repeat(selection.stars()))?;
    if graph.root_frame()? != scene as f64
        || optional(&global, "Reverse") != &Value::Bool(false)
        || number(&global, "SpeedUp")? != 2.0
        || number(&global, "SpeedY")? != 188.0
        || number(&global, "point")?
            != if selection == PinnedSelection::TougenkyoHard {
                5.0
            } else {
                0.0
            }
    {
        return Err(invalid("Unreviewed Kouhaku movement/selection settings"));
    }
    Ok(())
}
pub(super) fn kouhaku_reference(
    capture: &Value,
    selection: PinnedSelection,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Value> {
    check(capture, KOUHAKU_HASH)?;
    for item in items(capture)? {
        checkpoint()?;
        Graph::new(&item["state"])?;
    }
    let start = one_start(capture)?;
    if start["sampleRate"] != 22050 || start["stereo"] != false || start["latencySeek"] != 1633 {
        return Err(invalid("Unreviewed Kouhaku stream format/latency"));
    }
    let iteration = start["iteration"]
        .as_u64()
        .ok_or_else(|| invalid("Missing audio iteration"))?;
    let origin = at(capture, iteration)?;
    let menu = items(capture)?
        .iter()
        .find(|i| i["frame"].as_u64().is_some_and(|n| n < iteration))
        .ok_or_else(|| invalid("Missing Kouhaku menu snapshot"))?;
    kouhaku_settings(origin, selection)?;
    let graph = Graph::new(&origin["state"])?;
    let before = Graph::new(&menu["state"])?;
    if before.root_frame()? != 4.0 {
        return Err(invalid("Unreviewed Kouhaku menu stage"));
    }
    let (root, menu_root) = (graph.root()?, before.root()?);
    let tree = display_tree(origin)?;
    let mut controllers = vec![];
    for (i, d) in tree.iter().enumerate() {
        if !d["object"].is_null() && graph.props(&d["object"])?.contains_key("left_timeline") {
            controllers.push(i);
        }
    }
    if controllers.len() != 1 {
        return Err(invalid("Need unique Kouhaku controller"));
    }
    let display = &tree[controllers[0]];
    let controller = graph.props(&display["object"])?;
    let parent = display["parent"]
        .as_u64()
        .filter(|n| *n < tree.len() as u64)
        .ok_or_else(|| invalid("Missing controller parent"))? as usize;
    if tree[parent]["movieFrame"] != 2 || number(&controller, "Cnt")? != 1.0 {
        return Err(invalid("Consumed or unreviewed Kouhaku controller"));
    }
    let clock_path = path(tree, parent)?;
    for elapsed in [1, 2] {
        let later = at(
            capture,
            iteration
                .checked_add(elapsed)
                .ok_or_else(|| invalid("Pinned iteration overflow"))?,
        )?;
        Graph::new(&later["state"])?;
        let following = find_path(display_tree(later)?, &clock_path)?;
        if following["movieFrame"].as_u64() != Some(2 + elapsed) {
            return Err(invalid("Kouhaku timeline does not advance at 60 Hz"));
        }
    }
    let mut lanes = vec![];
    for (name, _) in FIVE {
        checkpoint()?;
        let key = format!("{name}_data");
        let raw = text(get(&root, &key)?)?;
        if raw != text(get(&menu_root, &key)?)? {
            return Err(invalid("Kouhaku source selection mutated"));
        }
        let source = raw
            .split(',')
            .map(|s| {
                if s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()) {
                    return Err(invalid("Unexpected Kouhaku frame syntax"));
                }
                s.parse::<u64>()
                    .ok()
                    .filter(|n| 150 < *n && *n < 100000)
                    .ok_or_else(|| invalid("Invalid Kouhaku frame"))
            })
            .collect::<Result<Vec<_>>>()?;
        if source.windows(2).any(|p| p[0] >= p[1])
            || nums(&graph, get(&controller, &format!("{name}_timeline"))?)?
                != source.iter().map(|n| *n as f64).collect::<Vec<_>>()
        {
            return Err(invalid("Kouhaku source/compiled arrays differ"));
        }
        lanes.push(json!({"name":name,"judgementFrames":source,"spawnMovieFrames":source.iter().map(|n|n-150).collect::<Vec<_>>(),"holdFrames":[],"tapsSeconds":source.iter().map(|n|(*n as f64-2.0)/60.0).collect::<Vec<_>>(),"holdsSeconds":[]}));
    }
    let raw_difficulty = text(get(&root, "difficulty")?)?;
    let difficulty_stars = raw_difficulty.chars().filter(|c| *c == '★').count();
    let (id, title, artist, sprite, _) = selection.details();
    Ok(
        json!({"schema":"ddi-danoni-avm1-reference/experimental-1","adapter":"iso50-kouhaku-2004/2","id":id,"sourceSwfSha256":KOUHAKU_HASH,"ruffle":capture["build"],"fps":60,"metadata":{"title":title,"artist":artist,"difficultyStars":difficulty_stars,"rawDifficulty":raw_difficulty,"difficultyClassification":null,"keyMode":"5","chartAuthor":null},"audioOrigin":{"logicalFrame":2,"iteration":iteration,"outputSampleFrame":start["outputSampleFrame"],"stream":start,"sprite":sprite,"scope":"Nominal native frame grid anchored to the Ruffle stream start; not pixel-perfect receptor-centre timing or original Flash seek compensation"},"stages":{"menu":menu["frame"],"initialized":iteration},"lanes":lanes,"speed":[],"boost":[],"colors":[],"sourceCompiledAgreement":true,"limits":["Pinned six-chart collection and default movement only","Original Flash MP3 latency and timeline seek behavior unverified","Ruffle stream decoder ignores declared 1633-sample delay","Nominal chart frames preserved; hit-box geometry and judge/gauge behavior not reproduced","Chart author not identified; credits retained from original page and runtime"]}),
    )
}
pub(super) fn project(
    capture: &Value,
    options: &ProjectionOptions,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Projection> {
    if options.mode != Mode::Five
        || !options.source_prefix.is_empty()
        || options.source_suffix.is_some()
        || options.review_missing_decorative_sprites
    {
        return Err(invalid("Unreviewed pinned adapter options"));
    }
    let reference = match options.family {
        Family::HiganPilot => higan_reference(capture, checkpoint)?,
        Family::Kouhaku2004 => kouhaku_reference(
            capture,
            options
                .pinned_selection
                .ok_or_else(|| invalid("Missing pinned Kouhaku selection"))?,
            checkpoint,
        )?,
        _ => return Err(invalid("Not a pinned family")),
    };
    let lanes=array(&reference["lanes"])?.iter().map(|l|json!({"name":l["name"],"nativeFrames":l["judgementFrames"],"nativeHoldFrames":l["holdFrames"]})).collect::<Vec<_>>();
    let mut chart = json!({"mode":"5","lanes":lanes,"speed":reference["speed"],"boost":reference["boost"],"colors":reference["colors"],"stops":reference.get("stops").cloned().unwrap_or_else(||json!([])),"metadata":reference["metadata"],"pinnedReference":reference});
    let prepared = if options.family == Family::HiganPilot {
        chart["clockField"] = json!("frame_num");
        chart["initialPixelSpeed"] = reference["metadata"]["initialPixelSpeed"].clone();
        reference["stages"]["prepared"].as_u64().unwrap()
    } else {
        let start = at(
            capture,
            reference["audioOrigin"]["iteration"].as_u64().unwrap(),
        )?;
        let g = Graph::new(&start["state"])?;
        let tree = display_tree(start)?;
        let controller = tree
            .iter()
            .find(|d| {
                !d["object"].is_null()
                    && g.props(&d["object"])
                        .is_ok_and(|p| p.contains_key("left_timeline"))
            })
            .ok_or_else(|| invalid("Missing timing controller"))?;
        chart["pinnedClockPath"] =
            json!(path(tree, controller["parent"].as_u64().unwrap() as usize)?);
        chart["clockField"] = json!("pinned-child-timeline");
        chart["initialPixelSpeed"] = json!(2);
        chart["difficultyStars"] = reference["metadata"]["difficultyStars"].clone();
        reference["stages"]["initialized"].as_u64().unwrap()
    };
    Ok(Projection {
        prepared_iteration: prepared,
        chart,
    })
}

fn clock_path(chart: &Value) -> Result<Vec<String>> {
    array(&chart["pinnedClockPath"])?
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| s.len() <= 8192)
                .map(str::to_owned)
                .ok_or_else(|| invalid("Invalid pinned clock path"))
        })
        .collect()
}
fn selected_display<'a>(tree: &'a [Value], name: &str) -> Result<(usize, &'a Value)> {
    let matching = tree
        .iter()
        .enumerate()
        .filter(|(_, d)| {
            d["kind"] == "movie-clip"
                && d["visible"] == true
                && !d["name"].is_null()
                && text(&d["name"]).is_ok_and(|n| n == name)
        })
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err(invalid("Missing/ambiguous pinned display"));
    }
    Ok(matching[0])
}
fn bounds(display: &Value) -> Result<[f64; 4]> {
    let b = array(&display["stageBoundsPixels"])?;
    if b.len() != 4 {
        return Err(invalid("Missing pinned stage bounds"));
    }
    let mut out = [0.0; 4];
    for (o, v) in out.iter_mut().zip(b) {
        *o = numeric(v)?;
        if o.abs() > 1e9 {
            return Err(invalid("Unbounded pinned coordinate"));
        }
    }
    if out[2] <= out[0] || out[3] <= out[1] {
        return Err(invalid("Empty pinned bounds"));
    }
    Ok(out)
}
fn validate_motion_settings(
    item: &Value,
    options: &ProjectionOptions,
    chart: &Value,
) -> Result<f64> {
    let graph = Graph::new(&item["state"])?;
    match options.family {
        Family::HiganPilot => {
            let root = graph.root()?;
            strings_equal(&root, "motion", "OFF")?;
            strings_equal(&root, "keyLabel", "5")?;
            if number(&root, "difficulty")? != 1.0
                || number(&root, "correction")? != 0.0
                || number(&root, "set_spi")? != numeric(&chart["initialPixelSpeed"])?
            {
                return Err(invalid("Unreviewed Higan motion settings"));
            }
            number(&root, "frame_num")
        }
        Family::Kouhaku2004 => {
            kouhaku_settings(
                item,
                options
                    .pinned_selection
                    .ok_or_else(|| invalid("Missing pinned selection"))?,
            )?;
            let d = find_path(display_tree(item)?, &clock_path(chart)?)?;
            numeric(&d["movieFrame"])
        }
        _ => Err(invalid("Not a pinned motion family")),
    }
}
/// Pinned historical adapters measure translating bounds and reference width.
/// They deliberately retain their original scope rather than claiming a proof
/// about receptor-centre judgement geometry from a bounding-box witness.
pub(super) fn motion(
    capture: &Value,
    chart: &Value,
    options: &ProjectionOptions,
    selection: &MotionSelection,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Value> {
    check(
        capture,
        if options.family == Family::HiganPilot {
            HIGAN_HASH
        } else {
            KOUHAKU_HASH
        },
    )?;
    if selection.iterations.len() < 3
        || selection.iterations.len() > 4096
        || selection.iterations.windows(2).any(|p| p[0] >= p[1])
        || selection.target_y.is_some()
        || selection.lane >= 5
    {
        return Err(invalid(
            "Pinned motion requires ordered lane observations and native bounds",
        ));
    }
    let lane_name = FIVE[selection.lane].0;
    let (note_prefix, expected_receptor, expected_geometry) =
        if options.family == Family::HiganPilot {
            if selection.lane != 0 {
                return Err(invalid("Pinned Higan motion uses the left arrow"));
            }
            ("le_s".to_string(), "le_step".to_string(), "le_step")
        } else {
            (
                format!("{lane_name}MC"),
                format!("{lane_name}_step"),
                "left_step",
            )
        };
    if !selection
        .note
        .strip_prefix(&note_prefix)
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()))
        || selection.receptor != expected_receptor
        || selection
            .geometry_reference
            .as_deref()
            .unwrap_or(expected_geometry)
            != expected_geometry
    {
        return Err(invalid("Unreviewed pinned arrow/reference selection"));
    }
    let raw = numeric(
        array(&chart["lanes"][selection.lane]["nativeFrames"])?
            .first()
            .ok_or_else(|| invalid("No pinned first tap"))?,
    )?;
    let initial = numeric(&chart["initialPixelSpeed"])?;
    if initial <= 0.0 {
        return Err(invalid("Invalid pinned initial speed"));
    }
    let mut samples = vec![];
    let mut identities = None;
    let mut first_geometry = None;
    for &frame in &selection.iterations {
        checkpoint()?;
        let item = at(capture, frame)?;
        let tree = display_tree(item)?;
        let (ni, note) = selected_display(tree, &selection.note)?;
        let (ri, receptor) = selected_display(tree, &selection.receptor)?;
        let (gi, geometry) = selected_display(
            tree,
            selection
                .geometry_reference
                .as_deref()
                .unwrap_or(expected_geometry),
        )?;
        let paths = [path(tree, ni)?, path(tree, ri)?, path(tree, gi)?];
        if identities.as_ref().is_some_and(|p| *p != paths) {
            return Err(invalid("Pinned display ancestry changed"));
        }
        identities = Some(paths);
        let (nb, rb, gb) = (bounds(note)?, bounds(receptor)?, bounds(geometry)?);
        let measures = [
            nb[2] - nb[0],
            nb[3] - nb[1],
            gb[2] - gb[0],
            gb[3] - gb[1],
            rb[0],
            rb[1],
            rb[2],
            rb[3],
        ];
        if first_geometry.as_ref().is_some_and(|first: &[f64; 8]| {
            first
                .iter()
                .zip(measures)
                .any(|(a, b)| (a - b).abs() > 1e-6)
        }) {
            return Err(invalid("Pinned arrow shape/reference bounds changed"));
        }
        first_geometry = Some(measures);
        let logical = validate_motion_settings(item, options, chart)?;
        if options.family == Family::HiganPilot {
            for event in array(&chart["speed"])? {
                if numeric(&event["nativeFrame"])? <= logical
                    && numeric(&event["multiplier"])? != 1.0
                {
                    return Err(Error::new(
                        "native-motion-segment-unproved",
                        "Pinned Higan motion must precede nonunit speed events",
                    ));
                }
            }
        }
        samples.push(
            json!({"iteration":frame,"movieFrame":logical,"x":nb[0],"y":nb[1],"targetY":rb[1]}),
        );
    }
    let first = &samples[0];
    let first_clock = numeric(&first["movieFrame"])?;
    let first_iteration = numeric(&first["iteration"])?;
    let first_x = numeric(&first["x"])?;
    let first_y = numeric(&first["y"])?;
    for sample in &samples {
        let ticks = numeric(&sample["movieFrame"])? - first_clock;
        if ticks != numeric(&sample["iteration"])? - first_iteration
            || (numeric(&sample["x"])? - first_x).abs() > 1e-6
            || (numeric(&sample["y"])? - (first_y - initial * ticks)).abs() > 0.05 + 1e-9
        {
            return Err(invalid(
                "Pinned movement/clock disagrees with initialized pixel increment",
            ));
        }
    }
    let width = first_geometry.unwrap()[2];
    let arrival = first_clock + (first_y - numeric(&first["targetY"])?) / initial;
    let base = initial * 60.0 / width;
    if !base.is_finite() {
        return Err(invalid("Pinned scroll overflow"));
    }
    let mut report = json!({"schema":"ddi-native-motion-measurement/experimental-1","policy":"pinned-bounds-width/1","sourceSwfSha256":capture["source"]["movieSha256"],"fps":60,"pixelsPerSecond":initial*60.0,"referencePixels":width,"baseArrowHeightsPerSecond":base,"observedAuthorMultiplier":1,"noteName":selection.note,"receptorName":selection.receptor,"geometryReferenceName":selection.geometry_reference.as_deref().unwrap_or(expected_geometry),"clockField":chart["clockField"],"stagePositionTolerancePixels":0.05,"arrivalDifferenceFrames":arrival-raw,"rawFirstFrame":raw,"observedArrivalMovieFrame":arrival,"arrivalProved":false,"samples":samples,"scope":"Pinned default upward translation and source reference width; nominal source frames retained, receptor-centre timing and original Flash/device latency unverified"});
    if options.family == Family::Kouhaku2004 {
        report["clockPath"] = json!(clock_path(chart)?);
    }
    Ok(report)
}
pub(super) fn audio_origin(
    capture: &Value,
    chart: &Value,
    options: &ProjectionOptions,
    index: usize,
) -> Result<Value> {
    check(
        capture,
        if options.family == Family::HiganPilot {
            HIGAN_HASH
        } else {
            KOUHAKU_HASH
        },
    )?;
    if index != 0 {
        return Err(invalid("Pinned work must have exactly one stream start"));
    }
    let start = one_start(capture)?;
    let item = at(
        capture,
        start["iteration"]
            .as_u64()
            .ok_or_else(|| invalid("Invalid pinned stream iteration"))?,
    )?;
    let logical = validate_motion_settings(item, options, chart)?;
    let expected = if options.family == Family::HiganPilot {
        145.0
    } else {
        2.0
    };
    if logical != expected {
        return Err(invalid("Pinned logical music origin differs"));
    }
    if options.family == Family::HiganPilot {
        if start["latencySeek"] != 0 {
            return Err(invalid("Higan stream latency differs"));
        }
        let (_, music) = selected_display(display_tree(item)?, "music_act")?;
        if music["movieFrame"] != 2 {
            return Err(invalid("Higan music clip origin differs"));
        }
    } else if start["sampleRate"] != 22050
        || start["stereo"] != false
        || start["latencySeek"] != 1633
    {
        return Err(invalid("Kouhaku stream metadata differs"));
    }
    let mut result = start.clone();
    result["logicalFrame"] = json!(logical);
    result["streamStartIndex"] = json!(0);
    result["clock"] = chart["clockField"].clone();
    result["scope"] = chart["pinnedReference"]["audioOrigin"]["scope"].clone();
    if options.family == Family::Kouhaku2004 {
        result["clockPath"] = json!(clock_path(chart)?);
    }
    Ok(result)
}
pub(super) fn validate_demux(report: &Value, options: &ProjectionOptions) -> Result<()> {
    let path = if options.family == Family::HiganPilot {
        "root/sprite1602".to_string()
    } else {
        format!(
            "root/sprite{}",
            options
                .pinned_selection
                .ok_or_else(|| invalid("Missing pinned selection"))?
                .details()
                .3
        )
    };
    if report["path"] != path
        || report["ruffleContinuousPayloadEligible"] != true
        || (options.family == Family::HiganPilot && report["continuousZeroLatencyEligible"] != true)
    {
        return Err(invalid("Pinned stream route/continuity differs"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn string(s: &str) -> Value {
        json!({"type":"string","utf16":s.encode_utf16().collect::<Vec<_>>(),"truncated":false})
    }
    fn witness() -> (Value, Value, ProjectionOptions, MotionSelection) {
        let captures = (10..13).map(|iteration| {
            let properties=[("frame_num",json!(iteration+135)),("set_spi",json!(3)),("difficulty",json!(1)),("correction",json!(0)),("keyLabel",string("5")),("motion",string("OFF"))].into_iter().map(|(name,value)|json!({"name":string(name),"value":value})).collect::<Vec<_>>();
            let note_y=200.0-3.0*(iteration-10) as f64;
            json!({"frame":iteration,"state":{"incomplete":false,"avm1Halted":false,"objects":[{"id":0,"truncated":false,"properties":properties}],"roots":[{"level":0,"value":{"ref":0},"movieFrame":149}],"displayTree":[{"id":0,"parent":null,"name":string(""),"kind":"movie-clip","visible":true,"object":{"ref":0}},{"id":1,"parent":0,"name":string("le_s0"),"kind":"movie-clip","visible":true,"stageBoundsPixels":[0,note_y,46,note_y+50.0]},{"id":2,"parent":0,"name":string("le_step"),"kind":"movie-clip","visible":true,"stageBoundsPixels":[0,20,46,70]}]}})
        }).collect::<Vec<_>>();
        (
            json!({"source":{"movieSha256":HIGAN_HASH},"swfHeaderFps":60,"avm1Halted":false,"audit":{"entries":[],"truncated":false,"runtimeErrors":0,"droppedEntries":0,"truncatedDetails":0},"captures":captures}),
            json!({"lanes":[{"nativeFrames":[205]}],"initialPixelSpeed":3,"speed":[],"clockField":"frame_num"}),
            ProjectionOptions {
                family: Family::HiganPilot,
                mode: Mode::Five,
                source_prefix: String::new(),
                source_suffix: None,
                pinned_selection: None,
                review_missing_decorative_sprites: false,
            },
            MotionSelection {
                note: "le_s0".into(),
                receptor: "le_step".into(),
                geometry_reference: None,
                lane: 0,
                iterations: vec![10, 11, 12],
                target_y: None,
            },
        )
    }
    #[test]
    fn pinned_bounds_require_identical_shape_translation_reference_and_counter() {
        let (capture, chart, options, selection) = witness();
        let report = motion(&capture, &chart, &options, &selection, &mut || Ok(())).unwrap();
        assert_eq!(report["referencePixels"], 46.0);
        assert_eq!(report["arrivalProved"], false);
        let mut bad = capture.clone();
        bad["captures"][1]["state"]["displayTree"][1]["stageBoundsPixels"][1] = json!(190);
        assert!(motion(&bad, &chart, &options, &selection, &mut || Ok(())).is_err());
        let mut bad = capture.clone();
        bad["captures"][1]["state"]["displayTree"][2]["stageBoundsPixels"][2] = json!(47);
        assert!(motion(&bad, &chart, &options, &selection, &mut || Ok(())).is_err());
        let mut bad = capture.clone();
        bad["captures"][1]["state"]["objects"][0]["properties"][0]["value"] = json!(150);
        assert!(motion(&bad, &chart, &options, &selection, &mut || Ok(())).is_err());
        let mut bad = selection.clone();
        bad.iterations = vec![10, 10, 12];
        assert!(motion(&capture, &chart, &options, &bad, &mut || Ok(())).is_err());
        let mut bad = selection;
        bad.note = "background0".into();
        assert!(motion(&capture, &chart, &options, &bad, &mut || Ok(())).is_err());
    }
    #[test]
    fn pinned_routes_keep_nonzero_kouhaku_latency_separate_from_payload_continuity() {
        let (_, _, mut options, _) = witness();
        let mut report = json!({"path":"root/sprite1602","ruffleContinuousPayloadEligible":true,"continuousZeroLatencyEligible":true});
        validate_demux(&report, &options).unwrap();
        report["continuousZeroLatencyEligible"] = json!(false);
        assert!(validate_demux(&report, &options).is_err());
        options.family = Family::Kouhaku2004;
        options.pinned_selection = Some(PinnedSelection::Tougenkyo);
        report["path"] = json!("root/sprite229");
        validate_demux(&report, &options).unwrap();
        report["path"] = json!("root/sprite232");
        assert!(validate_demux(&report, &options).is_err());
        report["path"] = json!("root/sprite229");
        report["ruffleContinuousPayloadEligible"] = json!(false);
        assert!(validate_demux(&report, &options).is_err());
    }
}
