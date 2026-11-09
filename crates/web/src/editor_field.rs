//! The chart editor's field in the player's skin on the web
//! (`ddi_render::editor_field`, `docs/plans/editor.md` step 3): the
//! editor screen's picture, handed over by its host ([`show`]), drawn
//! by the editor page's frame hook ([`hook`]) into the texture the 2D
//! pass shows as `editor-field`, before the pass and only when the
//! picture changed; the skin a play of the chart would draw it with,
//! loaded off the frame as play loads it (`crate::noteskins`), the
//! built-in arrows until then; made again on a new device.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_render::Renderer;
use ddi_render::editor_field::{EditorField, SOURCE};
use ddi_render::noteskin::NoteskinPlay;
use ddi_shell::editor_field::Shown;
use ddi_shell::field_preview::SkinSlot;
use ddi_ui::screens::editor_field::FieldPicture;
use ddi_ui::state::editor::Editor;

use crate::components::ui_dev::FrameHook;
use crate::field_preview::Load;
use crate::settings::Settings;
use crate::web::gfx::Gfx;

thread_local! {
    static SHOWN: RefCell<Shown> = RefCell::default();
    /// The skin drawn (`None`: the built-in arrows) and the frames drawn.
    static STATUS: RefCell<(Option<String>, u64)> = const { RefCell::new((None, 0)) };
}

/// The editor's picture from now on (`None`: none; the texture goes).
pub(crate) fn show(picture: Option<Rc<FieldPicture>>, editor: &Editor) {
    SHOWN.with_borrow_mut(|s| s.show(picture, editor, Settings::load));
}

/// The field on one device.
struct Host {
    field: EditorField,
    /// The device it was made on.
    generation: u64,
    /// The texture generation the 2D pass was given.
    registered: Option<u64>,
    /// The skin wanted, shown and loading.
    skin: SkinSlot<Load>,
}

/// The editor page's frame hook: the field drawn when its picture
/// changed; frames go on while a skin loads.
pub(crate) fn hook() -> FrameHook {
    let mut host = None;
    SHOWN.with_borrow_mut(|s| s.changed = true);
    Box::new(move |gfx, encoder, _ui, _time| draw(&mut host, gfx, encoder))
}

fn draw(host: &mut Option<Host>, gfx: &Gfx, encoder: &mut wgpu::CommandEncoder) -> bool {
    let shown = SHOWN.with_borrow_mut(Shown::take);
    let Some(picture) = shown.picture else {
        if let Some(h) = host.as_mut()
            && h.generation == gfx.generation
        {
            if h.registered.take().is_some() {
                crate::web::screen::with_painter(gfx, |p| {
                    p.set_external_image(&gfx.device, SOURCE, None);
                });
            }
            h.field.release_targets();
        }
        return false;
    };
    let mut redraw = shown.changed;
    if host.as_ref().is_none_or(|h| h.generation != gfx.generation) {
        *host = Some(Host {
            field: EditorField::new(
                &gfx.device,
                &gfx.queue,
                gfx.float_format(),
                &[ddi_render::HUD_FONT],
            ),
            generation: gfx.generation,
            registered: None,
            skin: SkinSlot::default(),
        });
        redraw = true;
    }
    let Some(h) = host.as_mut() else {
        return false;
    };
    if let Some(want) = &shown.want {
        h.skin.want_own_layout(&want.0);
        let skin = h
            .skin
            .step(|key, layout, _| Load::start(key, layout, want.1.clone(), || {}));
        if let Some(prepared) = skin {
            show_skin(h, gfx, prepared);
            redraw = true;
        }
    }
    redraw |= !h.field.has_targets();
    if redraw {
        let layout = ddi_shell::editor_field::layout(&picture);
        let d = ddi_shell::editor_field::draw(&picture, &layout, shown.colors);
        let size = ddi_shell::editor_field::size(&picture);
        h.field.draw(&gfx.device, &gfx.queue, encoder, size, &d);
        let name = h.field.noteskin().map(|n| n.name.clone());
        STATUS.set((name, h.field.frames()));
    }
    if h.registered != Some(h.field.generation()) && h.field.has_targets() {
        let texture = h.field.texture();
        crate::web::screen::with_painter(gfx, |p| {
            p.set_external_image(&gfx.device, SOURCE, texture);
        });
        h.registered = Some(h.field.generation());
    }
    h.skin.loading()
}

/// Shows `prepared` (`None`: the built-in arrows).
fn show_skin(h: &mut Host, gfx: &Gfx, prepared: Option<crate::noteskins::Prepared>) {
    let (play, images): (Option<NoteskinPlay>, _) = match prepared {
        Some(p) => (Some(p.play), p.images),
        None => (None, Vec::new()),
    };
    h.field.set_noteskin(
        &gfx.device,
        &gfx.queue,
        play,
        |renderer: &mut Renderer, play: &mut NoteskinPlay| {
            crate::game_loop::upload_noteskin(renderer, &gfx.device, &gfx.queue, play, &images);
        },
    );
    crate::web::screen::close_after_submit(images.into_iter().flatten());
}

/// The skin drawn (`None`: the built-in arrows) and the frames drawn,
/// for the debug state.
pub(crate) fn status() -> (Option<String>, u64) {
    STATUS.with_borrow(Clone::clone)
}
