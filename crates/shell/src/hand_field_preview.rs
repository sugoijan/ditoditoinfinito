//! Options' standing reference, rendered below the same note band as gameplay.
//! The reference is fitted once; no chart planning or per-frame solving.
use super::hand::{Picture, ProceduralPose, Report};
use ddi_character::appearance::{Geometry, HandFrame};
use ddi_ui::state::settings::Settings;
use std::{cell::RefCell, rc::Rc};
#[derive(Default)]
pub struct HandPreview {
    host: super::hand::Host,
    report: Rc<RefCell<Report>>,
    enabled: bool,
    frame: HandFrame,
    mat: Option<Geometry>,
    reference: Option<Rc<RefCell<ProceduralPose>>>,
}
impl HandPreview {
    pub fn for_new_device(&self) -> Self {
        Self {
            enabled: self.enabled,
            frame: self.frame,
            mat: self.mat,
            ..Self::default()
        }
    }
    pub fn release_targets(&mut self) {
        self.host = Default::default();
    }
    /// The hand under `settings`, shown on the style the preview draws.
    pub fn set_settings(&mut self, settings: &Settings) {
        self.enabled = settings.dancing_hand_for(&settings.style);
        self.frame = settings.hand_frame();
        let mat = ddi_hand_plan::layout_geometry(&settings.style).ok();
        if self.mat != mat {
            self.mat = mat;
            self.reference = None;
        }
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
        let mat = self.mat.unwrap_or_else(Geometry::single);
        let look = super::dance::gameplay_look();
        let rect = self.frame.rect(size);
        self.host.set(Some(Picture {
            mat,
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
                        let model = super::dance_runtime::bundled_model();
                        let mut plan = if mat.pads()==1 {
                            ddi_hand_pose::Plan::empty(3.)
                        } else {
                            ddi_hand_pose::Plan::load(&serde_json::to_vec(&serde_json::json!({
                                "version":2,"layout":"dance-double","mat":mat,"duration":3.,
                                "initial":{"L":6,"R":13},"tracks":{"L":[],"R":[]},"events":[],
                                "placement":{"tile_pitch_mm":24.,"panel_anchor_mm":15.,"center_offset_mm":9.,"other_offset_mm":5.5}
                            })).unwrap()).expect("double standing reference")
                        };
                        if mat.pads()==2 {plan.center_contacts(None);}
                        let q = plan.seed(&model, 0., None);
                        Rc::new(RefCell::new(ProceduralPose {
                            worlds: model.joint_worlds(&q),
                            weights: model.morph_weights(&q),
                        }))
                    })
                    .clone(),
            ),
            report: self.report.clone(),
        }));
        // The standing pose repeats: the renderer keeps its copy.
        if !self.host.draw(device, queue, encoder) && renderer.place_field_underlay(rect, 1.) {
            return;
        }
        renderer.set_field_underlay(device, encoder, self.host.texture().map(|t| (t, rect, 1.)));
    }
}
