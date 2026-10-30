//! Options' field preview on desktop (`ddi_render::field_preview`): the
//! demo chart drawn as play draws it with the settings, into a texture
//! the painter shows as the image `field-preview` where the options
//! screen asks for it ([`ddi_ui::UiRequest::FieldPreview`]), with the
//! noteskin play would draw its layout with (`Settings::noteskin_for`:
//! the pump skin for pump layouts), readied natively as play readies it
//! (`crate::play::skins`) and shown from the next frame after a pick, as
//! the web's preview does. Nothing is drawn until the first skin is in,
//! and a frame holds while the skin for another layout is read: never
//! the built-in arrows, nor a skin on a layout it was not readied for.
//!
//! The same preview draws the start prompt's receptor diagram
//! ([`Field::set_receptors`]), and readies its skin ahead while song
//! select shows a chart ([`Field::warm`], from the select screen's place
//! report), so the prompt shows its receptors at once.

use std::time::Instant;

use ddi_chart::Layout;
use ddi_render::Painter;
pub use ddi_render::field_preview::SkinWant;
use ddi_render::field_preview::{FieldPreview, FieldPreviewOptions, SOURCE};
use ddi_shell::field_preview::{SkinLoad, SkinSlot};
use ddi_ui::screens::prompt_controls::Controls;
use ddi_ui::state::settings::Settings;

use crate::play::skins::{Prepared, Reading};

/// A skin read on its thread, made for its layout on the UI thread once
/// in (the Lua's second pass).
pub struct Load {
    reading: Reading,
    layout: Layout,
}

impl SkinLoad for Load {
    type Skin = Prepared;
    fn poll(&mut self) -> Option<Option<Prepared>> {
        let read = self.reading.poll()?;
        Some(read.and_then(|r| crate::play::skins::build(r, &self.layout)))
    }
}

/// The preview, while a screen shows it.
pub struct Field {
    preview: Option<FieldPreview>,
    /// The skin wanted (a demo's layout, or the diagram's strip), shown
    /// and read.
    skin: SkinSlot<Load>,
    /// Where it shows, physical pixels `[x, y, w, h]`.
    rect: Option<[f32; 4]>,
    options: FieldPreviewOptions,
    receptors: Option<ddi_render::field_preview::ReceptorPreview>,
    receptor_ready: Option<std::rc::Rc<std::cell::Cell<bool>>>,
    /// The texture generation the painter was given.
    registered: Option<u64>,
    start: Instant,
}

impl Default for Field {
    fn default() -> Field {
        Field {
            preview: None,
            skin: SkinSlot::default(),
            rect: None,
            options: FieldPreviewOptions::default(),
            receptors: None,
            receptor_ready: None,
            registered: None,
            start: Instant::now(),
        }
    }
}

impl Field {
    /// The same preview for a new device: where it shows and what it
    /// shows kept, everything on the old device let go.
    pub fn for_new_device(&self) -> Field {
        Field {
            rect: self.rect,
            options: self.options.clone(),
            receptors: self.receptors.clone(),
            receptor_ready: self.receptor_ready.clone(),
            skin: self.skin.for_new_device(),
            ..Field::default()
        }
    }

    /// Where the screen shows it (`None`: nowhere; no frames are drawn).
    pub fn set_rect(&mut self, rect: Option<[f32; 4]>) {
        self.rect = rect;
    }

    pub fn shown(&self) -> bool {
        self.rect.is_some()
    }

    /// A pick's skin is being read.
    pub fn reading(&self) -> bool {
        self.skin.loading()
    }

    pub fn set_options(&mut self, options: FieldPreviewOptions) {
        self.options = options;
    }

    /// The options and the noteskin under `settings` (the skin readied
    /// on the next frame when it changed), for Options' demo.
    pub fn set_settings(&mut self, settings: &Settings) {
        self.receptors = None;
        self.receptor_ready = None;
        self.options = self.skin.set_settings(settings);
    }

    /// The prompt's receptor diagram (`None`: gone), its skin readied
    /// when it is not the one in.
    pub fn set_receptors(&mut self, picture: Option<&ddi_ui::screens::prompt_controls::Picture>) {
        self.receptor_ready = picture.map(|p| p.ready.clone());
        self.rect = picture.map(|p| [0.0, 0.0, p.size[0], p.size[1]]);
        self.receptors = picture.map(|p| {
            let (options, pose) = self.skin.set_receptors(p);
            self.options = options;
            pose
        });
    }

    /// Readies the diagram's skin for `controls` (a chart song select
    /// shows) without showing anything, so the prompt's diagram is in
    /// from its first frame.
    pub fn warm(&mut self, controls: &Controls) {
        self.options = self.skin.warm(controls);
    }

    /// Whether frames must go on: the demo plays, or the skin wanted is
    /// not in yet (its read started and polled by the frames).
    pub fn animating(&self) -> bool {
        !self.skin.ready() || (self.rect.is_some() && self.receptors.is_none())
    }

    /// The noteskin drawn (`None`: the built-in arrows), its name.
    pub fn skin_name(&self) -> Option<String> {
        self.preview
            .as_ref()
            .and_then(FieldPreview::noteskin)
            .map(|n| n.name.clone())
    }

    /// The values the drawn noteskin's Lua reads for its parameters.
    pub fn skin_parameters(&self) -> ddi_noteskin::params::Values {
        self.preview
            .as_ref()
            .and_then(FieldPreview::noteskin)
            .map(|n| n.parameters())
            .unwrap_or_default()
    }

    /// Whether the skin in is the one wanted, readied for the layout
    /// wanted.
    pub fn ready(&self) -> bool {
        self.skin.ready()
    }

    /// The layout the skin in was readied for (tests).
    pub fn shown_layout(&self) -> Option<&Layout> {
        self.skin.shown_layout()
    }

    /// Whether the preview holds its textures (tests).
    pub fn holds_targets(&self) -> bool {
        self.preview.as_ref().is_some_and(FieldPreview::has_targets)
    }

    /// Frames drawn so far.
    pub fn frames(&self) -> u64 {
        self.preview.as_ref().map_or(0, FieldPreview::frames)
    }

    /// Readies the skin wanted: read on a thread (the older pick's read
    /// let go), made and put on the preview once it comes; the one shown
    /// until then.
    fn ready_skin(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        // Half floats render and blend on every native backend (WebGPU's
        // core features).
        let preview = self.preview.get_or_insert_with(|| {
            FieldPreview::new(
                device,
                queue,
                Some(wgpu::TextureFormat::Rgba16Float),
                &[ddi_render::HUD_FONT],
            )
        });
        preview.set_options(self.options.clone());
        preview.set_receptors(self.receptors.clone());
        let skin = self.skin.step(|key, layout, env| Load {
            reading: Reading::start(key, layout, env, None),
            layout: layout.clone(),
        });
        if let Some(prepared) = skin {
            let (play, images) = match prepared {
                Some(p) => (Some(p.play), p.images),
                None => (None, Vec::new()),
            };
            preview.set_noteskin(device, queue, play, |renderer, play| {
                crate::play::view::upload_noteskin(renderer, device, queue, play, &images);
            });
        }
    }

    /// Readies the skin wanted ([`Field::ready_skin`]) and, while shown,
    /// records its frame into `encoder` (before the painter draws) and
    /// gives the painter its texture when it is a new one; once no screen
    /// shows it, takes the texture from the painter and lets the
    /// preview's textures go (its renderer and skin stay, for the next
    /// time).
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        painter: &mut Painter,
    ) {
        self.ready_skin(device, queue);
        let Some([_, _, w, h]) = self.rect else {
            if self.registered.take().is_some() {
                painter.set_external_image(device, SOURCE, None);
            }
            if let Some(p) = &mut self.preview {
                p.release_targets();
            }
            return;
        };
        let ready = self.ready();
        let Some(preview) = &mut self.preview else {
            return;
        };
        // Nothing until the skin wanted is in, readied for the layout
        // wanted: the panel's placeholder, or the last frame, never the
        // built-in arrows on the way.
        if ready {
            let size = [w.round().max(1.0) as u32, h.round().max(1.0) as u32];
            preview.draw(
                device,
                queue,
                encoder,
                size,
                self.start.elapsed().as_secs_f64(),
            );
        }
        if let Some(r) = &self.receptor_ready {
            r.set(ready && preview.texture().is_some());
        }
        if self.registered != Some(preview.generation()) {
            painter.set_external_image(device, SOURCE, preview.texture());
            self.registered = Some(preview.generation());
        }
    }
}
