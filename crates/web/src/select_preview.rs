//! Song select's previews: the selected song's preview plays, looping,
//! until another is selected or the page goes.
//!
//! One audio context for the page, made (or resumed) inside the first user
//! gesture and every one after (a key, a click, a tap: listeners the page
//! adds): Safari starts audio only within a gesture, and a context that
//! runs keeps running. It is kept, suspended, when song select goes (a
//! play, options: no second live output beside the play's) and resumed
//! when the player comes back (from the results by Enter, a wheel then
//! changing songs), which the browsers allow once the page has had input
//! (its sticky activation: Chrome, Firefox; Safari is the maintainer's
//! check). Coming back with none (the session began on a play's
//! address), one is made at once where the page has had input. A selection starts its preview a
//! moment later, once the wheel has settled; the song's music and its
//! chart (for the beat) are read then. Before the first gesture the music
//! is decoded on an `OfflineAudioContext` (any page may make one; no audio
//! starts), so the gesture only starts the sound. The beat pulse follows
//! the music as heard ([`beat`]).

use std::cell::RefCell;
use std::rc::Rc;

use ddi_chart::TimingMap;
use gloo::timers::callback::Timeout;
use web_sys::AudioBuffer;

use crate::songs::{ManifestEntry, load_chart, load_preview_music};
use crate::web::audio::WebAudio;

/// Wait after a selection before its preview loads, ms.
const SETTLE_MS: u32 = 300;
/// The rate a preview decodes at before there is a context (the usual
/// output's, and Opus's own): a context at another rate resamples it as
/// it plays.
const OFFLINE_RATE: f32 = 48_000.0;

struct Playing {
    id: String,
    /// The context's time at the preview's first sample, and the song's
    /// second there.
    started: f64,
    from: f64,
    timing: Option<Rc<TimingMap>>,
    /// How long it plays, and its file's length (for tests).
    length: f64,
    duration: f64,
}

/// A song's music and timing, read, not decoded yet.
struct Fetched {
    load: u64,
    id: String,
    bytes: Vec<u8>,
    target: ddi_platform::mp3::Target,
    timing: Option<Rc<TimingMap>>,
}

#[derive(Default)]
struct State {
    audio: Option<WebAudio>,
    /// The song wanted, and loads told apart.
    wanted: Option<ManifestEntry>,
    load: u64,
    timer: Option<Timeout>,
    playing: Option<Playing>,
    /// The decoded preview, kept for its loops.
    buffer: Option<(String, AudioBuffer, Option<Rc<TimingMap>>)>,
    /// Read before there was a context (no gesture yet) where the browser
    /// has no `OfflineAudioContext`: decoded at the first one, at once.
    fetched: Option<Fetched>,
    /// The song whose music could not be read, and its load: read again
    /// at the next gesture (the network may be back).
    failed: Option<(u64, ManifestEntry)>,
    volume: f32,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State { volume: 0.8, ..State::default() });
}

/// In a gesture's handler: the context made, or resumed.
pub(crate) fn gesture() {
    let made = STATE.with(|s| {
        let mut s = s.borrow_mut();
        match &s.audio {
            Some(a) => {
                a.resume_now();
                false
            }
            None => match WebAudio::new() {
                Ok(a) => {
                    a.resume_now();
                    a.set_master_volume(s.volume);
                    s.audio = Some(a);
                    true
                }
                Err(_) => false,
            },
        }
    });
    // A read that failed is tried again.
    let failed = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let load = s.load;
        s.failed.take().filter(|(l, _)| *l == load)
    });
    if let Some((load, entry)) = failed {
        wasm_bindgen_futures::spawn_local(fetch(entry, load));
    }
    if made {
        // The song selected before there was a context: read already, it
        // decodes now (a read still going on, or the settle's wait, find
        // the context themselves).
        let fetched = STATE.with(|s| {
            let mut s = s.borrow_mut();
            let load = s.load;
            s.fetched.take().filter(|f| f.load == load)
        });
        if let Some(f) = fetched
            && let Some(decoder) = STATE.with(|s| s.borrow().audio.as_ref().map(WebAudio::decoder))
        {
            wasm_bindgen_futures::spawn_local(decode(decoder, f));
            return;
        }
    }
    // A preview that waited for the context (decoded before it, offline,
    // or while it was held back) starts now, in this handler (the context
    // resumes with it).
    let waiting = STATE.with(|s| {
        let s = s.borrow();
        s.playing.is_none() && s.timer.is_none() && s.buffer.is_some()
    });
    if waiting {
        play(true);
    }
}

pub(crate) fn set_volume(volume: f32) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.volume = volume;
        if let Some(a) = &s.audio {
            a.set_master_volume(volume);
        }
    });
}

/// The song selected now (`None`: none): what plays changes after a
/// moment.
pub(crate) fn select(entry: Option<ManifestEntry>) {
    let same = STATE.with(|s| {
        let s = s.borrow();
        s.wanted.as_ref().map(|e| &e.id) == entry.as_ref().map(|e| &e.id)
    });
    if same {
        return;
    }
    stop();
    let load = STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.wanted = entry.clone();
        s.load += 1;
        s.buffer = None;
        s.load
    });
    let Some(entry) = entry else {
        return;
    };
    let timer = Timeout::new(SETTLE_MS, move || {
        STATE.with(|s| s.borrow_mut().timer = None);
        wasm_bindgen_futures::spawn_local(fetch(entry, load));
    });
    STATE.with(|s| s.borrow_mut().timer = Some(timer));
}

/// Stops what plays (the page goes, an import starts).
pub(crate) fn stop() {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.timer = None;
        s.playing = None;
        if let Some(a) = s.audio.as_mut() {
            a.stop_all();
        }
    });
}

/// The page went: nothing plays; the context is kept, suspended, for the
/// next time song select shows ([`arrive`]).
pub(crate) fn close() {
    stop();
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.wanted = None;
        s.buffer = None;
        // A decode in flight ends (its worker with it) and keeps nothing.
        s.load += 1;
        // A song read for a gesture that never came: not kept.
        s.fetched = None;
        s.failed = None;
        if let Some(a) = &s.audio {
            a.suspend_now();
        }
    });
}

/// Song select shows: where the browser lets audio start now (the
/// results' Enter that came here counted: Chrome, Firefox), the context
/// is made or the one kept is resumed, so the selected song's preview
/// plays without another input; elsewhere the next gesture does it
/// ([`gesture`]), never asked before (Firefox would refuse, and say so).
pub(crate) fn arrive() {
    if crate::web::autoplay::may_start() {
        gesture();
    }
}

async fn fetch(entry: ManifestEntry, load: u64) {
    let current = || STATE.with(|s| s.borrow().load == load);
    let (music, chart) = futures_join(load_preview_music(&entry), load_chart(&entry.id)).await;
    if !current() {
        return;
    }
    let bytes = match music {
        Ok(b) => b,
        Err(e) => {
            web_sys::console::warn_1(&format!("preview: {e}").into());
            STATE.with(|s| s.borrow_mut().failed = Some((load, entry.clone())));
            return;
        }
    };
    // A work's MP3 where Chrome's decode has it (by its stored chart's
    // extension when the chart did not load).
    let target = chart.as_ref().map_or_else(
        |_| ddi_session::music_target_of(&entry),
        ddi_session::music_target,
    );
    let timing = chart.ok().map(|song| Rc::new(song.timing));
    let fetched = Fetched {
        load,
        id: entry.id.clone(),
        bytes,
        target,
        timing,
    };
    // No context yet (no gesture): decoded offline now, so the first
    // gesture only starts it; without an offline context, kept for the
    // first gesture, which decodes it at once ([`gesture`]).
    let decoder = STATE
        .with(|s| s.borrow().audio.as_ref().map(WebAudio::decoder))
        .or_else(|| crate::web::audio::AudioDecoder::offline(OFFLINE_RATE));
    let Some(decoder) = decoder else {
        STATE.with(|s| s.borrow_mut().fetched = Some(fetched));
        return;
    };
    decode(decoder, fetched).await;
}

/// Decodes what was read; plays it when the context runs.
async fn decode(decoder: crate::web::audio::AudioDecoder, f: Fetched) {
    let current = || STATE.with(|s| s.borrow().load == f.load);
    // Running already, or allowed to: decoding waits for it (a context
    // the browser holds back decodes as well, and plays at the gesture).
    if crate::web::autoplay::may_start() {
        decoder.resume().await;
    }
    // The MP3 alignment stops when the player moved on.
    let buffer = match decoder.decode_while(&f.bytes, f.target, current).await {
        Ok(b) => b,
        // Given up because the player moved on: nothing to say.
        Err(_) if !current() => return,
        Err(e) => {
            web_sys::console::warn_1(&format!("preview: {e}").into());
            return;
        }
    };
    if !current() {
        return;
    }
    STATE.with(|s| s.borrow_mut().buffer = Some((f.id.clone(), buffer, f.timing)));
    // Decoded offline while a gesture made the context: it plays once that
    // runs.
    if crate::web::autoplay::may_start()
        && let Some(live) = STATE.with(|s| s.borrow().audio.as_ref().map(WebAudio::decoder))
    {
        live.resume().await;
        if !current() {
            return;
        }
    }
    replay();
}

/// Waits for both.
async fn futures_join<A, B>(
    a: impl std::future::Future<Output = A>,
    b: impl std::future::Future<Output = B>,
) -> (A, B) {
    // Sequential is enough: the chart is small and read from the same
    // place.
    let a = a.await;
    let b = b.await;
    (a, b)
}

/// Plays the decoded preview from its start, and again when it ends.
fn replay() {
    play(false);
}

/// [`replay`]; `in_gesture`: inside a gesture's handler, whose resume
/// lets the context run (it may not say so yet).
fn play(in_gesture: bool) {
    let played = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let entry = s.wanted.clone()?;
        let (id, buffer, timing) = s.buffer.clone()?;
        if id != entry.id {
            return None;
        }
        let volume = s.volume;
        let audio = s.audio.as_mut()?;
        if !audio.running() && !in_gesture {
            // Waits for a gesture ([`gesture`]).
            return None;
        }
        // A separate preview file (`#PREVIEW`) from its start, else the
        // music's sample.
        let (from, length) = if entry.preview.is_some() {
            let length = ddi_session::preview::file_length(entry.preview_length);
            (0.0, length.min(buffer.duration()))
        } else {
            let from = entry
                .preview_start
                .clamp(0.0, (buffer.duration() - 1.0).max(0.0));
            (from, ddi_session::preview::length(entry.preview_length))
        };
        let started = audio.context().current_time() + ddi_session::preview::LEAD;
        audio.play_preview(&buffer, from, length, volume).ok()?;
        let load = s.load;
        s.playing = Some(Playing {
            id,
            started,
            from,
            timing,
            length,
            duration: buffer.duration(),
        });
        Some((load, length))
    });
    let Some((load, length)) = played else {
        return;
    };
    // Again once it is over (a pause between, as cabinets do).
    let timer = Timeout::new((length * 1000.0) as u32 + 600, move || {
        let again = STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.timer = None;
            s.load == load
        });
        if again {
            STATE.with(|s| s.borrow_mut().playing = None);
            replay();
        }
    });
    STATE.with(|s| s.borrow_mut().timer = Some(timer));
}

/// The beat of song `id` as its preview is heard now, when it plays (the
/// song's tempo changes and offset in it); `None` otherwise.
pub(crate) fn beat(id: &str) -> Option<f64> {
    STATE.with(|s| {
        let s = s.borrow();
        let p = s.playing.as_ref().filter(|p| p.id == id)?;
        let audio = s.audio.as_ref()?;
        let now = audio.context().current_time();
        if now < p.started {
            return None;
        }
        let latency = ddi_platform::AudioBackend::now(audio).output_latency;
        let song = p.from + (now - p.started) - latency;
        let timing = p.timing.as_ref()?;
        Some(timing.beat_at(song))
    })
}

/// A preview playing: its song, and from, length and its file's length.
pub(crate) type PlayingWindow = (String, [f64; 3]);

/// For the debug state: the song playing (from, length and its file's
/// length), and whether the context runs.
pub(crate) fn status() -> (Option<String>, Option<PlayingWindow>, bool) {
    STATE.with(|s| {
        let s = s.borrow();
        (
            s.wanted.as_ref().map(|e| e.id.clone()),
            s.playing
                .as_ref()
                .map(|p| (p.id.clone(), [p.from, p.length, p.duration])),
            s.audio.as_ref().is_some_and(WebAudio::running),
        )
    })
}
