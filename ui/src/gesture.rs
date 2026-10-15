//! Turning one pointer's events into gestures: a tap, a long press, a
//! drag, a flick.
//!
//! Thresholds are in CSS (logical) pixels and seconds, so a gesture feels
//! the same at any device pixel ratio and any virtual screen size; what
//! [`crate::Ui`] hands screens is converted to virtual pixels. The
//! numbers are ours, in the range platforms use (Android's touch slop is
//! 8 dp and its long press 400–500 ms; iOS's long press 500 ms):
//!
//! - a press stays a tap until the pointer moves more than [`SLOP_TOUCH`]
//!   (touch and pen) or [`SLOP_MOUSE`] from where it went down;
//! - a touch or pen held still for [`LONG_PRESS`] is a long press (fired
//!   from [`Tracker::tick`], so never a user gesture: P2); a mouse has no
//!   long press;
//! - past the slop it is a drag, whose deltas count from the press (the
//!   slop is not lost);
//! - a drag released faster than [`FLICK_SPEED`] is a flick, carrying its
//!   velocity, measured over the last [`VELOCITY_WINDOW`] of movement;
//!   slower, it just ends.
//!
//! Taps fire on release whatever the pointer, as a `click` does: a touch
//! is only a user gesture when it lifts.

use std::collections::VecDeque;

use crate::event::PointerKind;

/// How far a touch or a pen may wander and still tap, CSS pixels.
pub const SLOP_TOUCH: f32 = 10.0;
/// How far a mouse may wander and still click, CSS pixels.
pub const SLOP_MOUSE: f32 = 4.0;
/// How long a touch or pen held still is a long press, seconds.
pub const LONG_PRESS: f32 = 0.5;
/// How fast a released drag must move to flick, CSS pixels a second.
pub const FLICK_SPEED: f32 = 300.0;
/// The movement a release's velocity is measured over, milliseconds.
pub const VELOCITY_WINDOW: f64 = 100.0;
/// The most a frame counts toward a long press, seconds: the first frame
/// after the shell was idle (or stalled) spans the gap, which must not
/// fire one.
pub const MAX_TICK: f32 = 0.1;

/// What a pointer did. Positions and deltas are in the units the
/// recogniser was fed (CSS pixels); [`crate::Ui`] converts them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gesture {
    /// The pointer went down on the target (a mouse's primary button).
    Press,
    /// Released near where it went down, before a long press.
    Tap,
    /// Held still for [`LONG_PRESS`] (touch and pen). The release after
    /// it is a [`Gesture::Release`], unless it drags first.
    LongPress,
    /// Moved past the slop: a drag begins. A [`Gesture::Drag`] follows at
    /// once with the movement so far.
    DragStart,
    /// Moved by `delta` since the last drag event.
    Drag { delta: [f32; 2] },
    /// A drag released slower than [`FLICK_SPEED`].
    DragEnd { velocity: [f32; 2] },
    /// A drag released faster: lists start their inertia.
    Flick { velocity: [f32; 2] },
    /// Released after a long press, with no drag.
    Release,
    /// The pointer was taken away (cancelled, focus lost): undo the
    /// press's look, do nothing.
    Cancel,
}

impl Gesture {
    /// The gesture with its lengths times `k`.
    pub fn scaled(self, k: f32) -> Gesture {
        let s = |v: [f32; 2]| [v[0] * k, v[1] * k];
        match self {
            Gesture::Drag { delta } => Gesture::Drag { delta: s(delta) },
            Gesture::DragEnd { velocity } => Gesture::DragEnd {
                velocity: s(velocity),
            },
            Gesture::Flick { velocity } => Gesture::Flick {
                velocity: s(velocity),
            },
            g => g,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Pressed,
    LongPressed,
    Dragging,
    Done,
}

/// One pointer's gesture, from its press to its release.
#[derive(Clone, Debug)]
pub struct Tracker {
    kind: PointerKind,
    start: [f32; 2],
    last: [f32; 2],
    held: f32,
    state: State,
    long_pressed: bool,
    /// Recent positions and their host times (ms).
    samples: VecDeque<(f64, [f32; 2])>,
}

impl Tracker {
    /// A press at `pos` (CSS pixels) at host time `time` (ms).
    pub fn press(kind: PointerKind, pos: [f32; 2], time: f64) -> Tracker {
        Tracker {
            kind,
            start: pos,
            last: pos,
            held: 0.0,
            state: State::Pressed,
            long_pressed: false,
            samples: VecDeque::from([(time, pos)]),
        }
    }

    pub fn kind(&self) -> PointerKind {
        self.kind
    }

    fn slop(&self) -> f32 {
        match self.kind {
            PointerKind::Mouse => SLOP_MOUSE,
            PointerKind::Touch | PointerKind::Pen => SLOP_TOUCH,
        }
    }

    fn sample(&mut self, time: f64, pos: [f32; 2]) {
        self.samples.push_back((time, pos));
        while self
            .samples
            .front()
            .is_some_and(|(t, _)| *t < time - VELOCITY_WINDOW * 2.0)
        {
            self.samples.pop_front();
        }
    }

    /// The pointer moved.
    pub fn moved(&mut self, pos: [f32; 2], time: f64, out: &mut Vec<Gesture>) {
        self.sample(time, pos);
        match self.state {
            State::Pressed | State::LongPressed => {
                let d = [pos[0] - self.start[0], pos[1] - self.start[1]];
                if d[0].hypot(d[1]) > self.slop() {
                    self.state = State::Dragging;
                    self.last = pos;
                    out.push(Gesture::DragStart);
                    out.push(Gesture::Drag { delta: d });
                }
            }
            State::Dragging => {
                let delta = [pos[0] - self.last[0], pos[1] - self.last[1]];
                self.last = pos;
                if delta != [0.0; 2] {
                    out.push(Gesture::Drag { delta });
                }
            }
            State::Done => {}
        }
    }

    /// The pointer was released.
    pub fn released(&mut self, pos: [f32; 2], time: f64, out: &mut Vec<Gesture>) {
        self.moved(pos, time, out);
        let state = std::mem::replace(&mut self.state, State::Done);
        match state {
            State::Pressed => out.push(Gesture::Tap),
            State::LongPressed => out.push(Gesture::Release),
            State::Dragging => {
                let velocity = self.velocity(time);
                if velocity[0].hypot(velocity[1]) > FLICK_SPEED {
                    out.push(Gesture::Flick { velocity });
                } else {
                    out.push(Gesture::DragEnd { velocity });
                }
            }
            State::Done => {}
        }
    }

    /// The pointer was taken away.
    pub fn cancel(&mut self, out: &mut Vec<Gesture>) {
        if self.state != State::Done {
            self.state = State::Done;
            out.push(Gesture::Cancel);
        }
    }

    /// `dt` seconds passed with the pointer down.
    /// `dt` is clamped to [`MAX_TICK`].
    pub fn tick(&mut self, dt: f32, out: &mut Vec<Gesture>) {
        self.held += dt.clamp(0.0, MAX_TICK);
        if self.state == State::Pressed
            && self.kind != PointerKind::Mouse
            && self.held >= LONG_PRESS
        {
            self.state = State::LongPressed;
            self.long_pressed = true;
            out.push(Gesture::LongPress);
        }
    }

    /// Whether a long press may still fire (frames must keep coming).
    pub fn waiting(&self) -> bool {
        self.state == State::Pressed && self.kind != PointerKind::Mouse
    }

    /// Whether a long press fired: its lift is no user gesture (P2).
    pub fn long_pressed(&self) -> bool {
        self.long_pressed
    }

    pub fn done(&self) -> bool {
        self.state == State::Done
    }

    /// Pixels a second over the samples in the window before `now`: from
    /// the oldest to the newest (a pointer that stopped before lifting
    /// has none moving, so no velocity).
    fn velocity(&self, now: f64) -> [f32; 2] {
        let recent: Vec<&(f64, [f32; 2])> = self
            .samples
            .iter()
            .filter(|(t, _)| *t >= now - VELOCITY_WINDOW)
            .collect();
        let (Some(first), Some(last)) = (recent.first(), recent.last()) else {
            return [0.0; 2];
        };
        let dt = ((last.0 - first.0) / 1000.0) as f32;
        if dt <= 0.0 {
            return [0.0; 2];
        }
        [(last.1[0] - first.1[0]) / dt, (last.1[1] - first.1[1]) / dt]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(kind: PointerKind, path: &[(f64, [f32; 2])]) -> Vec<Gesture> {
        let mut out = Vec::new();
        let (t0, p0) = path[0];
        let mut t = Tracker::press(kind, p0, t0);
        for &(time, pos) in &path[1..path.len() - 1] {
            t.moved(pos, time, &mut out);
        }
        let (time, pos) = path[path.len() - 1];
        t.released(pos, time, &mut out);
        out
    }

    #[test]
    fn small_moves_still_tap() {
        let g = run(
            PointerKind::Touch,
            &[
                (0.0, [10.0, 10.0]),
                (50.0, [16.0, 14.0]),
                (90.0, [17.0, 15.0]),
            ],
        );
        assert_eq!(g, vec![Gesture::Tap]);
        // A mouse's slop is smaller.
        let g = run(
            PointerKind::Mouse,
            &[
                (0.0, [10.0, 10.0]),
                (50.0, [16.0, 14.0]),
                (90.0, [16.0, 14.0]),
            ],
        );
        assert_eq!(g[0], Gesture::DragStart);
        assert_eq!(g[1], Gesture::Drag { delta: [6.0, 4.0] });
        assert!(matches!(g.last(), Some(Gesture::DragEnd { .. })));
    }

    #[test]
    fn fast_drags_flick_slow_ones_end() {
        // 200 px in 100 ms: 2,000 px/s.
        let g = run(
            PointerKind::Touch,
            &[
                (0.0, [0.0, 0.0]),
                (20.0, [0.0, 40.0]),
                (60.0, [0.0, 120.0]),
                (100.0, [0.0, 200.0]),
            ],
        );
        let Some(Gesture::Flick { velocity }) = g.last() else {
            panic!("{g:?}");
        };
        assert!((velocity[1] - 2000.0).abs() < 1.0, "{velocity:?}");
        // The deltas add up to the whole movement.
        let total: f32 = g
            .iter()
            .filter_map(|g| match g {
                Gesture::Drag { delta } => Some(delta[1]),
                _ => None,
            })
            .sum();
        assert_eq!(total, 200.0);
        // The same drag, then held still for 300 ms before lifting.
        let g = run(
            PointerKind::Touch,
            &[
                (0.0, [0.0, 0.0]),
                (100.0, [0.0, 200.0]),
                (400.0, [0.0, 200.0]),
            ],
        );
        assert_eq!(
            g.last(),
            Some(&Gesture::DragEnd {
                velocity: [0.0, 0.0]
            })
        );
    }

    #[test]
    fn long_presses_fire_from_ticks_on_touch_only() {
        let mut out = Vec::new();
        let mut t = Tracker::press(PointerKind::Touch, [0.0; 2], 0.0);
        // One long frame counts for at most `MAX_TICK`.
        t.tick(5.0, &mut out);
        assert!(out.is_empty() && t.waiting());
        for _ in 0..4 {
            t.tick(0.1, &mut out);
        }
        assert_eq!(out, vec![Gesture::LongPress]);
        assert!(t.long_pressed());
        t.released([1.0, 1.0], 600.0, &mut out);
        assert_eq!(out, vec![Gesture::LongPress, Gesture::Release]);

        let mut out = Vec::new();
        let mut t = Tracker::press(PointerKind::Mouse, [0.0; 2], 0.0);
        t.tick(2.0, &mut out);
        t.released([0.0; 2], 2000.0, &mut out);
        assert_eq!(out, vec![Gesture::Tap]);
    }

    #[test]
    fn cancel_once() {
        let mut out = Vec::new();
        let mut t = Tracker::press(PointerKind::Pen, [0.0; 2], 0.0);
        t.cancel(&mut out);
        t.cancel(&mut out);
        t.released([0.0; 2], 10.0, &mut out);
        assert_eq!(out, vec![Gesture::Cancel]);
    }
}
