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
    AnalyserNode, AudioBuffer, AudioBufferSourceNode, AudioContext, AudioContextLatencyCategory,
    AudioContextOptions, AudioContextState, AudioScheduledSourceNode, BaseAudioContext, GainNode,
};

use super::clock::PerformanceClock;
use super::js_err;

/// Consecutive implausible `getOutputTimestamp()` readings before the
/// implementation is distrusted for the rest of the session. A single bad
/// reading (e.g. zeros right after `resume()`) must not disable it.
const MAX_BAD_TIMESTAMPS: u32 = 5;

/// Samples the output meter reads per frame.
const ANALYSER_SIZE: usize = 2048;

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
    analyser: AnalyserNode,
    voices: Rc<RefCell<Voices>>,
    next_id: SoundId,
    /// Consecutive implausible `getOutputTimestamp` readings; `None` once
    /// the implementation has been given up on.
    bad_timestamps: Cell<Option<u32>>,
}

/// Starts `ctx` resuming without waiting, its promise's rejection caught
/// (Firefox rejects it with "Closed before resume completed" when the
/// context closes meanwhile: nothing to report).
fn resume_quietly(ctx: &AudioContext) {
    if let Ok(promise) = ctx.resume() {
        wasm_bindgen_futures::spawn_local(async move {
            let _ = JsFuture::from(promise).await;
        });
    }
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
        resume_quietly(&ctx);
        let master = ctx
            .create_gain()
            .map_err(|e| js_err("creating master gain", e))?;
        // The analyser sits in the output path (Safari only processes
        // nodes that reach the destination) and feeds the debug overlay's
        // output meter.
        let analyser = ctx
            .create_analyser()
            .map_err(|e| js_err("creating analyser", e))?;
        analyser.set_fft_size(ANALYSER_SIZE as u32);
        master
            .connect_with_audio_node(&analyser)
            .and_then(|_| analyser.connect_with_audio_node(&ctx.destination()))
            .map_err(|e| js_err("connecting master gain", e))?;
        Ok(WebAudio {
            ctx,
            master,
            analyser,
            voices: Rc::new(RefCell::new(Voices::default())),
            next_id: 1,
            bad_timestamps: Cell::new(Some(0)),
        })
    }

    /// One line for the debug overlay: context state, sample rate, master
    /// gain and the peak output level right now, so "no sound" can be told
    /// apart into "the game outputs nothing" and "the output goes nowhere".
    pub(crate) fn status_line(&self) -> String {
        let mut samples = vec![0.0f32; ANALYSER_SIZE];
        self.analyser.get_float_time_domain_data(&mut samples);
        let peak = samples.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let level = if peak > 0.0 {
            format!("{:.1} dBFS", 20.0 * peak.log10())
        } else {
            "silent".into()
        };
        format!(
            "audio: {:?} · {} Hz · master {:.2} · output peak {level}",
            self.ctx.state(),
            self.ctx.sample_rate(),
            self.master.gain().value()
        )
    }

    pub(crate) fn context(&self) -> &AudioContext {
        &self.ctx
    }

    /// Asks the context to start without waiting; call it inside the user
    /// gesture that created the context, then await [`Self::resume`].
    pub(crate) fn resume_now(&self) {
        resume_quietly(&self.ctx);
    }

    pub(crate) async fn resume(&self) {
        if !self.running()
            && let Ok(promise) = self.ctx.resume()
        {
            let _ = JsFuture::from(promise).await;
        }
    }

    /// Suspends the context (its output stops, its clock with it) without
    /// waiting; [`Self::resume_now`] starts it again.
    pub(crate) fn suspend_now(&self) {
        if let Ok(promise) = self.ctx.suspend() {
            wasm_bindgen_futures::spawn_local(async move {
                let _ = JsFuture::from(promise).await;
            });
        }
    }

    /// Whether the context is producing sound (not suspended by the browser
    /// or interrupted by the OS).
    pub(crate) fn running(&self) -> bool {
        self.ctx.state() == AudioContextState::Running
    }

    /// A handle that decodes on this context without borrowing it.
    pub(crate) fn decoder(&self) -> AudioDecoder {
        AudioDecoder {
            ctx: self.ctx.clone().into(),
            live: Some(self.ctx.clone()),
        }
    }

    /// Plays `length` seconds of `buffer` from `offset`, fading in and out,
    /// for previews. The fades are on the sound and the volume on the
    /// master, so [`Self::set_master_volume`] can change it while it plays.
    pub(crate) fn play_preview(
        &mut self,
        buffer: &AudioBuffer,
        offset: f64,
        length: f64,
        volume: f32,
    ) -> Result<SoundId, String> {
        use ddi_session::preview::{FADE, LEAD};
        self.set_master_volume(volume);
        let start = self.ctx.current_time() + LEAD;
        let end = start + length.max(2.0 * FADE);
        let id = self.play(buffer, start, offset, 0.0)?;
        if let Some(voice) = self.voices.borrow().live.get(&id) {
            let gain = voice.gain.gain();
            let _ = gain.set_value_at_time(0.0, start);
            let _ = gain.linear_ramp_to_value_at_time(1.0, start + FADE);
            let _ = gain.set_value_at_time(1.0, end - FADE);
            let _ = gain.linear_ramp_to_value_at_time(0.0, end);
            let scheduled: &AudioScheduledSourceNode = voice.source.as_ref();
            let _ = scheduled.stop_with_when(end);
        }
        Ok(id)
    }

    /// Moves the master volume smoothly (no click) to `volume`.
    pub(crate) fn set_master_volume(&self, volume: f32) {
        let gain = self.master.gain();
        let now = self.ctx.current_time();
        let _ = gain.cancel_scheduled_values(now);
        let _ = gain.set_target_at_time(volume, now, 0.015);
    }

    /// Stops and forgets every scheduled or playing sound.
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

/// Decoding and starting on a context, detached from [`WebAudio`] so a
/// decode in flight holds no borrow of it.
#[derive(Clone)]
pub(crate) struct AudioDecoder {
    /// What decodes: the live context, or an offline one.
    ctx: BaseAudioContext,
    /// The live context, when it is one.
    live: Option<AudioContext>,
}

impl AudioDecoder {
    /// A decoder on an `OfflineAudioContext` at `rate`, which any page may
    /// make before the player's first input (no audio starts): a song
    /// decodes before there is a context to play it on, and its buffer
    /// plays on that context later (resampled as it plays if the rates
    /// differ). `None` where the browser has none.
    pub(crate) fn offline(rate: f32) -> Option<AudioDecoder> {
        let ctx =
            web_sys::OfflineAudioContext::new_with_number_of_channels_and_length_and_sample_rate(
                2, 1, rate,
            )
            .ok()?;
        Some(AudioDecoder {
            ctx: ctx.into(),
            live: None,
        })
    }

    /// Waits for the context to run (call `resume_now` in the gesture first).
    pub(crate) async fn resume(&self) {
        if let Some(live) = &self.live
            && live.state() != AudioContextState::Running
            && let Ok(promise) = live.resume()
        {
            let _ = JsFuture::from(promise).await;
        }
    }

    /// Decodes a whole song. The browser's `decodeAudioData` comes first
    /// (native speed, off the main thread, PCM kept out of wasm memory);
    /// when it rejects the file, or returns silence, the Symphonia decoder
    /// in `ddi_platform::decode` takes over. Safari since 18.4 accepts Ogg
    /// Vorbis in `decodeAudioData` but decodes it to silence, so a rejection
    /// alone does not catch it.
    ///
    /// MP3s come out as StepMania 5.1 decodes them (`ddi_platform::mp3`,
    /// PLAN.md decision 46): a browser's decode is lined up with ours
    /// ([`Self::align_mp3`]); the built-in decoder decodes them that way.
    ///
    /// Test hook: `window.__DDI_FORCE_WASM_DECODE = true` skips the browser
    /// decoder, so the fallback can be exercised in browsers that decode
    /// Vorbis natively.
    pub(crate) async fn decode(
        &self,
        bytes: &[u8],
        target: ddi_platform::mp3::Target,
    ) -> Result<AudioBuffer, String> {
        self.decode_while(bytes, target, || true).await
    }

    /// [`Self::decode`], the MP3 alignment given up (the browser's decode
    /// returned as it is) once `wanted` says the result is no longer
    /// wanted (a preview of a song the player moved past). `target`:
    /// whose decode the song's notes are timed against (StepMania's, or
    /// Chrome's for a Dancing☆Onigiri work).
    pub(crate) async fn decode_while(
        &self,
        bytes: &[u8],
        target: ddi_platform::mp3::Target,
        wanted: impl Fn() -> bool,
    ) -> Result<AudioBuffer, String> {
        let (buffer, ours) = self.decode_any(bytes, &wanted).await?;
        if ours && ddi_platform::decode::is_mp3(bytes) {
            // Decoded as StepMania does: a work's starts where Chrome's
            // decode has it.
            if let Some(first) = ddi_platform::mp3::first_frame(bytes) {
                let shift = first.target_lag(target) as f64 / f64::from(first.header.rate.max(1));
                if shift != 0.0 {
                    set_lead(&buffer, -shift);
                }
                report_ours(&first, -shift);
            }
            return Ok(buffer);
        }
        if ours || !ddi_platform::decode::is_mp3(bytes) {
            if !ours && ddi_platform::mp3::free_format(bytes) {
                web_sys::console::warn_1(
                    &"MP3: a free-format file, which the built-in decoder does not read; \
                      the browser's decode is kept as it is"
                        .into(),
                );
            }
            return Ok(buffer);
        }
        self.align_mp3(bytes, &buffer, target, wanted).await;
        Ok(buffer)
    }

    /// The browser's decode, else the built-in one; whether it is the
    /// built-in one. The built-in one stops once `wanted` says no.
    async fn decode_any(
        &self,
        bytes: &[u8],
        wanted: &dyn Fn() -> bool,
    ) -> Result<(AudioBuffer, bool), String> {
        // A preview stopped mid-decode closes its context: give up rather
        // than run the wasm fallback for nothing.
        let closed = |ctx: &BaseAudioContext| ctx.state() == AudioContextState::Closed;
        if closed(&self.ctx) {
            return Err("audio context closed".into());
        }
        let browser_error = if force_wasm_decode() {
            "skipped by window.__DDI_FORCE_WASM_DECODE".to_string()
        } else {
            match self.decode_in_browser(bytes).await {
                Ok(buffer) if !looks_silent(&buffer) => return Ok((buffer, false)),
                Ok(buffer) => match self.decode_in_wasm(bytes, wanted).await {
                    Ok(wasm) => {
                        web_sys::console::info_1(
                            &"decodeAudioData returned silence; decoded in wasm instead".into(),
                        );
                        return Ok((wasm, true));
                    }
                    // Not a format the fallback reads: keep the browser's
                    // result, the song may really be silent there.
                    Err(_) => return Ok((buffer, false)),
                },
                Err(e) => e,
            }
        };
        if closed(&self.ctx) {
            return Err("audio context closed".into());
        }
        match self.decode_in_wasm(bytes, wanted).await {
            Ok(buffer) => {
                web_sys::console::info_1(
                    &format!("decodeAudioData failed ({browser_error}); decoded in wasm instead")
                        .into(),
                );
                Ok((buffer, true))
            }
            // Given up, not failed.
            Err(wasm_error) if !wanted() => Err(wasm_error),
            Err(wasm_error) => Err(format!(
                "could not decode the audio. Browser decoder: {browser_error}. \
                 Built-in decoder: {wasm_error}"
            )),
        }
    }

    /// Moves `buffer` (a browser's decode of the MP3 `bytes`) to where
    /// StepMania 5.1's decode of the file has it. Browsers trim the LAME
    /// delay or part of it and drop an `Info` frame StepMania plays as
    /// silence, each its own way (Chrome all of LAME's delay, Firefox the
    /// decoder's 529 samples, WebKit 529 always and more for some files).
    /// The first frame says which lags are possible
    /// ([`ddi_platform::mp3::FirstFrame::candidates`]); the start of the
    /// song is decoded here as StepMania does and the one the music
    /// agrees on is taken ([`ddi_platform::mp3::align`]), else (a tone, a
    /// loop, silence, a decode that differs) the one this browser was
    /// measured to have ([`ddi_platform::mp3::FirstFrame::expected`]),
    /// logged. The buffer keeps its samples: the lag is kept on it as
    /// its lead ([`set_lead`]), and sounds made from it start that much
    /// later (or further in), so no second copy of the song is made. The
    /// clock, offsets and calibration are untouched. A Dancing☆Onigiri
    /// work (`target` Chromium) goes where Chrome's decode has it instead:
    /// the lag found less Chrome's ([`ddi_platform::mp3::FirstFrame::target_lag`]),
    /// so Chrome's own decode stays as it is. What was done is kept in
    /// `window.__DDI_MP3` for tests.
    async fn align_mp3(
        &self,
        bytes: &[u8],
        buffer: &AudioBuffer,
        target: ddi_platform::mp3::Target,
        wanted: impl Fn() -> bool,
    ) {
        use ddi_platform::mp3;
        /// How far into the song the reference is decoded, at most.
        const STEPS: [f64; 3] = [8.0, 20.0, 40.0];
        const SLICE_MS: f64 = 25.0;
        let started = PerformanceClock::now_ms();
        let Some(first) = mp3::first_frame(bytes) else {
            return;
        };
        let browser = mp3::Browser::from_user_agent(
            &web_sys::window()
                .and_then(|w| w.navigator().user_agent().ok())
                .unwrap_or_default(),
        );
        let candidates = first.candidates();
        // The left channel only, room for the first step (it grows if
        // more is needed), not the song.
        let mut decoder =
            match ddi_platform::decode::Decoder::start_left(bytes, STEPS[0] + first.max_lag()) {
                Ok(d) => d,
                Err(e) => {
                    web_sys::console::warn_1(&format!("MP3 alignment: {e}").into());
                    return;
                }
            };
        let ref_rate = decoder.sample_rate();
        let rate = buffer.sample_rate() as u32;
        let length = buffer.length() as usize;
        let mut result = Err(mp3::Unsure::Silent);
        let mut done = false;
        for seconds in STEPS {
            let want = (seconds * f64::from(ref_rate)) as usize;
            while !done && decoder.frames() < want {
                let slice = PerformanceClock::now_ms();
                while !done
                    && decoder.frames() < want
                    && PerformanceClock::now_ms() - slice < SLICE_MS
                {
                    match decoder.step(8) {
                        Ok(d) => done = d,
                        Err(_) => done = true,
                    }
                }
                gloo::timers::future::TimeoutFuture::new(0).await;
                if !wanted() {
                    return;
                }
            }
            // The browser's samples only where the stretches are, read
            // from its buffer as they are needed.
            result = mp3::align(
                decoder.left(),
                ref_rate,
                rate,
                &candidates,
                |start, out: &mut [f32]| {
                    let n = length.saturating_sub(start).min(out.len());
                    if n == 0
                        || buffer
                            .copy_from_channel_with_start_in_channel(&mut out[..n], 0, start as u32)
                            .is_err()
                    {
                        out.fill(0.0);
                    } else {
                        out[n..].fill(0.0);
                    }
                },
            );
            if result.is_ok() || done {
                break;
            }
        }
        let took = PerformanceClock::now_ms() - started;
        let lag = match result {
            Ok(a) => a.lag,
            Err(_) => first.expected(browser) as f64 / f64::from(ref_rate),
        };
        let lead = lag - first.target_lag(target) as f64 / f64::from(ref_rate);
        set_lead(buffer, lead);
        report_mp3(&first, browser, result, lag, lead, rate, took);
    }

    async fn decode_in_browser(&self, bytes: &[u8]) -> Result<AudioBuffer, String> {
        // `Uint8Array::from` copies into a fresh JS ArrayBuffer, which
        // `decodeAudioData` detaches; `bytes` stays intact in wasm memory
        // for the fallback.
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

    /// Decodes with the built-in decoders: Ogg Opus in the Opus worker
    /// (`super::opus`, fetched only then: PLAN.md decision 9), resampled
    /// there to the context's rate as `decodeAudioData` gives it; Ogg
    /// Vorbis and MP3 here (`ddi_platform::decode`) on the main thread,
    /// yielding to the event loop every few milliseconds so the page keeps
    /// painting, at the file's own rate (the source node resamples it on
    /// playback, as with any buffer). Gives up once `wanted` says no (a
    /// preview of a song the player moved past) or the context closed.
    async fn decode_in_wasm(
        &self,
        bytes: &[u8],
        wanted: &dyn Fn() -> bool,
    ) -> Result<AudioBuffer, String> {
        /// Main-thread time per slice before yielding.
        const SLICE_MS: f64 = 25.0;
        /// Packets per `step` (~20 ms of audio each, well under 1 ms of work).
        const PACKETS_PER_STEP: usize = 8;

        let gone = || !wanted() || self.ctx.state() == AudioContextState::Closed;
        if ddi_platform::decode::is_ogg_opus(bytes) {
            return super::opus::decode(&self.ctx, bytes, &gone).await;
        }
        let mut decoder = ddi_platform::decode::Decoder::new(bytes).map_err(|e| e.to_string())?;
        loop {
            let slice_start = PerformanceClock::now_ms();
            let mut done = false;
            while !done && PerformanceClock::now_ms() - slice_start < SLICE_MS {
                done = decoder.step(PACKETS_PER_STEP).map_err(|e| e.to_string())?;
            }
            if done {
                break;
            }
            gloo::timers::future::TimeoutFuture::new(0).await;
            if gone() {
                return Err("the decode is no longer wanted".into());
            }
        }
        let pcm = decoder.finish().map_err(|e| e.to_string())?;
        if pcm.skipped_packets > 0 {
            web_sys::console::warn_1(
                &format!(
                    "audio decode: skipped {} undecodable packets",
                    pcm.skipped_packets
                )
                .into(),
            );
        }
        let frames = u32::try_from(pcm.frames()).map_err(|_| "audio is too long".to_string())?;
        let buffer = self
            .ctx
            .create_buffer(pcm.channels.len() as u32, frames, pcm.sample_rate as f32)
            .map_err(|e| js_err("createBuffer", e))?;
        // Hand each channel over and free it straight away, so wasm memory
        // never holds more than the PCM once.
        for (index, samples) in pcm.channels.into_iter().enumerate() {
            buffer
                .copy_to_channel(&samples, index as i32)
                .map_err(|e| js_err("copyToChannel", e))?;
        }
        Ok(buffer)
    }
}

/// The property an MP3's lead is kept in on its `AudioBuffer`.
const LEAD: &str = "__ddiLead";

/// Keeps on `buffer` the seconds its audio is to be heard later than its
/// samples say (earlier when negative): an MP3's lag against StepMania's
/// decode ([`AudioDecoder::align_mp3`]). [`WebAudio`] honours it when it
/// plays the buffer and in its duration.
fn set_lead(buffer: &AudioBuffer, seconds: f64) {
    let _ = Reflect::set(buffer, &JsValue::from_str(LEAD), &JsValue::from(seconds));
}

/// The lead [`set_lead`] kept on `buffer` (0 without one).
pub(crate) fn lead(buffer: &AudioBuffer) -> f64 {
    Reflect::get(buffer, &JsValue::from_str(LEAD))
        .ok()
        .and_then(|v| v.as_f64())
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
}

/// `window.__DDI_MP3`: the last MP3's tag, the lag applied (ms and
/// samples at the buffer's rate), whether the music told it or the
/// browser's measured lag stood in (and why), and how long it took.
fn report_mp3(
    first: &ddi_platform::mp3::FirstFrame,
    browser: ddi_platform::mp3::Browser,
    result: Result<ddi_platform::mp3::Alignment, ddi_platform::mp3::Unsure>,
    lag: f64,
    lead: f64,
    rate: u32,
    took_ms: f64,
) {
    use ddi_platform::mp3::{Tag, Unsure};
    let tag = match first.tag {
        Tag::Info => "Info",
        Tag::Xing => "Xing",
        Tag::Vbri => "VBRI",
        Tag::None => "none",
    };
    let how = match result {
        Ok(_) => "found",
        Err(Unsure::Silent) => "expected (no music in the first 40 s)",
        Err(Unsure::NoMatch { .. }) => "expected (the decodes differ)",
        Err(Unsure::Ambiguous { .. }) => "expected (the music cannot tell)",
    };
    let corr = match result {
        Ok(a) => Some(a.corr),
        Err(Unsure::NoMatch { best } | Unsure::Ambiguous { best, .. }) => Some(best),
        Err(Unsure::Silent) => None,
    };
    let o = js_sys::Object::new();
    let set = |k: &str, v: JsValue| {
        let _ = Reflect::set(&o, &JsValue::from_str(k), &v);
    };
    set("tag", JsValue::from_str(tag));
    set(
        "lameDelay",
        first.lame.map_or(JsValue::NULL, |l| JsValue::from(l.0)),
    );
    set("lagMs", JsValue::from(lag * 1000.0));
    set("frames", JsValue::from((lag * f64::from(rate)).round()));
    // What the buffer was moved by: the lag, less Chrome's for a work.
    set("leadMs", JsValue::from(lead * 1000.0));
    set("how", JsValue::from_str(how));
    set("browser", JsValue::from_str(&format!("{browser:?}")));
    set("corr", corr.map_or(JsValue::NULL, JsValue::from));
    set("tookMs", JsValue::from(took_ms));
    if let Some(w) = web_sys::window() {
        let _ = Reflect::set(&w, &JsValue::from_str("__DDI_MP3"), &o);
    }
    let what = match first.tag {
        Tag::None => "no tag".to_string(),
        _ => format!("{tag} tag"),
    };
    let line = if (lead - lag).abs() > 1e-9 {
        format!(
            "MP3 ({what}): the browser's decode is {:+.2} ms from StepMania's, {how}; moved \
             {:+.2} ms to where Chrome's has it (a work) ({:.0} ms)",
            lag * 1000.0,
            lead * 1000.0,
            took_ms
        )
    } else {
        format!(
            "MP3 ({what}): the browser's decode moved {:+.2} ms to match StepMania's, {how} \
             ({:.0} ms)",
            lag * 1000.0,
            took_ms
        )
    };
    if result.is_ok() {
        web_sys::console::info_1(&line.into());
    } else {
        web_sys::console::warn_1(&line.into());
    }
}

/// `window.__DDI_MP3` for an MP3 the built-in decoder decoded (as
/// StepMania does: no lag to find), and the lead it was given (a work's).
fn report_ours(first: &ddi_platform::mp3::FirstFrame, lead: f64) {
    use ddi_platform::mp3::Tag;
    let tag = match first.tag {
        Tag::Info => "Info",
        Tag::Xing => "Xing",
        Tag::Vbri => "VBRI",
        Tag::None => "none",
    };
    let o = js_sys::Object::new();
    let set = |k: &str, v: JsValue| {
        let _ = Reflect::set(&o, &JsValue::from_str(k), &v);
    };
    set("tag", JsValue::from_str(tag));
    set(
        "lameDelay",
        first.lame.map_or(JsValue::NULL, |l| JsValue::from(l.0)),
    );
    set("lagMs", JsValue::from(0.0));
    set("frames", JsValue::from(0.0));
    set("leadMs", JsValue::from(lead * 1000.0));
    set(
        "how",
        JsValue::from_str("decoded in wasm as StepMania does"),
    );
    set("browser", JsValue::NULL);
    set("corr", JsValue::NULL);
    set("tookMs", JsValue::from(0.0));
    if let Some(w) = web_sys::window() {
        let _ = Reflect::set(&w, &JsValue::from_str("__DDI_MP3"), &o);
    }
}

/// Whether a decoded buffer is all silence, judged from short windows
/// spread over its middle (intros and outros are often silent). Reads a
/// few thousand samples, not the song.
fn looks_silent(buffer: &AudioBuffer) -> bool {
    const WINDOWS: u32 = 24;
    const WINDOW: usize = 1024;
    let frames = buffer.length();
    if frames < 2 * WINDOW as u32 {
        return false;
    }
    let mut window = vec![0.0f32; WINDOW];
    for channel in 0..buffer.number_of_channels() as i32 {
        for i in 0..WINDOWS {
            let start = (u64::from(frames) * u64::from(1 + 2 * i) / u64::from(2 * WINDOWS)) as u32;
            let start = start.min(frames - WINDOW as u32);
            if buffer
                .copy_from_channel_with_start_in_channel(&mut window, channel, start)
                .is_err()
            {
                return false;
            }
            if window.iter().any(|v| v.abs() > 1e-5) {
                return false;
            }
        }
    }
    true
}

/// Whether `window.__DDI_FORCE_WASM_DECODE === true` (debug hook).
fn force_wasm_decode() -> bool {
    web_sys::window()
        .and_then(|w| Reflect::get(&w, &JsValue::from_str("__DDI_FORCE_WASM_DECODE")).ok())
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

impl AudioBackend for WebAudio {
    type Buffer = AudioBuffer;
    type Error = String;

    fn sample_rate(&self) -> f64 {
        f64::from(self.ctx.sample_rate())
    }

    fn master_ramp(&mut self, from: Option<f32>, to: f32, at: Option<f64>, seconds: f64) {
        let gain = self.master.gain();
        match at {
            // `fade_out`'s way: from now and the value now.
            None => {
                let now = self.ctx.current_time();
                let _ = gain.cancel_scheduled_values(now);
                let _ = gain.set_value_at_time(gain.value(), now);
                let _ = gain.linear_ramp_to_value_at_time(to, now + seconds.max(0.01));
            }
            // `fade_in_at`'s and `fade_out_at`'s.
            Some(at) => {
                let _ = gain.set_value_at_time(from.unwrap_or_else(|| gain.value()), at);
                let _ = gain.linear_ramp_to_value_at_time(to, at + seconds.max(0.01));
            }
        }
    }

    fn stop_all(&mut self) {
        WebAudio::stop_all(self);
    }

    fn running(&self) -> bool {
        WebAudio::running(self)
    }

    fn buffer_from_pcm(&self, channels: Vec<Vec<f32>>, rate: f64) -> Result<AudioBuffer, String> {
        let frames = channels.first().map_or(0, Vec::len) as u32;
        let buffer = self
            .ctx
            .create_buffer(channels.len().max(1) as u32, frames.max(1), rate as f32)
            .map_err(|e| js_err("createBuffer", e))?;
        for (i, c) in channels.iter().enumerate() {
            buffer
                .copy_to_channel(c, i as i32)
                .map_err(|e| js_err("copyToChannel", e))?;
        }
        Ok(buffer)
    }

    fn fingerprint(&self) -> Option<ddi_platform::DeviceFingerprint> {
        Some(WebAudio::fingerprint(self))
    }

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
        // An MP3's lead: its audio heard that much later than its
        // samples (or earlier).
        let offset = offset.max(0.0) - lead(buffer);
        let (when, offset) = if offset < 0.0 {
            (start_at_context_time - offset, 0.0)
        } else {
            (start_at_context_time, offset)
        };
        source
            .start_with_when_and_grain_offset(when, offset)
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

    fn duration(&self, buffer: &AudioBuffer) -> f64 {
        buffer.duration() + lead(buffer)
    }
}

thread_local! {
    /// The context song select handed to the play it opened, by its song
    /// and chart ([`keep_for_play`]).
    static FOR_PLAY: RefCell<Option<((String, usize), WebAudio)>> = const { RefCell::new(None) };
}

/// Keeps `audio` (made or resumed inside the gesture that opens the play,
/// or running already) for the play of `song`'s chart `chart`: its start
/// prompt decodes the music with it, so Start only starts the sound.
pub(crate) fn keep_for_play(song: &str, chart: usize, audio: WebAudio) {
    FOR_PLAY.with_borrow_mut(|k| *k = Some(((song.to_string(), chart), audio)));
}

/// The context kept for the play of `song`'s chart `chart`, if it is the
/// one it was kept for (another is let go).
pub(crate) fn take_for_play(song: &str, chart: usize) -> Option<WebAudio> {
    FOR_PLAY
        .with_borrow_mut(Option::take)
        .filter(|((s, c), _)| s == song && *c == chart)
        .map(|(_, audio)| audio)
}

impl Drop for WebAudio {
    fn drop(&mut self) {
        self.stop_all();
        // Browsers cap the number of live AudioContexts (Chrome: 6); release
        // the hardware instead of waiting for GC.
        let _ = self.ctx.close();
    }
}
