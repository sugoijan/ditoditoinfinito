//! App-wide device probing: a background audio context (started on the first
//! user gesture, since browsers block audio before one) plus a short rAF
//! sampler for the refresh rate, combined into a `DeviceProfile`.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_platform::DeviceProfile;
use gloo::render::{AnimationFrame, request_animation_frame};

use super::audio::WebAudio;
use super::display;

/// `navigator.mediaDevices`, or `None` where it is missing: browsers leave it
/// undefined outside secure contexts (e.g. a LAN address over plain HTTP),
/// and web-sys would hand back `undefined` as if it were the object.
pub(crate) fn media_devices() -> Option<web_sys::MediaDevices> {
    let md = web_sys::window()?.navigator().media_devices().ok()?;
    (!wasm_bindgen::JsValue::from(md.clone()).is_undefined()).then_some(md)
}

/// Measures the refresh rate over `frames` animation frames and calls `done`
/// with the snapped rate.
pub(crate) fn sample_refresh_rate(frames: u32, done: impl Fn(f64) + 'static) -> RefreshSampler {
    let state = Rc::new(RefCell::new(SamplerState {
        last: None,
        intervals: Vec::with_capacity(frames as usize),
        handle: None,
    }));
    let done = Rc::new(done);
    schedule(state.clone(), frames, done);
    RefreshSampler { _state: state }
}

pub(crate) struct RefreshSampler {
    _state: Rc<RefCell<SamplerState>>,
}

struct SamplerState {
    last: Option<f64>,
    /// Frame intervals in ms.
    intervals: Vec<f64>,
    handle: Option<AnimationFrame>,
}

/// Frames can be dropped (long intervals) but never added, so the refresh
/// period is the short end of the interval distribution: the 20th
/// percentile, which ignores start-up stalls and occasional drops.
fn estimate_hz(intervals: &mut [f64]) -> f64 {
    if intervals.is_empty() {
        return 60.0;
    }
    intervals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let period_ms = intervals[intervals.len() / 5].max(1.0);
    1000.0 / period_ms
}

fn schedule(state: Rc<RefCell<SamplerState>>, frames: u32, done: Rc<dyn Fn(f64)>) {
    let s2 = state.clone();
    let handle = request_animation_frame(move |t| {
        let finished = {
            let mut s = s2.borrow_mut();
            if let Some(last) = s.last {
                let dt = t - last;
                if dt > 0.0 {
                    s.intervals.push(dt);
                }
            }
            s.last = Some(t);
            if s.intervals.len() as u32 >= frames {
                Some(estimate_hz(&mut s.intervals))
            } else {
                None
            }
        };
        match finished {
            Some(hz) => done(display::snap_refresh_rate(hz)),
            None => schedule(s2, frames, done),
        }
    });
    state.borrow_mut().handle = Some(handle);
}

/// Fingerprints the audio path through `audio` (which must be running) and
/// the display at `refresh_hz`.
pub(crate) async fn probe(audio: &WebAudio, refresh_hz: f64) -> DeviceProfile {
    audio.resume().await;
    audio.wait_for_latency().await;
    DeviceProfile {
        audio: audio.fingerprint(),
        display: display::fingerprint(refresh_hz),
    }
}
