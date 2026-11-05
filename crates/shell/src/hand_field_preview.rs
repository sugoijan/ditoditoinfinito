//! Options' standing reference, rendered below the same note band as gameplay.
//! The reference is fitted once; no chart planning or per-frame solving.
use super::hand::{Picture, ProceduralPose, Report};
use ddi_character::appearance::HandFrame;
use ddi_ui::state::settings::Settings;
use std::{cell::RefCell, rc::Rc};
#[derive(Default)]
pub struct HandPreview {
    host: super::hand::Host,
    painter: Option<ddi_render::Painter>,
    report: Rc<RefCell<Report>>,
    enabled: bool,
    frame: HandFrame,
    reference: Option<Rc<RefCell<ProceduralPose>>>,
}
impl HandPreview {
    pub fn for_new_device(&self) -> Self {
        Self {
            enabled: self.enabled,
            frame: self.frame,
            ..Self::default()
        }
    }
    pub fn release_targets(&mut self) {
        self.host = Default::default();
        self.painter = None;
    }
    pub fn set_settings(&mut self, settings: &Settings, layout: &str) {
        self.enabled =
            settings.dancing_hand_for(layout) && settings.dancing_hand_for(&settings.style);
        self.frame = settings.hand_frame();
    }
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        renderer: &mut ddi_render::Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        size: [u32; 2],
        _time: f64,
    ) {
        if !self.enabled {
            renderer.set_field_underlay(device, encoder, None);
            return;
        }
        let look = super::dance::gameplay_look();
        let rect = self.frame.rect(size);
        self.host.set(Some(Picture {
            rect,
            playing: false,
            seconds: 0.,
            yaw: look.camera[0],
            pitch: look.camera[1],
            distance: look.camera[2],
            appearance: Some(look.appearance),
            ground_y: Some(0.98),
            procedural: Some(
                self.reference
                    .get_or_insert_with(|| {
                        let model = ddi_hand_pose::Model::load(include_bytes!(
                            "../../../assets/hand/model.json"
                        ))
                        .expect("bundled hand calibration");
                        let plan = ddi_hand_pose::Plan::empty(3.);
                        let q = plan.seed(&model, 0., None);
                        Rc::new(RefCell::new(ProceduralPose {
                            worlds: model.joint_worlds(&q),
                            weights: model.morph_weights(&q),
                            debug: serde_json::Value::Null,
                        }))
                    })
                    .clone(),
            ),
            report: self.report.clone(),
        }));
        let painter = self.painter.get_or_insert_with(|| {
            ddi_render::Painter::new(device, queue, wgpu::TextureFormat::Rgba8Unorm)
        });
        painter.submitted();
        self.host.draw(device, queue, encoder, painter);
        renderer.set_field_underlay(device, encoder, self.host.texture().map(|t| (t, rect, 1.)));
    }
}
