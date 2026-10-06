//! Web Audio backend.
//!
//! `AudioContext.currentTime` is the master clock. `now()` pairs it with the
//! host clock through `getOutputTimestamp()` (sanity-checked, since Safari has
//! shipped a broken implementation) and reports `outputLatency` when the
//! browser exposes it. Neither property has web-sys bindings, so both go
//! through `js_sys::Reflect`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use ddi_platform::{AudioBackend, ClockSample, HostTime, SoundId};
use js_sys::{Function, Reflect};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AudioBuffer, AudioBufferSourceNode, AudioContext, AudioContextLatencyCategory,
    AudioContextOptions, AudioContextState, AudioScheduledSourceNode, GainNode,
};

use super::clock::PerformanceClock;
use super::js_err;

/// Consecutive implausible `getOutputTimestamp()` readings before the
/// implementation is distrusted for the rest of the session. A single bad
/// reading (e.g. zeros right after `resume()`) must not disable it.
const MAX_BAD_TIMESTAMPS: u32 = 5;

struct Voice {
    source: AudioBufferSourceNode,
    gain: GainNode,
    /// `onended` handler; dropped with the voice so nothing leaks per sound.
    _on_ended: Closure<dyn FnMut()>,
}

#[derive(Default)]
struct Voices {
    live: HashMap<SoundId, Voice>,
    /// Voices whose `onended` fired. The handler cannot drop its own closure
    /// while it runs, so they are parked here and freed on the next call.
    finished: Vec<Voice>,
}

pub(crate) struct WebAudio {
    ctx: AudioContext,
    master: GainNode,
    voices: Rc<RefCell<Voices>>,
    next_id: SoundId,
    /// Consecutive implausible `getOutputTimestamp` readings; `None` once
    /// the implementation has been given up on.
    bad_timestamps: Cell<Option<u32>>,
}

impl WebAudio {
    /// Must be called from inside a user gesture so the context may start.
    pub(crate) fn new() -> Result<WebAudio, String> {
        let opts = AudioContextOptions::new();
        opts.set_latency_hint_audio_context_latency_category(
            AudioContextLatencyCategory::Interactive,
        );
        let ctx = AudioContext::new_with_context_options(&opts)
            .or_else(|_| AudioContext::new())
            .map_err(|e| js_err("creating AudioContext", e))?;
        // Kick off resuming while still inside the gesture; callers that need
        // the context running await `resume()` as well.
        let _ = ctx.resume();
        let master = ctx
            .create_gain()
            .map_err(|e| js_err("creating master gain", e))?;
        master
            .connect_with_audio_node(&ctx.destination())
            .map_err(|e| js_err("connecting master gain", e))?;
        Ok(WebAudio {
            ctx,
            master,
            voices: Rc::new(RefCell::new(Voices::default())),
            next_id: 1,
            bad_timestamps: Cell::new(Some(0)),
        })
    }

    pub(crate) fn context(&self) -> &AudioContext {
        &self.ctx
    }

    pub(crate) async fn resume(&self) {
        if !self.running()
            && let Ok(promise) = self.ctx.resume()
        {
            let _ = JsFuture::from(promise).await;
        }
    }

    /// Whether the context is producing sound (not suspended by the browser
    /// or interrupted by the OS).
    pub(crate) fn running(&self) -> bool {
        self.ctx.state() == AudioContextState::Running
    }

    pub(crate) async fn decode(&self, bytes: &[u8]) -> Result<AudioBuffer, String> {
        // Uint8Array::from copies, so the wasm-side slice can be dropped freely.
        let array = js_sys::Uint8Array::from(bytes);
        let promise = self
            .ctx
            .decode_audio_data(&array.buffer())
            .map_err(|e| js_err("decodeAudioData", e))?;
        JsFuture::from(promise)
            .await
            .map_err(|e| js_err("decodeAudioData", e))?
            .dyn_into::<AudioBuffer>()
            .map_err(|e| js_err("decodeAudioData result", e))
    }

    /// Stops and forgets every scheduled or playing sound.
    /// Ramps the master gain to silence over `seconds` (sounds keep running;
    /// call `stop_all` afterwards or let them end).
    pub(crate) fn fade_out(&self, seconds: f64) {
        let gain = self.master.gain();
        let now = self.ctx.current_time();
        let _ = gain.cancel_scheduled_values(now);
        let _ = gain.set_value_at_time(gain.value(), now);
        let _ = gain.linear_ramp_to_value_at_time(0.0, now + seconds.max(0.01));
    }

    pub(crate) fn stop_all(&mut self) {
        let mut voices = self.voices.borrow_mut();
        let live: Vec<Voice> = voices.live.drain().map(|(_, v)| v).collect();
        voices.finished.clear();
        drop(voices);
        for voice in live {
            Self::silence(&voice);
        }
    }

    fn silence(voice: &Voice) {
        let scheduled: &AudioScheduledSourceNode = voice.source.as_ref();
        scheduled.set_onended(None);
        let _ = scheduled.stop();
        let _ = voice.source.disconnect();
        let _ = voice.gain.disconnect();
    }

    /// `AudioContext.outputLatency` in seconds, or 0 when unsupported or
    /// implausible (anything ≥ 1 s is treated as garbage).
    fn latency_property(&self, name: &str) -> f64 {
        Reflect::get(&self.ctx, &JsValue::from_str(name))
            .ok()
            .and_then(|v| v.as_f64())
            .filter(|v| v.is_finite() && *v >= 0.0 && *v < 1.0)
            .unwrap_or(0.0)
    }

    /// Hardware/OS delay after the audio leaves the graph.
    fn output_latency(&self) -> f64 {
        self.latency_property("outputLatency")
    }

    /// Processing delay inside the graph before the output stage.
    fn base_latency(&self) -> f64 {
        self.latency_property("baseLatency")
    }

    /// Waits (up to ~800 ms) for the output stream to report its hardware
    /// latency, which reads 0 right after the context starts (`baseLatency`
    /// is available immediately and is not what we are waiting for). Browsers
    /// that never report it simply wait out the timeout.
    pub(crate) async fn wait_for_latency(&self) {
        for _ in 0..16 {
            if self.output_latency() > 0.0 {
                return;
            }
            gloo::timers::future::TimeoutFuture::new(50).await;
        }
    }

    /// Identity of the audio output path from what the browser exposes
    /// without any permission: sample rate and the two latencies (output
    /// latency bucketed to 5 ms, so small per-run jitter maps to the same
    /// device). Call once the context is running; latencies read 0 before.
    pub(crate) fn fingerprint(&self) -> ddi_platform::DeviceFingerprint {
        let rate = self.ctx.sample_rate().round() as u32;
        let base_ms = (self.base_latency() * 1000.0).round() as u32;
        let out_ms = (self.output_latency() * 1000.0 / 5.0).round() as u32 * 5;
        let kind = if out_ms >= 100 {
            "wireless?"
        } else if out_ms == 0 && base_ms == 0 {
            "unknown latency"
        } else {
            "wired/built-in?"
        };
        ddi_platform::DeviceFingerprint::new(
            format!("audio:{rate}:{base_ms}:{out_ms}"),
            format!(
                "{} kHz · ~{} ms output latency ({kind})",
                rate / 1000,
                out_ms + base_ms
            ),
        )
    }

    /// `(contextTime, performanceTime)` from `getOutputTimestamp()`, or `None`
    /// if unsupported, implausible, or the context is not running (a
    /// suspended context legitimately reports stale or zero values).
    fn output_timestamp(&self) -> Option<(f64, f64)> {
        let bad = self.bad_timestamps.get()?;
        if !self.running() {
            return None;
        }
        let func = Reflect::get(&self.ctx, &JsValue::from_str("getOutputTimestamp"))
            .ok()?
            .dyn_into::<Function>()
            .ok()?;
        let ts = func.call0(&self.ctx).ok()?;
        let context_time = Reflect::get(&ts, &JsValue::from_str("contextTime"))
            .ok()?
            .as_f64()?;
        let performance_time = Reflect::get(&ts, &JsValue::from_str("performanceTime"))
            .ok()?
            .as_f64()?;
        let now_ctx = self.ctx.current_time();
        let now_perf = PerformanceClock::now_ms();
        // Safari 15.1 reported contextTime ~10,000× too small; also reject
        // pairs that disagree with a direct reading by more than half a second.
        let plausible = context_time.is_finite()
            && performance_time.is_finite()
            && (context_time - now_ctx).abs() < 0.5
            && (performance_time - now_perf).abs() < 500.0
            && !(now_ctx > 1.0 && context_time < 0.001);
        if !plausible {
            // Right after start the first render quantum may not have reached
            // the device yet and browsers hand back zeros; only a streak of
            // bad readings means the implementation itself is broken.
            let bad = bad + 1;
            self.bad_timestamps
                .set((bad < MAX_BAD_TIMESTAMPS).then_some(bad));
            return None;
        }
        self.bad_timestamps.set(Some(0));
        Some((context_time, performance_time))
    }
}

impl AudioBackend for WebAudio {
    type Buffer = AudioBuffer;
    type Error = String;

    fn now(&self) -> ClockSample {
        // Everything between `currentTime` and the speaker: the graph's own
        // buffer (`baseLatency`) plus the hardware/OS path (`outputLatency`).
        let output_latency = self.base_latency() + self.output_latency();
        match self.output_timestamp() {
            Some((heard_context_time, performance_ms)) => ClockSample {
                // `getOutputTimestamp().contextTime` is the sample frame
                // leaving the device at `performanceTime`, i.e. it already
                // has the whole latency taken off. `ClockSample.context_time`
                // is defined as the `currentTime`-equivalent and the engine
                // subtracts `output_latency` itself, so add it back here;
                // otherwise the latency would be counted twice.
                context_time: heard_context_time + output_latency,
                host_time: HostTime(performance_ms / 1000.0),
                output_latency,
            },
            None => {
                // Back-to-back sampling: assume both reads are "now".
                let host = PerformanceClock::now_ms();
                let context_time = self.ctx.current_time();
                ClockSample {
                    context_time,
                    host_time: HostTime(host / 1000.0),
                    output_latency,
                }
            }
        }
    }

    fn play(
        &mut self,
        buffer: &AudioBuffer,
        start_at_context_time: f64,
        offset: f64,
        volume: f32,
    ) -> Result<SoundId, String> {
        // Free voices whose `onended` has fired since the last call.
        self.voices.borrow_mut().finished.clear();
        let source = self
            .ctx
            .create_buffer_source()
            .map_err(|e| js_err("createBufferSource", e))?;
        source.set_buffer(Some(buffer));
        let gain = self
            .ctx
            .create_gain()
            .map_err(|e| js_err("createGain", e))?;
        gain.gain().set_value(volume);
        source
            .connect_with_audio_node(&gain)
            .map_err(|e| js_err("connect source", e))?;
        gain.connect_with_audio_node(&self.master)
            .map_err(|e| js_err("connect gain", e))?;
        source
            .start_with_when_and_grain_offset(start_at_context_time, offset.max(0.0))
            .map_err(|e| js_err("start", e))?;
        let id = self.next_id;
        self.next_id += 1;
        let voices = self.voices.clone();
        let on_ended = Closure::<dyn FnMut()>::new(move || {
            let mut voices = voices.borrow_mut();
            if let Some(voice) = voices.live.remove(&id) {
                let _ = voice.source.disconnect();
                let _ = voice.gain.disconnect();
                voices.finished.push(voice);
            }
        });
        let scheduled: &AudioScheduledSourceNode = source.as_ref();
        scheduled.set_onended(Some(on_ended.as_ref().unchecked_ref()));
        self.voices.borrow_mut().live.insert(
            id,
            Voice {
                source,
                gain,
                _on_ended: on_ended,
            },
        );
        Ok(id)
    }

    fn stop(&mut self, id: SoundId) {
        let voice = {
            let mut voices = self.voices.borrow_mut();
            voices.finished.clear();
            voices.live.remove(&id)
        };
        if let Some(voice) = voice {
            Self::silence(&voice);
        }
    }

    fn set_volume(&mut self, id: SoundId, volume: f32) {
        if let Some(voice) = self.voices.borrow().live.get(&id) {
            voice.gain.gain().set_value(volume);
        }
    }

    fn duration(&self, buffer: &AudioBuffer) -> f64 {
        buffer.duration()
    }
}

impl Drop for WebAudio {
    fn drop(&mut self) {
        self.stop_all();
        // Browsers cap the number of live AudioContexts (Chrome: 6); release
        // the hardware instead of waiting for GC.
        let _ = self.ctx.close();
    }
}
