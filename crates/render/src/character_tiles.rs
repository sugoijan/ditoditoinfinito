// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
//! Rounded visual buttons. Their lower stop is the unchanged contact plane.
use super::Vertex;
pub(super) fn vertices() -> Vec<Vertex> {
    let half = 0.0115;
    let radius = 0.0028;
    let inner = half - radius;
    let mut out = Vec::new();
    for tile in 0..9 {
        let x = (tile % 3) as f32 * 0.024 - 0.024;
        let z = (tile / 3) as f32 * 0.024 - 0.024;
        let mut edge = Vec::new();
        for corner in 0..4 {
            let angle = corner as f32 * std::f32::consts::FRAC_PI_2;
            let cx = if corner == 0 || corner == 3 {
                inner
            } else {
                -inner
            };
            let cz = if corner < 2 { inner } else { -inner };
            for j in 0..=8 {
                let a = angle + j as f32 / 8. * std::f32::consts::FRAC_PI_2;
                let n = [a.cos(), 0., a.sin()];
                edge.push(([x + cx + radius * n[0], 1., z + cz + radius * n[2]], n));
            }
        }
        let vertex = |p, n| Vertex {
            position: p,
            normal: n,
            detail: [tile as f32, 0., 0., 0.],
        };
        for i in 0..edge.len() {
            let (a, na) = edge[i];
            let (b, nb) = edge[(i + 1) % edge.len()];
            out.extend([
                vertex([x, 1., z], [0., 1., 0.]),
                vertex(b, [0., 1., 0.]),
                vertex(a, [0., 1., 0.]),
            ]);
            let a0 = [a[0], 0., a[2]];
            let b0 = [b[0], 0., b[2]];
            out.extend([
                vertex(a, na),
                vertex(b, nb),
                vertex(b0, nb),
                vertex(a, na),
                vertex(b0, nb),
                vertex(a0, na),
            ]);
        }
    }
    out
}
