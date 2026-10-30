//! Options' field preview on the web (`ddi_render::field_preview`): the
//! demo chart drawn as play draws it with the settings being chosen, into
//! a texture the 2D pass shows as the image [`SOURCE`]. This side has
//! the preview's options built from the settings (`ddi_shell`, as a play
//! has its own), loads the noteskin play would use (in slices, off the
//! frame, as play loads it) and puts it on the preview once ready, and
//! makes the preview again on a new device. Nothing is drawn until the
//! first skin is in, and a frame holds while the skin for another
//! layout loads: never the built-in arrows, nor a skin on a layout it
//! was not readied for.
//!
//! One preview serves the whole app ([`slot`]), outliving the pages:
//! its skin stays in between Options, song select and the start prompt,
//! whose receptor diagram it draws too, and song select readies the
//! diagram's skin ahead ([`FieldPreviewHost::warm`]) so the prompt shows
//! its receptors at once.
//!
//! A screen showing it (Options) says where each frame, in canvas
//! pixels; [`FieldPreviewHost::draw`] records the preview's frame into
//! the frame's encoder before the 2D pass, so the image shows the same
//! frame.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use ddi_render::Renderer;
use ddi_render::field_preview::FieldPreview;
use ddi_render::noteskin::NoteskinPlay;
use ddi_shell::field_preview::{SkinLoad, SkinSlot};
use ddi_ui::screens::prompt_controls::Controls;

pub(crate) use ddi_render::field_preview::SOURCE;

use crate::noteskins::Prepared;
use crate::settings::Settings;
use crate::web::gfx::Gfx;

thread_local! {
    /// The app's one preview (made on first use, on the device of the
    /// time; made again on a new one).
    static SLOT: Rc<RefCell<Option<FieldPreviewHost>>> = Rc::default();
}

/// The app's preview's slot, shared by every page's host.
pub(crate) fn slot() -> Rc<RefCell<Option<FieldPreviewHost>>> {
    SLOT.with(Rc::clone)
}

/// A skin loading in a task, which fills its cell. Dropped, the task
/// gives up (`noteskins::prepare_while`'s `wanted`), so stepping through
/// skins loads only the last, and a skin loaded but never taken is
/// closed.
pub(crate) struct Load {
    cell: Rc<RefCell<Option<Option<Prepared>>>>,
    cancelled: Rc<Cell<bool>>,
}

impl Load {
    /// The skin is in, for the next step to show.
    fn done(&self) -> bool {
        self.cell.borrow().is_some()
    }
}

impl SkinLoad for Load {
    type Skin = Prepared;
    fn poll(&mut self) -> Option<Option<Prepared>> {
        self.cell.borrow_mut().take()
    }
}

impl Drop for Load {
    fn drop(&mut self) {
        self.cancelled.set(true);
        if let Some(Some(p)) = self.cell.borrow_mut().take() {
            p.close();
        }
    }
}

/// The preview on one device.
pub(crate) struct FieldPreviewHost {
    preview: FieldPreview,
    /// The device it was made on.
    generation: u64,
    /// The preview's texture generation the 2D pass was given.
    registered: Option<u64>,
    /// The skin wanted and the layout it is readied for, the one shown
    /// (none before the first) and the one loading.
    skin: SkinSlot<Load>,
    /// The device, for a skin loaded to be shown as soon as it is (its
    /// textures uploaded then, not on the preview's next draw, which is
    /// the prompt's first frame when song select readied it).
    gfx: Weak<Gfx>,
    /// What the shell last read the settings for (ms).
    read_ms: f64,
    pub(crate) status: String,
}

/// How often the settings are read again while the preview shows (ms):
/// a screen editing them may also hand them over at once
/// ([`FieldPreviewHost::set_settings`]).
const READ_EVERY_MS: f64 = 400.0;

impl FieldPreviewHost {
    /// A preview on `gfx`'s device.
    pub(crate) fn new(gfx: &Rc<Gfx>) -> FieldPreviewHost {
        let preview = FieldPreview::new(
            &gfx.device,
            &gfx.queue,
            gfx.float_format(),
            &[crate::game_loop::FONT],
        );
        FieldPreviewHost {
            preview,
            generation: gfx.generation,
            registered: None,
            skin: SkinSlot::default(),
            gfx: Rc::downgrade(gfx),
            read_ms: f64::NEG_INFINITY,
            status: "built-in arrows".into(),
        }
    }

    /// Whether it was made on `gfx`'s device.
    pub(crate) fn on(&self, gfx: &Gfx) -> bool {
        self.generation == gfx.generation
    }

    /// Holds the demo at `seconds` (a still preview, under reduced
    /// motion).
    pub(crate) fn seek(&mut self, seconds: f64) {
        self.preview.seek(seconds);
    }

    /// Whether the skin wanted is in, readied for the layout wanted, and
    /// a frame was drawn.
    pub(crate) fn ready(&self) -> bool {
        self.skin.ready() && self.preview.texture().is_some()
    }

    /// What is wanted against what is shown, for the verify skill
    /// ([`SkinSlot::debug_wants`]).
    pub(crate) fn debug_wants(&self) -> String {
        self.skin.debug_wants()
    }

    /// Whether the skin wanted is in: shown, or loaded for the next
    /// frame to show (the boot's preload waits for this, not a frame).
    pub(crate) fn skin_in(&self) -> bool {
        self.skin.skin_in() || self.skin.load().is_some_and(Load::done)
    }

    pub(crate) fn preview(&self) -> &FieldPreview {
        &self.preview
    }

    /// Shows `settings` from the next frame; the skin they want loads
    /// meanwhile.
    pub(crate) fn set_settings(&mut self, settings: &Settings) {
        self.preview.set_receptors(None);
        let options = self.skin.set_settings(settings);
        self.preview.set_options(options);
    }

    /// The prompt's receptor diagram, its skin loaded when it is not the
    /// one in.
    pub(crate) fn set_receptors(&mut self, p: &ddi_ui::screens::prompt_controls::Picture) {
        let (options, pose) = self.skin.set_receptors(p);
        self.preview.set_options(options);
        self.preview.set_receptors(Some(pose));
    }

    /// Loads the diagram's skin for `controls` (a chart song select
    /// shows) without showing anything, so the prompt's diagram is in
    /// from its first frame (`host` is this preview's slot, for the load
    /// to come back to).
    pub(crate) fn warm(
        &mut self,
        gfx: &Gfx,
        host: &Weak<RefCell<Option<FieldPreviewHost>>>,
        controls: &Controls,
    ) {
        let options = self.skin.warm(controls);
        self.preview.set_options(options);
        self.step(gfx, host);
    }

    /// Reads the stored settings again when it is time (`now_ms`).
    pub(crate) fn poll_settings(&mut self, now_ms: f64) {
        if now_ms - self.read_ms < READ_EVERY_MS && now_ms >= self.read_ms {
            return;
        }
        self.read_ms = now_ms;
        self.set_settings(&Settings::load());
    }

    /// Records a frame of `size` pixels at `now_ms` into `encoder`, and
    /// gives the 2D pass the texture when it is a new one. Starts loading
    /// the skin wanted (`host` is this preview's slot, for the load to
    /// come back to).
    pub(crate) fn draw(
        &mut self,
        gfx: &Gfx,
        encoder: &mut wgpu::CommandEncoder,
        size: [u32; 2],
        now_ms: f64,
        host: &Weak<RefCell<Option<FieldPreviewHost>>>,
    ) {
        self.step(gfx, host);
        // Nothing until the skin wanted is in, readied for the layout
        // wanted: the panel's placeholder, or the last frame, never the
        // built-in arrows on the way.
        if self.skin.ready() {
            self.preview
                .draw(&gfx.device, &gfx.queue, encoder, size, now_ms / 1000.0);
        }
        let generation = self.preview.generation();
        if self.registered != Some(generation) {
            let texture = self.preview.texture();
            crate::web::screen::with_painter(gfx, |p| {
                p.set_external_image(&gfx.device, SOURCE, texture);
            });
            self.registered = Some(generation);
        }
    }

    /// Once no screen shows it: takes the texture from the 2D pass and
    /// lets the preview's textures go (the skin stays, for the next
    /// time; a new frame makes them again).
    pub(crate) fn release(&mut self, gfx: &Gfx) {
        if self.registered.take().is_some() {
            crate::web::screen::with_painter(gfx, |p| {
                p.set_external_image(&gfx.device, SOURCE, None);
            });
        }
        self.preview.release_targets();
    }

    /// Starts loading the skin wanted unless it is shown or loading, and
    /// shows one loaded.
    fn step(&mut self, gfx: &Gfx, host: &Weak<RefCell<Option<FieldPreviewHost>>>) {
        let mut started = None;
        let skin = self.skin.step(|key, layout, env| {
            started = Some(key.to_string());
            let load = Load {
                cell: Rc::default(),
                cancelled: Rc::default(),
            };
            let (cell, cancelled) = (load.cell.clone(), load.cancelled.clone());
            let (key, layout, host) = (key.to_string(), layout.clone(), host.clone());
            wasm_bindgen_futures::spawn_local(async move {
                let wanted: crate::noteskins::Wanted = {
                    let cancelled = cancelled.clone();
                    Rc::new(move || !cancelled.get())
                };
                let prepared =
                    crate::noteskins::prepare_while(&key, &layout, env, wanted, None).await;
                if cancelled.get() {
                    if let Some(p) = prepared {
                        p.close();
                    }
                    return;
                }
                *cell.borrow_mut() = Some(prepared);
                // Shown now, its textures uploaded, so the screen that
                // draws it next (the prompt's diagram) has it from its
                // first frame; by the next frame where the device is gone.
                let Some(rc) = host.upgrade() else {
                    return;
                };
                let mut slot = rc.borrow_mut();
                if let Some(h) = slot.as_mut()
                    && let Some(gfx) = h.gfx.upgrade()
                    && h.on(&gfx)
                {
                    h.step(&gfx, &host);
                }
            });
            load
        });
        if let Some(key) = started {
            self.status = format!("loading {key}…");
        }
        if let Some(prepared) = skin {
            self.show_skin(gfx, prepared);
        }
    }

    /// Shows `prepared` (`None`: the built-in arrows) for the skin the
    /// slot now has shown.
    fn show_skin(&mut self, gfx: &Gfx, prepared: Option<Prepared>) {
        let key = self.skin.shown().and_then(|(w, _)| w.key.clone());
        let (play, images): (Option<NoteskinPlay>, _) = match prepared {
            Some(p) => (Some(p.play), p.images),
            None => (None, Vec::new()),
        };
        self.status = match (&play, key) {
            (Some(p), _) => format!("{} on {}", p.name, gfx.backend_name()),
            (None, None) => format!("built-in arrows on {}", gfx.backend_name()),
            (None, Some(key)) => format!("no noteskin {key}: the built-in arrows"),
        };
        let mut uploaded = Vec::new();
        self.preview.set_noteskin(
            &gfx.device,
            &gfx.queue,
            play,
            |renderer: &mut Renderer, play: &mut NoteskinPlay| {
                uploaded = crate::game_loop::upload_noteskin(
                    renderer,
                    &gfx.device,
                    &gfx.queue,
                    play,
                    &images,
                );
            },
        );
        // On the device once the frame is submitted (WebGL2 copies then):
        // closed after it, not before.
        crate::web::screen::close_after_submit(uploaded);
    }
}
