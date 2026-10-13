//! Quantization with Python-compatible ties-to-even rounding and lane fallback.
use crate::{Error, Result, model::*};

pub const MAX_SOURCE_SECONDS: f64 = 7200.0;
pub const MAX_ENDPOINTS: usize = 1_000_000;

pub fn validate_grid(grid: &MusicalGrid) -> Result<()> {
    if !grid.bpm.is_finite()
        || !(60.0..=360.0).contains(&grid.bpm)
        || !grid.beat_zero_seconds.is_finite()
        || grid.beat_zero_seconds.abs() > 600.0
        || ![1, 2, 3, 4, 6, 8].contains(&grid.subdivisions_per_beat)
        || !grid.maximum_error_ms.is_finite()
        || !(0.0 < grid.maximum_error_ms && grid.maximum_error_ms < 10.0)
    {
        return Err(Error::new(
            "invalid-musical-grid",
            "Unsupported tempo, phase, subdivision or timing budget",
        ));
    }
    Ok(())
}
fn time(seconds: f64) -> Result<()> {
    if !seconds.is_finite() || seconds.abs() > MAX_SOURCE_SECONDS {
        return Err(Error::new(
            "invalid-time",
            "Need a finite time within +/-7200 seconds",
        ));
    }
    Ok(())
}
pub fn source_frame(seconds: f64) -> Result<i64> {
    time(seconds)?;
    let tick = (seconds * 60.0).round_ties_even() as i64;
    if (seconds - tick as f64 / 60.0).abs() > 1e-9 {
        return Err(Error::new(
            "non-frame-time",
            "Exact timing requires the source 60 Hz frame grid",
        ));
    }
    Ok(tick)
}

#[derive(Clone, Copy, Debug)]
pub struct Position {
    pub tick: i64,
    pub error_seconds: f64,
    pub snapped: bool,
}
pub fn position(seconds: f64, grid: &MusicalGrid, rhythmic: bool) -> Result<Position> {
    validate_grid(grid)?;
    time(seconds)?;
    let quantum = 60.0 / grid.bpm / 48.0;
    let fine = ((seconds - grid.beat_zero_seconds) / quantum).round_ties_even() as i64;
    let subdivision = 48 / i64::from(grid.subdivisions_per_beat);
    let coarse = (((seconds - grid.beat_zero_seconds) / (quantum * subdivision as f64))
        .round_ties_even() as i64)
        * subdivision;
    let snapped = rhythmic
        && (grid.beat_zero_seconds + coarse as f64 * quantum - seconds).abs()
            <= grid.maximum_error_ms / 1000.0 + 1e-12;
    let tick = if snapped { coarse } else { fine };
    let error_seconds = grid.beat_zero_seconds + tick as f64 * quantum - seconds;
    if error_seconds.abs() > grid.maximum_error_ms / 1000.0 + 1e-12 {
        return Err(Error::new(
            "timing-budget-exceeded",
            "Musical quantization exceeds its explicit timing budget",
        ));
    }
    Ok(Position {
        tick,
        error_seconds,
        snapped,
    })
}
fn source_relationships(lane: &Lane) -> Result<()> {
    for t in lane
        .taps_seconds
        .iter()
        .chain(lane.holds_seconds.iter().flatten())
    {
        time(*t)?;
    }
    let mut taps = lane.taps_seconds.clone();
    taps.sort_by(f64::total_cmp);
    let mut holds = lane.holds_seconds.clone();
    holds.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    let invalid = taps.windows(2).any(|w| w[0] == w[1])
        || holds.iter().any(|h| h[0] >= h[1])
        || holds.windows(2).any(|w| w[1][0] <= w[0][1])
        || overlapping_tap(&taps, &holds);
    if invalid {
        return Err(Error::new(
            "invalid-source-lane",
            "Duplicate taps, invalid holds or overlapping notes",
        ));
    }
    Ok(())
}
// Sorted taps allow bounded relationship checks without comparing every tap
// with every hold. Source and mapped checks use the same closed intervals.
fn overlapping_tap<T: PartialOrd>(taps: &[T], holds: &[[T; 2]]) -> bool {
    holds.iter().any(|h| {
        let index = taps.partition_point(|t| t < &h[0]);
        taps.get(index).is_some_and(|t| t <= &h[1])
    })
}
fn valid(lane: &QuantizedLane) -> bool {
    let mut holds = lane.holds.clone();
    holds.sort_unstable();
    let mut taps = lane.taps.clone();
    taps.sort_unstable();
    !taps.windows(2).any(|w| w[0] == w[1])
        && holds.iter().all(|h| h[0] < h[1])
        && holds.windows(2).all(|w| w[1][0] > w[0][1])
        && !overlapping_tap(&taps, &holds)
}
fn map_lane(
    lane: &Lane,
    grid: Option<&MusicalGrid>,
    rhythmic: bool,
) -> Result<(QuantizedLane, Vec<Position>)> {
    let positions = lane
        .taps_seconds
        .iter()
        .chain(lane.holds_seconds.iter().flatten())
        .map(|t| match grid {
            Some(grid) => position(*t, grid, rhythmic),
            None => Ok(Position {
                tick: source_frame(*t)?,
                error_seconds: 0.0,
                snapped: false,
            }),
        })
        .collect::<Result<Vec<_>>>()?;
    let taps = positions[..lane.taps_seconds.len()]
        .iter()
        .map(|p| p.tick)
        .collect();
    let holds = positions[lane.taps_seconds.len()..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| [pair[0].tick, pair[1].tick])
        .collect();
    Ok((
        QuantizedLane {
            taps,
            holds,
            collision_safeguard: false,
        },
        positions,
    ))
}
pub(crate) fn validate_source_chart(chart: &Chart) -> Result<()> {
    if chart.id.is_empty() || chart.id.len() > 4096 || chart.lanes.len() != chart.mode.lanes() {
        return Err(Error::new(
            "invalid-chart",
            "Need id and supported lane count",
        ));
    }
    let count = chart
        .lanes
        .iter()
        .map(|l| l.taps_seconds.len() + l.holds_seconds.len() * 2)
        .sum::<usize>();
    if count == 0 || count > MAX_ENDPOINTS {
        return Err(Error::new(
            "endpoint-limit",
            "Need bounded nonempty source chart",
        ));
    }
    for lane in &chart.lanes {
        source_relationships(lane)?;
    }
    Ok(())
}
pub fn quantize(
    chart: &Chart,
    grid: Option<&MusicalGrid>,
) -> Result<(Vec<QuantizedLane>, Vec<Endpoint>)> {
    if chart.id.is_empty() || chart.id.len() > 4096 || chart.lanes.len() != chart.mode.lanes() {
        return Err(Error::new(
            "invalid-chart",
            "Need an id and the exact supported mode's lane count",
        ));
    }
    if let Some(grid) = grid {
        validate_grid(grid)?;
    }
    let count: usize = chart
        .lanes
        .iter()
        .map(|l| l.taps_seconds.len() + l.holds_seconds.len() * 2)
        .sum();
    if count == 0 || count > MAX_ENDPOINTS {
        return Err(Error::new(
            "endpoint-limit",
            "Chart must contain 1..=1000000 endpoints",
        ));
    }
    let mut lanes = Vec::new();
    let mut endpoints = Vec::with_capacity(count);
    for (lane_index, lane) in chart.lanes.iter().enumerate() {
        source_relationships(lane)?;
        let (mut mapped, mut positions) = map_lane(lane, grid, true)?;
        if !valid(&mapped) && grid.is_some() {
            (mapped, positions) = map_lane(lane, grid, false)?;
            mapped.collision_safeguard = true;
        }
        if !valid(&mapped) {
            return Err(Error::new(
                "unrepresentable-lane",
                "Fine grid cannot preserve lane relationships",
            ));
        }
        let sources = lane
            .taps_seconds
            .iter()
            .enumerate()
            .map(|(i, t)| (EndpointKind::Tap, i, *t))
            .chain(lane.holds_seconds.iter().enumerate().flat_map(|(i, pair)| {
                [
                    (EndpointKind::HoldHead, i, pair[0]),
                    (EndpointKind::HoldTail, i, pair[1]),
                ]
            }));
        for ((kind, index, seconds), p) in sources.zip(positions) {
            let method = if grid.is_none() {
                QuantizationMethod::ExactSourceFrame
            } else if mapped.collision_safeguard {
                QuantizationMethod::LaneCollisionFineFallback
            } else if p.snapped {
                QuantizationMethod::RhythmicSubdivision
            } else {
                QuantizationMethod::FineTickFallback
            };
            endpoints.push(Endpoint {
                lane: lane_index,
                kind,
                index,
                source_seconds: seconds,
                export_seconds: seconds + p.error_seconds,
                shift_ms: p.error_seconds * 1000.0,
                tick: p.tick,
                method,
            });
        }
        lanes.push(mapped);
    }
    Ok((lanes, endpoints))
}
