use super::*;

const PHONE: [f32; 2] = [390.0, 844.0];
const PHONE_SIDE: [f32; 2] = [844.0, 390.0];
const SMALL: [f32; 2] = [360.0, 640.0];
const TABLET: [f32; 2] = [820.0, 1180.0];
const TABLET_SIDE: [f32; 2] = [1180.0, 820.0];
const SCREENS: [[f32; 2]; 5] = [PHONE, PHONE_SIDE, SMALL, TABLET, TABLET_SIDE];
/// An iPhone's home indicator and notch, upright and on its side.
const NOTCH: [f32; 4] = [47.0, 0.0, 34.0, 0.0];
const NOTCH_SIDE: [f32; 4] = [0.0, 47.0, 21.0, 47.0];

fn layout(id: &str) -> Layout {
    Layout::builtin(id).unwrap_or_else(|| panic!("{id}"))
}

fn played() -> Vec<Layout> {
    Layout::builtin_ids().iter().map(|id| layout(id)).collect()
}

fn single(size: [f32; 2], style: Style) -> Controls {
    plan(
        &layout("dance-single"),
        Screen::new(size, [0.0; 4]),
        Choice {
            style,
            hand: Hand::Right,
        },
    )
    .expect("controls")
}

fn lane_of(c: &Controls, lane: usize) -> &Target {
    c.targets
        .iter()
        .find(|t| t.lane == Some(lane))
        .expect("a target")
}

#[test]
fn single_is_two_buttons_a_side_in_the_receptors_order() {
    let c = single(PHONE, Style::Buttons);
    assert_eq!(c.style, Style::Buttons);
    let lanes: Vec<_> = c.targets.iter().map(|t| t.lane).collect();
    assert_eq!(lanes, [Some(0), Some(1), Some(2), Some(3)]);
    let groups: Vec<_> = c
        .targets
        .iter()
        .map(|t| match t.kind {
            Kind::Button { group } => group,
            _ => 9,
        })
        .collect();
    assert_eq!(groups, [0, 0, 1, 1]);
    // Left to right, at the bottom, each wider than a thumb's 11 mm.
    for pair in c.targets.windows(2) {
        assert!(pair[0].rect[2] < pair[1].rect[0]);
    }
    for t in &c.targets {
        assert!((t.rect[3] - (844.0 - MARGIN)).abs() < 0.01);
        assert!(t.size()[0] * MM_PER_PX_PHONE > 11.0, "{:?}", t.size());
    }
    // The two groups apart by the middle at least.
    assert!(c.targets[2].rect[0] - c.targets[1].rect[2] >= MIDDLE);
}

#[test]
fn the_safe_area_is_kept_clear() {
    for (size, insets) in [(PHONE, NOTCH), (PHONE_SIDE, NOTCH_SIDE)] {
        for l in played() {
            for style in [Style::Buttons, Style::Pad] {
                let c = plan(
                    &l,
                    Screen::new(size, insets),
                    Choice {
                        style,
                        hand: Hand::Right,
                    },
                )
                .expect("controls");
                for t in &c.targets {
                    let [left, top, right, bottom] = t.rect;
                    assert!(left >= insets[3] + MARGIN - 0.01, "{} {size:?}", l.id);
                    assert!(right <= size[0] - insets[1] - MARGIN + 0.01, "{}", l.id);
                    assert!(bottom <= size[1] - insets[2] - MARGIN + 0.01, "{}", l.id);
                    assert!(top >= insets[0], "{}", l.id);
                }
            }
        }
    }
}

#[test]
fn an_odd_layout_has_its_extra_button_on_the_chosen_side() {
    let l = layout("pump-single");
    let count = |hand| {
        let c = plan(
            &l,
            Screen::new(PHONE, [0.0; 4]),
            Choice {
                style: Style::Buttons,
                hand,
            },
        )
        .unwrap();
        let left = c
            .targets
            .iter()
            .filter(|t| t.kind == Kind::Button { group: 0 })
            .count();
        (left, c.targets.len() - left)
    };
    assert_eq!(count(Hand::Right), (2, 3));
    assert_eq!(count(Hand::Left), (3, 2));
    // The middle (pump's center, lane 2) is the right group's first.
    let c = plan(&l, Screen::new(PHONE, [0.0; 4]), Choice::default()).unwrap();
    assert_eq!(c.targets[2].lane, Some(2));
    assert_eq!(c.targets[2].kind, Kind::Button { group: 1 });
}

#[test]
fn many_lanes_play_the_pad_and_columns_have_no_plan() {
    for id in [
        "dance-double",
        "pump-double",
        "danoni-9A",
        "danoni-9B",
        "techno-single8",
    ] {
        let l = layout(id);
        assert_eq!(effective(Style::Buttons, &l), Style::Pad, "{id}");
    }
    for id in [
        "dance-single",
        "dance-solo",
        "pump-single",
        "pump-halfdouble",
        "danoni-5",
        "danoni-7",
    ] {
        let l = layout(id);
        assert_eq!(effective(Style::Buttons, &l), Style::Buttons, "{id}");
        assert_eq!(effective(Style::Pad, &l), Style::Pad, "{id}");
    }
    assert_eq!(
        plan(
            &layout("dance-single"),
            Screen::new(PHONE, [0.0; 4]),
            Choice {
                style: Style::Columns,
                hand: Hand::Right
            }
        ),
        None
    );
}

#[test]
fn every_played_layout_has_every_lane_on_every_screen() {
    for l in played() {
        for size in SCREENS {
            for style in [Style::Buttons, Style::Pad] {
                for hand in [Hand::Right, Hand::Left] {
                    let c = plan(&l, Screen::new(size, [0.0; 4]), Choice { style, hand })
                        .unwrap_or_else(|| panic!("{} {size:?}", l.id));
                    for lane in 0..l.lanes.len() {
                        assert!(
                            c.targets.iter().any(|t| t.lane == Some(lane)),
                            "{} {size:?} {lane}",
                            l.id
                        );
                    }
                    // No two targets overlap.
                    for (i, a) in c.targets.iter().enumerate() {
                        for b in &c.targets[i + 1..] {
                            let apart = a.rect[2] <= b.rect[0]
                                || b.rect[2] <= a.rect[0]
                                || a.rect[3] <= b.rect[1]
                                || b.rect[3] <= a.rect[1];
                            assert!(apart, "{} {size:?} {a:?} {b:?}", l.id);
                        }
                        assert!(a.size()[0] > 0.0 && a.size()[1] > 0.0);
                    }
                }
            }
        }
    }
}

#[test]
fn the_pad_puts_each_lane_on_the_mats_tile() {
    let c = single(PHONE, Style::Pad);
    assert_eq!(c.style, Style::Pad);
    assert_eq!(c.targets.len(), 9);
    let cell = |lane| match lane_of(&c, lane).kind {
        Kind::Tile { cell, .. } => cell,
        _ => 0,
    };
    // Left 4, down 2, up 8, right 6.
    assert_eq!([cell(0), cell(1), cell(2), cell(3)], [4, 2, 8, 6]);
    // Up above down, left left of right.
    assert!(lane_of(&c, 2).rect[3] < lane_of(&c, 1).rect[1]);
    assert!(lane_of(&c, 0).rect[2] < lane_of(&c, 3).rect[0]);
    assert_eq!(c.targets.iter().filter(|t| t.lane.is_none()).count(), 5);
}

#[test]
fn a_lone_pad_sits_beside_the_field_on_a_wide_screen() {
    let l = layout("dance-single");
    let field = [366.0, 478.0];
    let at = |hand| {
        plan(
            &l,
            Screen::new(PHONE_SIDE, [0.0; 4]).with_field(field),
            Choice {
                style: Style::Pad,
                hand,
            },
        )
        .unwrap()
        .groups[0]
    };
    let right = at(Hand::Right);
    assert!(right[0] > field[1], "{right:?}");
    let left = at(Hand::Left);
    assert!(left[2] < field[0], "{left:?}");
    // Upright: under the field, centered.
    let c = single(PHONE, Style::Pad);
    let g = c.groups[0];
    assert!(((g[0] + g[2]) / 2.0 - 195.0).abs() < 0.01);
}

#[test]
fn doubles_have_two_pads_and_nine_a_two_onigiri() {
    for size in SCREENS {
        let c = plan(
            &layout("dance-double"),
            Screen::new(size, [0.0; 4]),
            Choice::default(),
        )
        .unwrap();
        assert_eq!(c.groups.len(), 2, "{size:?}");
        assert!(c.groups[0][2] < c.groups[1][0], "{size:?}");
        // The left pad's left is lane 0, the right pad's right lane 7.
        assert_eq!(lane_of(&c, 0).kind, Kind::Tile { pad: 0, cell: 4 });
        assert_eq!(lane_of(&c, 7).kind, Kind::Tile { pad: 1, cell: 6 });
    }
    let c = plan(
        &layout("danoni-9A"),
        Screen::new(PHONE_SIDE, [0.0; 4]),
        Choice::default(),
    )
    .unwrap();
    let oni: Vec<_> = c
        .targets
        .iter()
        .filter(|t| t.lane == Some(4))
        .map(|t| t.kind)
        .collect();
    assert_eq!(
        oni,
        [Kind::Tile { pad: 0, cell: 5 }, Kind::Tile { pad: 1, cell: 5 }]
    );
}

#[test]
fn a_finger_going_down_presses_the_nearest_lane_near_the_controls_only() {
    let c = single(PHONE, Style::Buttons);
    for t in &c.targets {
        assert_eq!(c.lane_at(t.center(), None), t.lane);
    }
    // In the gap between the left group's two: the nearer.
    let a = c.targets[0].rect;
    let b = c.targets[1].rect;
    let y = (a[1] + a[3]) / 2.0;
    assert_eq!(c.lane_at([a[2] + 1.0, y], None), Some(0));
    assert_eq!(c.lane_at([b[0] - 1.0, y], None), Some(1));
    // A thumb a little low, or wide of the left edge.
    assert_eq!(c.lane_at([a[0] + 5.0, a[3] + 8.0], None), Some(0));
    assert_eq!(c.lane_at([a[0] - 9.0, y], None), Some(0));
    // Far above the controls, or between the groups: nothing.
    assert_eq!(c.lane_at([100.0, 200.0], None), None);
    let mid = (c.targets[1].rect[2] + c.targets[2].rect[0]) / 2.0;
    assert_eq!(c.lane_at([mid, y], None), None);
    // A plain tile of the pad: nothing.
    let p = single(PHONE, Style::Pad);
    let corner = p.targets.iter().find(|t| t.lane.is_none()).unwrap();
    assert_eq!(p.lane_at(corner.center(), None), None);
}

#[test]
fn a_slide_changes_lane_only_well_inside_the_next() {
    let c = single(PHONE, Style::Buttons);
    let a = c.targets[0].rect;
    let b = c.targets[1].rect;
    let y = (a[1] + a[3]) / 2.0;
    // A path from the middle of the first to the middle of the second:
    // the lane changes once, past the dead band.
    let mut held = c.lane_at([(a[0] + a[2]) / 2.0, y], None);
    let mut changes = Vec::new();
    let from = (a[0] + a[2]) / 2.0;
    let to = (b[0] + b[2]) / 2.0;
    for i in 0..=100 {
        let x = from + (to - from) * i as f32 / 100.0;
        let now = c.lane_at([x, y], held);
        if now != held {
            changes.push((x, now));
        }
        held = now;
    }
    assert_eq!(changes.len(), 1, "{changes:?}");
    assert!(changes[0].0 >= b[0] + DEAD_BAND);
    assert_eq!(changes[0].1, Some(1));
    // Back again: the same dead band the other way, no flutter.
    let mut back = Vec::new();
    for i in 0..=100 {
        let x = to + (from - to) * i as f32 / 100.0;
        let now = c.lane_at([x, y], held);
        if now != held {
            back.push((x, now));
        }
        held = now;
    }
    assert_eq!(back.len(), 1, "{back:?}");
    assert!(back[0].0 <= a[2] - DEAD_BAND);
    // Jitter on the boundary changes nothing.
    let edge = b[0] + DEAD_BAND / 2.0;
    for i in 0..20 {
        let x = edge + if i % 2 == 0 { 3.0 } else { -3.0 };
        assert_eq!(c.lane_at([x, y], Some(0)), Some(0));
    }
}

#[test]
fn a_slide_off_the_controls_keeps_its_lane() {
    let c = single(PHONE, Style::Buttons);
    // Up the screen, off its edge, between the groups.
    for at in [[40.0, 300.0], [-30.0, 820.0], [195.0, 780.0], [200.0, 900.0]] {
        assert_eq!(c.lane_at(at, Some(1)), Some(1), "{at:?}");
    }
    // Across the middle to the other group: well inside, it changes.
    let r = c.targets[2].center();
    assert_eq!(c.lane_at(r, Some(1)), Some(2));
}

#[test]
fn a_slide_over_the_pads_plain_tiles_keeps_its_lane() {
    let p = single(PHONE, Style::Pad);
    let tile = |cell| {
        p.targets
            .iter()
            .find(|t| t.kind == Kind::Tile { pad: 0, cell })
            .unwrap()
    };
    // Left (4) up through the corner (7) to up (8).
    let mut held = p.lane_at(tile(4).center(), None);
    assert_eq!(held, Some(0));
    held = p.lane_at(tile(7).center(), held);
    assert_eq!(held, Some(0));
    held = p.lane_at(tile(8).center(), held);
    assert_eq!(held, Some(2));
    // Through the middle (5) to right (6).
    held = p.lane_at(tile(5).center(), held);
    assert_eq!(held, Some(2));
    held = p.lane_at(tile(6).center(), held);
    assert_eq!(held, Some(3));
    // Two fingers on one tile each press it (the shells count them).
    assert_eq!(p.lane_at(tile(6).center(), None), Some(3));
    let near = [tile(6).rect[0] + 2.0, tile(6).rect[1] + 2.0];
    assert_eq!(p.lane_at(near, None), Some(3));
}

#[test]
fn sizes_on_the_screens() {
    // The smallest target of each layout and screen, in millimeters on
    // a phone: Buttons' thumbs want 11 mm and more.
    let mm = |id: &str, size: [f32; 2], style| {
        plan(&layout(id), Screen::new(size, [0.0; 4]), Choice { style, hand: Hand::Right })
            .unwrap()
            .smallest()
            * MM_PER_PX_PHONE
    };
    assert!(mm("dance-single", PHONE, Style::Buttons) > 13.0);
    assert!(mm("dance-single", SMALL, Style::Buttons) > 12.0);
    assert!(mm("dance-single", PHONE_SIDE, Style::Buttons) > 15.0);
    assert!(mm("pump-single", PHONE, Style::Buttons) > 8.5);
    assert!(mm("dance-single", PHONE, Style::Pad) > 15.0);
    assert!(mm("dance-double", PHONE, Style::Pad) > 7.5);
}

/// The sizes' table for the plan (`docs/plans/touch-controls.md`):
/// `cargo test -p ddi-ui --lib touch_controls::tests::sizes_table --
/// --ignored --nocapture`.
#[test]
#[ignore]
fn sizes_table() {
    let screens = [
        ("phone 390×844", PHONE, MM_PER_PX_PHONE),
        ("phone 844×390", PHONE_SIDE, MM_PER_PX_PHONE),
        ("small 360×640", SMALL, MM_PER_PX_PHONE),
        ("tablet 820×1180", TABLET, MM_PER_PX_TABLET),
        ("tablet 1180×820", TABLET_SIDE, MM_PER_PX_TABLET),
    ];
    println!("| layout | style | screen | smallest target px | mm | covers from y | rows of notes clear above |");
    println!("|---|---|---|---|---|---|---|");
    for l in played() {
        for style in [Style::Buttons, Style::Pad] {
            if style == Style::Pad && effective(Style::Buttons, &l) == Style::Pad {
                continue;
            }
            for (name, size, mm) in screens {
                let c = plan(&l, Screen::new(size, [0.0; 4]), Choice { style, hand: Hand::Right })
                    .unwrap();
                let span = field_span(&l);
                let arrow = (size[1] / 10.0).min(size[0] / (span + 2.0)).clamp(24.0, 160.0);
                let receptor = arrow * 1.5;
                let fw = arrow * span;
                let fl = (size[0] - fw) / 2.0;
                // The controls over the field's columns: from their top
                // the notes pass under them.
                let over = c
                    .targets
                    .iter()
                    .filter(|t| t.rect[2] > fl && t.rect[0] < fl + fw)
                    .map(|t| t.rect[1])
                    .fold(size[1], f32::min);
                let rows = (over - receptor) / arrow;
                let all = (size[1] - receptor) / arrow;
                println!(
                    "| {} | {:?} | {name} | {:.0} | {:.1} | {:.0} | {:.1} of {:.1} |",
                    l.id,
                    c.style,
                    c.smallest(),
                    c.smallest() * mm,
                    c.top(),
                    rows,
                    all,
                );
            }
        }
    }
}
