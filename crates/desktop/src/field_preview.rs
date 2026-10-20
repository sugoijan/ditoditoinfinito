//! Options' field preview on desktop (`ddi_render::field_preview`): the
//! demo chart drawn as play draws it with the settings, into a texture
//! the painter shows as the image `field-preview` where the options
//! screen asks for it ([`ddi_ui::UiRequest::FieldPreview`]), with the
//! noteskin play would draw its layout with (`Settings::noteskin_for`:
//! the pump skin for pump layouts), readied natively as play readies it
//! (`crate::play::skins`) and shown from the next frame after a pick, as
//! the web's preview does.

use std::time::Instant;

use ddi_chart::Layout;
use ddi_render::field_preview::{BPM, FieldPreview, FieldPreviewOptions, SOURCE};
use ddi_render::{ColorScheme, Painter};
use ddi_ui::state::settings::{BackgroundFrame, NoteColors, Settings};

/// The HUD's face (the web's `game_loop::FONT`).
const FONT: &[u8] = include_bytes!("../../../assets/fonts/IosevkaCustom-ExtraBold-latin.ttf");

/// The note colors as the renderer has them (the web's
/// `settings::note_scheme`).
fn note_scheme(colors: NoteColors) -> ColorScheme {
    match colors {
        NoteColors::Quantized => ColorScheme::Quantized,
        NoteColors::Vivid => ColorScheme::Vivid,
        NoteColors::Flat => ColorScheme::Flat,
        NoteColors::Rainbow => ColorScheme::Rainbow,
        NoteColors::Single => ColorScheme::Single([0.35, 0.85, 1.0, 1.0]),
    }
}

/// The preview's options under `settings`, as the web builds them
/// (`crates/web/src/field_preview.rs`, play's own).
pub fn options(settings: &Settings) -> FieldPreviewOptions {
    let layout = if Layout::builtin(&settings.style).is_some() {
        settings.style.clone()
    } else {
        "dance-single".to_string()
    };
    let rules = ddi_engine::demo::song(&layout, BPM)
        .and_then(|s| s.charts.first().map(|c| settings.ruleset_for(c).key()))
        .unwrap_or_else(|| settings.ruleset().key());
    FieldPreviewOptions {
        autoplay_flashes: false,
        layout,
        play: settings.play_options(None),
        rules,
        note_colors: note_scheme(settings.note_colors),
        field_filter: settings.field_filter,
        show_deltas: settings.show_deltas,
        background: settings.bg_brightness,
        frame_aspect: match settings.background_frame {
            BackgroundFrame::Screen => None,
            BackgroundFrame::FourThree => Some(4.0 / 3.0),
            BackgroundFrame::SixteenNine | BackgroundFrame::Auto => Some(16.0 / 9.0),
        },
    }
}

/// The noteskin the preview's layout is drawn with and what it is
/// readied for (key, layout, direction): key `None` for the built-in
/// arrows. The demo has no colors of its own.
#[derive(Clone, Debug, PartialEq)]
pub struct SkinWant {
    pub key: Option<String>,
    pub layout: String,
    pub reverse: bool,
}

impl SkinWant {
    pub fn of(settings: &Settings, options: &FieldPreviewOptions) -> SkinWant {
        SkinWant {
            key: crate::play::skins::play_skin(
                settings.noteskin_for(&options.layout),
                settings.note_colors,
                false,
            ),
            layout: options.layout.clone(),
            reverse: options.play.scroll.reverse,
        }
    }
}

/// The preview, while a screen shows it.
pub struct Field {
    preview: Option<FieldPreview>,
    /// The skin wanted, and the one shown.
    wanted: Option<SkinWant>,
    shown: Option<SkinWant>,
    /// Where it shows, physical pixels `[x, y, w, h]`.
    rect: Option<[f32; 4]>,
    options: FieldPreviewOptions,
    /// The texture generation the painter was given.
    registered: Option<u64>,
    start: Instant,
    /// The skin wanted, read on its thread (a newer pick lets an older
    /// read go).
    reading: Option<(SkinWant, crate::play::skins::Reading, Layout)>,
}

impl Default for Field {
    fn default() -> Field {
        Field {
            preview: None,
            wanted: None,
            shown: None,
            rect: None,
            options: FieldPreviewOptions::default(),
            registered: None,
            start: Instant::now(),
            reading: None,
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
            wanted: self.wanted.clone().or_else(|| self.shown.clone()),
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
        self.reading.is_some()
    }

    pub fn set_options(&mut self, options: FieldPreviewOptions) {
        self.options = options;
    }

    /// The options and the noteskin under `settings` (the skin readied
    /// on the next frame when it changed).
    pub fn set_settings(&mut self, settings: &Settings) {
        let options = options(settings);
        self.wanted = Some(SkinWant::of(settings, &options));
        self.options = options;
    }

    /// The noteskin drawn (`None`: the built-in arrows), its name.
    pub fn skin_name(&self) -> Option<String> {
        self.preview
            .as_ref()
            .and_then(FieldPreview::noteskin)
            .map(|n| n.name.clone())
    }

    /// Whether the preview holds its textures (tests).
    pub fn holds_targets(&self) -> bool {
        self.preview.as_ref().is_some_and(FieldPreview::has_targets)
    }

    /// Frames drawn so far.
    pub fn frames(&self) -> u64 {
        self.preview.as_ref().map_or(0, FieldPreview::frames)
    }

    /// While shown, records its frame into `encoder` (before the painter
    /// draws) and gives the painter its texture when it is a new one;
    /// once no screen shows it, takes the texture from the painter and
    /// lets the preview's textures go (its renderer and skin stay, for
    /// the next time).
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        painter: &mut Painter,
    ) {
        let Some([_, _, w, h]) = self.rect else {
            if self.registered.take().is_some() {
                painter.set_external_image(device, SOURCE, None);
            }
            if let Some(p) = &mut self.preview {
                p.release_targets();
            }
            return;
        };
        // Half floats render and blend on every native backend (WebGPU's
        // core features).
        let preview = self.preview.get_or_insert_with(|| {
            FieldPreview::new(
                device,
                queue,
                Some(wgpu::TextureFormat::Rgba16Float),
                &[FONT],
            )
        });
        preview.set_options(self.options.clone());
        // A skin picked: read on a thread (the older pick's read let
        // go), made and drawn once it comes; the one shown until then.
        if let Some(want) = self.wanted.clone()
            && self.shown.as_ref() != Some(&want)
            && self.reading.as_ref().is_none_or(|(w, ..)| *w != want)
        {
            let layout = Layout::builtin(&want.layout).unwrap_or_else(Layout::dance_single);
            match &want.key {
                Some(key) => {
                    let env = ddi_engine::demo::song(&want.layout, BPM)
                        .and_then(|song| {
                            let chart = song.charts.first()?;
                            Some(crate::play::skins::lua_env(
                                &song,
                                chart,
                                &layout,
                                want.reverse,
                            ))
                        })
                        .unwrap_or_default();
                    let reading = crate::play::skins::Reading::start(key, &layout, env, None);
                    self.reading = Some((want, reading, layout));
                }
                None => {
                    preview.set_noteskin(device, queue, None, |_, _| {});
                    self.reading = None;
                    self.shown = Some(want);
                }
            }
        }
        if let Some((want, reading, layout)) = &self.reading
            && let Some(read) = reading.poll()
        {
            let prepared = read.and_then(|r| crate::play::skins::build(r, layout));
            let (play, images) = match prepared {
                Some(p) => (Some(p.play), p.images),
                None => (None, Vec::new()),
            };
            preview.set_noteskin(device, queue, play, |renderer, play| {
                crate::play::view::upload_noteskin(renderer, device, queue, play, &images);
            });
            self.shown = Some(want.clone());
            self.reading = None;
        }
        let size = [w.round().max(1.0) as u32, h.round().max(1.0) as u32];
        preview.draw(
            device,
            queue,
            encoder,
            size,
            self.start.elapsed().as_secs_f64(),
        );
        if self.registered != Some(preview.generation()) {
            painter.set_external_image(device, SOURCE, preview.texture());
            self.registered = Some(preview.generation());
        }
    }
}
