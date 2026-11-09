// SPDX-FileCopyrightText: 2026 すごいジャン <me@sugoijan.dev>
// SPDX-License-Identifier: MIT
use ddi_chart::{Layout, Note, NoteKind, Tick};
use ddi_engine::{AtOnce, PlayOptions, Player, TransformOptions, presets, transform};
use ddi_hand_plan::prepare_notes;

#[test]
fn hand_copy_always_caps_and_removes_stretches_without_changing_gameplay() {
    for id in [
        "dance-double",
        "pump-double",
        "pump-halfdouble",
        "danoni-9A",
    ] {
        let mut song = ddi_chart::formats::sm::parse_sm(
            "#BPMS:0=120;#NOTES:dance-single::Easy:1:0,0,0,0,0:1000;",
        )
        .unwrap();
        song.charts[0].layout = id.into();
        let l = Layout::builtin(id).unwrap();
        song.charts[0].notes = vec![
            Note::new(Tick(0), 0, NoteKind::Tap),
            Note::new(Tick(0), l.lane_count() as u8 - 1, NoteKind::Tap),
        ];
        let source = song.charts[0].notes.clone();
        let p = Player::new(&song, 0, presets::itg(), PlayOptions::default());
        let judged = p
            .judge()
            .notes()
            .iter()
            .map(|n| n.note.clone())
            .collect::<Vec<_>>();
        assert_eq!(judged, source);
        let hand = prepare_notes(&judged, &l);
        assert_eq!(hand.len(), if id == "pump-halfdouble" { 2 } else { 1 });
        assert_eq!(hand, prepare_notes(&hand, &l));
        assert_eq!(p.options().transform, TransformOptions::default());
        assert_eq!(p.judge().notes().len(), source.len());
        assert_eq!(song.charts[0].notes, source);
        assert!(ddi_hand_plan::layout_panels(id).is_ok());
    }
    for l in Layout::builtins().into_iter().filter(|l| {
        ddi_hand_plan::layout_panels(&l.id).is_ok()
            && ddi_hand_plan::layout_geometry(&l.id).unwrap().pads() == 1
    }) {
        let src = (0..l.lane_count() as u8)
            .map(|i| Note::new(Tick(0), i, NoteKind::Tap))
            .collect::<Vec<_>>();
        let old = transform::apply(
            &src,
            &l,
            &TransformOptions {
                at_once: AtOnce::Two,
                ..Default::default()
            },
            0,
        )
        .notes;
        assert_eq!(prepare_notes(&src, &l), old);
    }
}

#[test]
fn simultaneous_lifts_use_the_same_gameplay_and_hand_copy() {
    for id in [
        "pump-single",
        "danoni-5",
        "dance-double",
        "pump-double",
        "pump-halfdouble",
        "danoni-9A",
    ] {
        let mut song = ddi_chart::formats::sm::parse_sm(
            "#BPMS:0=120;#NOTES:dance-single::Easy:1:0,0,0,0,0:1000;",
        )
        .unwrap();
        song.charts[0].layout = id.into();
        song.charts[0].notes = (1..=3)
            .map(|lane| Note::new(Tick(48), lane, NoteKind::Lift))
            .collect();
        let src = song.charts[0].notes.clone();
        let l = Layout::builtin(id).unwrap();
        for no_stretch in [false, true] {
            let p = Player::new(
                &song,
                0,
                presets::itg(),
                PlayOptions {
                    transform: TransformOptions {
                        at_once: AtOnce::Two,
                        no_stretch,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            let judged: Vec<_> = p.judge().notes().iter().map(|n| n.note.clone()).collect();
            assert_eq!(judged, src[1..]);
            assert_eq!(prepare_notes(&src, &l), judged);
            assert_eq!(prepare_notes(&judged, &l), judged);
            assert_eq!(song.charts[0].notes, src);
        }
    }
}
