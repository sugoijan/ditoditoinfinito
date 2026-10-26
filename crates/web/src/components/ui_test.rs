//! The 2D pass's test scene (`docs/plans/ui-core.md`, step 2), at
//! `#/ui-test`, not linked from the menus: quads (plain, one color a
//! corner, turned, textured, a texture loading late, one that fails),
//! StepMania's blend modes, text (system fonts with Japanese, the
//! bundled font, tinted, turned, scaled, measured), clips, an external
//! slot, actors drawn through the bridge (`ddi_render::actors2d`: draw
//! order, corner colors, a shadow, a turned clip through the stencil, a
//! frame in perspective, a mask, a text actor), and a list of 2,000
//! titles scrolling through a clip, which runs the text atlas through
//! its budget. The canvas shows behind the page; a line of the page's
//! own text lies over it.
//!
//! Parameters: `t` holds the scene at that second; `scroll` holds the
//! list at that row; `delay` holds the late texture back that many ms
//! (`window.__DDI_TEXTURE_DELAY_MS`); `budget` keeps that many text pages;
//! `speed` scrolls the list that many rows a second (12); `all=1` sets
//! every title each frame, not only those in view. The scene
//! publishes `window.__DDI_DEBUG` (`frames`, `ui`, and the screen's
//! `screen`, `textures`, `text`, `pass2d`) every 10 frames and
//! `window.__DDI_UI_FRAMES` every frame.

use std::cell::RefCell;
use std::rc::Rc;

use crate::component::{Component, Context};
use ddi_actor::text::{TextMeasure, TextStyle};
use ddi_actor::{Actor, Kind, Sprite, Text, View};
use ddi_render::draw2d::Mask;
use ddi_render::texture_cache::{TextureId, TextureKey};
use ddi_render::{Affine, Blend, Command, DrawList, Quad, TextRun};
use gloo::render::{AnimationFrame, request_animation_frame};
use wasm_bindgen::JsValue;

use crate::web::gfx::{Gfx, SurfaceMode};
use crate::web::screen;

#[derive(Clone, Default, PartialEq)]
pub(crate) struct Props {
    pub(crate) params: Vec<(String, String)>,
}

/// The parameters.
#[derive(Clone, Debug, Default)]
struct Options {
    time: Option<f64>,
    scroll: Option<f32>,
    all: bool,
    budget_pages: Option<u64>,
    /// Rows a second the list scrolls by itself.
    speed: f32,
}

impl Options {
    fn parse(params: &[(String, String)]) -> Options {
        let get = |k: &str| params.iter().find(|(p, _)| p == k).map(|(_, v)| v.as_str());
        Options {
            time: get("t").and_then(|v| v.parse().ok()),
            scroll: get("scroll").and_then(|v| v.parse().ok()),
            all: get("all") == Some("1"),
            budget_pages: get("budget").and_then(|v| v.parse().ok()),
            speed: get("speed")
                .and_then(|v| v.parse().ok())
                .unwrap_or(ROWS_PER_SECOND),
        }
    }
}

/// Titles in the list.
const TITLES: usize = 2000;
/// The list's row, in units.
const ROW: f32 = 28.0;
/// Rows a second the list scrolls by itself.
const ROWS_PER_SECOND: f32 = 12.0;
/// The late texture (a bundled song's banner) and one that never loads.
const BANNER: &str = "songs/some-things-must/sevish-bn.png";
const MISSING: &str = "songs/no-such-song/missing.png";

struct Scene {
    options: Options,
    gfx: Option<Rc<Gfx>>,
    attachment: Option<screen::Attachment>,
    start_ms: Option<f64>,
    frames: u64,
    list: DrawList,
    titles: Vec<std::sync::Arc<str>>,
    textures: Option<(TextureId, TextureId)>,
    /// The actors' part, built once.
    actors: Option<Actor>,
    externals: u64,
    /// The last frame's time on the CPU (building, drawing, submitting),
    /// and the longest since the last report, ms.
    cpu_ms: f64,
    worst_ms: f64,
    closed: bool,
}

pub(crate) enum Msg {
    Failed(String),
}

pub(crate) struct UiTest {
    scene: Rc<RefCell<Scene>>,
    raf: Rc<RefCell<Option<AnimationFrame>>>,
    failure: Option<String>,
}

impl Component for UiTest {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let options = Options::parse(&ctx.props().params);
        if let (Some(ms), Some(w)) = (
            ctx.props()
                .params
                .iter()
                .find(|(k, _)| k == "delay")
                .and_then(|(_, v)| v.parse::<f64>().ok()),
            web_sys::window(),
        ) {
            let _ = js_sys::Reflect::set(&w, &"__DDI_TEXTURE_DELAY_MS".into(), &ms.into());
        }
        UiTest {
            scene: Rc::new(RefCell::new(Scene {
                options,
                gfx: None,
                attachment: None,
                start_ms: None,
                frames: 0,
                list: DrawList::new(),
                titles: titles(),
                textures: None,
                actors: None,
                externals: 0,
                cpu_ms: 0.0,
                worst_ms: 0.0,
                closed: false,
            })),
            raf: Rc::new(RefCell::new(None)),
            failure: None,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Msg) -> bool {
        match msg {
            Msg::Failed(e) => self.failure = Some(e),
        }
        true
    }

    fn rendered(&mut self, ctx: &Context<Self>, first_render: bool) {
        if !first_render {
            return;
        }
        self.scene.borrow_mut().attachment = screen::attach_backdrop();
        let scene = self.scene.clone();
        let raf = self.raf.clone();
        let link = ctx.link().clone();
        wasm_bindgen_futures::spawn_local(async move {
            match screen::gfx().await {
                Ok(gfx) => {
                    if scene.borrow().closed {
                        return;
                    }
                    gfx.set_mode(SurfaceMode::Plain);
                    scene.borrow_mut().gfx = Some(gfx);
                    schedule(raf, scene);
                }
                Err(e) => link.send_message(Msg::Failed(e)),
            }
        });
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        let mut scene = self.scene.borrow_mut();
        scene.closed = true;
        scene.attachment = None;
        self.raf.borrow_mut().take();
        if let Some(gfx) = scene.gfx.take() {
            // Back to the play's configuration for the routes that draw
            // the field.
            gfx.set_mode(SurfaceMode::Scene);
        }
    }

    fn view(&self, ctx: &Context<Self>) {
        let root = ctx.root();
        if let Some(e) = &self.failure {
            crate::component::no_gpu(root, e);
            return;
        }
        if root.child_element_count() == 0 {
            root.set_class_name("ui-test");
            crate::component::element(root, "p", "ui-test-dom").set_text_content(Some(
                "This line is the page's: the scene is drawn on the canvas behind it.",
            ));
        }
    }
}

fn schedule(raf: Rc<RefCell<Option<AnimationFrame>>>, scene: Rc<RefCell<Scene>>) {
    let raf2 = raf.clone();
    let scene2 = scene.clone();
    let handle = request_animation_frame(move |time_ms| {
        if scene2.borrow().closed {
            return;
        }
        frame(&mut scene2.borrow_mut(), time_ms);
        schedule(raf2, scene2);
    });
    *raf.borrow_mut() = Some(handle);
}

/// Deterministic song-list-like titles: Latin, kana and kanji, accents.
fn titles() -> Vec<std::sync::Arc<str>> {
    const LATIN: [&str; 12] = [
        "Infinite",
        "Dream",
        "Sakura",
        "Revolution",
        "Café",
        "Naïve",
        "Ångström",
        "Señorita",
        "Overdrive",
        "Paradise",
        "Twilight",
        "Zenith",
    ];
    const JAPANESE: [&str; 10] = [
        "さくら",
        "革命",
        "夢幻泡影",
        "ありがとう",
        "花火",
        "未来へ",
        "東京",
        "ゆめ",
        "桜吹雪",
        "旅立ち",
    ];
    (0..TITLES)
        .map(|i| {
            let a = LATIN[i % LATIN.len()];
            let b = LATIN[(i * 7 + 3) % LATIN.len()];
            let j = JAPANESE[(i * 3) % JAPANESE.len()];
            std::sync::Arc::from(format!("{:04} {a} {j} {b}", i + 1))
        })
        .collect()
}

/// A color from 8-bit sRGB values.
fn rgb(r: u8, g: u8, b: u8) -> [f32; 4] {
    [
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        1.0,
    ]
}

fn quad(rect: [f32; 4], colors: [[f32; 4]; 4], blend: Blend) -> Quad {
    Quad {
        colors,
        blend,
        ..Quad::plain(rect, [1.0; 4])
    }
}

/// Plays `commands` on `a` (as a metrics command string).
fn run(a: &mut Actor, commands: &str) {
    let mut p = ddi_actor::Problems::default();
    for c in ddi_actor::commands::parse(commands).expect("the scene's commands parse") {
        let _ = a.call(&c, &mut p, 0);
    }
}

/// An untextured quad actor of `size` after `commands`.
fn quad_actor(size: [f32; 2], commands: &str) -> Actor {
    let mut a = Actor::new(Kind::Sprite(Sprite::quad(size)));
    run(&mut a, commands);
    a
}

fn frame_actor(children: Vec<Actor>, commands: &str) -> Actor {
    let mut f = Actor::new(Kind::Frame(children));
    f.sort_by_draw_order();
    run(&mut f, commands);
    f
}

/// The actors' part (`y` 680–790): draw order (red, drawn last, on
/// top), a quad red at its top and blue at its bottom, a yellow quad's
/// shadow on white, a turned clip (through the stencil), a quad nearer
/// the eye of a frame in perspective, a magenta quad with a mask's hole,
/// and a text actor with a shadow.
fn actors() -> Actor {
    let order = frame_actor(
        vec![
            quad_actor([40.0, 40.0], "diffuse,1,0,0,1;draworder,2"),
            quad_actor([40.0, 40.0], "x,15;diffuse,0,1,0,1"),
            quad_actor([40.0, 40.0], "x,30;diffuse,0,0,1,1;draworder,1"),
        ],
        "x,40;y,720",
    );
    let gradient = quad_actor(
        [40.0, 60.0],
        "x,120;y,720;diffusetopedge,1,0,0,1;diffusebottomedge,0,0,1,1",
    );
    let shadow = frame_actor(
        vec![
            quad_actor([80.0, 80.0], "diffuse,1,1,1,1"),
            quad_actor([40.0, 40.0], "diffuse,1,0.9,0.2,1;shadowlength,6"),
        ],
        "x,200;y,730",
    );
    let mut clipped = frame_actor(
        vec![quad_actor([200.0, 200.0], "diffuse,0.2,0.8,0.3,1")],
        "x,300;y,730;rotationz,30",
    );
    clipped.clip = Some([-30.0, -30.0, 30.0, 30.0]);
    let perspective = frame_actor(
        vec![quad_actor(
            [40.0, 40.0],
            "x,420;y,730;z,200;diffuse,1,0.5,0,1",
        )],
        "fov,90;vanishpoint,420,730",
    );
    let mask = frame_actor(
        vec![
            quad_actor([20.0, 20.0], "MaskSource,true"),
            quad_actor([60.0, 60.0], "diffuse,0.9,0.2,0.9,1;MaskDest"),
        ],
        "x,520;y,730",
    );
    let mut text = Actor::new(Kind::Text(Text::new(
        "Actors 文字",
        TextStyle::new("system-ui, sans-serif", 20.0),
    )));
    run(
        &mut text,
        "x,600;y,730;horizalign,'HorizAlign_Left';shadowlength,2;diffuse,0.6,0.9,1,1",
    );
    frame_actor(
        vec![order, gradient, shadow, clipped, perspective, mask, text],
        "",
    )
}

/// The scene at `t` seconds, `width` units wide.
fn build(scene: &mut Scene, t: f64, width: f32, height: f32, measure: &mut dyn TextMeasure) {
    let (banner, missing) = scene.textures.expect("textures known");
    let l = &mut scene.list;
    l.clear();
    let white = [1.0; 4];
    let sans = TextStyle::new("system-ui, sans-serif", 24.0);
    let bundled = TextStyle {
        weight: 800,
        ..TextStyle::new(
            format!("\"{}\", monospace", crate::web::text::BUNDLED_FAMILY),
            24.0,
        )
    };
    l.text(
        "Dito Dito Infinito — the 2D pass",
        &bundled,
        24.0,
        16.0,
        white,
    );

    // Quads: plain, a color a corner, turned, textured (late), failed.
    l.rect([24.0, 70.0, 124.0, 170.0], rgb(224, 64, 64));
    l.quad(quad(
        [140.0, 70.0, 240.0, 170.0],
        [
            rgb(255, 0, 0),
            rgb(0, 255, 0),
            rgb(0, 0, 255),
            rgb(255, 255, 255),
        ],
        Blend::Normal,
    ));
    let turn = (t as f32) * 0.5 + 0.5;
    l.quad(Quad {
        transform: Affine::rotate(turn).then(Affine::translate(310.0, 120.0)),
        ..quad(
            [-40.0, -40.0, 40.0, 40.0],
            [rgb(240, 200, 40); 4],
            Blend::Normal,
        )
    });
    l.image([380.0, 70.0, 636.0, 150.0], banner, white);
    l.image([660.0, 70.0, 740.0, 150.0], missing, white);

    // Blend modes over grey: normal, add, subtract, modulate.
    for (i, blend) in [Blend::Normal, Blend::Add, Blend::Subtract, Blend::Modulate]
        .into_iter()
        .enumerate()
    {
        let x = 24.0 + i as f32 * 140.0;
        l.rect([x, 200.0, x + 120.0, 300.0], rgb(128, 128, 128));
        l.quad(quad(
            [x + 60.0, 220.0, x + 140.0, 280.0],
            [[0.5, 0.25, 0.75, 1.0]; 4],
            blend,
        ));
    }

    // Text: system fonts (Japanese through the fallback), accents, the
    // bundled font, tinted, turned, scaled, and measured.
    l.text("革命 KAKUMEI", &sans, 24.0, 320.0, white);
    l.text(
        "さくら — Ça déjà vu, Ångström, naïve, ñandú",
        &sans,
        24.0,
        356.0,
        white,
    );
    l.text(
        "INFINITO 0123456789 ÀÉÎÕÜ",
        &bundled,
        24.0,
        392.0,
        rgb(255, 128, 64),
    );
    let measured = "Measured: 夢幻泡影";
    let m = measure.measure(measured, &sans);
    l.text(measured, &sans, 24.0, 428.0, white);
    // Underlined by its advance, on its baseline.
    l.rect(
        [
            24.0,
            428.0 + m.baseline + 2.0,
            24.0 + m.advance,
            428.0 + m.baseline + 4.0,
        ],
        rgb(64, 200, 255),
    );
    l.push(Command::Text(TextRun {
        curve: None,
        text: "turned 回転".into(),
        style: sans.clone(),
        transform: Affine::rotate(-0.2).then(Affine::translate(420.0, 330.0)),
        color: rgb(160, 255, 160),
        blend: Blend::Normal,
        mask: Mask::None,
        perspective: None,
        blur: [0.0; 2],
    }));
    l.push(Command::Text(TextRun {
        curve: None,
        text: "×2".into(),
        style: sans.clone(),
        transform: Affine::scale(2.0, 2.0).then(Affine::translate(600.0, 380.0)),
        color: white,
        blend: Blend::Add,
        mask: Mask::None,
        perspective: None,
        blur: [0.0; 2],
    }));

    // Clips: a rectangle and text cut by one, and a clip within a clip.
    l.push_clip([24.0, 480.0, 224.0, 540.0]);
    l.rect([0.0, 470.0, 300.0, 560.0], rgb(40, 80, 160));
    l.text("clipped clipped clipped clipped", &sans, 30.0, 490.0, white);
    l.push_clip([150.0, 400.0, 400.0, 600.0]);
    l.rect([0.0, 0.0, 1000.0, 1000.0], rgb(200, 40, 160));
    l.pop_clip();
    l.pop_clip();

    // The shell's own drawing between layers (the play field, later).
    l.external(0);
    l.rect([24.0, 560.0, 124.0, 580.0], rgb(255, 255, 255));

    // StepMania's other blend modes over grey, a half-transparent color:
    // weighted multiply, invert dest, alpha multiply, copy source, no
    // effect.
    for (i, blend) in [
        Blend::WeightedMultiply,
        Blend::InvertDest,
        Blend::AlphaMultiply,
        Blend::CopySrc,
        Blend::NoEffect,
    ]
    .into_iter()
    .enumerate()
    {
        let x = 24.0 + i as f32 * 80.0;
        l.rect([x, 600.0, x + 60.0, 660.0], rgb(128, 128, 128));
        l.quad(quad(
            [x + 30.0, 610.0, x + 70.0, 650.0],
            [[0.5, 0.25, 0.75, 0.5]; 4],
            blend,
        ));
    }

    // Actors, through the bridge, measured as the painter draws.
    let actors = scene.actors.get_or_insert_with(actors);
    actors.measure(measure);
    let mut drawn = Vec::new();
    actors.draw_in(
        &View {
            transform: ddi_actor::TRANSFORM_IDENTITY,
            size: [width, height],
        },
        ddi_actor::TRANSFORM_IDENTITY,
        [1.0; 4],
        [1.0, 1.0, 1.0, 0.0],
        &mut drawn,
    );
    ddi_render::actors2d::append(&drawn, l, &mut |_| None);

    // The list: 2,000 titles through a clip, scrolling.
    let (x0, y0, x1, y1) = (width - 460.0, 70.0, width - 24.0, height - 24.0);
    l.rect([x0, y0, x1, y1], rgb(24, 24, 36));
    l.push_clip([x0, y0, x1, y1]);
    let scroll = scene
        .options
        .scroll
        .unwrap_or((t as f32 * scene.options.speed) % TITLES as f32);
    let first = scroll.floor().max(0.0) as usize;
    let rows = ((y1 - y0) / ROW).ceil() as usize + 1;
    let style = TextStyle::new("system-ui, sans-serif", 18.0);
    let shown: Box<dyn Iterator<Item = usize>> = if scene.options.all {
        Box::new(0..TITLES)
    } else {
        Box::new(first..(first + rows).min(TITLES))
    };
    for i in shown {
        let y = y0 + (i as f32 - scroll) * ROW + 4.0;
        l.text(scene.titles[i].clone(), &style, x0 + 12.0, y, white);
    }
    l.pop_clip();
}

fn frame(scene: &mut Scene, time_ms: f64) {
    let Some(gfx) = scene.gfx.clone() else {
        return;
    };
    // The device is gone (the route starts over on the new one), or
    // another screen took the canvas.
    if gfx.is_lost() || !scene.attachment.as_ref().is_some_and(|a| a.holds()) {
        return;
    }
    let Some(canvas) = screen::canvas() else {
        return;
    };
    let window = web_sys::window().expect("window");
    let dpr = window.device_pixel_ratio() as f32;
    // The canvas's own box (the window's less any scrollbar).
    let css_w = (canvas.client_width() as f32).max(1.0);
    let css_h = (canvas.client_height() as f32).max(1.0);
    let (w, h) = (
        (css_w * dpr).round().max(1.0) as u32,
        (css_h * dpr).round().max(1.0) as u32,
    );
    if canvas.width() != w || canvas.height() != h {
        canvas.set_width(w);
        canvas.set_height(h);
    }
    gfx.set_mode(SurfaceMode::Plain);
    gfx.resize(w, h);
    let now = || window.performance().map_or(0.0, |p| p.now());
    let began = now();
    let start = *scene.start_ms.get_or_insert(time_ms);
    let t = scene.options.time.unwrap_or((time_ms - start) / 1000.0);
    screen::prepare_textures(&gfx);
    let Some(format) = gfx.menu_format() else {
        return;
    };
    let budget = scene.options.budget_pages;
    let prepared = screen::with_painter(&gfx, |p| {
        if let Some(pages) = budget {
            let page = u64::from(ddi_render::text_atlas::PAGE);
            p.set_budgets(ddi_render::painter::TEXTURE_BUDGET, pages * page * page);
        }
        if scene.textures.is_none() {
            scene.textures = Some((
                p.texture(TextureKey::drawn(BANNER, 256.0 * dpr)),
                p.texture(TextureKey::drawn(MISSING, 80.0 * dpr)),
            ));
        }
        let mut measure = p.measure(dpr);
        build(scene, t, css_w, css_h, &mut measure);
    });
    if prepared.is_none() {
        return;
    }
    let Some(surface) = gfx.frame() else {
        return;
    };
    let view = surface.texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(format),
        ..Default::default()
    });
    let mut encoder = gfx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ui-test"),
        });
    let list = std::mem::take(&mut scene.list);
    let mut externals = 0;
    screen::with_painter(&gfx, |p| {
        p.draw(
            &gfx.device,
            &gfx.queue,
            &mut encoder,
            &view,
            [w, h],
            dpr,
            Some(wgpu::Color {
                r: 16.0 / 255.0,
                g: 16.0 / 255.0,
                b: 24.0 / 255.0,
                a: 1.0,
            }),
            &list,
            &mut |_slot, _encoder| externals += 1,
        );
    });
    scene.list = list;
    scene.externals += externals;
    gfx.queue.submit(Some(encoder.finish()));
    gfx.queue.present(surface);
    screen::after_submit();
    scene.cpu_ms = now() - began;
    scene.worst_ms = scene.worst_ms.max(scene.cpu_ms);
    scene.frames += 1;
    let _ = js_sys::Reflect::set(
        &window,
        &"__DDI_UI_FRAMES".into(),
        &(scene.frames as f64).into(),
    );
    if scene.frames % 10 == 1 {
        publish(scene, &gfx, t);
        scene.worst_ms = 0.0;
    }
}

fn publish(scene: &Scene, gfx: &Gfx, t: f64) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let obj = js_sys::Object::new();
    let set = |o: &js_sys::Object, k: &str, v: JsValue| {
        let _ = js_sys::Reflect::set(o, &k.into(), &v);
    };
    set(&obj, "frames", (scene.frames as f64).into());
    set(&obj, "backend", gfx.backend_name().into());
    let ui = js_sys::Object::new();
    set(&ui, "t", t.into());
    set(&ui, "commands", (scene.list.commands.len() as f64).into());
    set(&ui, "externals", (scene.externals as f64).into());
    set(&ui, "cpu_ms", scene.cpu_ms.into());
    set(&ui, "worst_ms", scene.worst_ms.into());
    set(&obj, "ui", ui.into());
    screen::debug_into(&obj);
    let _ = js_sys::Reflect::set(&window, &"__DDI_DEBUG".into(), &obj);
}
