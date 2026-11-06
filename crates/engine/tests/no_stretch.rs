// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use ddi_chart::{Layout, Note, NoteKind, Tick};
use ddi_engine::{AtOnce, TransformOptions, Turn, transform};

fn tap(tick: i64, lane: u8) -> Note {
    Note::new(Tick(tick), lane, NoteKind::Tap)
}
fn long(tick: i64, lane: u8, end: i64, roll: bool) -> Note {
    Note::new(
        Tick(tick),
        lane,
        if roll {
            NoteKind::RollHead { end: Tick(end) }
        } else {
            NoteKind::HoldHead { end: Tick(end) }
        },
    )
}
fn opts() -> TransformOptions {
    TransformOptions {
        no_stretch: true,
        ..Default::default()
    }
}
fn simplified(id: &str, notes: &[Note]) -> Vec<Note> {
    transform::apply(notes, &Layout::builtin(id).unwrap(), &opts(), 0).notes
}

#[test]
fn double_pair_coverage_uses_layout_regions_not_lane_index_distance() {
    for (id, count) in [
        ("dance-double", 17),
        ("pump-double", 29),
        ("pump-halfdouble", 15),
        ("danoni-9A", 25),
    ] {
        let layout = Layout::builtin(id).unwrap();
        let mut kept = 0;
        for a in 0..layout.lane_count() as u8 {
            for b in a + 1..layout.lane_count() as u8 {
                let src = [tap(0, a), tap(0, b)];
                let dst = simplified(id, &src);
                assert!(dst.iter().all(|n| src.contains(n)));
                kept += usize::from(dst.len() == 2);
                if dst.len() == 1 {
                    assert_eq!(dst, [tap(0, b)]);
                }
            }
        }
        assert_eq!(kept, count, "{id}");
    }
    // Same pad, inner seam, or an inner panel paired with the other pad's U/D.
    for (a, b) in [(0, 3), (1, 2), (3, 4), (2, 4), (3, 6)] {
        assert_eq!(simplified("dance-double", &[tap(0, a), tap(0, b)]).len(), 2);
    }
    // Both UP panels, or the two outermost arrows, stretch too far.
    for (a, b) in [(2, 6), (0, 7)] {
        assert_eq!(
            simplified("dance-double", &[tap(0, a), tap(0, b)]),
            [tap(0, b)]
        );
    }
    // The two Pump centers are allowed, unlike its outer corners.
    assert_eq!(simplified("pump-double", &[tap(0, 2), tap(0, 7)]).len(), 2);
    assert_eq!(
        simplified("pump-double", &[tap(0, 0), tap(0, 9)]),
        [tap(0, 9)]
    );
}
#[test]
fn oni_can_share_a_region_with_either_bank_but_banks_stay_distinct() {
    for arrow in [0, 1, 2, 3, 5, 6, 7, 8] {
        let mut source = vec![tap(0, arrow), tap(0, 4)];
        source.sort_by_key(|n| n.lane);
        assert_eq!(simplified("danoni-9A", &source), source);
    }
    assert_eq!(
        simplified("danoni-9A", &[tap(0, 0), tap(0, 4), tap(0, 8)]),
        [tap(0, 4), tap(0, 8)]
    );
    assert_eq!(
        simplified("danoni-9A", &[tap(0, 0), tap(0, 8)]),
        [tap(0, 8)]
    );
    // A held Oni remains logical; taps in either bank can be retained.
    let src = [long(0, 4, 192, false), tap(48, 0), tap(96, 8)];
    assert_eq!(simplified("danoni-9A", &src), src);
}
#[test]
fn previous_holds_and_rolls_survive_and_constrain_incoming_rows_including_the_tail() {
    for roll in [false, true] {
        let h = long(0, 0, 96, roll);
        let src = vec![
            h.clone(),
            Note::new(Tick(12), 0, NoteKind::Fake),
            Note::new(Tick(24), 0, NoteKind::Mine),
            tap(48, 7),
            Note::new(Tick(72), 7, NoteKind::Lift),
            tap(96, 7),
            tap(97, 7),
        ];
        let dst = simplified("dance-double", &src);
        assert_eq!(dst, [h, src[1].clone(), src[2].clone(), tap(97, 7)]);
        assert_eq!(src.len(), 7);
    }
}
#[test]
fn discarded_holds_do_not_delete_later_notes_under_the_joint_count_and_span_filter() {
    let src = [
        long(0, 2, 192, false),
        long(0, 7, 48, true),
        tap(96, 0),
        tap(96, 1),
    ];
    let dst = transform::apply(
        &src,
        &Layout::dance_double(),
        &TransformOptions {
            at_once: AtOnce::Two,
            ..opts()
        },
        0,
    )
    .notes;
    assert_eq!(dst, [src[1].clone(), src[2].clone(), src[3].clone()]);
    assert_eq!(
        transform::apply(
            &dst,
            &Layout::dance_double(),
            &TransformOptions {
                at_once: AtOnce::Two,
                ..opts()
            },
            0
        )
        .notes,
        dst
    );
}
#[test]
fn no_stretch_is_separate_from_at_once_and_no_holds_and_ignores_hazards() {
    let triple = [tap(0, 0), tap(0, 1), tap(0, 2)];
    assert_eq!(simplified("dance-double", &triple), triple);
    for (limit, count) in [(AtOnce::Two, 2), (AtOnce::One, 1)] {
        assert_eq!(
            transform::apply(
                &triple,
                &Layout::dance_double(),
                &TransformOptions {
                    at_once: limit,
                    ..opts()
                },
                0
            )
            .notes
            .len(),
            count
        );
    }
    let src = [long(0, 0, 192, false), tap(48, 7)];
    assert_eq!(
        transform::apply(
            &src,
            &Layout::dance_double(),
            &TransformOptions {
                no_holds: true,
                ..opts()
            },
            0
        )
        .notes,
        [tap(0, 0), tap(48, 7)]
    );
    let src = [
        tap(0, 0),
        Note::new(Tick(0), 7, NoteKind::Mine),
        Note::new(Tick(0), 6, NoteKind::Shock),
        Note::new(Tick(0), 5, NoteKind::Fake),
        Note::new(Tick(0), 4, NoteKind::Dummy),
        Note::new(Tick(0), 3, NoteKind::AutoKeysound),
    ];
    let mut expected = src.to_vec();
    expected.sort_by_key(|n| (n.tick, n.lane));
    assert_eq!(simplified("dance-double", &src), expected);
    assert_eq!(
        simplified(
            "dance-double",
            &[
                Note::new(Tick(0), 0, NoteKind::Lift),
                Note::new(Tick(0), 7, NoteKind::Lift)
            ]
        ),
        [Note::new(Tick(0), 7, NoteKind::Lift)]
    );
}
#[test]
fn the_final_turned_lanes_are_filtered_and_source_metadata_never_changes() {
    for id in [
        "dance-double",
        "pump-double",
        "pump-halfdouble",
        "danoni-9A",
    ] {
        let l = Layout::builtin(id).unwrap();
        let mut src = Vec::new();
        for a in 0..l.lane_count() as u8 {
            for b in a + 1..l.lane_count() as u8 {
                let t = src.len() as i64 * 48;
                for lane in [a, b] {
                    let mut n = tap(t, lane);
                    n.keysound = Some(3);
                    n.speed_mul = Some(1.25);
                    src.push(n);
                }
            }
        }
        let original = src.clone();
        for turn in [
            Turn::Off,
            Turn::Mirror,
            Turn::Left,
            Turn::Right,
            Turn::Shuffle,
        ] {
            for seed in 0..12 {
                let plain = transform::apply(
                    &src,
                    &l,
                    &TransformOptions {
                        turn,
                        ..Default::default()
                    },
                    seed,
                );
                let done = transform::apply(&src, &l, &TransformOptions { turn, ..opts() }, seed);
                assert_eq!(done.take_from, plain.take_from);
                assert_eq!(
                    done.notes,
                    transform::apply(&plain.notes, &l, &opts(), 0).notes
                );

                assert!(done.notes.iter().all(|n| plain.notes.contains(n)));
                // Already resolved physical lanes need neither a second turn nor further cuts.
                assert_eq!(
                    transform::apply(&done.notes, &l, &opts(), 0).notes,
                    done.notes
                );
            }
        }
        assert_eq!(src, original);
    }
}
#[test]
fn old_settings_default_off_and_the_new_flag_roundtrips_as_an_assist() {
    let legacy: TransformOptions =
        serde_json::from_str(r#"{"at_once":"Two","no_jumps":false}"#).unwrap();
    assert!(!legacy.no_stretch);
    let current: TransformOptions =
        serde_json::from_str(&serde_json::to_string(&opts()).unwrap()).unwrap();
    assert_eq!(current, opts());
    assert!(current.is_assist());
    let source = [tap(0, 0), tap(0, 7)];
    assert_eq!(
        transform::apply(&source, &Layout::dance_double(), &Default::default(), 0).notes,
        source
    );
    // Single mats keep precisely the old At once selection and turn ordering.
    for id in [
        "dance-single",
        "dance-solo",
        "pump-single",
        "techno-single8",
        "danoni-5",
        "danoni-7",
        "danoni-9B",
    ] {
        let layout = Layout::builtin(id).unwrap();
        let src = (0..layout.lane_count() as u8)
            .map(|i| tap(0, i))
            .collect::<Vec<_>>();
        for turn in [Turn::Off, Turn::Mirror, Turn::Shuffle] {
            let old = TransformOptions {
                at_once: AtOnce::Two,
                turn,
                ..Default::default()
            };
            assert_eq!(
                transform::apply(&src, &layout, &old, 17),
                transform::apply(
                    &src,
                    &layout,
                    &TransformOptions {
                        no_stretch: true,
                        ..old
                    },
                    17
                )
            );
        }
    }
}

#[test]
fn complete_incoming_rows_and_retained_holds_match_whole_chart_preparation() {
    for id in [
        "dance-double",
        "pump-double",
        "pump-halfdouble",
        "danoni-9A",
    ] {
        let l = Layout::builtin(id).unwrap();
        let last = l.lane_count() as u8 - 1;
        let mut src = vec![
            long(0, 0, 144, false),
            tap(48, last),
            tap(96, 1),
            tap(144, last),
            long(192, last, 336, true),
            tap(240, 0),
            tap(288, last - 1),
            tap(336, 0),
            tap(384, 0),
        ];
        src.sort_by_key(|n| (n.tick, n.lane));
        let options = TransformOptions {
            at_once: AtOnce::Two,
            ..opts()
        };
        let expected = transform::apply(&src, &l, &options, 0).notes;
        for chunk in [1, 2, 3, src.len()] {
            let mut carry: Vec<Note> = Vec::new();
            let mut got = Vec::new();
            for rows in src.chunks(chunk) {
                let first = rows.first().unwrap().tick;
                let last = rows.last().unwrap().tick;
                let mut input = carry.clone();
                input.extend_from_slice(rows);
                let prepared = transform::apply(&input, &l, &options, 0).notes;
                got.extend(prepared.iter().filter(|n| n.tick >= first).cloned());
                carry = prepared
                    .into_iter()
                    .filter(|n| n.kind.hold_end().is_some_and(|t| t >= last))
                    .collect();
            }
            assert_eq!(got, expected, "{id}, {chunk}");
        }
    }
}
