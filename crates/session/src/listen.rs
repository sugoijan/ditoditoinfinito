//! The chart editor's listening (`docs/plans/editor.md`, step 5): the
//! song's music from a second on, and the song second heard now, on
//! either shell's audio. As a play hears it (`ddi_engine::clock`): the
//! output's latency and the player's audio offset taken off the context
//! time, so the cursor follows what is heard. [`Listener`] keeps what
//! both shells keep around it; each reads and decodes the music its way.

use ddi_library::path::{RelPath, RelPathBuf};
use ddi_platform::AudioBackend;

/// Listening in the editor: the music file read (or being read), its
/// sound once read, a listen asked for before then, what plays.
pub struct Listener<A: AudioBackend> {
    file: Option<RelPathBuf>,
    music: Option<A::Buffer>,
    waiting: Option<Ask>,
    playing: Option<Listening>,
}

/// A listen asked for: from that song second, at that volume and audio
/// offset.
#[derive(Clone, Copy)]
struct Ask {
    from: f64,
    volume: f32,
    audio_offset: f64,
}

impl<A: AudioBackend> Default for Listener<A> {
    fn default() -> Self {
        Listener {
            file: None,
            music: None,
            waiting: None,
            playing: None,
        }
    }
}

impl<A: AudioBackend> Listener<A> {
    /// The music is `file` (a chart's, `ManifestEntry::music_for`):
    /// whether the shell reads it now (another file than the one read or
    /// being read; that one is let go).
    pub fn read(&mut self, file: &RelPath) -> bool {
        if self.file.as_deref() == Some(file) {
            return false;
        }
        self.file = Some(file.to_owned());
        self.music = None;
        true
    }

    /// The music file is not known yet (the shell finds it out, then
    /// calls [`Listener::read`]).
    pub fn forget(&mut self) {
        self.file = None;
        self.music = None;
    }

    /// `file` read: kept, and played if a listen waited for it (on
    /// `audio`, when there is one); nothing if another file was asked for
    /// since.
    pub fn music_ready(
        &mut self,
        file: &RelPath,
        music: A::Buffer,
        audio: Option<&mut A>,
    ) -> Result<(), A::Error> {
        if self.file.as_deref() != Some(file) {
            return Ok(());
        }
        self.music = Some(music);
        match (self.waiting.take(), audio) {
            (Some(ask), Some(audio)) => self.start(audio, ask),
            _ => Ok(()),
        }
    }

    /// `file` could not be read (`None`: a file not yet known, as after
    /// [`Listener::forget`]): no listen waits for it.
    pub fn music_failed(&mut self, file: Option<&RelPath>) {
        if self.file.as_deref() == file {
            self.waiting = None;
        }
    }

    /// Plays the music from song second `from` (what played stopped), or
    /// once it is read.
    pub fn ask(
        &mut self,
        audio: &mut A,
        from: f64,
        volume: f32,
        audio_offset: f64,
    ) -> Result<(), A::Error> {
        self.stop(audio);
        let ask = Ask {
            from,
            volume,
            audio_offset,
        };
        if self.music.is_some() {
            self.start(audio, ask)
        } else {
            self.waiting = Some(ask);
            Ok(())
        }
    }

    fn start(&mut self, audio: &mut A, ask: Ask) -> Result<(), A::Error> {
        let Some(music) = &self.music else {
            return Ok(());
        };
        self.playing = Some(Listening::start(
            audio,
            music,
            ask.from,
            ask.volume,
            ask.audio_offset,
        )?);
        Ok(())
    }

    /// The song second heard now: where the listen waiting starts, else
    /// the music's; `None` when none plays (or it has ended).
    pub fn time(&mut self, audio: &A) -> Option<f64> {
        if let Some(ask) = self.waiting {
            return Some(ask.from);
        }
        let t = self.playing.as_ref().and_then(|p| p.time(audio));
        if t.is_none() {
            self.playing = None;
        }
        t
    }

    /// Stops what plays, and the listen waiting.
    pub fn stop(&mut self, audio: &mut A) {
        if let Some(p) = self.playing.take() {
            p.stop(audio);
        }
        self.waiting = None;
    }
}

/// Seconds between asking and the music's start.
pub const LOOKAHEAD: f64 = 0.1;

/// The music playing from a song second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listening {
    /// Context time song second `from` plays at.
    start: f64,
    from: f64,
    /// The music's end, song seconds.
    end: f64,
    audio_offset: f64,
}

impl Listening {
    /// Plays `buffer` on `audio` from song second `from` (before the
    /// music's start: its start waits that long) at `volume`;
    /// `audio_offset` is the player's (seconds, as the settings keep it).
    pub fn start<A: AudioBackend>(
        audio: &mut A,
        buffer: &A::Buffer,
        from: f64,
        volume: f32,
        audio_offset: f64,
    ) -> Result<Listening, A::Error> {
        let start = audio.now().context_time + LOOKAHEAD;
        let before = (-from).max(0.0);
        audio.play(buffer, start + before, from.max(0.0), volume)?;
        Ok(Listening {
            start,
            from,
            end: audio.duration(buffer),
            audio_offset,
        })
    }

    /// The song second heard now (`from` until the music is heard);
    /// `None` once the music has ended.
    pub fn time<A: AudioBackend>(&self, audio: &A) -> Option<f64> {
        let now = audio.now();
        let heard = now.context_time - now.output_latency - self.audio_offset;
        let t = self.from + (heard - self.start).max(0.0);
        (t <= self.end).then_some(t)
    }

    /// Stops the music.
    pub fn stop<A: AudioBackend>(&self, audio: &mut A) {
        audio.stop_all();
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use ddi_platform::{ClockSample, DeviceFingerprint, HostTime, SoundId};

    use super::*;

    struct Fake {
        clock: Rc<Cell<f64>>,
        played: Vec<(f64, f64)>,
        stopped: bool,
    }

    impl AudioBackend for Fake {
        type Buffer = f64;
        type Error = String;
        fn sample_rate(&self) -> f64 {
            48_000.0
        }
        fn master_ramp(&mut self, _: Option<f32>, _: f32, _: Option<f64>, _: f64) {}
        fn stop_all(&mut self) {
            self.stopped = true;
        }
        fn running(&self) -> bool {
            true
        }
        fn buffer_from_pcm(&self, _: Vec<Vec<f32>>, _: f64) -> Result<f64, String> {
            Ok(0.0)
        }
        fn fingerprint(&self) -> Option<DeviceFingerprint> {
            None
        }
        fn now(&self) -> ClockSample {
            ClockSample {
                context_time: self.clock.get(),
                host_time: HostTime(self.clock.get()),
                output_latency: 0.02,
            }
        }
        fn play(&mut self, _: &f64, at: f64, offset: f64, _: f32) -> Result<SoundId, String> {
            self.played.push((at, offset));
            Ok(1)
        }
        fn duration(&self, b: &f64) -> f64 {
            *b
        }
    }

    #[test]
    fn the_second_heard_follows_the_clock_past_the_latency_and_the_offset() {
        let clock = Rc::new(Cell::new(5.0));
        let mut audio = Fake {
            clock: clock.clone(),
            played: Vec::new(),
            stopped: false,
        };
        let l = Listening::start(&mut audio, &60.0, 10.0, 1.0, 0.01).unwrap();
        assert_eq!(audio.played, vec![(5.0 + LOOKAHEAD, 10.0)]);
        assert_eq!(l.time(&audio), Some(10.0), "not heard yet");
        clock.set(5.0 + LOOKAHEAD + 0.03 + 2.0);
        assert!((l.time(&audio).unwrap() - 12.0).abs() < 1e-9);
        clock.set(5.0 + LOOKAHEAD + 0.03 + 51.0);
        assert_eq!(l.time(&audio), None, "the music has ended");
        l.stop(&mut audio);
        assert!(audio.stopped);
        // Before the music's start: it waits.
        let l = Listening::start(&mut audio, &60.0, -1.5, 1.0, 0.0).unwrap();
        let start = clock.get() + LOOKAHEAD;
        assert_eq!(audio.played[1], (start + 1.5, 0.0));
        clock.set(start + 0.02 + 1.0);
        assert!((l.time(&audio).unwrap() + 0.5).abs() < 1e-9);
    }

    #[test]
    fn a_listen_waits_for_its_music_read_once_a_file() {
        let clock = Rc::new(Cell::new(1.0));
        let mut audio = Fake {
            clock: clock.clone(),
            played: Vec::new(),
            stopped: false,
        };
        let (a, b) = (RelPath::new("a.ogg"), RelPath::new("b.ogg"));
        let mut l = Listener::<Fake>::default();
        assert!(l.read(a));
        l.ask(&mut audio, 3.0, 1.0, 0.0).unwrap();
        assert_eq!(l.time(&audio), Some(3.0), "waiting");
        // Another chart, the same file: still read once.
        assert!(!l.read(a));
        // A read of a file let go is not kept.
        assert!(l.read(b));
        l.music_ready(a, 60.0, Some(&mut audio)).unwrap();
        assert!(audio.played.is_empty());
        l.music_ready(b, 60.0, Some(&mut audio)).unwrap();
        assert_eq!(audio.played, vec![(1.0 + LOOKAHEAD, 3.0)]);
        // Read already: plays at once.
        l.ask(&mut audio, 5.0, 1.0, 0.0).unwrap();
        assert_eq!(audio.played.len(), 2);
        l.stop(&mut audio);
        assert_eq!(l.time(&audio), None);
        // A failed read: nothing waits.
        assert!(l.read(a));
        l.ask(&mut audio, 1.0, 1.0, 0.0).unwrap();
        l.music_failed(Some(b));
        assert_eq!(l.time(&audio), Some(1.0), "another file's");
        l.music_failed(Some(a));
        assert_eq!(l.time(&audio), None);
        // A file not known yet.
        l.forget();
        l.ask(&mut audio, 2.0, 1.0, 0.0).unwrap();
        l.music_failed(None);
        assert_eq!(l.time(&audio), None);
    }
}
