//! Where an announcer's sound stops speaking and only its tail is left
//! (`docs/plans/announcers.md`, "A line ends where its voice ends"): the
//! voice counts a line as said until then, and the sound itself plays on
//! to its end, so the next line may start over a fading reverb
//! (the maintainer, 2026-11-17: "Tail doesn't block").
//!
//! The rule, [`VOICE_TAIL`] (`voice-tail-v0`), over the decoded samples
//! of every channel:
//!
//! 1. the sound's level is measured over 10 ms frames, every channel's
//!    samples together, as the leveler measures it, and its voiced level
//!    is the leveler's: the RMS of the frames within 30 dB of the loudest
//!    (`tidy`'s voiced RMS);
//! 2. its voice ends at the end of the last frame whose RMS is at least
//!    the voiced level less [`TAIL_DB`] (20 dB);
//! 3. what follows is its tail, unless that is shorter than [`MIN_TAIL`]
//!    (a quarter of a second): a sound whose last voiced frame is that
//!    near its end has no tail, its voice ending at its end.
//!
//! A silent sound (or one shorter than a frame) has no voiced frame and
//! no tail: it is counted whole, as before. The loudest frame is always
//! voiced, so no sound is all tail by this rule.
//!
//! Why these numbers (measured on our own pack, both languages, its 433
//! sounds as committed and as they were before the stadium processing,
//! the sounds decoded at 48 kHz): 20 dB under the voiced level, the
//! sounds before the processing end at most 0.17 s after their last frame
//! over it (the leveler keeps 0.1 s after the speech and fades it), while
//! every processed line but the opening's (ready, restart, here we go,
//! which kept their dry length) has 0.52 s or more after it; the voiced
//! end of a processed line falls a median 0.08 s (English) and 0.10 s
//! (Japanese) after the same speech's before (its reverb's first,
//! loudest moment); at 15 dB a dry line's quiet last word measured
//! 0.31 s, at 30 dB the reverb counted for 0.2 s of the voice.
//!
//! Comparisons are of powers (no logarithm), so the same samples give the
//! same tail on every system; the shells' decoders (a browser's, desktop's)
//! may differ in their samples by a frame's worth.

use super::tidy;

/// The rule's name, for the record.
pub const VOICE_TAIL: &str = "voice-tail-v0";
/// A frame this far under the sound's voiced level, dB, or further, is
/// not its voice.
pub const TAIL_DB: f64 = 20.0;
/// [`TAIL_DB`] as a share of the voiced level's power (written out: no
/// library's rounding).
const TAIL_POWER: f64 = 0.01;
/// What follows the voice is a tail only from this long, seconds.
pub const MIN_TAIL: f64 = 0.25;

/// The tail of a sound of planar `channels` at `rate` Hz, in seconds from
/// its voice's end to its end (0: none, the voice ends at its end).
pub fn tail<C: AsRef<[f32]>>(channels: &[C], rate: u32) -> f64 {
    let len = channels.first().map_or(0, |c| c.as_ref().len());
    let frame = tidy::frame_len(rate);
    let count = channels.len().max(1);
    let powers: Vec<f64> = (0..len / frame)
        .map(|i| {
            let mut sum = 0.0f64;
            for c in channels {
                for &s in &c.as_ref()[i * frame..(i + 1) * frame] {
                    sum += f64::from(s) * f64::from(s);
                }
            }
            sum / (frame * count) as f64
        })
        .collect();
    tail_of(&powers, frame, len, rate)
}

/// [`tail`] of `samples` of `channels` interleaved.
pub fn tail_interleaved(samples: &[f32], channels: usize, rate: u32) -> f64 {
    let channels = channels.max(1);
    let len = samples.len() / channels;
    let frame = tidy::frame_len(rate);
    let powers: Vec<f64> = samples
        .chunks_exact(frame * channels)
        .map(|f| {
            let mut sum = 0.0f64;
            for &s in f {
                sum += f64::from(s) * f64::from(s);
            }
            sum / (frame * channels) as f64
        })
        .collect();
    tail_of(&powers, frame, len, rate)
}

/// The tail of a sound `len` samples long whose frames of `frame` samples
/// have `powers`.
fn tail_of(powers: &[f64], frame: usize, len: usize, rate: u32) -> f64 {
    let voiced = tidy::voiced_of(powers);
    let floor = voiced * voiced * TAIL_POWER;
    if floor.is_nan() || floor <= 0.0 {
        return 0.0;
    }
    let Some(last) = powers.iter().rposition(|&p| p >= floor) else {
        return 0.0;
    };
    let after = len.saturating_sub((last + 1) * frame);
    let tail = after as f64 / f64::from(rate.max(1));
    if tail < MIN_TAIL { 0.0 } else { tail }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn seconds(s: f64) -> usize {
        (s * f64::from(RATE)).round() as usize
    }

    /// A voice: `voice` seconds of a 220 Hz tone at amplitude 0.5, with a
    /// little of a second tone so that it is not one pure wave; then a tail
    /// starting `start_db` under it and falling `fall_db` a second for
    /// `tail` seconds; then `silence` seconds of nothing.
    fn sound(voice: f64, start_db: f64, fall_db: f64, tail: f64, silence: f64) -> Vec<f32> {
        let tone = |i: usize| {
            let t = i as f64 / f64::from(RATE);
            0.5 * (t * 220.0 * std::f64::consts::TAU).sin()
                + 0.05 * (t * 1_310.0 * std::f64::consts::TAU).sin()
        };
        let mut out: Vec<f32> = (0..seconds(voice)).map(|i| tone(i) as f32).collect();
        let from = out.len();
        for k in 0..seconds(tail) {
            let t = k as f64 / f64::from(RATE);
            let db = -start_db - fall_db * t;
            out.push((tone(from + k) * 10f64.powf(db / 20.0)) as f32);
        }
        out.extend(std::iter::repeat_n(0.0, seconds(silence)));
        out
    }

    fn voiced_end(samples: &[f32]) -> f64 {
        samples.len() as f64 / f64::from(RATE) - tail(&[samples], RATE)
    }

    /// A tail that begins under the threshold is all tail: the voice ends
    /// with the burst, within a frame.
    #[test]
    fn a_quiet_tail_is_counted_from_the_voice_s_end() {
        let s = sound(1.0, 26.0, 40.0, 0.6, 0.0);
        let t = tail(&[&s], RATE);
        assert!((t - 0.6).abs() <= 0.01, "{t}");
        assert!((voiced_end(&s) - 1.0).abs() <= 0.01);
    }

    /// A reverb as loud as ours at first (12 dB under the voice, falling
    /// 80 dB a second) counts from where it falls 20 dB under the voiced
    /// level: about 0.11 s after the voice (its first 0.22 s count among
    /// the voiced frames, lowering that level by 0.8 dB), within 20 ms.
    #[test]
    fn a_loud_reverb_is_voice_until_it_falls_under_the_threshold() {
        let s = sound(1.0, 12.0, 80.0, 0.7, 0.0);
        let end = voiced_end(&s);
        assert!((end - 1.11).abs() <= 0.02, "{end}");
        assert!(tail(&[&s], RATE) >= MIN_TAIL);
    }

    /// No tail: the voice and the 0.1 s the leveler keeps after it (and a
    /// tail shorter than a quarter of a second) end where the sound does.
    #[test]
    fn a_sound_without_a_tail_ends_at_its_end() {
        for s in [
            sound(1.0, 0.0, 0.0, 0.0, 0.0),
            sound(1.0, 0.0, 0.0, 0.0, 0.1),
            sound(1.0, 26.0, 40.0, 0.2, 0.0),
            sound(0.3, 30.0, 60.0, 0.1, 0.12),
        ] {
            assert_eq!(tail(&[&s], RATE), 0.0);
        }
    }

    /// Silence, nothing, and less than a frame: no voice, no tail.
    #[test]
    fn silence_has_no_tail() {
        assert_eq!(tail(&[vec![0.0f32; 48_000]], RATE), 0.0);
        assert_eq!(tail::<Vec<f32>>(&[], RATE), 0.0);
        assert_eq!(tail(&[vec![0.5f32; 100]], RATE), 0.0);
        assert_eq!(tail_interleaved(&[], 2, RATE), 0.0);
    }

    /// Planar and interleaved, one channel or two, give the same tail; so
    /// does the voice in one channel alone.
    #[test]
    fn channels_planar_or_interleaved_agree() {
        let s = sound(0.8, 14.0, 60.0, 0.9, 0.05);
        let one = tail(&[&s], RATE);
        let silent = vec![0.0f32; s.len()];
        let two = tail(&[&s, &silent], RATE);
        let mixed: Vec<f32> = s.iter().flat_map(|&x| [x, 0.0]).collect();
        assert!(one > MIN_TAIL);
        assert_eq!(one, two);
        assert_eq!(tail_interleaved(&mixed, 2, RATE), two);
        assert_eq!(tail_interleaved(&s, 1, RATE), one);
    }
}
