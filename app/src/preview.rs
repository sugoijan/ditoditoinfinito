//! Song previews, played through Web Audio like gameplay: `<audio>` cannot
//! play Ogg in Safari, while `decodeAudioData` (with the wasm fallback) can.
//! Used by the song list and by the volume sample in the options.
//!
//! [`Preview::start`] must run inside the click handler itself: it creates
//! the audio context, and Safari only lets a page start audio synchronously
//! within the gesture (Yew runs `update` later, in a microtask).

use std::future::Future;

use gloo::timers::callback::Timeout;
use web_sys::AudioBuffer;

use crate::songs::{ManifestEntry, load_music};
use crate::web::audio::WebAudio;

/// Preview length when the simfile gives none, and the longest one played.
const DEFAULT_LENGTH: f64 = 12.0;
const MAX_LENGTH: f64 = 30.0;

pub(crate) struct Preview {
    /// Song id.
    pub(crate) id: String,
    entry: ManifestEntry,
    audio: WebAudio,
    /// Fires the end callback after the preview's length.
    timer: Option<Timeout>,
}

impl Preview {
    /// Creates the audio context. Call inside the click handler.
    pub(crate) fn start(entry: &ManifestEntry) -> Result<Preview, String> {
        let audio = WebAudio::new()?;
        audio.resume_now();
        Ok(Preview {
            id: entry.id.clone(),
            entry: entry.clone(),
            audio,
            timer: None,
        })
    }

    /// Loads and decodes the song. The future holds no borrow of the
    /// preview: dropping the preview meanwhile closes the context, the
    /// decode gives up, and its result is ignored by id.
    pub(crate) fn load(&self) -> impl Future<Output = Result<AudioBuffer, String>> + 'static {
        let decoder = self.audio.decoder();
        let entry = self.entry.clone();
        async move {
            let bytes = load_music(&entry).await?;
            decoder.resume().await;
            decoder.decode(&bytes).await
        }
    }

    /// Plays the song's preview slice at `volume`; `on_end` runs once it is
    /// over.
    pub(crate) fn play(
        &mut self,
        buffer: &AudioBuffer,
        volume: f32,
        on_end: impl FnOnce() + 'static,
    ) -> Result<(), String> {
        let length = if self.entry.preview_length > 1.0 {
            self.entry.preview_length.min(MAX_LENGTH)
        } else {
            DEFAULT_LENGTH
        };
        let start = self
            .entry
            .preview_start
            .clamp(0.0, (buffer.duration() - 1.0).max(0.0));
        self.audio.play_preview(buffer, start, length, volume)?;
        self.timer = Some(Timeout::new((length * 1000.0) as u32 + 200, on_end));
        Ok(())
    }

    /// Changes the volume while it plays.
    pub(crate) fn set_volume(&self, volume: f32) {
        self.audio.set_master_volume(volume);
    }
}
