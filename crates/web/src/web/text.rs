//! Text runs drawn by the browser (`docs/plans/ui-core.md`, P4): the
//! web's [`RunRasterizer`]. Canvas2D sets and draws each run (system
//! fonts and their fallback, Japanese included, nothing downloaded) on a
//! hidden staging canvas a page large, and the runs drawn in a frame go
//! up to their page's texture in one copy (step 0's findings):
//!
//! - WebGPU copies the canvas's dirty rectangle with
//!   `copyExternalImageToTexture`, which reads the canvas when it is
//!   called, so the rectangle is cleared right after for the next runs.
//! - WebGL2: wgpu runs that copy at the next submit, from a source origin
//!   of zero only (and never from an `OffscreenCanvas`), so the staging
//!   canvas would be read after it changed. There the rectangle is read
//!   with `getImageData` and written with `write_texture`, at any origin.
//!
//! Runs are white, so the page keeps coverage alone (`R8Unorm`): copied
//! premultiplied, red is alpha. Bundled faces reach the canvas as
//! `FontFace`s: the HUD's from the wasm's bytes ([`load_bundled_font`]),
//! the theme's text face from files served next to the app
//! ([`load_theme_fonts`], fetched at start; the boot shell stays until
//! every face settled, [`faces`]). Runs drawn
//! before a face loaded are drawn again once it has
//! ([`RunRasterizer::fonts_version`]), and pages drawing on demand are
//! woken for it ([`on_fonts_loaded`]).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ddi_actor::text::{RunMetrics, TextStyle};
use ddi_render::atlas::PageRect;
use ddi_render::text_atlas::{PAGE, RunRasterizer};
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

/// The bundled font's family, as styles name it (its own name, so a
/// style reads the same to cosmic-text on desktop).
pub(crate) const BUNDLED_FAMILY: &str = "Iosevka Custom";

/// The theme's text face (`ddi_theme`'s `fonts.text`): its weights and
/// where they are served (`index.html` copies them from `assets/fonts`).
pub(crate) const THEME_FAMILY: &str = "Iosevka Term";
const THEME_FACES: [(&str, &str); 3] = [
    ("400 500", "fonts/IosevkaTerm-Regular-latin.ttf"),
    ("600", "fonts/IosevkaTerm-SemiBold-latin.ttf"),
    ("700 900", "fonts/IosevkaTerm-Bold-latin.ttf"),
];

thread_local! {
    /// Bumped when a bundled face finishes loading.
    static FONTS_VERSION: Rc<Cell<u64>> = Rc::new(Cell::new(0));
    /// The theme's faces were asked for.
    static THEME_ASKED: Cell<bool> = const { Cell::new(false) };
    /// Each face's load, as asked for.
    static FACES: RefCell<Vec<js_sys::Promise>> = const { RefCell::new(Vec::new()) };
    /// Called when a face finishes loading; dropped once they answer
    /// false (their page is gone).
    static WAKERS: RefCell<Vec<Box<dyn Fn() -> bool>>> = RefCell::new(Vec::new());
}

/// Calls `f` each time a face finishes loading (a page drawing only on
/// demand asks for a frame then), until it returns false.
pub(crate) fn on_fonts_loaded(f: impl Fn() -> bool + 'static) {
    WAKERS.with(|w| w.borrow_mut().push(Box::new(f)));
}

fn loaded() {
    FONTS_VERSION.with(|v| v.set(v.get() + 1));
    // Not borrowed while they run: a waker may draw.
    let mut wakers = WAKERS.with(|w| std::mem::take(&mut *w.borrow_mut()));
    wakers.retain(|f| f());
    WAKERS.with(|w| {
        let mut w = w.borrow_mut();
        let added = std::mem::take(&mut *w);
        *w = wakers;
        w.extend(added);
    });
}

/// Registers the theme's text face, its three weights fetched from their
/// files (27 KB each, not in the wasm); once.
pub(crate) fn load_theme_fonts() {
    if THEME_ASKED.with(|a| a.replace(true)) {
        return;
    }
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    for (weight, url) in THEME_FACES {
        let descriptors = web_sys::FontFaceDescriptors::new();
        descriptors.set_weight(weight);
        let face = match web_sys::FontFace::new_with_str_and_descriptors(
            THEME_FAMILY,
            &format!("url({url})"),
            &descriptors,
        ) {
            Ok(f) => f,
            Err(e) => {
                web_sys::console::warn_1(&format!("theme font {url}: {e:?}").into());
                continue;
            }
        };
        let _ = document.fonts().add(&face);
        let Ok(promise) = face.load() else {
            continue;
        };
        FACES.with(|f| f.borrow_mut().push(promise.clone()));
        wasm_bindgen_futures::spawn_local(async move {
            match wasm_bindgen_futures::JsFuture::from(promise).await {
                Ok(_) => loaded(),
                Err(e) => web_sys::console::warn_1(&format!("theme font {url}: {e:?}").into()),
            }
        });
    }
}

/// The loads of the faces asked for so far, each settling when its face
/// loaded or failed to.
pub(crate) fn faces() -> Vec<js_sys::Promise> {
    FACES.with(|f| f.borrow().clone())
}

/// Faces loaded so far (a page compares it with the last it drew with).
pub(crate) fn fonts_version() -> u64 {
    FONTS_VERSION.with(|v| v.get())
}

/// Registers the bundled font with the document (`FontFace` from its
/// bytes, 1–3 ms in step 0); text drawn meanwhile uses the fallback and
/// is drawn again when it is ready.
pub(crate) fn load_bundled_font() {
    let descriptors = web_sys::FontFaceDescriptors::new();
    descriptors.set_weight("800");
    let face = match web_sys::FontFace::new_with_u8_array_and_descriptors(
        BUNDLED_FAMILY,
        crate::game_loop::FONT,
        &descriptors,
    ) {
        Ok(f) => f,
        Err(e) => {
            web_sys::console::warn_1(&format!("bundled font: {e:?}").into());
            return;
        }
    };
    if let Some(document) = web_sys::window().and_then(|w| w.document()) {
        let _ = document.fonts().add(&face);
    }
    let Ok(promise) = face.load() else {
        return;
    };
    FACES.with(|f| f.borrow_mut().push(promise.clone()));
    wasm_bindgen_futures::spawn_local(async move {
        match wasm_bindgen_futures::JsFuture::from(promise).await {
            Ok(_) => loaded(),
            Err(e) => web_sys::console::warn_1(&format!("bundled font: {e:?}").into()),
        }
    });
}

pub(crate) struct CanvasRasterizer {
    ctx: CanvasRenderingContext2d,
    canvas: HtmlCanvasElement,
    /// Pages are filled from `getImageData` (WebGL2), not copied from the
    /// canvas.
    read_back: bool,
    /// The CSS font last set, and the letter spacing (pixels).
    font: String,
    spacing: f32,
}

impl CanvasRasterizer {
    /// A staging canvas of a page; `read_back` on WebGL2 (the canvas is
    /// then kept in memory, `willReadFrequently`).
    pub(crate) fn new(read_back: bool) -> Result<CanvasRasterizer, String> {
        let document = web_sys::window()
            .and_then(|w| w.document())
            .ok_or("no document")?;
        let canvas: HtmlCanvasElement = document
            .create_element("canvas")
            .map_err(|e| format!("canvas: {e:?}"))?
            .dyn_into()
            .map_err(|_| "not a canvas")?;
        canvas.set_width(PAGE);
        canvas.set_height(PAGE);
        let options = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&options, &"willReadFrequently".into(), &read_back.into());
        let ctx: CanvasRenderingContext2d = canvas
            .get_context_with_context_options("2d", &options)
            .map_err(|e| format!("2D canvas: {e:?}"))?
            .ok_or("no 2D canvas")?
            .dyn_into()
            .map_err(|_| "no 2D canvas")?;
        ctx.set_fill_style_str("#ffffff");
        Ok(CanvasRasterizer {
            ctx,
            canvas,
            read_back,
            font: String::new(),
            spacing: 0.0,
        })
    }

    fn set_font(&mut self, style: &TextStyle, scale: f32) {
        let font = format!(
            "{}{} {}px {}",
            if style.italic { "italic " } else { "" },
            style.weight,
            style.size() * scale,
            style.family
        );
        if font != self.font {
            self.ctx.set_font(&font);
            self.font = font;
        }
        // Letter spacing (the theme's small capitals): `letterSpacing`
        // where the browser has it, none where it does not.
        let spacing = f32::from(style.tracking) / 1000.0 * style.size() * scale;
        if spacing != self.spacing {
            let _ = js_sys::Reflect::set(
                &self.ctx,
                &"letterSpacing".into(),
                &format!("{spacing}px").into(),
            );
            self.spacing = spacing;
        }
    }
}

/// `v`, or `fallback` where the browser gives no number.
fn or(v: f64, fallback: f64) -> f32 {
    if v.is_finite() {
        v as f32
    } else {
        fallback as f32
    }
}

impl RunRasterizer for CanvasRasterizer {
    fn measure(&mut self, text: &str, style: &TextStyle, scale: f32) -> RunMetrics {
        self.set_font(style, scale);
        let size = f64::from(style.size() * scale);
        let Ok(m) = self.ctx.measure_text(text) else {
            return RunMetrics::default();
        };
        let advance = m.width() as f32;
        // The primary font's box centres the baseline in our line box (the
        // box covers that font alone: its height is not the line's, P4).
        let ascent = or(m.font_bounding_box_ascent(), size * 0.8);
        let descent = or(m.font_bounding_box_descent(), size * 0.2);
        let line = style.line_height() * scale;
        RunMetrics {
            advance,
            ink: [
                -or(m.actual_bounding_box_left(), 0.0),
                -or(m.actual_bounding_box_ascent(), size * 0.8),
                or(m.actual_bounding_box_right(), f64::from(advance)),
                or(m.actual_bounding_box_descent(), size * 0.2),
            ],
            baseline: (line + ascent - descent) / 2.0,
        }
    }

    fn draw(&mut self, text: &str, style: &TextStyle, scale: f32, pen: [f32; 2], cell: PageRect) {
        self.set_font(style, scale);
        // Clipped to the cell: ink past it would never be cleared.
        self.ctx.save();
        self.ctx.begin_path();
        self.ctx.rect(
            f64::from(cell.x),
            f64::from(cell.y),
            f64::from(cell.width),
            f64::from(cell.height),
        );
        self.ctx.clip();
        let _ = self
            .ctx
            .fill_text(text, f64::from(pen[0]), f64::from(pen[1]));
        self.ctx.restore();
    }

    fn upload(&mut self, queue: &wgpu::Queue, texture: &wgpu::Texture, rect: PageRect) {
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
        if w == 0 || h == 0 {
            return;
        }
        if self.read_back {
            if let Ok(data) =
                self.ctx
                    .get_image_data(f64::from(x), f64::from(y), f64::from(w), f64::from(h))
            {
                // White runs: coverage is alpha.
                let alpha: Vec<u8> = data
                    .data()
                    .0
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|p| p[3])
                    .collect();
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x, y, z: 0 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &alpha,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(w),
                        rows_per_image: None,
                    },
                    wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                );
            }
        } else {
            queue.copy_external_image_to_texture(
                &wgpu::CopyExternalImageSourceInfo {
                    source: wgpu::ExternalImageSource::HTMLCanvasElement(self.canvas.clone()),
                    origin: wgpu::Origin2d { x, y },
                    flip_y: false,
                },
                wgpu::CopyExternalImageDestInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x, y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                    color_space: wgpu::PredefinedColorSpace::Srgb,
                    // White premultiplied: red carries the coverage.
                    premultiplied_alpha: true,
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        }
        self.ctx
            .clear_rect(f64::from(x), f64::from(y), f64::from(w), f64::from(h));
    }

    fn fonts_version(&self) -> u64 {
        FONTS_VERSION.with(|v| v.get())
    }
}

impl Drop for CanvasRasterizer {
    fn drop(&mut self) {
        // Frees the page's memory now rather than at collection.
        self.canvas.set_width(0);
        self.canvas.set_height(0);
    }
}
