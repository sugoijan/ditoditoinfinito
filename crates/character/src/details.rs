// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
//! Rest-surface material coordinates. Never modify vertices, weights or poses.
use crate::{
    Character, SurfaceKind,
    math::{cross, dot, normalize},
};
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
impl Character {
    /// Elliptical nail UV, dorsal eligibility, and radial cap coordinate. These
    /// interpolate on the existing triangles and follow every skin/morph pose.
    pub fn surface_details(&self) -> Vec<[f32; 4]> {
        let mut out = vec![[100., 100., 0., 0.]; self.vertices.len()];
        let origin = |joint: usize| {
            let m = self.inverse_bind[joint];
            let t = [m[12], m[13], m[14]];
            [
                -dot([m[0], m[1], m[2]], t),
                -dot([m[4], m[5], m[6]], t),
                -dot([m[8], m[9], m[10]], t),
            ]
        };
        for finger in ["index", "middle", "ring", "little", "thumb"] {
            let names = if finger == "thumb" {
                ["metacarpal", "proximal", "distal"]
            } else {
                ["proximal", "middle", "distal"]
            };
            let ids = names.map(|part| {
                self.joint_names
                    .iter()
                    .position(|n| n == &format!("{finger}.{part}"))
            });
            let [Some(a), Some(b), Some(c)] = ids else {
                continue;
            };
            let weight = |v: &crate::Vertex, j: usize| {
                v.joints
                    .iter()
                    .zip(v.weights)
                    .filter(|(id, _)| usize::from(**id) == j)
                    .map(|(_, w)| w)
                    .sum::<f32>()
            };
            let cloud: Vec<_> = self
                .vertices
                .iter()
                .filter(|v| weight(v, c) > 0.65)
                .collect();
            if cloud.len() < 6 {
                continue;
            }
            let mean: [f32; 3] = std::array::from_fn(|i| {
                cloud.iter().map(|v| v.position[i]).sum::<f32>() / cloud.len() as f32
            });
            let start = origin(c);
            let mut axis = normalize(sub(mean, start));
            // Principal direction of terminal skin, signed toward the tip.
            for _ in 0..12 {
                let mut v = [0.; 3];
                for p in &cloud {
                    let d = sub(p.position, mean);
                    let k = dot(d, axis);
                    for i in 0..3 {
                        v[i] += d[i] * k;
                    }
                }
                axis = normalize(v);
            }
            if dot(axis, sub(mean, start)) < 0. {
                axis = axis.map(|v| -v);
            }
            let lateral = normalize(cross(
                normalize(sub(origin(b), origin(a))),
                normalize(sub(start, origin(b))),
            ));
            let dorsal = normalize(cross(axis, lateral));
            let side = normalize(cross(dorsal, axis));
            if dot(dorsal, dorsal) < 0.9 {
                continue;
            }
            let length = cloud
                .iter()
                .map(|v| dot(sub(v.position, start), axis))
                .fold(0_f32, f32::max);
            let lo = cloud
                .iter()
                .map(|v| dot(sub(v.position, start), side))
                .fold(f32::INFINITY, f32::min);
            let hi = cloud
                .iter()
                .map(|v| dot(sub(v.position, start), side))
                .fold(f32::NEG_INFINITY, f32::max);
            let half_width = ((hi - lo) * 0.35).max(0.0015);
            let half_length = (length * 0.32).max(0.002);
            for (i, v) in self.vertices.iter().enumerate() {
                let w = weight(v, c);
                if w < 0.05 {
                    continue;
                }
                let p = sub(v.position, start);
                out[i][0] = (dot(p, side) - (lo + hi) * 0.5) / half_width;
                out[i][1] = (dot(p, axis) - length * 0.60) / half_length;
                out[i][2] = ((dot(v.normal, dorsal) - 0.15) / 0.45).clamp(0., 1.)
                    * ((w - 0.15) / 0.5).clamp(0., 1.);
            }
        }
        for primitive in self
            .primitives
            .iter()
            .filter(|p| p.surface == SurfaceKind::Interior)
        {
            let mut ids = self.indices
                [primitive.indices.start as usize..primitive.indices.end as usize]
                .to_vec();
            ids.sort_unstable();
            ids.dedup();
            let lo: [f32; 3] = std::array::from_fn(|i| {
                ids.iter()
                    .map(|v| self.vertices[*v as usize].position[i])
                    .fold(f32::INFINITY, f32::min)
            });
            let hi: [f32; 3] = std::array::from_fn(|i| {
                ids.iter()
                    .map(|v| self.vertices[*v as usize].position[i])
                    .fold(f32::NEG_INFINITY, f32::max)
            });
            let mut axes = [0, 1, 2];
            axes.sort_by(|a, b| (hi[*b] - lo[*b]).total_cmp(&(hi[*a] - lo[*a])));
            for id in ids {
                let p = self.vertices[id as usize].position;
                let r = axes[..2]
                    .iter()
                    .map(|i| {
                        ((p[*i] - (hi[*i] + lo[*i]) * 0.5) / ((hi[*i] - lo[*i]) * 0.5).max(1e-6))
                            .powi(2)
                    })
                    .sum::<f32>()
                    .sqrt();
                out[id as usize][3] = r;
            }
        }
        out
    }
}
