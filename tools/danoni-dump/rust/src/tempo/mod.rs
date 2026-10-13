//! Musical estimates are evidence. Original note times and quantizer bounds
//! remain authoritative, including hold tails and unsupported effects.
mod job;
mod pulse;
use crate::{
    Error, Result,
    model::{Chart, MusicalGrid},
    timing,
};
pub use job::{AnalysisPcm, TempoRequest, prepare_tempo};
pub use pulse::{OnsetReport, onset_estimates};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
const SUBS: [u8; 6] = [1, 2, 3, 4, 6, 8];
const MAX_TIMES: usize = 16384;
fn invalid(s: impl Into<String>) -> Error {
    Error::new("invalid-tempo", s)
}
fn insufficient(s: impl Into<String>) -> Error {
    Error::new("insufficient-tempo", s)
}
fn validate_times(times: &[f64]) -> Result<()> {
    if times.is_empty()
        || times.len() > MAX_TIMES
        || times.iter().any(|t| !t.is_finite() || t.abs() > 7200.0)
    {
        return Err(invalid(
            "Need 1..16384 finite analysis times within source bounds",
        ));
    }
    Ok(())
}
pub fn distinct_heads(chart: &Chart) -> Result<Vec<f64>> {
    let mut v = chart
        .lanes
        .iter()
        .flat_map(|l| {
            l.taps_seconds
                .iter()
                .copied()
                .chain(l.holds_seconds.iter().map(|p| p[0]))
        })
        .collect::<Vec<_>>();
    v.sort_by(f64::total_cmp);
    v.dedup();
    validate_times(&v)?;
    Ok(v)
}
fn quantile(sorted: &[f64], q: f64) -> f64 {
    let at = (sorted.len() - 1) as f64 * q;
    let a = at.floor() as usize;
    let b = at.ceil() as usize;
    sorted[a] + (sorted[b] - sorted[a]) * at.fract()
}
fn response(times: &[f64], bpm: f64, sub: u8) -> (f64, f64) {
    let omega = std::f64::consts::TAU * bpm * sub as f64 / 60.0;
    let (mut x, mut y) = (0.0, 0.0);
    for t in times {
        let (s, c) = (omega * t).sin_cos();
        x += c;
        y += s;
    }
    (x / times.len() as f64, y / times.len() as f64)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartFit {
    pub bpm: f64,
    pub subdivisions_per_beat: u8,
    pub coherence: f64,
    pub grid_phase_seconds: f64,
    pub grid_period_seconds: f64,
    pub median_grid_residual_ms: f64,
    pub p95_grid_residual_ms: f64,
    pub within_half_source_frame_fraction: f64,
}
pub fn chart_fit(times: &[f64], bpm: f64, sub: u8) -> Result<ChartFit> {
    validate_times(times)?;
    if !bpm.is_finite() || !(1.0..=720.0).contains(&bpm) || !SUBS.contains(&sub) {
        return Err(invalid("Invalid lattice tempo/subdivision"));
    }
    let period = 60.0 / bpm / sub as f64;
    let (x, y) = response(times, bpm, sub);
    let phase = (y.atan2(x) / std::f64::consts::TAU * period).rem_euclid(period);
    let mut errors = times
        .iter()
        .map(|t| ((t - phase + period / 2.0).rem_euclid(period) - period / 2.0).abs())
        .collect::<Vec<_>>();
    errors.sort_by(f64::total_cmp);
    Ok(ChartFit {
        bpm,
        subdivisions_per_beat: sub,
        coherence: x.hypot(y).min(1.0),
        grid_phase_seconds: phase,
        grid_period_seconds: period,
        median_grid_residual_ms: quantile(&errors, 0.5) * 1000.0,
        p95_grid_residual_ms: quantile(&errors, 0.95) * 1000.0,
        within_half_source_frame_fraction: errors.iter().filter(|e| **e <= 1.0 / 120.0).count()
            as f64
            / errors.len() as f64,
    })
}
fn strong(f: &ChartFit) -> bool {
    f.coherence >= 0.85 && f.p95_grid_residual_ms <= 12.0
}
/// Same coarse/fine search as the prior policy, with alias-frequency responses
/// memoized. Work identity never participates in fitting.
pub fn chart_candidates(
    times: &[f64],
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Vec<ChartFit>> {
    validate_times(times)?;
    if times.len() < 8 {
        return Err(insufficient("Need eight distinct heads"));
    }
    let mut cache = vec![None; 60001];
    let mut output = Vec::new();
    for sub in SUBS {
        let mut values = Vec::new();
        for i in 0..=15000usize {
            if i.is_multiple_of(64) {
                checkpoint()?;
            }
            let units = (3000 + i) * sub as usize;
            let r = if units <= 60000 {
                *cache[units].get_or_insert_with(|| {
                    let (x, y) = response(times, (3000 + i) as f64 / 50.0, sub);
                    x.hypot(y)
                })
            } else {
                0.0
            };
            values.push(r);
        }
        let mut peaks = (0..values.len())
            .filter(|i| {
                60.0 / ((3000 + i) as f64 / 50.0) / sub as f64 * 60.0 >= 3.0
                    && values[*i] >= if *i == 0 { 0.0 } else { values[i - 1] }
                    && values[*i] >= values.get(i + 1).copied().unwrap_or(0.0)
            })
            .collect::<Vec<_>>();
        peaks.sort_by(|a, b| values[*b].total_cmp(&values[*a]).then(a.cmp(b)));
        let mut chosen = Vec::<f64>::new();
        for i in peaks {
            let bpm = (3000 + i) as f64 / 50.0;
            if chosen.iter().any(|b| (b - bpm).abs() < 1.0) {
                continue;
            }
            chosen.push(bpm);
            let mut best = None::<ChartFit>;
            for j in 0..=100 {
                let f = chart_fit(times, bpm - 0.025 + j as f64 * 0.0005, sub)?;
                if best.as_ref().is_none_or(|b| f.coherence > b.coherence) {
                    best = Some(f);
                }
            }
            output.push(best.unwrap());
            if chosen.len() == 2 {
                break;
            }
        }
    }
    output.sort_by(|a, b| b.coherence.total_cmp(&a.coherence));
    Ok(output)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioEstimate {
    pub bpm: f64,
    pub beat_times_seconds: Vec<f64>,
    #[serde(default)]
    pub tempo_prior_bpm: Option<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioPulse {
    pub bpm: f64,
    pub beat_zero_seconds: f64,
    pub retained_beats: usize,
    pub total_beats: usize,
    pub p95_beat_residual_ms: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refinement: Option<String>,
}
pub fn audio_pulse(e: &AudioEstimate) -> Result<Option<AudioPulse>> {
    if !e.bpm.is_finite() || !(20.0..=720.0).contains(&e.bpm) || e.beat_times_seconds.len() > 16384
    {
        return Err(invalid("Invalid audio beat estimate"));
    }
    let times = &e.beat_times_seconds;
    if times.iter().any(|t| !t.is_finite() || t.abs() > 7200.0)
        || times.windows(2).any(|w| w[0] >= w[1])
    {
        return Err(invalid("Audio beat times must be finite and ordered"));
    }
    if times.len() < 8 {
        return Ok(None);
    }
    let seed = 60.0 / e.bpm;
    let mut index = 0.0;
    let points = times
        .iter()
        .enumerate()
        .map(|(i, t)| {
            if i > 0 {
                index += ((t - times[i - 1]) / seed).round_ties_even().max(1.0);
            }
            (index, *t)
        })
        .collect::<Vec<_>>();
    let fit = |p: &[(f64, f64)]| {
        let mx = p.iter().map(|v| v.0).sum::<f64>() / p.len() as f64;
        let my = p.iter().map(|v| v.1).sum::<f64>() / p.len() as f64;
        let a = p.iter().map(|v| (v.0 - mx) * (v.1 - my)).sum::<f64>()
            / p.iter().map(|v| (v.0 - mx).powi(2)).sum::<f64>();
        (a, my - a * mx)
    };
    let mut kept = points.clone();
    for _ in 0..4 {
        let (period, phase) = fit(&kept);
        let mut residuals = kept
            .iter()
            .map(|(x, y)| (y - period * x - phase).abs())
            .collect::<Vec<_>>();
        residuals.sort_by(f64::total_cmp);
        let threshold = (residuals[residuals.len() / 2] * 3.0).max(0.03);
        let next = points
            .iter()
            .copied()
            .filter(|(x, y)| (y - period * x - phase).abs() <= threshold)
            .collect::<Vec<_>>();
        if next.len() < 8 || next == kept {
            break;
        }
        kept = next;
    }
    let (period, phase) = fit(&kept);
    if period <= 0.0 || !(60.0..=360.0).contains(&(60.0 / period)) {
        return Ok(None);
    }
    let mut errors = kept
        .iter()
        .map(|(x, y)| (y - period * x - phase).abs())
        .collect::<Vec<_>>();
    errors.sort_by(f64::total_cmp);
    let p95 = errors[((0.95 * errors.len() as f64) as usize).min(errors.len() - 1)] * 1000.0;
    let coarse = p95 > 40.0f64.max(period * 80.0);
    let (bpm, phase) = if coarse {
        let (x, y) = times.iter().fold((0.0, 0.0), |(x, y), t| {
            let (s, c) = (std::f64::consts::TAU * t / seed).sin_cos();
            (x + c, y + s)
        });
        (
            e.bpm,
            (y.atan2(x) / std::f64::consts::TAU * seed).rem_euclid(seed),
        )
    } else {
        (60.0 / period, phase.rem_euclid(period))
    };
    Ok(Some(AudioPulse {
        bpm,
        beat_zero_seconds: phase,
        retained_beats: kept.len(),
        total_beats: times.len(),
        p95_beat_residual_ms: p95,
        refinement: coarse.then(|| "coarse audio tempo; phase from circular beat evidence".into()),
    }))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartWindow {
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub notes: usize,
    #[serde(flatten)]
    pub fit: ChartFit,
}
pub fn dominant_grid(
    times: &[f64],
    windows: &[ChartWindow],
    tolerance: f64,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Value> {
    validate_times(times)?;
    if !tolerance.is_finite() || tolerance <= 0.0 || windows.len() > 720 {
        return Err(invalid("Invalid local window bounds"));
    }
    let eligible = (0..windows.len())
        .filter(|i| {
            windows[*i].fit.coherence >= 0.94 && windows[*i].fit.p95_grid_residual_ms <= 12.0
        })
        .collect::<Vec<_>>();
    if eligible.is_empty() {
        return Ok(
            json!({"status":"insufficient-stable-windows","excludedWindowIndices":(0..windows.len()).collect::<Vec<_>>()}),
        );
    }
    let mut cluster = Vec::new();
    let mut weight = (0, 0);
    for j in &eligible {
        let ids = eligible
            .iter()
            .copied()
            .filter(|i| (windows[*i].fit.bpm - windows[*j].fit.bpm).abs() <= tolerance)
            .collect::<Vec<_>>();
        let w = (
            ids.iter().map(|i| windows[*i].notes).sum::<usize>(),
            ids.len(),
        );
        if w > weight {
            weight = w;
            cluster = ids;
        }
    }
    let excluded = (0..windows.len())
        .filter(|i| !cluster.contains(i))
        .collect::<Vec<_>>();
    if cluster.len() < 2 {
        return Ok(
            json!({"status":"insufficient-consensus","inlierWindowIndices":cluster,"excludedWindowIndices":excluded}),
        );
    }
    let retained = times
        .iter()
        .copied()
        .filter(|t| {
            cluster
                .iter()
                .any(|i| windows[*i].start_seconds <= *t && *t < windows[*i].end_seconds)
        })
        .collect::<Vec<_>>();
    if retained.len() < 8 {
        return Err(insufficient("Too few retained stable heads"));
    }
    let centre = cluster
        .iter()
        .map(|i| windows[*i].fit.bpm * windows[*i].notes as f64)
        .sum::<f64>()
        / weight.0 as f64;
    let sub = windows[cluster[0]].fit.subdivisions_per_beat;
    let fit = refine_range(
        &retained,
        centre - 0.15,
        centre + 0.15,
        0.0005,
        sub,
        checkpoint,
    )?;
    let consistent = fit.coherence >= 0.94 && fit.p95_grid_residual_ms <= 12.0;
    Ok(
        json!({"status":if consistent{"dominant-portions-only"}else{"tempo-consensus-phase-unresolved"},"tempoConsensusBpm":centre,"phaseConsistent":consistent,"scope":"Excluded portions retain original notes/timing; not a whole-song tempo claim","inlierWindowIndices":cluster,"excludedWindowIndices":excluded,"retainedNotes":retained.len(),"totalNotes":times.len(),"retainedFraction":retained.len() as f64/times.len() as f64,"windowBpmTolerance":tolerance,"fit":fit,"automaticAcceptance":false}),
    )
}
fn refine_range(
    times: &[f64],
    min: f64,
    max: f64,
    step: f64,
    sub: u8,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<ChartFit> {
    let min = min.max(60.0);
    let max = max.min(360.0);
    let n = ((max - min) / step + 0.5).floor() as usize;
    let mut best = None::<ChartFit>;
    for i in 0..=n {
        if i.is_multiple_of(64) {
            checkpoint()?;
        }
        let f = chart_fit(times, min + i as f64 * step, sub)?;
        if best.as_ref().is_none_or(|b| f.coherence > b.coherence) {
            best = Some(f);
        }
    }
    best.ok_or_else(|| invalid("Empty refinement interval"))
}
pub fn local_windows(
    times: &[f64],
    seed: &ChartFit,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Vec<ChartWindow>> {
    validate_times(times)?;
    let min = times.iter().copied().min_by(f64::total_cmp).unwrap();
    let max = times.iter().copied().max_by(f64::total_cmp).unwrap();
    let mut out = Vec::new();
    let mut start = min;
    while start < max {
        let subset = times
            .iter()
            .copied()
            .filter(|t| start <= *t && *t < start + 20.0)
            .collect::<Vec<_>>();
        if subset.len() >= 8 {
            let fit = refine_range(
                &subset,
                seed.bpm - 2.0,
                seed.bpm + 2.0,
                0.01,
                seed.subdivisions_per_beat,
                checkpoint,
            )?;
            out.push(ChartWindow {
                start_seconds: start,
                end_seconds: start + 20.0,
                notes: subset.len(),
                fit,
            });
        }
        start += 20.0;
    }
    Ok(out)
}
fn score(bpm: f64, sub: u8, coherence: f64, pulses: &[AudioPulse]) -> f64 {
    let votes = pulses
        .iter()
        .map(|p| (1.0 - (bpm / p.bpm).ln().abs() / 0.04).max(0.0))
        .sum::<f64>()
        / pulses.len() as f64;
    let prior = 0.02 * (1.0 - (bpm / 150.0).ln().abs() / 0.8).max(0.0)
        - if [3, 6].contains(&sub) { 0.04 } else { 0.0 };
    0.75 * coherence + 0.25 * votes + prior
}
fn closest(bpm: f64, pulses: &[AudioPulse]) -> &AudioPulse {
    pulses
        .iter()
        .min_by(|a, b| {
            (a.bpm / bpm)
                .ln()
                .abs()
                .total_cmp(&(b.bpm / bpm).ln().abs())
        })
        .unwrap()
}
fn alias_phase(f: &ChartFit, p: &AudioPulse) -> f64 {
    let period = 60.0 / f.bpm;
    (0..f.subdivisions_per_beat)
        .map(|i| f.grid_phase_seconds + i as f64 * f.grid_period_seconds)
        .min_by(|a, b| {
            ((a - p.beat_zero_seconds + period / 2.0).rem_euclid(period) - period / 2.0)
                .abs()
                .total_cmp(
                    &((b - p.beat_zero_seconds + period / 2.0).rem_euclid(period) - period / 2.0)
                        .abs(),
                )
        })
        .unwrap()
        .rem_euclid(period)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthoredRange {
    pub minimum_bpm: f64,
    pub maximum_bpm: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackAnalysis {
    pub chart_candidates: Vec<ChartFit>,
    pub audio_estimates: Vec<AudioEstimate>,
    #[serde(default)]
    pub dominant_grid: Value,
    #[serde(default)]
    pub authored_bpm_range: Option<AuthoredRange>,
}
fn validate_track(t: &TrackAnalysis) -> Result<()> {
    if t.chart_candidates.len() > 256 || t.audio_estimates.len() > 16 {
        return Err(invalid("Too many analysis candidates"));
    }
    if t.chart_candidates.iter().any(|f| {
        !f.bpm.is_finite()
            || !(1.0..=720.0).contains(&f.bpm)
            || !SUBS.contains(&f.subdivisions_per_beat)
            || [
                f.coherence,
                f.grid_phase_seconds,
                f.grid_period_seconds,
                f.median_grid_residual_ms,
                f.p95_grid_residual_ms,
                f.within_half_source_frame_fraction,
            ]
            .iter()
            .any(|v| !v.is_finite())
            || !(0.0..=1.0 + 1e-12).contains(&f.coherence)
            || f.grid_period_seconds <= 0.0
            || f.median_grid_residual_ms < 0.0
            || f.p95_grid_residual_ms < 0.0
            || !(0.0..=1.0).contains(&f.within_half_source_frame_fraction)
    }) {
        return Err(invalid("Invalid chart fit evidence"));
    }
    if let Some(r) = &t.authored_bpm_range
        && (!r.minimum_bpm.is_finite()
            || !r.maximum_bpm.is_finite()
            || r.minimum_bpm <= 0.0
            || r.minimum_bpm > r.maximum_bpm)
    {
        return Err(invalid("Invalid authored BPM range"));
    }
    Ok(())
}
fn inside(bpm: f64, range: &Option<AuthoredRange>) -> bool {
    range
        .as_ref()
        .is_none_or(|r| r.minimum_bpm * 0.99 <= bpm && bpm <= r.maximum_bpm * 1.01)
}
fn grid(
    clock: (f64, f64, u8),
    method: &str,
    fit: Option<&ChartFit>,
    pulse: &AudioPulse,
    range: &Option<AuthoredRange>,
    extra: Value,
) -> Result<MusicalGrid> {
    let (bpm, phase, sub) = clock;
    let mut evidence = serde_json::Map::new();
    evidence.insert("estimationMethod".into(), json!(method));
    evidence.insert("chartFit".into(), json!(fit));
    evidence.insert("audioPulse".into(), json!(pulse));
    evidence.insert("authoredBpmRange".into(), json!(range));
    evidence.insert(
        "phaseStatus".into(),
        json!("Estimated subdivision phase; audio guides quarter-beat alias, not author metadata"),
    );
    evidence.insert(
        "confidenceScope".into(),
        json!("Fit/phase statistics are evidence, not a correctness probability"),
    );
    if let Some(m) = extra.as_object() {
        evidence.extend(m.clone());
    }
    let g = MusicalGrid {
        bpm,
        beat_zero_seconds: phase,
        subdivisions_per_beat: sub,
        maximum_error_ms: 9.0,
        evidence,
    };
    timing::validate_grid(&g)?;
    Ok(g)
}
pub fn select_grid(times: &[f64], track: &TrackAnalysis) -> Result<(MusicalGrid, Vec<AudioPulse>)> {
    validate_times(times)?;
    validate_track(track)?;
    let mut pulses = Vec::new();
    for e in &track.audio_estimates {
        if let Some(p) = audio_pulse(e)?
            && (60.0..=360.0).contains(&p.bpm)
        {
            pulses.push(p);
        }
    }
    if pulses.is_empty() {
        return Err(insufficient("No usable audio pulse"));
    }
    let mut candidates = track
        .chart_candidates
        .iter()
        .filter(|f| (60.0..=360.0).contains(&f.bpm))
        .map(|f| chart_fit(times, f.bpm, f.subdivisions_per_beat))
        .collect::<Result<Vec<_>>>()?;
    if track.dominant_grid["status"] == "dominant-portions-only"
        && track.dominant_grid["retainedFraction"]
            .as_f64()
            .unwrap_or(0.0)
            >= 0.35
    {
        let f: ChartFit = serde_json::from_value(track.dominant_grid["fit"].clone())
            .map_err(|e| invalid(e.to_string()))?;
        candidates.push(f);
    }
    for p in &pulses {
        for sub in SUBS {
            if 60.0 / p.bpm / sub as f64 * 60.0 >= 3.0 {
                candidates.push(chart_fit(times, p.bpm, sub)?);
            }
        }
    }
    let filtered = candidates
        .iter()
        .filter(|f| inside(f.bpm, &track.authored_bpm_range))
        .cloned()
        .collect::<Vec<_>>();
    if !filtered.is_empty() {
        candidates = filtered;
    }
    let chosen = candidates
        .into_iter()
        .max_by(|a, b| {
            score(a.bpm, a.subdivisions_per_beat, a.coherence, &pulses).total_cmp(&score(
                b.bpm,
                b.subdivisions_per_beat,
                b.coherence,
                &pulses,
            ))
        })
        .ok_or_else(|| insufficient("No viable tempo candidate"))?;
    let p = closest(chosen.bpm, &pulses);
    let joint = strong(&chosen) && (chosen.bpm / p.bpm).ln().abs() <= 0.02;
    let (bpm, phase, method) = if joint {
        (chosen.bpm, alias_phase(&chosen, p), "joint-chart-audio")
    } else {
        (
            p.bpm,
            p.beat_zero_seconds,
            "audio-pulse-with-fine-note-fallback",
        )
    };
    Ok((
        grid(
            (bpm, phase, chosen.subdivisions_per_beat),
            method,
            Some(&chosen),
            p,
            &track.authored_bpm_range,
            json!({"alternativePulses":pulses}),
        )?,
        pulses,
    ))
}
/// Independent charts get equal votes; dense charts do not dominate a cohort.
pub fn select_shared(
    charts: &BTreeMap<String, Vec<f64>>,
    track: &TrackAnalysis,
    pulses: &[AudioPulse],
    legacy: &MusicalGrid,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<(BTreeMap<String, MusicalGrid>, Value)> {
    validate_track(track)?;
    if pulses.iter().any(|p| {
        !p.bpm.is_finite()
            || !(60.0..=360.0).contains(&p.bpm)
            || !p.beat_zero_seconds.is_finite()
            || p.beat_zero_seconds.abs() > 600.0
    }) {
        return Err(invalid("Invalid shared audio pulse"));
    }
    for t in charts.values() {
        validate_times(t)?;
    }
    let fallback = || charts.keys().map(|k| (k.clone(), legacy.clone())).collect();
    let eligible = charts
        .iter()
        .filter(|(_, v)| v.len() >= 8)
        .collect::<Vec<_>>();
    if eligible.len() < 2 {
        return Ok((
            fallback(),
            json!({"status":"insufficient-independent-charts"}),
        ));
    }
    if pulses.is_empty() {
        return Err(insufficient("Shared tempo needs audio pulse evidence"));
    }
    let mut candidates = Vec::new();
    for sub in SUBS {
        let mut scores = vec![0.0; 15001];
        for (_, t) in &eligible {
            for (i, v) in scores.iter_mut().enumerate() {
                if i.is_multiple_of(128) {
                    checkpoint()?;
                }
                let (x, y) = response(t, 60.0 + i as f64 * 0.02, sub);
                *v += x.hypot(y) / eligible.len() as f64;
            }
        }
        let mut peaks = (0..scores.len())
            .filter(|i| {
                60.0 / (60.0 + *i as f64 * 0.02) / sub as f64 * 60.0 >= 3.0
                    && scores[*i] >= if *i == 0 { 0.0 } else { scores[i - 1] }
                    && scores[*i] >= scores.get(i + 1).copied().unwrap_or(0.0)
            })
            .collect::<Vec<_>>();
        peaks.sort_by(|a, b| scores[*b].total_cmp(&scores[*a]).then(a.cmp(b)));
        let mut chosen = Vec::<f64>::new();
        for i in peaks {
            let b = 60.0 + i as f64 * 0.02;
            if chosen.iter().any(|v| (v - b).abs() < 1.0) {
                continue;
            }
            chosen.push(b);
            candidates.push((b, sub));
            if chosen.len() == 2 {
                break;
            }
        }
    }
    for p in pulses {
        for sub in SUBS {
            if 60.0 / p.bpm / sub as f64 * 60.0 >= 3.0 {
                candidates.push((p.bpm, sub));
            }
        }
    }
    let filtered = candidates
        .iter()
        .copied()
        .filter(|(b, _)| inside(*b, &track.authored_bpm_range))
        .collect::<Vec<_>>();
    if !filtered.is_empty() {
        candidates = filtered;
    }
    let mut best = None;
    let mut ranked = Vec::new();
    let mut refined = BTreeMap::<(i64, i64, i64), BTreeMap<String, ChartFit>>::new();
    for (bpm, sub) in candidates {
        checkpoint()?;
        let key = (
            (bpm * sub as f64 * 100000.0).round_ties_even() as i64,
            ((bpm - bpm * 0.0015).max(60.0) * sub as f64 * 100000.0).round_ties_even() as i64,
            ((bpm + bpm * 0.0015).min(360.0) * sub as f64 * 100000.0).round_ties_even() as i64,
        );
        if let std::collections::btree_map::Entry::Vacant(entry) = refined.entry(key) {
            let mut fits = BTreeMap::new();
            for (id, times) in &eligible {
                fits.insert(
                    (*id).clone(),
                    refine_range(
                        times,
                        bpm - bpm * 0.0015,
                        bpm + bpm * 0.0015,
                        0.0005,
                        sub,
                        checkpoint,
                    )?,
                );
            }
            entry.insert(fits);
        }
        let fits = refined[&key]
            .iter()
            .map(|(id, f)| {
                let mut f = f.clone();
                f.bpm = f.bpm * f.subdivisions_per_beat as f64 / sub as f64;
                f.subdivisions_per_beat = sub;
                (id.clone(), f)
            })
            .collect::<BTreeMap<_, _>>();
        let c = fits.values().map(|f| f.coherence).sum::<f64>() / fits.len() as f64;
        let s = score(bpm, sub, c, pulses);
        ranked.push(
            json!({"bpm":bpm,"subdivisionsPerBeat":sub,"coherence":c,"score":s,"chartFits":fits}),
        );
        if best.as_ref().is_none_or(|(_, _, v, _)| s > *v) {
            best = Some((bpm, sub, s, fits));
        }
    }
    let (bpm, sub, _, mut fits) = best.ok_or_else(|| insufficient("No shared tempo candidate"))?;
    let mut supported = fits
        .iter()
        .filter(|(_, f)| strong(f))
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();
    let mut values = supported.iter().map(|id| fits[id].bpm).collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    let consensus = if values.is_empty() {
        bpm
    } else {
        quantile(&values, 0.5)
    };
    for (id, times) in &eligible {
        if (fits[*id].bpm / consensus - 1.0).abs() > 0.0015 {
            fits.insert(
                (*id).clone(),
                refine_range(
                    times,
                    consensus - consensus * 0.0015,
                    consensus + consensus * 0.0015,
                    0.0005,
                    sub,
                    checkpoint,
                )?,
            );
        }
    }
    supported = fits
        .iter()
        .filter(|(_, f)| strong(f))
        .map(|(k, _)| k.clone())
        .collect();
    let p = closest(consensus, pulses);
    let majority = supported.len() > eligible.len() / 2 && (consensus / p.bpm).ln().abs() <= 0.02;
    let status = if majority {
        "independent-chart-consensus"
    } else {
        "no-majority-chart-consensus"
    };
    let summary = json!({"status":status,"consensusBpm":consensus,"eligibleChartCount":eligible.len(),"supportingCharts":supported,"refinementFraction":0.0015});
    let evidence = json!({"summary":summary,"chosenBpm":bpm,"subdivisionsPerBeat":sub,"audioPulse":p,"candidates":ranked,"scope":"Independent chart phases and bounded tempo refinements; no source notes removed"});
    if !majority {
        return Ok((fallback(), evidence));
    }
    let mut grids = BTreeMap::new();
    for (id, times) in charts {
        let fit = fits.get(id);
        let support=fit.map(|f|{let chance=((1.0/60.0)/f.grid_period_seconds).min(1.0);let required=(chance+3.0*(chance*(1.0-chance)/times.len() as f64).sqrt()).max(0.35);json!({"supported":f.within_half_source_frame_fraction>=required,"distinctHeads":times.len(),"matchedFraction":f.within_half_source_frame_fraction,"uniformPhaseFraction":chance,"requiredFraction":required,"scope":"Uniform-phase safeguard; not musical correctness probability"})});
        let local = support.as_ref().is_some_and(|s| s["supported"] == true);
        let (bpm, phase, method) = if local {
            let f = fit.unwrap();
            (f.bpm, alias_phase(f, p), "independent-chart-phase")
        } else {
            (
                consensus,
                p.beat_zero_seconds.rem_euclid(60.0 / consensus),
                "shared-tempo-with-fine-note-fallback",
            )
        };
        grids.insert(
            id.clone(),
            grid(
                (bpm, phase, sub),
                method,
                fit,
                p,
                &track.authored_bpm_range,
                json!({"phaseEvidence":support,"sharedTempo":summary}),
            )?,
        );
    }
    Ok((grids, evidence))
}
