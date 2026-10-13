//! SSC chart blocks. Musical colours and native motion are independent.
use crate::{Error, Result, model::*, policy, timing};
use std::{collections::BTreeMap, fmt::Write};

/// Escape StepMania tag characters; reject delimiters that cannot be represented.
pub fn header_value(value: &str) -> Result<String> {
    if value.chars().any(|c| matches!(c, ';' | '\r' | '\n')) {
        return Err(Error::new(
            "unsafe-header",
            "Header contains an unrepresentable delimiter",
        ));
    }
    let mut escaped = String::new();
    for c in value.chars() {
        if matches!(c, '\\' | ':' | '#' | '/') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    Ok(escaped)
}

/// Compile from normalized seconds and already validated review decisions.
/// The caller remains responsible for hash-binding reviews to source evidence.
pub fn compile_chart(
    chart: &Chart,
    metadata: &Metadata,
    options: &PreparationOptions,
) -> Result<PreparedChart> {
    let stars = match options.rating {
        RatingPolicy::Source => policy::source_stars(chart, metadata)?,
        RatingPolicy::Reviewed { stars } => stars,
    };
    let inferred_tier = policy::tier_for_stars(stars, options.tier_bounds)?;
    let tier = match options.tier_policy {
        TierPolicy::Stars => inferred_tier,
        TierPolicy::Edit => Tier::Edit,
    };
    let (lanes, endpoints) = timing::quantize(chart, options.grid.as_ref())?;
    let bpm = options.grid.as_ref().map_or(75.0, |g| g.bpm);
    let phase = options.grid.as_ref().map_or(0.0, |g| g.beat_zero_seconds);
    let quantum = 60.0 / bpm / 48.0;
    let base = policy::base_ratio(options.native_motion.as_ref(), bpm, options.scroll_factor)?;
    if chart.speed.len() > 100_000 {
        return Err(Error::new("event-limit", "Too many scroll events"));
    }
    let mut scroll_events = Vec::with_capacity(chart.speed.len());
    for event in &chart.speed {
        // Existing SSC policy requires source-frame alignment for speed events
        // even when notes use an estimated musical grid.
        let frame = timing::source_frame(event.seconds)?;
        let p = match options.grid.as_ref() {
            Some(grid) => timing::position(event.seconds, grid, false)?,
            None => timing::Position {
                tick: frame,
                error_seconds: 0.0,
                snapped: false,
            },
        };
        if !event.multiplier.is_finite() || !(event.multiplier * base).is_finite() {
            return Err(Error::new(
                "invalid-scroll-multiplier",
                "Scroll multiplier must be finite",
            ));
        }
        scroll_events.push(ScrollEvent {
            source_seconds: event.seconds,
            export_seconds: event.seconds + p.error_seconds,
            shift_ms: p.error_seconds * 1000.0,
            tick: p.tick,
            multiplier: event.multiplier,
        });
    }
    let first_tick = endpoints
        .iter()
        .map(|e| e.tick)
        .chain(scroll_events.iter().map(|e| e.tick))
        .min()
        .unwrap();
    let mut padding = (-first_tick).max(0);
    if options.grid.is_some() {
        padding = ((padding + 47) / 48) * 48;
    }
    if options.native_motion.is_some() && phase / quantum > padding as f64 {
        padding += (((phase / quantum - padding as f64) / 192.0).ceil() as i64) * 192;
    }
    let offset = padding as f64 * quantum - phase;
    let mut rows = BTreeMap::<i64, Vec<u8>>::new();
    for e in &endpoints {
        let row = rows
            .entry(e.tick + padding)
            .or_insert_with(|| vec![b'0'; chart.mode.lanes()]);
        if row[e.lane] != b'0' {
            return Err(Error::new(
                "conflicting-cell",
                "Multiple notes map to the same cell",
            ));
        }
        row[e.lane] = match e.kind {
            EndpointKind::Tap => b'1',
            EndpointKind::HoldHead => b'2',
            EndpointKind::HoldTail => b'3',
        };
    }
    let label = match chart.metadata.get("difficultyLabel") {
        None => "",
        Some(value) => value
            .as_str()
            .ok_or_else(|| Error::new("unsafe-label", "Difficulty label must be text"))?,
    };
    let escaped_label = header_value(label)?;
    let mut ssc = String::from("#NOTEDATA:;\n");
    if !label.is_empty() && (stars.is_none() || options.preserve_label) {
        writeln!(ssc, "#CHARTNAME:{escaped_label};").unwrap();
    }
    // Shortest round-trip f64 formatting preserves numerical meaning. Python's
    // .15g whitespace/decimal spelling is not an artifact identity guarantee.
    write!(ssc, "#STEPSTYPE:danoni-{};\n#DESCRIPTION:;\n#DIFFICULTY:{};\n#METER:{};\n#OFFSET:{};\n#BPMS:0={};\n",
        chart.mode.label(), tier.label(), stars.unwrap_or(0), offset, bpm).unwrap();
    let mut scroll = Vec::new();
    if options.native_motion.is_some() {
        scroll.push(format!("0={base}"));
    }
    for event in &scroll_events {
        scroll.push(format!(
            "{}={}",
            (event.tick + padding) as f64 / 48.0,
            event.multiplier * base
        ));
    }
    if !scroll.is_empty() {
        writeln!(ssc, "#SCROLLS:{};", scroll.join(",")).unwrap();
    }
    ssc.push_str("#NOTES:");
    let max_tick = *rows.last_key_value().unwrap().0;
    let measures = max_tick / 192 + 1;
    if measures > 20_000 {
        return Err(Error::new("row-limit", "Chart exceeds 20000 measures"));
    }
    let zero = "0".repeat(chart.mode.lanes());
    for measure in 0..measures {
        if measure != 0 {
            ssc.push(',');
        }
        ssc.push('\n');
        for i in 0..192 {
            if i != 0 {
                ssc.push('\n');
            }
            match rows.get(&(measure * 192 + i)) {
                Some(row) => ssc.push_str(std::str::from_utf8(row).unwrap()),
                None => ssc.push_str(&zero),
            }
        }
    }
    ssc.push_str(";\n");
    let mut unrepresented_effects = BTreeMap::new();
    for key in ["colors", "boost", "stops", "effects"] {
        if let Some(value) = chart.source.get(key) {
            let events = value
                .as_array()
                .ok_or_else(|| Error::new("invalid-effects", format!("{key} must be an array")))?;
            if !events.is_empty() {
                unrepresented_effects.insert(key.to_owned(), events.len());
            }
        }
    }
    let maximum_note_error_ms = endpoints
        .iter()
        .map(|e| e.shift_ms.abs())
        .fold(0.0, f64::max);
    Ok(PreparedChart {
        schema: "ddi-danoni-prepared-chart/1",
        id: chart.id.clone(),
        mode: chart.mode,
        difficulty_stars: stars,
        difficulty_classification: tier,
        timing_bpm: bpm,
        offset_seconds: offset,
        padding_ticks: padding,
        ssc_base_ratio: base,
        lanes,
        endpoints,
        scroll_events,
        maximum_note_error_ms,
        unrepresented_effects,
        ssc,
    })
}
