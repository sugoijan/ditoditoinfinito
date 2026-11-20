//! Frozen residual network. File order: (mean, deviation) for global and bin
//! features, then row-major (weights, biases) for the four affine layers.
use super::{features::Chart, prior};
use std::{array, sync::OnceLock};

// The released inference evaluates nonlinearities in f64, then stores f32 activations.
fn activation(value: f32) -> f32 {
    f64::from(value).tanh() as f32
}

struct Layer<const N: usize> {
    weights: Vec<f32>,
    bias: [f32; N],
}
impl<const N: usize> Layer<N> {
    fn read(data: &mut impl Iterator<Item = f32>, width: usize) -> Self {
        Self {
            weights: data.take(width * N).collect(),
            bias: array::from_fn(|_| data.next().unwrap()),
        }
    }
    fn apply(&self, x: &[f32]) -> [f32; N] {
        debug_assert_eq!(x.len() * N, self.weights.len());
        array::from_fn(|i| {
            let w = &self.weights[i * x.len()..(i + 1) * x.len()];
            (w.iter()
                .zip(x)
                .map(|(&a, &b)| f64::from(a) * f64::from(b))
                .sum::<f64>()
                + f64::from(self.bias[i])) as f32
        })
    }
}
struct Norm<const N: usize> {
    mu: [f32; N],
    sd: [f32; N],
}
impl<const N: usize> Norm<N> {
    fn read(data: &mut impl Iterator<Item = f32>) -> Self {
        Self {
            mu: array::from_fn(|_| data.next().unwrap()),
            sd: array::from_fn(|_| data.next().unwrap()),
        }
    }
    fn apply(&self, x: &mut [f32]) {
        assert_eq!(x.len(), N);
        for ((v, m), s) in x.iter_mut().zip(self.mu).zip(self.sd) {
            *v = ((f64::from(*v) - f64::from(m)) / f64::from(s)).clamp(-5., 5.) as f32;
        }
    }
}
struct Model {
    global: Norm<169>,
    bin: Norm<51>,
    row: Layer<16>,
    temporal: Layer<16>,
    hidden: Layer<40>,
    readout: Layer<90>,
}
fn model() -> &'static Model {
    static MODEL: OnceLock<Model> = OnceLock::new();
    MODEL.get_or_init(|| {
        let bytes: &[u8; 13826 * 4] = include_bytes!("weights.f32le");
        let mut data = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .copied()
            .map(f32::from_le_bytes);
        let model = Model {
            global: Norm::read(&mut data),
            bin: Norm::read(&mut data),
            row: Layer::read(&mut data, 51),
            temporal: Layer::read(&mut data, 48),
            hidden: Layer::read(&mut data, 201),
            readout: Layer::read(&mut data, 40),
        };
        assert!(data.next().is_none());
        model
    })
}

pub(super) fn predict(chart: &Chart) -> ([[f64; 9]; 10], [bool; 8]) {
    const BINS: usize = 16;
    let m = model();
    let rows = &chart.rows;
    let n = rows.len() as f64;
    let duration = rows.last().unwrap().time - rows[0].time;
    let mut means = [0.; 25];
    let mut peaks = [0.0_f64; 25];
    let mut bins = [[0.; 51]; BINS];
    let mut counts = [0; BINS];
    let mut heads = [false; 4];
    for (i, row) in rows.iter().enumerate() {
        let b = i * BINS / rows.len();
        counts[b] += 1;
        heads[usize::from(row.demand.count.min(3))] = true;
        for (j, &v) in row.channels.iter().enumerate() {
            means[j] += v;
            peaks[j] = peaks[j].max(v);
            bins[b][j] += v;
            bins[b][25 + j] = f64::max(bins[b][25 + j], v);
        }
    }
    let means = means.map(|v| v / n);
    let mut variance = [0.; 25];
    for row in rows {
        for (j, &v) in row.channels.iter().enumerate() {
            variance[j] += (v - means[j]).powi(2);
        }
    }
    let prior = prior::predict(chart);
    let mut global = Vec::with_capacity(201);
    global.extend(
        [
            n.ln_1p(),
            duration.ln_1p(),
            chart.heads.ln_1p(),
            chart.heads / (duration + 1.),
        ]
        .map(|v| v as f32),
    );
    global.extend(means.map(|v| v as f32));
    global.extend(variance.map(|v| (v / n).sqrt() as f32));
    global.extend(peaks.map(|v| v as f32));
    global.extend(
        prior
            .iter()
            .flatten()
            .map(|v| f64::from(v / 5.).asinh() as f32),
    );
    m.global.apply(&mut global);
    let embedded: [_; BINS] = array::from_fn(|i| {
        let mut bin = bins[i];
        let count = counts[i];
        if count > 0 {
            for x in &mut bin[..25] {
                *x /= f64::from(count);
            }
            bin[50] = 1.;
        }
        let mut bin = bin.map(|v| v as f32);
        m.bin.apply(&mut bin);
        m.row.apply(&bin).map(activation)
    });
    let mut sum = [0.0_f64; 16];
    let mut peak = [f32::NEG_INFINITY; 16];
    for i in 0..BINS {
        let mut window = [0.; 48];
        if i > 0 {
            window[..16].copy_from_slice(&embedded[i - 1]);
        }
        window[16..32].copy_from_slice(&embedded[i]);
        if i + 1 < BINS {
            window[32..].copy_from_slice(&embedded[i + 1]);
        }
        for (j, x) in m
            .temporal
            .apply(&window)
            .map(activation)
            .into_iter()
            .enumerate()
        {
            sum[j] += f64::from(x);
            peak[j] = peak[j].max(x);
        }
    }
    global.extend(sum.map(|v| (v / BINS as f64) as f32));
    global.extend(peak);
    let z = m.readout.apply(&m.hidden.apply(&global).map(activation));
    let mask = [
        true,
        heads[1],
        heads[2],
        heads[3],
        true,
        heads[1],
        heads[2] || heads[3],
        true,
        chart.mines > 0,
    ];
    let means = array::from_fn(|i| {
        array::from_fn(|j| {
            if !mask[j] {
                return 0.;
            }
            let base = prior[i][j];
            let v = f64::from(base);
            let scale = if j == 0 {
                0.65 + 0.1 * (100. - v).max(0.)
            } else {
                0.18 + 0.3 * v.abs().sqrt() + 0.13 * v.abs()
            };
            let result = base + scale as f32 * z[i * 9 + j];
            f64::from(if j == 0 { result.min(100.) } else { result })
        })
    });
    (means, array::from_fn(|k| mask[k + 1]))
}
