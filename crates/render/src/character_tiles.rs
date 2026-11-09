// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
//! Rounded visual buttons. Their lower stop is the unchanged contact plane.
use super::Vertex;
use ddi_character::appearance::{Geometry, tile_center};
pub(super) fn vertices(mat: Geometry) -> Vec<Vertex> {
    let half = (mat.tile_half_mm() * 0.001) as f32;
    let radius = 0.0028 * (mat.pitch_mm() / 24.) as f32;
    let inner = half - radius;
    let mut out = Vec::new();
    for tile in 0..usize::from(mat.pads()) * 9 {
        let [x, z] = tile_center(mat, tile).expect("active tile");
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mesh_uses_the_same_eighteen_centers_and_unchanged_hit_plane() {
        for mat in [Geometry::single(), Geometry::new(2, 24., 0.).unwrap()] {
            let data = vertices(mat);
            let n = usize::from(mat.pads()) * 9;
            assert_eq!(data.len(), n * 36 * 9);
            let mut seen = vec![false; n];
            for v in data {
                let i = v.detail[0] as usize;
                seen[i] = true;
                let [x, z] = tile_center(mat, i).unwrap();
                let half = (mat.tile_half_mm() * 0.001) as f32;
                assert!((v.position[0] - x).abs() <= half + 1e-8);
                assert!((v.position[2] - z).abs() <= half + 1e-8);
                assert!(v.position[1] == 0. || v.position[1] == 1.);
            }
            assert!(seen.iter().all(|v| *v));
        }
    }
}
