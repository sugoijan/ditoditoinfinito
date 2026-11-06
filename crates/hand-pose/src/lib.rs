//! Single-threaded hand kinematics and bounded contact fitting. No I/O, clocks,
//! threads or platform APIs. Millimeters/degrees internally; glTF output in meters.
// The kinematics' matrices and Jacobians read as their math, by row and
// column (`a[c][k]` and `a[k][c]` cleared together): index loops kept.
#![allow(clippy::needless_range_loop)]
mod contact_curve;
mod correction;
mod curve;
mod fit;
mod kinematics;
mod motion;
mod plan;
mod rolling;
mod runtime;
pub use runtime::{DanceRuntime, RuntimeReport};
mod stream;
pub use contact_curve::{
    ContactCurve, ContactCurveError, ContactCurvePreparation, ContactCurvePublication,
    ContactCurveReport, ContactCurveSample,
};
pub use correction::{ContactCorrection, correct_contact_pose};
pub use fit::PoseFit;
pub use motion::{Guide, Preparation};
pub use rolling::{PreparationMode, Rolling, RollingOptions, WindowReport};
pub use stream::StreamOptions;
mod solve;
pub use plan::{Mine, Plan, RollPulse, Targets, panel_position};
use serde::Deserialize;
pub use solve::{Goal, SolveReport, refine_contacts, solve};
pub type Parameters = [f64; 14];
pub type Point = [f64; 3];
pub type Matrix = [[f64; 4]; 4];
pub const IDENTITY: Matrix = [
    [1., 0., 0., 0.],
    [0., 1., 0., 0.],
    [0., 0., 1., 0.],
    [0., 0., 0., 1.],
];
pub fn mul(a: &Matrix, b: &Matrix) -> Matrix {
    let mut o = IDENTITY;
    for i in 0..3 {
        for j in 0..4 {
            o[i][j] =
                (0..3).map(|k| a[i][k] * b[k][j]).sum::<f64>() + if j == 3 { a[i][3] } else { 0. };
        }
    }
    o
}
pub fn point(m: &Matrix, p: Point) -> Point {
    std::array::from_fn(|i| m[i][3] + (0..3).map(|j| m[i][j] * p[j]).sum::<f64>())
}
fn vector(m: &Matrix, p: Point) -> Point {
    std::array::from_fn(|i| (0..3).map(|j| m[i][j] * p[j]).sum())
}
#[derive(Deserialize)]
pub struct Tip {
    p: Point,
    w: [f64; 4],
    b: [usize; 4],
}
#[derive(Deserialize, Clone)]
pub struct Style {
    pub q: Parameters,
    pub tips: [Point; 2],
    pub com: Point,
}
#[derive(Deserialize)]
pub struct Model {
    a: Matrix,
    inv_a: Matrix,
    bind: [Matrix; 16],
    fixed: [Matrix; 16],
    locals: [Matrix; 6],
    shifts: [Vec<f64>; 6],
    directions: [Point; 6],
    tips: [Vec<Tip>; 2],
    mass_fixed: Point,
    mass_local: [Point; 6],
    mass_weight: [f64; 6],
    origins: [Point; 19],
    helper_origins: [Point; 19],
    pub parents: [i32; 19],
    pub style: Vec<Style>,
    morph_features: Vec<[f64; 8]>,
    morph_coeff: Vec<Vec<f64>>,
    pub lo: Parameters,
    pub hi: Parameters,
    mass_correction: Vec<Vec<f64>>,
    pub mass_correction_training_max_mm: f64,
    /// Derived from `tips` at load; never read from the calibration file.
    #[serde(skip)]
    skin: kinematics::Skin,
    /// Derived from `style` at load: each style's fingertip span, the angle
    /// from its left to its right fingertip and where its mass center lies
    /// between them, as `Plan::seed` compares them.
    #[serde(skip)]
    style_shape: Vec<StyleShape>,
}
#[derive(Clone, Copy, Default)]
pub(crate) struct StyleShape {
    pub(crate) span: f64,
    pub(crate) angle: f64,
    pub(crate) support: f64,
}
#[derive(Clone)]
pub struct Geometry {
    pub proxy_com: Point,
    pub tips: [Point; 2],
    pub patches: [Point; 288],
    pub com: Point,
    pub knuckles: [f64; 2],
}
impl Model {
    pub fn load(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 4 * 1024 * 1024 {
            return Err("calibration byte limit".into());
        }
        let m: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if m.tips
            .iter()
            .any(|t| t.len() != 144 || t.iter().any(|v| v.b.iter().any(|&i| i >= 19)))
            || m.shifts.iter().any(|s| s.len() != 441)
            || m.style.is_empty()
            || m.style.len() > 2048
            || m.morph_features.len() != m.morph_coeff.len()
            || m.morph_coeff.iter().any(|v| v.len() != 76)
            || m.morph_coeff.len() > 2048
            || m.mass_correction.len() != 3
            || m.mass_correction.iter().any(|r| r.len() != 45)
        {
            return Err("invalid calibration dimensions".into());
        }
        if m.lo
            .iter()
            .zip(m.hi)
            .any(|(a, b)| !a.is_finite() || !b.is_finite() || *a >= b)
        {
            return Err("invalid parameter bounds".into());
        }
        let mut m = m;
        m.skin = kinematics::Skin::new(&m);
        m.style_shape = m
            .style
            .iter()
            .map(|s| {
                let v = [s.tips[1][0] - s.tips[0][0], s.tips[1][1] - s.tips[0][1]];
                let span = v[0].hypot(v[1]);
                StyleShape {
                    span,
                    angle: v[1].atan2(v[0]),
                    support: (((s.com[0] - s.tips[0][0]) * v[0]
                        + (s.com[1] - s.tips[0][1]) * v[1])
                        / span.powi(2).max(1e-8))
                    .clamp(0., 1.),
                }
            })
            .collect();
        Ok(m)
    }
    pub fn clamp(&self, q: &mut Parameters) {
        for i in 0..14 {
            q[i] = q[i].clamp(self.lo[i] + 1e-6, self.hi[i] - 1e-6)
        }
    }
    pub fn transforms(&self, q: &Parameters) -> ([Matrix; 19], [Matrix; 16]) {
        let [ax, ay, az] = [
            -1.40 + q[11].to_radians(),
            -0.32 + q[12].to_radians(),
            q[13].to_radians(),
        ];
        let (sx, cx) = ax.sin_cos();
        let (sy, cy) = ay.sin_cos();
        let (sz, cz) = az.sin_cos();
        let root = [
            [
                cz * cy,
                cz * sy * sx - sz * cx,
                cz * sy * cx + sz * sx,
                q[8],
            ],
            [
                sz * cy,
                sz * sy * sx + cz * cx,
                sz * sy * cx - cz * sx,
                q[9],
            ],
            [-sy, cy * sx, cy * cx, q[10]],
            [0., 0., 0., 1.],
        ];
        let mut b = std::array::from_fn(|i| mul(&root, &self.fixed[i]));
        for d in 0..2 {
            let mut p = IDENTITY;
            for j in 0..3 {
                let i = d * 3 + j;
                let bend = q[d * 4 + j];
                let (sa, ca) = bend.to_radians().sin_cos();
                let (ss, cs) = if j == 0 {
                    q[d * 4 + 3].to_radians().sin_cos()
                } else {
                    (0., 1.)
                };
                let u = (bend * 4.).clamp(0., 440.);
                let k = (u.floor() as usize).min(439);
                let shift = self.shifts[i][k]
                    + (u - k as f64) * (self.shifts[i][k + 1] - self.shifts[i][k]);
                let dr = self.directions[i].map(|v| v * shift);
                let delta = [
                    [cs, -ss, 0., dr[0]],
                    [ca * ss, ca * cs, sa, dr[1]],
                    [-sa * ss, -sa * cs, ca, dr[2]],
                    [0., 0., 0., 1.],
                ];
                p = mul(&mul(&p, &self.locals[i]), &delta);
                b[i + 1] = mul(&root, &p);
            }
        }
        let mut g = [IDENTITY; 19];
        for i in 0..16 {
            g[i + 3] = mul(&mul(&self.a, &b[i]), &self.bind[i]);
        }
        let palm = self.helper_origins[3];
        let moved = point(&g[3], palm);
        for i in 1..3 {
            for k in 0..3 {
                g[i][k][3] = moved[k] - palm[k];
            }
        }
        (g, b)
    }
    pub fn geometry(&self, q: &Parameters) -> Geometry {
        self.geometry_into(q, &mut kinematics::Kin::default())
    }
    pub fn morph_weights(&self, q: &Parameters) -> [f32; 76] {
        let scales = [10., 14., 9., 14., 10., 14., 9., 14.];
        let distance =
            |f: &[f64; 8]| -> f64 { (0..8).map(|i| (q[i] / scales[i] - f[i]).powi(2)).sum() };
        let low = self
            .morph_features
            .iter()
            .map(distance)
            .fold(f64::INFINITY, f64::min);
        let mut out = [0_f64; 76];
        let mut sum = 0.;
        for (f, c) in self.morph_features.iter().zip(&self.morph_coeff) {
            let w = (-(distance(f) - low) / 0.35_f64.powi(2)).exp();
            sum += w;
            for j in 0..76 {
                out[j] += w * c[j];
            }
        }
        out.map(|w| (w / sum) as f32)
    }
    /// glTF joint world matrices, column-major, without evaluating any clip.
    pub fn joint_worlds(&self, q: &Parameters) -> [[f32; 16]; 19] {
        let (g, _) = self.transforms(q);
        std::array::from_fn(|i| {
            let p = point(&g[i], self.origins[i]);
            std::array::from_fn(|j| {
                let r = j % 4;
                let c = j / 4;
                if r == 3 {
                    if c == 3 { 1. } else { 0. }
                } else if c == 3 {
                    (p[r] * 0.01 * if r == 2 { -1. } else { 1. }) as f32
                } else {
                    (g[i][r][c] * if (r == 2) ^ (c == 2) { -1. } else { 1. }) as f32
                }
            })
        })
    }
}
