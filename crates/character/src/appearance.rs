//! Presentation overrides only: never enter kinematics or contact planning.
pub use ddi_dance_mat::Geometry;
use serde::Serialize;
/// Render slots retain the original row-major visual ordering within each pad.
pub const MAX_TILES: usize = 18;
/// Physical identity for a visual slot. Exported X is reflected from solver X.
pub fn physical_tile(mat: Geometry, slot: usize) -> Option<ddi_dance_mat::Tile> {
    if slot >= usize::from(mat.pads()) * 9 {
        return None;
    }
    let cell = (slot % 9) as u8 + 1;
    ddi_dance_mat::Tile::new((slot / 9) as u8, cell + 2 - 2 * ((cell - 1) % 3))
}
/// Tile center in exported right-handed Y-up metres (X/Z floor coordinates).
pub fn tile_center(mat: Geometry, slot: usize) -> Option<[f32; 2]> {
    let tile = physical_tile(mat, slot)?;
    if mat == Geometry::single() {
        return Some([
            (slot % 3) as f32 * 0.024 - 0.024,
            (slot / 3) as f32 * 0.024 - 0.024,
        ]);
    }
    let p = mat.position(tile)?;
    Some([(-p[0] * 0.001) as f32, (p[1] * 0.001) as f32])
}
fn contact_tile(mat: Geometry, p: [f32; 3]) -> Option<usize> {
    if p.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let half = (mat.tile_half_mm() * 0.001) as f32;
    (0..usize::from(mat.pads()) * 9).find(|&i| {
        let [x, z] = tile_center(mat, i).expect("active tile");
        (p[0] - x).abs() <= half && (p[2] - z).abs() <= half
    })
}
/// UP panel centre in exported right-handed Y-up metres.
pub const UP_TILE_CENTER: [f32; 3] = [0., 0., 0.024];
/// Accepted playground framing, adopted from the maintainer’s copy on 2026-11-05.
/// Kept separate from the legacy renderer camera and all motion/model data.
pub const BASE_CAMERA: [f32; 3] = [29.28354, 15.031203, 0.49];
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Appearance {
    pub transparent: bool,
    /// Screen-space silhouette backing, independent from the scene lighting.
    pub backing_color: [f32; 3],
    pub backing_opacity: f32,
    /// Padding as a fraction of the rendered frame's shorter edge (0–6%).
    pub backing_margin: f32,
    pub studio: bool,
    pub random_colors: bool,
    pub palette_seed: u32,
    pub spill: f32,
    pub mat: bool,
    pub tint: [f32; 3],
    pub roughness: f32,
    pub specular: f32,
    pub key: f32,
    pub fill: f32,
    pub rim: f32,
    pub ambient: f32,
    pub light_yaw: f32,
    pub pulse: f32,
    pub bpm: f32,
    pub sway: f32,
    pub mat_color: [f32; 3],
    pub accent: [f32; 3],
    pub response: f32,
    /// Diagnostic tile, 0 = actual contacts, 1..=18 = explicit render-slot preview.
    pub test_tile: u8,
}
impl Appearance {
    pub fn has_backing(self) -> bool {
        self.transparent && self.backing_opacity.is_finite() && self.backing_opacity > 0.
    }
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            transparent: true,
            backing_color: [0.12, 0.186, 0.268],
            backing_opacity: 0.598,
            backing_margin: 0.00984,
            studio: true,
            random_colors: true,
            palette_seed: 1,
            spill: 1.,
            mat: true,
            tint: [0.818, 0.661, 0.93],
            roughness: 0.588,
            specular: 0.315,
            key: 0.645,
            fill: 0.14,
            rim: 0.53,
            ambient: 0.12,
            light_yaw: 115.2,
            pulse: 1.,
            bpm: 120.,
            sway: 1.,
            mat_color: [0.055649996, 0.06824999, 0.10955],
            accent: [0.16, 0.9, 0.68],
            response: 1.,
            test_tile: 0,
        }
    }
}
/// Seeded bright hues, stable during a contact and across repeated rendering.
/// CPU supplies the same linear RGB to the emitter and its reflected light.
pub fn tile_color(seed: u32, tile: usize, strike: u32) -> [f32; 3] {
    let mut h = seed
        .wrapping_add((tile as u32).wrapping_mul(0x9e3779b9))
        .wrapping_add(strike.wrapping_mul(0x85ebca6b));
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb352d);
    h ^= h >> 15;
    let hue = (h % 3600) as f32 / 600.;
    let x = 1. - (hue % 2. - 1.).abs();
    let rgb = match hue as u32 {
        0 => [1., x, 0.],
        1 => [x, 1., 0.],
        2 => [0., 1., x],
        3 => [0., x, 1.],
        4 => [x, 0., 1.],
        _ => [1., 0., x],
    };
    rgb.map(|v| 0.12 + v * 0.88)
}
/// Render-only contact indicators in world metres. No solver tolerance changes.
/// The 24 mm pitch and 23 mm tiles match the accepted procedural pad.
pub const CONTACT_SUSTAIN: f32 = 0.28;
const CONTACT_DECAY: f32 = 0.045;
const RELEASE_DECAY: f32 = 0.030;
/// Resting tile height in metres. Activation stays at the original zero plane.
pub const TILE_TRAVEL: f32 = 0.001;
pub fn tile_heights(contacts: &[[f32; 3]]) -> [f32; 9] {
    tile_heights_on(Geometry::single(), contacts)[..9]
        .try_into()
        .unwrap()
}
pub fn tile_heights_on(mat: Geometry, contacts: &[[f32; 3]]) -> [f32; MAX_TILES] {
    let mut heights = [0.; MAX_TILES];
    heights[..usize::from(mat.pads()) * 9].fill(TILE_TRAVEL);
    for &p in contacts {
        if let Some(i) = contact_tile(mat, p) {
            heights[i] = heights[i].min(p[1].clamp(0., TILE_TRAVEL));
        }
    }
    heights
}
#[derive(Default)]
pub struct ContactLights {
    pub values: [f32; MAX_TILES],
    pub strikes: [u32; MAX_TILES],
    previous: [bool; MAX_TILES],
    time: Option<f32>,
    mat: Option<Geometry>,
}
impl ContactLights {
    pub fn update(&mut self, seconds: f32, contacts: &[[f32; 3]]) {
        self.update_on(Geometry::single(), seconds, contacts);
    }
    pub fn update_on(&mut self, mat: Geometry, seconds: f32, contacts: &[[f32; 3]]) {
        let dt = self.time.map_or(0., |t| seconds - t);
        if self.mat != Some(mat) || self.time.is_none() || !(0. ..=0.2).contains(&dt) {
            self.values = [0.; MAX_TILES];
            self.strikes = [0; MAX_TILES];
            self.previous = [false; MAX_TILES];
        }
        self.mat = Some(mat);
        let mut planted = [false; MAX_TILES];
        for &p in contacts {
            if p[1].abs() <= 0.00025
                && let Some(i) = contact_tile(mat, p)
            {
                planted[i] = true;
            }
        }
        for (i, on) in planted.into_iter().enumerate() {
            if on {
                if !self.previous[i] {
                    self.strikes[i] = self.strikes[i].wrapping_add(1);
                    self.values[i] = 1.;
                } else {
                    self.values[i] = CONTACT_SUSTAIN
                        + (self.values[i] - CONTACT_SUSTAIN) * (-dt.max(0.) / CONTACT_DECAY).exp();
                }
            } else {
                self.values[i] *= (-dt.max(0.) / RELEASE_DECAY).exp();
            }
            self.previous[i] = on;
        }
        self.time = Some(seconds);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn colors_change_on_restrike_and_are_stable_when_paused() {
        assert_eq!(tile_color(1, 2, 3), tile_color(1, 2, 3));
        assert_ne!(tile_color(1, 2, 3), tile_color(1, 2, 4));
        assert_ne!(tile_color(1, 2, 3), tile_color(2, 2, 3));
        let mut l = ContactLights::default();
        l.update(0., &[[0.; 3]]);
        assert_eq!(l.strikes[4], 1);
        l.update(0., &[[0.; 3]]);
        assert_eq!(l.strikes[4], 1);
        l.update(0.05, &[]);
        l.update(0.1, &[[0.; 3]]);
        assert_eq!(l.strikes[4], 2);
    }
    #[test]
    fn all_tiles_contact_only_and_seek_reset() {
        for i in 0..9 {
            let mut lights = ContactLights::default();
            let p = [
                (i % 3) as f32 * 0.024 - 0.024,
                0.,
                (i / 3) as f32 * 0.024 - 0.024,
            ];
            lights.update(1., &[p]);
            assert_eq!(lights.values[i], 1.);
            assert_eq!(lights.values.iter().filter(|v| **v > 0.).count(), 1);
            lights.update(1.1, &[[p[0], 0.004, p[2]]]);
            assert!(lights.values[i] < 1.);
            let paused = lights.values;
            lights.update(1.1, &[]);
            assert_eq!(paused, lights.values);
            lights.update(0., &[]);
            assert_eq!(lights.values, [0.; MAX_TILES]);
        }
        let mut l = ContactLights::default();
        l.update(0., &[[0.012, 0., 0.], [0., 0.001, 0.], [f32::NAN, 0., 0.]]);
        assert_eq!(l.values, [0.; MAX_TILES]);
    }
}

/// Temporary play-view presentation study, shared by native and browser hosts.
/// Keeping this separate from the runtime prevents camera edits from replanning.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct PlayLook {
    pub camera: [f32; 3],
    pub appearance: Appearance,
    pub guide: bool,
}
impl Default for PlayLook {
    fn default() -> Self {
        Self {
            camera: BASE_CAMERA,
            appearance: Appearance::default(),
            guide: true,
        }
    }
}

/// Screen-relative camera frame. Never changes the mat or solver's scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandFrame {
    pub center: f32,
    pub width: f32,
    pub height: f32,
}
impl Default for HandFrame {
    fn default() -> Self {
        Self {
            center: 0.75,
            width: 0.5,
            height: 1.,
        }
    }
}
impl HandFrame {
    pub fn rect(self, size: [u32; 2]) -> [f32; 4] {
        let [w, h] = size.map(|s| s.max(1) as f32);
        let margin = 12_f32.min(w / 4.).min(h / 4.);
        let fw = (w * self.width.clamp(0.3, 1.) - 2. * margin).max(0.5);
        let fh = (h * self.height.clamp(0.5, 1.) - 2. * margin).max(0.5);
        let x =
            (w * self.center.clamp(0., 1.) - fw / 2.).clamp(margin, (w - fw - margin).max(margin));
        [x, h - fh - margin, fw, fh]
    }
}
#[cfg(test)]
mod frame_tests {
    use super::*;
    #[test]
    fn frames_mirror_and_fit_small_and_large_viewports() {
        for size in [[1920, 1080], [390, 844], [1, 1]] {
            for width in [0.3, 0.5, 1.] {
                let a = HandFrame {
                    center: 0.25,
                    width,
                    height: 0.7,
                }
                .rect(size);
                let b = HandFrame {
                    center: 0.75,
                    width,
                    height: 0.7,
                }
                .rect(size);
                assert!((a[0] + b[0] + a[2] - size[0] as f32).abs() < 0.001);
                assert!(a.iter().all(|v| v.is_finite()));
                assert!(a[2] > 0. && a[3] > 0.);
            }
        }
    }
}

#[cfg(test)]
mod polish_tests {
    use super::*;
    #[test]
    fn flash_settles_to_a_dimmer_hold_and_releases_quickly() {
        let mut a = ContactLights::default();
        a.update(0., &[[0.; 3]]);
        assert_eq!(a.values[4], 1.);
        a.update(0.15, &[[0.; 3]]);
        assert!(a.values[4] > CONTACT_SUSTAIN && a.values[4] < 0.32);
        let paused = a.values;
        a.update(0.15, &[[0.; 3]]);
        assert_eq!(a.values, paused);
        a.update(0.25, &[]);
        assert!(a.values[4] < 0.012);
        a.update(0.26, &[[0.; 3]]);
        assert_eq!(a.values[4], 1.);
        assert_eq!(a.strikes[4], 2);
        let mut b = ContactLights::default();
        b.update(0., &[[0.; 3]]);
        for i in 1..=9 {
            b.update(i as f32 / 60., &[[0.; 3]]);
        }
        assert!((b.values[4] - paused[4]).abs() < 1e-6);
    }
    #[test]
    fn visual_buttons_bottom_out_on_the_unchanged_contact_plane() {
        assert_eq!(tile_heights(&[]), [0.001; 9]);
        for i in 0..9 {
            let p = [
                (i % 3) as f32 * 0.024 - 0.024,
                0.,
                (i / 3) as f32 * 0.024 - 0.024,
            ];
            for y in [0., 0.0002, 0.0005, 0.001, 0.003] {
                let mut tip = p;
                tip[1] = y;
                let h = tile_heights(&[tip]);
                assert_eq!(h[i], y.min(TILE_TRAVEL));
                assert!(h[i] <= y);
            }
            let mut tip = p;
            tip[1] = 0.0008;
            assert_eq!(tile_heights(&[p, tip])[i], 0.);
        }
        assert_eq!(
            tile_heights(&[[f32::NAN, 0., 0.], [0.012, 0., 0.]]),
            [0.001; 9]
        );
    }
}

#[cfg(test)]
mod double_mat_tests {
    use super::*;
    #[test]
    fn both_pads_use_physical_tiles_with_reflected_x_and_no_seam_contact() {
        for mat in [
            Geometry::single(),
            Geometry::new(2, 24., 0.).unwrap(),
            Geometry::new(2, 24., 6.).unwrap(),
        ] {
            for i in 0..usize::from(mat.pads()) * 9 {
                let tile = physical_tile(mat, i).unwrap();
                let physical = mat.position(tile).unwrap();
                let [x, z] = tile_center(mat, i).unwrap();
                assert!((x as f64 + physical[0] * 0.001).abs() < 1e-8);
                assert!((z as f64 - physical[1] * 0.001).abs() < 1e-8);
                let mut lights = ContactLights::default();
                lights.update_on(mat, 0., &[[x, 0., z]]);
                assert_eq!(lights.strikes[i], 1);
                assert_eq!(lights.values.iter().filter(|v| **v > 0.).count(), 1);
                assert_eq!(tile_heights_on(mat, &[[x, 0., z]])[i], 0.);
                assert_eq!(tile_heights_on(mat, &[[x, 0.0005, z]])[i], 0.0005);
                lights.update_on(mat, 0.05, &[[x, 0.004, z]]);
                assert!(lights.values[i] < 0.3);
            }
        }
        let mat = Geometry::new(2, 24., 0.).unwrap();
        let mut lights = ContactLights::default();
        lights.update_on(
            mat,
            0.,
            &[[0., 0., 0.], [0.072, 0., 0.], [f32::NAN, 0., 0.]],
        );
        assert_eq!(lights.values, [0.; MAX_TILES]);
        assert_eq!(physical_tile(mat, 7).unwrap().id(), 8);
        assert_eq!(physical_tile(mat, 16).unwrap().id(), 17);
    }
    #[test]
    fn paused_geometry_switch_resets_lights_and_inactive_slots() {
        let double = Geometry::new(2, 24., 0.).unwrap();
        let [x, z] = tile_center(double, 16).unwrap();
        let mut lights = ContactLights::default();
        lights.update_on(double, 1., &[[x, 0., z]]);
        assert_eq!(lights.values[16], 1.);
        lights.update_on(Geometry::single(), 1., &[[0.; 3]]);
        assert_eq!(lights.values[16], 0.);
        assert_eq!(lights.values[4], 1.);
        assert!(
            tile_heights_on(Geometry::single(), &[])[9..]
                .iter()
                .all(|v| *v == 0.)
        );
    }
}
