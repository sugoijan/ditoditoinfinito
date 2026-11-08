use std::cell::RefCell;
use std::rc::Rc;

use ddi_actor::{Actor, Kind};

use super::*;
use crate::look::{Plain, call, quad};
use crate::testing::Harness;
use crate::{Mark, PointerKind, Spot, TextMode};

type Log = Rc<RefCell<Vec<String>>>;

fn env() -> Env {
    Env::new(
        Rc::new(Plain),
        Rc::new(|s| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s)),
    )
}

/// A screen with a thing on it (a box, a part inside it), which can be
/// pointed at, and taps logged.
struct Shown {
    log: Log,
}

impl Screen for Shown {
    fn name(&self) -> &str {
        "shown"
    }

    fn build(&mut self, _cx: &mut Cx) -> Actor {
        let mut part = quad("Part", [40.0, 20.0], [1.0; 4]);
        call(&mut part, "xy", &[30.0, 0.0]);
        let mut thing = crate::screens::frame(
            "Thing",
            vec![quad("Plate", [200.0, 60.0], [0.2, 0.2, 0.2, 1.0]), part],
        );
        call(&mut thing, "xy", &[400.0, 300.0]);
        Actor::new(Kind::Frame(vec![thing]))
    }

    fn spot(&self, what: Mark) -> Option<Spot> {
        match what {
            Mark::Score => Some(Spot::new("Thing", &["Part"])),
            Mark::Grade => Some(Spot::new("Thing", &[])),
            _ => None,
        }
    }

    fn input(&mut self, input: &Input, _root: &mut Actor, _cx: &mut Cx) -> Handled {
        if let Input::Pointer {
            target: Some(t),
            gesture: crate::Gesture::Tap,
            ..
        } = input
        {
            self.log.borrow_mut().push(format!("tap {}", t.name));
            return Handled::Yes;
        }
        Handled::No
    }
}

fn frame_of(back: Log) -> StageFrame {
    StageFrame::new(
        env(),
        "Hard chart",
        "An example by Mr. Clank",
        "A hard chart is chosen.",
        Rc::new(move |_| back.borrow_mut().push("back".into())),
    )
}

fn close(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.01)
}

#[test]
fn the_frame_shows_the_screens_smaller_and_gives_them_back() {
    let log: Log = Rc::default();
    let mut h = Harness::new([1280.0, 720.0], [1280.0, 720.0], 1.0, TextMode::Own);
    h.push(Shown { log: log.clone() });
    h.run(0.3);
    let whole = h.ui.bounds("Thing").unwrap();
    h.ui.push_chrome(frame_of(log.clone()));
    h.frame(0.05);
    // On its way in: smaller already, not yet all the way.
    let coming = h.ui.bounds("Thing").unwrap();
    h.run(0.5);
    let small = h.ui.bounds("Thing").unwrap();
    let (scale, at, _, _) = geometry([1280.0, 720.0]);
    assert!(scale > 0.6 && scale < 0.9, "{scale}");
    let want = [
        whole[0] * scale + at[0],
        whole[1] * scale + at[1],
        whole[2] * scale + at[0],
        whole[3] * scale + at[1],
    ];
    assert!(close(small, want), "{small:?} {want:?}");
    assert!(coming[2] - coming[0] < whole[2] - whole[0]);
    assert!(coming[2] - coming[0] > small[2] - small[0]);
    // The screen is laid out as before: only drawn smaller.
    assert_eq!(h.ui.viewport().screen.width, 1280.0);
    // A tap on the picture is the screen's; the frame's own way back is
    // the shell's; the ground around the picture takes its taps.
    h.tap(PointerKind::Mouse, "Thing");
    assert_eq!(*log.borrow(), ["tap Plate"]);
    h.tap_at(PointerKind::Mouse, [4.0, 4.0]);
    h.tap(PointerKind::Mouse, "Stage.Back");
    assert_eq!(*log.borrow(), ["tap Plate", "back"]);
    // The caption and the words said.
    h.ui.broadcast(Message::new(CAPTION).with("Text", Param::Str("Now the next.".into())));
    h.ui.broadcast(
        Message::new(SAY)
            .with("Text", Param::Str("Whoa!".into()))
            .with("Seconds", Param::Num(1.0)),
    );
    h.frame(0.016);
    let texts = h.texts();
    assert!(texts.iter().any(|t| t == "Now the next."), "{texts:?}");
    assert!(texts.iter().any(|t| t == "Whoa!"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "“Hard chart”"), "{texts:?}");
    h.run(1.2);
    assert!(!h.texts().iter().any(|t| t == "Whoa!"));
    // Over: the picture grows back and the frame goes.
    h.ui.broadcast(Message::new(END));
    h.run(0.6);
    assert!(close(h.ui.bounds("Thing").unwrap(), whole));
    assert!(h.ui.bounds("Stage.Back").is_none(), "the frame stayed");
    assert!(!h.ui.needs_redraw() || h.ui.layers().iter().all(|l| l.phase == "active"));
}

/// A scene heard as the last one's frame is still going: the new frame
/// is the one on top, and shows the screens smaller.
#[test]
fn a_frame_shown_as_the_last_one_goes_is_the_frame() {
    let log: Log = Rc::default();
    let mut h = Harness::new([1280.0, 720.0], [1280.0, 720.0], 1.0, TextMode::Own);
    h.push(Shown { log: log.clone() });
    h.ui.push_chrome(frame_of(log.clone()));
    h.run(1.0);
    let small = h.ui.bounds("Thing").unwrap();
    h.ui.broadcast(Message::new(END));
    h.run(0.1);
    h.ui.push_chrome(frame_of(log.clone()));
    h.run(1.0);
    assert!(
        h.ui.layers().iter().all(|l| l.name != "stage"),
        "both frames are chrome: {:?}",
        h.ui.layers()
    );
    assert!(close(h.ui.bounds("Thing").unwrap(), small));
}

/// What a line in play is said for is ringed on the picture, where the
/// play shows it, inside the frame.
#[test]
fn the_mark_rings_a_plays_parts_on_its_picture() {
    let spots: SharedSpots = Rc::default();
    let mut h = Harness::new([1280.0, 720.0], [1280.0, 720.0], 1.0, TextMode::Own);
    h.push(Picture::new(PICTURE).with_spots(spots.clone()));
    h.ui.push_chrome(frame_of(Rc::default()));
    h.run(0.5);
    // Not known yet: nothing to ring.
    h.ui.mark(Some(Mark::Combo));
    h.frame(0.016);
    assert_eq!(h.ui.mark_bounds(), None);
    spots.set(PlaySpots {
        combo: [0.4, 0.5, 0.6, 0.6],
        life: [0.3, 0.03, 0.7, 0.04],
        ..PlaySpots::default()
    });
    h.frame(0.016);
    let (scale, at, _, _) = geometry([1280.0, 720.0]);
    let want = [
        0.4 * 1280.0 * scale + at[0],
        0.5 * 720.0 * scale + at[1],
        0.6 * 1280.0 * scale + at[0],
        0.6 * 720.0 * scale + at[1],
    ];
    let ringed = h.ui.mark_bounds().expect("the combo");
    assert!(close(ringed, want), "{ringed:?} {want:?}");
    h.ui.mark(Some(Mark::Life));
    h.frame(0.016);
    assert!(h.ui.mark_bounds().is_some());
    // What the picture does not show is not ringed.
    h.ui.mark(Some(Mark::Grade));
    h.frame(0.016);
    assert_eq!(h.ui.mark_bounds(), None);
}

#[test]
fn the_progress_fills_until_the_scene_ends() {
    let log: Log = Rc::default();
    let mut h = Harness::new([1280.0, 720.0], [1280.0, 720.0], 1.0, TextMode::Own);
    h.push(Shown { log: log.clone() });
    h.ui.push_chrome(frame_of(log));
    h.run(0.5);
    h.ui.broadcast(Message::new(UNTIL).with("Seconds", Param::Num(2.0)));
    let width = |h: &Harness| {
        let b = h.ui.bounds("Stage.Progress").unwrap_or([0.0; 4]);
        b[2] - b[0]
    };
    h.run(1.0);
    let half = width(&h);
    let track = h.ui.bounds("Stage.Track").unwrap();
    let full = track[2] - track[0];
    assert!((half / full - 0.5).abs() < 0.05, "{half} of {full}");
    h.run(1.5);
    assert!((width(&h) - full).abs() < 0.5);
}

#[test]
fn the_mark_rings_a_spot_once_its_screen_is_in() {
    let log: Log = Rc::default();
    let mut h = Harness::new([1280.0, 720.0], [1280.0, 720.0], 1.0, TextMode::Own);
    h.push(Shown { log: log.clone() });
    h.ui.mark(Some(Mark::Score));
    h.run(0.3);
    let part = h.ui.bounds_of("Thing", "Part").unwrap();
    assert!(close(h.ui.mark_bounds().unwrap(), part));
    // The whole of it where no part is named; nothing where the screen
    // has no such thing.
    h.ui.mark(Some(Mark::Grade));
    assert!(close(
        h.ui.mark_bounds().unwrap(),
        h.ui.bounds("Thing").unwrap()
    ));
    h.ui.mark(Some(Mark::Lamp));
    assert_eq!(h.ui.mark_bounds(), None);
    // Drawn: the ring comes in, then stays.
    h.ui.mark(Some(Mark::Score));
    let red = [1.0, 0.23, 0.19, 1.0];
    h.frame(0.016);
    assert!(h.ui.needs_redraw(), "the ring's entrance");
    h.run(0.5);
    assert_eq!(h.quads_of(red).len(), 1);
    // On a stage, where the picture puts it.
    h.ui.push_chrome(frame_of(log));
    h.run(0.5);
    let part = h.ui.bounds_of("Thing", "Part").unwrap();
    assert!(close(h.ui.mark_bounds().unwrap(), part));
    assert!(part[2] - part[0] < 40.0);
    h.ui.mark(None);
    h.frame(0.016);
    assert!(h.quads_of(red).is_empty());
}
