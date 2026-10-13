//! Versioned native spectral-flux estimator; implementation is distinct from
//! librosa. Confidence/aliases remain explicit in the policy layer.
use super::*;
use crate::audio::{self, Pcm16};
use rustfft::{FftPlanner, num_complex::Complex};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnsetReport {
    pub algorithm: String,
    pub sample_rate: u32,
    pub hop_samples: usize,
    pub fft_samples: usize,
    pub envelope: Vec<f64>,
    pub estimates: Vec<AudioEstimate>,
    pub scope: String,
}
/// Channel spectra are computed independently, avoiding anti-phase cancellation.
pub fn onset_estimates(
    pcm: &Pcm16,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<OnsetReport> {
    let (audio, _) = audio::resample(pcm, 22050, false, checkpoint)?;
    let n = 1024;
    let hop = 64;
    let frames = audio.frames().div_ceil(hop);
    let mut envelope = vec![0f64; frames];
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(n);
    let window = (0..n)
        .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos())
        .collect::<Vec<_>>();
    for ch in 0..audio.channels {
        let mut previous = vec![0f64; 64];
        for (frame, out) in envelope.iter_mut().enumerate() {
            if frame.is_multiple_of(64) {
                checkpoint()?;
            }
            let mut buffer = vec![Complex::new(0.0, 0.0); n];
            for i in 0..n {
                let at = frame as isize * hop as isize + i as isize - n as isize / 2;
                if at >= 0 && (at as usize) < audio.frames() {
                    buffer[i].re = audio.samples[at as usize * audio.channels + ch] as f64
                        / 32768.0
                        * window[i];
                }
            }
            fft.process(&mut buffer);
            let mut bands = [0f64; 64];
            for (k, v) in buffer.iter().take(n / 2 + 1).enumerate().skip(1) {
                let hz = k as f64 * 22050.0 / n as f64;
                let band = ((1127.0 * (1.0 + hz / 700.0).ln())
                    / (1127.0 * (1.0 + 11025.0f64 / 700.0).ln())
                    * 64.0)
                    .floor()
                    .min(63.0) as usize;
                bands[band] += v.norm_sqr();
            }
            let mut strength = 0.0;
            for (i, power) in bands.iter().enumerate() {
                let value = (1.0 + power * 10.0).ln();
                strength += (value - previous[i]).max(0.0);
                previous[i] = value;
            }
            *out += strength / audio.channels as f64;
        }
    }
    // Local energy normalization prevents one loud section setting every vote.
    let radius = (22050.0 / hop as f64) as usize;
    let mut prefix = vec![0f64; frames + 1];
    for i in 0..frames {
        prefix[i + 1] = prefix[i] + envelope[i];
    }
    let raw = envelope.clone();
    for (i, v) in envelope.iter_mut().enumerate() {
        let a = i.saturating_sub(radius);
        let b = (i + radius + 1).min(frames);
        let mean = (prefix[b] - prefix[a]) / (b - a) as f64;
        *v = if mean > 1e-5 { raw[i] / mean } else { 0.0 };
    }
    let mut estimates = Vec::new();
    if envelope.iter().filter(|v| **v > 1.0).count() >= 8 {
        let min = (22050.0 / hop as f64 * 60.0 / 360.0).ceil() as usize;
        let max = (22050.0 / hop as f64).floor() as usize;
        let mut ac = Vec::new();
        let mean = envelope.iter().sum::<f64>() / frames as f64;
        for lag in min..=max.min(frames.saturating_sub(1)) {
            checkpoint()?;
            let (mut xy, mut xx, mut yy) = (0.0, 0.0, 0.0);
            for i in lag..frames {
                let x = envelope[i] - mean;
                let y = envelope[i - lag] - mean;
                xy += x * y;
                xx += x * x;
                yy += y * y;
            }
            let score = if xx * yy > 0.0 {
                xy / (xx * yy).sqrt()
            } else {
                0.0
            };
            ac.push((lag, score));
        }
        let peaks = (0..ac.len())
            .filter(|i| {
                ac[*i].1 >= if *i == 0 { -1.0 } else { ac[i - 1].1 }
                    && ac[*i].1 >= ac.get(i + 1).map_or(-1.0, |v| v.1)
            })
            .collect::<Vec<_>>();
        for prior in [60.0f64, 120.0, 180.0] {
            let Some(i) = peaks.iter().copied().max_by(|a, b| {
                let score = |i: usize| {
                    let bpm = 22050.0 / hop as f64 * 60.0 / ac[i].0 as f64;
                    ac[i].1 - 0.12 * (bpm / prior).log2().powi(2)
                };
                score(*a).total_cmp(&score(*b))
            }) else {
                continue;
            };
            if ac[i].1 <= 0.02 {
                continue;
            }
            let lag = ac[i].0;
            let bpm = 22050.0 / hop as f64 * 60.0 / lag as f64;
            let beats = beat_path(&envelope, lag, 22050.0 / hop as f64, checkpoint)?;
            if beats.len() >= 8 {
                estimates.push(AudioEstimate {
                    bpm,
                    beat_times_seconds: beats,
                    tempo_prior_bpm: Some(prior),
                });
            }
        }
    }
    // Priors are views of the same recording, rather than independent votes.
    let mut unique = Vec::<(AudioEstimate, AudioPulse)>::new();
    for estimate in estimates {
        let Some(p) = audio_pulse(&estimate)? else {
            continue;
        };
        if let Some(i) = unique
            .iter()
            .position(|(_, old)| (p.bpm / old.bpm).ln().abs() <= 0.005)
        {
            if p.p95_beat_residual_ms < unique[i].1.p95_beat_residual_ms {
                unique[i] = (estimate, p);
            }
        } else {
            unique.push((estimate, p));
        }
    }
    let estimates = unique.into_iter().map(|(e, _)| e).collect();
    Ok(OnsetReport{algorithm:"ddi-spectral-flux-dp/3".into(),sample_rate:22050,hop_samples:hop,fft_samples:n,envelope,estimates,scope:"Native log-spectral-flux/autocorrelation/beat-path evidence; not librosa parity or author BPM".into()})
}

// A cumulative, rather than discounted, path retains the whole recording.
// Gaussian local support tolerates an onset's finite width; interval tightness
// prevents strong subdivisions from pulling a quarter-beat path off tempo.
fn beat_path(
    envelope: &[f64],
    lag: usize,
    frames_per_second: f64,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Vec<f64>> {
    let frames = envelope.len();
    let mean = envelope.iter().sum::<f64>() / frames as f64;
    let deviation =
        (envelope.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / frames as f64).sqrt();
    if deviation < 1e-9 {
        return Ok(vec![]);
    }
    let sigma = (lag as f64 / 32.0).max(1.0);
    let radius = (sigma * 3.0).ceil() as usize;
    let kernel = (0..=radius)
        .map(|d| (-0.5 * (d as f64 / sigma).powi(2)).exp())
        .collect::<Vec<_>>();
    let mut local = vec![0.0; frames];
    for (t, v) in local.iter_mut().enumerate() {
        if t.is_multiple_of(256) {
            checkpoint()?;
        }
        for at in t.saturating_sub(radius)..=(t + radius).min(frames - 1) {
            *v += kernel[t.abs_diff(at)] * envelope[at] / deviation;
        }
    }
    let low = (lag as f64 * 0.5).floor().max(1.0) as usize;
    let high = (lag as f64 * 2.0).ceil() as usize;
    let penalties = (0..=high)
        .map(|d| {
            if d == 0 {
                f64::INFINITY
            } else {
                100.0 * (d as f64 / lag as f64).ln().powi(2)
            }
        })
        .collect::<Vec<_>>();
    let mut scores = vec![0.0; frames];
    let mut links = vec![None; frames];
    for t in 0..frames {
        if t.is_multiple_of(256) {
            checkpoint()?;
        }
        let mut best = (0.0, None);
        for d in low..=high.min(t) {
            let value = scores[t - d] - penalties[d];
            if value > best.0 {
                best = (value, Some(t - d));
            }
        }
        scores[t] = local[t] + best.0;
        links[t] = best.1;
    }
    let mut beats = Vec::new();
    // End at an observed local maximum, avoiding a predicted silent tail.
    let endpoint = (0..frames)
        .filter(|t| {
            envelope[*t] >= 0.3
                && local[*t] >= if *t == 0 { 0.0 } else { local[t - 1] }
                && local[*t] >= local.get(t + 1).copied().unwrap_or(0.0)
        })
        .max_by(|a, b| scores[*a].total_cmp(&scores[*b]));
    if let Some(mut t) = endpoint {
        loop {
            if envelope[t] >= 0.3 {
                beats.push(t as f64 / frames_per_second);
            }
            let Some(previous) = links[t] else {
                break;
            };
            t = previous;
        }
    }
    beats.reverse();
    Ok(beats)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cumulative_path_retains_quiet_later_beats_and_measured_phase() {
        let mut onset = vec![0.0; 6100];
        for i in 0..120 {
            onset[17 + i * 50] = if i < 20 { 2.0 } else { 0.4 };
        }
        let beats = beat_path(&onset, 50, 100.0, &mut || Ok(())).unwrap();
        assert_eq!(beats.len(), 120);
        assert!((beats[0] - 0.17).abs() < 0.001);
        assert!(beats.last().unwrap() > &59.0);
    }
    #[test]
    fn strong_offbeat_subdivisions_do_not_pull_the_quarter_path() {
        let mut onset = vec![0.0; 6100];
        for i in 0..120 {
            onset[17 + i * 50] = 2.0;
            onset[42 + i * 50] = 1.8;
        }
        let beats = beat_path(&onset, 50, 100.0, &mut || Ok(())).unwrap();
        let pulse = audio_pulse(&AudioEstimate {
            bpm: 120.0,
            beat_times_seconds: beats,
            tempo_prior_bpm: None,
        })
        .unwrap()
        .unwrap();
        assert!((pulse.bpm - 120.0).abs() < 0.01);
        assert!((pulse.beat_zero_seconds - 0.17).abs() < 0.005);
    }
}
