//! The mixer, run in the output callback (`docs/plans/desktop.md`, D2):
//! what a play needs of Web Audio and no more. Voices of decoded PCM
//! started on a frame of the output timeline, each with a gain that
//! follows scheduled events (set, hold, linear ramp, and the exponential
//! approach of `setTargetAtTime`), a master gain with the same events,
//! stops on a frame or at once with a 5 ms fade (no click).
//!
//! Nothing here locks or allocates: voices are built on the main thread
//! and sent in boxes, with room for their events reserved; finished ones
//! are handed back to be dropped there ([`Mixer::render`]'s `done`).
//!
//! Frames are the output's: frame `n` is `n / rate` seconds of the
//! stream's timeline, the `currentTime` of [`super::Audio`]'s clock.

use std::sync::Arc;

use ddi_platform::SoundId;
use ddi_platform::decode_all::DecodedAudio;

/// Voices at most (a play uses one or two, a preview one, sounds a few).
pub const MAX_VOICES: usize = 64;
/// Events pending on one gain at most.
pub const MAX_EVENTS: usize = 16;
/// An immediate stop fades over this long (Web Audio stops dead; a click
/// is not worth fidelity).
pub const STOP_FADE: f64 = 0.005;
/// The loudest gain taken (+18 dB); anything else, not finite or below
/// zero, is cleaned by [`gain`].
pub const MAX_GAIN: f32 = 8.0;
/// A target is reached when this close, or when a step no longer moves
/// the value: it snaps to it and the event ends (an exponential never
/// arrives, and would go subnormal).
const TARGET_SNAP: f32 = 1e-6;

/// `value` as a gain: finite, in `0..=MAX_GAIN` (a NaN would poison the
/// whole mix).
pub fn gain(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, MAX_GAIN)
    } else {
        0.0
    }
}

/// Decoded PCM, interleaved, mono or stereo.
#[derive(Debug, PartialEq)]
pub struct Pcm {
    pub rate: u32,
    pub channels: u16,
    pub data: Vec<f32>,
}

/// A sound to play: shared, never copied.
pub type Sound = Arc<Pcm>;

impl Pcm {
    pub fn frames(&self) -> usize {
        self.data.len() / usize::from(self.channels.max(1))
    }

    pub fn duration(&self) -> f64 {
        self.frames() as f64 / f64::from(self.rate.max(1))
    }

    /// Planar channels interleaved; more than two folded to stereo as
    /// Web Audio's speaker rules do for 5.1 (centre and surrounds at
    /// √½, the LFE dropped), else the first two kept.
    pub fn from_decoded(audio: DecodedAudio) -> Pcm {
        let n = audio.frames();
        let ch = &audio.channels;
        let (channels, data) = match ch.len() {
            0 => (1, Vec::new()),
            1 => (1, audio.channels.into_iter().next().unwrap_or_default()),
            6 => {
                let k = std::f32::consts::FRAC_1_SQRT_2;
                let mut data = Vec::with_capacity(n * 2);
                let fronts = ch[0].iter().zip(&ch[1]);
                let rest = ch[2].iter().zip(&ch[4]).zip(&ch[5]);
                for ((&l, &r), ((&c, &sl), &sr)) in fronts.zip(rest) {
                    data.push(l + k * (c + sl));
                    data.push(r + k * (c + sr));
                }
                (2, data)
            }
            _ => {
                let mut data = Vec::with_capacity(n * 2);
                for (&l, &r) in ch[0].iter().zip(&ch[1]) {
                    data.push(l);
                    data.push(r);
                }
                (2, data)
            }
        };
        Pcm {
            rate: audio.sample_rate,
            channels,
            data,
        }
    }

    /// `audio` with at most two channels (planar, as decoded): more
    /// folded as [`Pcm::from_decoded`] folds them.
    pub fn fold(audio: DecodedAudio) -> DecodedAudio {
        if audio.channels.len() <= 2 {
            return audio;
        }
        let rate = audio.sample_rate;
        let skipped = audio.skipped_packets;
        let stereo = Pcm::from_decoded(audio);
        let n = stereo.frames();
        let mut l = Vec::with_capacity(n);
        let mut r = Vec::with_capacity(n);
        for p in stereo.data.as_chunks::<2>().0 {
            l.push(p[0]);
            r.push(p[1]);
        }
        DecodedAudio {
            sample_rate: rate,
            channels: vec![l, r],
            skipped_packets: skipped,
        }
    }

    /// The left and right samples at frame `i` (mono on both).
    #[inline]
    fn at(&self, i: usize) -> (f32, f32) {
        if self.channels == 1 {
            let s = self.data[i];
            (s, s)
        } else {
            (self.data[2 * i], self.data[2 * i + 1])
        }
    }
}

/// A scheduled change of a gain, at output frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// `setValueAtTime`.
    Set { at: u64, value: f32 },
    /// The value it has at `at`, kept as a ramp's start (what
    /// `setValueAtTime(param.value, t)` does on the web).
    Hold { at: u64 },
    /// `linearRampToValueAtTime`: from where the previous event left it
    /// (or from the frame the ramp was scheduled, if none is pending) to
    /// `value` at `end`.
    Ramp { end: u64, value: f32 },
    /// `setTargetAtTime`: from `at`, approach `value` with time constant
    /// `tau` frames, until the next event.
    Target { at: u64, value: f32, tau: f32 },
}

impl Event {
    fn time(&self) -> u64 {
        match *self {
            Event::Set { at, .. } | Event::Hold { at } | Event::Target { at, .. } => at,
            Event::Ramp { end, .. } => end,
        }
    }
}

/// A gain and its events: a small `AudioParam`.
#[derive(Debug)]
pub struct Param {
    value: f32,
    /// Where a ramp starts: the frame and value the last event ended at.
    from: (u64, f32),
    events: Vec<Event>,
    /// A `Target`'s per-frame step, worked out once.
    coef: f32,
}

impl Param {
    pub fn new(value: f32) -> Param {
        let value = gain(value);
        Param {
            value,
            from: (0, value),
            events: Vec::with_capacity(MAX_EVENTS),
            coef: 0.0,
        }
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    /// Adds `event`, scheduled at output frame `now`, in time order
    /// (after events at the same time). A ramp scheduled with nothing
    /// pending before it, or after a target, starts at `now` from the
    /// value then (the target ends there). Dropped if full.
    pub fn schedule(&mut self, event: Event, now: u64) {
        if self.events.len() == self.events.capacity() {
            return;
        }
        let event = match event {
            Event::Set { at, value } => Event::Set {
                at,
                value: gain(value),
            },
            Event::Ramp { end, value } => Event::Ramp {
                end,
                value: gain(value),
            },
            Event::Target { at, value, tau } => Event::Target {
                at,
                value: gain(value),
                tau,
            },
            hold => hold,
        };
        let i = self.events.partition_point(|e| e.time() <= event.time());
        if let Event::Ramp { .. } = event {
            let after_target = i > 0 && matches!(self.events[i - 1], Event::Target { .. });
            if after_target {
                self.events.remove(i - 1);
            }
            if i == 0 || after_target {
                self.from = (now, self.value);
            }
            let i = self.events.partition_point(|e| e.time() <= event.time());
            self.events.insert(i, event);
            return;
        }
        self.events.insert(i, event);
    }

    /// Drops the events at or after frame `from` (`cancelScheduledValues`).
    pub fn cancel(&mut self, from: u64) {
        self.events.retain(|e| e.time() < from);
    }

    /// Nothing scheduled: the value stays.
    pub fn idle(&self) -> bool {
        self.events.is_empty()
    }

    /// The value at `frame`; frames are asked for in order.
    #[inline]
    pub fn advance(&mut self, frame: u64) -> f32 {
        while let Some(&e) = self.events.first() {
            match e {
                Event::Set { at, value } if frame >= at => {
                    self.value = value;
                    self.from = (at, value);
                    self.events.remove(0);
                }
                Event::Hold { at } if frame >= at => {
                    self.from = (at, self.value);
                    self.events.remove(0);
                }
                Event::Ramp { end, value } => {
                    if frame >= end {
                        self.value = value;
                        self.from = (end, value);
                        self.events.remove(0);
                        continue;
                    }
                    let (t0, v0) = self.from;
                    if frame >= t0 && end > t0 {
                        let k = (frame - t0) as f64 / (end - t0) as f64;
                        self.value = v0 + (value - v0) * k as f32;
                    }
                    break;
                }
                Event::Target { at, value, tau } if frame >= at => {
                    if self.events.get(1).is_some_and(|n| frame >= n.time()) {
                        self.from = (frame, self.value);
                        self.events.remove(0);
                        continue;
                    }
                    if self.coef == 0.0 || self.from.0 != at {
                        // First frame of this target.
                        self.coef = 1.0 - (-1.0 / tau.max(1e-3)).exp();
                        self.from = (at, self.value);
                    }
                    let next = self.value + (value - self.value) * self.coef;
                    // Arrived, or a step too small for an f32 to take
                    // (0.8 to 0.5 stalls 2e-5 short).
                    let stalled = next == self.value;
                    self.value = next;
                    if stalled || (value - self.value).abs() < TARGET_SNAP {
                        self.value = value;
                        self.from = (frame, value);
                        self.events.remove(0);
                    }
                    break;
                }
                _ => break,
            }
        }
        self.value
    }
}

/// One sound playing (or waiting for its frame).
#[derive(Debug)]
pub struct Voice {
    pub id: SoundId,
    sound: Sound,
    /// The output frame it starts on.
    start: u64,
    /// Where in the sound, in its frames, and how far a frame moves
    /// (its rate over the output's: 1 unless the stream reopened at
    /// another rate).
    pos: f64,
    step: f64,
    pub gain: Param,
    /// Ends on this output frame (`stop(when)`).
    stop_at: Option<u64>,
    /// Fading out after an immediate stop: frames left, and of how many.
    fade: Option<(u32, u32)>,
    /// It has been heard (a frame rendered): a stop fades it; before, a
    /// stop ends it at once.
    sounding: bool,
}

impl Voice {
    /// `sound` from `offset` seconds into it, starting on output frame
    /// `start` of a stream at `rate`, at `gain`.
    pub fn new(id: SoundId, sound: Sound, start: u64, offset: f64, gain: f32, rate: u32) -> Voice {
        let pos = (offset.max(0.0) * f64::from(sound.rate)).round();
        let step = f64::from(sound.rate) / f64::from(rate.max(1));
        Voice {
            id,
            sound,
            start,
            pos,
            step,
            gain: Param::new(gain),
            stop_at: None,
            fade: None,
            sounding: false,
        }
    }

    pub fn sound(&self) -> &Sound {
        &self.sound
    }

    /// The left and right samples at the current position.
    #[inline]
    fn sample(&self) -> (f32, f32) {
        let i = self.pos as usize;
        let frac = (self.pos - i as f64) as f32;
        let a = self.sound.at(i);
        if frac == 0.0 || i + 1 >= self.sound.frames() {
            return a;
        }
        let b = self.sound.at(i + 1);
        (a.0 + (b.0 - a.0) * frac, a.1 + (b.1 - a.1) * frac)
    }
}

/// What the main thread asks the mixer.
#[derive(Debug)]
pub enum Command {
    Play(Box<Voice>),
    /// Ends a voice: on frame `at`, or at once (with [`STOP_FADE`]).
    Stop {
        id: SoundId,
        at: Option<u64>,
    },
    StopAll,
    /// A voice's gain: an event, or its events from a frame cancelled.
    Gain {
        id: SoundId,
        event: Event,
    },
    CancelGain {
        id: SoundId,
        from: u64,
    },
    Master(Event),
    CancelMaster {
        from: u64,
    },
}

/// The mixer: its voices and the master gain.
pub struct Mixer {
    rate: u32,
    // Boxed: voices move through the rings as one pointer, built and
    // dropped on the main thread.
    #[allow(clippy::vec_box)]
    voices: Vec<Box<Voice>>,
    master: Param,
    fade_frames: u32,
}

impl Mixer {
    pub fn new(rate: u32, master: f32) -> Mixer {
        Mixer {
            rate,
            voices: Vec::with_capacity(MAX_VOICES),
            master: Param::new(master),
            fade_frames: (STOP_FADE * f64::from(rate)).round().max(1.0) as u32,
        }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn master(&self) -> f32 {
        self.master.value()
    }

    pub fn voices(&self) -> usize {
        self.voices.len()
    }

    fn voice(&mut self, id: SoundId) -> Option<&mut Box<Voice>> {
        self.voices.iter_mut().find(|v| v.id == id)
    }

    /// Carries out `c`, at output frame `now` (the next frame rendered).
    /// A voice refused (too many) goes to `done`.
    pub fn apply(&mut self, c: Command, now: u64, done: &mut impl FnMut(Box<Voice>)) {
        match c {
            Command::Play(v) => {
                if self.voices.len() < self.voices.capacity() {
                    self.voices.push(v);
                } else {
                    done(v);
                }
            }
            Command::Stop { id, at } => {
                let fade = self.fade_frames;
                let Some(i) = self.voices.iter().position(|v| v.id == id) else {
                    return;
                };
                match at {
                    Some(at) => self.voices[i].stop_at = Some(at),
                    // Not heard yet: gone at once; else faded out.
                    None if !self.voices[i].sounding => done(self.voices.swap_remove(i)),
                    None => self.voices[i].fade = Some((fade, fade)),
                }
            }
            Command::StopAll => {
                let fade = self.fade_frames;
                let mut i = 0;
                while i < self.voices.len() {
                    if self.voices[i].sounding {
                        self.voices[i].fade.get_or_insert((fade, fade));
                        i += 1;
                    } else {
                        done(self.voices.swap_remove(i));
                    }
                }
            }
            Command::Gain { id, event } => {
                if let Some(v) = self.voice(id) {
                    v.gain.schedule(event, now);
                }
            }
            Command::CancelGain { id, from } => {
                if let Some(v) = self.voice(id) {
                    v.gain.cancel(from);
                }
            }
            Command::Master(event) => self.master.schedule(event, now),
            Command::CancelMaster { from } => self.master.cancel(from),
        }
    }

    /// Mixes into `out` (stereo, interleaved, overwritten), whose first
    /// frame is output frame `first`. Voices that end go to `done`.
    pub fn render(&mut self, out: &mut [f32], first: u64, done: &mut impl FnMut(Box<Voice>)) {
        out.fill(0.0);
        let frames = out.len() / 2;
        let end = first + frames as u64;
        let mut i = 0;
        while i < self.voices.len() {
            let v = &mut self.voices[i];
            let mut over = false;
            // Frames before its start are silent for it.
            let from = v.start.max(first);
            let mut f = from;
            while f < end {
                if v.stop_at.is_some_and(|s| f >= s) || v.pos as usize >= v.sound.frames() {
                    over = true;
                    break;
                }
                let mut g = v.gain.advance(f);
                if let Some((left, total)) = &mut v.fade {
                    if *left == 0 {
                        over = true;
                        break;
                    }
                    g *= *left as f32 / *total as f32;
                    *left -= 1;
                }
                let (l, r) = v.sample();
                v.sounding = true;
                let o = ((f - first) as usize) * 2;
                out[o] += l * g;
                out[o + 1] += r * g;
                v.pos += v.step;
                f += 1;
            }
            if !over && (v.stop_at.is_some_and(|s| s <= end) || v.pos as usize >= v.sound.frames())
            {
                over = true;
            }
            if over {
                done(self.voices.swap_remove(i));
            } else {
                i += 1;
            }
        }
        if self.master.idle() {
            let g = self.master.value();
            if g != 1.0 {
                for s in out.iter_mut() {
                    *s *= g;
                }
            }
        } else {
            for (k, pair) in out.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                let g = self.master.advance(first + k as u64);
                pair[0] *= g;
                pair[1] *= g;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 1_000;

    /// A sound of `n` frames of `value` (mono).
    fn dc(n: usize, value: f32) -> Sound {
        Arc::new(Pcm {
            rate: RATE,
            channels: 1,
            data: vec![value; n],
        })
    }

    /// A ramp 0, 1, 2, … (mono): where each output frame read from.
    fn counting(n: usize) -> Sound {
        Arc::new(Pcm {
            rate: RATE,
            channels: 1,
            data: (0..n).map(|i| i as f32).collect(),
        })
    }

    /// Renders `blocks` blocks of `size` frames from frame 0; the left
    /// channel, and the ids of the voices that ended.
    fn run(m: &mut Mixer, blocks: usize, size: usize) -> (Vec<f32>, Vec<SoundId>) {
        let mut out = Vec::new();
        let mut ended = Vec::new();
        let mut buf = vec![0.0; size * 2];
        for b in 0..blocks {
            m.render(&mut buf, (b * size) as u64, &mut |v| ended.push(v.id));
            out.extend(buf.as_chunks::<2>().0.iter().map(|p| p[0]));
        }
        (out, ended)
    }

    fn play(m: &mut Mixer, id: SoundId, sound: Sound, start: u64, offset: f64, gain: f32) {
        let v = Voice::new(id, sound, start, offset, gain, m.rate());
        m.apply(Command::Play(Box::new(v)), 0, &mut |_| panic!("refused"));
    }

    #[test]
    fn a_voice_starts_on_its_frame_inside_a_block() {
        let mut m = Mixer::new(RATE, 1.0);
        // Frame 37, in the middle of the third 16-frame block.
        play(&mut m, 1, counting(10), 37, 0.0, 1.0);
        let (out, ended) = run(&mut m, 4, 16);
        assert!(out[..37].iter().all(|&s| s == 0.0));
        assert_eq!(out[37..47], [0.0, 1., 2., 3., 4., 5., 6., 7., 8., 9.]);
        assert!(out[47..].iter().all(|&s| s == 0.0));
        assert_eq!(ended, [1]);
    }

    #[test]
    fn an_offset_skips_into_the_sound_and_a_late_start_plays_at_once() {
        let mut m = Mixer::new(RATE, 1.0);
        // 0.005 s into it at 1 kHz: from sample 5.
        play(&mut m, 1, counting(20), 3, 0.005, 1.0);
        let (out, _) = run(&mut m, 1, 8);
        assert_eq!(out[3..8], [5., 6., 7., 8., 9.]);
        // Asked for a frame already past: starts with the next block, from
        // its offset (Web Audio's rule).
        let mut m = Mixer::new(RATE, 1.0);
        play(&mut m, 2, counting(20), 2, 0.0, 1.0);
        let mut buf = vec![0.0; 16];
        m.render(&mut buf, 10, &mut |_| {});
        assert_eq!(buf[0], 0.0);
        assert_eq!(buf[2], 1.0);
    }

    #[test]
    fn stereo_and_mono_and_sums() {
        let mut m = Mixer::new(RATE, 1.0);
        let st = Arc::new(Pcm {
            rate: RATE,
            channels: 2,
            data: vec![0.25, -0.5, 0.25, -0.5],
        });
        play(&mut m, 1, st, 0, 0.0, 1.0);
        play(&mut m, 2, dc(2, 0.125), 0, 0.0, 2.0);
        let mut buf = vec![0.0; 4];
        m.render(&mut buf, 0, &mut |_| {});
        assert_eq!(buf, [0.5, -0.25, 0.5, -0.25]);
    }

    #[test]
    fn ramps_are_linear_and_land_on_their_frames() {
        let mut m = Mixer::new(RATE, 1.0);
        play(&mut m, 1, dc(100, 1.0), 0, 0.0, 0.0);
        let mut ended = |_| {};
        // As the web's preview: 0 at 10, up to 1 at 20, held from 30,
        // down to 0 at 40, stopped at 40.
        for e in [
            Event::Set { at: 10, value: 0.0 },
            Event::Ramp {
                end: 20,
                value: 1.0,
            },
            Event::Set { at: 30, value: 1.0 },
            Event::Ramp {
                end: 40,
                value: 0.0,
            },
        ] {
            m.apply(Command::Gain { id: 1, event: e }, 0, &mut ended);
        }
        m.apply(
            Command::Stop {
                id: 1,
                at: Some(40),
            },
            0,
            &mut ended,
        );
        let (out, ended) = run(&mut m, 7, 7);
        // What each frame should be.
        let want = |f: usize| match f {
            0..=10 => 0.0,
            11..=20 => (f - 10) as f32 / 10.0,
            21..=30 => 1.0,
            31..=39 => (40 - f) as f32 / 10.0,
            _ => 0.0,
        };
        for (f, &s) in out.iter().enumerate() {
            assert!((s - want(f)).abs() < 1e-6, "{f}: {s}");
        }
        assert!(out[40..].iter().all(|&s| s == 0.0));
        assert_eq!(ended, [1]);
    }

    #[test]
    fn a_ramp_with_nothing_before_it_starts_when_it_is_scheduled() {
        let mut p = Param::new(1.0);
        for f in 0..50 {
            p.advance(f);
        }
        p.schedule(
            Event::Ramp {
                end: 60,
                value: 0.0,
            },
            50,
        );
        assert_eq!(p.advance(50), 1.0);
        assert!((p.advance(55) - 0.5).abs() < 1e-6);
        assert_eq!(p.advance(60), 0.0);
        // A hold makes it start later, from the value then.
        let mut p = Param::new(0.8);
        p.schedule(Event::Hold { at: 10 }, 0);
        p.schedule(
            Event::Ramp {
                end: 20,
                value: 0.0,
            },
            0,
        );
        assert_eq!(p.advance(5), 0.8);
        assert_eq!(p.advance(10), 0.8);
        assert!((p.advance(15) - 0.4).abs() < 1e-6);
        assert_eq!(p.advance(25), 0.0);
        assert!(p.idle());
    }

    #[test]
    fn a_target_approaches_with_its_time_constant() {
        // The master's volume change: τ = 15 ms at 48 kHz = 720 frames.
        let mut p = Param::new(0.0);
        p.schedule(
            Event::Target {
                at: 0,
                value: 1.0,
                tau: 720.0,
            },
            0,
        );
        let mut v = 0.0;
        for f in 0..=720 {
            v = p.advance(f);
        }
        // 1 − e^-1 after one time constant.
        assert!((v - (1.0 - (-1f32).exp())).abs() < 2e-3, "{v}");
        for f in 721..10_000 {
            v = p.advance(f);
        }
        assert!((v - 1.0).abs() < 1e-4);
        // The next event ends it.
        p.schedule(
            Event::Set {
                at: 10_100,
                value: 0.25,
            },
            10_000,
        );
        assert_eq!(p.advance(10_100), 0.25);
        assert!(p.idle());
        // Cancelled, nothing changes.
        let mut p = Param::new(0.5);
        p.schedule(Event::Set { at: 10, value: 0.0 }, 0);
        p.cancel(5);
        assert_eq!(p.advance(20), 0.5);
    }

    #[test]
    fn the_master_scales_everything_and_follows_its_events() {
        let mut m = Mixer::new(RATE, 0.5);
        play(&mut m, 1, dc(100, 1.0), 0, 0.0, 1.0);
        play(&mut m, 2, dc(100, 1.0), 0, 0.0, 1.0);
        let mut nop = |_| {};
        m.apply(Command::Master(Event::Hold { at: 10 }), 0, &mut nop);
        m.apply(
            Command::Master(Event::Ramp {
                end: 20,
                value: 0.0,
            }),
            0,
            &mut nop,
        );
        let (out, _) = run(&mut m, 3, 10);
        assert_eq!(out[0], 1.0);
        assert_eq!(out[10], 1.0);
        assert!((out[15] - 0.5).abs() < 1e-6);
        assert_eq!(out[20], 0.0);
        assert_eq!(m.master(), 0.0);
    }

    #[test]
    fn stopping_at_once_fades_out_in_5_ms() {
        let mut m = Mixer::new(RATE, 1.0);
        play(&mut m, 1, dc(1_000, 1.0), 0, 0.0, 1.0);
        let mut buf = vec![0.0; 20];
        m.render(&mut buf, 0, &mut |_| {});
        m.apply(Command::Stop { id: 1, at: None }, 10, &mut |_| {});
        let mut ended = Vec::new();
        m.render(&mut buf, 10, &mut |v| ended.push(v.id));
        // 5 frames at 1 kHz: 1, 0.8, …, 0.2, then nothing.
        let left: Vec<f32> = buf.as_chunks::<2>().0.iter().map(|p| p[0]).collect();
        assert_eq!(left[..6], [1.0, 0.8, 0.6, 0.4, 0.2, 0.0]);
        assert_eq!(ended, [1]);
        assert_eq!(m.voices(), 0);
    }

    #[test]
    fn a_voice_stopped_before_it_sounds_is_never_heard() {
        // Stopped at once, and by stop_all, before its start frame.
        for all in [false, true] {
            let mut m = Mixer::new(RATE, 1.0);
            play(&mut m, 1, dc(1_000, 1.0), 100, 0.0, 1.0);
            let mut buf = vec![0.0; 64];
            m.render(&mut buf, 0, &mut |_| {});
            let mut ended = Vec::new();
            let c = if all {
                Command::StopAll
            } else {
                Command::Stop { id: 1, at: None }
            };
            m.apply(c, 32, &mut |v| ended.push(v.id));
            assert_eq!(ended, [1], "handed back at once");
            assert_eq!(m.voices(), 0);
            let (out, _) = run(&mut m, 10, 32);
            assert!(out.iter().all(|&s| s == 0.0));
        }
        // One already sounding is faded, the other ended at once.
        let mut m = Mixer::new(RATE, 1.0);
        play(&mut m, 1, dc(1_000, 1.0), 0, 0.0, 1.0);
        play(&mut m, 2, dc(1_000, 1.0), 500, 0.0, 1.0);
        let mut buf = vec![0.0; 20];
        m.render(&mut buf, 0, &mut |_| {});
        let mut ended = Vec::new();
        m.apply(Command::StopAll, 10, &mut |v| ended.push(v.id));
        assert_eq!(ended, [2]);
        assert_eq!(m.voices(), 1);
    }

    #[test]
    fn gains_are_cleaned_and_targets_arrive() {
        let mut m = Mixer::new(RATE, f32::NAN);
        play(&mut m, 1, dc(10, 0.5), 0, 0.0, f32::NAN);
        play(&mut m, 2, dc(10, 0.5), 0, 0.0, 1e6);
        play(&mut m, 3, dc(10, 0.5), 0, 0.0, -1.0);
        m.apply(
            Command::Master(Event::Set {
                at: 0,
                value: f32::INFINITY,
            }),
            0,
            &mut |_| {},
        );
        let mut buf = vec![0.0; 8];
        m.render(&mut buf, 0, &mut |_| {});
        // NaN and −1 count as 0, 1e6 as MAX_GAIN; the master's ∞ as 0.
        assert!(buf.iter().all(|s| s.is_finite()));
        assert_eq!(buf[0], 0.0);
        m.apply(
            Command::Master(Event::Set { at: 4, value: 1.0 }),
            4,
            &mut |_| {},
        );
        m.render(&mut buf, 4, &mut |_| {});
        assert_eq!(buf[0], 0.5 * MAX_GAIN);
        // A target lands on its value and ends, from 0.8 to 0.5 and to 0.
        for want in [0.5, 0.0] {
            let mut p = Param::new(0.8);
            p.schedule(
                Event::Target {
                    at: 0,
                    value: want,
                    tau: 720.0,
                },
                0,
            );
            let mut v = 0.8;
            for f in 0..48_000 {
                v = p.advance(f);
            }
            assert_eq!(v, want);
            assert!(p.idle());
        }
    }

    #[test]
    fn too_many_voices_are_refused_and_handed_back() {
        let mut m = Mixer::new(RATE, 1.0);
        for id in 0..MAX_VOICES as u32 {
            play(&mut m, id, dc(10, 0.0), 0, 0.0, 1.0);
        }
        let mut refused = Vec::new();
        let v = Voice::new(999, dc(10, 0.0), 0, 0.0, 1.0, RATE);
        m.apply(Command::Play(Box::new(v)), 0, &mut |v| refused.push(v.id));
        assert_eq!(refused, [999]);
    }

    #[test]
    fn another_rate_is_interpolated() {
        // A 1 kHz sound on a 2 kHz stream: each sample twice as long.
        let mut m = Mixer::new(2 * RATE, 1.0);
        play(&mut m, 1, counting(4), 0, 0.0, 1.0);
        let mut buf = vec![0.0; 16];
        m.render(&mut buf, 0, &mut |_| {});
        let left: Vec<f32> = buf.as_chunks::<2>().0.iter().map(|p| p[0]).collect();
        assert_eq!(left, [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.0]);
    }

    #[test]
    fn surround_folds_to_stereo() {
        let audio = DecodedAudio {
            sample_rate: 48_000,
            channels: vec![
                vec![1.0],
                vec![0.0],
                vec![1.0],
                vec![5.0],
                vec![0.0],
                vec![1.0],
            ],
            skipped_packets: 0,
        };
        let pcm = Pcm::from_decoded(audio);
        let k = std::f32::consts::FRAC_1_SQRT_2;
        assert_eq!(pcm.channels, 2);
        assert_eq!(pcm.data, [1.0 + k, 2.0 * k]);
    }
}
