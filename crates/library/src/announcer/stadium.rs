//! The studio and bundled packs' stadium-soft treatment. Measured responses
//! preserve the approved FFmpeg reference; no external effect engine is used.
//! Processing is offline, before playback. Channels remain independent.

use serde::Deserialize;
use std::sync::OnceLock;

use crate::fft::Fft;

pub const VERSION: &str = "announcer-stadium-rust-v1";
pub const RATE: u32 = 48_000;
pub const TAIL: f64 = 0.6;

#[derive(Deserialize)]
struct Saturation {
    drive_db: f64,
    mix: f32,
    tone_hz: f64,
    output_db: f32,
}
#[derive(Deserialize)]
struct Recipe {
    version: String,
    sample_rate: u32,
    tail_seconds: f64,
    opening_wet_fade_seconds: f64,
    direct_gain: f32,
    bounce_gain: f32,
    saturation: Saturation,
}
fn recipe() -> &'static Recipe {
    static RECIPE: OnceLock<Recipe> = OnceLock::new();
    RECIPE.get_or_init(|| {
        let r: Recipe = serde_json::from_str(include_str!(
            "../../../../tools/announcer-design/stadium.json"
        ))
        .expect("stadium recipe");
        assert_eq!(r.version, VERSION);
        assert_eq!(r.sample_rate, RATE);
        assert_eq!(r.tail_seconds, TAIL);
        r
    })
}

pub fn opening(line: Option<super::Line>) -> bool {
    matches!(
        line,
        Some(super::Line::Ready | super::Line::Restart | super::Line::HereWeGo)
    )
}

/// The duration after processing, before encoder rounding.
pub fn seconds(body: f64, line: Option<super::Line>) -> f64 {
    body + if opening(line) { 0.0 } else { TAIL }
}

/// FFT convolution in bounded blocks. IR spectra are prepared once per
/// processor; each channel uses its own zeroed overlap and input history.
struct Convolver {
    fft: Fft,
    kernels: Vec<(Vec<f32>, Vec<f32>)>,
}
const BLOCK: usize = 2048;
const FFT: usize = BLOCK * 2;

impl Convolver {
    fn new(response: &[f32]) -> Self {
        let fft = Fft::new(FFT);
        let kernels = response
            .chunks(BLOCK)
            .map(|chunk| {
                let mut re = vec![0.0; FFT];
                re[..chunk.len()].copy_from_slice(chunk);
                let mut im = vec![0.0; FFT];
                fft.run(&mut re, &mut im);
                (re, im)
            })
            .collect();
        Self { fft, kernels }
    }

    fn apply(&self, input: &[f32], len: usize) -> Vec<f32> {
        let mut history = vec![(vec![0.0; FFT], vec![0.0; FFT]); self.kernels.len()];
        let mut out = vec![0.0; len];
        let mut overlap = vec![0.0; BLOCK];
        let mut re = vec![0.0; FFT];
        let mut im = vec![0.0; FFT];
        for (block, start) in (0..len).step_by(BLOCK).enumerate() {
            let slot = block % history.len();
            let (xr, xi) = &mut history[slot];
            xr.fill(0.0);
            xi.fill(0.0);
            let end = (start + BLOCK).min(input.len());
            if start < end {
                xr[..end - start].copy_from_slice(&input[start..end]);
            }
            self.fft.run(xr, xi);
            re.fill(0.0);
            im.fill(0.0);
            for (k, (hr, hi)) in self.kernels.iter().enumerate().take(block + 1) {
                let (xr, xi) = &history[(slot + history.len() - k) % history.len()];
                for i in 0..FFT {
                    re[i] += xr[i] * hr[i] - xi[i] * hi[i];
                    im[i] += xr[i] * hi[i] + xi[i] * hr[i];
                }
            }
            // Inverse via the conjugated forward transform.
            for v in &mut im {
                *v = -*v;
            }
            self.fft.run(&mut re, &mut im);
            for i in 0..BLOCK {
                if start + i < len {
                    out[start + i] = re[i] / FFT as f32 + overlap[i];
                }
                overlap[i] = re[i + BLOCK] / FFT as f32;
            }
        }
        out
    }
}

struct Responses {
    eq: Convolver,
    space: Convolver,
}
static RESPONSES: OnceLock<Responses> = OnceLock::new();
pub const RESPONSE_VERSION: &str = env!("DDI_STADIUM_RESPONSE_VERSION");

/// The web fetches these only when the studio needs the effect; desktop
/// and the CLI embed them. They never enter the web's initial wasm payload.
pub fn loaded() -> bool {
    RESPONSES.get().is_some()
}
pub fn load(eq: &[u8], space: &[u8]) -> Result<(), String> {
    if loaded() {
        return Ok(());
    }
    let read = |bytes, len| -> Result<Vec<f32>, String> {
        let (mut pcm, rate) = ddi_platform::wav::read(bytes)?;
        if rate != RATE
            || pcm.len() != 1
            || pcm[0].len() != len
            || !pcm[0].iter().all(|s| s.is_finite())
        {
            return Err("Invalid stadium response".into());
        }
        Ok(pcm.remove(0))
    };
    let eq = read(eq, 9600)?;
    let space = read(space, 96000)?;
    let _ = RESPONSES.set(Responses {
        eq: Convolver::new(&eq),
        space: Convolver::new(&space),
    });
    Ok(())
}
fn responses() -> &'static Responses {
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    if !loaded() {
        load(
            include_bytes!("../../../../assets/announcer-filters/eq.wav"),
            include_bytes!("../../../../assets/announcer-filters/space.wav"),
        )
        .expect("built-in stadium responses");
    }
    RESPONSES
        .get()
        .expect("stadium responses loaded before processing")
}

/// A centered windowed-sinc conversion with no latency or channel mixing.
/// The filter is used only when the source differs from the effect's 48 kHz.
fn resample(input: &[f32], from: u32) -> Vec<f32> {
    if from == RATE {
        return input
            .iter()
            .map(|s| if s.is_finite() { *s } else { 0.0 })
            .collect();
    }
    if from == 0 || input.is_empty() {
        return Vec::new();
    }
    let ratio = f64::from(from) / f64::from(RATE);
    let len = (input.len() as f64 / ratio).round() as usize;
    let cutoff = (1.0 / ratio).min(1.0) * 0.97;
    let radius = (16.0 / cutoff).ceil() as isize;
    // Precompute the repeating fractional phases, rather than a sine for
    // every tap of every sample (44.1 kHz and 24 kHz are common inputs).
    let gcd = |mut a: u32, mut b: u32| {
        while b != 0 {
            (a, b) = (b, a % b);
        }
        a
    };
    let phases = RATE / gcd(from, RATE);
    let filters: Vec<Vec<f64>> = (0..phases)
        .map(|n| {
            let frac = (f64::from(n) * ratio).fract();
            let mut taps: Vec<f64> = (-radius..=radius)
                .map(|j| {
                    let x = (j as f64 - frac) * cutoff;
                    let sinc = if x.abs() < 1e-12 {
                        1.0
                    } else {
                        (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
                    };
                    let w = (j as f64 - frac) / (radius as f64 + 1.0);
                    sinc * (0.42
                        + 0.5 * (std::f64::consts::PI * w).cos()
                        + 0.08 * (2.0 * std::f64::consts::PI * w).cos())
                })
                .collect();
            let sum: f64 = taps.iter().sum();
            for tap in &mut taps {
                *tap /= sum;
            }
            taps
        })
        .collect();
    (0..len)
        .map(|n| {
            let center = (n as f64 * ratio).floor() as isize;
            filters[n % phases as usize]
                .iter()
                .enumerate()
                .map(|(j, w)| {
                    let i = center + j as isize - radius;
                    let i = i.clamp(0, input.len() as isize - 1) as usize;
                    f64::from(if input[i].is_finite() { input[i] } else { 0.0 }) * w
                })
                .sum::<f64>() as f32
        })
        .collect()
}

/// Apply creative processing only, to already prepared speech. Output is
/// always 48 kHz. Call `tidy::finished` once afterward, using the body's
/// duration (not the added tail), as both the studio and pack generator do.
pub fn apply<C: AsRef<[f32]>>(
    channels: &[C],
    rate: u32,
    line: Option<super::Line>,
) -> Vec<Vec<f32>> {
    let r = responses();
    let settings = recipe();
    let fade = (settings.opening_wet_fade_seconds * f64::from(RATE)).round() as usize;
    let timed = opening(line);
    channels
        .iter()
        .map(|channel| {
            let mut input = resample(channel.as_ref(), rate);
            let body = input.len();
            let len = body
                + if timed {
                    0
                } else {
                    (TAIL * f64::from(RATE)) as usize
                };
            input.resize(len, 0.0);
            let color = r.eq.apply(&input, len);
            let drive = 10f64.powf(settings.saturation.drive_db / 20.0);
            let norm = drive.tanh();
            let a = (-2.0 * std::f64::consts::PI * settings.saturation.tone_hz / f64::from(RATE))
                .exp() as f32;
            let output = 10f32.powf(settings.saturation.output_db / 20.0);
            let mut lp = 0.0f32;
            let color: Vec<f32> = color
                .into_iter()
                .map(|x| {
                    let wet = ((drive * f64::from(x)).tanh() / norm) as f32;
                    lp = wet + (lp - wet) * a;
                    (x * (1.0 - settings.saturation.mix) + lp * settings.saturation.mix) * output
                })
                .collect();
            let mut full = r.space.apply(&color, len);
            for i in 0..len {
                if timed && i >= body.saturating_sub(fade) {
                    let n = fade.min(body);
                    let remaining = (body - i - 1) as f32 / (n.saturating_sub(1).max(1)) as f32;
                    let direct = color[i] * settings.direct_gain;
                    full[i] = direct + (full[i] - direct) * remaining;
                }
                full[i] *= settings.bounce_gain;
            }
            full
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partitioned_convolution_matches_direct_across_block_edges() {
        let input: Vec<f32> = (0..5001)
            .map(|i| ((i * 47 % 127) as f32 - 63.0) / 100.0)
            .collect();
        let mut ir = vec![0.0; BLOCK * 2 + 9];
        for (i, v) in ir.iter_mut().enumerate() {
            *v = (i as f32 * 0.13).sin() * (-0.002 * i as f32).exp() * 0.01;
        }
        let actual = Convolver::new(&ir).apply(&input, input.len() + ir.len());
        for (n, a) in actual.iter().enumerate() {
            let expected: f32 = ir
                .iter()
                .enumerate()
                .filter_map(|(j, h)| n.checked_sub(j).and_then(|i| input.get(i)).map(|x| x * h))
                .sum();
            assert!((a - expected).abs() < 2e-5, "{n}: {a} vs {expected}");
        }
    }

    #[test]
    fn preserves_channels_silence_and_opening_duration() {
        let mut impulse = vec![0.0; 2400];
        impulse[100] = 0.2;
        let a = apply(&[&impulse, &vec![0.0; 2400]], 24000, None);
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].len(), 33600);
        assert!(a[1].iter().all(|s| *s == 0.0));
        let timed = apply(&[impulse], 24000, Some(super::super::Line::Ready));
        assert_eq!(timed[0].len(), 4800);
        assert_eq!(&a[0][..1900], &timed[0][..1900]);
    }
}
