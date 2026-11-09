//! The chart editor's field in the player's skin on desktop
//! (`ddi_render::editor_field`, `docs/plans/editor.md` step 3): the
//! editor screen's picture, handed over by its host
//! ([`show`]), drawn into the texture the painter shows as
//! `editor-field` before the UI's frame, only when the picture changed;
//! the skin a play of the chart would draw it with, read on a thread as
//! play reads it (`crate::play::skins`), the built-in arrows until then.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_render::Painter;
use ddi_render::editor_field::{EditorField, SOURCE};
use ddi_shell::editor_field::Shown;
use ddi_shell::field_preview::SkinSlot;
use ddi_ui::screens::editor_field::FieldPicture;
use ddi_ui::state::editor::Editor;
use ddi_ui::state::settings::Settings;

use crate::field_preview::Load;

thread_local! {
    static SHOWN: RefCell<Shown> = RefCell::default();
}

/// The editor's picture from now on (`None`: none; the texture goes).
pub fn show(picture: Option<Rc<FieldPicture>>, editor: &Editor) {
    SHOWN.with_borrow_mut(|s| s.show(picture, editor, Settings::cached));
}

/// The field on the device, while the editor shows it.
#[derive(Default)]
pub struct EditorFieldHost {
    field: Option<EditorField>,
    /// The skin wanted, shown and being read.
    skin: SkinSlot<Load>,
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
        self.skin.loading()
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
        let shown = SHOWN.with_borrow_mut(Shown::take);
        let Some(picture) = shown.picture else {
            if self.registered.take().is_some() {
                painter.set_external_image(device, SOURCE, None);
            }
            if let Some(f) = &mut self.field {
                f.release_targets();
            }
            self.skin.cancel();
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
        let mut redraw = shown.changed || !field.has_targets();
        // The skin wanted: read on a thread (an older read let go).
        if let Some(want) = &shown.want {
            self.skin.want_own_layout(&want.0);
            let skin = self
                .skin
                .step(|key, layout, _| Load::start(key, layout, want.1.clone()));
            if let Some(prepared) = skin {
                let (play, images) = match prepared {
                    Some(p) => (Some(p.play), p.images),
                    None => (None, Vec::new()),
                };
                field.set_noteskin(device, queue, play, |renderer, play| {
                    crate::play::view::upload_noteskin(renderer, device, queue, play, &images);
                });
                redraw = true;
            }
        }
        if redraw {
            let layout = ddi_shell::editor_field::layout(&picture);
            let d = ddi_shell::editor_field::draw(&picture, &layout, shown.colors);
            let size = ddi_shell::editor_field::size(&picture);
            field.draw(device, queue, encoder, size, &d);
        }
        if self.registered != Some(field.generation()) && field.has_targets() {
            painter.set_external_image(device, SOURCE, field.texture());
            self.registered = Some(field.generation());
        }
    }
}
