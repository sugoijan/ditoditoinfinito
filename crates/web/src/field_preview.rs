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

use ddi_chart::Layout;
use ddi_render::Renderer;
use ddi_render::field_preview::{FieldPreview, SkinWant};
use ddi_render::noteskin::NoteskinPlay;
use ddi_ui::screens::prompt_controls::Controls;

pub(crate) use ddi_render::field_preview::SOURCE;

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

/// The preview on one device.
pub(crate) struct FieldPreviewHost {
    preview: FieldPreview,
    /// The device it was made on.
    generation: u64,
    /// The preview's texture generation the 2D pass was given.
    registered: Option<u64>,
    /// The skin shown (none before the first) and the layout it was
    /// readied for, the one wanted, the one being loaded, and one loaded
    /// for the next frame to show (`None` inside: the built-in arrows).
    shown: Option<(SkinWant, Layout)>,
    wanted: Option<(SkinWant, Layout)>,
    loading: Option<(SkinWant, Layout)>,
    ready: Option<(SkinWant, Layout, Option<crate::noteskins::Prepared>)>,
    /// The number of the load wanted: a load whose number is not it any
    /// more gives up (`noteskins::prepare_while`), so stepping through
    /// skins loads only the last.
    load_number: Rc<Cell<u64>>,
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
    pub(crate) fn new(gfx: &Gfx) -> FieldPreviewHost {
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
            shown: None,
            wanted: None,
            loading: None,
            ready: None,
            load_number: Rc::default(),
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
        self.shown == self.wanted && self.preview.texture().is_some()
    }
    pub(crate) fn preview(&self) -> &FieldPreview {
        &self.preview
    }

    /// Shows `settings` from the next frame; the skin they want loads
    /// meanwhile.
    pub(crate) fn set_settings(&mut self, settings: &Settings) {
        self.preview.set_receptors(None);
        let options = ddi_shell::field_preview_options(settings);
        let want = ddi_shell::field_preview_skin(settings, &options);
        let layout = want.layout();
        self.wanted = Some((want, layout));
        self.preview.set_options(options);
    }

    /// The prompt's receptor diagram, its skin loaded when it is not the
    /// one in.
    pub(crate) fn set_receptors(&mut self, p: &ddi_ui::screens::prompt_controls::Picture) {
        let (options, want, pose) = ddi_shell::receptor_preview_options(p);
        self.wanted = Some((want, pose.layout.clone()));
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
        let (options, want, layout) = ddi_shell::receptor_skin(controls);
        self.wanted = Some((want, layout));
        self.preview.set_options(options);
        self.load_skin(gfx, host);
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
        if let Some((want, layout, prepared)) = self.ready.take() {
            self.show_skin(gfx, want, layout, prepared);
        }
        self.load_skin(gfx, host);
        // Nothing until the skin wanted is in, readied for the layout
        // wanted: the panel's placeholder, or the last frame, never the
        // built-in arrows on the way.
        if self.wanted.is_none() || self.shown == self.wanted {
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

    /// Starts loading the wanted skin unless it is shown or loading.
    fn load_skin(&mut self, gfx: &Gfx, host: &Weak<RefCell<Option<FieldPreviewHost>>>) {
        let Some((want, layout)) = self.wanted.clone() else {
            return;
        };
        if self.loading.as_ref() == Some(&(want.clone(), layout.clone())) {
            return;
        }
        // The load running, if any, is not wanted any more: it gives up.
        let number = self.load_number.get() + 1;
        self.load_number.set(number);
        self.loading = None;
        if self.shown == self.wanted
            || self
                .ready
                .as_ref()
                .is_some_and(|(w, l, _)| (w, l) == (&want, &layout))
        {
            return;
        }
        let Some(key) = want.key.clone() else {
            // The built-in arrows: at once.
            self.show_skin(gfx, want, layout, None);
            return;
        };
        self.loading = Some((want.clone(), layout.clone()));
        self.status = format!("loading {key}…");
        let host = host.clone();
        let current = self.load_number.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let env = want.env(&layout);
            let wanted: crate::noteskins::Wanted = Rc::new(move || current.get() == number);
            let prepared = crate::noteskins::prepare_while(&key, &layout, env, wanted, None).await;
            let Some(rc) = host.upgrade() else {
                if let Some(p) = prepared {
                    p.close();
                }
                return;
            };
            let mut slot = rc.borrow_mut();
            match slot.as_mut() {
                // Still wanted, on the same preview.
                Some(h) if h.loading.as_ref() == Some(&(want.clone(), layout.clone())) => {
                    h.loading = None;
                    if prepared.is_none() {
                        h.status = format!("no noteskin {key}: the built-in arrows");
                    }
                    // Shown by the next frame, which has the device.
                    if let Some((_, _, Some(old))) = h.ready.replace((want, layout, prepared)) {
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
    }

    /// Shows `prepared` (`None`: the built-in arrows) for `want`, readied
    /// for `layout`.
    fn show_skin(
        &mut self,
        gfx: &Gfx,
        want: SkinWant,
        layout: Layout,
        prepared: Option<crate::noteskins::Prepared>,
    ) {
        let (play, images): (Option<NoteskinPlay>, _) = match prepared {
            Some(p) => (Some(p.play), p.images),
            None => (None, Vec::new()),
        };
        if let Some(p) = &play {
            self.status = format!("{} on {}", p.name, gfx.backend_name());
        } else if want.key.is_none() {
            self.status = format!("built-in arrows on {}", gfx.backend_name());
        }
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
        self.shown = Some((want, layout));
    }
}

impl Drop for FieldPreviewHost {
    fn drop(&mut self) {
        // A load running gives up.
        self.load_number.set(u64::MAX);
        if let Some((_, _, Some(p))) = self.ready.take() {
            p.close();
        }
    }
}
