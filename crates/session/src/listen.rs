//! The chart editor's listening (`docs/plans/editor.md`, step 5): the
//! song's music from a second on, and the song second heard now, on
//! either shell's audio. As a play hears it (`ddi_engine::clock`): the
//! output's latency and the player's audio offset taken off the context
//! time, so the cursor follows what is heard. [`Listener`] keeps what
//! both shells keep around it; each reads and decodes the music its way.
//!
//! Along the music, [`Cues`]: the metronome, calibration's click on
//! every beat (its note's click on each measure's first, its lead-in's
//! on the others), and the assist clap on the rows of notes, each
//! scheduled ahead on the audio clock at the context time the music's
//! own sample for that second plays, as a play schedules its claps
//! (`crate::sounds`): heard with the music whatever the latency. A
//! beat's second is the one the engine judges a note on it at
//! (`TimingMap::seconds_at`: after a delay on its row, before a stop),
//! so the clicks follow BPM changes, stops, delays and warps.

use ddi_chart::{Note, NoteKind, TICKS_PER_BEAT, Tick, TimingMap};
use ddi_library::path::{RelPath, RelPathBuf};
use ddi_platform::AudioBackend;
use ddi_ui::state::editor::Editor;

use crate::calibration::{BEAT_CLICK, NOTE_CLICK, click};
use crate::sounds::{CLAP_LOOKAHEAD, clap};

/// A cue scheduled this late (a long frame) is left out rather than
/// heard off the beat.
const CUE_LATE: f64 = 0.01;

/// The most beats one [`Listener::cue`] looks at (a long warp's beats
/// all fall on one second).
const MAX_BEATS: usize = 4096;

/// What sounds along the music.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    /// A measure's first beat.
    Accent,
    Beat,
    Clap,
}

/// The cues of a listen: the metronome's beats on a timing, the claps'
/// rows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cues {
    /// The timing the metronome's beats are on; `None`: no metronome.
    beats: Option<TimingMap>,
    /// The assist clap's rows, song seconds, in order, once each.
    claps: Vec<f64>,
}

impl Cues {
    /// The metronome on `timing`'s beats when `metronome`, the clap on
    /// the rows of `notes` when `clap`: those with a tap or a hold's or
    /// roll's head judged on them, as a play claps (`sounds::clap_rows`).
    pub fn new(timing: &TimingMap, notes: &[Note], metronome: bool, clap: bool) -> Cues {
        let mut claps: Vec<f64> = if clap {
            notes
                .iter()
                .filter(|n| {
                    matches!(
                        n.kind,
                        NoteKind::Tap | NoteKind::HoldHead { .. } | NoteKind::RollHead { .. }
                    ) && timing.judgeable(n.tick)
                })
                .map(|n| timing.seconds_at(n.tick))
                .collect()
        } else {
            Vec::new()
        };
        claps.sort_by(f64::total_cmp);
        claps.dedup();
        Cues {
            beats: metronome.then(|| timing.clone()),
            claps,
        }
    }

    /// The editor's: its sounds on the chart shown.
    pub fn of(editor: &Editor) -> Cues {
        let s = editor.sounds();
        Cues::new(editor.timing(), &editor.chart().notes, s.metronome, s.clap)
    }

    pub fn is_empty(&self) -> bool {
        self.beats.is_none() && self.claps.is_empty()
    }
}

/// Whether `tick` starts a measure of the time signatures in force
/// (4/4 before any): `n/d` is a measure of `n` notes of a `d`th, `4n/d`
/// beats from the signature's row on.
pub fn starts_measure(timing: &TimingMap, tick: Tick) -> bool {
    let (from, n, d) = timing
        .time_signatures
        .iter()
        .rev()
        .find(|s| s.tick <= tick)
        .map_or((Tick::ZERO, 4, 4), |s| (s.tick, s.numerator, s.denominator));
    let measure = i128::from(n) * 4 * i128::from(TICKS_PER_BEAT);
    measure > 0 && (i128::from(tick.0 - from.0) * i128::from(d)).rem_euclid(measure) == 0
}

/// Where the cues are scheduled to.
#[derive(Clone, Copy, Debug)]
struct Schedule {
    /// The next beat looked at.
    beat: i64,
    /// The next clap's index.
    clap: usize,
    /// The song second everything before is scheduled.
    until: f64,
    /// The last beat's second (beats in a warp fall on one: one click).
    last_beat: f64,
}

impl Schedule {
    fn at(cues: &Cues, until: f64) -> Schedule {
        let beat = cues
            .beats
            .as_ref()
            .map_or(0, |t| (t.beat_at(until).floor() as i64 - 1).max(0));
        Schedule {
            beat,
            clap: cues.claps.partition_point(|&s| s < until),
            until,
            last_beat: f64::NEG_INFINITY,
        }
    }
}

/// What the cues scheduled (the web's debug state shows it).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Scheduled {
    pub accents: u32,
    pub beats: u32,
    pub claps: u32,
    /// The last: which, its song second and the context time it plays.
    pub last: Option<(Cue, f64, f64)>,
}

/// The cues' sounds at the output's rate.
struct CueSounds<B> {
    accent: Option<B>,
    beat: Option<B>,
    clap: Option<B>,
}

impl<B> CueSounds<B> {
    fn new<A: AudioBackend<Buffer = B>>(audio: &A) -> CueSounds<B> {
        let rate = audio.sample_rate() as f32;
        let make = |data: Vec<f32>| audio.buffer_from_pcm(vec![data], f64::from(rate)).ok();
        CueSounds {
            accent: make(click(rate, NOTE_CLICK)),
            beat: make(click(rate, BEAT_CLICK)),
            clap: make(clap(rate)),
        }
    }

    fn of(&self, cue: Cue) -> Option<&B> {
        match cue {
            Cue::Accent => self.accent.as_ref(),
            Cue::Beat => self.beat.as_ref(),
            Cue::Clap => self.clap.as_ref(),
        }
    }
}

/// Listening in the editor: the music file read (or being read), its
/// sound once read, a listen asked for before then, what plays.
pub struct Listener<A: AudioBackend> {
    file: Option<RelPathBuf>,
    music: Option<A::Buffer>,
    waiting: Option<Ask>,
    playing: Option<Listening>,
    cues: Cues,
    schedule: Schedule,
    sounds: Option<CueSounds<A::Buffer>>,
    /// What the cues scheduled so far.
    pub scheduled: Scheduled,
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
            cues: Cues::default(),
            schedule: Schedule::at(&Cues::default(), 0.0),
            sounds: None,
            scheduled: Scheduled::default(),
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
        // A beat on the second it starts from is heard too.
        self.schedule = Schedule::at(&self.cues, ask.from - 1e-9);
        self.cue(audio);
        Ok(())
    }

    /// What sounds along the music from now on (what is scheduled
    /// already plays).
    pub fn set_cues(&mut self, cues: Cues) {
        if cues != self.cues {
            self.schedule = Schedule::at(&cues, self.schedule.until);
            self.cues = cues;
        }
    }

    /// Schedules the cues due before [`CLAP_LOOKAHEAD`] from now while
    /// the music plays (each frame).
    pub fn cue(&mut self, audio: &mut A) {
        let Some(p) = self.playing.filter(|_| self.waiting.is_none()) else {
            return;
        };
        if self.cues.is_empty() {
            return;
        }
        let now = audio.now().context_time;
        let horizon = p.song_at(now + CLAP_LOOKAHEAD).min(p.end);
        let from = self.schedule.until;
        if horizon <= from {
            return;
        }
        let sounds = self.sounds.get_or_insert_with(|| CueSounds::new(audio));
        let s = &mut self.schedule;
        let mut due = Vec::new();
        if let Some(t) = &self.cues.beats {
            for _ in 0..MAX_BEATS {
                let tick = Tick::from_beats(s.beat);
                let at = t.seconds_at(tick);
                if at >= horizon {
                    break;
                }
                s.beat += 1;
                if at < from || at <= s.last_beat {
                    continue;
                }
                s.last_beat = at;
                let cue = if starts_measure(t, tick) {
                    Cue::Accent
                } else {
                    Cue::Beat
                };
                due.push((at, cue));
            }
        }
        while let Some(&at) = self.cues.claps.get(s.clap) {
            if at >= horizon {
                break;
            }
            s.clap += 1;
            due.push((at, Cue::Clap));
        }
        s.until = horizon;
        for (second, cue) in due {
            let at = p.context_at(second);
            let Some(buffer) = sounds.of(cue) else {
                continue;
            };
            if at < now - CUE_LATE || audio.play(buffer, at, 0.0, p.volume).is_err() {
                continue;
            }
            let n = &mut self.scheduled;
            match cue {
                Cue::Accent => n.accents += 1,
                Cue::Beat => n.beats += 1,
                Cue::Clap => n.claps += 1,
            }
            n.last = Some((cue, second, at));
        }
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
    volume: f32,
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
            volume,
        })
    }

    /// The context time the music plays song second `second` at.
    pub fn context_at(&self, second: f64) -> f64 {
        self.start + (second - self.from)
    }

    /// The song second the music plays at context time `at`.
    pub fn song_at(&self, at: f64) -> f64 {
        self.from + (at - self.start)
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
        /// The cues played: their sound (its peak, negated) and when.
        cues: Vec<(f64, f64)>,
    }

    impl Fake {
        fn new(clock: &Rc<Cell<f64>>) -> Fake {
            Fake {
                clock: clock.clone(),
                played: Vec::new(),
                stopped: false,
                cues: Vec::new(),
            }
        }
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
        fn buffer_from_pcm(&self, pcm: Vec<Vec<f32>>, _: f64) -> Result<f64, String> {
            let peak = pcm[0].iter().fold(0.0f32, |m, v| m.max(v.abs()));
            Ok(-f64::from(peak))
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
        fn play(&mut self, b: &f64, at: f64, offset: f64, _: f32) -> Result<SoundId, String> {
            if *b < 0.0 {
                self.cues.push((*b, at));
            } else {
                self.played.push((at, offset));
            }
            Ok(1)
        }
        fn duration(&self, b: &f64) -> f64 {
            *b
        }
    }

    #[test]
    fn the_second_heard_follows_the_clock_past_the_latency_and_the_offset() {
        let clock = Rc::new(Cell::new(5.0));
        let mut audio = Fake::new(&clock);
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
        let mut audio = Fake::new(&clock);
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

    /// 120 BPM, beat 0 at -0.1 s; 240 from beat 8, a stop of 0.5 s on
    /// beat 12, a delay of 0.25 s on beat 14, a warp over beats 16 and
    /// 17, 3/4 from beat 20.
    fn timing() -> TimingMap {
        use ddi_chart::{BpmSegment, StopSegment, TimeSignature, WarpSegment};
        let mut t = TimingMap::constant(120.0, 0.1);
        t.bpms.push(BpmSegment {
            tick: Tick::from_beats(8),
            bpm: 240.0,
        });
        t.stops.push(StopSegment {
            tick: Tick::from_beats(12),
            seconds: 0.5,
        });
        t.delays.push(StopSegment {
            tick: Tick::from_beats(14),
            seconds: 0.25,
        });
        t.warps.push(WarpSegment {
            tick: Tick::from_beats(16),
            length: Tick::from_beats(2),
        });
        t.time_signatures.push(TimeSignature {
            tick: Tick::from_beats(20),
            numerator: 3,
            denominator: 4,
        });
        t.tidy();
        t
    }

    /// Listens from `from` with `cues` on a clock stepped a frame at a
    /// time for `seconds`, the cues scheduled each frame; what played,
    /// as (which, song second).
    fn listen_with(cues: Cues, from: f64, seconds: f64) -> Vec<(Cue, f64)> {
        let clock = Rc::new(Cell::new(1.0));
        let mut audio = Fake::new(&clock);
        let file = RelPath::new("a.ogg");
        let mut l = Listener::<Fake>::default();
        l.read(file);
        l.music_ready(file, 60.0, None).unwrap();
        l.set_cues(cues);
        l.ask(&mut audio, from, 1.0, 0.01).unwrap();
        let start = 1.0 + LOOKAHEAD;
        let mut checked = 0;
        for i in 0..(seconds * 60.0) as usize {
            clock.set(1.0 + i as f64 / 60.0);
            l.cue(&mut audio);
            // Each cue heard (past the latency and the audio offset) when
            // its second is.
            for &(_, at) in &audio.cues[checked..] {
                let now = clock.get();
                assert!(at >= now - CUE_LATE && at <= now + CLAP_LOOKAHEAD + 1e-9);
                clock.set(at + 0.02 + 0.01);
                let heard = l.time(&audio).unwrap();
                assert!((heard - (from + at - start)).abs() < 1e-9);
                clock.set(now);
            }
            checked = audio.cues.len();
        }
        let accent = -f64::from(
            click(48_000.0, NOTE_CLICK)
                .iter()
                .fold(0.0f32, |m, v| m.max(v.abs())),
        );
        let beat = -f64::from(
            click(48_000.0, BEAT_CLICK)
                .iter()
                .fold(0.0f32, |m, v| m.max(v.abs())),
        );
        audio
            .cues
            .iter()
            .map(|&(b, at)| {
                let cue = if b == accent {
                    Cue::Accent
                } else if b == beat {
                    Cue::Beat
                } else {
                    Cue::Clap
                };
                (cue, from + at - start)
            })
            .collect()
    }

    fn near(a: &[(Cue, f64)], b: &[(Cue, f64)]) -> bool {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(x, y)| x.0 == y.0 && (x.1 - y.1).abs() < 1e-9)
    }

    #[test]
    fn the_metronome_clicks_on_the_beats_heard_through_the_timing() {
        let t = timing();
        let cues = listen_with(Cues::new(&t, &[], true, false), 0.9, 12.0);
        let (a, b) = (Cue::Accent, Cue::Beat);
        // Beat 2 on the second it starts from; 120 BPM, then 240 from
        // beat 8 (3.9 s); beat 13 after the stop, beat 14 after its
        // delay; one click for the warp's beats (16 to 18); 3/4 from 20.
        let want = [
            (b, 0.9),
            (b, 1.4),
            (a, 1.9),
            (b, 2.4),
            (b, 2.9),
            (b, 3.4),
            (a, 3.9),
            (b, 4.15),
            (b, 4.4),
            (b, 4.65),
            (a, 4.9),
            (b, 5.65),
            (b, 6.15),
            (b, 6.4),
            (a, 6.65),
            (b, 6.9),
            (a, 7.15),
            (b, 7.4),
            (b, 7.65),
            (a, 7.9),
        ];
        assert!(near(&cues[..want.len()], &want), "{cues:?}");
        // Every beat after to the music's end, each on its second.
        let mut last = 0.0;
        for (i, &(cue, s)) in cues.iter().enumerate().skip(want.len()) {
            let beat = 20 + (i - want.len()) as i64 + 4;
            assert!((s - t.seconds_at(Tick::from_beats(beat))).abs() < 1e-9);
            assert_eq!(cue == Cue::Accent, (beat - 20) % 3 == 0);
            assert!(s > last);
            last = s;
        }
        assert!(last > 12.0 && last < 13.0, "{last}");
    }

    #[test]
    fn the_clap_sounds_on_the_rows_of_notes_judged() {
        let t = timing();
        let note = |beat: f64, kind| Note::new(Tick::from_beat_f64(beat), 0, kind);
        let notes = [
            note(2.0, NoteKind::Tap),
            note(3.0, NoteKind::Fake),
            note(4.5, NoteKind::Tap),
            Note::new(Tick::from_beats(4) + Tick(24), 1, NoteKind::Tap),
            note(
                9.0,
                NoteKind::HoldHead {
                    end: Tick::from_beats(10),
                },
            ),
            note(10.0, NoteKind::Mine),
            note(17.0, NoteKind::Tap),
            note(19.0, NoteKind::Lift),
        ];
        let cues = listen_with(Cues::new(&t, &notes, false, true), 0.0, 8.0);
        let c = Cue::Clap;
        assert!(near(&cues, &[(c, 0.9), (c, 2.15), (c, 4.15)]), "{cues:?}");
        // From a second on: only the rows after.
        let cues = listen_with(Cues::new(&t, &notes, true, true), 2.0, 2.5);
        assert_eq!(cues[0], (Cue::Clap, 2.15));
        assert!(cues.iter().all(|c| c.1 >= 2.0));
        assert_eq!(cues.iter().filter(|c| c.0 == Cue::Clap).count(), 2);
    }

    #[test]
    fn cues_changed_while_listening_go_on_from_what_was_scheduled() {
        let t = timing();
        let clock = Rc::new(Cell::new(1.0));
        let mut audio = Fake::new(&clock);
        let file = RelPath::new("a.ogg");
        let mut l = Listener::<Fake>::default();
        l.read(file);
        l.music_ready(file, 60.0, None).unwrap();
        l.ask(&mut audio, 0.0, 1.0, 0.0).unwrap();
        let step = |l: &mut Listener<Fake>, audio: &mut Fake, frames: usize| {
            for _ in 0..frames {
                clock.set(clock.get() + 1.0 / 60.0);
                l.cue(audio);
            }
        };
        step(&mut l, &mut audio, 60);
        assert!(audio.cues.is_empty(), "no cues asked for");
        l.set_cues(Cues::new(&t, &[], true, false));
        step(&mut l, &mut audio, 60);
        let on = audio.cues.len();
        assert!(on >= 2, "{on}");
        // The same cues again: nothing twice.
        l.set_cues(Cues::new(&t, &[], true, false));
        step(&mut l, &mut audio, 30);
        let mut at: Vec<f64> = audio.cues.iter().map(|c| c.1).collect();
        at.dedup();
        assert_eq!(at.len(), audio.cues.len());
        l.set_cues(Cues::default());
        let off = audio.cues.len();
        step(&mut l, &mut audio, 60);
        assert_eq!(audio.cues.len(), off, "off");
        assert_eq!(l.scheduled.beats + l.scheduled.accents, off as u32);
        // Stopped: nothing more.
        l.set_cues(Cues::new(&t, &[], true, false));
        l.stop(&mut audio);
        step(&mut l, &mut audio, 60);
        assert_eq!(audio.cues.len(), off);
    }

    #[test]
    fn measures_follow_the_time_signatures() {
        let t = timing();
        let starts: Vec<i64> = (0..30)
            .filter(|b| starts_measure(&t, Tick::from_beats(*b)))
            .collect();
        assert_eq!(starts, [0, 4, 8, 12, 16, 20, 23, 26, 29]);
        // 7/8: a measure of three and a half beats.
        let mut t = TimingMap::constant(120.0, 0.0);
        t.time_signatures.push(ddi_chart::TimeSignature {
            tick: Tick::ZERO,
            numerator: 7,
            denominator: 8,
        });
        let starts: Vec<i64> = (0..15)
            .filter(|b| starts_measure(&t, Tick::from_beats(*b)))
            .collect();
        assert_eq!(starts, [0, 7, 14]);
        assert!(starts_measure(&t, Tick(168)));
    }
}
