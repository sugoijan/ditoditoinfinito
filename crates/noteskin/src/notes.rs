//! What the engine does to a note's actor before drawing it: the frame of
//! its animation from the song's beat (`NoteDisplay::SetActiveFrame`,
//! `src/NoteDisplay.cpp:667-695`) and the texture translation that picks
//! its color (`NoteDisplay::DrawActor`, `:1346-1381`), 5_1-new.

use crate::metrics::{NoteColorType, PartMetrics};

/// `NoteType` by the note's row: 4th = 0 … 192nd = 8 (`GetNoteType`,
/// `src/NoteTypes.cpp:128-139`; 192 rows a measure, from beat 0, whatever
/// the time signature).
pub fn note_type(beat: f32) -> u32 {
    // `BeatToNoteRow`: rounded to the nearest of 48 rows a beat (the
    // chart's ticks; the product taken exactly, in f64).
    let row = ddi_chart::Tick::from_beat_f64(f64::from(beat)).0;
    [48, 24, 16, 12, 8, 6, 4, 3]
        .iter()
        .position(|&d| row % d == 0)
        .map_or(8, |i| i as u32)
}

/// How far into its animation a note part is, `0..1`, at `song_beat` (or
/// `music_seconds` when the skin's animation is not beat based), for a note
/// at `note_beat`. Multiply by the sprite's animation length in seconds and
/// pass to `SetSecondsIntoAnimation`.
pub fn animation_percent(
    part: &PartMetrics,
    beat_based: bool,
    song_beat: f32,
    music_seconds: f32,
    note_beat: f32,
) -> f32 {
    let length = part.animation_length;
    if length.is_nan() || length <= 0.0 {
        // StepMania divides by it; a skin with 0 shows the first frame.
        return 0.0;
    }
    let clock = if beat_based { song_beat } else { music_seconds };
    let mut percent = (clock % length) / length;
    if part.animation_is_vivid {
        let fraction = note_beat % 1.0;
        let interval = 1.0 / length;
        percent += if part.vivid_rounded {
            // OpenITG: `Quantize` (half up, truncated toward 0) of the
            // fraction a quarter of the length's step early.
            let early = fraction - 0.25 / length;
            ((early + interval / 2.0) / interval).trunc() * interval
        } else {
            // `QuantizeDown`.
            (fraction / interval).floor() * interval
        };
        percent = percent.rem_euclid(1.0);
    } else if percent < 0.0 {
        percent += 1.0;
    }
    percent
}

/// The texture translation for a note part at `note_beat`, in
/// padded-texture units, if the part has one.
pub fn color_offset(part: &PartMetrics, note_beat: f32, is_addition: bool) -> Option<[f32; 2]> {
    let addition = if is_addition {
        part.addition_offset
    } else {
        [0.0; 2]
    };
    let spacing = part.color_spacing;
    if addition == [0.0; 2] && spacing == [0.0; 2] {
        return None;
    }
    let count = part.color_count as f32;
    let color = match part.color_type {
        // StepMania stops on an unknown type; it is colored by
        // denominator here.
        Some(NoteColorType::Denominator) | None => {
            (note_type(note_beat) as f32).clamp(0.0, (count - 1.0).max(0.0))
        }
        Some(NoteColorType::Progress) if count > 0.0 => (note_beat * count).ceil() % count,
        Some(NoteColorType::ProgressAlternate) if count > 0.0 => {
            let mut scaled = note_beat * count;
            if scaled == scaled.trunc() {
                // On a boundary: the previous frame.
                scaled += count - 1.0;
            }
            scaled.ceil() % count
        }
        // `fmod` by 0 or less: no color.
        Some(_) => 0.0,
    };
    let color = if color.is_finite() { color } else { 0.0 };
    Some([
        addition[0] + spacing[0] * color,
        addition[1] + spacing[1] * color,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(length: f32, vivid: bool) -> PartMetrics {
        PartMetrics {
            animation_length: length,
            animation_is_vivid: vivid,
            animation_on_children: false,
            addition_offset: [0.0; 2],
            color_spacing: [0.0, 0.125],
            color_count: 8,
            color_type: Some(NoteColorType::Denominator),
            note_color: false,
            vivid_rounded: false,
        }
    }

    #[test]
    fn note_types_by_row() {
        assert_eq!(note_type(0.0), 0);
        assert_eq!(note_type(1.5), 1);
        assert_eq!(note_type(1.0 / 3.0), 2);
        assert_eq!(note_type(0.25), 3);
        assert_eq!(note_type(1.0 / 6.0), 4);
        assert_eq!(note_type(0.125), 5);
        assert_eq!(note_type(1.0 / 12.0), 6);
        assert_eq!(note_type(1.0 / 16.0), 7);
        assert_eq!(note_type(1.0 / 48.0), 8);
        assert_eq!(note_type(-0.5), 1);
    }

    #[test]
    fn frames_follow_the_beat_and_vivid_shifts_them() {
        let p = part(4.0, false);
        assert_eq!(animation_percent(&p, true, 1.0, 0.0, 0.0), 0.25);
        assert_eq!(animation_percent(&p, true, -1.0, 0.0, 0.0), 0.75);
        assert_eq!(animation_percent(&p, false, 99.0, 2.0, 0.0), 0.5);
        // Vivid: an 8th (fraction .5) is half a cycle on, a 16th a quarter.
        let v = part(4.0, true);
        assert_eq!(animation_percent(&v, true, 1.0, 0.0, 3.5), 0.75);
        assert_eq!(animation_percent(&v, true, 1.0, 0.0, 3.25), 0.5);
        // OpenITG's rounds a quarter step early: a note just before the
        // beat takes the beat's frame.
        let mut rounded = v;
        rounded.vivid_rounded = true;
        assert_eq!(animation_percent(&rounded, true, 1.0, 0.0, 3.5), 0.75);
        assert_eq!(animation_percent(&rounded, true, 1.0, 0.0, 3.99), 0.25);
        assert_eq!(animation_percent(&v, true, 1.0, 0.0, 3.99), 0.0);
        // With a length of 1, vivid does nothing.
        let one = part(1.0, true);
        assert_eq!(animation_percent(&one, true, 0.5, 0.0, 3.5), 0.5);
        assert_eq!(
            animation_percent(&part(0.0, false), true, 0.5, 0.0, 0.0),
            0.0
        );
    }

    #[test]
    fn colors_by_division_and_progress() {
        let p = part(1.0, false);
        // dance/default's `.125` steps down a 1x8 sheet.
        assert_eq!(color_offset(&p, 2.0, false), Some([0.0, 0.0]));
        assert_eq!(color_offset(&p, 2.25, false), Some([0.0, 0.375]));
        // 192nds clamp to color 7.
        assert_eq!(color_offset(&p, 1.0 / 48.0, false), Some([0.0, 0.875]));
        let mut r = p;
        r.color_type = Some(NoteColorType::Progress);
        assert_eq!(color_offset(&r, 0.3, false), Some([0.0, 0.125 * 3.0]));
        r.color_type = Some(NoteColorType::ProgressAlternate);
        assert_eq!(color_offset(&r, 0.25, false), Some([0.0, 0.125]));
        r.color_count = 0;
        assert_eq!(color_offset(&r, 0.25, false), Some([0.0, 0.0]));
        let mut flat = p;
        flat.color_spacing = [0.0; 2];
        assert_eq!(color_offset(&flat, 0.25, false), None);
        flat.addition_offset = [0.5, 0.0];
        assert_eq!(color_offset(&flat, 0.25, true), Some([0.5, 0.0]));
    }
}
