//! Chart-derived dancing-hand accompaniment, shared by both shells.
use super::hand::{Picture, Report};
pub use ddi_character::appearance::{BASE_CAMERA, PlayLook};
use ddi_chart::Song;
use ddi_render::{DrawList, Painter, texture_cache::TextureKey};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

/// The painter compositing the picture over a target, only where it is
/// not the field's underlay (no look, or the guide shown).
struct Surface {
    device: wgpu::Device,
    painter: Painter,
    list: DrawList,
    /// The hand's texture bound in the painter, once one was.
    bound: Option<Option<wgpu::Texture>>,
}
#[derive(Default)]
pub struct Preview {
    enabled: bool,
    look: Option<PlayLook>,
    frame: ddi_character::appearance::HandFrame,
    rect: [f32; 4],
    opacity: f32,
    format: Option<wgpu::TextureFormat>,
    runtime: Option<super::dance_runtime::RuntimePreview>,
    runtime_error: Option<String>,
    preload_key: Option<Vec<u8>>,
    preload_ready: bool,
    origin: f64,
    duration: f64,
    failed_at: Option<f64>,
    ever_available: bool,
    host: super::hand::Host,
    surface: Option<Surface>,
    report: Rc<RefCell<Report>>,
    song_time: f64,
    visible: bool,
    /// A stage's place behind the field ([`Preview::set_behind_field`]).
    behind: Option<[f32; 4]>,
}

impl Preview {
    /// Presentation only: does not invalidate or prepare contact/motion buffers.
    pub fn set_look(&mut self, look: Option<PlayLook>) {
        self.look = look;
    }
    /// Apply saved composition without touching prepared motion or a developer camera.
    pub fn configure(&mut self, settings: &ddi_ui::state::settings::Settings) {
        self.frame = settings.hand_frame();
        if self.look.is_none() {
            self.look = Some(gameplay_look());
        }
    }
    /// Where the hand's picture goes on a target of `size`, `[x, y,
    /// width, height]`: behind the field where a stage put it
    /// ([`Preview::set_behind_field`]), else where the settings put it.
    fn frame_rect(&self, size: [u32; 2]) -> [f32; 4] {
        if let Some(rect) = self.behind {
            return rect;
        }
        let (w, h) = (size[0] as f32, size[1] as f32);
        let pw = (w * 0.34).clamp(150., 480.).min(w - 16.);
        let ph = (h * 0.65).min(pw * 1.3);
        if self.look.is_some() {
            self.frame.rect(size)
        } else {
            [w - pw - 12., h - ph - 60., pw, ph]
        }
    }

    /// A multiplayer stage's place for the hand (`[x, y, width, height]`,
    /// the target's pixels: behind the field, as the play study's "Hand
    /// behind field", out of the opponents' miniatures' way); `None`, the
    /// settings' frame. Presentation only.
    pub fn set_behind_field(&mut self, rect: Option<[f32; 4]>) {
        self.behind = rect;
    }

    pub fn behind_field(&self) -> bool {
        self.look.is_some()
    }
    pub fn up_marker(&self) -> Option<[f32; 2]> {
        if !self.visible || !self.look.is_some_and(|l| l.guide) {
            return None;
        }
        self.report
            .borrow()
            .up
            .filter(|p| p.iter().all(|v| (0. ..=1.).contains(v)))
            .map(|p| {
                [
                    self.rect[0] + p[0] * self.rect[2],
                    self.rect[1] + p[1] * self.rect[3],
                ]
            })
    }

    /// Ordinary play uses the Player's effective input, never a second transform.
    pub fn bind_player(
        &mut self,
        player: &ddi_engine::player::Player,
        layout: &str,
        enabled: bool,
        start: Option<f64>,
    ) {
        self.preload_key = None;
        self.preload_ready = false;
        self.enabled = false;
        self.visible = false;
        self.origin = 0.;
        self.duration = 0.;
        self.runtime = None;
        self.runtime_error = None;
        self.failed_at = None;
        self.ever_available = false;
        if !enabled {
            return;
        }
        if start.is_some_and(|t| t > 0.) {
            self.runtime_error =
                Some("dancing hand requires starting at the chart beginning".into());
            return;
        }
        match super::dance_runtime::RuntimePreview::for_player(player, layout) {
            Ok(r) => {
                self.origin = r.origin();
                self.duration = r.duration();
                self.runtime = Some(r);
                self.enabled = true;
            }
            Err(e) => self.runtime_error = Some(e),
        }
    }
    /// Prepare the same effective Player as the eventual session, without audio
    /// scheduling or input capture. Presentation changes reuse the motion buffer.
    /// The hand plays where `settings` accompany the chart's layout
    /// ([`Settings::dancing_hand_for`]), never in calibration; `song` is
    /// shifted by the song offset `shift` only then. Shells call it when the
    /// song is read, when the play's options change and while
    /// [`Preview::needs_preload`].
    ///
    /// [`Settings::dancing_hand_for`]: ddi_ui::state::settings::Settings::dancing_hand_for
    pub fn preload(
        &mut self,
        song: &Song,
        shift: f64,
        chart: usize,
        settings: &ddi_ui::state::settings::Settings,
        config: &ddi_session::play::SessionConfig,
    ) {
        self.configure(settings);
        let enabled = config.calibration.is_none()
            && song
                .charts
                .get(chart)
                .is_some_and(|c| settings.dancing_hand_for(&c.layout));
        if !enabled {
            self.reset_preload();
            self.preload_ready = true;
            self.preload_key = Some(Vec::new());
            return;
        }
        let song = song.shifted(shift);
        let (rules, options) = match config.player_spec(&song, chart, settings) {
            Ok(spec) => spec,
            Err(e) => {
                self.reset_preload();
                self.runtime_error = Some(e);
                self.preload_ready = true;
                self.preload_key = Some(Vec::new());
                return;
            }
        };
        let c = &song.charts[chart];
        let key = serde_json::to_vec(&(
            chart,
            &c.layout,
            ddi_hand_plan::layout_geometry(&c.layout).ok(),
            &c.notes,
            c.timing(&song),
            &rules.id,
            &rules.judge,
            options,
            config.start,
        ))
        .expect("finite play configuration");
        if self.preload_key.as_ref() == Some(&key) {
            return;
        }
        let player = ddi_engine::player::Player::new(&song, chart, rules, options);
        self.bind_player(&player, &c.layout, true, config.start);
        self.preload_key = Some(key);
        self.preload_ready = !self.enabled;
    }

    /// Whether nothing was preloaded since the song or the play changed
    /// ([`Preview::reset_preload`]): the shell calls [`Preview::preload`].
    pub fn needs_preload(&self) -> bool {
        self.preload_key.is_none()
    }

    pub fn preload_pending(&self) -> bool {
        !self.preload_ready && self.runtime_error.is_none()
    }
    /// Bounded cooperative CPU work, then an off-screen first GPU frame. The
    /// prompt may release playback only after that texture has been submitted.
    pub fn preload_work(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        size: [u32; 2],
        clock: &dyn Fn() -> f64,
    ) {
        if self.preload_key.is_none() || !self.preload_pending() {
            return;
        }
        if let Some(r) = self.runtime.as_mut() {
            r.prepare(-1., clock);
            if let Some(e) = r.failure() {
                self.runtime_error = Some(e.to_string());
                return;
            }
            if !r.available {
                return;
            }
        }
        let mut encoder = device.create_command_encoder(&Default::default());
        self.update_picture(device, queue, &mut encoder, format, size, self.origin);
        queue.submit([encoder.finish()]);
        self.preload_ready = self.host.texture().is_some();
        if let Some(e) = self.report.borrow().error.clone() {
            self.runtime_error = Some(e);
        }
    }
    /// Reuse the frozen opening buffer; the next timestamp belongs to audio.
    pub fn start_preloaded(&mut self) {
        if let Some(r) = self.runtime.as_mut() {
            r.start_preloaded();
        }
    }

    pub fn reset_preload(&mut self) {
        self.preload_key = None;
        self.preload_ready = false;
        self.runtime = None;
        self.enabled = false;
        self.runtime_error = None;
        self.failed_at = None;
        self.ever_available = false;
        self.visible = false;
    }

    /// The milliseconds the last frame spent preparing motion.
    pub fn prepare_ms(&self) -> f64 {
        self.runtime.as_ref().map_or(0., |r| r.work_ms)
    }
    /// Numerical preparation runs before drawing, under a host clock shared
    /// by all its stages. Sampling/rendering below never calls the pose solver.
    pub fn prepare(&mut self, song_time: f64, clock: &dyn Fn() -> f64) {
        if self.failed_at.is_some() {
            return;
        }
        if let Some(r) = self.runtime.as_mut() {
            let time = song_time - self.origin;
            r.prepare(time, clock);
            self.ever_available |= r.available;
            if let Some(e) = r.failure() {
                self.runtime_error = Some(e.to_string());
                self.failed_at = Some(time);
            } else if r.starved_frames > 0 {
                self.runtime_error = Some("dancing hand motion buffer exhausted".into());
                self.failed_at = Some(time);
            }
        }
    }
    /// A play's frame before its field is drawn: the motion prepared to
    /// song second `song_time` (under `clock`, host seconds), then the
    /// picture placed under the field as its underlay where it stands
    /// behind it, else no underlay ([`Preview::after_field`] draws it).
    #[allow(clippy::too_many_arguments)]
    pub fn before_field(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        renderer: &mut ddi_render::Renderer,
        format: wgpu::TextureFormat,
        size: [u32; 2],
        song_time: f64,
        clock: &dyn Fn() -> f64,
    ) {
        self.prepare(song_time, clock);
        if self.behind_field() {
            self.draw_underlay(device, queue, encoder, renderer, format, size, song_time);
        } else {
            renderer.set_field_underlay(device, encoder, None);
        }
    }

    /// A play's frame once its field is drawn on `target`: the guide over
    /// it where the picture stands behind the field, else the picture.
    #[allow(clippy::too_many_arguments)]
    pub fn after_field(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        size: [u32; 2],
        song_time: f64,
    ) {
        if self.behind_field() {
            self.draw_guide(device, queue, encoder, target, size);
        } else {
            self.draw(device, queue, encoder, target, format, size, song_time);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        size: [u32; 2],
        song_time: f64,
    ) {
        if let Some((rect, opacity, _)) =
            self.update_picture(device, queue, encoder, format, size, song_time)
        {
            self.composite(device, queue, encoder, target, size, rect, opacity, false);
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn draw_underlay(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        renderer: &mut ddi_render::Renderer,
        format: wgpu::TextureFormat,
        size: [u32; 2],
        song_time: f64,
    ) {
        let ready = self.update_picture(device, queue, encoder, format, size, song_time);
        // The renderer keeps its copy while the picture repeats.
        if let Some((rect, opacity, false)) = ready
            && renderer.place_field_underlay(rect, opacity)
        {
            return;
        }
        let image =
            ready.and_then(|(rect, opacity, _)| self.host.texture().map(|t| (t, rect, opacity)));
        renderer.set_field_underlay(device, encoder, image);
    }
    pub fn draw_guide(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
    ) {
        if self.visible && self.look.is_some_and(|l| l.guide) {
            self.composite(
                device,
                queue,
                encoder,
                target,
                size,
                self.rect,
                self.opacity,
                true,
            );
        }
    }
    fn update_picture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        format: wgpu::TextureFormat,
        size: [u32; 2],
        song_time: f64,
    ) -> Option<([f32; 4], f32, bool)> {
        self.song_time = song_time;
        self.format = Some(format);
        let mut t = song_time - self.origin;
        if self.preload_key.is_some() {
            t = t.max(0.);
        }
        self.visible = self.enabled && (0.0..=self.duration).contains(&t);
        let mut opacity = 1.;
        if let Some(failed) = self.failed_at {
            opacity = (1. - (t - failed) / 0.2).clamp(0., 1.) as f32;
            self.visible &= self.ever_available && opacity > 0.;
        } else {
            self.visible &= self.runtime.as_ref().is_some_and(|r| r.available);
        }
        if !self.visible {
            return None;
        }
        let rect = self.frame_rect(size);
        self.rect = rect;
        let mut camera = self.look.map_or([-25., 14., 0.52], |l| l.camera);
        if let Some([_, _, w, h]) = self.behind {
            camera[2] = behind_field_distance(camera[2], w, h);
        }
        self.host.set(Some(Picture {
            mat: self
                .runtime
                .as_ref()
                .map_or_else(ddi_character::appearance::Geometry::single, |r| r.mat()),
            ground_y: self.look.map(|_| 0.98),
            appearance: self.look.map(|l| l.appearance),
            procedural: self.runtime.as_ref().map(|r| r.payload.clone()),
            rect,
            playing: true,
            seconds: t as f32,
            yaw: camera[0],
            pitch: camera[1],
            distance: camera[2],
            report: self.report.clone(),
        }));
        let drawn = self.host.draw(device, queue, encoder);
        self.opacity = opacity;
        Some((rect, opacity, drawn))
    }
    #[allow(clippy::too_many_arguments)]
    fn composite(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
        rect: [f32; 4],
        opacity: f32,
        guide_only: bool,
    ) {
        let Some(format) = self.format else {
            return;
        };
        if self
            .surface
            .as_ref()
            .is_none_or(|s| s.painter.format() != format || s.device != *device)
        {
            self.surface = Some(Surface {
                device: device.clone(),
                painter: Painter::new(device, queue, format),
                list: DrawList::new(),
                bound: None,
            });
        }
        let s = self.surface.as_mut().unwrap();
        s.painter.submitted();
        let texture = self.host.texture();
        if s.bound.as_ref().is_none_or(|b| b.as_ref() != texture) {
            s.painter
                .set_external_image(device, ddi_render::character::SOURCE, texture);
            s.bound = Some(texture.cloned());
        }
        let [_, _, pw, ph] = rect;
        let texture = s
            .painter
            .texture(TextureKey::drawn(ddi_render::character::SOURCE, pw.max(ph)));
        s.list.clear();
        if !guide_only {
            s.list.image(
                [rect[0], rect[1], rect[0] + pw, rect[1] + ph],
                texture,
                [1., 1., 1., opacity],
            );
        }
        // A compositor decoration, never burned into the character texture.
        if self.look.is_some_and(|l| l.guide)
            && let Some(p) = self
                .report
                .borrow()
                .up
                .filter(|p| p.iter().all(|v| (0. ..=1.).contains(v)))
        {
            let x = rect[0] + p[0] * pw;
            let y = rect[1] + p[1] * ph;
            let gold = [0.95, 0.86, 0.55, opacity];
            s.list.rect([x - 2., y - 2., x + 2., y + 2.], gold);
            // Tiny UI-only UP legend without a platform font dependency.
            for (row, bits) in [0b101110u8, 0b101101, 0b101110, 0b101100, 0b111100]
                .into_iter()
                .enumerate()
            {
                for col in 0..6 {
                    if bits & (1 << (5 - col)) != 0 {
                        let px = x - 7. + col as f32 * 2.5;
                        let py = y - 19. + row as f32 * 2.;
                        s.list.rect([px, py, px + 2., py + 2.], gold);
                    }
                }
            }
        }
        s.painter.draw(
            device,
            queue,
            encoder,
            target,
            size,
            1.,
            None,
            &s.list,
            &mut |_, _| {},
        );
    }
    pub fn debug(&self) -> Value {
        let mut character = self.host.debug();
        if let (Some(r), Some(c)) = (&self.runtime, character.as_object_mut()) {
            c.insert("rolling".into(), r.sample_debug());
        }
        json!({"behind_field":self.behind_field(),"look":self.look,"up_marker":self.up_marker(),"enabled":self.enabled,"visible":self.visible,"song_time":self.song_time,
        "origin":self.origin,"duration":self.duration,"character":character,
        "runtime":self.runtime.as_ref().map_or(Value::Null, |r| r.debug()),
        "runtime_error":self.runtime_error,"preloaded":self.preload_ready,"preload_pending":self.preload_pending(),
        "status":if self.failed_at.is_some() {
            "stopped"
        } else if self.runtime_error.is_some() {
            "unsupported"
        } else if !self.enabled {
            "off"
        } else if self.runtime.as_ref().is_some_and(|r| r.available) {
            "active"
        } else {
            "warming"
        }})
    }
}

/// How much larger the hand is behind the field than its picture would
/// be at the camera's own distance in a frame the band's width
/// (PROVISIONAL, `docs/plans/multiplayer-prototype-progress.md`, M7c): the
/// hand and the mat it plays on fill the room below the judgment, the
/// arm above them cut at the frame's top.
pub const BEHIND_FIELD_ZOOM: f32 = 1.9;

/// The camera's distance for the hand behind the field in a frame `w`
/// wide and `h` high: as large as [`BEHIND_FIELD_ZOOM`] says against a
/// frame as wide, whatever its height (the picture is framed by its
/// narrower side, `ddi_render::character::view_projection`).
pub fn behind_field_distance(distance: f32, w: f32, h: f32) -> f32 {
    if w <= 0.0 || h <= 0.0 {
        return distance;
    }
    distance * w.min(h) / (BEHIND_FIELD_ZOOM * w)
}

pub fn gameplay_look() -> PlayLook {
    let mut look = PlayLook {
        camera: BASE_CAMERA,
        guide: false,
        ..PlayLook::default()
    };
    look.appearance.sway = 1.;
    look
}

#[cfg(test)]
mod preload_tests {
    use super::*;

    #[test]
    fn a_stage_puts_the_hand_behind_the_field_and_only_a_stage() {
        let mut p = Preview::default();
        p.configure(&ddi_ui::state::settings::Settings::default());
        let size = [1280, 720];
        let own = p.frame_rect(size);
        let band = [496.0, 12.0, 288.0, 696.0];
        p.set_behind_field(Some(band));
        assert_eq!(p.frame_rect(size), band);
        // Back to the settings' frame, as before.
        p.set_behind_field(None);
        assert_eq!(p.frame_rect(size), own);
    }
    #[test]
    fn effective_input_changes_invalidate_preparation_but_appearance_does_not() {
        let song = ddi_chart::formats::sm::parse_sm(
            "#BPMS:0=120;#NOTES:dance-single::Easy:2:0,0,0,0,0:\n1000\n0100\n0010\n0001;",
        )
        .unwrap();
        let settings = ddi_ui::state::settings::Settings {
            dancing_hand: true,
            ..Default::default()
        };
        let mut config = ddi_session::play::SessionConfig::from_settings(&settings, true, None);
        config.seed = 17;
        let mut p = Preview::default();
        p.preload(&song, 0., 0, &settings, &config);
        let payload = p.runtime.as_ref().unwrap().payload.clone();
        assert!(p.preload_pending());
        p.preload(&song, 0., 0, &settings, &config);
        assert!(Rc::ptr_eq(&payload, &p.runtime.as_ref().unwrap().payload));
        let mut look = gameplay_look();
        look.camera[0] += 10.;
        p.set_look(Some(look));
        p.preload(&song, 0., 0, &settings, &config);
        assert!(Rc::ptr_eq(&payload, &p.runtime.as_ref().unwrap().payload));
        let mut edited = song.clone();
        edited.charts[0].notes[0].lane = 1;
        p.preload(&edited, 0., 0, &settings, &config);
        assert!(!Rc::ptr_eq(&payload, &p.runtime.as_ref().unwrap().payload));
        p.preload(&song, 0., 0, &settings, &config);
        let payload = p.runtime.as_ref().unwrap().payload.clone();
        config.options.transform.turn = ddi_engine::Turn::Mirror;
        p.preload(&song, 0., 0, &settings, &config);
        assert!(!Rc::ptr_eq(&payload, &p.runtime.as_ref().unwrap().payload));
        let off = ddi_ui::state::settings::Settings {
            dancing_hand: false,
            ..settings.clone()
        };
        p.preload(&song, 0., 0, &off, &config);
        assert!(!p.preload_pending());
        assert!(!p.needs_preload());
        p.preload(&song, 0., 0, &settings, &config);
        assert!(p.preload_pending());
        config.start = Some(1.);
        p.preload(&song, 0., 0, &settings, &config);
        assert!(!p.preload_pending());
        assert!(p.runtime_error.as_ref().unwrap().contains("beginning"));
        p.reset_preload();
        assert!(p.runtime.is_none());
        config.start = None;
        p.preload(&song, 0., 0, &settings, &config);
        p.preload(&song, 0., 99, &settings, &config);
        assert!(!p.preload_pending());
        p.preload(&song, 0., 0, &settings, &config);
        assert!(p.preload_pending());
        assert!(p.runtime_error.is_none());
        // Nothing prepared in calibration, nor for a layout the hand
        // does not play.
        let unsupported = ddi_chart::formats::sm::parse_sm(
            "#BPMS:0=120;#NOTES:dance-couple::Easy:2:0,0,0,0,0:\n10000000\n00000001;",
        )
        .unwrap();
        p.preload(&unsupported, 0., 0, &settings, &config);
        assert!(!p.preload_pending() && p.runtime_error.is_none());
        config.calibrate(ddi_session::calibration::CalMode::Visual);
        p.preload(&song, 0., 0, &settings, &config);
        assert!(!p.preload_pending() && p.runtime_error.is_none());
    }

    /// The settings offer the hand on the layouts its planner places on
    /// the mat, and on no other.
    #[test]
    fn the_hand_layouts_are_those_the_planner_places() {
        let builtins = ddi_chart::Layout::builtin_ids();
        for layout in ddi_character::HAND_LAYOUTS {
            assert!(builtins.iter().any(|b| b == layout), "{layout}");
        }
        for id in &builtins {
            assert_eq!(
                ddi_hand_plan::layout_panels(id).is_ok(),
                ddi_character::HAND_LAYOUTS.contains(&id.as_str()),
                "{id}"
            );
        }
    }
}
