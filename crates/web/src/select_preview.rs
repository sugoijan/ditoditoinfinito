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
//! chart (whose decode an MP3 lines up with) are read then. Before the
//! first gesture the music is decoded on an `OfflineAudioContext` (any
//! page may make one; no audio starts), so the gesture only starts the
//! sound. A play opened from song select takes
//! the context ([`hand_over`]), running, to ready its music before Start.

use std::cell::RefCell;

use gloo::timers::callback::Timeout;
use web_sys::AudioBuffer;

use crate::songs::{ManifestEntry, load_chart, load_preview_music};
use crate::web::audio::WebAudio;

/// The rate a preview decodes at before there is a context (the usual
/// output's, and Opus's own): a context at another rate resamples it as
/// it plays.
const OFFLINE_RATE: f32 = 48_000.0;

struct Playing {
    id: String,
    /// The song's second it starts at.
    from: f64,
    /// How long it plays, and its file's length (for tests).
    length: f64,
    duration: f64,
}

/// A song's music, read, not decoded yet.
struct Fetched {
    load: u64,
    id: String,
    bytes: Vec<u8>,
    target: ddi_platform::mp3::Target,
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
    buffer: Option<(String, AudioBuffer)>,
    /// Read before there was a context (no gesture yet) where the browser
    /// has no `OfflineAudioContext`: decoded at the first one, at once.
    fetched: Option<Fetched>,
    /// The song whose music could not be read, and its load: read again
    /// at the next gesture (the network may be back).
    failed: Option<(u64, ManifestEntry)>,
    volume: f32,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        volume: ddi_ui::state::settings::DEFAULT_VOLUME,
        ..State::default()
    });
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
    let timer = Timeout::new(ddi_session::preview::SETTLE_MS, move || {
        STATE.with(|s| s.borrow_mut().timer = None);
        wasm_bindgen_futures::spawn_local(fetch(entry, load));
    });
    STATE.with(|s| s.borrow_mut().timer = Some(timer));
}

/// Stops what plays (the page goes, an import starts): the preview,
/// not the announcer's line.
pub(crate) fn stop() {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.timer = None;
        s.playing = None;
        if let Some(a) = s.audio.as_mut() {
            a.stop_music();
        }
    });
}

/// What a play opening from song select stops ([`hand_over`]): the
/// preview, and the intro still speaking (the announcer studio's scenes
/// on song select, which keep the context).
pub(crate) fn hush() {
    stop();
    STATE.with(|s| {
        if let Some(a) = s.borrow_mut().audio.as_mut() {
            a.stop_all();
        }
    });
}

/// Speaks `buffer` at `share` of the music's volume on song select's
/// context, running or about to (made in the press that showed it):
/// the announcer studio's scenes. Its length, if it was scheduled.
pub(crate) fn speak_staged(buffer: &AudioBuffer, share: f32) -> Option<f64> {
    use ddi_platform::AudioBackend;
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let a = s.audio.as_mut()?;
        let at = a.context().current_time() + 0.02;
        a.play_voice(buffer, at, share).ok()?;
        Some(buffer.duration())
    })
}

/// `channels` at `rate` as a buffer of song select's context (`None`:
/// none yet).
pub(crate) fn make_buffer(channels: Vec<Vec<f32>>, rate: u32) -> Option<AudioBuffer> {
    use ddi_platform::AudioBackend;
    STATE.with(|s| {
        s.borrow()
            .audio
            .as_ref()?
            .buffer_from_pcm(channels, f64::from(rate))
            .ok()
    })
}

/// Speaks an announcer's `buffer` now at `share` of the music's volume
/// `volume`: on song select's context where it runs (its master at the
/// music's volume), else on the one a play took from it
/// ([`hand_over`]; at full volume); whether it did.
pub(crate) fn speak(buffer: &AudioBuffer, share: f32, volume: f32) -> bool {
    use ddi_platform::AudioBackend;
    let here = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let a = s.audio.as_mut().filter(|a| a.running())?;
        let at = a.context().current_time() + 0.02;
        Some(a.play_voice(buffer, at, share).is_ok())
    });
    here.or_else(|| {
        crate::web::audio::with_kept_for_play(|a| {
            let at = a.context().current_time() + 0.02;
            a.play_voice(buffer, at, volume * share).is_ok()
        })
    })
    .unwrap_or(false)
}

/// A play opens: the page's context for it, running (made or resumed in
/// this gesture, `in_gesture`, or running since an earlier one: a pad's
/// press); `None` where none may start. Song select makes another when it
/// shows again ([`arrive`]).
pub(crate) fn hand_over(in_gesture: bool) -> Option<WebAudio> {
    stop();
    let audio = STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.audio.as_ref().is_some_and(|a| in_gesture || a.running()) {
            s.audio.take()
        } else {
            None
        }
    });
    let mut audio = match audio {
        Some(a) => Some(a),
        None if in_gesture => WebAudio::new().ok(),
        None => None,
    }?;
    // The intro still speaking stops (the chosen song's comment comes).
    audio.stop_all();
    audio.resume_now();
    // A play's master is at full volume (its sounds have their own).
    audio.set_master_volume(1.0);
    Some(audio)
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
    let fetched = Fetched {
        load,
        id: entry.id.clone(),
        bytes,
        target,
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
    STATE.with(|s| s.borrow_mut().buffer = Some((f.id.clone(), buffer)));
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
        let (id, buffer) = s.buffer.clone()?;
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
            let from = ddi_session::preview::start(entry.preview_start, buffer.duration());
            (from, ddi_session::preview::length(entry.preview_length))
        };
        audio.play_preview(&buffer, from, length, volume).ok()?;
        let load = s.load;
        s.playing = Some(Playing {
            id,
            from,
            length,
            duration: buffer.duration(),
        });
        Some((load, length))
    });
    let Some((load, length)) = played else {
        return;
    };
    // Again once it is over (a pause between, as cabinets do).
    let again = (length * 1000.0) as u32 + ddi_session::preview::AGAIN_MS;
    let timer = Timeout::new(again, move || {
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
