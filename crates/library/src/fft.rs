//! A radix-2 Fourier transform (and the real transform and downmix on
//! it), for the music's analyses: the chart
//! editor's audio view (`ddi_ui::state::editor::audio`) and the
//! generative backgrounds' ([`crate::music`]).

/// A radix-2 transform of `n` complex values (a power of two), its
/// twiddles and order.
#[derive(Clone)]
pub struct Fft {
    n: usize,
    reverse: Vec<u32>,
    twiddles: Vec<(f32, f32)>,
}

impl Fft {
    pub fn new(n: usize) -> Fft {
        let bits = n.trailing_zeros();
        let reverse = (0..n as u32)
            .map(|i| {
                if bits == 0 {
                    0
                } else {
                    i.reverse_bits() >> (32 - bits)
                }
            })
            .collect();
        let twiddles = (0..n / 2)
            .map(|k| {
                let a = -2.0 * std::f64::consts::PI * k as f64 / n as f64;
                (a.cos() as f32, a.sin() as f32)
            })
            .collect();
        Fft {
            n,
            reverse,
            twiddles,
        }
    }

    /// The values it transforms.
    pub fn len(&self) -> usize {
        self.n
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    /// In place.
    pub fn run(&self, re: &mut [f32], im: &mut [f32]) {
        let n = self.n;
        for (i, &j) in self.reverse.iter().enumerate().take(n) {
            let j = j as usize;
            if j > i {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= n {
            let half = len / 2;
            let stride = n / len;
            for start in (0..n).step_by(len) {
                for k in 0..half {
                    let (wr, wi) = self.twiddles[k * stride];
                    let (a, b) = (start + k, start + k + half);
                    let tr = re[b] * wr - im[b] * wi;
                    let ti = re[b] * wi + im[b] * wr;
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
            }
            len *= 2;
        }
    }
}

/// A transform of `n` real values (a power of two, at least 2) as a
/// complex one of `n / 2`, even values real and odd imaginary, unpacked
/// into each bin's power with its twiddles: the music's analyses' step.
#[derive(Clone)]
pub struct RealFft {
    fft: Fft,
    /// The unpacking's twiddles, a bin each (`0..=n / 2`).
    turns: Vec<(f32, f32)>,
    re: Vec<f32>,
    im: Vec<f32>,
}

impl RealFft {
    /// A transform of `n` values, a power of two of at least 2.
    pub fn new(n: usize) -> RealFft {
        assert!(
            n >= 2 && n.is_power_of_two(),
            "RealFft needs a power of two of at least 2, not {n}"
        );
        let half = n / 2;
        RealFft {
            fft: Fft::new(half),
            turns: (0..=half)
                .map(|k| {
                    let a = -2.0 * std::f64::consts::PI * k as f64 / n as f64;
                    (a.cos() as f32, a.sin() as f32)
                })
                .collect(),
            re: vec![0.0; half],
            im: vec![0.0; half],
        }
    }

    /// Transforms the `n` values `sample(i)` gives (windowed already);
    /// [`RealFft::power`] then reads its bins.
    pub fn run(&mut self, sample: impl Fn(usize) -> f32) {
        for m in 0..self.re.len() {
            self.re[m] = sample(2 * m);
            self.im[m] = sample(2 * m + 1);
        }
        self.fft.run(&mut self.re, &mut self.im);
    }

    /// Bin `k`'s power (`0..=n / 2`) of the last [`RealFft::run`].
    pub fn power(&self, k: usize) -> f32 {
        let half = self.re.len();
        let (zr, zi) = (self.re[k % half], self.im[k % half]);
        let (cr, ci) = (self.re[(half - k) % half], -self.im[(half - k) % half]);
        // Even part and odd part, the odd turned by its twiddle.
        let (er, ei) = ((zr + cr) / 2.0, (zi + ci) / 2.0);
        let (dr, di) = ((zr - cr) / 2.0, (zi - ci) / 2.0);
        // (d / i) = (di, -dr)
        let (or, oi) = (di, -dr);
        let (wr, wi) = self.turns[k];
        let (xr, xi) = (er + or * wr - oi * wi, ei + or * wi + oi * wr);
        xr * xr + xi * xi
    }
}

/// Appends each frame's mean over its `channels` (planar, the same
/// length), `limit` frames at most, to `out`; the frames taken.
pub fn downmix_planar(channels: &[&[f32]], limit: usize, out: &mut Vec<f32>) -> usize {
    let Some(n) = channels.iter().map(|c| c.len()).min() else {
        return 0;
    };
    let n = n.min(limit);
    let k = channels.len() as f32;
    out.extend((0..n).map(|i| channels.iter().map(|c| c[i]).sum::<f32>() / k));
    n
}

/// The same for `data` interleaved `channels` to a frame.
pub fn downmix_interleaved(
    data: &[f32],
    channels: usize,
    limit: usize,
    out: &mut Vec<f32>,
) -> usize {
    let channels = channels.max(1);
    let k = channels as f32;
    let n = (data.len() / channels).min(limit);
    out.extend(
        data.chunks_exact(channels)
            .take(n)
            .map(|f| f.iter().sum::<f32>() / k),
    );
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "power of two")]
    fn a_real_transform_refuses_a_size_it_cannot_halve() {
        RealFft::new(1000);
    }

    #[test]
    fn the_transform_finds_a_sine() {
        let fft = Fft::new(256);
        let mut re: Vec<f32> = (0..256)
            .map(|i| (2.0 * std::f32::consts::PI * 10.0 * i as f32 / 256.0).cos())
            .collect();
        let mut im = vec![0.0; 256];
        fft.run(&mut re, &mut im);
        let mag: Vec<f32> = re
            .iter()
            .zip(&im)
            .map(|(r, i)| (r * r + i * i).sqrt())
            .collect();
        assert!((mag[10] - 128.0).abs() < 1e-2, "{}", mag[10]);
        assert!((mag[246] - 128.0).abs() < 1e-2);
        assert!(mag[11] < 1e-2 && mag[3] < 1e-2);
    }
}
