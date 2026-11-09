//! The frame recorder: `#/?perf=1` (or `perf=1` on any song select or play
//! address) records every frame of the plays of this tab, for finding what
//! a stutter on a player's own machine was made of; `#/?perf=0` stops it.
//!
//! Each play's end says on the console how many frames were late and how
//! many bursts of them there were. `__DDI_PERF.save()` then downloads the
//! last few plays as JSON: a row of times a frame ([`COLUMNS`], milliseconds
//! but for the song's second), and when the main thread answered a timer
//! late, which tells a thread kept busy between frames from a browser
//! that did not ask for one.
//!
//! Off, each call here reads one thread-local. Firefox rounds its clock to
//! a millisecond without cross-origin isolation, so there the columns of a
//! single frame are that coarse; their sums over frames are not.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use gloo::timers::callback::Interval;
use wasm_bindgen::prelude::*;

use crate::web::clock::PerformanceClock;

/// The tab's switch, read when a play begins.
const STORAGE_KEY: &str = "ddi.perf";
/// The plays kept for [`save`].
const PLAYS_KEPT: usize = 4;
/// How often the main thread is asked to answer, milliseconds.
const HEARTBEAT_MS: u32 = 20;
/// An answer this much later than asked is written down, milliseconds.
const HEARTBEAT_LATE_MS: f64 = 8.0;

/// A part of a frame, timed from the end of the one before it.
#[derive(Clone, Copy)]
pub(crate) enum Lap {
    /// Decoded backgrounds going up to the GPU.
    Upload,
    /// The session's tick: input, judging, the audio clock.
    Tick,
    /// Getting the canvas's texture.
    Surface,
    /// Background movies.
    Movies,
    /// What is built before the field: generated background, skin, scripts.
    Scene,
    /// The dancing hand: its preparation and its picture.
    Hand,
    /// The field's passes recorded.
    Render,
    /// What is drawn over the play, and the commands finished and submitted.
    Submit,
    /// The texture handed back to the browser, and the debug state.
    Present,
}

const LAPS: usize = 9;
/// A frame's row after the time it was asked for.
const COLUMNS: [&str; 16] = [
    "song_s",
    "late_start",
    "upload",
    "tick",
    "surface",
    "movies",
    "scene",
    "hand",
    "render",
    "submit",
    "present",
    "total",
    "hand_prepare",
    "pads",
    "pad_polls",
    "gpu_done",
];
const SONG: usize = 0;
const LATE_START: usize = 1;
const FIRST_LAP: usize = 2;
const TOTAL: usize = FIRST_LAP + LAPS;
const HAND_PREPARE: usize = TOTAL + 1;
const PADS: usize = TOTAL + 2;
const PAD_POLLS: usize = TOTAL + 3;
const GPU_DONE: usize = TOTAL + 4;

type Row = [f32; COLUMNS.len()];

/// One play's frames.
struct Play {
    /// What it was played on, as JSON members.
    meta: String,
    /// When each frame was asked for (the animation frame's time).
    at: Vec<f64>,
    rows: Vec<Row>,
    /// When the main thread answered late, and by how much.
    late_answers: Vec<(f64, f32)>,
}

struct Recording {
    at: Vec<f64>,
    rows: Vec<Row>,
    row: Row,
    asked: f64,
    began: f64,
    lap_began: f64,
    pads_ms: f32,
    pad_polls: f32,
    late_answers: Rc<RefCell<Vec<(f64, f32)>>>,
    _heartbeat: Interval,
}

impl Recording {
    fn new() -> Self {
        let late_answers: Rc<RefCell<Vec<(f64, f32)>>> = Rc::default();
        let answers = late_answers.clone();
        let mut last = PerformanceClock::now_ms();
        let heartbeat = Interval::new(HEARTBEAT_MS, move || {
            let now = PerformanceClock::now_ms();
            let late = now - last - f64::from(HEARTBEAT_MS);
            last = now;
            if late > HEARTBEAT_LATE_MS {
                answers.borrow_mut().push((now, late as f32));
            }
        });
        Self {
            at: Vec::with_capacity(1 << 15),
            rows: Vec::with_capacity(1 << 15),
            row: [0.0; COLUMNS.len()],
            asked: 0.0,
            began: 0.0,
            lap_began: 0.0,
            pads_ms: 0.0,
            pad_polls: 0.0,
            late_answers,
            _heartbeat: heartbeat,
        }
    }

    fn finish(self, meta: String, period: f64) -> Play {
        let mut rows = self.rows;
        for (row, spent) in GPU_SPENT.take() {
            if let Some(r) = rows.get_mut(row) {
                r[GPU_DONE] = spent;
            }
        }
        let seconds: Vec<f64> = self.at.iter().map(|t| t / 1000.0).collect();
        let (late, bursts) = ddi_session::stutter::bursts(&seconds, period);
        let worst = bursts
            .iter()
            .map(|b| format!("{:.1} s ({} late)", rows[b.first][SONG], b.late))
            .collect::<Vec<_>>()
            .join(", ");
        web_sys::console::info_1(
            &format!(
                "perf: {} frames, {late} late, {} bursts{}{worst}. __DDI_PERF.save() downloads the last plays.",
                rows.len(),
                bursts.len(),
                if bursts.is_empty() { "" } else { " at " },
            )
            .into(),
        );
        Play {
            meta,
            at: self.at,
            rows,
            late_answers: self.late_answers.take(),
        }
    }
}

thread_local! {
    static RECORDING: RefCell<Option<Recording>> = const { RefCell::new(None) };
    static PLAYS: RefCell<VecDeque<Play>> = const { RefCell::new(VecDeque::new()) };
    /// How long the GPU took on the frames whose work is done, by row
    /// (here, as a callback for wgpu must be `Send`).
    static GPU_SPENT: RefCell<Vec<(usize, f32)>> = const { RefCell::new(Vec::new()) };
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.session_storage().ok()?
}

/// An address's `perf` parameter, kept for the tab.
pub(crate) fn note_address(query: &str) {
    for kv in query.split('&') {
        if let Some(("perf", value)) = kv.split_once('=')
            && let Some(s) = storage()
        {
            let _ = if value == "1" {
                s.set_item(STORAGE_KEY, "1")
            } else {
                s.remove_item(STORAGE_KEY)
            };
        }
    }
}

/// A play starts: records it if the tab asked to. What was recording ends
/// unsaid (a play restarted at once is ended first, [`end_play`]).
pub(crate) fn begin_play() {
    let wanted = storage()
        .and_then(|s| s.get_item(STORAGE_KEY).ok().flatten())
        .is_some();
    if wanted {
        install_save();
    }
    GPU_SPENT.take();
    RECORDING.with_borrow_mut(|r| *r = wanted.then(Recording::new));
}

/// The play is over: says what its frames did and keeps them. `meta` (JSON
/// members) and `period` (seconds a refresh) are asked for only if recording.
pub(crate) fn end_play(meta: impl FnOnce() -> (String, f64)) {
    let Some(recording) = RECORDING.with_borrow_mut(Option::take) else {
        return;
    };
    let (meta, period) = meta();
    let play = recording.finish(meta, period);
    PLAYS.with_borrow_mut(|plays| {
        if plays.len() == PLAYS_KEPT {
            plays.pop_front();
        }
        plays.push_back(play);
    });
}

fn with(f: impl FnOnce(&mut Recording)) {
    RECORDING.with_borrow_mut(|r| {
        if let Some(r) = r {
            f(r);
        }
    });
}

/// A frame begins, the one the browser asked for at `asked_ms`.
pub(crate) fn frame_start(asked_ms: f64) {
    with(|r| {
        let now = PerformanceClock::now_ms();
        r.asked = asked_ms;
        r.row = [0.0; COLUMNS.len()];
        r.row[LATE_START] = (now - asked_ms) as f32;
        r.began = now;
        r.lap_began = now;
    });
}

/// The part `lap` of the frame ended now.
pub(crate) fn lap(lap: Lap) {
    with(|r| {
        let now = PerformanceClock::now_ms();
        r.row[FIRST_LAP + lap as usize] += (now - r.lap_began) as f32;
        r.lap_began = now;
    });
}

/// The frame's commands were submitted to `queue`: the time until the GPU
/// is done with them goes in its row. WebGPU only: WebGL2's never ends
/// without the device being polled.
pub(crate) fn submitted(queue: &wgpu::Queue, webgpu: bool) {
    if !webgpu {
        return;
    }
    with(|r| {
        let row = r.rows.len();
        let submitted = PerformanceClock::now_ms();
        queue.on_submitted_work_done(move || {
            let spent = PerformanceClock::now_ms() - submitted;
            GPU_SPENT.with_borrow_mut(|g| g.push((row, spent as f32)));
        });
    });
}

/// The frame ended, showing song second `song`, the hand having prepared
/// for `hand_prepare_ms`.
pub(crate) fn frame_end(song: f64, hand_prepare_ms: f64) {
    with(|r| {
        let now = PerformanceClock::now_ms();
        r.row[SONG] = song as f32;
        r.row[TOTAL] = (now - r.began) as f32;
        r.row[HAND_PREPARE] = hand_prepare_ms as f32;
        r.row[PADS] = std::mem::take(&mut r.pads_ms);
        r.row[PAD_POLLS] = std::mem::take(&mut r.pad_polls);
        r.at.push(r.asked);
        r.rows.push(r.row);
    });
}

/// Whether the pads' polls are being timed ([`pad_poll`]).
pub(crate) fn recording() -> bool {
    RECORDING.with_borrow(Option::is_some)
}

/// A poll of the pads took `ms`.
pub(crate) fn pad_poll(ms: f64) {
    with(|r| {
        r.pads_ms += ms as f32;
        r.pad_polls += 1.0;
    });
}

fn json() -> String {
    use std::fmt::Write;
    let columns = COLUMNS.map(|c| format!("\"{c}\"")).join(",");
    PLAYS.with_borrow(|plays| {
        let mut out = format!("{{\"columns\":[\"asked_ms\",{columns}],\"plays\":[");
        for (p, play) in plays.iter().enumerate() {
            if p > 0 {
                out.push(',');
            }
            let _ = write!(out, "{{{},\"frames\":[", play.meta);
            for (i, (at, row)) in play.at.iter().zip(&play.rows).enumerate() {
                if i > 0 {
                    out.push(',');
                }
                let _ = write!(out, "[{at:.2}");
                for v in row {
                    // JSON has no NaN: a frame without a play has no second.
                    let _ = if v.is_finite() {
                        write!(out, ",{v:.2}")
                    } else {
                        write!(out, ",null")
                    };
                }
                out.push(']');
            }
            out.push_str("],\"late_answers\":[");
            for (i, (at, late)) in play.late_answers.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                let _ = write!(out, "[{at:.2},{late:.2}]");
            }
            out.push_str("]}");
        }
        out.push_str("]}");
        out
    })
}

fn save() {
    let text = json();
    let Ok(blob) = crate::web::files::bytes_blob(text.as_bytes(), "application/json") else {
        return;
    };
    let _ = crate::web::files::save_blob(&blob, "ddi-perf.json");
}

/// `window.__DDI_PERF`, once.
fn install_save() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let key = JsValue::from_str("__DDI_PERF");
    if js_sys::Reflect::has(&window, &key).unwrap_or(false) {
        return;
    }
    let object = js_sys::Object::new();
    let call = Closure::<dyn Fn()>::new(save);
    let json_call = Closure::<dyn Fn() -> String>::new(json);
    let _ = js_sys::Reflect::set(&object, &"save".into(), call.as_ref());
    let _ = js_sys::Reflect::set(&object, &"json".into(), json_call.as_ref());
    let _ = js_sys::Reflect::set(&window, &key, &object);
    call.forget();
    json_call.forget();
}
