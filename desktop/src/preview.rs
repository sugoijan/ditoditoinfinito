//! Song select's previews on desktop, as the web's
//! (`web/src/select_preview.rs`): the selected song's preview plays,
//! looping, until another is selected. A selection starts its preview a
//! moment later (300 ms, once the wheel has settled); the decoder thread
//! reads the music from `#SAMPLESTART` for `#SAMPLELENGTH` (12 s when the
//! simfile gives none, 30 s at most), and it plays with the web's 0.5 s
//! fades in and out, again 0.6 s after it ends. No gesture is needed:
//! it starts as soon as it is decoded. The beat pulse follows the music
//! as heard ([`Player::beat`]).
//!
//! For now the songs are the bundled ones, read from `assets/songs/`
//! (song select runs on sample data until the library on disk, step
//! 7.3); the made-up songs have no music and pulse by their BPM.
//!
//! The [`Player`] also plays the developer screen's tone and the
//! options' volume sample.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use ddi_chart::TimingMap;

use crate::audio::decoder::Decoder;
use crate::audio::{Audio, Sound};

/// Wait after a selection before its preview loads.
const SETTLE: Duration = Duration::from_millis(300);
/// Preview length when the simfile gives none, and the longest one played.
const DEFAULT_LENGTH: f64 = 12.0;
const MAX_LENGTH: f64 = 30.0;
/// The fades in and out.
const FADE: f64 = 0.5;
/// After the start is asked for, when it starts (context seconds).
const LEAD: f64 = 0.05;
/// The pause between loops.
const AGAIN: f64 = 0.6;

/// A song's preview, from its simfile.
#[derive(Clone, Debug)]
pub struct PreviewSong {
    pub id: String,
    pub music: PathBuf,
    /// Seconds into the music, and how long.
    pub from: f64,
    pub length: f64,
    pub timing: Rc<TimingMap>,
    /// Whose decode an MP3 lines up with (a work's: Chrome's).
    pub target: ddi_platform::mp3::Target,
}

impl PreviewSong {
    /// A bundled song's preview (`assets/songs/<id>/`), if it has music.
    pub fn bundled(id: &str) -> Option<PreviewSong> {
        let dir = crate::images::assets().join("songs").join(id);
        PreviewSong::from_dir(id, &dir)
    }

    /// The preview of the song in `dir` (its `.ssc`, else `.sm`).
    pub fn from_dir(id: &str, dir: &Path) -> Option<PreviewSong> {
        let files: Vec<PathBuf> = std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .collect();
        let chart = ["ssc", "sm"].iter().find_map(|want| {
            files.iter().find(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case(want))
            })
        })?;
        let ext = chart.extension()?.to_str()?.to_ascii_lowercase();
        let text = std::fs::read_to_string(chart).ok()?;
        let song = ddi_chart::formats::sm::parse_simfile(&text, &ext).ok()?;
        let music = dir.join(song.music.as_deref()?);
        Some(PreviewSong::of(
            id,
            music,
            song.preview_start,
            song.preview_length,
            song.timing,
        ))
    }

    /// A preview of `music` from the simfile's `#SAMPLESTART` and
    /// `#SAMPLELENGTH` (the defaults where it gives none).
    pub fn of(id: &str, music: PathBuf, start: f64, length: f64, timing: TimingMap) -> PreviewSong {
        let length = if length > 1.0 {
            length.min(MAX_LENGTH)
        } else {
            DEFAULT_LENGTH
        };
        PreviewSong {
            id: id.to_string(),
            music,
            from: start.max(0.0),
            length,
            timing: Rc::new(timing),
            target: Default::default(),
        }
    }
}

/// A decoded preview.
struct Loaded {
    song: PreviewSong,
    sound: Sound,
}

/// What plays: since which context time, from which song second.
struct Playing {
    id: String,
    started: f64,
    from: f64,
    timing: Rc<TimingMap>,
    /// When to play it again.
    again: Instant,
    /// When its first frame is heard, until then (`started` is set).
    heard_at: Option<Instant>,
}

/// Previews and the shell's sounds.
pub struct Player {
    pub audio: Audio,
    decoder: Decoder,
    /// Finds a song's preview by id.
    find: fn(&str) -> Option<PreviewSong>,
    wanted: Option<String>,
    settle: Option<Instant>,
    job: Option<(u64, PreviewSong)>,
    loaded: Option<Loaded>,
    playing: Option<Playing>,
    /// The developer screen's tone, made once.
    tone: Option<Sound>,
    /// A preview began to be heard since [`Player::take_started`] (the
    /// screen draws a frame to start its pulse).
    started: bool,
    /// Stopped for a play ([`Player::pause`]): the wanted preview plays
    /// again when song select shows again ([`Player::resume`]).
    paused: bool,
    /// Decodes and how long they took, for the record.
    pub decoded: Vec<(String, Duration)>,
    pub errors: Vec<String>,
}

pub type SharedPlayer = Rc<RefCell<Player>>;

impl Player {
    /// On the default output at master volume `volume`.
    pub fn new(volume: f32) -> Player {
        Player::with(Audio::new(volume), PreviewSong::bundled)
    }

    pub fn with(audio: Audio, find: fn(&str) -> Option<PreviewSong>) -> Player {
        Player {
            audio,
            decoder: Decoder::new(),
            find,
            wanted: None,
            settle: None,
            job: None,
            loaded: None,
            playing: None,
            tone: None,
            started: false,
            paused: false,
            decoded: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// The song shown changed (`None`: none): what plays changes after
    /// a moment.
    pub fn select(&mut self, id: Option<&str>) {
        if self.wanted.as_deref() == id {
            return;
        }
        self.stop();
        self.wanted = id.map(str::to_string);
        self.loaded = None;
        self.job = None;
        self.settle = id.map(|_| Instant::now() + SETTLE);
    }

    /// A library revision changed beneath the selected id. Future preview
    /// playback must decode its new immutable path, even if the id is stable.
    pub fn invalidate(&mut self, ids: &[String]) {
        if self.wanted.as_ref().is_some_and(|id| ids.contains(id)) {
            self.stop();
            self.loaded = None;
            self.job = None;
            self.settle = Some(Instant::now() + SETTLE);
        }
    }

    /// The wanted song's preview may be found now (it was being made
    /// ready on a thread, `crate::library::preview_song`): looked for
    /// again unless it plays or decodes.
    pub fn refind(&mut self) {
        if self.wanted.is_some()
            && self.loaded.is_none()
            && self.job.is_none()
            && self.settle.is_none()
        {
            self.settle = Some(Instant::now());
        }
    }

    /// Stops what plays (the preview stays wanted, and plays again
    /// after its pause).
    pub fn stop(&mut self) {
        if self.playing.take().is_some() {
            self.audio.stop_all();
        }
    }

    /// A play starts: what plays stops, until [`Player::resume`].
    pub fn pause(&mut self) {
        self.stop();
        self.paused = true;
    }

    /// Whether a play paused the preview (and nothing resumed it yet).
    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Song select shows again (after a play's results, its prompt let
    /// go): the wanted song's preview plays again, at once when it is
    /// decoded.
    pub fn resume(&mut self) {
        if !std::mem::take(&mut self.paused) || self.playing.is_some() {
            return;
        }
        let ready = self
            .loaded
            .as_ref()
            .is_some_and(|l| self.wanted.as_deref() == Some(l.song.id.as_str()));
        if ready {
            self.replay();
        } else if self.wanted.is_some() && self.job.is_none() {
            self.settle = Some(Instant::now());
        }
    }

    /// Nothing wanted, nothing playing.
    pub fn close(&mut self) {
        self.select(None);
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.audio.set_master_volume(volume);
    }

    /// The options' volume sample: song `id`'s preview at `volume`, at
    /// once (`play`), or stopped; whether it plays now.
    pub fn sample(&mut self, id: &str, play: bool, volume: f32) -> bool {
        if !play {
            self.close();
            return false;
        }
        self.set_volume(volume);
        self.select(Some(id));
        self.settle = Some(Instant::now());
        (self.find)(id).is_some()
    }

    /// Moves things on: the settle wait, the decode, the loops, the
    /// device. When to call again (`None`: on the next event).
    pub fn tick(&mut self) -> Option<Instant> {
        self.audio.tick();
        let now = Instant::now();
        if let Some(at) = self.settle {
            if now < at {
                return Some(at);
            }
            self.settle = None;
            if let Some(song) = self.wanted.as_deref().and_then(self.find) {
                let id = self.decoder.decode_for(
                    &song.music,
                    song.from,
                    Some(song.from + song.length),
                    self.audio.sample_rate(),
                    song.target,
                );
                self.job = Some((id, song));
            }
        }
        while let Some(done) = self.decoder.poll() {
            let Some((id, song)) = self.job.take_if(|(id, _)| *id == done.id) else {
                continue;
            };
            let _ = id;
            match done.result {
                Ok(sound) => {
                    self.decoded.push((song.id.clone(), done.took));
                    self.loaded = Some(Loaded { song, sound });
                    self.replay();
                }
                Err(e) => self.errors.push(format!("{}: {e}", song.id)),
            }
        }
        if let Some(p) = &self.playing
            && now >= p.again
        {
            self.playing = None;
            self.replay();
        }
        if let Some(p) = &mut self.playing
            && p.heard_at.is_some_and(|at| now >= at)
        {
            p.heard_at = None;
            self.started = true;
        }
        let mut next = self
            .playing
            .as_ref()
            .map(|p| p.heard_at.unwrap_or(p.again).min(p.again));
        if self.decoder.busy() {
            let soon = now + Duration::from_millis(8);
            next = Some(next.map_or(soon, |n| n.min(soon)));
        }
        // The device is looked at once a second: a stream that failed,
        // or no device at all, is tried again.
        let check = now + Duration::from_secs(1);
        Some(next.map_or(check, |n| n.min(check)))
    }

    /// Plays the decoded preview from its start, with its fades.
    fn replay(&mut self) {
        let Some(l) = &self.loaded else { return };
        if self.wanted.as_deref() != Some(l.song.id.as_str()) {
            return;
        }
        let length = l.sound.duration();
        if length <= 0.0 {
            return;
        }
        let start = self.audio.context_time() + LEAD;
        let end = start + length.max(2.0 * FADE);
        let id = self.audio.play_sound(&l.sound, start, 0.0, 0.0);
        self.audio.gain_at(id, 0.0, start);
        self.audio.gain_ramp(id, 1.0, start + FADE);
        self.audio.gain_at(id, 1.0, end - FADE);
        self.audio.gain_ramp(id, 0.0, end);
        self.audio.stop_at(id, Some(end));
        let latency = ddi_platform::AudioBackend::now(&self.audio).output_latency;
        self.playing = Some(Playing {
            id: l.song.id.clone(),
            started: start,
            from: l.song.from,
            timing: l.song.timing.clone(),
            again: Instant::now() + Duration::from_secs_f64(end - start + AGAIN - LEAD),
            heard_at: Some(Instant::now() + Duration::from_secs_f64(LEAD + latency.max(0.0))),
        });
    }

    /// Whether a preview began to be heard since last asked.
    pub fn take_started(&mut self) -> bool {
        std::mem::take(&mut self.started)
    }

    /// The beat of song `id` as its preview is heard now; `None` when it
    /// does not play (or not yet: its first frame is still on its way).
    pub fn beat(&self, id: &str) -> Option<f64> {
        let p = self.playing.as_ref().filter(|p| p.id == id)?;
        let s = ddi_platform::AudioBackend::now(&self.audio);
        let heard = s.context_time - s.output_latency;
        if heard < p.started {
            return None;
        }
        Some(p.timing.beat_at(p.from + heard - p.started))
    }

    /// The song wanted and the one playing.
    pub fn status(&self) -> (Option<&str>, Option<&str>) {
        (
            self.wanted.as_deref(),
            self.playing.as_ref().map(|p| p.id.as_str()),
        )
    }

    /// The developer screen's tone: 0.2 s of 880 Hz, 5 ms in, then
    /// fading, peaking at −14 dBFS (the web's), through the master.
    pub fn beep(&mut self) {
        if self.tone.is_none() {
            let rate = 48_000;
            let n = rate as usize / 5;
            let samples: Vec<f32> = (0..n)
                .map(|i| {
                    let t = i as f32 / rate as f32;
                    let envelope = (t / 0.005).min(1.0) * (-t * 18.0).exp();
                    (t * 880.0 * std::f32::consts::TAU).sin() * 0.2 * envelope
                })
                .collect();
            self.tone = self.audio.buffer_from_pcm(vec![samples], rate).ok();
        }
        if let Some(tone) = &self.tone {
            let at = self.audio.context_time();
            self.audio.play_sound(tone, at, 0.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_replaced_selected_song_restarts_its_preview_without_changing_pause() {
        let mut p = Player::with(Audio::silent(0.0), PreviewSong::bundled);
        p.select(Some("replacement-test"));
        p.pause();
        p.settle = None;
        p.invalidate(&["other-song".into()]);
        assert!(p.settle.is_none());
        p.invalidate(&["replacement-test".into()]);
        assert!(p.settle.is_some());
        assert_eq!(p.wanted.as_deref(), Some("replacement-test"));
        assert!(p.paused);
    }

    #[test]
    fn bundled_songs_have_previews() {
        for id in [
            "beatucada",
            "into-my-dream",
            "some-things-must",
            "sweeteners",
        ] {
            let p = PreviewSong::bundled(id).unwrap_or_else(|| panic!("{id}"));
            assert!(p.music.is_file(), "{}", p.music.display());
            assert!(
                p.length > 1.0 && p.length <= MAX_LENGTH,
                "{id}: {}",
                p.length
            );
            assert!(p.from >= 0.0);
        }
        assert!(PreviewSong::bundled("not-a-song").is_none());
    }

    #[test]
    fn a_selection_settles_then_decodes_then_plays_and_pulses() {
        let mut p = Player::with(Audio::silent(0.8), PreviewSong::bundled);
        p.select(Some("sweeteners"));
        assert_eq!(p.status(), (Some("sweeteners"), None));
        // Settling: nothing yet.
        assert!(p.tick().is_some());
        assert!(p.job.is_none());
        std::thread::sleep(SETTLE);
        let start = Instant::now();
        while p.playing.is_none() {
            p.tick();
            assert!(start.elapsed() < Duration::from_secs(30), "{:?}", p.errors);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(p.status(), (Some("sweeteners"), Some("sweeteners")));
        let (id, took) = &p.decoded[0];
        assert_eq!(id, "sweeteners");
        assert!(took.as_secs_f64() < 20.0);
        let len = p.loaded.as_ref().unwrap().sound.duration();
        let song = PreviewSong::bundled("sweeteners").unwrap();
        assert!((len - song.length).abs() < 0.01, "{len}");
        // Silent backend: the clock is the host's, no latency; the beat
        // comes once its first frame is heard.
        assert!(!p.take_started());
        std::thread::sleep(Duration::from_millis(80));
        // Heard now: the shell is told once, to draw.
        p.tick();
        assert!(p.take_started());
        assert!(!p.take_started());
        let beat = p.beat("sweeteners").unwrap();
        let want = song.timing.beat_at(song.from);
        assert!(beat >= want && beat < want + 1.0, "{beat} {want}");
        assert_eq!(p.beat("beatucada"), None);
        // Another selection stops it.
        p.select(Some("beatucada"));
        assert_eq!(p.status(), (Some("beatucada"), None));
        p.close();
        assert_eq!(p.status(), (None, None));
    }

    #[test]
    fn a_play_pauses_the_preview_and_song_select_plays_it_again() {
        let mut p = Player::with(Audio::silent(0.8), PreviewSong::bundled);
        p.select(Some("sweeteners"));
        let start = Instant::now();
        while p.playing.is_none() {
            p.tick();
            assert!(start.elapsed() < Duration::from_secs(30), "{:?}", p.errors);
            std::thread::sleep(Duration::from_millis(5));
        }
        // A play: silence, and it stays so while the play runs.
        p.pause();
        assert!(p.paused());
        assert_eq!(p.status(), (Some("sweeteners"), None));
        for _ in 0..5 {
            p.tick();
        }
        assert_eq!(p.status(), (Some("sweeteners"), None));
        // Back on song select: the same song's preview, at once (no
        // selection needed).
        p.resume();
        assert!(!p.paused());
        assert_eq!(p.status(), (Some("sweeteners"), Some("sweeteners")));
    }
}
