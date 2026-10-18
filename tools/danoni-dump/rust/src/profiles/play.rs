//! Bounded support for observing audio past an early hold intro. These are
//! source-derived physical key events, not gauge/clock/data patches or autoplay.
use super::*;
use std::collections::BTreeMap;
pub fn has_early_holds(chart: &Value) -> Result<bool> {
    let lanes = chart["lanes"]
        .as_array()
        .ok_or_else(|| invalid("Missing lanes"))?;
    let mut tap = f64::INFINITY;
    let mut hold = f64::INFINITY;
    for lane in lanes {
        for v in lane["nativeFrames"].as_array().into_iter().flatten() {
            tap = tap.min(extraction::numeric(v)?);
        }
        for p in lane["nativeHoldFrames"].as_array().into_iter().flatten() {
            hold = hold.min(extraction::numeric(&p[0])?);
        }
    }
    Ok(hold < tap)
}
/// Cover only holds beginning before the first tap, with two native iterations
/// after their tails. Unknown physical keys/overlong inputs are not guessed.
pub fn early_hold_inputs(
    chart: &Value,
    origin: &Value,
    after: u64,
    slots: usize,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Vec<Value>> {
    let lanes = chart["lanes"]
        .as_array()
        .ok_or_else(|| invalid("Missing lanes"))?;
    let indices = chart["nativeLaneIndices"]
        .as_array()
        .ok_or_else(|| invalid("Missing native lane indices"))?;
    let keys = chart["nativeKeyCodes"]
        .as_array()
        .ok_or_else(|| invalid("Missing native key codes"))?;
    if indices.len() != lanes.len() || lanes.len() > 7 || slots > 64 {
        return Err(invalid("Unproved input lane layout"));
    }
    let offset =
        extraction::numeric(&origin["logicalFrame"])? - extraction::numeric(&origin["iteration"])?;
    let mut first = f64::INFINITY;
    for lane in lanes {
        for v in lane["nativeFrames"].as_array().into_iter().flatten() {
            first = first.min(extraction::numeric(v)?);
        }
    }
    let mut by_key: BTreeMap<&str, Vec<(u64, u64)>> = BTreeMap::new();
    for (lane, index) in lanes.iter().zip(indices) {
        checkpoint()?;
        let index = extraction::numeric(index)?;
        if index.fract() != 0. || index < 0. || index >= keys.len() as f64 {
            return Err(invalid("Invalid native key index"));
        }
        for pair in lane["nativeHoldFrames"].as_array().into_iter().flatten() {
            let pair = pair
                .as_array()
                .filter(|p| p.len() == 2)
                .ok_or_else(|| invalid("Incomplete input hold pair"))?;
            let head = extraction::numeric(&pair[0])?;
            let tail = extraction::numeric(&pair[1])?;
            if head >= first {
                continue;
            }
            let code = match extraction::numeric(&keys[index as usize])? {
                37. => "ArrowLeft",
                38. => "ArrowUp",
                39. => "ArrowRight",
                40. => "ArrowDown",
                32. => "Space",
                _ => {
                    return Err(Error::new(
                        "native-profile-play-inputs-unavailable",
                        "Native key has no reviewed worker event",
                    ));
                }
            };
            let start = (head - offset).round();
            if start > 1200. {
                continue;
            }
            let end = (tail - offset).ceil() + 2.;
            if tail <= head
                || (head - offset - start).abs() > 1e-9
                || start <= after as f64
                || start < 1.
                || end > 1201.
                || end <= start
            {
                return Err(Error::new(
                    "native-profile-play-inputs-unavailable",
                    "Hold input lies outside the proved clock/initialization/worker interval",
                ));
            }
            by_key
                .entry(code)
                .or_default()
                .push((start as u64, end as u64));
        }
    }
    let mut events = vec![];
    for (code, mut spans) in by_key {
        spans.sort_unstable();
        let mut merged: Vec<(u64, u64)> = vec![];
        for (start, end) in spans {
            if let Some(last) = merged.last_mut()
                && start <= last.1
            {
                last.1 = last.1.max(end);
            } else {
                merged.push((start, end));
            }
        }
        for (start, end) in merged {
            if end - start > 120 || events.len() >= slots {
                return Err(Error::new(
                    "native-profile-play-inputs-unavailable",
                    "Derived inputs exceed existing hold/event bounds",
                ));
            }
            events.push(json!({"type":"key","frame":start,"code":code,"holdFrames":end-start}));
        }
    }
    events.sort_by(|a, b| {
        a["frame"]
            .as_u64()
            .cmp(&b["frame"].as_u64())
            .then(a["code"].as_str().cmp(&b["code"].as_str()))
    });
    Ok(events)
}
