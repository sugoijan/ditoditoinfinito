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
//! chart (for the beat) are read then. The beat pulse follows the music
//! as heard ([`beat`]).

use std::cell::RefCell;
use std::rc::Rc;

use ddi_chart::TimingMap;
use gloo::timers::callback::Timeout;
use web_sys::AudioBuffer;

use crate::songs::{ManifestEntry, load_chart, load_music};
use crate::web::audio::WebAudio;

/// Wait after a selection before its preview loads, ms.
const SETTLE_MS: u32 = 300;
/// Preview length when the simfile gives none, and the longest one played.
const DEFAULT_LENGTH: f64 = 12.0;
const MAX_LENGTH: f64 = 30.0;

struct Playing {
    id: String,
    /// The context's time at the preview's first sample, and the song's
    /// second there.
    started: f64,
    from: f64,
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
    if made {
        // The song selected before there was a context: its preview now.
        let wanted = STATE.with(|s| s.borrow_mut().wanted.take());
        select(wanted);
        return;
    }
    // A preview that waited for the context to run starts now.
    let waiting = STATE.with(|s| {
        let s = s.borrow();
        s.playing.is_none() && s.timer.is_none() && s.buffer.is_some()
    });
    if waiting {
        replay();
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
        if let Some(a) = &s.audio {
            a.suspend_now();
        }
    });
}

/// Song select shows: without a context yet, one is made now where the
/// page has had the player's input before (the results' Enter that came
/// here), so the selected song's preview plays without another.
pub(crate) fn arrive() {
    let none = STATE.with(|s| s.borrow().audio.is_none());
    if none && sticky_activation() {
        gesture();
        return;
    }
    // The context kept from before, suspended when song select went: on
    // again (a browser that wants a fresh gesture for it gets the next
    // one, [`gesture`]).
    STATE.with(|s| {
        if let Some(a) = &s.borrow().audio {
            a.resume_now();
        }
    });
}

/// Whether the page has had a user gesture (`navigator.userActivation
/// .hasBeenActive`; `false` where the browser does not say).
fn sticky_activation() -> bool {
    let Some(nav) = web_sys::window().map(|w| w.navigator()) else {
        return false;
    };
    js_sys::Reflect::get(&nav, &"userActivation".into())
        .ok()
        .filter(|u| u.is_object())
        .and_then(|u| js_sys::Reflect::get(&u, &"hasBeenActive".into()).ok())
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

async fn fetch(entry: ManifestEntry, load: u64) {
    let current = || STATE.with(|s| s.borrow().load == load);
    let decoder = STATE.with(|s| s.borrow().audio.as_ref().map(WebAudio::decoder));
    let Some(decoder) = decoder else {
        // No context yet (no gesture): the next gesture starts it.
        return;
    };
    let (music, chart) = futures_join(load_music(&entry), load_chart(&entry.id)).await;
    if !current() {
        return;
    }
    let bytes = match music {
        Ok(b) => b,
        Err(e) => {
            web_sys::console::warn_1(&format!("preview: {e}").into());
            return;
        }
    };
    // A work's MP3 where Chrome's decode has it (by its stored chart's
    // extension when the chart did not load).
    let target = chart.as_ref().map_or_else(
        |_| {
            if entry.chart.ends_with(ddi_library::danoni::STORED_EXT) {
                ddi_platform::mp3::Target::Chromium
            } else {
                ddi_platform::mp3::Target::StepMania
            }
        },
        ddi_session::music_target,
    );
    let timing = chart.ok().map(|song| Rc::new(song.timing));
    decoder.resume().await;
    // The MP3 alignment stops when the player moved on.
    let buffer = match decoder.decode_while(&bytes, target, current).await {
        Ok(b) => b,
        Err(e) => {
            web_sys::console::warn_1(&format!("preview: {e}").into());
            return;
        }
    };
    if !current() {
        return;
    }
    STATE.with(|s| s.borrow_mut().buffer = Some((entry.id.clone(), buffer, timing)));
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
    let played = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let entry = s.wanted.clone()?;
        let (id, buffer, timing) = s.buffer.clone()?;
        if id != entry.id {
            return None;
        }
        let volume = s.volume;
        let audio = s.audio.as_mut()?;
        if !audio.running() {
            // Waits for a gesture ([`gesture`]).
            return None;
        }
        let length = if entry.preview_length > 1.0 {
            entry.preview_length.min(MAX_LENGTH)
        } else {
            DEFAULT_LENGTH
        };
        let from = entry
            .preview_start
            .clamp(0.0, (buffer.duration() - 1.0).max(0.0));
        let started = audio.context().current_time() + 0.05;
        audio.play_preview(&buffer, from, length, volume).ok()?;
        let load = s.load;
        s.playing = Some(Playing {
            id,
            started,
            from,
            timing,
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

/// For the debug state: the song playing, and whether the context runs.
pub(crate) fn status() -> (Option<String>, Option<String>, bool) {
    STATE.with(|s| {
        let s = s.borrow();
        (
            s.wanted.as_ref().map(|e| e.id.clone()),
            s.playing.as_ref().map(|p| p.id.clone()),
            s.audio.as_ref().is_some_and(WebAudio::running),
        )
    })
}
