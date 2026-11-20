use super::*;
use crate::eventsub::{ChatMessage, Fragment};

const LINE: f32 = 30.0;

fn options(style: Style, reduced: bool) -> Options {
    Options {
        level: Level::Messages,
        style,
        names: false,
        reduced,
    }
}

fn overlay(style: Style, reduced: bool) -> Overlay {
    Overlay::new(CHAT_OVERLAY_V0, options(style, reduced), EmoteSets::default())
}

fn area() -> Area {
    Area::new([1280.0, 720.0], LINE, [500.0, 780.0], &[], Level::Messages)
}

fn say(id: u32, chatter: u32, text: &str) -> Event {
    Event::Message(ChatMessage {
        id: format!("m{id}"),
        chatter_id: format!("u{chatter}"),
        chatter_name: format!("User{chatter}"),
        text: text.into(),
        fragments: vec![Fragment::Text(text.into())],
        kind: "text".into(),
        ..ChatMessage::default()
    })
}

fn kappa(id: u32, chatter: u32) -> Event {
    Event::Message(ChatMessage {
        id: format!("m{id}"),
        chatter_id: format!("u{chatter}"),
        text: "Kappa".into(),
        fragments: vec![Fragment::Emote {
            text: "Kappa".into(),
            id: "25".into(),
            still: true,
        }],
        kind: "text".into(),
        ..ChatMessage::default()
    })
}

/// Ten pixels a character.
fn measure(t: &str) -> f32 {
    t.chars().count() as f32 * 10.0
}

fn ready(_: &Emote) -> Readiness {
    Readiness::Ready { aspect: 1.0 }
}

fn advance(o: &mut Overlay, now: f64) {
    o.advance(now, &area(), &ready, &mut measure);
}

#[test]
fn at_most_eight_new_items_a_second() {
    let mut o = overlay(Style::Across, false);
    for i in 0..50 {
        o.receive(&say(i, i, "hi"), 0.01 * f64::from(i), true);
    }
    assert_eq!(o.len(), 8);
    assert_eq!(o.stats().rate, 42);
    // A second later there is room again.
    o.receive(&say(100, 100, "hi"), 1.05, true);
    assert_eq!(o.len(), 9);
}

#[test]
fn one_item_a_chatter_every_two_seconds() {
    let mut o = overlay(Style::Across, false);
    o.receive(&say(1, 7, "a"), 0.0, true);
    o.receive(&say(2, 7, "b"), 1.9, true);
    o.receive(&say(3, 7, "c"), 2.0, true);
    assert_eq!(o.len(), 2);
    assert_eq!(o.stats().chatter, 1);
}

#[test]
fn forty_on_screen_at_most_and_full_drops() {
    let mut o = overlay(Style::Falling, false);
    let mut t = 0.0;
    let mut n = 0;
    while t < 20.0 {
        for _ in 0..8 {
            o.receive(&say(n, n, "x"), t, true);
            n += 1;
        }
        advance(&mut o, t);
        assert!(o.len() <= 40, "{} at {t}", o.len());
        t += 1.0;
    }
    assert!(o.stats().full + o.stats().no_room > 0);
}

#[test]
fn nothing_new_while_frames_run_late_but_moderation_applies() {
    let mut o = overlay(Style::Across, false);
    o.receive(&say(1, 1, "kept"), 0.0, true);
    o.receive(&say(2, 2, "shed"), 0.1, false);
    assert_eq!(o.len(), 1);
    assert_eq!(o.stats().shed, 1);
    o.receive(
        &Event::Delete {
            message_id: "m1".into(),
        },
        0.2,
        false,
    );
    assert!(o.is_empty());
}

#[test]
fn deletes_and_clears_take_items_off_at_once() {
    let mut o = overlay(Style::Across, false);
    for i in 0..6 {
        o.receive(&say(i, i % 3, "hello"), f64::from(i) * 0.1, true);
    }
    advance(&mut o, 1.0);
    // (Chatter 0, 1, 2 at 0.0, 0.1, 0.2; the second of each within 2 s dropped.)
    assert_eq!(o.drawn(1.0).count(), 3);
    o.receive(
        &Event::Delete {
            message_id: "m1".into(),
        },
        1.1,
        true,
    );
    assert_eq!(o.drawn(1.1).count(), 2);
    o.receive(
        &Event::ClearUser {
            user_id: "u2".into(),
        },
        1.2,
        true,
    );
    assert_eq!(o.drawn(1.2).count(), 1);
    o.receive(&Event::Clear, 1.3, true);
    assert_eq!(o.drawn(1.3).count(), 0);
    assert_eq!(o.stats().removed, 3);
}

#[test]
fn across_takes_a_row_only_once_its_last_item_cleared_the_edge() {
    let mut o = overlay(Style::Across, false);
    o.receive(&say(1, 1, &"a".repeat(20)), 0.0, true);
    advance(&mut o, 0.0);
    o.receive(&say(2, 2, "short"), 0.1, true);
    advance(&mut o, 0.1);
    let ys: Vec<f32> = o.drawn(0.1).map(|d| d.y).collect();
    // Still on the first row's edge: the second goes a row down.
    assert_eq!(ys.len(), 2);
    assert!(ys[1] > ys[0]);
    // Long after, the first row is free again.
    o.receive(&say(3, 3, "later"), 3.0, true);
    advance(&mut o, 3.0);
    let last = o.drawn(3.0).last().unwrap();
    assert_eq!(last.y, ys[0]);
    // Every item crosses in the same time whatever its length: on screen
    // until its time is up, off the left edge then.
    let d = o.drawn(0.0).next().unwrap();
    assert_eq!(d.x, 1280.0);
    let gone = o.drawn(CHAT_OVERLAY_V0.across - 1e-3).next().unwrap();
    assert!((gone.x + gone.width).abs() < 1.0);
}

#[test]
fn across_never_lets_a_fast_item_catch_a_slow_one() {
    // Short and long items mixed: in any frame, two items of a row never
    // overlap.
    let mut o = overlay(Style::Across, false);
    let mut n = 0;
    for f in 0..1200 {
        let now = f64::from(f) / 60.0;
        if f % 9 == 0 {
            let len = [1, 90, 3, 60, 1, 120, 10][(n % 7) as usize];
            o.receive(&say(n, n, &"b".repeat(len)), now, true);
            n += 1;
        }
        advance(&mut o, now);
        let drawn: Vec<_> = o.drawn(now).collect();
        for a in &drawn {
            for b in &drawn {
                if a.id < b.id && a.y == b.y {
                    let apart = a.x + a.width <= b.x || b.x + b.width <= a.x;
                    assert!(apart, "{} and {} overlap at {now}", a.id, b.id);
                }
            }
        }
    }
    assert!(o.stats().shown > 50);
}

#[test]
fn falling_columns_use_the_side_space_and_go_behind_miniatures_when_full() {
    let free = Area::new([1280.0, 720.0], LINE, [500.0, 780.0], &[], Level::Messages);
    assert!(free.columns.iter().all(|c| c[1] <= 500.0 || c[0] >= 780.0));
    assert!(free.columns.len() >= 2);
    // Miniatures over the right side: columns on the left alone.
    let right = [[800.0, 0.0, 1270.0, 720.0]];
    let a = Area::new([1280.0, 720.0], LINE, [500.0, 780.0], &right, Level::Messages);
    assert!(a.columns.iter().all(|c| c[1] <= 500.0));
    // Miniatures over both sides: columns behind them.
    let both = [[10.0, 0.0, 490.0, 720.0], [790.0, 0.0, 1270.0, 720.0]];
    let a = Area::new([1280.0, 720.0], LINE, [500.0, 780.0], &both, Level::Messages);
    assert!(!a.columns.is_empty());
    assert!(a.columns.iter().all(|c| c[1] <= 500.0 || c[0] >= 780.0));
    // A phone held upright: no side space, columns over the whole width.
    let phone = Area::new([390.0, 844.0], LINE, [20.0, 370.0], &[], Level::Messages);
    assert_eq!(phone.columns.first().map(|c| c[0]), Some(LINE * 0.5));
}

#[test]
fn falling_items_come_down_and_spread_over_the_columns() {
    let mut o = overlay(Style::Falling, false);
    for i in 0..4 {
        o.receive(&say(i, i, "hey"), 0.0, true);
    }
    advance(&mut o, 0.0);
    let xs: Vec<f32> = o.drawn(0.0).map(|d| d.x).collect();
    let mut unique = xs.clone();
    unique.dedup();
    assert_eq!(unique.len(), xs.len(), "{xs:?}");
    let y0 = o.drawn(0.0).next().unwrap().y;
    let y1 = o.drawn(1.0).next().unwrap().y;
    assert!(y1 > y0);
}

#[test]
fn reduced_motion_stands_still_and_fades() {
    let mut o = overlay(Style::Across, true);
    o.receive(&say(1, 1, "still"), 0.0, true);
    advance(&mut o, 0.0);
    let a = o.drawn(0.1).next().unwrap();
    let b = o.drawn(2.0).next().unwrap();
    assert_eq!((a.x, a.y), (b.x, b.y));
    assert!(a.alpha < 1.0 && b.alpha == 1.0);
    let late = o.drawn(CHAT_OVERLAY_V0.still - 0.1).next().unwrap();
    assert!(late.alpha < 1.0);
    assert_eq!(o.drawn(CHAT_OVERLAY_V0.still + 0.1).count(), 0);
    // Falling's cells too.
    let mut o = overlay(Style::Falling, true);
    for i in 0..3 {
        o.receive(&say(i, i, "x"), 0.0, true);
    }
    advance(&mut o, 0.0);
    let at: Vec<(f32, f32)> = o.drawn(1.0).map(|d| (d.x, d.y)).collect();
    let later: Vec<(f32, f32)> = o.drawn(3.0).map(|d| (d.x, d.y)).collect();
    assert_eq!(at, later);
    assert_eq!(at.len(), 3);
}

#[test]
fn an_item_waits_a_second_for_its_emotes_then_goes_without() {
    let mut o = overlay(Style::Across, false);
    let mut m = say(1, 1, "hi ");
    if let Event::Message(c) = &mut m {
        c.fragments.push(Fragment::Emote {
            text: "Kappa".into(),
            id: "25".into(),
            still: true,
        });
    }
    o.receive(&m, 0.0, true);
    let waiting = |_: &Emote| Readiness::Waiting;
    o.advance(0.5, &area(), &waiting, &mut measure);
    assert_eq!(o.drawn(0.5).count(), 0);
    assert_eq!(o.wanted(), [&Emote::twitch("25")]);
    o.advance(1.0, &area(), &waiting, &mut measure);
    let d = o.drawn(1.0).next().unwrap();
    assert_eq!(d.laid.len(), 1);
    // An emote-only item whose emote never came is dropped.
    o.receive(&kappa(2, 2), 1.0, true);
    o.advance(2.0, &area(), &|_: &Emote| Readiness::Failed, &mut measure);
    assert_eq!(o.stats().no_image, 1);
}

#[test]
fn names_lead_and_runs_show_their_count() {
    let mut o = Overlay::new(
        CHAT_OVERLAY_V0,
        Options {
            names: true,
            ..options(Style::Across, false)
        },
        EmoteSets::default(),
    );
    let mut m = say(1, 1, "");
    if let Event::Message(c) = &mut m {
        c.fragments = (0..3)
            .flat_map(|_| {
                [
                    Fragment::Emote {
                        text: "Kappa".into(),
                        id: "25".into(),
                        still: true,
                    },
                    Fragment::Text(" ".into()),
                ]
            })
            .collect();
        c.text = "Kappa Kappa Kappa ".into();
    }
    o.receive(&m, 0.0, true);
    advance(&mut o, 0.0);
    let d = o.drawn(0.0).next().unwrap();
    let what: Vec<&Glyphs> = d.laid.iter().map(|l| &l.what).collect();
    assert_eq!(
        what,
        [
            &Glyphs::Name("User1".into()),
            &Glyphs::Emote(Emote::twitch("25")),
            &Glyphs::Count("×3".into()),
        ]
    );
    assert!(d.laid[1].x > d.laid[0].width);
}

#[test]
fn the_same_events_and_clock_give_the_same_layer() {
    let run = |style, reduced| {
        let mut o = overlay(style, reduced);
        let mut out = Vec::new();
        for f in 0..600 {
            let now = f64::from(f) / 60.0;
            if f % 4 == 0 {
                o.receive(&say(f, f % 13, &"w".repeat((f % 30) as usize + 1)), now, f % 50 != 0);
            }
            advance(&mut o, now);
            out.extend(o.drawn(now).map(|d| (d.id, d.x.to_bits(), d.y.to_bits(), d.alpha.to_bits())));
        }
        out
    };
    for style in [Style::Across, Style::Falling] {
        for reduced in [false, true] {
            assert_eq!(run(style, reduced), run(style, reduced));
        }
    }
}

#[test]
fn off_shows_nothing() {
    let mut o = Overlay::new(
        CHAT_OVERLAY_V0,
        Options {
            level: Level::Off,
            ..options(Style::Across, false)
        },
        EmoteSets::default(),
    );
    o.receive(&say(1, 1, "hi"), 0.0, true);
    advance(&mut o, 0.0);
    assert!(o.is_empty());
}
