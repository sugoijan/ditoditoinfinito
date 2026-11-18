//! A chart's rows as Etterna feeds them to its calculator (our own code,
//! after Etterna's `NoteData::SerializeNoteData2`): rows in judged time
//! (not in a warp or a fake segment) with a tap or a hold or roll head
//! give their lanes as bits; mines, lifts, fakes and keysounds give no
//! bits but still count as a row, so that the first row of any kind is
//! the time origin, as Etterna's `etaner[idx] - etaner[0]` makes it.
//! Times are seconds at rate 1.0, from our exact timing (Etterna's own
//! times are `float` sums, a few microseconds apart at most).

use ddi_chart::{Chart, Layout, Song, TimingMap};

use crate::note_data::{NUM_SKILLSET, NoteInfo};

/// Whether charts of layout `id` have an MSD: four lanes for one player
/// (of the built-in layouts, dance-single; the generated versions on it
/// too). The same charts the CPU plays.
pub fn four_keys(layout_id: &str) -> bool {
    matches!(Layout::builtin(layout_id), Some(l) if l.players == 1 && l.lanes.len() == 4)
}

/// The rows of `chart` under `timing`.
pub fn rows(chart: &Chart, timing: &TimingMap) -> Vec<NoteInfo> {
    let mut walk = timing.walk();
    let mut marked: Vec<(f64, u32)> = Vec::new();
    let mut i = 0;
    let notes = &chart.notes;
    while i < notes.len() {
        let tick = notes[i].tick;
        let mut bits = 0u32;
        let start = i;
        while i < notes.len() && notes[i].tick == tick {
            let n = &notes[i];
            if n.kind.is_press_judged() && n.lane < 32 {
                bits |= 1 << n.lane;
            }
            i += 1;
        }
        debug_assert!(i > start);
        if !timing.judgeable(tick) {
            continue;
        }
        let seconds = if tick.0 >= 0 {
            walk.seconds(tick.beat())
        } else {
            timing.seconds_at(tick)
        };
        marked.push((seconds, bits));
    }
    let Some(&(origin, _)) = marked.first() else {
        return Vec::new();
    };
    marked
        .into_iter()
        .filter(|&(_, bits)| bits != 0)
        .map(|(seconds, notes)| NoteInfo {
            notes,
            row_time: (seconds - origin) as f32,
        })
        .collect()
}

/// The listed MSD (rate 1.0, goal 93%) of `song`'s chart, or `None` when
/// its layout has none.
pub fn chart_msd(song: &Song, chart: &Chart) -> Option<[f32; NUM_SKILLSET]> {
    four_keys(&chart.layout)
        .then(|| crate::msd(&rows(chart, chart.timing(song)), crate::LISTED_RATE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_chart::{Difficulty, Note, NoteKind, Tick};

    fn note(beat_48: i64, lane: u8, kind: NoteKind) -> Note {
        Note {
            tick: Tick(beat_48),
            lane,
            kind,
            keysound: None,
            color: None,
            speed_mul: None,
        }
    }

    #[test]
    fn taps_and_heads_give_bits_and_a_mine_row_is_the_origin() {
        let chart = Chart::plain(
            "dance-single",
            Difficulty::Hard,
            9,
            vec![
                note(0, 1, NoteKind::Mine),
                note(48, 0, NoteKind::Tap),
                note(48, 3, NoteKind::HoldHead { end: Tick(96) }),
                note(96, 2, NoteKind::Lift),
                note(144, 2, NoteKind::RollHead { end: Tick(192) }),
                note(144, 1, NoteKind::Fake),
            ],
        );
        let timing = TimingMap::constant(120.0, 0.0);
        let rows = rows(&chart, &timing);
        assert_eq!(
            rows,
            vec![
                NoteInfo {
                    notes: 0b1001,
                    row_time: 0.5
                },
                NoteInfo {
                    notes: 0b0100,
                    row_time: 1.5
                },
            ]
        );
        assert!(four_keys("dance-single"));
        assert!(!four_keys("dance-double"));
        assert!(!four_keys("pump-single"));
        assert!(!four_keys("danoni-5"));
    }
}
