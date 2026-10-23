//! The chart editor's field in the player's skin on the web
//! (`ddi_render::editor_field`, `docs/plans/editor.md` step 3): the
//! editor screen's picture, handed over by its host ([`show`]), drawn
//! by the editor page's frame hook ([`hook`]) into the texture the 2D
//! pass shows as `editor-field`, before the pass and only when the
//! picture changed; the skin a play of the chart would draw it with,
//! loaded off the frame as play loads it (`crate::noteskins`), the
//! built-in arrows until then; made again on a new device.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use ddi_render::editor_field::{EditorField, SOURCE};
use ddi_render::field_preview::SkinWant;
use ddi_render::noteskin::NoteskinPlay;
use ddi_render::{ColorScheme, Renderer};
use ddi_ui::screens::editor_field::FieldPicture;
use ddi_ui::state::editor::Editor;

use crate::components::ui_dev::FrameHook;
use crate::settings::Settings;
use crate::web::gfx::Gfx;

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
    /// The skin drawn (`None`: the built-in arrows) and the frames drawn.
    static STATUS: RefCell<(Option<String>, u64)> = const { RefCell::new((None, 0)) };
}

/// The editor's picture from now on (`None`: none; the texture goes).
pub(crate) fn show(picture: Option<&FieldPicture>, editor: &Editor) {
    SHOWN.with_borrow_mut(|s| {
        match picture {
            Some(p) if s.read_for.as_ref() != Some(&(p.layout.clone(), p.reverse)) => {
                let settings = Settings::load();
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

/// The field on one device.
struct Host {
    field: EditorField,
    /// The device it was made on.
    generation: u64,
    /// The texture generation the 2D pass was given.
    registered: Option<u64>,
    /// The skin shown, the one being loaded, and one loaded for the next
    /// frame to show.
    shown: Option<SkinWant>,
    loading: Option<SkinWant>,
    ready: Option<(SkinWant, Option<crate::noteskins::Prepared>)>,
    /// The number of the load wanted (an older load gives up).
    load_number: Rc<Cell<u64>>,
}

impl Drop for Host {
    fn drop(&mut self) {
        self.load_number.set(u64::MAX);
        if let Some((_, Some(p))) = self.ready.take() {
            p.close();
        }
    }
}

/// The editor page's frame hook: the field drawn when its picture
/// changed; frames go on while a skin loads.
pub(crate) fn hook() -> FrameHook {
    let slot: Rc<RefCell<Option<Host>>> = Rc::default();
    SHOWN.with_borrow_mut(|s| s.changed = true);
    Box::new(move |gfx, encoder, _ui, _time| draw(&slot, gfx, encoder))
}

fn draw(slot: &Rc<RefCell<Option<Host>>>, gfx: &Gfx, encoder: &mut wgpu::CommandEncoder) -> bool {
    let (picture, want, colors, changed) = SHOWN.with_borrow_mut(|s| {
        let changed = std::mem::take(&mut s.changed);
        (s.picture.clone(), s.want.clone(), s.colors, changed)
    });
    let mut guard = slot.borrow_mut();
    let Some(picture) = picture else {
        if let Some(h) = guard.as_mut()
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
    let mut redraw = changed;
    if guard
        .as_ref()
        .is_none_or(|h| h.generation != gfx.generation)
    {
        *guard = Some(Host {
            field: EditorField::new(
                &gfx.device,
                &gfx.queue,
                gfx.float_format(),
                &[crate::game_loop::FONT],
            ),
            generation: gfx.generation,
            registered: None,
            shown: None,
            loading: None,
            ready: None,
            load_number: Rc::default(),
        });
        redraw = true;
    }
    let Some(h) = guard.as_mut() else {
        return false;
    };
    if let Some((want, prepared)) = h.ready.take() {
        show_skin(h, gfx, want, prepared);
        redraw = true;
    }
    if let Some((want, env)) = want {
        redraw |= load_skin(h, gfx, want, env, &Rc::downgrade(slot));
    }
    redraw |= !h.field.has_targets();
    if redraw {
        let layout = ddi_shell::editor_field::layout(&picture);
        let d = ddi_shell::editor_field::draw(&picture, &layout, colors);
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
    h.loading.is_some() || h.ready.is_some()
}

/// Starts loading `want` unless it is shown or loading; whether it was
/// shown at once (the built-in arrows).
fn load_skin(
    h: &mut Host,
    gfx: &Gfx,
    want: SkinWant,
    env: ddi_noteskin::lua::Env,
    slot: &Weak<RefCell<Option<Host>>>,
) -> bool {
    if h.loading.as_ref() == Some(&want) || h.shown.as_ref() == Some(&want) {
        return false;
    }
    let number = h.load_number.get() + 1;
    h.load_number.set(number);
    h.loading = None;
    let layout = want.layout();
    let Some(key) = want.key.clone() else {
        show_skin(h, gfx, want, None);
        return true;
    };
    h.loading = Some(want.clone());
    let slot = slot.clone();
    let current = h.load_number.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let wanted: crate::noteskins::Wanted = Rc::new(move || current.get() == number);
        let prepared = crate::noteskins::prepare_while(&key, &layout, env, wanted).await;
        let Some(rc) = slot.upgrade() else {
            if let Some(p) = prepared {
                p.close();
            }
            return;
        };
        let mut guard = rc.borrow_mut();
        match guard.as_mut() {
            Some(h) if h.loading.as_ref() == Some(&want) => {
                h.loading = None;
                if let Some((_, Some(old))) = h.ready.replace((want, prepared)) {
                    old.close();
                }
            }
            _ => {
                if let Some(p) = prepared {
                    p.close();
                }
            }
        }
    });
    false
}

/// Shows `prepared` (`None`: the built-in arrows) for `want`.
fn show_skin(
    h: &mut Host,
    gfx: &Gfx,
    want: SkinWant,
    prepared: Option<crate::noteskins::Prepared>,
) {
    let (play, images): (Option<NoteskinPlay>, _) = match prepared {
        Some(p) => (Some(p.play), p.images),
        None => (None, Vec::new()),
    };
    let mut uploaded = Vec::new();
    h.field.set_noteskin(
        &gfx.device,
        &gfx.queue,
        play,
        |renderer: &mut Renderer, play: &mut NoteskinPlay| {
            uploaded =
                crate::game_loop::upload_noteskin(renderer, &gfx.device, &gfx.queue, play, &images);
        },
    );
    crate::web::screen::close_after_submit(uploaded);
    h.shown = Some(want);
}

/// The skin drawn (`None`: the built-in arrows) and the frames drawn,
/// for the debug state.
pub(crate) fn status() -> (Option<String>, u64) {
    STATUS.with_borrow(Clone::clone)
}
