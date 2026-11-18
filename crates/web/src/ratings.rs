//! Chart ratings (MSD) made in the background on the web
//! (`ddi_ui::state::ratings`, `docs/plans/msd-ratings.md`): an imported
//! song's simfile read from the library (`crate::songs::chart_text`) and
//! handed to the rating worker (`crates/msd-worker`, built by Trunk beside
//! the app, its files named by their hash: `xtask stage-msd`, index.html's
//! `ddi-msd-worker` meta), which parses and rates it off the page's
//! thread. One song at a time, only while an option wants ratings
//! ([`kick`], at boot and on every route), none started while a play runs
//! (`crate::history::set_playing`); the worker is started with the first
//! song and ended once nothing has waited for [`IDLE_MS`].
//!
//! `__DDI_DEBUG.ratings` says where it stands ([`debug_into`]).

use std::cell::RefCell;

use ddi_ui::state::ratings::{self, Job, Rated};
use js_sys::{Array, Object, Reflect, WebAssembly};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{Event, MessageEvent, Worker};

use crate::web::js_err;

/// How often a song is looked for while none is being rated, ms.
const TICK_MS: u32 = 400;

/// How long the worker stays once nothing waits, ms.
const IDLE_MS: f64 = 10_000.0;

/// How long the worker may take to say it is ready, ms.
const READY_MS: u32 = 25_000;

struct Running {
    worker: Worker,
    ready: bool,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error: Closure<dyn FnMut(Event)>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.worker.set_onmessage(None);
        self.worker.set_onerror(None);
        self.worker.terminate();
    }
}

#[derive(Default)]
struct State {
    module: Option<WebAssembly::Module>,
    boot: Option<String>,
    worker: Option<Running>,
    /// The song being read or rated, and its message once read (sent when
    /// the worker is ready).
    busy: Option<(u32, String)>,
    /// Where the song being rated is no simfile (a Dancing☆Onigiri work,
    /// an FFR level): its charts as written for the worker, each one's
    /// place in the song and digest (`ratings::rating_simfile`).
    written: Option<Vec<(usize, ddi_chart::ChartDigest)>>,
    queued: Option<Object>,
    next: u32,
    /// A tick is scheduled.
    ticking: bool,
    /// When the worker last had a song (`performance.now()`).
    last_busy: f64,
    /// Why no worker starts, once one did not: songs then keep their own
    /// numbers.
    broken: Option<String>,
    /// Songs the worker rated, and its time doing so, ms.
    songs: u32,
    ms: f64,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

fn now() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map_or(0.0, |p| p.now())
}

/// Starts looking for songs to rate where an option wants ratings (at
/// boot, and on every route, `App::show`: an option turned On in Options
/// starts them on the way back; before round Z1 only the boot did, and
/// imported songs were never rated until the page was loaded again).
pub(crate) fn kick() {
    if !ratings::wanted() {
        return;
    }
    let start = STATE.with_borrow_mut(|s| !std::mem::replace(&mut s.ticking, true));
    if start {
        schedule(0);
    }
}

fn schedule(ms: u32) {
    gloo::timers::callback::Timeout::new(ms, tick).forget();
}

/// Takes the next song where none is being rated, and ends the worker
/// once idle; stops when no option wants ratings.
fn tick() {
    if !ratings::wanted() {
        STATE.with_borrow_mut(|s| {
            s.ticking = false;
            if s.busy.is_none() {
                s.worker = None;
            }
        });
        return;
    }
    let busy = STATE.with_borrow(|s| s.busy.is_some());
    if !busy && let Some(job) = ratings::next() {
        rate(job);
    }
    STATE.with_borrow_mut(|s| {
        if s.busy.is_none() && s.worker.is_some() && now() - s.last_busy > IDLE_MS {
            s.worker = None;
        }
    });
    schedule(TICK_MS.max(crate::perf::msd_slow_ms()));
}

/// Reads `job`'s simfile and hands it to the worker.
fn rate(job: Job) {
    if STATE.with_borrow(|s| s.broken.is_some()) {
        ratings::done(&job.song, Vec::new());
        return;
    }
    let id = STATE.with_borrow_mut(|s| {
        s.next = s.next.wrapping_add(1);
        s.busy = Some((s.next, job.song.clone()));
        s.written = None;
        s.last_busy = now();
        s.next
    });
    wasm_bindgen_futures::spawn_local(async move {
        let text = match crate::songs::chart_text(&job.song).await {
            Ok(t) => Ok(t),
            // A Dancing☆Onigiri work or an FFR level (round Y3): read as a
            // play reads it, its four-lane charts (generated versions
            // among them) written as a simfile the worker reads, their
            // numbers put back by their own digests.
            Err(_) => match crate::songs::load_chart(&job.song).await {
                Ok(song) => {
                    let (text, written) = ratings::rating_simfile(&song, &job.charts);
                    STATE.with_borrow_mut(|s| s.written = Some(written));
                    Ok((text, "ssc".to_string()))
                }
                Err(e) => Err(e),
            },
        };
        let (text, ext) = match text {
            Ok(t) => t,
            Err(_) => {
                finish(id, Vec::new());
                return;
            }
        };
        let charts_asked: Vec<usize> = match STATE.with_borrow(|s| s.written.as_ref().map(Vec::len)) {
            Some(n) => (0..n).collect(),
            None => job.charts.clone(),
        };
        let msg = Object::new();
        let set = |k: &str, v: &JsValue| {
            let _ = Reflect::set(&msg, &JsValue::from_str(k), v);
        };
        set("id", &JsValue::from(id));
        set("text", &JsValue::from_str(&text));
        set("ext", &JsValue::from_str(&ext));
        let charts = Array::new();
        for c in &charts_asked {
            charts.push(&JsValue::from(*c as f64));
        }
        set("charts", &charts);
        if let Err(e) = ensure_worker().await {
            STATE.with_borrow_mut(|s| s.broken = Some(e));
            finish(id, Vec::new());
            return;
        }
        let posted = STATE.with_borrow_mut(|s| match &s.worker {
            Some(w) if w.ready => Some(w.worker.post_message(&msg).is_ok()),
            Some(_) => {
                s.queued = Some(msg.clone());
                None
            }
            None => Some(false),
        });
        if posted == Some(false) {
            finish(id, Vec::new());
        }
    });
}

/// The song of `id` done (its charts' numbers, none where it could not be
/// rated).
fn finish(id: u32, rated: Vec<Rated>) {
    let song = STATE.with_borrow_mut(|s| match &s.busy {
        Some((b, song)) if *b == id => {
            let song = song.clone();
            s.busy = None;
            s.queued = None;
            s.last_busy = now();
            Some((song, s.written.take()))
        }
        _ => None,
    });
    if let Some((song, written)) = song {
        let rated = match written {
            Some(w) => ratings::placed(rated, &w),
            None => rated,
        };
        ratings::done(&song, rated);
    }
}

async fn ensure_worker() -> Result<(), String> {
    if STATE.with_borrow(|s| s.worker.is_some()) {
        return Ok(());
    }
    let (glue, wasm) = crate::web::wasm_worker::urls("ddi-msd-worker", "msd-worker");
    let module = match STATE.with_borrow(|s| s.module.clone()) {
        Some(m) => m,
        None => {
            let m = crate::web::wasm_worker::compile(&wasm, "the rating worker").await?;
            STATE.with_borrow_mut(|s| s.module = Some(m.clone()));
            m
        }
    };
    if STATE.with_borrow(|s| s.worker.is_some()) {
        return Ok(());
    }
    let boot = match STATE.with_borrow(|s| s.boot.clone()) {
        Some(b) => b,
        None => {
            let b = crate::web::wasm_worker::boot_url("failed", "the rating worker")?;
            STATE.with_borrow_mut(|s| s.boot = Some(b.clone()));
            b
        }
    };
    let worker = Worker::new(&boot).map_err(|e| js_err("starting the rating worker", e))?;
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new(|event: MessageEvent| {
        heard(event.data());
    });
    let on_error = Closure::<dyn FnMut(Event)>::new(|event: Event| {
        event.prevent_default();
        let detail = crate::web::wasm_worker::error_detail(&event);
        lost(&format!("the rating worker failed: {detail}"));
    });
    worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    crate::web::wasm_worker::post_start(&worker, &glue, &wasm, &module)
        .map_err(|e| js_err("starting the rating worker", e))?;
    STATE.with_borrow_mut(|s| {
        s.worker = Some(Running {
            worker,
            ready: false,
            _on_message: on_message,
            _on_error: on_error,
        })
    });
    gloo::timers::callback::Timeout::new(READY_MS, || {
        if STATE.with_borrow(|s| s.worker.as_ref().is_some_and(|w| !w.ready)) {
            lost("the rating worker did not start in time");
        }
    })
    .forget();
    Ok(())
}

/// The worker is gone: what it had keeps its numbers, and no more is
/// rated this session.
fn lost(why: &str) {
    let id = STATE.with_borrow_mut(|s| {
        s.broken = Some(why.to_string());
        s.worker = None;
        s.busy.as_ref().map(|(id, _)| *id)
    });
    if let Some(id) = id {
        finish(id, Vec::new());
    }
}

/// What the worker said.
fn heard(data: JsValue) {
    let get = |k: &str| Reflect::get(&data, &JsValue::from_str(k)).ok();
    if get("ready").is_some_and(|v| v.is_truthy()) {
        let lost_message = STATE.with_borrow_mut(|s| {
            let queued = s.queued.take();
            match (&mut s.worker, queued) {
                (Some(w), Some(msg)) => {
                    w.ready = true;
                    w.worker.post_message(&msg).is_err()
                }
                (Some(w), None) => {
                    w.ready = true;
                    false
                }
                _ => false,
            }
        });
        if lost_message {
            lost("the rating worker took no song");
        }
        return;
    }
    if let Some(why) = get("failed").and_then(|f| f.as_string()) {
        lost(&why);
        return;
    }
    let Some(id) = get("id").and_then(|v| v.as_f64()).map(|v| v as u32) else {
        return;
    };
    if let Some(ms) = get("ms").and_then(|v| v.as_f64()) {
        STATE.with_borrow_mut(|s| {
            s.songs += 1;
            s.ms += ms;
        });
    }
    let rated = get("rated")
        .map(|r| Array::from(&r))
        .map(|a| a.iter().filter_map(|e| rated_of(&Array::from(&e))).collect())
        .unwrap_or_default();
    finish(id, rated);
    // The next song at once (spaced out by the testing aid `msdslow`).
    if ratings::wanted()
        && crate::perf::msd_slow_ms() == 0
        && let Some(job) = ratings::next()
    {
        rate(job);
    }
}

/// `[index, digest, values]` as [`Rated`].
fn rated_of(e: &Array) -> Option<Rated> {
    let index = e.get(0).as_f64()? as usize;
    let digest = e.get(1).as_string()?.parse().ok()?;
    let values = e.get(2);
    let values = if values.is_null() {
        None
    } else {
        let a = Array::from(&values);
        let mut v = [0f32; 8];
        for (k, x) in v.iter_mut().enumerate() {
            *x = a.get(k as u32).as_f64()? as f32;
        }
        Some(v)
    };
    Some(Rated {
        index,
        digest,
        values,
    })
}

/// `ratings` in `window.__DDI_DEBUG`: whether an option wants ratings,
/// whether a play pauses them, the songs waiting, the song being rated,
/// the charts rated this session and known, and the worker's songs and
/// time (ms), and why it is broken if it is.
pub(crate) fn debug_into(obj: &Object) {
    let st = ratings::status();
    let o = Object::new();
    let set = |k: &str, v: JsValue| {
        let _ = Reflect::set(&o, &JsValue::from_str(k), &v);
    };
    set("wanted", st.wanted.into());
    set("paused", st.paused.into());
    set("pending", (st.pending as f64).into());
    set(
        "rating",
        st.rating.map_or(JsValue::NULL, |s| JsValue::from_str(&s)),
    );
    set("rated", (st.rated as f64).into());
    set("known", (st.known as f64).into());
    STATE.with_borrow(|s| {
        set("worker", s.worker.is_some().into());
        set("workerSongs", f64::from(s.songs).into());
        set("workerMs", s.ms.into());
        set(
            "broken",
            s.broken
                .clone()
                .map_or(JsValue::NULL, |b| JsValue::from_str(&b)),
        );
    });
    let _ = Reflect::set(obj, &JsValue::from_str("ratings"), &o);
}
