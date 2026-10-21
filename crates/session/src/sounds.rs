//! A play's event sounds (`docs/plans/rules-options.md`, step 5): the
//! sound of a mine or shock stepped on, and the assist clap on every row
//! of notes (StepMania's `GameplayAssist` "clap", the assist tick of 3.9),
//! both made here in code (no audio files) and played through the
//! session's [`AudioBackend`]. The clap is scheduled ahead on the audio
//! clock at the context time the music's own sample for its row plays,
//! so it lands on the heard beat whatever the output latency; the mine
//! sound plays as the step is judged, as StepMania's `Player` plays its
//! "mine" sound. A noteskin's `Def.Sound` stays silent.

use ddi_chart::NoteKind;
use ddi_engine::judge::{JudgeEvent, JudgeEventKind, JudgedNote};
use ddi_platform::AudioBackend;

/// How far ahead claps are scheduled, seconds: past a long frame, and
/// late enough that a play quit or failed stops what is left (StepMania
/// asks for the tick a quarter second plus the latency ahead).
pub const CLAP_LOOKAHEAD: f64 = 0.3;

/// A clap scheduled this late (a long frame, a start part-way) is left
/// out rather than heard off the beat.
const CLAP_LATE: f64 = 0.01;

/// The clap's length, seconds.
const CLAP_SECONDS: f32 = 0.12;

/// The mine sound's length, seconds.
const MINE_SECONDS: f32 = 0.4;

/// A small deterministic noise source (xorshift32), so the sounds are the
/// same every time.
struct Noise(u32);

impl Noise {
    /// In `-1..1`.
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// A biquad band-pass (constant peak gain, the RBJ cookbook's).
struct BandPass {
    b: [f32; 3],
    a: [f32; 2],
    x: [f32; 2],
    y: [f32; 2],
}

impl BandPass {
    fn new(rate: f32, hz: f32, q: f32) -> BandPass {
        let w = std::f32::consts::TAU * hz / rate;
        let alpha = w.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        BandPass {
            b: [alpha / a0, 0.0, -alpha / a0],
            a: [-2.0 * w.cos() / a0, (1.0 - alpha) / a0],
            x: [0.0; 2],
            y: [0.0; 2],
        }
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.b[1] * self.x[0] + self.b[2] * self.x[1]
            - self.a[0] * self.y[0]
            - self.a[1] * self.y[1];
        self.x = [x, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }
}

/// Scales `data` so its loudest sample is `peak`.
fn normalized(mut data: Vec<f32>, peak: f32) -> Vec<f32> {
    let max = data.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    if max > 0.0 {
        for v in &mut data {
            *v *= peak / max;
        }
    }
    data
}

/// The assist clap at `rate`: a hand clap's few quick bursts of
/// band-passed noise and a short tail, its first burst at the first
/// sample (the beat).
pub fn clap(rate: f32) -> Vec<f32> {
    let n = (CLAP_SECONDS * rate) as usize;
    let mut noise = Noise(0x2545_f491);
    let mut low = BandPass::new(rate, 1100.0, 1.6);
    let mut high = BandPass::new(rate, 2400.0, 2.2);
    let data = (0..n)
        .map(|i| {
            let t = i as f32 / rate;
            // Three hands, 9 ms apart, then the room.
            let bursts: f32 = (0..3)
                .map(|k| {
                    let at = t - k as f32 * 0.009;
                    if at >= 0.0 {
                        (-at / 0.0035).exp() * (1.0 - 0.2 * k as f32)
                    } else {
                        0.0
                    }
                })
                .sum();
            let tail = if t >= 0.018 {
                0.45 * (-(t - 0.018) / 0.03).exp()
            } else {
                0.0
            };
            let x = noise.next();
            (low.run(x) + 0.6 * high.run(x)) * (bursts + tail)
        })
        .collect();
    normalized(data, 0.7)
}

/// The mine sound at `rate`: a low thump falling in pitch under a short
/// burst of crackle.
pub fn mine(rate: f32) -> Vec<f32> {
    let n = (MINE_SECONDS * rate) as usize;
    let mut noise = Noise(0x9e37_79b9);
    let mut crackle = BandPass::new(rate, 900.0, 0.8);
    let mut phase = 0.0f32;
    let data = (0..n)
        .map(|i| {
            let t = i as f32 / rate;
            let hz = 48.0 + 140.0 * (-t / 0.05).exp();
            phase += std::f32::consts::TAU * hz / rate;
            let thump = phase.sin() * (-t / 0.12).exp();
            let burst = crackle.run(noise.next()) * (-t / 0.025).exp() * 2.5;
            // A 2 ms fade in: no click at the start.
            (thump + burst) * (t / 0.002).min(1.0)
        })
        .collect();
    normalized(data, 0.8)
}

/// The rows the assist clap sounds on: the song seconds of the rows with
/// a tap or the head of a hold or roll judged on them
/// (`IsThereATapOrHoldHeadAtRow` on a judgeable row), in order, once each.
pub fn clap_rows(notes: &[JudgedNote]) -> Vec<f64> {
    let mut rows: Vec<f64> = notes
        .iter()
        .filter(|n| {
            n.judgeable
                && matches!(
                    n.note.kind,
                    NoteKind::Tap | NoteKind::HoldHead { .. } | NoteKind::RollHead { .. }
                )
        })
        .map(|n| n.seconds)
        .collect();
    rows.sort_by(f64::total_cmp);
    rows.dedup();
    rows
}

/// A play's event sounds and what is left to schedule.
pub struct EventSounds<B> {
    clap: Option<B>,
    mine: Option<B>,
    volume: f32,
    /// [`clap_rows`], and the first not scheduled yet.
    rows: Vec<f64>,
    next: usize,
    /// How many claps were scheduled and mine sounds played (the web's
    /// debug state shows them).
    pub claps: u32,
    pub mines: u32,
}

impl<B> EventSounds<B> {
    /// The sounds the settings ask for (`clap`, `mine`), made at
    /// `audio`'s rate; claps on the rows of `notes`, from `song_offset`
    /// on (the rows before it are not played).
    pub fn new<A: AudioBackend<Buffer = B>>(
        audio: &A,
        clap_on: bool,
        mine_on: bool,
        volume: f32,
        notes: &[JudgedNote],
        song_offset: f64,
    ) -> EventSounds<B> {
        let rate = audio.sample_rate() as f32;
        let make = |on: bool, f: fn(f32) -> Vec<f32>| {
            on.then(|| audio.buffer_from_pcm(vec![f(rate)], f64::from(rate)).ok())
                .flatten()
        };
        let clap = make(clap_on, clap);
        let rows = if clap.is_some() {
            clap_rows(notes)
        } else {
            Vec::new()
        };
        let next = rows.partition_point(|&t| t < song_offset);
        EventSounds {
            clap,
            mine: make(mine_on, mine),
            volume,
            rows,
            next,
            claps: 0,
            mines: 0,
        }
    }

    /// No sounds at all (calibration: nothing may cue the player).
    pub fn none() -> EventSounds<B> {
        EventSounds {
            clap: None,
            mine: None,
            volume: 0.0,
            rows: Vec::new(),
            next: 0,
            claps: 0,
            mines: 0,
        }
    }

    /// Schedules the claps due before `now + CLAP_LOOKAHEAD` on the
    /// audio clock: `now` the context time now, `ctx_of` a song second's
    /// context time (where the music plays it).
    pub fn schedule<A: AudioBackend<Buffer = B>>(
        &mut self,
        audio: &mut A,
        now: f64,
        ctx_of: impl Fn(f64) -> f64,
    ) {
        let Some(clap) = &self.clap else {
            return;
        };
        while let Some(&row) = self.rows.get(self.next) {
            let at = ctx_of(row);
            if at >= now + CLAP_LOOKAHEAD {
                break;
            }
            self.next += 1;
            if at >= now - CLAP_LATE && audio.play(clap, at, 0.0, self.volume).is_ok() {
                self.claps += 1;
            }
        }
    }

    /// The mine sound, now, when `events` step on a mine or a shock (once
    /// however many).
    pub fn judged<A: AudioBackend<Buffer = B>>(
        &mut self,
        audio: &mut A,
        now: f64,
        events: &[JudgeEvent],
    ) {
        let Some(mine) = &self.mine else {
            return;
        };
        let stepped = events
            .iter()
            .any(|e| matches!(e.kind, JudgeEventKind::HitMine | JudgeEventKind::HitShock));
        if stepped && audio.play(mine, now, 0.0, self.volume).is_ok() {
            self.mines += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(data: &[f32]) -> f32 {
        data.iter().fold(0.0f32, |m, v| m.max(v.abs()))
    }

    #[test]
    fn the_sounds_are_short_loud_at_once_and_fade_out() {
        for rate in [44_100.0, 48_000.0] {
            let c = clap(rate);
            assert_eq!(c.len(), (CLAP_SECONDS * rate) as usize);
            assert!((peak(&c) - 0.7).abs() < 1e-6);
            // On the beat: loud within its first 2 ms, quiet at its end.
            let ms = (rate / 1000.0) as usize;
            assert!(peak(&c[..2 * ms]) > 0.2, "{}", peak(&c[..2 * ms]));
            assert!(peak(&c[c.len() - 5 * ms..]) < 0.05);
            let m = mine(rate);
            assert!((peak(&m) - 0.8).abs() < 1e-6);
            assert!(m[0].abs() < 1e-3, "no click at the start");
            assert!(peak(&m[m.len() - 5 * ms..]) < 0.05);
            assert!(c.iter().chain(&m).all(|v| v.is_finite()));
            // The same every time.
            assert_eq!(clap(rate), c);
        }
    }
}
