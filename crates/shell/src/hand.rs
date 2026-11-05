//! The gameplay hand picture, shared by desktop and browser.
use ddi_character::appearance::{Appearance, ContactLights, UP_TILE_CENTER};
use ddi_character::{Character, Pose};
use ddi_render::{
    Painter,
    character::{Camera, CharacterScene, Presentation, SOURCE, animated_camera, project},
};
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
    pub debug: serde_json::Value,
}

type DrawKey = ([u32; 2], [f32; 4], Option<f32>, Option<Appearance>);
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
        self.picture = picture;
    }
    /// Draw the next frame even if its time and camera repeat (a pose replay
    /// alternating between streams at one instant).
    pub fn invalidate(&mut self) {
        self.last = None;
    }
    /// Drop device-owned resources even when a paused picture has not changed.
    pub fn device_changed(&mut self) {
        self.scene = None;
        self.last = None;
    }
    pub fn release(&mut self, device: &wgpu::Device, painter: &mut Painter) {
        painter.set_external_image(device, SOURCE, None);
        self.scene = None;
        self.last = None;
    }
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        painter: &mut Painter,
    ) {
        let Some(p) = self.picture.as_ref() else {
            if self.scene.is_some() {
                self.release(device, painter);
            }
            return;
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
            return;
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
            size,
            [p.seconds, p.yaw, p.pitch, p.distance],
            p.ground_y,
            p.appearance,
        );
        if self.last == Some(key) {
            return;
        }

        if let Some(external) = &p.procedural {
            let external = external.borrow();
            if let Err(e) = asset.pose_from_joints(&external.worlds, &external.weights, pose) {
                self.error = Some(e);
                return;
            }
        } else {
            asset.sample(p.seconds, pose);
        }
        let scene = self.scene.as_mut().unwrap();
        if p.appearance.is_some() {
            self.lights.update(p.seconds, &asset.contacts(pose));
        }
        scene.draw_styled(
            device,
            queue,
            encoder,
            asset,
            pose,
            Presentation {
                camera: Camera {
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
            },
        );
        painter.set_external_image(device, SOURCE, scene.texture());
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
            contacts: asset.contacts(pose),
            up: p.appearance.filter(|a| a.mat).and_then(|a| {
                project(
                    animated_camera(
                        Camera {
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
                    UP_TILE_CENTER,
                )
            }),
        };
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
            return serde_json::json!({"error":self.error,"rolling":p.procedural.as_ref().map(|p|p.borrow().debug.clone())});
        };
        let Some(pose) = &self.pose else {
            return serde_json::Value::Null;
        };
        let anchors = a.contact_centers(pose);
        serde_json::json!({"appearance":p.appearance,"tiles":self.lights.values,"tile_heights_m":ddi_character::appearance::tile_heights(&a.contacts(pose)),"strikes":self.lights.strikes,"up":p.report.borrow().up,"rect":p.rect,"time":p.seconds,"playing":p.playing,"yaw":p.yaw,"pitch":p.pitch,"distance":p.distance,"vertices":a.vertices.len(),"triangles":a.indices.len()/3,"jointCount":a.joint_names.len(),"morphCount":a.morph_count(),"drawn":self.drawn,"contacts":a.contacts(pose),"anchors":anchors,"positions":if p.playing{None}else{Some(&pose.positions)},"error":self.error,"rolling":p.procedural.as_ref().map(|p|p.borrow().debug.clone())})
    }
}
