//! A chart's Groove radar values (stream, voltage, air, freeze, chaos), as
//! StepMania 5.1 computes its radar values (`NoteDataUtil`'s
//! `GetStreamRadarValue` and its siblings, StepMania's MIT-style sources;
//! the formulas restated here, no code copied), for song select's radar
//! under "Chart ratings: From the song" (`docs/plans/msd-ratings.md`).
//!
//! Each value is 0 to 1:
//! - **stream**: notes a second (taps and hold heads, the holds counted
//!   once more, as StepMania counts them) over 7;
//! - **voltage**: the densest eight beats' notes a beat, times the song's
//!   average beats a second, over 10;
//! - **air**: rows of two notes or more (jumps) a second;
//! - **freeze**: holds and rolls a second;
//! - **chaos**: rows off the eighth-note grid (12ths, 16ths and finer) a
//!   second, halved.
//!
//! `seconds` is the length the rates are taken over: StepMania takes the
//! music's; the game takes the chart's own, from its start to its last
//! note's end (the music is not decoded to list a song).

use crate::TICKS_PER_BEAT;
use crate::model::{Chart, NoteKind, Song};

/// The five values' names, in the radar's order (clockwise from the top).
pub const AXES: [&str; 5] = ["stream", "voltage", "air", "freeze", "chaos"];

/// `chart`'s Groove values over `seconds` (none where it has no length).
pub fn values(chart: &Chart, seconds: f64) -> [f32; 5] {
    if seconds.is_nan() || seconds <= 0.0 {
        return [0.0; 5];
    }
    let judged = |k: &NoteKind| !matches!(k, NoteKind::Mine | NoteKind::Fake);
    let notes: Vec<_> = chart.notes.iter().filter(|n| judged(&n.kind)).collect();
    let holds = notes
        .iter()
        .filter(|n| matches!(n.kind, NoteKind::HoldHead { .. } | NoteKind::RollHead { .. }))
        .count();
    // Rows: ticks with their note counts.
    let mut rows: Vec<(i64, usize)> = Vec::new();
    let mut ticks: Vec<i64> = notes.iter().map(|n| n.tick.0).collect();
    ticks.sort_unstable();
    for t in ticks {
        match rows.last_mut() {
            Some((last, n)) if *last == t => *n += 1,
            _ => rows.push((t, 1)),
        }
    }
    let s = seconds as f32;
    let stream = ((notes.len() + holds) as f32 / s / 7.0).min(1.0);
    // Voltage: the densest window of eight beats.
    let window = 8 * TICKS_PER_BEAT;
    let last = notes.iter().map(|n| n.end_tick().0).max().unwrap_or(0);
    let mut densest = 0.0f32;
    let mut start = 0i64;
    while start <= last {
        let n = notes.iter().filter(|x| x.tick.0 >= start && x.tick.0 < start + window).count()
            + notes
                .iter()
                .filter(|x| {
                    x.tick.0 >= start
                        && x.tick.0 < start + window
                        && matches!(x.kind, NoteKind::HoldHead { .. } | NoteKind::RollHead { .. })
                })
                .count();
        densest = densest.max(n as f32 / 8.0);
        start += window;
    }
    let beats_a_second = last as f32 / TICKS_PER_BEAT as f32 / s;
    let voltage = (densest * beats_a_second / 10.0).min(1.0);
    let jumps = rows.iter().filter(|(_, n)| *n >= 2).count();
    let air = (jumps as f32 / s).min(1.0);
    let freeze = (holds as f32 / s).min(1.0);
    let eighth = TICKS_PER_BEAT / 2;
    let chaotic = rows.iter().filter(|(t, _)| t.rem_euclid(eighth) != 0).count();
    let chaos = (chaotic as f32 / s * 0.5).min(1.0);
    [stream, voltage, air, freeze, chaos]
}

/// `song`'s chart `index`'s Groove values over the chart's own length.
pub fn of(song: &Song, index: usize) -> Option<[f32; 5]> {
    let chart = song.charts.get(index)?;
    let timing = chart.timing(song);
    let end = chart.notes.iter().map(|n| n.end_tick()).max()?;
    Some(values(chart, timing.seconds_at(end)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Difficulty, Note, Tick};

    fn chart(notes: Vec<Note>) -> Chart {
        Chart::plain("dance-single", Difficulty::Hard, 9, notes)
    }

    #[test]
    fn each_value_counts_its_own_kind_of_notes() {
        let b = TICKS_PER_BEAT;
        // Quarter notes for 60 beats: no jumps, no holds, no chaos.
        let quarters: Vec<Note> = (0..60).map(|k| Note::new(Tick(k * b), (k % 4) as u8, NoteKind::Tap)).collect();
        let v = values(&chart(quarters.clone()), 30.0);
        assert!((v[0] - 60.0 / 30.0 / 7.0).abs() < 1e-5, "{v:?}");
        assert_eq!((v[2], v[3], v[4]), (0.0, 0.0, 0.0));
        // Jumps make air; 16ths make chaos; holds make freeze.
        let mut more = quarters;
        more.extend((0..30).map(|k| Note::new(Tick(k * 2 * b), 3, NoteKind::Tap)));
        more.extend((0..20).map(|k| Note::new(Tick(k * 2 * b + b / 4), 1, NoteKind::Tap)));
        more.extend((0..10).map(|k| Note::new(Tick(k * 4 * b + b / 2), 2, NoteKind::HoldHead { end: Tick(k * 4 * b + b) })));
        let v = values(&chart(more), 30.0);
        assert!(v[2] > 0.5 && v[3] > 0.3 && v[4] > 0.3, "{v:?}");
        // Mines and fakes count for nothing; no length, nothing.
        let mines: Vec<Note> = (0..40).map(|k| Note::new(Tick(k * b), 0, NoteKind::Mine)).collect();
        assert_eq!(values(&chart(mines), 30.0)[0], 0.0);
        assert_eq!(values(&chart(Vec::new()), 0.0), [0.0; 5]);
    }
}
