//! The window's saved state as plain data: its round trip, the check
//! against the displays present, the writes' timing on given times (no
//! clock is read), and which runs remember it.

use super::*;
use ddi_ui::event::Modifiers;

/// A Mac's built-in display, 1440 × 900 points at 2×, the main one.
fn built_in() -> Display {
    Display {
        name: Some("Built-in Retina Display".into()),
        position: [0, 0],
        size: [2880, 1800],
        scale: 2.0,
    }
}

/// An external display left of it, 1920 × 1080 at 1× (points and pixels
/// alike).
fn external() -> Display {
    Display {
        name: Some("DELL U2720Q".into()),
        position: [-1920, 0],
        size: [1920, 1080],
        scale: 1.0,
    }
}

/// What the shell would see of a normal window on `d`.
fn seen_on(d: &Display, inner_logical: [f64; 2], offset: [f64; 2]) -> Observed {
    Observed {
        inner: [
            (inner_logical[0] * d.scale) as u32,
            (inner_logical[1] * d.scale) as u32,
        ],
        outer: Some([
            d.position[0] + (offset[0] * d.scale) as i32,
            d.position[1] + (offset[1] * d.scale) as i32,
        ]),
        scale: d.scale,
        display: Some(d.clone()),
        maximized: false,
        minimized: false,
        fullscreen: false,
    }
}

fn saved_on(d: &Display, size: [f64; 2], offset: [f64; 2]) -> Saved {
    Saved::observe(None, &seen_on(d, size, offset)).unwrap()
}

#[test]
fn nothing_saved_opens_as_before() {
    let p = place(None, &[built_in()], Some(0), Coordinates::Points);
    assert_eq!(p.reason, Reason::Fresh);
    assert_eq!(p.size, DEFAULT_SIZE);
    // Where the system puts a new window.
    assert_eq!(p.at, None);
    assert!(!p.maximized);
    assert_eq!(p.fullscreen, None);
    // No displays told at all: the same.
    let p = place(None, &[], None, Coordinates::Pixels);
    assert_eq!(
        (p.size, p.at, p.reason),
        (DEFAULT_SIZE, None, Reason::Fresh)
    );
}

#[test]
fn a_small_display_gets_a_window_that_fits() {
    let laptop = Display {
        name: Some("Generic PnP Monitor".into()),
        position: [0, 0],
        size: [1366, 768],
        scale: 1.0,
    };
    let s = sensible(Some(&laptop));
    assert!(s[0] <= 1366.0 * FILL && s[1] <= 768.0 * FILL, "{s:?}");
    // Its shape kept.
    assert!((s[0] / s[1] - 16.0 / 9.0).abs() < 0.01, "{s:?}");
    // A large one keeps the usual size.
    assert_eq!(sensible(Some(&built_in())), DEFAULT_SIZE);
}

#[test]
fn a_window_comes_back_where_it_was_left() {
    for (coords, d) in [
        (Coordinates::Points, built_in()),
        (Coordinates::Points, external()),
        (Coordinates::Pixels, external()),
        (
            Coordinates::Pixels,
            Display {
                name: Some("LG HDR 4K".into()),
                position: [2560, -200],
                size: [3840, 2160],
                scale: 1.5,
            },
        ),
    ] {
        let saved = saved_on(&d, [1000.0, 600.0], [100.0, 50.0]);
        // Written and read back as JSON, as the store keeps it.
        let json = serde_json::to_string(&saved).unwrap();
        let read: Saved = serde_json::from_str(&json).unwrap();
        assert_eq!(read, saved);
        let displays = [built_in(), external(), d.clone()];
        let p = place(Some(&read), &displays, Some(0), coords);
        assert_eq!(p.reason, Reason::Restored, "{d:?}");
        assert_eq!(p.size, [1000.0, 600.0]);
        let want = match coords {
            Coordinates::Points => At::Logical([
                f64::from(d.position[0]) / d.scale + 100.0,
                f64::from(d.position[1]) / d.scale + 50.0,
            ]),
            Coordinates::Pixels => At::Physical([
                d.position[0] + (100.0 * d.scale) as i32,
                d.position[1] + (50.0 * d.scale) as i32,
            ]),
        };
        assert_eq!(p.at, Some(want), "{d:?}");
        // What the window is made as is what was saved.
        assert_eq!(Saved::made(&p, &displays), saved);
    }
}

#[test]
fn a_value_that_does_not_read_gives_a_new_window() {
    // Not the shape of a saved window (a file edited, another version's
    // value): a new window, said so.
    for text in ["{", "[]", "{\"frame\":1}", "\"window\"", "nonsense"] {
        let r = read(Some(text));
        assert!(matches!(r, Some(Err(_))), "{text}");
        let p = place_read(r.as_ref(), &[built_in()], Some(0), Coordinates::Points);
        assert_eq!(p.reason, Reason::Unreadable, "{text}");
        assert_eq!(p.at, Some(At::Logical([80.0, 90.0])));
    }
    // Nothing kept: a first start's.
    let p = place_read(
        read(None).as_ref(),
        &[built_in()],
        Some(0),
        Coordinates::Points,
    );
    assert_eq!(p.reason, Reason::Fresh);
    // A kept one reads back as it was written.
    let good = saved_on(&built_in(), [1000.0, 600.0], [100.0, 50.0]);
    let text = serde_json::to_string(&good).unwrap();
    assert_eq!(read(Some(&text)), Some(Ok(good.clone())));
    // Read, but no window's numbers.
    let mut bad = Vec::new();
    for size in [[0.0, 600.0], [-5.0, 600.0], [1e12, 600.0], [f64::NAN, 1.0]] {
        let mut s = good.clone();
        s.frame.size = size;
        bad.push(s);
    }
    for offset in [[f64::INFINITY, 0.0], [0.0, -1e9]] {
        let mut s = good.clone();
        s.frame.offset = Some(offset);
        bad.push(s);
    }
    let mut s = good.clone();
    s.frame.scale = f64::NAN;
    bad.push(s);
    for s in &bad {
        let p = place(
            Some(s),
            &[external(), built_in()],
            Some(1),
            Coordinates::Points,
        );
        assert_eq!(p.reason, Reason::Unreadable, "{s:?}");
        assert_eq!(p.size, DEFAULT_SIZE);
        // On the main display, centered.
        assert_eq!(p.at, Some(At::Logical([80.0, 90.0])));
        assert_eq!(p.display, Some(1));
        assert!(Reason::Unreadable.line().is_some());
    }
}

#[test]
fn a_display_gone_brings_the_window_to_the_main_one() {
    let saved = saved_on(&external(), [1600.0, 900.0], [40.0, 30.0]);
    // Unplugged: the built-in display alone, second in the list and the
    // main one.
    let other = Display {
        name: Some("Sidecar".into()),
        position: [1440, 0],
        size: [2732, 2048],
        scale: 2.0,
    };
    let p = place(
        Some(&saved),
        &[other, built_in()],
        Some(1),
        Coordinates::Points,
    );
    assert_eq!(p.reason, Reason::DisplayGone);
    assert_eq!(p.display, Some(1));
    assert_eq!(p.size, DEFAULT_SIZE);
    assert_eq!(p.at, Some(At::Logical([80.0, 90.0])));
}

#[test]
fn a_resolution_changed_keeps_the_window_where_it_still_fits() {
    let saved = saved_on(&external(), [1200.0, 800.0], [100.0, 100.0]);
    // The same display at 2560 × 1440, somewhere else in the arrangement.
    let larger = Display {
        position: [1440, 0],
        size: [2560, 1440],
        ..external()
    };
    let p = place(
        Some(&saved),
        &[built_in(), larger],
        Some(0),
        Coordinates::Pixels,
    );
    assert_eq!(p.reason, Reason::Restored);
    assert_eq!(p.at, Some(At::Physical([1540, 100])));
    // At 1280 × 720 the window is larger than the display: a new one on
    // the main display.
    let smaller = Display {
        size: [1280, 720],
        ..external()
    };
    let p = place(
        Some(&saved),
        &[built_in(), smaller],
        Some(0),
        Coordinates::Pixels,
    );
    assert_eq!(p.reason, Reason::TooLarge);
    assert_eq!(p.display, Some(0));
    assert_eq!(p.size, DEFAULT_SIZE);
}

#[test]
fn a_scale_changed_keeps_the_size_as_it_looks() {
    let saved = saved_on(&built_in(), [1200.0, 800.0], [120.0, 50.0]);
    assert_eq!(saved.frame.scale, 2.0);
    // The same panel at 1× ("More space"): 2880 × 1800 points.
    let wider = Display {
        scale: 1.0,
        ..built_in()
    };
    let p = place(Some(&saved), &[wider], Some(0), Coordinates::Points);
    assert_eq!(p.reason, Reason::Restored);
    assert_eq!(p.size, [1200.0, 800.0]);
    assert_eq!(p.at, Some(At::Logical([120.0, 50.0])));
    // At 3×, 960 × 600 points: too large for it now.
    let larger_text = Display {
        scale: 3.0,
        ..built_in()
    };
    let p = place(Some(&saved), &[larger_text], Some(0), Coordinates::Points);
    assert_eq!(p.reason, Reason::TooLarge);
    assert!(p.size[0] <= 960.0 * FILL && p.size[1] <= 600.0 * FILL);
}

#[test]
fn a_window_too_small_to_use_is_made_again() {
    for size in [[100.0, 600.0], [800.0, 100.0], [319.0, 239.0]] {
        let saved = saved_on(&built_in(), size, [10.0, 40.0]);
        let p = place(Some(&saved), &[built_in()], Some(0), Coordinates::Points);
        assert_eq!(p.reason, Reason::TooSmall, "{size:?}");
        assert_eq!(p.size, DEFAULT_SIZE);
    }
    let saved = saved_on(&built_in(), MIN_SIZE, [10.0, 40.0]);
    let p = place(Some(&saved), &[built_in()], Some(0), Coordinates::Points);
    assert_eq!(p.reason, Reason::Restored);
}

#[test]
fn a_window_off_its_display_is_brought_back() {
    let room = built_in().logical();
    let off = [
        // Its title bar above the display.
        [100.0, -200.0],
        // Below it, or too low to grab.
        [100.0, room[1] + 10.0],
        [100.0, room[1] - 20.0],
        // Right or left of it, too little left to grab.
        [room[0] - 50.0, 100.0],
        [-950.0, 100.0],
    ];
    for offset in off {
        let saved = saved_on(&built_in(), [1000.0, 600.0], offset);
        let p = place(Some(&saved), &[built_in()], Some(0), Coordinates::Points);
        assert_eq!(p.reason, Reason::OffDisplay, "{offset:?}");
        assert_eq!(p.at, Some(At::Logical([80.0, 90.0])));
    }
    // Partly off but its title bar there to grab: as left, and Windows'
    // invisible borders a few pixels over the edge.
    for offset in [[room[0] - 200.0, 100.0], [-800.0, 300.0], [-7.0, -7.0]] {
        let saved = saved_on(&built_in(), [1000.0, 600.0], offset);
        let p = place(Some(&saved), &[built_in()], Some(0), Coordinates::Points);
        assert_eq!(p.reason, Reason::Restored, "{offset:?}");
    }
}

#[test]
fn displays_of_one_name_are_told_apart_by_their_place() {
    let left = Display {
        name: Some("DELL U2720Q".into()),
        position: [0, 0],
        size: [2560, 1440],
        scale: 1.0,
    };
    let right = Display {
        position: [2560, 0],
        ..left.clone()
    };
    let displays = [left.clone(), right.clone()];
    assert_eq!(find(&right.key(), &displays), Some(1));
    assert_eq!(find(&left.key(), &displays), Some(0));
    // Moved in the arrangement: the one with its size, else the first by
    // name.
    let moved = DisplayKey {
        position: [9999, 0],
        ..right.key()
    };
    assert_eq!(find(&moved, &displays), Some(0));
    // A display with no name: only its place and size find it.
    let unnamed = Display {
        name: None,
        ..right.clone()
    };
    let displays = [left, unnamed.clone()];
    assert_eq!(find(&unnamed.key(), &displays), Some(1));
    let elsewhere = DisplayKey {
        position: [0, 0],
        ..unnamed.key()
    };
    assert_eq!(find(&elsewhere, &displays), None);
}

#[test]
fn another_machines_data_folder_opens_usable() {
    // Saved on a desktop with a large display that this laptop lacks.
    let saved = saved_on(&external(), [1800.0, 1000.0], [60.0, 40.0]);
    let mut full = saved.clone();
    full.fullscreen = true;
    full.fullscreen_on = Some(external().key());
    let laptop = Display {
        name: Some("Color LCD".into()),
        position: [0, 0],
        size: [2560, 1600],
        scale: 2.0,
    };
    let p = place(
        Some(&saved),
        std::slice::from_ref(&laptop),
        None,
        Coordinates::Points,
    );
    assert_eq!(p.reason, Reason::DisplayGone);
    assert_eq!(p.display, Some(0));
    let p = place(Some(&full), &[laptop], None, Coordinates::Points);
    assert_eq!(p.fullscreen, Some(Some(0)));
    assert_eq!(p.fullscreen_reason, Some(Reason::DisplayGone));
}

#[test]
fn fullscreen_comes_back_on_its_display_or_the_main_one() {
    let mut saved = saved_on(&built_in(), [1000.0, 600.0], [100.0, 50.0]);
    saved.fullscreen = true;
    saved.fullscreen_on = Some(external().key());
    // Its display there: on it, the normal frame kept for leaving it.
    let p = place(
        Some(&saved),
        &[built_in(), external()],
        Some(0),
        Coordinates::Points,
    );
    assert_eq!(p.fullscreen, Some(Some(1)));
    assert_eq!(p.fullscreen_reason, Some(Reason::Restored));
    assert_eq!(p.reason, Reason::Restored);
    assert_eq!(p.size, [1000.0, 600.0]);
    assert!(!p.maximized);
    // Gone: fullscreen on the main display.
    let p = place(Some(&saved), &[built_in()], Some(0), Coordinates::Points);
    assert_eq!(p.fullscreen, Some(Some(0)));
    assert_eq!(p.fullscreen_reason, Some(Reason::DisplayGone));
    // No displays told (none listed): fullscreen where the window is.
    let p = place(Some(&saved), &[], None, Coordinates::Pixels);
    assert_eq!(p.fullscreen, Some(None));
    // Fullscreen saved where no display was told: on the main one.
    saved.fullscreen_on = None;
    let p = place(
        Some(&saved),
        &[external(), built_in()],
        Some(1),
        Coordinates::Points,
    );
    assert_eq!(p.fullscreen, Some(Some(1)));
    assert_eq!(p.fullscreen_reason, Some(Reason::Restored));
}

#[test]
fn maximized_comes_back_maximized_even_over_a_new_frame() {
    let mut saved = saved_on(&external(), [1000.0, 600.0], [100.0, 50.0]);
    saved.maximized = true;
    let p = place(
        Some(&saved),
        &[external(), built_in()],
        Some(1),
        Coordinates::Pixels,
    );
    assert!(p.maximized);
    assert_eq!(p.reason, Reason::Restored);
    let p = place(Some(&saved), &[built_in()], Some(0), Coordinates::Pixels);
    assert!(p.maximized);
    assert_eq!(p.reason, Reason::DisplayGone);
}

#[test]
fn what_the_window_says_is_learned_as_its_normal_frame() {
    let d = external();
    let normal = seen_on(&d, [1000.0, 600.0], [100.0, 50.0]);
    let kept = Saved::observe(None, &normal).unwrap();
    assert_eq!(kept.frame.size, [1000.0, 600.0]);
    assert_eq!(kept.frame.offset, Some([100.0, 50.0]));
    assert_eq!(kept.frame.display, Some(d.key()));
    // Minimized (Windows' −32000 and nothing) or of no size: nothing.
    for seen in [
        Observed {
            minimized: true,
            outer: Some([-32000, -32000]),
            ..normal.clone()
        },
        Observed {
            inner: [0, 0],
            ..normal.clone()
        },
    ] {
        assert_eq!(Saved::observe(Some(&kept), &seen), Some(kept.clone()));
        assert_eq!(Saved::observe(None, &seen), None);
    }
    // Maximized or fullscreen: the frame before kept, the state learned.
    let max = Observed {
        maximized: true,
        inner: [1920, 1040],
        outer: Some([-1920, 0]),
        ..normal.clone()
    };
    let m = Saved::observe(Some(&kept), &max).unwrap();
    assert_eq!(m.frame, kept.frame);
    assert!(m.maximized && !m.fullscreen);
    let full = Observed {
        fullscreen: true,
        display: Some(built_in()),
        inner: [2880, 1800],
        outer: Some([0, 0]),
        scale: 2.0,
        ..normal.clone()
    };
    let f = Saved::observe(Some(&m), &full).unwrap();
    assert_eq!(f.frame, kept.frame);
    assert!(f.fullscreen && !f.maximized);
    assert_eq!(f.fullscreen_on, Some(built_in().key()));
    // Left again: the frame as it is then.
    let back = Saved::observe(Some(&f), &normal).unwrap();
    assert_eq!(back, kept);
    // No place told (Wayland): the size alone, put where the system puts
    // it.
    let wayland = Observed {
        outer: None,
        ..normal
    };
    let w = Saved::observe(None, &wayland).unwrap();
    assert_eq!(w.frame.offset, None);
    let p = place(Some(&w), &[external()], None, Coordinates::Pixels);
    assert_eq!(
        (p.reason, p.at, p.size),
        (Reason::Restored, None, [1000.0, 600.0])
    );
}

#[test]
fn a_window_made_fullscreen_keeps_the_frame_it_goes_back_to() {
    let mut saved = saved_on(&built_in(), [1000.0, 600.0], [100.0, 50.0]);
    saved.fullscreen = true;
    saved.fullscreen_on = Some(built_in().key());
    let displays = [built_in()];
    let p = place(Some(&saved), &displays, Some(0), Coordinates::Points);
    let made = Saved::made(&p, &displays);
    assert_eq!(made, saved);
    let mut w = Writes::new(Some(saved.clone()), made);
    // The window's first words, fullscreen at the display's size: the
    // frame is still the one it was made with, nothing to write.
    let full = Observed {
        fullscreen: true,
        ..seen_on(&built_in(), [1440.0, 900.0], [0.0, 0.0])
    };
    w.observe(&full, 0.0);
    assert_eq!(w.current(), Some(&saved));
    assert_eq!(w.wake(), None);
    assert_eq!(w.last(), None);
}

#[test]
fn what_was_not_given_back_is_replaced() {
    // Saved off its display: made centered on the main one, and that is
    // what is written at the end even if nothing changes.
    let bad = saved_on(&built_in(), [1000.0, 600.0], [100.0, -500.0]);
    let displays = [built_in()];
    let p = place(Some(&bad), &displays, Some(0), Coordinates::Points);
    assert_eq!(p.reason, Reason::OffDisplay);
    let made = Saved::made(&p, &displays);
    let mut w = Writes::new(Some(bad), made.clone());
    assert_eq!(w.last(), Some(made));
    assert_eq!(w.last(), None);
}

#[test]
fn writes_wait_until_the_window_is_left_as_it_is() {
    let d = built_in();
    let start = saved_on(&d, [1000.0, 600.0], [100.0, 50.0]);
    let mut w = Writes::new(Some(start.clone()), start.clone());
    assert_eq!(w.wake(), None);
    assert_eq!(w.due(100.0), None);
    // The same as written: nothing.
    w.observe(&seen_on(&d, [1000.0, 600.0], [100.0, 50.0]), 0.0);
    assert_eq!(w.wake(), None);
    // Dragged: each move puts the write off.
    w.observe(&seen_on(&d, [1000.0, 600.0], [110.0, 50.0]), 10.0);
    assert_eq!(w.wake(), Some(10.0 + WRITE_AFTER));
    assert_eq!(w.due(10.5), None);
    w.observe(&seen_on(&d, [1000.0, 600.0], [120.0, 60.0]), 10.8);
    assert_eq!(w.due(10.0 + WRITE_AFTER), None);
    let moved = saved_on(&d, [1000.0, 600.0], [120.0, 60.0]);
    assert_eq!(w.due(10.8 + WRITE_AFTER), Some(moved.clone()));
    // Written once.
    assert_eq!(w.wake(), None);
    assert_eq!(w.due(1000.0), None);
    assert_eq!(w.last(), None);
    // Moved and back before it was due: nothing to write.
    w.observe(&seen_on(&d, [1000.0, 600.0], [300.0, 60.0]), 20.0);
    w.observe(&seen_on(&d, [1000.0, 600.0], [120.0, 60.0]), 20.2);
    assert_eq!(w.wake(), None);
    assert_eq!(w.last(), None);
    // Resized and closed at once: written as it closes.
    w.observe(&seen_on(&d, [900.0, 600.0], [120.0, 60.0]), 30.0);
    let resized = saved_on(&d, [900.0, 600.0], [120.0, 60.0]);
    assert_eq!(w.last(), Some(resized));
    assert_eq!(w.due(1000.0), None);
}

#[test]
fn a_fresh_start_writes_what_the_window_became() {
    let displays = [built_in()];
    let p = place(None, &displays, Some(0), Coordinates::Points);
    let mut w = Writes::new(None, Saved::made(&p, &displays));
    // Placed by the system, then told: written once left so.
    w.observe(&seen_on(&built_in(), [1280.0, 720.0], [80.0, 90.0]), 1.0);
    assert_eq!(
        w.due(1.0 + WRITE_AFTER),
        Some(saved_on(&built_in(), [1280.0, 720.0], [80.0, 90.0]))
    );
}

#[test]
fn the_fullscreen_key_is_the_systems_own() {
    let none = Modifiers::default();
    let ctrl_cmd = Modifiers {
        ctrl: true,
        meta: true,
        ..none
    };
    // macOS: ⌃⌘F, by the layout's F.
    assert!(is_fullscreen_key("KeyF", "f", ctrl_cmd, true));
    assert!(is_fullscreen_key("KeyU", "f", ctrl_cmd, true));
    assert!(!is_fullscreen_key(
        "KeyF",
        "f",
        Modifiers { meta: true, ..none },
        true
    ));
    assert!(!is_fullscreen_key(
        "KeyF",
        "f",
        Modifiers {
            shift: true,
            ..ctrl_cmd
        },
        true
    ));
    assert!(!is_fullscreen_key("F11", "F11", none, true));
    // Windows and Linux: F11 alone.
    assert!(is_fullscreen_key("F11", "F11", none, false));
    assert!(!is_fullscreen_key(
        "F11",
        "F11",
        Modifiers {
            shift: true,
            ..none
        },
        false
    ));
    assert!(!is_fullscreen_key("KeyF", "f", ctrl_cmd, false));
}

#[test]
fn timed_runs_neither_read_nor_write_it() {
    let words = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
    for (line, remembers) in [
        ("", true),
        ("--screen select", true),
        ("--play beatucada --chart 0 --auto --start", true),
        ("--pacing-report --play beatucada", true),
        ("--seconds 5", false),
        ("--screen options --seconds 3 --silent", false),
    ] {
        let w = words(line);
        assert_eq!(super::remembers(crate::args::Args(&w)), remembers, "{line}");
    }
}
