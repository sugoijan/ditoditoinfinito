use super::*;
use crate::eventsub::{ChatMessage, Fragment};

const LINE: f32 = 30.0;
const SIZE: [f32; 2] = [1280.0, 720.0];

fn options(style: Style, reduced: bool) -> Options {
    Options {
        level: Level::Messages,
        style,
        names: false,
        reduced,
    }
}

fn rules(look: Look) -> Rules {
    Rules {
        look,
        ..CHAT_OVERLAY_V2
    }
}

/// `chat-overlay-v2` is v1 with its limits tripled (round DE), the rest
/// as it was.
#[test]
fn v2_is_v1_with_its_limits_tripled() {
    let (v1, v2) = (CHAT_OVERLAY_V1, CHAT_OVERLAY_V2);
    assert_eq!(v2.name, crate::RULES_NAME);
    assert_eq!((v1.cut.emotes, v1.on_screen, v1.emotes_on_screen, v1.per_second), (8, 80, 160, 20));
    assert_eq!(
        (v2.cut.emotes, v2.on_screen, v2.emotes_on_screen, v2.per_second),
        (3 * 8, 3 * 80, 3 * 160, 3 * 20)
    );
    let same = Rules {
        name: v1.name,
        cut: v1.cut,
        on_screen: v1.on_screen,
        emotes_on_screen: v1.emotes_on_screen,
        per_second: v1.per_second,
        ..v2
    };
    assert_eq!(same, v1, "nothing else changed");
}

/// In the look that keeps a message's emotes in one item (most tests
/// look at whole messages).
fn overlay(style: Style, reduced: bool) -> Overlay {
    Overlay::new(rules(LOOK_LARGER), options(style, reduced), EmoteSets::default())
}

fn area() -> Area {
    Area::new(SIZE, LINE)
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

fn emote(id: &str) -> Fragment {
    Fragment::Emote {
        text: format!("e{id}"),
        id: id.into(),
        still: true,
        animated: false,
    }
}

/// An emote-only message of `ids`, a space between each.
fn emotes(id: u32, chatter: u32, ids: &[&str]) -> Event {
    let mut fragments = Vec::new();
    for (k, e) in ids.iter().enumerate() {
        if k > 0 {
            fragments.push(Fragment::Text(" ".into()));
        }
        fragments.push(emote(e));
    }
    Event::Message(ChatMessage {
        id: format!("m{id}"),
        chatter_id: format!("u{chatter}"),
        chatter_name: format!("User{chatter}"),
        text: ids.join(" "),
        fragments,
        kind: "text".into(),
        ..ChatMessage::default()
    })
}

fn kappa(id: u32, chatter: u32) -> Event {
    emotes(id, chatter, &["25"])
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

fn emotes_in(d: &Drawn) -> usize {
    d.laid.iter().filter(|l| matches!(l.what, Glyphs::Emote(_))).count()
}

#[test]
fn at_most_sixty_new_messages_a_second() {
    for r in [CHAT_OVERLAY_V2, CHAT_OVERLAY_V1] {
        let mut o = Overlay::new(Rules { look: LOOK_LARGER, ..r }, options(Style::Across, false), EmoteSets::default());
        for i in 0..100 {
            o.receive(&say(i, i, "hi"), 0.005 * f64::from(i), true);
        }
        assert_eq!(o.len(), r.per_second, "{}", r.name);
        assert_eq!(o.stats().rate, 100 - r.per_second as u64);
        // A second later there is room again.
        o.receive(&say(1000, 1000, "hi"), 1.05, true);
        assert_eq!(o.len(), r.per_second + 1);
    }
}

#[test]
fn one_message_a_chatter_every_two_seconds() {
    let mut o = overlay(Style::Across, false);
    o.receive(&say(1, 7, "a"), 0.0, true);
    o.receive(&say(2, 7, "b"), 1.9, true);
    o.receive(&say(3, 7, "c"), 2.0, true);
    assert_eq!(o.len(), 2);
    assert_eq!(o.stats().chatter, 1);
}

/// The screen holds 240 messages and 480 emotes at most (v1: 80 and
/// 160); past them a message is dropped whole (never queued, never
/// summarized).
#[test]
fn the_messages_and_the_emotes_on_screen_are_capped() {
    for r in [CHAT_OVERLAY_V2, CHAT_OVERLAY_V1] {
        for style in [Style::Across, Style::Falling] {
            let mut o = Overlay::new(Rules { look: LOOK_LARGER, ..r }, options(style, false), EmoteSets::default());
            let mut t = 0.0;
            let mut n = 0;
            let mut most = 0;
            while t < 20.0 {
                for _ in 0..r.per_second {
                    o.receive(&say(n, n, "x"), t, true);
                    n += 1;
                }
                advance(&mut o, t);
                assert!(o.messages() <= r.on_screen, "{} at {t}", o.messages());
                most = most.max(o.messages());
                t += 1.0;
            }
            assert!(o.stats().full + o.stats().no_room > 0);
            assert!(most > r.on_screen / 2, "{} {style:?}: {most}", r.name);
        }
        // Emotes: a message's own at most, the screen's at most.
        let mut o = Overlay::new(Rules { look: LOOK_LARGER, ..r }, options(Style::Across, false), EmoteSets::default());
        let ids: Vec<String> = (0..r.cut.emotes).map(|i| (i % 9).to_string()).collect();
        let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
        let each = r.emotes_on_screen / r.cut.emotes;
        for i in 0..(each as u32 + 2) {
            o.receive(&emotes(i, i, &ids), 0.0, true);
        }
        assert_eq!(o.emotes(), r.emotes_on_screen, "{}", r.name);
        assert!(o.stats().full >= 2, "{:?}", o.stats());
    }
}

/// A message of fifty emotes brings 24 (v1: eight): the rest left out,
/// said in the statistics, never a count on screen. Scattered (v2's own
/// look) each of the 24 is an item of its own; in a look that keeps a
/// message's emotes in one row the row is also cut at the screen's width
/// (21 of them at 1280 pixels with lines of 30); scattered, an emote
/// that finds no free place on its way in is left out as any item is.
#[test]
fn a_message_of_fifty_emotes_shows_twenty_four() {
    let ids: Vec<String> = (0..50).map(|i| (i % 3).to_string()).collect();
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    for r in [CHAT_OVERLAY_V2, CHAT_OVERLAY_V1] {
        let mut o = Overlay::new(Rules { look: LOOK_LARGER, ..r }, options(Style::Across, false), EmoteSets::default());
        o.receive(&emotes(1, 1, &ids), 0.0, true);
        advance(&mut o, 0.0);
        let d = o.drawn(0.0).next().unwrap();
        // (This item's size, which varies by up to 15% from item to item,
        // fits 21 across.)
        assert_eq!(emotes_in(&d), r.cut.emotes.min(21), "{}", r.name);
        assert_eq!(d.laid.len(), emotes_in(&d), "the emotes and nothing else");
        assert_eq!(o.stats().cut, 50 - r.cut.emotes as u64);
        // Scattered: each emote an item.
        let mut o = Overlay::new(r, options(Style::Across, false), EmoteSets::default());
        o.receive(&emotes(1, 1, &ids), 0.0, true);
        let mut items = std::collections::HashSet::new();
        for f in 0..300 {
            let now = f64::from(f) / 60.0;
            advance(&mut o, now);
            items.extend(o.drawn(now).map(|d| d.id));
        }
        // (One that finds no free place on its way in is left out.)
        assert!(items.len() <= r.cut.emotes && items.len() > r.cut.emotes * 3 / 4, "{}: {}", r.name, items.len());
    }
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

/// Across: every item crosses the whole screen in its own time whatever
/// its length, from the right edge to past the left one.
#[test]
fn across_crosses_the_whole_screen() {
    let mut o = Overlay::new(rules(LOOK_AS_BUILT), options(Style::Across, false), EmoteSets::default());
    o.receive(&say(1, 1, &"a".repeat(20)), 0.0, true);
    advance(&mut o, 0.0);
    let d = o.drawn(0.0).next().unwrap();
    assert_eq!(d.x, SIZE[0]);
    let gone = o.drawn(CHAT_OVERLAY_V1.across - 1e-3).next().unwrap();
    assert!((gone.x + gone.width).abs() < 1.0, "{}", gone.x);
}

/// In any frame, two moving items never overlap, whatever their lengths,
/// sizes and speeds (the looks that vary them too), in both styles.
#[test]
fn moving_items_never_overlap() {
    for look in LOOKS {
        for style in [Style::Across, Style::Falling] {
            let mut o = Overlay::new(rules(look), options(style, false), EmoteSets::default());
            let mut n = 0;
            for f in 0..1200 {
                let now = f64::from(f) / 60.0;
                if f % 5 == 0 {
                    let e = if n % 3 == 0 {
                        emotes(n, n, &["1", "2", "2", "3"][..(n % 4 + 1) as usize])
                    } else {
                        let len = [1, 90, 3, 60, 1, 120, 10][(n % 7) as usize];
                        say(n, n, &"b".repeat(len))
                    };
                    o.receive(&e, now, true);
                    n += 1;
                }
                advance(&mut o, now);
                let drawn: Vec<_> = o.drawn(now).collect();
                for a in &drawn {
                    for b in &drawn {
                        if a.id < b.id {
                            let (aw, ah) = (a.width * a.scale, a.height * a.scale);
                            let (bw, bh) = (b.width * b.scale, b.height * b.scale);
                            let apart = a.x + aw <= b.x + 0.01
                                || b.x + bw <= a.x + 0.01
                                || a.y + ah <= b.y + 0.01
                                || b.y + bh <= a.y + 0.01;
                            assert!(apart, "{} {style:?}: {} and {} overlap at {now}", look.name, a.id, b.id);
                        }
                    }
                }
            }
            assert!(o.stats().shown > 50, "{} {style:?}: {:?}", look.name, o.stats());
        }
    }
}

/// Round TW5: the places cover the whole screen, behind the play's field
/// (the middle) and behind an opponent's miniature (a corner) alike;
/// nothing is routed around them.
#[test]
fn both_styles_go_over_the_whole_screen() {
    // The field's band in the middle, a miniature at the top right.
    let field = [500.0, 0.0, 780.0, 720.0];
    let miniature = [1050.0, 30.0, 1250.0, 260.0];
    for style in [Style::Across, Style::Falling] {
        for reduced in [false, true] {
            let mut o = overlay(style, reduced);
            let mut behind_field = false;
            let mut behind_miniature = false;
            let mut across: [f32; 2] = [f32::MAX, f32::MIN];
            for f in 0..1800u32 {
                let now = f64::from(f) / 60.0;
                if f % 6 == 0 {
                    o.receive(&kappa(f, f), now, true);
                }
                advance(&mut o, now);
                for d in o.drawn(now) {
                    let (w, h) = (d.width * d.scale, d.height * d.scale);
                    let inside = |r: [f32; 4]| d.x >= r[0] && d.x + w <= r[2] && d.y >= r[1] && d.y + h <= r[3];
                    behind_field |= inside(field);
                    behind_miniature |= inside(miniature);
                    let (lo, hi) = if style == Style::Across && !reduced {
                        (d.y, d.y + h)
                    } else {
                        (d.x, d.x + w)
                    };
                    across = [across[0].min(lo), across[1].max(hi)];
                }
            }
            let room = if style == Style::Across && !reduced { SIZE[1] } else { SIZE[0] };
            assert!(behind_field, "{style:?} {reduced}: behind the field");
            assert!(behind_miniature, "{style:?} {reduced}: behind a miniature");
            assert!(across[0] < LINE && across[1] > room - LINE, "{style:?} {reduced}: {across:?}");
        }
    }
}

#[test]
fn falling_items_come_down_from_the_top() {
    let mut o = overlay(Style::Falling, false);
    for i in 0..4 {
        o.receive(&say(i, i, "hey"), 0.0, true);
    }
    advance(&mut o, 0.0);
    let ds: Vec<_> = o.drawn(0.0).collect();
    assert_eq!(ds.len(), 4);
    assert!(ds.iter().all(|d| d.y + d.height * d.scale <= 0.01), "from above the top");
    let y0 = o.drawn(0.0).next().unwrap().y;
    let y1 = o.drawn(1.0).next().unwrap().y;
    assert!(y1 > y0);
}

#[test]
fn reduced_motion_stands_still_and_fades() {
    for style in [Style::Across, Style::Falling] {
        let mut o = overlay(style, true);
        o.receive(&say(1, 1, "still"), 0.0, true);
        advance(&mut o, 0.0);
        let a = o.drawn(0.1).next().unwrap();
        let b = o.drawn(2.0).next().unwrap();
        assert_eq!((a.x, a.y), (b.x, b.y));
        assert!(a.alpha < 1.0 && b.alpha == 1.0);
        let late = o.drawn(CHAT_OVERLAY_V1.still - 0.1).next().unwrap();
        assert!(late.alpha < 1.0);
        assert_eq!(o.drawn(CHAT_OVERLAY_V1.still + 0.1).count(), 0);
        let mut o = overlay(style, true);
        for i in 0..30 {
            o.receive(&kappa(i, i), 0.0, true);
        }
        advance(&mut o, 0.0);
        let at: Vec<(f32, f32)> = o.drawn(1.0).map(|d| (d.x, d.y)).collect();
        let later: Vec<(f32, f32)> = o.drawn(3.0).map(|d| (d.x, d.y)).collect();
        assert_eq!(at, later);
        assert_eq!(at.len(), 30.min(CHAT_OVERLAY_V2.per_second));
    }
}

#[test]
fn an_item_waits_a_second_for_its_emotes_then_goes_without() {
    let mut o = overlay(Style::Across, false);
    let mut m = say(1, 1, "hi ");
    if let Event::Message(c) = &mut m {
        c.fragments.push(emote("25"));
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

/// Names lead; four of an emote are four emotes, side by side, never one
/// with a count.
#[test]
fn names_lead_and_every_emote_is_drawn() {
    let mut o = Overlay::new(
        rules(LOOK_LARGER),
        Options {
            names: true,
            ..options(Style::Across, false)
        },
        EmoteSets::default(),
    );
    o.receive(&emotes(1, 1, &["25", "25", "25", "25"]), 0.0, true);
    advance(&mut o, 0.0);
    let d = o.drawn(0.0).next().unwrap();
    let what: Vec<&Glyphs> = d.laid.iter().map(|l| &l.what).collect();
    let kappa = Glyphs::Emote(Emote::twitch("25"));
    assert_eq!(what, [&Glyphs::Name("User1".into()), &kappa, &kappa, &kappa, &kappa]);
    assert!(d.laid[1].x > d.laid[0].width);
    assert!(d.laid.windows(2).all(|w| w[1].x >= w[0].x + w[0].width), "side by side");
}

/// A property over scripted floods: the emotes drawn are every emote
/// admitted, one for one: each drawn item shows its message's emotes, the
/// first eight of them, a repeated one as often as it came.
#[test]
fn no_count_is_ever_drawn() {
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut rand = move || {
        seed ^= seed >> 12;
        seed ^= seed << 25;
        seed ^= seed >> 27;
        seed.wrapping_mul(0x2545_f491_4f6c_dd1d)
    };
    for look in [LOOK_AS_BUILT, LOOK_LARGER] {
        for style in [Style::Across, Style::Falling] {
            let mut o = Overlay::new(rules(look), options(style, false), EmoteSets::default());
            let mut expected: HashMap<String, usize> = HashMap::new();
            let mut seen = 0;
            let mut id = 0u32;
            for f in 0..900u32 {
                let now = f64::from(f) / 60.0;
                for _ in 0..(rand() % 3) {
                    let n = 1 + (rand() % 20) as usize;
                    let ids: Vec<String> = (0..n).map(|_| (rand() % 4).to_string()).collect();
                    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
                    id += 1;
                    expected.insert(format!("m{id}"), n.min(CHAT_OVERLAY_V2.cut.emotes));
                    o.receive(&emotes(id, id, &ids), now, true);
                }
                advance(&mut o, now);
                for d in o.drawn(now) {
                    assert!(d.laid.iter().all(|l| matches!(l.what, Glyphs::Emote(_))));
                    let message = &o.shown.iter().find(|s| s.id == d.id).unwrap().content.message_id;
                    assert_eq!(emotes_in(&d), expected[message], "{}", look.name);
                    seen += 1;
                }
            }
            assert!(seen > 1000, "{seen}");
        }
    }
}

/// The same property scattered: every drawn item one emote, and no
/// message more items than its emotes (24 at most).
#[test]
fn scattered_items_are_one_emote_each() {
    for style in [Style::Across, Style::Falling] {
        let mut o = Overlay::new(rules(LOOK_SCATTERED), options(style, false), EmoteSets::default());
        let mut items: HashMap<String, std::collections::HashSet<u64>> = HashMap::new();
        for f in 0..900u32 {
            let now = f64::from(f) / 60.0;
            if f % 3 == 0 {
                let n = 1 + (f as usize * 7) % 40;
                let ids: Vec<&str> = (0..n).map(|k| ["1", "2", "3"][k % 3]).collect();
                o.receive(&emotes(f, f, &ids), now, true);
            }
            advance(&mut o, now);
            for d in o.drawn(now) {
                assert_eq!(emotes_in(&d), 1);
                let m = o.shown.iter().find(|s| s.id == d.id).unwrap().content.message_id.clone();
                items.entry(m).or_default().insert(d.id);
            }
        }
        assert!(items.values().all(|i| i.len() <= CHAT_OVERLAY_V2.cut.emotes));
        assert!(items.values().any(|i| i.len() >= 4), "{style:?}: a message as several items");
    }
}

/// The scattered look: an emote-only message's emotes each an item of
/// their own, coming in a little apart at places of their own; a delete
/// takes them all.
#[test]
fn a_scattered_message_is_each_emote_by_itself() {
    let mut o = Overlay::new(rules(LOOK_SCATTERED), options(Style::Across, false), EmoteSets::default());
    o.receive(&emotes(1, 1, &["25", "25", "25", "25"]), 0.0, true);
    for f in 0..60 {
        advance(&mut o, f64::from(f) / 60.0);
    }
    let ds: Vec<_> = o.drawn(1.0).collect();
    assert_eq!(ds.len(), 4);
    assert!(ds.iter().all(|d| emotes_in(d) == 1));
    let mut ys: Vec<f32> = ds.iter().map(|d| d.y).collect();
    ys.dedup();
    assert!(ys.len() > 1, "at places of their own: {ys:?}");
    let mut xs: Vec<f32> = ds.iter().map(|d| d.x).collect();
    xs.sort_by(f32::total_cmp);
    xs.dedup();
    assert_eq!(xs.len(), 4, "coming in apart");
    assert_eq!(o.messages(), 1);
    // v1's own look (the recommendation) is this one.
    assert_eq!(CHAT_OVERLAY_V1.look, LOOK_SCATTERED);
    o.receive(
        &Event::Delete {
            message_id: "m1".into(),
        },
        1.1,
        true,
    );
    assert!(o.is_empty());
}

#[test]
fn the_same_events_and_clock_give_the_same_layer() {
    let run = |look, style, reduced| {
        let mut o = Overlay::new(rules(look), options(style, reduced), EmoteSets::default());
        let mut out = Vec::new();
        for f in 0..600 {
            let now = f64::from(f) / 60.0;
            if f % 4 == 0 {
                let e = if f % 8 == 0 {
                    emotes(f, f % 13, &["1", "1", "2"])
                } else {
                    say(f, f % 13, &"w".repeat((f % 30) as usize + 1))
                };
                o.receive(&e, now, f % 50 != 0);
            }
            advance(&mut o, now);
            out.extend(o.drawn(now).map(|d| (d.id, d.x.to_bits(), d.y.to_bits(), d.alpha.to_bits())));
        }
        out
    };
    for look in LOOKS {
        for style in [Style::Across, Style::Falling] {
            for reduced in [false, true] {
                assert_eq!(run(look, style, reduced), run(look, style, reduced));
            }
        }
    }
}

#[test]
fn off_shows_nothing() {
    let mut o = Overlay::new(
        CHAT_OVERLAY_V1,
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

#[test]
fn a_line_too_wide_is_cut_with_an_ellipsis() {
    // Falling's lines are 14 lines wide at most.
    let mut o = overlay(Style::Falling, false);
    o.receive(&say(1, 1, &"w".repeat(80)), 0.0, true);
    advance(&mut o, 0.0);
    let d = o.drawn(0.0).next().unwrap();
    assert!(d.width <= LINE * 14.0 + 0.5, "{}", d.width);
    let Glyphs::Text(t) = &d.laid[0].what else {
        panic!()
    };
    assert!(t.ends_with('…'), "{t}");
}

/// The chosen style is the style: a message under "Emotes" shows its
/// emotes alone, under "Emotes and messages" its text too, in either
/// style; nothing about the screen's shape changes either.
#[test]
fn the_level_and_the_style_are_the_devices() {
    let mixed = Event::Message(ChatMessage {
        id: "m1".into(),
        chatter_id: "u1".into(),
        text: "hello Kappa".into(),
        fragments: vec![Fragment::Text("hello ".into()), emote("25")],
        kind: "text".into(),
        ..ChatMessage::default()
    });
    // A phone held upright: messages too (round TW5).
    let phone = Area::new([1170.0, 2532.0], 84.0);
    for level in [Level::Emotes, Level::Messages] {
        for style in [Style::Across, Style::Falling] {
            let mut o = Overlay::new(
                CHAT_OVERLAY_V1,
                Options {
                    level,
                    ..options(style, false)
                },
                EmoteSets::default(),
            );
            o.receive(&mixed, 0.0, true);
            o.receive(&say(2, 2, "words alone"), 0.0, true);
            o.advance(0.0, &phone, &ready, &mut measure);
            let drawn: Vec<_> = o.drawn(0.0).collect();
            if level == Level::Emotes {
                assert_eq!(drawn.len(), 1, "the words alone are left out");
                assert!(drawn[0].laid.iter().all(|l| matches!(l.what, Glyphs::Emote(_))));
            } else {
                assert_eq!(drawn.len(), 2);
            }
            assert_eq!(o.options().style, style);
        }
    }
}

/// The looks' sizes: an emote-only item's emotes and a line's inline ones
/// as tall as the look says, the text a line, centered together.
#[test]
fn the_look_sizes_emotes_and_text() {
    for look in LOOKS {
        let mut o = Overlay::new(rules(look), options(Style::Across, false), EmoteSets::default());
        o.receive(&kappa(1, 1), 0.0, true);
        let mut m = say(2, 2, "hi ");
        if let Event::Message(c) = &mut m {
            c.fragments.push(emote("25"));
        }
        o.receive(&m, 0.0, true);
        advance(&mut o, 0.0);
        let ds: Vec<_> = o.drawn(0.0).collect();
        let only = ds.iter().find(|d| d.laid.len() == 1).unwrap();
        assert!((only.height - LINE * look.emote).abs() < 0.01);
        let line = ds.iter().find(|d| d.laid.len() == 2).unwrap();
        assert_eq!(line.laid[0].height, LINE);
        assert!((line.laid[1].height - LINE * look.inline).abs() < 0.01);
        assert!((line.height - LINE * look.inline.max(1.0)).abs() < 0.01);
    }
}
