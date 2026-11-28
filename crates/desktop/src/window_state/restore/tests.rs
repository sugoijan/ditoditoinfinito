//! The fullscreen restore as plain data: signals and times given, the
//! loop's turns driven at the times the restore asks for (no clock).

use super::*;

/// A window settled: focused, in view, on the active Space.
fn settled() -> Signals {
    Signals {
        focused: true,
        visible: true,
        on_space: true,
        fullscreen: false,
        playing: false,
    }
}

fn full() -> Signals {
    Signals {
        fullscreen: true,
        ..settled()
    }
}

/// Turns of the loop with no event from `from` to `to`, at the times the
/// restore wakes for, the window as `s`: every act but `Nothing`, timed.
fn idle(r: &mut Restore, s: Signals, from: f64, to: f64) -> Vec<(f64, Act)> {
    let mut acts = Vec::new();
    let mut now = from;
    let mut turns = 0;
    while now <= to {
        let act = r.see(&s, now);
        if act != Act::Nothing {
            acts.push((now, act));
        }
        let Some(next) = r.wake(now) else { break };
        // The loop never spins: each wake is later than the turn.
        assert!(next > now, "woke for {next} at {now}");
        now = next;
        turns += 1;
        assert!(turns < 1000, "too many turns");
    }
    acts
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn a_settled_window_is_asked_once_and_takes() {
    let mut r = Restore::new(10.0);
    // Not before it was steady for the quiet time.
    let acts = idle(&mut r, settled(), 10.0, 10.0 + QUIET);
    assert_eq!(acts.len(), 1);
    assert!(close(acts[0].0, 10.0 + QUIET));
    assert_eq!(acts[0].1, Act::Ask { tries: 1 });
    // The window says it is fullscreen at the next turn: done.
    assert_eq!(r.see(&full(), 10.5), Act::Took { tries: 1 });
    assert!(!r.active());
    assert_eq!(r.wake(10.5), None);
    assert_eq!(r.see(&settled(), 11.0), Act::Nothing);
}

#[test]
fn where_the_window_is_made_fullscreen_it_takes_at_once() {
    // Windows and Linux: made fullscreen by winit, so it is at its first
    // turn, nothing asked.
    let mut r = Restore::new(0.0);
    assert_eq!(r.see(&full(), 0.0), Act::Took { tries: 0 });
    assert!(!r.active());
}

#[test]
fn a_space_switch_is_waited_out_before_asking() {
    // Every display shows another app's fullscreen Space: the window is
    // made on a desktop Space, focused at once, and macOS switches a
    // display to it, which ends about a second later; meanwhile the
    // window is covered, then shown, then on the active Space.
    let mut r = Restore::new(0.0);
    let switching = Signals {
        visible: false,
        on_space: false,
        ..settled()
    };
    assert!(idle(&mut r, switching, 0.0, 0.6).is_empty());
    let shown = Signals {
        on_space: false,
        ..settled()
    };
    assert!(idle(&mut r, shown, 0.6, 1.0).is_empty());
    // On the active Space from 1.0: asked once that has held.
    let acts = idle(&mut r, settled(), 1.0, 1.0 + QUIET + 0.01);
    assert_eq!(acts.len(), 1);
    assert!(acts[0].0 >= 1.0 + QUIET, "{acts:?}");
    assert_eq!(acts[0].1, Act::Ask { tries: 1 });
    assert_eq!(r.see(&full(), 2.0), Act::Took { tries: 1 });
}

#[test]
fn a_refused_request_is_made_again_and_takes() {
    let mut r = Restore::new(0.0);
    let acts = idle(&mut r, settled(), 0.0, 0.3 + WAITS[0]);
    assert_eq!(
        acts.iter().map(|a| a.1).collect::<Vec<_>>(),
        [Act::Ask { tries: 1 }, Act::Ask { tries: 2 }]
    );
    assert!(close(acts[1].0, 0.3 + WAITS[0]));
    assert_eq!(r.see(&full(), 1.2), Act::Took { tries: 2 });
}

#[test]
fn a_retry_waits_for_the_windows_own_transition() {
    let mut r = Restore::new(0.0);
    assert_eq!(
        idle(&mut r, settled(), 0.0, 0.3).last().unwrap().1,
        Act::Ask { tries: 1 }
    );
    // Covered during a transition from just after the request until 1.4:
    // its wait ends at 0.7, but no request is made into it.
    let covered = Signals {
        visible: false,
        ..settled()
    };
    assert!(idle(&mut r, covered, 0.35, 1.4).is_empty());
    let acts = idle(&mut r, settled(), 1.4, 2.0);
    assert_eq!(acts.len(), 1, "{acts:?}");
    assert!(acts[0].0 >= 1.4 + QUIET);
    assert_eq!(acts[0].1, Act::Ask { tries: 2 });
}

#[test]
fn a_restore_never_taken_gives_up_after_the_last_request() {
    let mut r = Restore::new(0.0);
    let acts = idle(&mut r, settled(), 0.0, 20.0);
    let kinds: Vec<Act> = acts.iter().map(|a| a.1).collect();
    assert_eq!(
        kinds,
        [
            Act::Ask { tries: 1 },
            Act::Ask { tries: 2 },
            Act::Ask { tries: 3 },
            Act::Ask { tries: 4 },
            Act::GaveUp { tries: 4 },
        ]
    );
    // The waits grow as WAITS says, the requests settled each time.
    let times: Vec<f64> = acts.iter().map(|a| a.0).collect();
    for (i, w) in WAITS.iter().enumerate() {
        assert!(close(times[i + 1] - times[i], *w), "{times:?}");
    }
    // Quiet then: no more turns asked for, nothing asked.
    assert_eq!(r.wake(20.0), None);
    assert!(r.active() && !r.asking());
    assert_eq!(r.see(&settled(), 30.0), Act::Nothing);
    // The player's own fullscreen later (the green button) is taken.
    assert_eq!(r.see(&full(), 40.0), Act::Took { tries: 0 });
}

#[test]
fn the_player_pressing_the_key_ends_it() {
    let mut r = Restore::new(0.0);
    idle(&mut r, settled(), 0.0, 0.3);
    assert_eq!(r.toggled(), Act::Stopped(Why::Toggled));
    assert!(!r.active());
    // Nothing asked after, whatever the window does.
    assert!(idle(&mut r, settled(), 0.4, 10.0).is_empty());
    assert_eq!(r.toggled(), Act::Nothing);
}

#[test]
fn a_play_starting_stops_the_requests_but_not_the_wish() {
    let mut r = Restore::new(0.0);
    idle(&mut r, settled(), 0.0, 0.3);
    let playing = Signals {
        playing: true,
        ..settled()
    };
    assert_eq!(r.see(&playing, 0.5), Act::Stopped(Why::Play));
    assert!(idle(&mut r, playing, 0.5, 10.0).is_empty());
    assert!(r.active() && !r.asking());
    // The player resizing the window afterwards is their choice.
    assert_eq!(r.frame_changed(&settled(), 12.0), Act::Stopped(Why::Frame));
    assert!(!r.active());
}

#[test]
fn the_windows_own_frame_changes_are_not_the_players() {
    let mut r = Restore::new(0.0);
    // Before it settled (macOS putting it below the menu bar): the
    // system's.
    assert_eq!(r.frame_changed(&settled(), 0.1), Act::Nothing);
    idle(&mut r, settled(), 0.1, 0.4);
    assert_eq!(r.tries(), 1);
    // Just after a request (a refusal putting the frame back): the
    // system's.
    assert_eq!(r.frame_changed(&settled(), 0.5), Act::Nothing);
    assert!(r.active());
    // Long after it: the player's.
    let mut r2 = r.clone();
    assert_eq!(
        r2.frame_changed(&settled(), 0.4 + TRANSITION + 0.1),
        Act::Stopped(Why::Frame)
    );
    // Once given up, any change is the player's.
    idle(&mut r, settled(), 0.5, 20.0);
    assert!(!r.asking());
    assert_eq!(r.frame_changed(&settled(), 20.0), Act::Stopped(Why::Frame));
}

#[test]
fn flapping_occlusion_is_waited_out_and_never_waited_on_forever() {
    // Covered and shown every 0.2 s for 3 s, then steady: asked once
    // steady for the quiet time.
    let mut r = Restore::new(0.0);
    let mut now = 0.0;
    let mut visible = true;
    while now < 3.0 {
        let s = Signals {
            visible,
            ..settled()
        };
        assert_eq!(r.see(&s, now), Act::Nothing, "at {now}");
        visible = !visible;
        now += 0.2;
    }
    let acts = idle(&mut r, settled(), now, now + 1.0);
    assert_eq!(acts[0].1, Act::Ask { tries: 1 });
    assert!(acts[0].0 >= now + QUIET - 1e-9);
    // Flapping for good: asked anyway after the limit, each wait too.
    let mut r = Restore::new(0.0);
    let mut asks = Vec::new();
    let mut now = 0.0;
    let mut visible = true;
    while now < 40.0 {
        let s = Signals {
            visible,
            ..settled()
        };
        match r.see(&s, now) {
            Act::Nothing => {}
            a => asks.push((now, a)),
        }
        visible = !visible;
        now += 0.2;
    }
    assert!(close_to(asks[0].0, SETTLE_LIMIT), "{asks:?}");
    assert_eq!(asks.last().unwrap().1, Act::GaveUp { tries: 4 });
}

fn close_to(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.21
}
