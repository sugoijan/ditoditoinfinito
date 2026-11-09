// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use ddi_chart::{Layout, Note, NoteKind, Tick};
use ddi_engine::{AtOnce, TransformOptions, transform};

fn lift(t: i64, lane: u8) -> Note {
    let mut n = Note::new(Tick(t), lane, NoteKind::Lift);
    n.keysound = Some(3);
    n.speed_mul = Some(1.25);
    n
}
fn cut(src: &[Note], id: &str, limit: AtOnce, no_stretch: bool) -> Vec<Note> {
    transform::apply(
        src,
        &Layout::builtin(id).unwrap(),
        &TransformOptions {
            at_once: limit,
            no_stretch,
            ..Default::default()
        },
        0,
    )
    .notes
}
const LAYOUTS: &[&str] = &[
    "dance-single",
    "pump-single",
    "danoni-5",
    "dance-double",
    "pump-double",
    "pump-halfdouble",
    "danoni-9A",
];

#[test]
fn two_trims_leftmost_excess_lifts_without_changing_retained_notes() {
    let src = vec![lift(192, 1), lift(192, 2), lift(192, 3)];
    for id in LAYOUTS {
        for no_stretch in [false, true] {
            let dst = cut(&src, id, AtOnce::Two, no_stretch);
            assert_eq!(dst, src[1..], "{id} no_stretch={no_stretch}");
            assert_eq!(cut(&dst, id, AtOnce::Two, no_stretch), dst);
            assert_eq!(cut(&src, id, AtOnce::Any, no_stretch), src);
            // One keeps its existing lift protection.
            assert_eq!(cut(&src, id, AtOnce::One, no_stretch), src);
        }
    }
}

#[test]
fn taps_and_new_heads_are_removed_before_excess_lifts() {
    for kind in [
        NoteKind::Tap,
        NoteKind::HoldHead { end: Tick(96) },
        NoteKind::RollHead { end: Tick(96) },
    ] {
        let src = vec![
            lift(0, 0),
            lift(0, 1),
            Note::new(Tick(0), 2, kind),
            lift(0, 3),
            Note::new(Tick(48), 0, NoteKind::Tap),
        ];
        let expected = vec![src[1].clone(), src[3].clone(), src[4].clone()];
        for id in LAYOUTS {
            for no_stretch in [false, true] {
                assert_eq!(cut(&src, id, AtOnce::Two, no_stretch), expected, "{id}");
            }
        }
    }
}

#[test]
fn retained_holds_and_rolls_count_through_the_tail_and_hazards_survive() {
    for kind in [
        NoteKind::HoldHead { end: Tick(96) },
        NoteKind::RollHead { end: Tick(96) },
    ] {
        let mut src = vec![Note::new(Tick(0), 0, kind)];
        for t in [48, 96, 144] {
            src.extend((1..=3).map(|lane| lift(t, lane)));
        }
        for kind in [
            NoteKind::Mine,
            NoteKind::Shock,
            NoteKind::Fake,
            NoteKind::Dummy,
            NoteKind::AutoKeysound,
        ] {
            src.push(Note::new(Tick(24), 1, kind));
        }
        src.sort_by_key(|n| (n.tick, n.lane));
        let expected: Vec<_> = src
            .iter()
            .filter(|n| {
                n.kind != NoteKind::Lift
                    || if n.tick.0 <= 96 {
                        n.lane == 3
                    } else {
                        n.lane >= 2
                    }
            })
            .cloned()
            .collect();
        for id in LAYOUTS {
            for no_stretch in [false, true] {
                assert_eq!(cut(&src, id, AtOnce::Two, no_stretch), expected, "{id}");
            }
        }
    }
}
