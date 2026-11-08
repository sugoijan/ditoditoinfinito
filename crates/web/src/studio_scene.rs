//! The announcer studio's scenes in play (`docs/plans/announcer-studio.md`,
//! step 4), between the studio and the play route: the studio stages a
//! line's play (`components::studio_page`) and opens the play route on its
//! chart with `scene=<line>` (`Route::Play`), handing over here what the
//! play is said with ([`StagedPlay`]); the play, staged as the line's
//! scene says (`ddi_session::announcer::scene::in_play`) and shown in the
//! studio's frame, hands back what was said ([`Heard`]) as it goes back to
//! the studio.

use std::cell::RefCell;
use std::collections::HashMap;

use ddi_library::announcer::Line;
pub(crate) use ddi_session::announcer::scene::director::Heard;
use ddi_session::announcer::scene::director::SaidWords;
use web_sys::AudioBuffer;

/// What a staged play is said with, from the studio.
pub(crate) type StagedPlay = ddi_session::announcer::scene::director::StagedPlay<AudioBuffer>;

thread_local! {
    static STAGED: RefCell<Option<StagedPlay>> = const { RefCell::new(None) };
    static HEARD: RefCell<Option<Heard>> = const { RefCell::new(None) };
}

/// The studio stages a play of `play.line`'s scene: the play route opened
/// next takes it.
pub(crate) fn stage(play: StagedPlay) {
    STAGED.with_borrow_mut(|s| *s = Some(play));
    HEARD.with_borrow_mut(|h| *h = None);
}

/// The play route's: the play staged for scene `line` (its folder), if it
/// is the one staged (a reload staged none).
pub(crate) fn take(line: &str) -> Option<StagedPlay> {
    STAGED
        .with_borrow_mut(Option::take)
        .filter(|p| p.line.folder() == line)
}

/// The play's scene is over: what it said, for the studio.
pub(crate) fn heard(heard: Heard) {
    HEARD.with_borrow_mut(|h| *h = Some(heard));
}

/// The studio's, as it opens again: what the last scene in play said.
pub(crate) fn take_heard() -> Option<Heard> {
    HEARD.with_borrow_mut(Option::take)
}

/// The texture source the staged play is drawn under (its picture in the
/// frame, `ddi_ui::screens::stage::Picture`).
pub(crate) const SOURCE: &str = ddi_ui::screens::stage::PICTURE;

/// What the frame around a staged play says.
pub(crate) struct FrameSpec {
    pub(crate) name: String,
    pub(crate) voice_label: String,
    pub(crate) caption: String,
    /// Each line's words and length, for the words shown as it is said.
    pub(crate) words: HashMap<Line, String>,
    pub(crate) lengths: HashMap<Line, f64>,
}

/// A staged play shown in the announcer studio's frame: the play drawn
/// into a texture of its own (as on the whole screen), then a small UI of
/// its own over the canvas, the picture of it inside the frame
/// (`ddi_ui::screens::stage`), drawn by the app's 2D pass.
pub(crate) struct Framed {
    picture: Option<(wgpu::Texture, wgpu::TextureView)>,
    registered: bool,
    ui: ddi_ui::Ui,
    list: ddi_render::DrawList,
    /// The canvas's CSS size and pixel ratio told to the UI.
    viewport: Option<(f64, f64, f64)>,
    last: Option<f64>,
    /// The lines' words, told to the frame as each starts.
    words: SaidWords,
}

impl Framed {
    pub(crate) fn new(spec: FrameSpec) -> Framed {
        use ddi_ui::screens::stage::{Picture, StageFrame};
        let theme = crate::components::ui_dev::theme_host(std::rc::Rc::new(
            crate::components::ui_dev::nfkd,
        ));
        let mut ui = ddi_ui::Ui::new(ddi_ui::screens::DESIGN, ddi_ui::TextMode::Mirror, false);
        ui.set_focus_ring(false);
        ui.push(Picture::new(SOURCE));
        // Its way back: the page's (the keys' Escape, the button's press
        // through `back_at`).
        let back: ddi_ui::screens::stage::BackFn = std::rc::Rc::new(|_| {});
        ui.push_chrome(
            StageFrame::new(
                theme.env().clone(),
                &spec.name,
                &spec.voice_label,
                &spec.caption,
                back,
            )
            .at_once(),
        );
        Framed {
            picture: None,
            registered: false,
            ui,
            list: ddi_render::DrawList::new(),
            viewport: None,
            last: None,
            words: SaidWords::new(spec.words, spec.lengths),
        }
    }

    /// The play's target at `size` (made again at another), and whether
    /// it is a new one.
    pub(crate) fn target(&mut self, device: &wgpu::Device, size: [u32; 2]) -> &wgpu::TextureView {
        let fresh = self
            .picture
            .as_ref()
            .is_none_or(|(t, _)| [t.width(), t.height()] != size);
        if fresh {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("ddi-staged-play"),
                size: wgpu::Extent3d {
                    width: size[0].max(1),
                    height: size[1].max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: ddi_render::field_preview::FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.picture = Some((texture, view));
            self.registered = false;
        }
        &self.picture.as_ref().expect("made above").1
    }

    /// The frame's caption now.
    pub(crate) fn caption(&mut self, text: &str) {
        let m = ddi_ui::Message::new(ddi_ui::screens::stage::CAPTION)
            .with("Text", ddi_ui::Param::Str(text.into()));
        self.ui.broadcast(m);
    }

    /// The scene ends in `seconds` (the frame's bar).
    pub(crate) fn until(&mut self, seconds: f64) {
        let m = ddi_ui::Message::new(ddi_ui::screens::stage::UNTIL)
            .with("Seconds", ddi_ui::Param::Num(seconds));
        self.ui.broadcast(m);
    }

    /// The lines said so far (`report.spoken`: folder, context time):
    /// the words of each one begun by `now` (the play's context time) shown.
    pub(crate) fn said(&mut self, spoken: &[(String, f64)], now: f64) {
        for m in self.words.next(spoken, now) {
            self.ui.broadcast(m);
        }
    }

    /// Where the frame's way back is on the page, CSS pixels `[left, top,
    /// right, bottom]` (a press there leaves the scene).
    pub(crate) fn back_at(&self) -> Option<[f32; 4]> {
        let b = self.ui.visible_bounds("Stage.Back")?;
        let vp = self.ui.viewport();
        let a = vp.to_physical([b[0], b[1]]);
        let c = vp.to_physical([b[2], b[3]]);
        Some([a[0], a[1], c[0], c[1]].map(|v| v / vp.ratio))
    }

    /// The frame and the play's picture in it, drawn over `target` (the
    /// canvas's plain view, `size` pixels; `css` its CSS size, `ratio`
    /// pixels to a CSS pixel) at host second `now`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw(
        &mut self,
        gfx: &crate::web::gfx::Gfx,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
        css: (f64, f64),
        ratio: f64,
        now: f64,
    ) {
        use ddi_render::texture_cache::TextureKey;
        if self.viewport != Some((css.0, css.1, ratio)) {
            self.viewport = Some((css.0, css.1, ratio));
            self.ui.handle(ddi_ui::UiEvent::Resize {
                size: [css.0 as f32, css.1 as f32],
                ratio: ratio as f32,
            });
        }
        let dt = self.last.map_or(0.0, |l| (now - l).clamp(0.0, 0.25));
        self.last = Some(now);
        self.ui.set_clock(now);
        self.ui.update(dt as f32);
        let vp = *self.ui.viewport();
        let shift = [
            1.0,
            0.0,
            0.0,
            1.0,
            vp.offset[0] / vp.scale,
            vp.offset[1] / vp.scale,
        ];
        let picture = self.picture.as_ref().map(|(t, _)| t);
        let registered = &mut self.registered;
        let ui = &mut self.ui;
        let list = &mut self.list;
        crate::web::screen::with_painter(gfx, |p| {
            if !*registered {
                p.set_external_image(&gfx.device, SOURCE, picture);
                *registered = true;
            }
            let drawn = {
                let mut measure = p.measure(vp.scale);
                ui.draw_in(&mut measure, shift)
            };
            list.clear();
            ddi_render::actors2d::append(&drawn, list, &mut |t| {
                let image = ui.texture_source(t)?;
                Some(p.texture(TextureKey::drawn(image.source, image.side * vp.scale)))
            });
            p.draw(
                &gfx.device,
                &gfx.queue,
                encoder,
                target,
                size,
                vp.scale,
                Some(wgpu::Color::BLACK),
                list,
                &mut |_, _| {},
            );
        });
    }

    /// `__DDI_DEBUG.stage` on the play route: the frame's way back (CSS
    /// pixels `[left, top, right, bottom]`), the lines' words shown so
    /// far, and the picture's side (pixels).
    pub(crate) fn debug(&self) -> wasm_bindgen::JsValue {
        let o = js_sys::Object::new();
        let set = |k: &str, v: wasm_bindgen::JsValue| {
            let _ = js_sys::Reflect::set(&o, &k.into(), &v);
        };
        let back = js_sys::Array::new();
        for v in self.back_at().into_iter().flatten() {
            back.push(&f64::from(v).into());
        }
        set("back", back.into());
        set("told", (self.words.told() as f64).into());
        set(
            "picture",
            self.picture
                .as_ref()
                .map_or(wasm_bindgen::JsValue::NULL, |(t, _)| {
                    f64::from(t.width().max(t.height())).into()
                }),
        );
        o.into()
    }

    /// After the frame's submit: the painter's frame is over.
    pub(crate) fn submitted(&self, gfx: &crate::web::gfx::Gfx) {
        crate::web::screen::with_painter(gfx, |p| p.submitted());
    }

    /// The picture taken from the 2D pass (the play is over).
    pub(crate) fn release(&mut self, gfx: &crate::web::gfx::Gfx) {
        if self.registered {
            crate::web::screen::with_painter(gfx, |p| {
                p.set_external_image(&gfx.device, SOURCE, None);
            });
            self.registered = false;
        }
    }
}
