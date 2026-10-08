//! Note colour schemes.
//!
//! - **Quantized**: by subdivision (DDR "NOTE", the StepMania default; the
//!   game's default is Vivid).
//! - **Vivid**: every note cycles through the rainbow once per four beats,
//!   its phase shifted by its position within the beat, so subdivisions stay
//!   apart while they shimmer, each arrow a gradient through the next part
//!   of the cycle from tail to tip (DDR "VIVID"; measured on footage, see
//!   `docs/research/rules-ddr-itg-dwi.md` §8.1).
//! - **Flat**: the same cycle with no shift, all notes alike (DDR "FLAT").
//! - **Rainbow**: by position within the beat, in four colour families,
//!   each arrow carrying a gradient of its family that flows once per beat
//!   (DDR "RAINBOW", 2014–A3: "arrows have different gradient cycles which
//!   correspond to note value").
//! - **Single**: one colour for everything.
//!
//! Vivid and Flat follow StepMania 5.1's `midi-vivid` noteskin and
//! `NoteDisplay::SetActiveFrame` (`src/NoteDisplay.cpp` 667–690 at
//! `825467b`): a 16-frame cycle over `TapNoteAnimationLength=4` beats of the
//! song, plus, for vivid, the note's beat fraction rounded down to a quarter
//! of the cycle. The 16 hues are those of that noteskin's frames, from
//! orange backwards round the wheel; saturation and brightness are this
//! skin's, and the hue moves smoothly instead of in 16 steps. Rainbow's
//! bands are DDR's (pink up to 1/16 of a measure into the beat, blue to
//! 1/8, purple to 3/16, orange to the next beat; a band includes its end),
//! which StepMania's `ProgressAlternate` colouring matches on 4ths, 8ths
//! and 16ths (it puts notes between them one band later).
//! Sources in `docs/research/rules-ddr-itg-dwi.md` §8.

use ddi_chart::Quantization;

/// Colour choice for notes.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub enum ColorScheme {
    #[default]
    Quantized,
    Vivid,
    Flat,
    Rainbow,
    /// One colour for everything.
    Single([f32; 4]),
}

pub fn quantization_color(q: Quantization) -> [f32; 4] {
    match q {
        Quantization::N4 => [0.95, 0.25, 0.3, 1.0],
        Quantization::N8 => [0.3, 0.5, 1.0, 1.0],
        Quantization::N12 => [0.7, 0.35, 0.95, 1.0],
        Quantization::N16 => [0.95, 0.85, 0.25, 1.0],
        Quantization::N24 => [0.95, 0.45, 0.75, 1.0],
        Quantization::N32 => [0.95, 0.6, 0.25, 1.0],
        Quantization::N48 => [0.3, 0.85, 0.9, 1.0],
        Quantization::N64 => [0.35, 0.9, 0.45, 1.0],
        Quantization::N192 => [0.75, 0.75, 0.75, 1.0],
    }
}

/// Hues (degrees) of the 16 `midi-vivid` frames, unwrapped so they only
/// decrease; frame 16 is frame 0 again.
const VIVID_HUES: [f32; 16] = [
    32.0, 26.0, 9.0, -24.0, -41.0, -52.0, -71.0, -137.0, -161.0, -168.0, -194.0, -246.0, -274.0,
    -287.0, -300.0, -316.0,
];

/// Beats per colour cycle (`TapNoteAnimationLength`).
const CYCLE_BEATS: f64 = 4.0;

/// How much of the cycle one vivid arrow spans from tip to tail. Measured on
/// DDR footage (VIVID, 170 BPM): an arrow runs from yellow at its tip
/// through orange to red at its tail, about a sixth of the wheel; three of
/// the 16 frames' hues cover about that.
const VIVID_SPAN: f64 = 3.0 / 16.0;

/// A gradient across an arrow, beyond its base colour.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Gradient {
    /// Down the screen from the base colour to `to` and back, shifted by
    /// `phase` cycles (Rainbow).
    Flow { to: [f32; 3], phase: f32 },
    /// Along the arrow, from the base colour at its tail to `tip` at its
    /// tip (Vivid, Flat): the cycle's later colour sits at the tail, so as
    /// the cycle moves the colour flows towards the tip.
    Ramp { tip: [f32; 3] },
}

/// Rainbow bands: pink, blue, purple, orange.
const RAINBOW: [[f32; 4]; 4] = [
    [1.0, 0.45, 0.75, 1.0],
    [0.3, 0.65, 1.0, 1.0],
    [0.65, 0.4, 1.0, 1.0],
    [1.0, 0.6, 0.25, 1.0],
];

/// The other end of each rainbow band's gradient: light pink, cyan,
/// magenta, yellow.
const RAINBOW_GRADIENT: [[f32; 3]; 4] = [
    [1.0, 0.82, 0.92],
    [0.35, 0.95, 1.0],
    [0.95, 0.45, 0.95],
    [1.0, 0.92, 0.35],
];

/// `[r, g, b, 1]` for a hue in degrees at this skin's saturation and value.
fn hue_color(hue: f32) -> [f32; 4] {
    let (s, v) = (0.75f32, 1.0f32);
    let h = hue.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m, 1.0]
}

/// The cycling colour at `phase` (cycles; only the fraction matters).
pub fn cycle_color(phase: f64) -> [f32; 4] {
    let p = phase.rem_euclid(1.0) as f32 * 16.0;
    let i = (p.floor() as usize).min(15);
    let t = p - i as f32;
    let from = VIVID_HUES[i];
    let to = if i == 15 {
        VIVID_HUES[0] - 360.0
    } else {
        VIVID_HUES[i + 1]
    };
    hue_color(from + (to - from) * t)
}

/// Rainbow band (0..4) of a note `beat_frac` into its beat: a band runs up
/// to and including its end, so a note on the beat ends the previous one.
pub fn rainbow_band(beat_frac: f32) -> usize {
    let quarters = beat_frac.rem_euclid(1.0) * 4.0;
    if quarters == 0.0 {
        return 3;
    }
    (quarters.ceil() as usize - 1).min(3)
}

impl ColorScheme {
    /// The gradient across an arrow of a scheme that draws one, beyond
    /// [`ColorScheme::note_color`]: Rainbow's flows down the screen once per
    /// beat; Vivid's and Flat's run along the arrow through the next part of
    /// the cycle.
    pub fn note_gradient(self, beat_frac: f32, song_beat: f64) -> Option<Gradient> {
        match self {
            ColorScheme::Rainbow => Some(Gradient::Flow {
                to: RAINBOW_GRADIENT[rainbow_band(beat_frac)],
                phase: song_beat.rem_euclid(1.0) as f32,
            }),
            ColorScheme::Vivid | ColorScheme::Flat => {
                let [r, g, b, _] = cycle_color(self.cycle_phase(beat_frac, song_beat) - VIVID_SPAN);
                Some(Gradient::Ramp { tip: [r, g, b] })
            }
            _ => None,
        }
    }

    /// Phase in cycles of a Vivid or Flat note's tail colour.
    fn cycle_phase(self, beat_frac: f32, song_beat: f64) -> f64 {
        let phase = song_beat / CYCLE_BEATS;
        match self {
            // QuantizeDown(fraction, 1 / AnimationLength).
            ColorScheme::Vivid => {
                phase + (f64::from(beat_frac.rem_euclid(1.0)) * CYCLE_BEATS).floor() / CYCLE_BEATS
            }
            _ => phase,
        }
    }

    /// Colour of a note in `quantization`, `beat_frac` into its beat, while
    /// the song is at `song_beat`.
    pub fn note_color(
        self,
        quantization: Quantization,
        beat_frac: f32,
        song_beat: f64,
    ) -> [f32; 4] {
        match self {
            ColorScheme::Quantized => quantization_color(quantization),
            ColorScheme::Single(c) => c,
            ColorScheme::Rainbow => RAINBOW[rainbow_band(beat_frac)],
            ColorScheme::Flat | ColorScheme::Vivid => {
                cycle_color(self.cycle_phase(beat_frac, song_beat))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hue_of(c: [f32; 4]) -> f32 {
        let (r, g, b) = (c[0], c[1], c[2]);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let d = max - min;
        let h = if max == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        h * 60.0
    }

    fn close(a: f32, b: f32) -> bool {
        let d = (a - b).rem_euclid(360.0);
        d.min(360.0 - d) < 0.5
    }

    #[test]
    fn the_cycle_hits_every_frame_hue_and_repeats_every_four_beats() {
        for (i, h) in VIVID_HUES.iter().enumerate() {
            let c = cycle_color(i as f64 / 16.0);
            assert!(close(hue_of(c), *h), "frame {i}: {} vs {h}", hue_of(c));
        }
        let s = ColorScheme::Flat;
        let a = s.note_color(Quantization::N4, 0.0, 1.3);
        let b = s.note_color(Quantization::N4, 0.0, 5.3);
        for k in 0..4 {
            assert!((a[k] - b[k]).abs() < 1e-5);
        }
        // Halfway between two frames the hue is halfway between them.
        assert!(close(hue_of(cycle_color(0.5 / 16.0)), 29.0));
    }

    #[test]
    fn flat_ignores_the_note_and_vivid_shifts_by_quarter_beats() {
        let flat = ColorScheme::Flat;
        assert_eq!(
            flat.note_color(Quantization::N4, 0.0, 2.0),
            flat.note_color(Quantization::N8, 0.5, 2.0)
        );
        let vivid = ColorScheme::Vivid;
        // An eighth note is half a cycle (two beats) ahead of a quarter note.
        let eighth = vivid.note_color(Quantization::N8, 0.5, 0.0);
        let quarter_later = vivid.note_color(Quantization::N4, 0.0, 2.0);
        for k in 0..4 {
            assert!((eighth[k] - quarter_later[k]).abs() < 1e-5);
        }
        // Rounded down to quarter beats: a 12th at 1/3 shifts like a 16th at 1/4.
        assert_eq!(
            vivid.note_color(Quantization::N12, 1.0 / 3.0, 0.7),
            vivid.note_color(Quantization::N16, 0.25, 0.7)
        );
        // A quarter note on the downbeat starts the cycle: orange.
        assert!(close(
            hue_of(vivid.note_color(Quantization::N4, 0.0, 0.0)),
            32.0
        ));
    }

    #[test]
    fn vivid_tips_show_what_the_tail_showed_earlier() {
        // The tail has the note's colour; the tip the colour the tail had
        // 3/16 of a cycle (0.75 beats) ago, so colour flows tail to tip.
        let vivid = ColorScheme::Vivid;
        let Some(Gradient::Ramp { tip }) = vivid.note_gradient(0.0, 0.75) else {
            panic!("vivid arrows ramp");
        };
        let earlier = vivid.note_color(Quantization::N4, 0.0, 0.0);
        for k in 0..3 {
            assert!((tip[k] - earlier[k]).abs() < 1e-5);
        }
        assert_ne!(vivid.note_color(Quantization::N4, 0.0, 0.75), earlier);
        assert!(matches!(
            ColorScheme::Flat.note_gradient(0.5, 1.0),
            Some(Gradient::Ramp { .. })
        ));
    }

    #[test]
    fn rainbow_bands_end_on_their_boundary() {
        assert_eq!(rainbow_band(0.0), 3); // on the beat: orange
        assert_eq!(rainbow_band(0.1), 0); // pink
        assert_eq!(rainbow_band(0.25), 0); // a 16th ends the pink band
        assert_eq!(rainbow_band(1.0 / 3.0), 1); // blue
        assert_eq!(rainbow_band(0.5), 1); // eighths are blue
        assert_eq!(rainbow_band(0.75), 2); // purple
        assert_eq!(rainbow_band(0.9), 3); // orange
        let r = ColorScheme::Rainbow;
        assert_eq!(r.note_color(Quantization::N4, 0.0, 3.0), RAINBOW[3]);
        assert_eq!(r.note_color(Quantization::N8, 0.5, 0.0), RAINBOW[1]);
        // Each band's gradient flows with the song's beat.
        assert_eq!(
            r.note_gradient(0.5, 2.25),
            Some(Gradient::Flow {
                to: RAINBOW_GRADIENT[1],
                phase: 0.25
            })
        );
        assert_eq!(
            r.note_gradient(0.0, 7.75),
            Some(Gradient::Flow {
                to: RAINBOW_GRADIENT[3],
                phase: 0.75
            })
        );
        assert_eq!(ColorScheme::Quantized.note_gradient(0.0, 1.0), None);
    }

    #[test]
    fn quantized_and_single_are_static() {
        let q = ColorScheme::Quantized;
        assert_eq!(
            q.note_color(Quantization::N8, 0.5, 0.0),
            q.note_color(Quantization::N8, 0.5, 9.9)
        );
        let s = ColorScheme::Single([0.1, 0.2, 0.3, 1.0]);
        assert_eq!(
            s.note_color(Quantization::N16, 0.75, 1.0),
            [0.1, 0.2, 0.3, 1.0]
        );
    }
}
