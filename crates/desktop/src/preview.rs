//! Song select's previews on desktop, as the web's
//! (`crates/web/src/select_preview.rs`): the selected song's preview plays,
//! looping, until another is selected. A selection starts its preview a
//! moment later (300 ms, once the wheel has settled); the decoder thread
//! reads the music from `#SAMPLESTART` for `#SAMPLELENGTH` (12 s when the
//! simfile gives none, 30 s at most), and it plays with the web's 0.5 s
//! fades in and out, again 0.6 s after it ends. No gesture is needed:
//! it starts as soon as it is decoded.
//!
//! The songs are the bundled ones, read from `assets/songs/`, and the
//! library's (`crate::library::preview_song`); the made-up songs of the
//! sample screens have no music.
//!
//! The [`Player`] also plays the developer screen's tone, the options'
//! volume sample, the chosen announcer's sample line
//! ([`Player::announce`]) and song select's lines
//! ([`Player::announce_menu`]), these on the voice's bus.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use ddi_library::announcer::Line;
use ddi_session::announcer::MenuVoice;
use ddi_session::announcer::scene::director::{StagedVoice, Told};
use ddi_session::preview::{AGAIN_MS, FADE, LEAD, SETTLE_MS};
use ddi_ui::state::settings::Settings;

use crate::audio::decoder::Decoder;
use crate::audio::{Audio, Sound};

/// Wait after a selection before its preview loads.
const SETTLE: Duration = Duration::from_millis(SETTLE_MS as u64);
/// The pause between loops, seconds.
const AGAIN: f64 = AGAIN_MS as f64 / 1000.0;

/// A song's preview, from its simfile.
#[derive(Clone, Debug)]
pub struct PreviewSong {
    pub id: String,
    pub music: PathBuf,
    /// Seconds into the music, and how long.
    pub from: f64,
    pub length: f64,
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
        ))
    }

    /// A preview of `music` from the simfile's `#SAMPLESTART` and
    /// `#SAMPLELENGTH` (the defaults where it gives none).
    pub fn of(id: &str, music: PathBuf, start: f64, length: f64) -> PreviewSong {
        PreviewSong {
            id: id.to_string(),
            music,
            from: start.max(0.0),
            length: ddi_session::preview::length(length),
            target: Default::default(),
        }
    }
}

/// A decoded preview.
struct Loaded {
    song: PreviewSong,
    sound: Sound,
}

/// What plays, and when it plays again.
struct Playing {
    id: String,
    again: Instant,
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
    /// Stopped for a play ([`Player::pause`]): the wanted preview plays
    /// again when song select shows again ([`Player::resume`]).
    paused: bool,
    /// Decodes and how long they took, for the record.
    pub decoded: Vec<(String, Duration)>,
    pub errors: Vec<String>,
    /// The announcer's sample line read and decoded on a thread: its
    /// sound's path and share of the volume.
    voice: Option<(Decoding, String, f32)>,
    /// The sample lines spoken, for the record.
    pub spoken: Vec<String>,
    /// Song select's lines of the announcer chosen, decoded when first
    /// asked for (`docs/plans/announcers.md`, step 5).
    menu: Option<(MenuVoice<Sound>, crate::play::voice::VoiceLoad)>,
    /// What the menu's lines are timed on.
    epoch: Instant,
    /// The announcer studio's voice, while its scene shows song select.
    staged: Option<StagedVoice<Sound>>,
}

pub type SharedPlayer = Rc<RefCell<Player>>;

/// Who is told of a line the studio's voice was asked for, the line, and
/// what spoke for it and for how long ([`Player::announce_menu`]).
pub type StagedLine = (Told, Line, Option<(Line, f64)>);

/// A sound being decoded on a thread.
type Decoding = std::sync::mpsc::Receiver<Result<Sound, String>>;

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
            paused: false,
            decoded: Vec::new(),
            errors: Vec::new(),
            voice: None,
            spoken: Vec::new(),
            menu: None,
            epoch: Instant::now(),
            staged: None,
        }
    }

    /// The announcer studio's voice speaks song select's lines from now
    /// (`None`: the announcer chosen again).
    pub fn stage(&mut self, staged: Option<StagedVoice<Sound>>) {
        self.staged = staged;
    }

    /// What a play opening stops: the preview, and what is said.
    pub fn hush(&mut self) {
        self.pause();
        self.audio.stop_all();
    }

    /// Song select's `line` (`UiRequest::Announce`), where the announcer
    /// chosen has it: its sounds decoded the first time (the line spoken
    /// once decoded, if that is soon), at its share of the volume. The
    /// studio's voice instead while it stages song select: who to tell,
    /// to be told outside this player's borrow.
    pub fn announce_menu(&mut self, line: Line) -> Option<StagedLine> {
        if let Some(s) = self.staged.as_mut() {
            let told = s.told.clone();
            let found = s.voice.next(line).map(|(f, b)| (f, b.clone()));
            let volume = found.as_ref().map_or(s.share, |(f, _)| s.volume(*f));
            let spoke = found.map(|(found, sound)| {
                let at = self.audio.context_time() + LEAD;
                self.audio.play_voice_sound(&sound, at, volume);
                self.spoken.push(found.folder());
                (found, sound.duration())
            });
            return Some((told, line, spoke));
        }
        let settings = Settings::cached();
        let key = settings.announcer.clone()?;
        if self.menu.as_ref().is_none_or(|(m, _)| m.key() != key) {
            let load = crate::play::voice::VoiceLoad::start(
                key.clone(),
                crate::library::all_folders(),
                crate::play::voice::For::Menu,
                self.audio.sample_rate(),
                None,
            );
            self.menu = Some((MenuVoice::new(key, load.voice.clone()), load));
        }
        let now = self.epoch.elapsed().as_secs_f64();
        let found = self.menu.as_mut().and_then(|(m, load)| {
            load.poll();
            m.ask(line, now)
        });
        self.speak_menu(found, settings.announcer_volume);
        None
    }

    /// Speaks song select's line found, if any, at `share` of the volume.
    fn speak_menu(&mut self, found: Option<(ddi_library::announcer::Line, Sound)>, share: f32) {
        let Some((found, sound)) = found else {
            return;
        };
        let at = self.audio.context_time() + LEAD;
        self.audio.play_voice_sound(&sound, at, share);
        self.spoken.push(found.folder());
    }

    /// Speaks `sound` (a path of its lines) of announcer `a` at `share`
    /// of the volume, once read and decoded on a thread; a line asked for
    /// meanwhile replaces it.
    pub fn announce(
        &mut self,
        a: &crate::library::announcers::DiskAnnouncer,
        sound: &ddi_library::path::RelPath,
        share: f32,
    ) {
        let (a, sound) = (a.clone(), sound.to_rel_path_buf());
        let rate = self.audio.sample_rate();
        let label = sound.to_string();
        let rx = crate::os::background("ddi-announcer", None, move || {
            a.reader().and_then(|r| {
                let bytes = r.read(&a.name_in_reader(&sound))?;
                crate::audio::decoder::sound(bytes, sound.extension(), Default::default(), rate)
            })
        });
        self.voice = Some((rx, label, share));
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
    /// after its pause); an announcer's line speaks on.
    pub fn stop(&mut self) {
        if self.playing.take().is_some() {
            self.audio.stop_music();
        }
    }

    /// A play starts: what plays stops, until [`Player::resume`].
    pub fn pause(&mut self) {
        self.stop();
        self.paused = true;
        self.job = None;
        self.settle = None;
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
        self.paused = false;
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
        if let Some((rx, label, share)) = &self.voice {
            match rx.try_recv() {
                Ok(Ok(sound)) => {
                    let at = self.audio.context_time() + LEAD;
                    self.audio.play_voice_sound(&sound, at, *share);
                    self.spoken.push(label.clone());
                    self.voice = None;
                }
                Ok(Err(e)) => {
                    self.errors.push(format!("announcer {label}: {e}"));
                    self.voice = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.voice = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        // Song select's line waiting for its sound, while it is due.
        if let Some((m, load)) = self.menu.as_mut().filter(|(m, _)| m.waiting()) {
            load.poll();
            let found = m.poll(now.duration_since(self.epoch).as_secs_f64());
            self.speak_menu(found, Settings::cached().announcer_volume);
        }
        // While a line decodes, soon again.
        let waiting = self.voice.is_some() || self.menu.as_ref().is_some_and(|(m, _)| m.waiting());
        let voice = waiting.then(|| now + Duration::from_millis(8));
        if self.paused {
            return voice;
        }
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
        let mut next = self.playing.as_ref().map(|p| p.again);
        if let Some(v) = voice {
            next = Some(next.map_or(v, |n| n.min(v)));
        }
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
        self.playing = Some(Playing {
            id: l.song.id.clone(),
            again: Instant::now() + Duration::from_secs_f64(end - start + AGAIN - LEAD),
        });
    }

    /// A preview is wanted and on its way (settling, decoding): not
    /// playing yet, nor paused for a play, nor failed.
    pub fn loading(&self) -> bool {
        !self.paused && self.playing.is_none() && (self.settle.is_some() || self.job.is_some())
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
    fn an_announcers_sample_line_is_read_decoded_and_spoken() {
        let dir = std::env::temp_dir()
            .join("ddi-library-tests")
            .join(format!("announce-sample-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // 0.1 s of a 16-bit mono WAV.
        let samples: Vec<f32> = (0..4_800)
            .map(|i| ((i % 48) - 24) as f32 * 600.0 / 32767.0)
            .collect();
        let wav = ddi_library::wav::write(&[samples], 48_000);
        for line in ["gameplay ready", "gameplay failed", "select music intro"] {
            let at = dir.join("Announcers/Voice").join(line);
            std::fs::create_dir_all(&at).unwrap();
            std::fs::write(at.join("a.wav"), &wav).unwrap();
        }
        let found = crate::library::announcers::in_folder(&dir);
        let a = &found[0];
        let sound = a.kept.sample(0).unwrap();
        assert_eq!(sound.as_str(), "select music intro/a.wav");
        let mut p = Player::with(Audio::silent(0.8), PreviewSong::bundled);
        p.announce(a, sound, 0.5);
        let until = Instant::now() + Duration::from_secs(10);
        while p.spoken.is_empty() && p.errors.is_empty() && Instant::now() < until {
            p.tick();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(p.errors, Vec::<String>::new());
        assert_eq!(p.spoken, ["select music intro/a.wav"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn leaving_during_settle_or_decode_cannot_start_a_preview() {
        for decoding in [false, true] {
            let mut p = Player::with(Audio::silent(0.0), PreviewSong::bundled);
            p.select(Some("sweeteners"));
            if decoding {
                p.settle = Some(Instant::now());
                p.tick();
            }
            p.pause();
            p.invalidate(&["sweeteners".into()]);
            p.refind();
            for _ in 0..10 {
                p.tick();
            }
            assert!(p.paused());
            assert!(p.playing.is_none());
            assert!(p.job.is_none());
        }
    }

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
                p.length > 1.0 && p.length <= ddi_session::preview::MAX_LENGTH,
                "{id}: {}",
                p.length
            );
            assert!(p.from >= 0.0);
        }
        assert!(PreviewSong::bundled("not-a-song").is_none());
    }

    #[test]
    fn a_selection_settles_then_decodes_then_plays() {
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
