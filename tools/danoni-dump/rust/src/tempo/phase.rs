//! Measured quarter-phase support for a precise chart subdivision lattice.
use super::*;
/// Compare every quarter alias against the actual onset envelope, using the
/// chart's refined period instead of a rounded audio lag. An unresolved alias
/// leaves the chosen phase intact and reports the ambiguity explicitly.
pub fn resolve_quarter_phase(
    grid: &mut MusicalGrid,
    report: &OnsetReport,
    heads: &[f64],
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<()> {
    timing::validate_grid(grid)?;
    validate_times(heads)?;
    if report.sample_rate != 22050
        || report.hop_samples != 64
        || report.envelope.is_empty()
        || report.envelope.len() > 300000
        || report.envelope.iter().any(|v| !v.is_finite() || *v < 0.0)
    {
        return Err(invalid("Invalid phase envelope"));
    }
    let rate = report.sample_rate as f64 / report.hop_samples as f64;
    let period = 60.0 / grid.bpm;
    let subdivision = period / grid.subdivisions_per_beat as f64;
    let start = heads.iter().copied().fold(f64::INFINITY, f64::min).max(0.0);
    let end = heads
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max)
        .min((report.envelope.len() - 1) as f64 / rate);
    let baseline = if end > start {
        let a = (start * rate).floor() as usize;
        let b = ((end * rate).ceil() as usize + 1).min(report.envelope.len());
        report.envelope[a..b].iter().sum::<f64>() / (b - a) as f64
    } else {
        0.0
    };
    let radius = (0.020 * rate).ceil() as isize;
    let kernel = (-radius..=radius)
        .map(|d| (-0.5 * (d as f64 / (0.007 * rate)).powi(2)).exp())
        .collect::<Vec<_>>();
    let kernel_total = kernel.iter().sum::<f64>();
    let mut alternatives = Vec::new();
    for alias in 0..grid.subdivisions_per_beat {
        let phase = (grid.beat_zero_seconds + alias as f64 * subdivision).rem_euclid(period);
        let mut beat = ((start - phase) / period).ceil();
        let mut strength = 0.0;
        let mut count = 0;
        while phase + beat * period <= end {
            if count % 128 == 0 {
                checkpoint()?;
            }
            let at = (phase + beat * period) * rate;
            let mut support = 0.0;
            for (d, weight) in (-radius..=radius).zip(&kernel) {
                let pos = at + d as f64;
                if pos >= 0.0 && pos < (report.envelope.len() - 1) as f64 {
                    let i = pos.floor() as usize;
                    support += weight
                        * (report.envelope[i] * (1.0 - pos.fract())
                            + report.envelope[i + 1] * pos.fract());
                }
            }
            strength += support / kernel_total;
            count += 1;
            beat += 1.0;
        }
        alternatives.push(json!({"beatZeroSeconds":phase,"meanOnsetSupport":if count>0{strength/count as f64}else{0.0},"observedQuarters":count}));
    }
    alternatives.sort_by(|a, b| {
        b["meanOnsetSupport"]
            .as_f64()
            .unwrap()
            .total_cmp(&a["meanOnsetSupport"].as_f64().unwrap())
    });
    let top = alternatives[0]["meanOnsetSupport"].as_f64().unwrap();
    let runner = alternatives
        .get(1)
        .map_or(top, |v| v["meanOnsetSupport"].as_f64().unwrap());
    let contrast = if top > 0.0 { (top - runner) / top } else { 0.0 };
    let count = alternatives[0]["observedQuarters"].as_u64().unwrap();
    let observed = top > baseline * 1.25 && count >= 8;
    let resolved = grid.subdivisions_per_beat > 1 && observed && contrast >= 0.10;
    let status = if resolved {
        "audio-supported-quarter-alias"
    } else if !observed {
        "insufficient-quarter-onset-support"
    } else {
        "ambiguous-quarter-alias"
    };
    if resolved {
        grid.beat_zero_seconds = alternatives[0]["beatZeroSeconds"].as_f64().unwrap();
    }
    grid.evidence.insert("nativeQuarterPhase".into(), json!({"algorithm":"ddi-quarter-onset-support/1","status":status,"relativeContrast":contrast,"baselineMeanOnset":baseline,"alternatives":alternatives,"sourceStartSeconds":start,"sourceEndSeconds":end,"scope":"Quarter-pulse support at the refined chart period; no downbeat/metre claim"}));
    grid.evidence.insert(
        "phaseStatus".into(),
        json!(if resolved {
            "Estimated quarter alias supported by observed audio; downbeat unverified"
        } else {
            "Quarter alias unresolved; provisional phase retained; downbeat unverified"
        }),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(equal: bool) -> (MusicalGrid, OnsetReport, Vec<f64>) {
        let rate = 22050.0 / 64.0;
        let mut envelope = vec![0.0; (12.0 * rate) as usize];
        for i in 0..44 {
            let at = (0.137 + i as f64 * 0.25) * rate;
            let strength = if equal || i % 2 == 0 { 2.0 } else { 0.4 };
            for (j, v) in envelope.iter_mut().enumerate() {
                *v += strength * (-0.5 * ((j as f64 - at) / 1.5).powi(2)).exp();
            }
        }
        let grid = MusicalGrid {
            bpm: 120.0,
            beat_zero_seconds: 0.387,
            subdivisions_per_beat: 4,
            maximum_error_ms: 9.0,
            evidence: serde_json::Map::new(),
        };
        let report = OnsetReport {
            algorithm: "synthetic measured onsets".into(),
            sample_rate: 22050,
            hop_samples: 64,
            fft_samples: 1024,
            envelope,
            estimates: vec![],
            scope: "Original synthetic signal".into(),
        };
        (
            grid,
            report,
            (0..80).map(|i| 0.137 + i as f64 * 0.125).collect(),
        )
    }
    #[test]
    fn strong_quarters_choose_the_right_alias_without_changing_note_times() {
        let (mut grid, report, heads) = fixture(false);
        let before = heads
            .iter()
            .map(|t| timing::position(*t, &grid, true).unwrap().error_seconds)
            .collect::<Vec<_>>();
        resolve_quarter_phase(&mut grid, &report, &heads, &mut || Ok(())).unwrap();
        assert!((grid.beat_zero_seconds - 0.137).abs() < 1e-9);
        assert_eq!(
            grid.evidence["nativeQuarterPhase"]["status"],
            "audio-supported-quarter-alias"
        );
        for (t, error) in heads.iter().zip(before) {
            assert!(
                (timing::position(*t, &grid, true).unwrap().error_seconds - error).abs() < 1e-9
            );
        }
    }
    #[test]
    fn equal_eighth_pulses_remain_explicitly_ambiguous() {
        let (mut grid, report, heads) = fixture(true);
        resolve_quarter_phase(&mut grid, &report, &heads, &mut || Ok(())).unwrap();
        assert_eq!(grid.beat_zero_seconds, 0.387);
        assert_eq!(
            grid.evidence["nativeQuarterPhase"]["status"],
            "ambiguous-quarter-alias"
        );
        assert!(
            grid.evidence["nativeQuarterPhase"]["relativeContrast"]
                .as_f64()
                .unwrap()
                < 0.10
        );
        let mut malformed = report;
        malformed.sample_rate = u32::MAX;
        assert!(resolve_quarter_phase(&mut grid, &malformed, &heads, &mut || Ok(())).is_err());
    }
}
