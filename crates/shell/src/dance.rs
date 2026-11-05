//! Chart-derived dancing-hand accompaniment, shared by both shells.
use super::hand::{Picture, Report};
pub use ddi_character::appearance::{BASE_CAMERA, PlayLook};
use ddi_chart::Song;
use ddi_render::{DrawList, Painter, texture_cache::TextureKey};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

struct Surface {
    device: wgpu::Device,
    painter: Painter,
    list: DrawList,
}
#[derive(Default)]
pub struct Preview {
    enabled: bool,
    look: Option<PlayLook>,
    frame: ddi_character::appearance::HandFrame,
    rect: [f32; 4],
    paused: bool,
    opacity: f32,
    runtime: Option<super::dance_runtime::RuntimePreview>,
    window: Option<f64>,
    runtime_error: Option<String>,
    preload_key: Option<Vec<u8>>,
    preload_ready: bool,
    shift: f64,
    origin: f64,
    duration: f64,
    failed_at: Option<f64>,
    ever_available: bool,
    host: super::hand::Host,
    surface: Option<Surface>,
    report: Rc<RefCell<Report>>,
    song_time: f64,
    visible: bool,
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
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }
    pub fn behind_field(&self) -> bool {
        self.look.is_some()
    }
    pub fn look(&self) -> Option<PlayLook> {
        self.look
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
        self.shift = 0.;
        self.origin = 0.;
        self.duration = 0.;
        self.window = Some(3.);
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
    pub fn preload(
        &mut self,
        song: &Song,
        chart: usize,
        settings: &ddi_ui::state::settings::Settings,
        config: &ddi_session::play::SessionConfig,
        enabled: bool,
    ) {
        self.configure(settings);
        if !enabled {
            self.reset_preload();
            self.preload_ready = true;
            self.preload_key = Some(Vec::new());
            return;
        }
        let (rules, options) = match config.player_spec(song, chart, settings) {
            Ok(spec) => spec,
            Err(e) => {
                self.reset_preload();
                self.runtime_error = Some(e);
                self.preload_ready = true;
                return;
            }
        };
        let c = &song.charts[chart];
        let key = serde_json::to_vec(&(
            chart,
            &c.layout,
            c.timing(song),
            &rules.id,
            &rules.judge,
            options,
            enabled,
            config.start,
        ))
        .expect("finite play configuration");
        if self.preload_key.as_ref() == Some(&key) {
            return;
        }
        let player = ddi_engine::player::Player::new(song, chart, rules, options);
        self.bind_player(&player, &c.layout, enabled, config.start);
        self.preload_key = Some(key);
        self.preload_ready = !self.enabled;
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
        self.update_picture(
            device,
            queue,
            &mut encoder,
            format,
            size,
            self.origin + self.shift,
        );
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

    pub fn set_window(&mut self, window: f64) {
        if window.is_finite() && (2. ..=3.).contains(&window) {
            self.window = Some(window);
            if let Some(r) = self.runtime.as_mut() {
                r.set_window(window);
            }
        }
    }
    /// Numerical preparation runs before drawing, under a host clock shared
    /// by all its stages. Sampling/rendering below never calls the pose solver.
    pub fn prepare(&mut self, song_time: f64, clock: &dyn Fn() -> f64) {
        if self.failed_at.is_some() || self.paused {
            return;
        }
        if let Some(r) = self.runtime.as_mut() {
            let time = song_time - self.origin - self.shift;
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
        if let Some((rect, opacity)) =
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
        let image =
            ready.and_then(|(rect, opacity)| self.host.texture().map(|t| (t, rect, opacity)));
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
        if self.visible {
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
    ) -> Option<([f32; 4], f32)> {
        self.song_time = song_time;
        #[allow(unused_mut)]
        let mut t = song_time - self.origin - self.shift;

        if self.preload_key.is_some() {
            t = t.max(0.);
        }
        self.visible = self.enabled && (0.0..=self.duration).contains(&t);
        #[allow(unused_mut)]
        let mut opacity = 1.;

        {
            if let Some(failed) = self.failed_at {
                opacity = (1. - (t - failed) / 0.2).clamp(0., 1.) as f32;
                self.visible &= self.ever_available && opacity > 0.;
            } else {
                self.visible &= self.runtime.as_ref().is_some_and(|r| r.available);
            }
        }
        if !self.visible {
            return None;
        }
        if self
            .surface
            .as_ref()
            .is_none_or(|s| s.painter.format() != format || s.device != *device)
        {
            self.surface = Some(Surface {
                device: device.clone(),
                painter: Painter::new(device, queue, format),
                list: DrawList::new(),
            });
            self.host.device_changed();
        }
        let (w, h) = (size[0] as f32, size[1] as f32);
        let pw = (w * 0.34).clamp(150., 480.).min(w - 16.);
        let ph = (h * 0.65).min(pw * 1.3);
        let rect = if self.look.is_some() {
            self.frame.rect(size)
        } else {
            [w - pw - 12., h - ph - 60., pw, ph]
        };
        self.rect = rect;
        let camera = self.look.map_or([-25., 14., 0.52], |l| l.camera);
        self.host.set(Some(Picture {
            ground_y: self.look.map(|_| 0.98),
            appearance: self.look.map(|l| l.appearance),

            procedural: self.runtime.as_ref().map(|r| r.payload.clone()),
            rect,
            playing: !self.paused,
            seconds: t as f32,
            yaw: camera[0],
            pitch: camera[1],
            distance: camera[2],
            report: self.report.clone(),
        }));
        let s = self.surface.as_mut().unwrap();
        s.painter.submitted();
        self.host.draw(device, queue, encoder, &mut s.painter);
        self.opacity = opacity;
        Some((rect, opacity))
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
        let s = self.surface.as_mut().unwrap();
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
        #[allow(unused_mut)]
        let mut result = json!({"paused":self.paused,"behind_field":self.behind_field(),"look":self.look,"up_marker":self.up_marker(),"enabled":self.enabled,"visible":self.visible,"song_time":self.song_time,
            "origin":self.origin+self.shift,"duration":self.duration,"character":self.host.debug()});

        {
            result["runtime"] = self.runtime.as_ref().map_or(Value::Null, |r| r.debug());
            result["runtime_error"] = json!(self.runtime_error);
            result["preloaded"] = json!(self.preload_ready);
            result["preload_pending"] = json!(self.preload_pending());
            result["status"] = json!(if self.failed_at.is_some() {
                "stopped"
            } else if self.runtime_error.is_some() {
                "unsupported"
            } else if !self.enabled {
                "off"
            } else if self.runtime.as_ref().is_some_and(|r| r.available) {
                "active"
            } else {
                "warming"
            });
        }
        result
    }
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
    fn effective_input_changes_invalidate_preparation_but_appearance_does_not() {
        let song = ddi_chart::formats::sm::parse_sm(
            "#BPMS:0=120;#NOTES:dance-single::Easy:2:0,0,0,0,0:\n1000\n0100\n0010\n0001;",
        )
        .unwrap();
        let mut settings = ddi_ui::state::settings::Settings::default();
        settings.dancing_hand = true;
        let mut config = ddi_session::play::SessionConfig::from_settings(&settings, true, None);
        config.seed = 17;
        let mut p = Preview::default();
        p.preload(&song, 0, &settings, &config, true);
        let payload = p.runtime.as_ref().unwrap().payload.clone();
        assert!(p.preload_pending());
        p.preload(&song, 0, &settings, &config, true);
        assert!(Rc::ptr_eq(&payload, &p.runtime.as_ref().unwrap().payload));
        let mut look = gameplay_look();
        look.camera[0] += 10.;
        p.set_look(Some(look));
        p.preload(&song, 0, &settings, &config, true);
        assert!(Rc::ptr_eq(&payload, &p.runtime.as_ref().unwrap().payload));
        config.options.transform.turn = ddi_engine::Turn::Mirror;
        p.preload(&song, 0, &settings, &config, true);
        assert!(!Rc::ptr_eq(&payload, &p.runtime.as_ref().unwrap().payload));
        p.preload(&song, 0, &settings, &config, false);
        assert!(!p.preload_pending());
        p.preload(&song, 0, &settings, &config, true);
        assert!(p.preload_pending());
        config.start = Some(1.);
        p.preload(&song, 0, &settings, &config, true);
        assert!(!p.preload_pending());
        assert!(p.runtime_error.as_ref().unwrap().contains("beginning"));
        p.reset_preload();
        assert!(p.runtime.is_none());
        config.start = None;
        p.preload(&song, 0, &settings, &config, true);
        p.preload(&song, 99, &settings, &config, true);
        assert!(!p.preload_pending());
        p.preload(&song, 0, &settings, &config, true);
        assert!(p.preload_pending());
        assert!(p.runtime_error.is_none());
    }
}
