//! The chart editor's listening (`docs/plans/editor.md`, step 5): the
//! song's music from a second on, and the song second heard now, on
//! either shell's audio. As a play hears it (`ddi_engine::clock`): the
//! output's latency and the player's audio offset taken off the context
//! time, so the cursor follows what is heard.

use ddi_platform::AudioBackend;

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
}
