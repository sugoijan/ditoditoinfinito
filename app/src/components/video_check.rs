//! `#/video-check`: runs the software video decoder (the FFmpeg module in
//! its worker) on movies the user picks, to measure it in this browser and
//! check the worker protocol: frames straight through, time per frame, and
//! seeks that must land on the frames decoding straight through gave. Not
//! linked from the menus; `docs/plans/video-backgrounds.md`, step 2.
//! `?src=a,b` checks files of the site instead (headless tests), and the
//! results are mirrored to `window.__DDI_VIDEO_CHECK`.

use std::cell::Cell;
use std::rc::Rc;

use ddi_platform::video::{
    VideoBackend, VideoCodec, VideoDecoder, VideoEvent, VideoFrame, YuvMatrix,
};
use gloo::timers::callback::Interval;
use gloo::timers::future::TimeoutFuture;
use serde::Serialize;
use wasm_bindgen::{Clamped, JsCast};
use web_sys::{Blob, CanvasRenderingContext2d, HtmlCanvasElement, HtmlInputElement, ImageData};
use yew::prelude::*;

use crate::router::Route;
use crate::web::clock::PerformanceClock;
use crate::web::video::{ModuleStatus, VideoWorker};

/// Frames asked for at a time, and how few may be outstanding before more
/// are asked for.
const BATCH: u32 = 8;
/// The frame shown for each file.
const SAMPLE_FRAME: usize = 30;
/// About this many seek targets per file.
const SEEK_TARGETS: usize = 24;
/// A seek that has not delivered a frame after this long fails, and so does
/// a file the decoder has said nothing about for this long.
const TIMEOUT_MS: f64 = 30_000.0;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub(crate) struct FileCheck {
    name: String,
    bytes: f64,
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
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Sample {
    index: usize,
    pts: f64,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

#[derive(Clone, PartialEq, Properties)]
pub(crate) struct Props {
    /// Site paths to check instead of picked files.
    #[prop_or_default]
    pub sources: Vec<String>,
}

pub(crate) enum Msg {
    Picked(Vec<(String, Blob)>),
    Fetched(Result<Vec<(String, Blob)>, String>),
    Update(usize, FileCheck),
    Finished,
    Tick,
}

pub(crate) struct VideoCheck {
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
                let first = self.files.len();
                for (name, blob) in &files {
                    self.files.push(FileCheck {
                        name: name.clone(),
                        bytes: blob.size(),
                        state: "waiting".into(),
                        ..FileCheck::default()
                    });
                    self.canvases.push(NodeRef::default());
                    self.drawn.push(false);
                }
                self.running = true;
                let link = ctx.link().clone();
                let cancelled = Rc::clone(&self.cancelled);
                wasm_bindgen_futures::spawn_local(async move {
                    for (i, (name, blob)) in files.into_iter().enumerate() {
                        let report = {
                            let link = link.clone();
                            move |check: &FileCheck| {
                                link.send_message(Msg::Update(first + i, check.clone()))
                            }
                        };
                        check_file(worker.clone(), blob, name, &report, &cancelled).await;
                        if cancelled.get() {
                            return;
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
                    *slot = check;
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

    fn view(&self, ctx: &Context<Self>) -> Html {
        let onchange = ctx.link().batch_callback(|e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            let picked: Vec<(String, Blob)> = crate::web::files::from_input(&input)
                .into_iter()
                .map(|p| (p.path, p.file.into()))
                .collect();
            input.set_value("");
            (!picked.is_empty()).then_some(Msg::Picked(picked))
        });
        let agent = web_sys::window()
            .map(|w| w.navigator().user_agent().unwrap_or_default())
            .unwrap_or_default();
        html! {
            <main class="shell video-check">
                <h1>{ "Video decoder check" }</h1>
                <p>
                    { "Plays movies through the game's own video decoder in this browser, \
                       to see whether it works here and how fast it is. Pick one or more \
                       movie files from a pack (any format StepMania plays: AVI, MPEG, MP4, \
                       MOV, MKV, WebM, OGV, WMV, FLV); nothing is stored." }
                </p>
                <p class="muted">{ agent }</p>
                <p>{ self.status_line() }</p>
                if let Some(e) = &self.error {
                    <p class="error">{ e }</p>
                }
                <p>
                    <input type="file" multiple=true accept=".avi,.f4v,.flv,.m2v,.mkv,.mov,.mp4,.mpeg,.mpg,.ogv,.webm,.wmv,video/*"
                        disabled={self.running} {onchange} />
                </p>
                if !self.files.is_empty() {
                    <table class="results-table video-check-table">
                        <tr>
                            <th>{ "file" }</th><th>{ "movie" }</th><th>{ "frames" }</th>
                            <th>{ "decode" }</th><th>{ "seeks" }</th><th>{ "state" }</th>
                        </tr>
                        { for self.files.iter().map(row) }
                    </table>
                    { for self.files.iter().zip(&self.canvases).map(|(f, canvas)| html! {
                        if let Some(s) = &f.sample {
                            <figure class="video-check-frame">
                                <canvas ref={canvas.clone()} />
                                <figcaption class="muted">
                                    { format!("{}: frame {} at {:.3} s", f.name, s.index, s.pts) }
                                </figcaption>
                            </figure>
                        }
                    }) }
                }
                <p><a href={Route::Home.to_hash()}>{ "← back" }</a></p>
            </main>
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

fn row(f: &FileCheck) -> Html {
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
    let state = match &f.error {
        Some(e) => html! { <span class="error">{ e }</span> },
        None => html! { { &f.state } },
    };
    html! {
        <>
            <tr>
                <td>{ &f.name }</td><td>{ movie }</td><td>{ frames }</td>
                <td>{ decode }</td><td>{ seeks }</td><td>{ state }</td>
            </tr>
            { for f.seek_failures.iter().map(|s| html! {
                <tr><td></td><td colspan="5" class="error">{ s }</td></tr>
            }) }
        </>
    }
}

/// FNV-1a over the planes: enough to tell two decodes of a frame apart.
fn fingerprint(planes: &[u8]) -> u64 {
    planes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

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

async fn check_file(
    mut worker: VideoWorker,
    blob: Blob,
    name: String,
    report: &impl Fn(&FileCheck),
    cancelled: &Cell<bool>,
) {
    let mut check = FileCheck {
        name: name.clone(),
        bytes: blob.size(),
        state: "decoding".into(),
        ..FileCheck::default()
    };
    report(&check);
    let mut video = worker.open(blob, &name, &VideoCodec::Other(String::new()));
    let mut events = Vec::new();
    // Straight through: every frame's time and fingerprint.
    let mut frames: Vec<(f64, u64)> = Vec::new();
    let mut opened_at = None;
    let mut decode_ms = 0.0;
    let mut last_report = PerformanceClock::now_ms();
    let mut last_event = last_report;
    'decode: loop {
        if cancelled.get() {
            return;
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
            report(&check);
            return;
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
                    if index == 0 || index == SAMPLE_FRAME {
                        check.sample = sample(index, &frame);
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
                    frames.push((frame.pts, fingerprint(&frame.planes)));
                }
                VideoEvent::End { .. } => break 'decode,
                VideoEvent::Error(e) => {
                    check.state = "failed".into();
                    check.error = Some(e);
                    report(&check);
                    return;
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
            report(&check);
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
        report(&check);
        return;
    }

    // Seeks: to frames' own times and to the middle of their periods, the
    // first and the last frame, then past the end.
    check.state = "seeking".into();
    report(&check);
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
            return;
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
            Ok(Some(f)) if (f.pts - pts).abs() < 1e-6 && fingerprint(&f.planes) == print => {
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
                report(&check);
                return;
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
    report(&check);
}
