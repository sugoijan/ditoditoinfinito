//! A small, bounded glTF2 character contract. No files, clocks, GPU or platform APIs.
//! LINEAR TRS tracks, sparse position morphs and four weighted joints per vertex.
//! Unsupported formats fail explicitly; this is not a general glTF importer.
pub mod appearance;
pub mod details;
mod glb;
pub mod math;
use math::{IDENTITY, Mat4, mul, normalize, point, slerp, trs, vector};

pub const HAND_MODEL: &[u8] = include_bytes!("../../../assets/hand/hand.glb");

/// The layouts the dancing hand plays (`docs/plans/dancing-hand.md`):
/// the settings offer it on these alone, and `ddi_hand_plan::layout_panels`
/// places each on the mat.
pub const HAND_LAYOUTS: &[&str] = &[
    "dance-single",
    "dance-double",
    "dance-solo",
    "danoni-5",
    "danoni-7",
    "danoni-9A",
    "danoni-9B",
    "pump-single",
    "pump-halfdouble",
    "pump-double",
    "techno-single8",
];

#[derive(Clone, Debug)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SurfaceKind {
    Skin,
    Rim,
    Interior,
    DiagnosticMat,
    Nail,
}
#[derive(Clone, Debug)]
pub struct Primitive {
    pub indices: std::ops::Range<u32>,
    pub color: [f32; 4],
    pub double_sided: bool,
    pub surface: SurfaceKind,
}
#[derive(Clone, Debug)]
struct Node {
    parent: Option<usize>,
    translation: [f32; 3],
    rotation: [f32; 4],
}
#[derive(Clone, Debug)]
struct Morph {
    offsets: Vec<(usize, [f32; 3])>,
}
#[derive(Clone, Copy, Debug)]
enum Path {
    Translation,
    Rotation,
    Weights,
}
#[derive(Clone, Debug)]
struct Track {
    node: usize,
    path: Path,
    times: Vec<f32>,
    values: Vec<f32>,
    width: usize,
}

#[derive(Clone, Debug)]
pub struct Character {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub primitives: Vec<Primitive>,
    pub duration: f32,
    pub joint_names: Vec<String>,
    pub fingertips: Vec<Vec<usize>>,
    nodes: Vec<Node>,
    order: Vec<usize>,
    joints: Vec<usize>,
    inverse_bind: Vec<Mat4>,
    morphs: Vec<Morph>,
    initial_weights: Vec<f32>,
    tracks: Vec<Track>,
}
impl Character {
    pub fn load(bytes: &[u8]) -> Result<Self, String> {
        glb::load(bytes)
    }
    pub fn morph_count(&self) -> usize {
        self.morphs.len()
    }
    pub fn new_pose(&self) -> Pose {
        Pose {
            positions: vec![[0.; 3]; self.vertices.len()],
            normals: vec![[0.; 3]; self.vertices.len()],
            joint_matrices: vec![IDENTITY; self.joints.len()],
            worlds: vec![IDENTITY; self.nodes.len()],
            translations: self.nodes.iter().map(|n| n.translation).collect(),
            rotations: self.nodes.iter().map(|n| n.rotation).collect(),
            morph_weights: self.initial_weights.clone(),
        }
    }
    /// Clamp at both endpoints. Sampling does not mutate the asset or allocate after pose creation.
    pub fn sample(&self, time: f32, p: &mut Pose) {
        let t = if time.is_finite() {
            time.clamp(0., self.duration)
        } else {
            0.
        };
        for (i, n) in self.nodes.iter().enumerate() {
            p.translations[i] = n.translation;
            p.rotations[i] = n.rotation;
        }
        p.morph_weights.copy_from_slice(&self.initial_weights);
        for track in &self.tracks {
            let hi = track
                .times
                .partition_point(|x| *x <= t)
                .min(track.times.len() - 1);
            let lo = if track.times[hi] > t {
                hi.saturating_sub(1)
            } else {
                hi
            };
            let f = if lo == hi {
                0.
            } else {
                (t - track.times[lo]) / (track.times[hi] - track.times[lo])
            };
            let a = &track.values[lo * track.width..(lo + 1) * track.width];
            let b = &track.values[hi * track.width..(hi + 1) * track.width];
            match track.path {
                Path::Translation => {
                    p.translations[track.node] = std::array::from_fn(|i| a[i] + f * (b[i] - a[i]))
                }
                Path::Rotation => {
                    p.rotations[track.node] = slerp(a.try_into().unwrap(), b.try_into().unwrap(), f)
                }
                Path::Weights => {
                    for (i, w) in p.morph_weights.iter_mut().enumerate() {
                        *w = a[i] + f * (b[i] - a[i]);
                    }
                }
            }
        }
        for &i in &self.order {
            let local = trs(p.translations[i], p.rotations[i]);
            p.worlds[i] = self.nodes[i]
                .parent
                .map_or(local, |parent| mul(&p.worlds[parent], &local));
        }
        for (i, &node) in self.joints.iter().enumerate() {
            p.joint_matrices[i] = mul(&p.worlds[node], &self.inverse_bind[i]);
        }
        self.deform(p);
    }
    /// Apply already evaluated joint world transforms and corrective weights.
    /// This bypasses clip sampling; after pose creation, successful calls allocate nothing.
    pub fn pose_from_joints(
        &self,
        worlds: &[Mat4],
        weights: &[f32],
        p: &mut Pose,
    ) -> Result<(), String> {
        if worlds.len() != self.joints.len()
            || weights.len() != self.morphs.len()
            || p.positions.len() != self.vertices.len()
            || p.normals.len() != self.vertices.len()
            || p.joint_matrices.len() != self.joints.len()
            || p.morph_weights.len() != self.morphs.len()
            || p.worlds.len() != self.nodes.len()
            || worlds
                .iter()
                .flatten()
                .chain(weights.iter())
                .any(|v| !v.is_finite())
        {
            return Err("procedural pose dimensions or nonfinite values".into());
        }
        p.morph_weights.copy_from_slice(weights);
        for (i, &node) in self.joints.iter().enumerate() {
            p.worlds[node] = worlds[i];
            p.joint_matrices[i] = mul(&worlds[i], &self.inverse_bind[i]);
        }
        self.deform(p);
        Ok(())
    }
    fn deform(&self, p: &mut Pose) {
        for (o, v) in p.positions.iter_mut().zip(&self.vertices) {
            *o = v.position;
        }
        for (m, &w) in self.morphs.iter().zip(&p.morph_weights) {
            if w != 0. {
                for &(i, d) in &m.offsets {
                    for (j, x) in d.iter().enumerate() {
                        p.positions[i][j] += w * x;
                    }
                }
            }
        }
        for (i, v) in self.vertices.iter().enumerate() {
            let source = p.positions[i];
            let mut position = [0.; 3];
            let mut normal = [0.; 3];
            for (&joint, &weight) in v.joints.iter().zip(&v.weights) {
                if weight == 0. {
                    continue;
                }
                let matrix = &p.joint_matrices[usize::from(joint)];
                let a = point(matrix, source);
                let b = vector(matrix, v.normal);
                for k in 0..3 {
                    position[k] += weight * a[k];
                    normal[k] += weight * b[k];
                }
            }
            p.positions[i] = position;
            p.normals[i] = normalize(normal);
        }
    }
    /// Smooth horizontal center of each lowest fingertip patch, with actual
    /// minimum height. Matches the offline hand solver's0.8mm soft-min patch.
    pub fn contact_centers(&self, p: &Pose) -> Vec<[f32; 3]> {
        self.fingertips
            .iter()
            .map(|ids| {
                let low = ids
                    .iter()
                    .map(|&i| p.positions[i][1])
                    .fold(f32::INFINITY, f32::min);
                let mut sum = 0.;
                let mut x = 0.;
                let mut z = 0.;
                for &i in ids {
                    let point = p.positions[i];
                    let w = (-(point[1] - low) / 0.0008).exp();
                    sum += w;
                    x += w * point[0];
                    z += w * point[2];
                }
                [x / sum, low, z / sum]
            })
            .collect()
    }

    /// Lowest actual mesh point in each protected fingertip region, in meters.
    pub fn contacts(&self, p: &Pose) -> Vec<[f32; 3]> {
        self.fingertips
            .iter()
            .map(|ids| {
                ids.iter()
                    .map(|&i| p.positions[i])
                    .min_by(|a, b| a[1].total_cmp(&b[1]))
                    .unwrap_or([0.; 3])
            })
            .collect()
    }
}
#[derive(Clone, Debug)]
pub struct Pose {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub joint_matrices: Vec<Mat4>,
    pub morph_weights: Vec<f32>,
    pub worlds: Vec<Mat4>,
    translations: Vec<[f32; 3]>,
    rotations: Vec<[f32; 4]>,
}

#[cfg(test)]
mod tests;
