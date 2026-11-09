//! The gameplay hand picture, shared by desktop and browser.
use ddi_character::appearance::{
    Appearance, ContactLights, Geometry, physical_tile, tile_center, tile_heights_on,
};
use ddi_character::{Character, Pose};
use ddi_render::character::{Camera, CharacterScene, Presentation, animated_camera, project};
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub error: Option<String>,
    pub vertices: usize,
    pub joints: usize,
    pub morphs: usize,
    pub drawn: u64,
    pub revision: u64,
    pub contacts: Vec<[f32; 3]>,
    pub up: Option<[f32; 2]>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub mat: Geometry,
    pub ground_y: Option<f32>,
    pub appearance: Option<ddi_character::appearance::Appearance>,
    pub procedural: Option<Rc<RefCell<ProceduralPose>>>,
    pub rect: [f32; 4],
    pub playing: bool,
    pub seconds: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub report: Rc<RefCell<Report>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ProceduralPose {
    pub worlds: [[f32; 16]; 19],
    pub weights: [f32; 76],
}

type DrawKey = (
    Geometry,
    [u32; 2],
    [f32; 4],
    Option<f32>,
    Option<Appearance>,
);
#[derive(Default)]
pub struct Host {
    picture: Option<Picture>,
    asset: Option<Character>,
    pose: Option<Pose>,
    error: Option<String>,
    scene: Option<CharacterScene>,
    last: Option<DrawKey>,
    lights: ContactLights,
    drawn: u64,
}
impl Host {
    pub fn set(&mut self, picture: Option<Picture>) {
        let same_source = match (self.picture.as_ref(), picture.as_ref()) {
            (Some(a), Some(b)) if a.mat == b.mat => match (&a.procedural, &b.procedural) {
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            },
            _ => false,
        };
        if !same_source {
            self.last = None;
            self.lights = ContactLights::default();
        }
        self.picture = picture;
    }
    /// Draws the picture into [`Host::texture`]; whether it drew (a
    /// picture repeating its time, camera and size keeps the last image).
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
    ) -> bool {
        let Some(p) = self.picture.as_ref() else {
            self.scene = None;
            self.last = None;
            return false;
        };
        if self.asset.is_none() && self.error.is_none() {
            match Character::load(ddi_character::HAND_MODEL) {
                Ok(a) => {
                    self.pose = Some(a.new_pose());
                    self.asset = Some(a);
                }
                Err(e) => self.error = Some(e),
            }
        }

        if let Some(e) = &self.error {
            let mut report = p.report.borrow_mut();
            if report.error.as_ref() != Some(e) {
                report.error = Some(e.clone());
                report.revision += 1;
            }
            return false;
        }
        let asset = self.asset.as_ref().unwrap();
        let pose = self.pose.as_mut().unwrap();
        let fresh = self.scene.as_ref().is_none_or(|s| !s.on(device));
        if fresh {
            self.scene = Some(CharacterScene::new(device, asset));
            self.last = None;
        }
        let requested = [p.rect[2].max(1.), p.rect[3].max(1.)];
        // Fill the destination on large/Retina screens without allocating an
        // unbounded offscreen target or exceeding a WebGL device's limit.
        let limit = device.limits().max_texture_dimension_2d.min(2048) as f32;
        let scale = (limit / requested[0].max(requested[1])).min(1.);
        let size = requested.map(|v| (v * scale).round().max(1.) as u32);
        let key = (
            p.mat,
            size,
            [p.seconds, p.yaw, p.pitch, p.distance],
            p.ground_y,
            p.appearance,
        );
        if self.last == Some(key) {
            return false;
        }

        if let Some(external) = &p.procedural {
            let external = external.borrow();
            if let Err(e) = asset.pose_from_joints(&external.worlds, &external.weights, pose) {
                self.error = Some(e);
                return false;
            }
        } else {
            asset.sample(p.seconds, pose);
        }
        let scene = self.scene.as_mut().unwrap();
        let contacts = asset.contacts(pose);
        if p.appearance.is_some() {
            self.lights.update_on(p.mat, p.seconds, &contacts);
        }
        scene.draw_styled(
            device,
            queue,
            encoder,
            asset,
            pose,
            Presentation {
                camera: Camera {
                    mat: p.mat,
                    yaw: p.yaw,
                    pitch: p.pitch,
                    distance: p.distance,
                    scale: 1.,
                    floor: false,
                    ground_y: p.ground_y,
                },
                size,
                appearance: p.appearance,
                tiles: self.lights.values,
                strikes: self.lights.strikes,
                seconds: p.seconds,
                contacts: &contacts,
            },
        );
        self.last = Some(key);
        self.drawn += 1;
        let revision = p.report.borrow().revision + 1;
        *p.report.borrow_mut() = Report {
            error: None,
            vertices: asset.vertices.len(),
            joints: asset.joint_names.len(),
            morphs: asset.morph_count(),
            drawn: self.drawn,
            revision,
            contacts,
            up: p.appearance.filter(|a| a.mat).and_then(|a| {
                project(
                    animated_camera(
                        Camera {
                            mat: p.mat,
                            ground_y: p.ground_y,
                            yaw: p.yaw,
                            pitch: p.pitch,
                            distance: p.distance,
                            ..Default::default()
                        },
                        Some(a),
                        p.seconds,
                    ),
                    size[0] as f32 / size[1] as f32,
                    {
                        let [x, z] = tile_center(p.mat, 7).expect("UP on first pad");
                        [x, 0., z]
                    },
                )
            }),
        };
        true
    }
    /// Prepared straight-alpha image, for a host's behind-field compositor.
    pub fn texture(&self) -> Option<&wgpu::Texture> {
        self.scene.as_ref().and_then(CharacterScene::texture)
    }
    /// Full positions are exposed only while paused, for native/browser comparisons.
    pub fn debug(&self) -> serde_json::Value {
        let Some(p) = &self.picture else {
            return serde_json::Value::Null;
        };
        let Some(a) = &self.asset else {
            return serde_json::json!({"error":self.error});
        };
        let Some(pose) = &self.pose else {
            return serde_json::Value::Null;
        };
        let anchors = a.contact_centers(pose);
        serde_json::json!({"mat":p.mat,"tile_ids":(0..usize::from(p.mat.pads())*9).map(|i|physical_tile(p.mat,i).unwrap().id()).collect::<Vec<_>>(),"appearance":p.appearance,"tiles":self.lights.values,"tile_heights_m":tile_heights_on(p.mat,&a.contacts(pose)),"strikes":self.lights.strikes,"up":p.report.borrow().up,"rect":p.rect,"time":p.seconds,"playing":p.playing,"yaw":p.yaw,"pitch":p.pitch,"distance":p.distance,"vertices":a.vertices.len(),"triangles":a.indices.len()/3,"jointCount":a.joint_names.len(),"morphCount":a.morph_count(),"drawn":self.drawn,"contacts":a.contacts(pose),"anchors":anchors,"positions":if p.playing{None}else{Some(&pose.positions)},"error":self.error})
    }
}
