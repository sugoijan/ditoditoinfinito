//! A model's data as StepMania holds it for noteskins
//! (`docs/plans/models.md`): meshes and materials (`RageModelGeometry`,
//! `Model`), bones posed at a frame (`Model::SetBones`, `src/Model.cpp`),
//! and the animated textures materials name (`AnimatedTexture`,
//! `src/ModelTypes.cpp`); 5_1-new @ 825467b. The MilkShape files that fill
//! them are read by `ddi_noteskin::model`.

use crate::actor::{Mat4, mul4};

/// A model's meshes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Geometry {
    pub meshes: Vec<Mesh>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    pub name: String,
    /// Index into the materials; `None`: drawn untextured, grey.
    pub material: Option<usize>,
    pub vertices: Vec<Vertex>,
    pub triangles: Vec<[u16; 3]>,
    /// The bone every vertex of the mesh follows (`OptimizeBones`), when
    /// they all follow the same one; the vertices' own are then `None`.
    pub bone: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    /// MilkShape's coordinates: y up, in the skin's pixels.
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub bone: Option<usize>,
    /// The note's colour translation does not move this vertex's texture
    /// coordinate in u, in v (vertex flags 1 and 2, `TextureMatrixScale`).
    pub fixed: [bool; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub name: String,
    pub ambient: [f32; 4],
    pub diffuse: [f32; 4],
    pub specular: [f32; 4],
    pub emissive: [f32; 4],
    pub shininess: f32,
    /// Parsed; StepMania never uses it.
    pub transparency: f32,
    /// Relative to the materials file, `/`-separated; empty: none.
    pub texture: String,
    /// Drawn added over the texture by StepMania; empty: none.
    pub alpha_texture: String,
}

impl Material {
    /// The colour a mesh of this material has, unlit (`SetMaterial` with
    /// lighting off: ambient + diffuse + emissive, the diffuse's alpha),
    /// before the actor's diffuse multiplies it; the product is clamped,
    /// as a vertex colour is.
    pub fn unlit_color(&self) -> [f32; 4] {
        let c = |i: usize| self.ambient[i] + self.diffuse[i] + self.emissive[i];
        [c(0), c(1), c(2), self.diffuse[3]]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bone {
    pub name: String,
    /// Index of the parent bone; `None`: a root.
    pub parent: Option<usize>,
    pub position: [f32; 3],
    /// Radians, about x, y and z.
    pub rotation: [f32; 3],
    /// `(frame, position)`.
    pub position_keys: Vec<(f32, [f32; 3])>,
    /// `(frame, radians about x, y, z)`.
    pub rotation_keys: Vec<(f32, [f32; 3])>,
}

/// A model's bones: its one animation, "default".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Animation {
    pub bones: Vec<Bone>,
    /// The last key's frame, whole frames (30 a second).
    pub total_frames: u32,
}

/// A bone at rest (`Model::PlayAnimation`): relative to its parent, and in
/// the model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rest {
    pub relative: Mat4,
    pub absolute: Mat4,
}

impl Animation {
    /// Every bone at rest, in order: a parent listed after its child is
    /// taken as it is then (the identity), as StepMania's loop does.
    pub fn rest(&self) -> Vec<Rest> {
        let mut rest: Vec<Rest> = Vec::with_capacity(self.bones.len());
        for b in &self.bones {
            let mut relative = angles4(b.rotation);
            relative[12..15].copy_from_slice(&b.position);
            let absolute = match b.parent.and_then(|p| rest.get(p)) {
                Some(parent) => mul4(&parent.absolute, &relative),
                None => relative,
            };
            rest.push(Rest { relative, absolute });
        }
        rest
    }

    /// `Model::SetBones`: each bone's matrix at `frame` into `bones` (the
    /// last ones: a parent listed after its child is taken as it was).
    /// A bone without keys stays at rest; between keys, positions are
    /// interpolated and rotations slerped; past the last, the last holds
    /// (a frame that is not a number is past every key).
    pub fn pose(&self, rest: &[Rest], frame: f32, bones: &mut Vec<Mat4>) {
        bones.resize(self.bones.len(), IDENTITY4);
        for (i, (b, r)) in self.bones.iter().zip(rest).enumerate() {
            if b.position_keys.is_empty() && b.rotation_keys.is_empty() {
                bones[i] = r.absolute;
                continue;
            }
            let position = around(&b.position_keys, frame).map_or([0.0; 3], |(a, z, s)| {
                [0, 1, 2].map(|k| a[k] + (z[k] - a[k]) * s)
            });
            let rotation = around(&b.rotation_keys, frame)
                .map_or([0.0, 0.0, 0.0, 1.0], |(a, z, s)| {
                    slerp(quaternion(*a), quaternion(*z), s)
                });
            let mut key = quaternion_matrix(rotation);
            key[12..15].copy_from_slice(&position);
            let relative = mul4(&r.relative, &key);
            bones[i] = match b.parent.and_then(|p| bones.get(p)) {
                Some(parent) => mul4(parent, &relative),
                None => relative,
            };
        }
    }
}

/// `PlayAnimation`'s "subtract out the bone's resting position": vertices
/// that follow a bone of their own (not their mesh's) made relative to it.
pub fn to_rest(geometry: &mut Geometry, rest: &[Rest]) {
    for mesh in &mut geometry.meshes {
        for v in &mut mesh.vertices {
            let Some(r) = v.bone.and_then(|b| rest.get(b)) else {
                continue;
            };
            let m = &r.absolute;
            let p = [0, 1, 2].map(|k| v.pos[k] - m[12 + k]);
            // The rotation's transpose, its inverse.
            v.pos = [0, 1, 2].map(|k| m[k * 4] * p[0] + m[k * 4 + 1] * p[1] + m[k * 4 + 2] * p[2]);
        }
    }
}

/// The keys either side of `frame` and how far between them it is
/// (`SetBones`' search: the first key at or past it, and the one before).
fn around<T>(keys: &[(f32, T)], frame: f32) -> Option<(&T, &T, f32)> {
    match keys.iter().position(|k| k.0 >= frame) {
        Some(0) => Some((&keys[0].1, &keys[0].1, 0.0)),
        Some(i) => {
            let (a, z) = (&keys[i - 1], &keys[i]);
            Some((&a.1, &z.1, (frame - a.0) / (z.0 - a.0)))
        }
        None => keys.last().map(|k| (&k.1, &k.1, 0.0)),
    }
}

const IDENTITY4: Mat4 = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

/// `RageMatrixAngles` (radians about x, y and z): StepMania's rows are
/// this crate's columns.
fn angles4(a: [f32; 3]) -> Mat4 {
    let (sr, cr) = a[0].sin_cos();
    let (sp, cp) = a[1].sin_cos();
    let (sy, cy) = a[2].sin_cos();
    [
        cp * cy,
        cp * sy,
        -sp,
        0.0,
        sr * sp * cy - cr * sy,
        sr * sp * sy + cr * cy,
        sr * cp,
        0.0,
        cr * sp * cy + sr * sy,
        cr * sp * sy - sr * cy,
        cr * cp,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}

/// `RageQuatFromHPR` (radians about x, y and z): `[x, y, z, w]`.
fn quaternion(a: [f32; 3]) -> [f32; 4] {
    let (sx, cx) = (a[0] / 2.0).sin_cos();
    let (sy, cy) = (a[1] / 2.0).sin_cos();
    let (sz, cz) = (a[2] / 2.0).sin_cos();
    [
        sx * cy * cz - cx * sy * sz,
        cx * sy * cz + sx * cy * sz,
        cx * cy * sz - sx * sy * cz,
        cx * cy * cz + sx * sy * sz,
    ]
}

/// `RageMatrixFromQuat`.
fn quaternion_matrix(q: [f32; 4]) -> Mat4 {
    let [x, y, z, w] = q;
    let (xx, xy, xz) = (x * (x + x), x * (y + y), x * (z + z));
    let (wx, wy, wz) = (w * (x + x), w * (y + y), w * (z + z));
    let (yy, yz, zz) = (y * (y + y), y * (z + z), z * (z + z));
    [
        1.0 - (yy + zz),
        xy + wz,
        xz - wy,
        0.0,
        xy - wz,
        1.0 - (xx + zz),
        yz + wx,
        0.0,
        xz + wy,
        yz - wx,
        1.0 - (xx + yy),
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}

/// `RageQuatSlerp`.
fn slerp(from: [f32; 4], to: [f32; 4], t: f32) -> [f32; 4] {
    let mut cos = (0..4).map(|k| from[k] * to[k]).sum::<f32>();
    let mut to = to;
    if cos < 0.0 {
        cos = -cos;
        to = to.map(|v| -v);
    }
    let (a, b) = if cos < 0.9999 {
        let omega = cos.acos();
        let sin = omega.sin();
        (((1.0 - t) * omega).sin() / sin, (t * omega).sin() / sin)
    } else {
        (1.0 - t, t)
    };
    [0, 1, 2, 3].map(|k| a * from[k] + b * to[k])
}

/// A material's texture as StepMania animates it: frames (paths relative
/// to the `.ini`, each shown for its delay) and a scroll.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimatedTexture {
    /// `(path, seconds)`.
    pub frames: Vec<(String, f32)>,
    /// Texture units per cycle of the animation (`TexVelocityX/Y`).
    pub velocity: [f32; 2],
    /// `TexOffsetX/Y`.
    pub offset: [f32; 2],
    /// Per frame, `TranslateX/Y####`.
    pub translate: Vec<[f32; 2]>,
}

impl AnimatedTexture {
    /// A material texture that is a plain image: one frame of a second.
    pub fn image(path: &str) -> AnimatedTexture {
        AnimatedTexture {
            frames: vec![(path.to_string(), 1.0)],
            velocity: [0.0; 2],
            offset: [0.0; 2],
            translate: vec![[0.0; 2]],
        }
    }

    /// Seconds of one cycle.
    pub fn length(&self) -> f32 {
        self.frames.iter().map(|(_, d)| d.max(0.0)).sum()
    }

    /// The frame shown `seconds` into the animation (looping).
    pub fn frame_at(&self, seconds: f32) -> usize {
        let length = self.length();
        if length <= 0.0 || !seconds.is_finite() {
            return 0;
        }
        let mut t = seconds.rem_euclid(length);
        for (i, (_, d)) in self.frames.iter().enumerate() {
            if t < d.max(0.0) {
                return i;
            }
            t -= d.max(0.0);
        }
        self.frames.len().saturating_sub(1)
    }

    /// The texture translation `seconds` into the animation:
    /// velocity × the fraction of the cycle + offset + the frame's own.
    pub fn translation(&self, seconds: f32) -> [f32; 2] {
        let length = self.length();
        let fraction = if length > 0.0 && seconds.is_finite() {
            seconds.rem_euclid(length) / length
        } else {
            0.0
        };
        let own = self
            .translate
            .get(self.frame_at(seconds))
            .copied()
            .unwrap_or([0.0; 2]);
        [
            self.velocity[0] * fraction + self.offset[0] + own[0],
            self.velocity[1] * fraction + self.offset[1] + own[1],
        ]
    }
}
