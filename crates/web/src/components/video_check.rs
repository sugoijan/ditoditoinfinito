//! `#/video-check`: runs the software video decoder (the FFmpeg module in
//! its worker) on movies the user picks, to measure it in this browser and
//! check the worker protocol: frames straight through, time per frame, and
//! seeks that must land on the frames decoding straight through gave. Not
//! linked from the menus; `docs/plans/video-backgrounds.md`, step 2.
//! `?src=a,b` checks files of the site instead (headless tests), and the
//! results are mirrored to `window.__DDI_VIDEO_CHECK`. `?decoder=webcodecs`
//! checks the browser's decoder (H.264 AVIs, step 6) and `?decoder=both`
//! both, each file's frames compared between them (times and pixels).

use std::cell::Cell;
use std::rc::Rc;

use crate::component::{Component, Context, NodeRef, element};
use ddi_platform::video::{
    VideoBackend, VideoCodec, VideoContainer, VideoDecoder, VideoEvent, VideoFormat, VideoFrame,
    YuvMatrix,
};
use gloo::timers::callback::Interval;
use gloo::timers::future::TimeoutFuture;
use serde::Serialize;
use wasm_bindgen::{Clamped, JsCast};
use web_sys::{Blob, CanvasRenderingContext2d, HtmlCanvasElement, HtmlInputElement, ImageData};

use crate::router::Route;
use crate::web::clock::PerformanceClock;
use crate::web::video::{ModuleStatus, VideoWorker};
use crate::web::webcodecs::WebCodecsVideo;

/// Frames asked for at a time, and how few may be outstanding before more
/// are asked for.
const BATCH: u32 = 8;
/// The frame shown for each file.
const SAMPLE_FRAME: usize = 30;
/// …and later in a long movie, for the color comparison (frame 30 is
/// often a fade).
const LATE_SAMPLE_FRAME: usize = 900;
/// About this many seek targets per file.
const SEEK_TARGETS: usize = 24;
/// A seek that has not delivered a frame after this long fails, and so does
/// a file the decoder has said nothing about for this long.
const TIMEOUT_MS: f64 = 30_000.0;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub(crate) struct FileCheck {
    name: String,
    /// `module` or `webcodecs`.
    decoder: String,
    bytes: f64,
    /// With both decoders checked, on the browser's row: whether every
    /// frame had the module's time and pixels, or the first difference.
    compared: Option<String>,
    /// With both checked and the browser's frames in its own colors: how
    /// close they are to the module's frame under each matrix and range.
    colors: Option<String>,
    /// `waiting`, `decoding`, `seeking`, `done` or `failed`.
    state: String,
    error: Option<String>,
    codec: String,
    width: u32,
    height: u32,
    duration: Option<f64>,
    frames: usize,
    decode_ms_per_frame: f64,
    wall_ms_per_frame: f64,
    first_pts: Option<f64>,
    last_pts: Option<f64>,
    matrix: String,
    full_range: bool,
    seeks: usize,
    seeks_ok: usize,
    seek_failures: Vec<String>,
    slowest_seek_ms: f64,
    #[serde(skip)]
    sample: Option<Sample>,
    /// The module's frame behind `sample`, for the color comparison.
    #[serde(skip)]
    sample_yuv: Option<VideoFrame>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Sample {
    index: usize,
    pts: f64,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

#[derive(Clone, Default, PartialEq)]
pub(crate) struct Props {
    /// Site paths to check instead of picked files.
    pub sources: Vec<String>,
    /// `module` (also when empty), `webcodecs` or `both`.
    pub decoder: String,
}

/// The page's elements that change (it is plain DOM).
struct Dom {
    status: web_sys::HtmlElement,
    error: web_sys::HtmlElement,
    input: HtmlInputElement,
    table: web_sys::HtmlElement,
    frames: web_sys::HtmlElement,
    /// Each file's row of frames, by name.
    pairs: Vec<(String, web_sys::HtmlElement)>,
    _change: gloo::events::EventListener,
}

pub(crate) enum Msg {
    Picked(Vec<(String, Blob)>),
    Fetched(Result<Vec<(String, Blob)>, String>),
    Update(usize, Box<FileCheck>),
    Finished,
    Tick,
}

pub(crate) struct VideoCheck {
    dom: std::cell::RefCell<Option<Dom>>,
    worker: Option<VideoWorker>,
    files: Vec<FileCheck>,
    canvases: Vec<NodeRef>,
    drawn: Vec<bool>,
    running: bool,
    error: Option<String>,
    cancelled: Rc<Cell<bool>>,
    _tick: Interval,
}

impl Component for VideoCheck {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let sources = ctx.props().sources.clone();
        if !sources.is_empty() {
            ctx.link().send_future(async move {
                let mut files = Vec::new();
                for path in sources {
                    let bytes = match crate::songs::fetch_bytes(&path).await {
                        Ok(b) => b,
                        Err(e) => return Msg::Fetched(Err(e)),
                    };
                    match crate::web::files::bytes_blob(&bytes, "") {
                        Ok(blob) => files.push((path, blob)),
                        Err(e) => return Msg::Fetched(Err(e)),
                    }
                }
                Msg::Fetched(Ok(files))
            });
        }
        let link = ctx.link().clone();
        VideoCheck {
            worker: None,
            files: Vec::new(),
            canvases: Vec::new(),
            dom: std::cell::RefCell::new(None),
            drawn: Vec::new(),
            running: false,
            error: None,
            cancelled: Rc::new(Cell::new(false)),
            _tick: Interval::new(250, move || link.send_message(Msg::Tick)),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Picked(files) | Msg::Fetched(Ok(files)) => {
                if self.running || files.is_empty() {
                    return false;
                }
                let worker = match &self.worker {
                    Some(w) => w.clone(),
                    None => match VideoWorker::new() {
                        Ok(w) => {
                            self.worker = Some(w.clone());
                            w
                        }
                        Err(e) => {
                            self.error = Some(e);
                            self.publish();
                            return true;
                        }
                    },
                };
                let decoders: &[&str] = match ctx.props().decoder.as_str() {
                    "webcodecs" => &["webcodecs"],
                    "both" => &["module", "webcodecs"],
                    _ => &["module"],
                };
                let first = self.files.len();
                for (name, blob) in &files {
                    for decoder in decoders {
                        self.files.push(FileCheck {
                            name: name.clone(),
                            decoder: decoder.to_string(),
                            bytes: blob.size(),
                            state: "waiting".into(),
                            ..FileCheck::default()
                        });
                        self.canvases.push(NodeRef::default());
                        self.drawn.push(false);
                    }
                }
                self.running = true;
                let link = ctx.link().clone();
                let cancelled = Rc::clone(&self.cancelled);
                let decoders: Vec<&'static str> = decoders.to_vec();
                wasm_bindgen_futures::spawn_local(async move {
                    let mut row = first;
                    for (name, blob) in files {
                        let mut module_frames = None;
                        let mut module_yuv = None;
                        for &decoder in &decoders {
                            let report = {
                                let link = link.clone();
                                let at = row;
                                move |check: &FileCheck| {
                                    link.send_message(Msg::Update(at, Box::new(check.clone())))
                                }
                            };
                            let mut check = FileCheck {
                                name: name.clone(),
                                decoder: decoder.into(),
                                bytes: blob.size(),
                                state: "decoding".into(),
                                ..FileCheck::default()
                            };
                            let frames = if decoder == "webcodecs" {
                                let video = WebCodecsVideo::open(blob.clone());
                                check_file(video, &mut check, &report, &cancelled).await
                            } else {
                                // The worker probes the file itself; what
                                // the import would sniff does not matter.
                                let unknown = VideoFormat {
                                    container: VideoContainer::Unknown,
                                    codec: VideoCodec::Unknown,
                                };
                                let video = worker.clone().open(blob.clone(), &name, &unknown);
                                check_file(video, &mut check, &report, &cancelled).await
                            };
                            if cancelled.get() {
                                return;
                            }
                            match (decoder, &module_frames) {
                                ("module", _) => {
                                    module_yuv = check.sample_yuv.take();
                                    module_frames = Some(frames);
                                }
                                ("webcodecs", Some(expected)) => {
                                    check.compared = Some(compare(expected, &frames));
                                    let browser_colors =
                                        !frames.is_empty() && frames.iter().all(|f| f.1 == 0);
                                    if let (true, Some(m), Some(b)) =
                                        (browser_colors, &module_yuv, &check.sample)
                                    {
                                        check.colors = Some(color_match(m, b));
                                    }
                                    report(&check);
                                }
                                _ => {}
                            }
                            row += 1;
                        }
                    }
                    link.send_message(Msg::Finished);
                });
                self.publish();
                true
            }
            Msg::Fetched(Err(e)) => {
                self.error = Some(e);
                self.publish();
                true
            }
            Msg::Update(i, check) => {
                if let Some(slot) = self.files.get_mut(i) {
                    if check.sample != slot.sample {
                        self.drawn[i] = false;
                    }
                    *slot = *check;
                }
                self.publish();
                true
            }
            Msg::Finished => {
                self.running = false;
                self.publish();
                true
            }
            Msg::Tick => {
                if self.running {
                    self.publish();
                }
                self.running
            }
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, _first_render: bool) {
        for (i, check) in self.files.iter().enumerate() {
            if self.drawn[i] {
                continue;
            }
            let (Some(sample), Some(canvas)) =
                (&check.sample, self.canvases[i].cast::<HtmlCanvasElement>())
            else {
                continue;
            };
            canvas.set_width(sample.width);
            canvas.set_height(sample.height);
            let image = ImageData::new_with_u8_clamped_array_and_sh(
                Clamped(&sample.rgba),
                sample.width,
                sample.height,
            );
            let context = canvas
                .get_context("2d")
                .ok()
                .flatten()
                .and_then(|c| c.dyn_into::<CanvasRenderingContext2d>().ok());
            if let (Ok(image), Some(context)) = (image, context) {
                let _ = context.put_image_data(&image, 0.0, 0.0);
                self.drawn[i] = true;
            }
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.cancelled.set(true);
        if let Some(window) = web_sys::window() {
            let _ = js_sys::Reflect::delete_property(&window, &"__DDI_VIDEO_CHECK".into());
        }
    }

    fn view(&self, ctx: &Context<Self>) {
        let root = ctx.root();
        let mut dom = self.dom.borrow_mut();
        let dom = dom.get_or_insert_with(|| {
            root.set_class_name("shell video-check");
            element(root, "h1", "").set_text_content(Some("Video decoder check"));
            element(root, "p", "").set_text_content(Some(
                "Plays movies through the game's own video decoder in this browser, \
                 to see whether it works here and how fast it is. Pick one or more \
                 movie files from a pack (any format StepMania plays: AVI, MPEG, MP4, \
                 MOV, MKV, WebM, OGV, WMV, FLV); nothing is stored.",
            ));
            let agent = web_sys::window()
                .map(|w| w.navigator().user_agent().unwrap_or_default())
                .unwrap_or_default();
            element(root, "p", "muted").set_text_content(Some(&agent));
            let status = element(root, "p", "");
            let error = element(root, "p", "error");
            let holder = element(root, "p", "");
            let input: HtmlInputElement = element(&holder, "input", "").unchecked_into();
            input.set_type("file");
            input.set_multiple(true);
            input.set_accept(
                ".avi,.f4v,.flv,.m2v,.mkv,.mov,.mp4,.mpeg,.mpg,.ogv,.webm,.wmv,video/*",
            );
            let link = ctx.link().clone();
            let change = gloo::events::EventListener::new(&input, "change", move |e| {
                let Some(input) = e
                    .target()
                    .and_then(|t| t.dyn_into::<HtmlInputElement>().ok())
                else {
                    return;
                };
                let picked: Vec<(String, Blob)> = crate::web::files::from_input(&input)
                    .into_iter()
                    .map(|p| (p.path, p.file.into()))
                    .collect();
                input.set_value("");
                if !picked.is_empty() {
                    link.send_message(Msg::Picked(picked));
                }
            });
            let table = element(root, "table", "results-table video-check-table");
            let frames = element(root, "div", "");
            let back = element(&element(root, "p", ""), "a", "");
            let _ = back.set_attribute("href", &Route::Home.to_hash());
            back.set_text_content(Some("← back"));
            Dom {
                status,
                error,
                input,
                table,
                frames,
                pairs: Vec::new(),
                _change: change,
            }
        });
        dom.status.set_text_content(Some(&self.status_line()));
        match &self.error {
            Some(e) => {
                dom.error.set_hidden(false);
                dom.error.set_text_content(Some(e));
            }
            None => dom.error.set_hidden(true),
        }
        dom.input.set_disabled(self.running);
        dom.table.set_inner_html("");
        dom.table.set_hidden(self.files.is_empty());
        if !self.files.is_empty() {
            let head = element(&dom.table, "tr", "");
            for h in ["file", "movie", "frames", "decode", "seeks", "state"] {
                element(&head, "th", "").set_text_content(Some(h));
            }
            for f in &self.files {
                row(&dom.table, f);
            }
        }
        // Each file's sampled frames, side by side; a frame's canvas is
        // made once and drawn once (`rendered`).
        for (i, f) in self.files.iter().enumerate() {
            let Some(sample) = &f.sample else { continue };
            if self.canvases[i].get().is_some() {
                continue;
            }
            let pair = match dom.pairs.iter().find(|(n, _)| *n == f.name) {
                Some((_, p)) => p.clone(),
                None => {
                    let p = element(&dom.frames, "div", "video-check-pair");
                    dom.pairs.push((f.name.clone(), p.clone()));
                    p
                }
            };
            let figure = element(&pair, "figure", "video-check-frame");
            let canvas = element(&figure, "canvas", "");
            self.canvases[i].set(&canvas);
            element(&figure, "figcaption", "muted").set_text_content(Some(&format!(
                "{} · {}: frame {} at {:.3} s",
                f.name, f.decoder, sample.index, sample.pts
            )));
        }
    }
}

impl VideoCheck {
    fn status_line(&self) -> String {
        match self.worker.as_ref().map(VideoWorker::status) {
            None | Some(ModuleStatus::Idle) => {
                "The decoder loads when the first movie opens.".into()
            }
            Some(ModuleStatus::Loading { loaded, total }) if total > 0.0 => format!(
                "Loading the video decoder… {:.0} %",
                (loaded / total * 100.0).min(100.0)
            ),
            Some(ModuleStatus::Loading { .. }) => "Loading the video decoder…".into(),
            Some(ModuleStatus::Ready { load_ms }) => {
                format!("Video decoder loaded in {load_ms:.0} ms.")
            }
            Some(ModuleStatus::Failed(e)) => format!("The video decoder did not load: {e}."),
        }
    }

    /// Mirrors the results to `window.__DDI_VIDEO_CHECK` for headless tests.
    fn publish(&self) {
        #[derive(Serialize)]
        struct State<'a> {
            running: bool,
            status: String,
            error: &'a Option<String>,
            files: &'a [FileCheck],
        }
        let state = State {
            running: self.running,
            status: self.status_line(),
            error: &self.error,
            files: &self.files,
        };
        let (Some(window), Ok(json)) = (web_sys::window(), serde_json::to_string(&state)) else {
            return;
        };
        if let Ok(value) = js_sys::JSON::parse(&json) {
            let _ = js_sys::Reflect::set(&window, &"__DDI_VIDEO_CHECK".into(), &value);
        }
    }
}

/// A file's rows in `table`.
fn row(table: &web_sys::HtmlElement, f: &FileCheck) {
    let movie = if f.codec.is_empty() {
        format!("{:.1} MB", f.bytes / 1e6)
    } else {
        format!(
            "{} {}×{}, {} {}",
            f.codec,
            f.width,
            f.height,
            f.matrix,
            if f.full_range { "full" } else { "limited" }
        )
    };
    let frames = match (f.first_pts, f.last_pts) {
        (Some(a), Some(b)) => format!("{} ({a:.3}–{b:.3} s)", f.frames),
        _ => f.frames.to_string(),
    };
    let decode = if f.frames > 0 {
        format!(
            "{:.2} ms/frame ({:.2} with hand-over)",
            f.decode_ms_per_frame, f.wall_ms_per_frame
        )
    } else {
        String::new()
    };
    let seeks = if f.seeks > 0 {
        format!(
            "{}/{} exact, slowest {:.0} ms",
            f.seeks_ok, f.seeks, f.slowest_seek_ms
        )
    } else {
        String::new()
    };
    let tr = element(table, "tr", "");
    for text in [
        format!("{} · {}", f.name, f.decoder),
        movie,
        frames,
        decode,
        seeks,
    ] {
        element(&tr, "td", "").set_text_content(Some(&text));
    }
    let state = element(&tr, "td", "");
    match &f.error {
        Some(e) => element(&state, "span", "error").set_text_content(Some(e)),
        None => state.set_text_content(Some(&f.state)),
    }
    let note = |text: String, class: &str| {
        let tr = element(table, "tr", "");
        element(&tr, "td", "");
        let td = element(&tr, "td", class);
        let _ = td.set_attribute("colspan", "5");
        td.set_text_content(Some(&text));
    };
    if let Some(c) = &f.colors {
        note(format!("browser colors: {c}"), "");
    }
    if let Some(c) = &f.compared {
        note(
            format!("against the module: {c}"),
            if c.starts_with("same") { "" } else { "error" },
        );
    }
    for s in &f.seek_failures {
        note(s.clone(), "error");
    }
}

/// A frame's fingerprint; 0 for one in the browser's colors (no planes).
fn print_of(frame: &VideoFrame) -> u64 {
    if frame.external.is_some() {
        0
    } else {
        ddi_library::pack::fnv1a64(&frame.planes)
    }
}

/// FNV-1a over the planes: enough to tell two decodes of a frame apart.
fn sample(index: usize, frame: &VideoFrame) -> Option<Sample> {
    let (y, u, v) = frame.split()?;
    let (w, h) = (frame.width as usize, frame.height as usize);
    let cw = w.div_ceil(2);
    let mut rgba = Vec::with_capacity(w * h * 4);
    for row in 0..h {
        for col in 0..w {
            let c = (row / 2) * cw + col / 2;
            let rgb = ddi_platform::video::yuv_to_rgb(
                y[row * w + col],
                u[c],
                v[c],
                frame.matrix,
                frame.full_range,
            );
            rgba.extend(rgb.map(|x| (x * 255.0).round() as u8));
            rgba.push(255);
        }
    }
    Some(Sample {
        index,
        pts: frame.pts,
        width: frame.width,
        height: frame.height,
        rgba,
    })
}

/// Waits for the next frame of `video` (after a seek), or the end.
async fn next_frame(
    video: &mut impl VideoDecoder,
    events: &mut Vec<VideoEvent>,
) -> Result<Option<VideoFrame>, String> {
    let start = PerformanceClock::now_ms();
    loop {
        video.poll(events);
        for event in events.drain(..) {
            match event {
                VideoEvent::Frame(f) => return Ok(Some(f)),
                VideoEvent::End { .. } => return Ok(None),
                VideoEvent::Error(e) => return Err(e),
                VideoEvent::Opened(_) => {}
            }
        }
        if PerformanceClock::now_ms() - start > TIMEOUT_MS {
            return Err("no frame after 30 s".into());
        }
        TimeoutFuture::new(1).await;
    }
}

/// Whether the browser's frames are the module's: the same times and the
/// same pixels, frame for frame.
fn compare(module: &[(f64, u64)], browser: &[(f64, u64)]) -> String {
    // Frames in the browser's colors have no planes to compare (print 0).
    let colors = !browser.is_empty() && browser.iter().all(|b| b.1 == 0);
    for (i, (m, b)) in module.iter().zip(browser).enumerate() {
        if (m.0 - b.0).abs() > 1e-6 {
            return format!("frame {i} at {:.4} s, the module's at {:.4} s", b.0, m.0);
        }
        if m.1 != b.1 && !colors {
            return format!("frame {i} ({:.4} s) has other pixels", m.0);
        }
    }
    if module.len() != browser.len() {
        return format!("{} frames, the module gave {}", browser.len(), module.len());
    }
    if colors {
        "same times (the pixels are in the browser's colors: see the frames)".into()
    } else {
        "same".into()
    }
}

/// How close the browser's frame (`browser`, RGBA as drawn) is to the
/// module's frame converted under each matrix and range, closest first,
/// as mean differences in 1/255.
fn color_match(module: &VideoFrame, browser: &Sample) -> String {
    use ddi_platform::video::{YuvMatrix, yuv_to_rgb};
    if (module.width, module.height) != (browser.width, browser.height) {
        return format!(
            "the browser's frame is {}×{}, the module's {}×{}",
            browser.width, browser.height, module.width, module.height
        );
    }
    let Some((y, u, v)) = module.split() else {
        return "no module frame to compare".into();
    };
    let (w, h) = (module.width as usize, module.height as usize);
    let cw = w.div_ceil(2);
    let mut scores = Vec::new();
    for (matrix, matrix_name) in [(YuvMatrix::Bt601, "BT.601"), (YuvMatrix::Bt709, "BT.709")] {
        for (full, range) in [(false, "limited"), (true, "full")] {
            let mut sum = 0.0;
            for row in 0..h {
                for col in 0..w {
                    let c = (row / 2) * cw + col / 2;
                    let rgb = yuv_to_rgb(y[row * w + col], u[c], v[c], matrix, full);
                    let at = 4 * (row * w + col);
                    for (k, c) in rgb.iter().enumerate() {
                        sum += (c * 255.0 - f32::from(browser.rgba[at + k])).abs() as f64;
                    }
                }
            }
            scores.push((sum / (3 * w * h) as f64, format!("{matrix_name} {range}")));
        }
    }
    scores.sort_by(|a, b| a.0.total_cmp(&b.0));
    let used = format!(
        "{} {}",
        if module.matrix == YuvMatrix::Bt709 {
            "BT.709"
        } else {
            "BT.601"
        },
        if module.full_range { "full" } else { "limited" }
    );
    let list: Vec<String> = scores
        .iter()
        .map(|(d, name)| format!("{name} {d:.1}"))
        .collect();
    format!(
        "closest to {} (mean difference of 255: {}); the module uses {used}",
        scores[0].1,
        list.join(", ")
    )
}

/// A browser frame on the direct path drawn as the browser draws it, for
/// the side by side.
fn sample_external(index: usize, frame: &VideoFrame) -> Option<Sample> {
    let external = frame.external.as_ref()?;
    let direct = external
        .0
        .downcast_ref::<crate::web::webcodecs::DirectFrame>()?;
    let document = web_sys::window()?.document()?;
    let canvas: HtmlCanvasElement = document.create_element("canvas").ok()?.dyn_into().ok()?;
    canvas.set_width(frame.width);
    canvas.set_height(frame.height);
    let context: CanvasRenderingContext2d = canvas.get_context("2d").ok()??.dyn_into().ok()?;
    // `drawImage(bitmap, 0, 0)`.
    let draw: js_sys::Function = js_sys::Reflect::get(&context, &"drawImage".into())
        .ok()?
        .dyn_into()
        .ok()?;
    draw.call3(&context, direct.bitmap(), &0.into(), &0.into())
        .ok()?;
    let image = context
        .get_image_data(0.0, 0.0, f64::from(frame.width), f64::from(frame.height))
        .ok()?;
    Some(Sample {
        index,
        pts: frame.pts,
        width: frame.width,
        height: frame.height,
        rgba: image.data().0,
    })
}

/// Decodes a movie straight through and seeks in it; the frames' times and
/// fingerprints, for comparing decoders.
async fn check_file(
    mut video: impl VideoDecoder,
    check: &mut FileCheck,
    report: &impl Fn(&FileCheck),
    cancelled: &Cell<bool>,
) -> Vec<(f64, u64)> {
    let check = &mut *check;
    report(check);
    let mut events = Vec::new();
    // Straight through: every frame's time and fingerprint.
    let mut frames: Vec<(f64, u64)> = Vec::new();
    let mut opened_at = None;
    let mut decode_ms = 0.0;
    let mut last_report = PerformanceClock::now_ms();
    let mut last_event = last_report;
    'decode: loop {
        if cancelled.get() {
            return frames;
        }
        if video.pending() < BATCH {
            video.want(BATCH);
        }
        TimeoutFuture::new(1).await;
        video.poll(&mut events);
        let now = PerformanceClock::now_ms();
        if !events.is_empty() {
            last_event = now;
        } else if now - last_event > TIMEOUT_MS {
            check.state = "failed".into();
            check.error = Some("the decoder stopped answering".into());
            report(check);
            return frames;
        }
        for event in events.drain(..) {
            match event {
                VideoEvent::Opened(info) => {
                    opened_at = Some(PerformanceClock::now_ms());
                    check.codec = info.codec;
                    check.width = info.width;
                    check.height = info.height;
                    check.duration = info.duration;
                }
                VideoEvent::Frame(frame) => {
                    let index = frames.len();
                    if index == 0 || index == SAMPLE_FRAME || index == LATE_SAMPLE_FRAME {
                        check.sample = if frame.external.is_some() {
                            sample_external(index, &frame)
                        } else {
                            check.sample_yuv = Some(frame.clone());
                            sample(index, &frame)
                        };
                    }
                    if index == 0 {
                        check.first_pts = Some(frame.pts);
                        check.matrix = match frame.matrix {
                            YuvMatrix::Bt601 => "BT.601".into(),
                            YuvMatrix::Bt709 => "BT.709".into(),
                        };
                        check.full_range = frame.full_range;
                    }
                    check.last_pts = Some(frame.pts);
                    decode_ms += frame.decode_ms;
                    frames.push((frame.pts, print_of(&frame)));
                }
                VideoEvent::End { .. } => break 'decode,
                VideoEvent::Error(e) => {
                    check.state = "failed".into();
                    check.error = Some(e);
                    report(check);
                    return frames;
                }
            }
        }
        let now = PerformanceClock::now_ms();
        check.frames = frames.len();
        if let (Some(t0), true) = (opened_at, !frames.is_empty()) {
            check.decode_ms_per_frame = decode_ms / frames.len() as f64;
            check.wall_ms_per_frame = (now - t0) / frames.len() as f64;
        }
        if now - last_report > 250.0 {
            last_report = now;
            report(check);
        }
    }
    check.frames = frames.len();
    if let Some(t0) = opened_at.filter(|_| !frames.is_empty()) {
        check.decode_ms_per_frame = decode_ms / frames.len() as f64;
        check.wall_ms_per_frame = (PerformanceClock::now_ms() - t0) / frames.len() as f64;
    }
    if frames.is_empty() {
        check.state = "failed".into();
        check.error = Some("no frame decoded".into());
        report(check);
        return frames;
    }

    // Seeks: to frames' own times and to the middle of their periods, the
    // first and the last frame, then past the end.
    check.state = "seeking".into();
    report(check);
    let n = frames.len();
    let step = (n / SEEK_TARGETS).max(1);
    let mut targets = vec![(0.0, 0), (frames[n - 1].0, n - 1)];
    for i in (0..n).step_by(step) {
        targets.push((frames[i].0, i));
        if i + 1 < n {
            targets.push(((frames[i].0 + frames[i + 1].0) / 2.0, i));
        }
    }
    // Into every gap (dropped frames hold the one before), wherever it is.
    let mut periods: Vec<f64> = frames.windows(2).map(|w| w[1].0 - w[0].0).collect();
    periods.sort_by(f64::total_cmp);
    if let Some(&period) = periods.get(periods.len() / 2) {
        for i in 0..n - 1 {
            if frames[i + 1].0 - frames[i].0 > 1.5 * period {
                targets.push(((frames[i].0 + frames[i + 1].0) / 2.0, i));
            }
        }
    }
    for (t, expect) in targets {
        if cancelled.get() {
            return frames;
        }
        let start = PerformanceClock::now_ms();
        video.seek(t);
        video.want(1);
        let got = next_frame(&mut video, &mut events).await;
        check.slowest_seek_ms = check
            .slowest_seek_ms
            .max(PerformanceClock::now_ms() - start);
        check.seeks += 1;
        let (pts, print) = frames[expect];
        match got {
            Ok(Some(f)) if (f.pts - pts).abs() < 1e-6 && print_of(&f) == print => {
                check.seeks_ok += 1
            }
            Ok(Some(f)) if (f.pts - pts).abs() < 1e-6 => check.seek_failures.push(format!(
                "seek to {t:.4} s: frame {expect} with different pixels"
            )),
            Ok(Some(f)) => check.seek_failures.push(format!(
                "seek to {t:.4} s: the frame at {:.4} s, expected frame {expect} at {pts:.4} s",
                f.pts
            )),
            Ok(None) => check.seek_failures.push(format!(
                "seek to {t:.4} s: the end, expected frame {expect}"
            )),
            Err(e) => {
                // The movie is closed after an error; later seeks would
                // only time out.
                check.seek_failures.push(format!("seek to {t:.4} s: {e}"));
                check.state = "failed".into();
                report(check);
                return frames;
            }
        }
    }
    video.seek(frames[n - 1].0 + 10.0);
    video.want(1);
    check.seeks += 1;
    match next_frame(&mut video, &mut events).await {
        Ok(None) => check.seeks_ok += 1,
        Ok(Some(f)) => check
            .seek_failures
            .push(format!("seek past the end: a frame at {:.4} s", f.pts)),
        Err(e) => check.seek_failures.push(format!("seek past the end: {e}")),
    }
    check.state = "done".into();
    report(check);
    frames
}
