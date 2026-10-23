//! The chart editor's field in the player's skin on desktop
//! (`ddi_render::editor_field`, `docs/plans/editor.md` step 3): the
//! editor screen's picture, handed over by its host
//! ([`show`]), drawn into the texture the painter shows as
//! `editor-field` before the UI's frame, only when the picture changed;
//! the skin a play of the chart would draw it with, read on a thread as
//! play reads it (`crate::play::skins`), the built-in arrows until then.

use std::cell::RefCell;

use ddi_chart::Layout;
use ddi_render::editor_field::{EditorField, SOURCE};
use ddi_render::field_preview::SkinWant;
use ddi_render::{ColorScheme, Painter};
use ddi_ui::screens::editor_field::FieldPicture;
use ddi_ui::state::editor::Editor;

/// What the editor shows: its picture, the skin it wants and what that
/// skin's Lua is told, the note colors (read from the settings for each
/// layout and direction), and whether it changed since it was drawn.
#[derive(Default)]
struct Shown {
    picture: Option<FieldPicture>,
    want: Option<(SkinWant, ddi_noteskin::lua::Env)>,
    colors: ColorScheme,
    read_for: Option<(String, bool)>,
    changed: bool,
}

thread_local! {
    static SHOWN: RefCell<Shown> = RefCell::default();
}

/// The editor's picture from now on (`None`: none; the texture goes).
pub fn show(picture: Option<&FieldPicture>, editor: &Editor) {
    SHOWN.with_borrow_mut(|s| {
        match picture {
            Some(p) if s.read_for.as_ref() != Some(&(p.layout.clone(), p.reverse)) => {
                let settings = crate::store::settings_cached();
                let want = ddi_shell::editor_field::skin(&settings, Some(editor.chart()), p);
                if s.want.as_ref().is_none_or(|(w, _)| *w != want) {
                    let env =
                        ddi_shell::editor_field::skin_env(&want, editor.song(), editor.chart());
                    s.want = Some((want, env));
                }
                s.colors = ddi_shell::editor_field::note_colors(&settings);
                s.read_for = Some((p.layout.clone(), p.reverse));
            }
            Some(_) => {}
            // The settings read again for the next editor.
            None => s.read_for = None,
        }
        s.picture = picture.cloned();
        s.changed = true;
    });
}

/// The field on the device, while the editor shows it.
#[derive(Default)]
pub struct EditorFieldHost {
    field: Option<EditorField>,
    /// The skin shown, and the one being read.
    shown: Option<SkinWant>,
    reading: Option<(SkinWant, crate::play::skins::Reading, Layout)>,
    /// The texture generation the painter was given.
    registered: Option<u64>,
}

impl EditorFieldHost {
    /// For a new device: drawn again from the picture shown.
    pub fn for_new_device() -> EditorFieldHost {
        SHOWN.with_borrow_mut(|s| s.changed = true);
        EditorFieldHost::default()
    }

    /// A skin is being read: frames go on until it comes.
    pub fn busy(&self) -> bool {
        self.reading.is_some()
    }

    /// The noteskin drawn (`None`: the built-in arrows), its name.
    pub fn skin_name(&self) -> Option<String> {
        self.field
            .as_ref()
            .and_then(EditorField::noteskin)
            .map(|n| n.name.clone())
    }

    /// Frames drawn so far.
    pub fn frames(&self) -> u64 {
        self.field.as_ref().map_or(0, EditorField::frames)
    }

    /// The texture drawn into (tests read it back).
    pub fn texture(&self) -> Option<&wgpu::Texture> {
        self.field.as_ref().and_then(EditorField::texture)
    }

    /// While the editor shows a picture, records its frame into
    /// `encoder` when it changed (or the skin came) and gives the painter
    /// the texture when it is a new one; once it shows none, takes the
    /// texture from the painter and lets it go.
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        painter: &mut Painter,
    ) {
        let (picture, want, colors, changed) = SHOWN.with_borrow_mut(|s| {
            let changed = std::mem::take(&mut s.changed);
            (s.picture.clone(), s.want.clone(), s.colors, changed)
        });
        let Some(picture) = picture else {
            if self.registered.take().is_some() {
                painter.set_external_image(device, SOURCE, None);
            }
            if let Some(f) = &mut self.field {
                f.release_targets();
            }
            self.reading = None;
            return;
        };
        let field = self.field.get_or_insert_with(|| {
            EditorField::new(
                device,
                queue,
                Some(wgpu::TextureFormat::Rgba16Float),
                &[ddi_render::HUD_FONT],
            )
        });
        let mut redraw = changed || !field.has_targets();
        // The skin wanted: read on a thread (an older read let go).
        if let Some((want, env)) = want
            && self.shown.as_ref() != Some(&want)
            && self.reading.as_ref().is_none_or(|(w, ..)| *w != want)
        {
            let layout = want.layout();
            match &want.key {
                Some(key) => {
                    let reading = crate::play::skins::Reading::start(key, &layout, env, None);
                    self.reading = Some((want, reading, layout));
                }
                None => {
                    field.set_noteskin(device, queue, None, |_, _| {});
                    self.reading = None;
                    self.shown = Some(want);
                    redraw = true;
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
            field.set_noteskin(device, queue, play, |renderer, play| {
                crate::play::view::upload_noteskin(renderer, device, queue, play, &images);
            });
            self.shown = Some(want.clone());
            self.reading = None;
            redraw = true;
        }
        if redraw {
            let layout = ddi_shell::editor_field::layout(&picture);
            let d = ddi_shell::editor_field::draw(&picture, &layout, colors);
            let size = ddi_shell::editor_field::size(&picture);
            field.draw(device, queue, encoder, size, &d);
        }
        if self.registered != Some(field.generation()) && field.has_targets() {
            painter.set_external_image(device, SOURCE, field.texture());
            self.registered = Some(field.generation());
        }
    }
}
