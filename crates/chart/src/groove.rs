//! A chart's Groove radar values (stream, voltage, air, freeze, chaos), as
//! StepMania 5.1 computes its radar values, for song select's radar under
//! "Chart ratings: From the song" (`docs/plans/msd-ratings.md`).
//!
//! The logic of `NoteDataUtil::CalculateRadarValues` (`src/NoteDataUtil.cpp`
//! of StepMania's `5_1-new` branch, commit `7cbaff8c`; MIT-style licence,
//! `LICENSES/LicenseRef-StepMania.txt`, credited in
//! `credits/THIRD_PARTY.toml`), restated in Rust, with
//! `NoteData::GetLastRow` and `GetNoteType` beside it:
//!
//! - the notes are walked row by row; a row whose timing judges it
//!   (outside warps and fake segments, `IsJudgableAtRow`) counts its taps,
//!   hold and roll heads and lifts (StepMania's attacks have no kind here);
//!   mines, fakes and anything on a row that is not judged count for none
//!   of the five but chaos;
//! - **stream**: those notes a second, over 7;
//! - **voltage**: the most of them in any window of eight beats (from a
//!   note's row back eight beats, both ends in), over 8, times the chart's
//!   average beats a second (its last row, a hold's end included, as beats
//!   over the seconds: "pretend the beats are evenly spaced"), over 10;
//! - **air**: jumps a second (a judged row's second note counts one);
//! - **freeze**: holds a second (rolls are counted apart and left out);
//! - **chaos**: rows (any note at all, judged or not) off the eighth-note
//!   grid, a second, halved.
//!
//! The values are not capped: StepMania writes them so (`#RADARVALUES`) and
//! its radar clamps each to 0 to 1 as it draws (`GrooveRadar`'s
//! `clamp(rv, 0, 1)`), which song select's radar does too.
//!
//! `seconds` is what StepMania passes as the music's length; the game
//! passes the song's length as song select shows it (the library's
//! `length`: the end of the last note of the song's playable charts; the
//! music is not decoded to list a song), else the chart's own span.

use crate::TICKS_PER_BEAT;
use crate::model::{Chart, NoteKind, Song};

/// The five values' names, in the radar's order (clockwise from the top).
pub const AXES: [&str; 5] = ["stream", "voltage", "air", "freeze", "chaos"];

/// StepMania's voltage window, in beats.
const VOLTAGE_BEATS: i64 = 8;

/// `chart`'s Groove values over `seconds`, raw (0 to 1 drawn; may be
/// more), none where it has no length.
pub fn values(chart: &Chart, timing: &crate::timing::TimingMap, seconds: f64) -> [f32; 5] {
    let mut notes: Vec<_> = chart.notes.iter().collect();
    notes.sort_by_key(|n| (n.tick, n.lane));
    // Judged taps, hold and roll heads and lifts, by row.
    let mut total = 0usize;
    let mut jumps = 0usize;
    let mut holds = 0usize;
    let mut chaos_rows = 0usize;
    let mut counted_rows: Vec<i64> = Vec::new();
    let mut row: Option<i64> = None;
    let mut on_row = 0usize;
    let mut judgable = false;
    for n in &notes {
        let t = n.tick.0;
        if row != Some(t) {
            row = Some(t);
            on_row = 0;
            judgable = timing.judgeable(n.tick);
            // `GetNoteType(row) >= NOTE_TYPE_12TH`: neither a quarter's
            // nor an eighth's row (48 rows a beat, as StepMania's).
            if t.rem_euclid(TICKS_PER_BEAT / 2) != 0 {
                chaos_rows += 1;
            }
        }
        if !judgable {
            continue;
        }
        match n.kind {
            NoteKind::Tap | NoteKind::HoldHead { .. } | NoteKind::RollHead { .. } | NoteKind::Lift => {
                total += 1;
                on_row += 1;
                counted_rows.push(t);
                if on_row == 2 {
                    jumps += 1;
                }
                if matches!(n.kind, NoteKind::HoldHead { .. }) {
                    holds += 1;
                }
            }
            _ => {}
        }
    }
    // Voltage: the most counted notes from any one's row back eight
    // beats, both ends in (the rolling window keeps rows not older than
    // the current row less the window).
    let window = VOLTAGE_BEATS * TICKS_PER_BEAT;
    let mut most = 0usize;
    let mut from = 0usize;
    for (i, &r) in counted_rows.iter().enumerate() {
        while counted_rows[from] < r - window {
            from += 1;
        }
        most = most.max(i + 1 - from);
    }
    if seconds.is_nan() || seconds <= 0.0 || seconds.is_infinite() {
        return [0.0; 5];
    }
    let s = seconds as f32;
    // `NoteData::GetLastRow`: each lane's last note, a hold's or a roll's
    // end where that is one, the latest of them.
    let mut last_in_lane: Vec<Option<&crate::model::Note>> = Vec::new();
    for n in &notes {
        let lane = n.lane as usize;
        if last_in_lane.len() <= lane {
            last_in_lane.resize(lane + 1, None);
        }
        last_in_lane[lane] = Some(n);
    }
    let last_row = last_in_lane
        .iter()
        .flatten()
        .map(|n| n.end_tick().0)
        .max()
        .unwrap_or(0)
        .max(0);
    let beats_a_second = last_row as f32 / TICKS_PER_BEAT as f32 / s;
    let stream = total as f32 / s / 7.0;
    let voltage = most as f32 / VOLTAGE_BEATS as f32 * beats_a_second / 10.0;
    let air = jumps as f32 / s;
    let freeze = holds as f32 / s;
    let chaos = chaos_rows as f32 / s * 0.5;
    [stream, voltage, air, freeze, chaos]
}

/// The seconds `song`'s chart `index`'s values are taken over: `length`
/// (the song's, as song select shows it) where it has one, else the
/// chart's own span, its start to its last note's end.
pub fn seconds_for(song: &Song, index: usize, length: f64) -> f64 {
    if length > 0.0 && length.is_finite() {
        return length;
    }
    let Some(chart) = song.charts.get(index) else {
        return 0.0;
    };
    let timing = chart.timing(song);
    chart
        .notes
        .iter()
        .map(|n| n.end_tick())
        .max()
        .map_or(0.0, |end| timing.seconds_at(end))
}

/// `song`'s chart `index`'s Groove values over the song's `length` (else
/// the chart's own span, [`seconds_for`]).
pub fn of(song: &Song, index: usize, length: f64) -> Option<[f32; 5]> {
    let chart = song.charts.get(index)?;
    Some(values(chart, chart.timing(song), seconds_for(song, index, length)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Difficulty, Note, Tick};
    use crate::timing::TimingMap;

    fn chart(notes: Vec<Note>) -> Chart {
        Chart::plain("dance-single", Difficulty::Hard, 9, notes)
    }

    fn timing() -> TimingMap {
        TimingMap::constant(120.0, 0.0)
    }

    const B: i64 = TICKS_PER_BEAT;

    #[test]
    fn stream_counts_each_judged_note_once() {
        // Quarter notes for 60 beats, ten of them holds: no jumps, no
        // chaos; the holds counted once (as StepMania's `total_taps`).
        let notes: Vec<Note> = (0..60)
            .map(|k| {
                let kind = if k % 6 == 0 {
                    NoteKind::HoldHead { end: Tick(k * B + B / 2) }
                } else {
                    NoteKind::Tap
                };
                Note::new(Tick(k * B), (k % 4) as u8, kind)
            })
            .collect();
        let v = values(&chart(notes), &timing(), 30.0);
        assert!((v[0] - 60.0 / 30.0 / 7.0).abs() < 1e-6, "{v:?}");
        assert!((v[3] - 10.0 / 30.0).abs() < 1e-6, "{v:?}");
        assert_eq!((v[2], v[4]), (0.0, 0.0));
    }

    #[test]
    fn freeze_leaves_rolls_out_and_lifts_are_notes() {
        let notes = vec![
            Note::new(Tick(0), 0, NoteKind::RollHead { end: Tick(B) }),
            Note::new(Tick(2 * B), 1, NoteKind::HoldHead { end: Tick(3 * B) }),
            Note::new(Tick(4 * B), 2, NoteKind::Lift),
        ];
        let v = values(&chart(notes), &timing(), 10.0);
        assert!((v[3] - 0.1).abs() < 1e-6, "one hold, no roll: {v:?}");
        assert!((v[0] - 3.0 / 10.0 / 7.0).abs() < 1e-6, "{v:?}");
    }

    #[test]
    fn air_is_jumps_and_chaos_counts_any_row_off_the_eighths() {
        let notes = vec![
            // A jump, a hand (still one jump), a 16th, a 12th, a mine on a
            // 24th and a fake on a 16th.
            Note::new(Tick(0), 0, NoteKind::Tap),
            Note::new(Tick(0), 1, NoteKind::Tap),
            Note::new(Tick(B), 0, NoteKind::Tap),
            Note::new(Tick(B), 1, NoteKind::Tap),
            Note::new(Tick(B), 2, NoteKind::Tap),
            Note::new(Tick(B + B / 2), 3, NoteKind::Tap),
            Note::new(Tick(2 * B + B / 4), 3, NoteKind::Tap),
            Note::new(Tick(3 * B + B / 3), 3, NoteKind::Tap),
            Note::new(Tick(4 * B + B / 6), 0, NoteKind::Mine),
            Note::new(Tick(5 * B + B / 4), 0, NoteKind::Fake),
        ];
        let v = values(&chart(notes), &timing(), 2.0);
        assert!((v[2] - 1.0).abs() < 1e-6, "two jump rows: {v:?}");
        assert!((v[4] - 4.0 / 2.0 * 0.5).abs() < 1e-6, "four rows off the grid: {v:?}");
        // Mines and fakes are no notes.
        assert!((v[0] - 8.0 / 2.0 / 7.0).abs() < 1e-6, "{v:?}");
    }

    #[test]
    fn voltage_is_the_densest_rolling_eight_beats() {
        // Sixteen notes in beats 10 to 17.5 (eighths), none elsewhere but
        // a note at beat 0 and one at 39: the window catches the sixteen
        // and the last row is beat 39.
        let mut notes = vec![Note::new(Tick(0), 0, NoteKind::Tap)];
        notes.extend((0..16).map(|k| Note::new(Tick(10 * B + k * B / 2), (k % 4) as u8, NoteKind::Tap)));
        notes.push(Note::new(Tick(39 * B), 0, NoteKind::Tap));
        let v = values(&chart(notes.clone()), &timing(), 20.0);
        let expected = 16.0 / 8.0 * (39.0 / 20.0) / 10.0;
        assert!((v[1] - expected).abs() < 1e-6, "{v:?} against {expected}");
        // A window from a row back eight beats has both ends in: a note
        // exactly eight beats before another is in its window.
        let pair = vec![Note::new(Tick(0), 0, NoteKind::Tap), Note::new(Tick(8 * B), 1, NoteKind::Tap)];
        let v = values(&chart(pair), &timing(), 4.0);
        assert!((v[1] - 2.0 / 8.0 * (8.0 / 4.0) / 10.0).abs() < 1e-6, "{v:?}");
    }

    #[test]
    fn the_last_row_takes_a_hold_s_end_and_values_are_not_capped() {
        let mut notes: Vec<Note> = (0..400).map(|k| Note::new(Tick(k * B / 4), (k % 4) as u8, NoteKind::Tap)).collect();
        notes.push(Note::new(Tick(100 * B), 0, NoteKind::HoldHead { end: Tick(140 * B) }));
        let v = values(&chart(notes), &timing(), 10.0);
        assert!(v[0] > 1.0, "uncapped: {v:?}");
        // 33 notes in a window (sixteenths, both ends in), 140 beats over
        // 10 s.
        let expected = 33.0 / 8.0 * 14.0 / 10.0;
        assert!((v[1] - expected).abs() < 1e-4, "{v:?} against {expected}");
    }

    #[test]
    fn notes_the_timing_does_not_judge_count_only_for_chaos() {
        let mut t = timing();
        t.fakes.push(crate::timing::FakeSegment { tick: Tick(0), length: Tick(4 * B) });
        let notes = vec![
            Note::new(Tick(B / 4), 0, NoteKind::Tap),
            Note::new(Tick(B), 0, NoteKind::Tap),
            Note::new(Tick(B), 1, NoteKind::Tap),
            Note::new(Tick(8 * B), 2, NoteKind::Tap),
        ];
        let v = values(&chart(notes), &t, 1.0);
        assert!((v[0] - 1.0 / 7.0).abs() < 1e-6, "{v:?}");
        assert_eq!(v[2], 0.0, "the jump is in the fake segment");
        assert!((v[4] - 0.5).abs() < 1e-6, "{v:?}");
    }

    #[test]
    fn no_length_no_values() {
        let notes = vec![Note::new(Tick(0), 0, NoteKind::Tap)];
        assert_eq!(values(&chart(notes), &timing(), 0.0), [0.0; 5]);
        assert_eq!(values(&chart(Vec::new()), &timing(), 30.0), [0.0; 5]);
    }
}
