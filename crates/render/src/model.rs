//! A noteskin's 3D model on the screen, as StepMania draws one in the note
//! field (`Model::DrawPrimitives`, `Player::PushPlayerMatrix`): seen in
//! the field's perspective (or the `fov` frame's around it), back faces
//! culled, its own faces hiding each other (here sorted far to near rather than through a depth buffer,
//! which StepMania clears after every note anyway), then textured
//! triangles in the skin's passes. `docs/plans/models.md`.

use ddi_actor::{FrameCamera, ModelDraw, Perspective, View, mul4, transform_point};

use crate::scene::FieldGeometry;

/// The camera's distance from the screen in StepMania's pixels: a 45°
/// field of view across its 16:9 screen, 480 high
/// (`RageDisplay::LoadMenuPerspective`: `(W/2) / tan 22.5°`, with
/// `tan 22.5° = √2 − 1`; about 1030.87).
const SM_DISTANCE: f32 = 427.0 * (std::f32::consts::SQRT_2 + 1.0);

/// The field's perspective.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// The vanishing point, in screen pixels: the field's centre.
    pub vanish: [f32; 2],
    /// From the camera to the screen, in screen pixels.
    pub distance: f32,
}

impl Camera {
    /// The camera of a field: on its centre, as far from the screen in
    /// arrows as StepMania's is from its 64-pixel arrows.
    pub fn of(geo: &FieldGeometry) -> Camera {
        Camera {
            vanish: [geo.width / 2.0, geo.height / 2.0],
            distance: SM_DISTANCE * geo.arrow / 64.0,
        }
    }

    /// StepMania's 16:9 screen, 480 high, as this camera sees it: centred
    /// on its vanishing point, at the scale its distance is to
    /// StepMania's. What a skin's `fov` frames and shadows measure in.
    pub fn view(&self) -> View {
        let k = self.distance / SM_DISTANCE;
        let size = [854.0, 480.0];
        View {
            transform: [
                k,
                0.0,
                0.0,
                k,
                self.vanish[0] - k * size[0] / 2.0,
                self.vanish[1] - k * size[1] / 2.0,
            ],
            size,
        }
    }

    /// Where a point (screen pixels, z toward the viewer) is seen, and its
    /// `w`; `None` outside [`Camera::perspective`]'s frustum.
    pub fn project(&self, p: [f32; 3]) -> Option<([f32; 2], f32)> {
        self.perspective().project(p)
    }

    /// This camera as `Player::PushPlayerMatrix`'s `LoadMenuPerspective(45,
    /// …)` sets it, the same as an `fov(45)` frame on the field's center
    /// ([`Perspective::new`]): the frustum of `GetFrustumMatrix(…, 1,
    /// fDistCameraFromImage + 1000)` (`RageDisplay.cpp`), its near plane
    /// one of StepMania's pixels in front of the eye and its far one 1000
    /// behind the screen, scaled with it.
    pub fn perspective(&self) -> Perspective {
        let k = self.distance / SM_DISTANCE;
        Perspective {
            vanish: self.vanish,
            distance: self.distance,
            near: k,
            far: self.distance + 1000.0 * k,
        }
    }
}

/// One triangle seen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    /// Index of its mesh.
    pub mesh: usize,
    pub points: [[f32; 2]; 3],
    pub w: [f32; 3],
    pub uv: [[f32; 2]; 3],
    /// How far it is from the eye (its corners' distances summed, in the
    /// camera's own measure), for drawing far to near.
    pub depth: f32,
}

/// What sees a model's points: StepMania loads one projection for an
/// `fov` frame and everything inside it (`ActorFrame::BeginDraw` →
/// `LoadMenuPerspective`), so a model is seen as the quads beside it.
#[derive(Clone, Copy, Debug)]
enum Eye {
    Perspective(Perspective),
    /// `fov(0)`: `GetOrthoMatrix(0, W, H, 0, -1000, +1000)`, z within
    /// `depth` of the screen.
    Orthographic {
        depth: f32,
    },
}

impl Eye {
    /// The `fov` frame's camera around `model`, or `camera` (the field's)
    /// under none.
    fn of(model: &ModelDraw, camera: &Camera) -> Eye {
        match model.camera {
            FrameCamera::Drawer => Eye::Perspective(camera.perspective()),
            FrameCamera::Perspective(p) => Eye::Perspective(p),
            FrameCamera::Orthographic { depth } => Eye::Orthographic { depth },
        }
    }

    /// Where `p` is seen, its `w` and its distance from the eye; `None`
    /// outside the depth seen.
    fn see(&self, p: [f32; 3]) -> Option<([f32; 2], f32, f32)> {
        match self {
            Eye::Perspective(c) => c.project(p).map(|(at, w)| (at, w, w)),
            Eye::Orthographic { depth } => {
                (p[2].abs() <= *depth).then_some(([p[0], p[1]], 1.0, -p[2]))
            }
        }
    }
}

/// `model`'s triangles facing the camera (the `fov` frame's around it, or
/// `camera` under none), far to near (a model's own
/// triangles in a mesh order where they tie), each corner's texture
/// coordinates moved by its material's translation and `offset` (the
/// note's color) except along the axes its vertex fixes.
pub fn triangles(model: &ModelDraw, camera: &Camera, offset: [f32; 2], out: &mut Vec<Seen>) {
    triangles_with(model, camera, offset, out, &mut Vec::new());
}

/// Where a model's vertices are seen, their `w` and distances from the
/// eye.
pub(crate) type Corners = Vec<Option<([f32; 2], f32, f32)>>;

/// [`triangles`], with a scratch kept between calls for the corners.
pub(crate) fn triangles_with(
    model: &ModelDraw,
    camera: &Camera,
    offset: [f32; 2],
    out: &mut Vec<Seen>,
    corners: &mut Corners,
) {
    let start = out.len();
    let eye = Eye::of(model, camera);
    for (i, mesh) in model.geometry.meshes.iter().enumerate() {
        let translation = mesh
            .material
            .and_then(|m| model.materials.get(m))
            .map_or([0.0; 2], |m| m.2);
        let shift = [translation[0] + offset[0], translation[1] + offset[1]];
        // A mesh that follows a bone is drawn under it (`DrawMesh`); a
        // vertex that follows its own is moved by it first
        // (`UpdateTempGeometry`).
        let placed = match mesh.bone.and_then(|b| model.bones.get(b)) {
            Some(bone) => mul4(&model.transform, bone),
            None => model.transform,
        };
        corners.clear();
        corners.extend(mesh.vertices.iter().map(|v| {
            let pos = match v.bone.and_then(|b| model.bones.get(b)) {
                Some(bone) => transform_point(bone, v.pos),
                None => v.pos,
            };
            eye.see(transform_point(&placed, pos))
        }));
        for t in &mesh.triangles {
            let mut points = [[0.0; 2]; 3];
            let mut w = [0.0; 3];
            let mut uv = [[0.0; 2]; 3];
            let mut depth = 0.0;
            let mut whole = true;
            for (c, &index) in t.iter().enumerate() {
                let index = usize::from(index);
                let (Some(Some((at, cw, far))), Some(v)) =
                    (corners.get(index), mesh.vertices.get(index))
                else {
                    whole = false;
                    break;
                };
                points[c] = *at;
                w[c] = *cw;
                depth += *far;
                uv[c] = [
                    v.uv[0] + if v.fixed[0] { 0.0 } else { shift[0] },
                    v.uv[1] + if v.fixed[1] { 0.0 } else { shift[1] },
                ];
            }
            // Front faces turn counter-clockwise as seen (MilkShape's y up,
            // the screen's y down: a negative area here); a corner that is
            // not a number drops the triangle.
            let front = area(&points) < 0.0;
            if !whole || !front {
                continue;
            }
            out.push(Seen {
                mesh: i,
                points,
                w,
                uv,
                depth,
            });
        }
    }
    out[start..].sort_by(|a, b| b.depth.total_cmp(&a.depth));
}

/// Twice the signed area, positive when the corners turn clockwise as seen
/// (y down).
fn area(p: &[[f32; 2]; 3]) -> f32 {
    (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1])
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ddi_actor::Blend;
    use ddi_actor::model::{Geometry, Mesh, Vertex};

    use super::*;

    fn vertex(pos: [f32; 3], uv: [f32; 2], fixed: [bool; 2]) -> Vertex {
        Vertex {
            pos,
            uv,
            bone: None,
            fixed,
        }
    }

    /// A unit square at z (MilkShape's y up, counter-clockwise from the
    /// viewer) and the same square facing away, one mesh each.
    fn squares(z: f32) -> ModelDraw {
        let vertices = vec![
            vertex([-1.0, -1.0, z], [0.0, 1.0], [false; 2]),
            vertex([1.0, -1.0, z], [1.0, 1.0], [true, false]),
            vertex([1.0, 1.0, z], [1.0, 0.0], [false, true]),
            vertex([-1.0, 1.0, z], [0.0, 0.0], [false; 2]),
        ];
        let front = Mesh {
            name: "front".into(),
            material: Some(0),
            vertices: vertices.clone(),
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            bone: None,
        };
        let back = Mesh {
            name: "back".into(),
            material: None,
            vertices,
            triangles: vec![[0, 2, 1], [0, 3, 2]],
            bone: None,
        };
        let mut transform = [0.0; 16];
        // 10 pixels a unit, y flipped (`Scale(1,-1,1)`), at (100, 100).
        transform[0] = 10.0;
        transform[5] = -10.0;
        transform[10] = 10.0;
        transform[12] = 100.0;
        transform[13] = 100.0;
        transform[15] = 1.0;
        ModelDraw {
            transform,
            geometry: Arc::new(Geometry {
                meshes: vec![front, back],
            }),
            materials: vec![(Some(0), [1.0; 4], [0.25, 0.5])],
            bones: Vec::new(),
            diffuse: [1.0; 4],
            glow: [1.0, 1.0, 1.0, 0.0],
            blend: Blend::Normal,
            depth: Default::default(),
            camera: FrameCamera::Drawer,
        }
    }

    const CAMERA: Camera = Camera {
        vanish: [100.0, 100.0],
        distance: 1000.0,
    };

    #[test]
    fn the_screen_plane_is_seen_one_to_one_and_depth_shrinks_toward_the_centre() {
        assert_eq!(
            CAMERA.project([50.0, 120.0, 0.0]),
            Some(([50.0, 120.0], 1.0))
        );
        let (at, w) = CAMERA.project([-200.0, 100.0, -500.0]).unwrap();
        assert!((at[0] + 100.0).abs() < 1e-3 && at[1] == 100.0 && w == 1.5);
        // StepMania's far plane, 1000 of its pixels behind the screen.
        assert_eq!(CAMERA.project([0.0, 0.0, -980.0]), None);
        let (at, _) = CAMERA.project([0.0, 100.0, 500.0]).unwrap();
        assert_eq!(at, [-100.0, 100.0]);
        assert_eq!(CAMERA.project([0.0, 0.0, 999.5]), None);
        assert_eq!(CAMERA.project([0.0, 0.0, -3000.0]), None);
    }

    #[test]
    fn back_faces_are_culled_and_color_moves_unfixed_coordinates() {
        let mut out = Vec::new();
        triangles(&squares(0.0), &CAMERA, [0.5, 0.0], &mut out);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|s| s.mesh == 0));
        let first = out[0];
        assert_eq!(first.points, [[90.0, 110.0], [110.0, 110.0], [110.0, 90.0]]);
        assert_eq!(first.w, [1.0; 3]);
        // The material's (0.25, 0.5) and the note's (0.5, 0): vertex 1
        // keeps its u, vertex 2 its v.
        assert_eq!(first.uv, [[0.75, 1.5], [1.0, 1.5], [1.75, 0.0]]);

        // Mirrored, the other mesh faces the viewer.
        let mut mirrored = squares(0.0);
        mirrored.transform[0] = -10.0;
        out.clear();
        triangles(&mirrored, &CAMERA, [0.0; 2], &mut out);
        assert!(out.len() == 2 && out.iter().all(|s| s.mesh == 1));
    }

    #[test]
    fn a_transform_that_is_not_a_number_draws_nothing() {
        let mut model = squares(0.0);
        model.transform[0] = f32::NAN;
        let mut out = Vec::new();
        triangles(&model, &CAMERA, [0.0; 2], &mut out);
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn triangles_are_drawn_far_to_near() {
        // A box's two faces seen through each other: tilted squares at
        // two depths, given near first.
        let mut model = squares(0.0);
        let geometry = Arc::make_mut(&mut model.geometry);
        let mut far = geometry.meshes[0].clone();
        for v in &mut far.vertices {
            v.pos[2] = -5.0;
        }
        geometry.meshes[1] = far;
        let mut out = Vec::new();
        triangles(&model, &CAMERA, [0.0; 2], &mut out);
        let meshes: Vec<usize> = out.iter().map(|s| s.mesh).collect();
        assert_eq!(meshes, [1, 1, 0, 0]);
        assert!(out[0].w[0] > out[3].w[0]);
        // Orthographic (`fov(0)`), by depth alone.
        model.camera = FrameCamera::Orthographic { depth: 1000.0 };
        out.clear();
        triangles(&model, &CAMERA, [0.0; 2], &mut out);
        let meshes: Vec<usize> = out.iter().map(|s| s.mesh).collect();
        assert_eq!(meshes, [1, 1, 0, 0]);
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-4 * b.abs().max(1.0)
    }

    #[test]
    fn the_fields_camera_is_an_fov_45_frame_on_its_center() {
        let camera = Camera {
            vanish: [300.0, 200.0],
            distance: SM_DISTANCE * 1.5,
        };
        let view = camera.view();
        let frame = Perspective::new(&view, 45.0, [427.0, 240.0]).unwrap();
        let field = camera.perspective();
        assert!(close(field.vanish[0], frame.vanish[0]) && close(field.vanish[1], frame.vanish[1]));
        assert!(close(field.distance, frame.distance), "{field:?} {frame:?}");
        assert!(close(field.near, frame.near) && close(field.near, 1.5));
        assert!(close(field.far, frame.far), "{field:?} {frame:?}");
    }

    /// How many of [`squares`]' triangles at `z` are seen.
    fn seen_at(z: f32, camera: FrameCamera) -> usize {
        let mut model = squares(z);
        model.camera = camera;
        let mut out = Vec::new();
        triangles(&model, &CAMERA, [0.0; 2], &mut out);
        out.len()
    }

    #[test]
    fn models_under_an_fov_frame_are_clipped_by_its_frustum() {
        // A 90° frame on a target at twice the screen's scale, its
        // vanishing point on the squares: the eye 854 target pixels away,
        // its planes 2 in front of it and 2000 behind the screen
        // (`GetFrustumMatrix(…, 1, fDistCameraFromImage + 1000)`), as the
        // frame's quads have them.
        let view = View {
            transform: [2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
            ..View::default()
        };
        let p = Perspective::new(&view, 90.0, [50.0, 50.0]).unwrap();
        assert!(close(p.distance, 854.0) && close(p.near, 2.0));
        assert!(close(p.far, 854.0 + 2000.0));
        let frame = FrameCamera::Perspective(p);
        // The squares are 10 target pixels a unit deep: 1.5 and 2.5 from
        // the eye.
        assert_eq!(seen_at(85.25, frame), 0);
        assert_eq!(seen_at(85.15, frame), 2);
        // 2844 and 2864 from the eye: within the frame's far plane, then
        // past it; the field's camera (1000 away) would drop both.
        assert_eq!(seen_at(-199.0, frame), 2);
        assert_eq!(seen_at(-201.0, frame), 0);
        assert_eq!(seen_at(-199.0, FrameCamera::Drawer), 0);
    }

    #[test]
    fn models_under_an_fov_0_frame_are_orthographic() {
        let flat = FrameCamera::Orthographic { depth: 1000.0 };
        let mut model = squares(50.0);
        model.camera = flat;
        let mut out = Vec::new();
        triangles(&model, &CAMERA, [0.0; 2], &mut out);
        // Near the eye of the field's camera, but seen as it is.
        assert_eq!(
            out[0].points,
            [[90.0, 110.0], [110.0, 110.0], [110.0, 90.0]]
        );
        assert_eq!(out[0].w, [1.0; 3]);
        // Its depth: ±1000 pixels.
        assert_eq!(seen_at(99.0, flat), 2);
        assert_eq!(seen_at(-99.0, flat), 2);
        assert_eq!(seen_at(101.0, flat), 0);
        assert_eq!(seen_at(-101.0, flat), 0);
    }
}
