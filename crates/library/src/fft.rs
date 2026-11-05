//! A radix-2 Fourier transform, for the music's analyses: the chart
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
        for i in 0..n {
            let j = self.reverse[i] as usize;
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

#[cfg(test)]
mod tests {
    use super::*;

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
