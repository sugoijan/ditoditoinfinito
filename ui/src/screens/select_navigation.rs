//! Song-select arrow gestures calibrated with the local timing lab.
//! Seconds throughout. Physical releases distinguish taps from holds;
//! opposite pairs stay provisional so Up Up Down Down never navigates.

use crate::nav::{MenuAction, Source};

pub const TAP_WINDOW: f64 = 0.260;
pub const AFTER_QUIET: f64 = 0.180;
pub const BEFORE_QUIET: f64 = 0.400;
pub const PREVIEW: f64 = 0.20;
pub const SETTLE: f64 = 0.160;
const HOLD_DELAY: f64 = 0.275;
const BASE_RATE: f64 = 8.0;
const MAX_RATE: f64 = 24.0;
const RAMP: f64 = 1.2;

fn direction(a: MenuAction) -> i32 {
    if matches!(a, MenuAction::Up | MenuAction::Left) {
        -1
    } else {
        1
    }
}
fn same_device(a: &Source, b: &Source) -> bool {
    match (a, b) {
        (Source::Key(_), Source::Key(_)) => true,
        (Source::Pad(a, _), Source::Pad(b, _)) => a == b,
        _ => false,
    }
}
#[derive(Clone, Debug)]
struct Tap {
    action: MenuAction,
    source: Source,
    at: f64,
    released: Option<f64>,
}
#[derive(Debug)]
struct Pending {
    first: Tap,
    second: Option<Tap>,
    pairs: Vec<Tap>,
}
#[derive(Debug)]
struct Held {
    tap: Tap,
    next: f64,
    repeated: bool,
}
#[derive(Debug)]
pub(super) enum Event {
    Preview(MenuAction),
    Freeze,
    Move(Vec<MenuAction>, bool),
    Charts(Vec<i32>),
    Stop,
}
#[derive(Debug)]
pub(super) struct Navigation {
    pending: Option<Pending>,
    held: Vec<Held>,
    browsing: bool,
    difficulty: Option<Source>,
    last: f64,
}
impl Default for Navigation {
    fn default() -> Self {
        Self {
            pending: None,
            held: Vec::new(),
            browsing: false,
            difficulty: None,
            last: f64::NEG_INFINITY,
        }
    }
}
impl Navigation {
    pub fn holding(&self) -> bool {
        !self.held.is_empty()
    }
    pub fn pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn animating(&self) -> bool {
        self.pending() || self.holding() || self.browsing
    }
    pub fn cancel(&mut self) {
        *self = Self::default();
    }
    fn take(&mut self) -> Vec<Tap> {
        self.pending.take().map_or_else(Vec::new, |p| {
            let mut taps = p.pairs;
            taps.push(p.first);
            taps.extend(p.second);
            taps
        })
    }
    fn moves(&mut self, taps: Vec<Tap>, out: &mut Vec<Event>) {
        if taps.is_empty() {
            return;
        }
        self.browsing = true;
        self.difficulty = None;
        let pad = matches!(taps.last().unwrap().source, Source::Pad(..));
        out.push(Event::Move(
            taps.into_iter().map(|t| t.action).collect(),
            pad,
        ));
    }
    fn charts(&mut self, out: &mut Vec<Event>) {
        let p = self.pending.take().unwrap();
        self.difficulty = Some(p.first.source.clone());
        self.browsing = false;
        let mut changes: Vec<_> = p
            .pairs
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| direction(pair[0].action))
            .collect();
        changes.push(direction(p.first.action));
        out.push(Event::Charts(changes));
    }
    pub fn finish(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        if self.pending.as_ref().is_some_and(|p| p.second.is_some()) {
            self.charts(&mut out);
        } else {
            let taps = self.take();
            self.moves(taps, &mut out);
        }
        out
    }
    pub fn advance(&mut self, now: f64) -> Vec<Event> {
        let mut out = Vec::new();
        let mut repeats = 0;
        for _ in 0..64 {
            let deadline = self.pending.as_ref().map_or(f64::INFINITY, |p| {
                p.second.as_ref().map_or(p.first.at + TAP_WINDOW, |s| {
                    s.released.map_or(f64::INFINITY, |t| t + AFTER_QUIET)
                })
            });
            let repeat = self.held.last().map_or(f64::INFINITY, |h| h.next);
            if deadline.min(repeat) > now + 1e-9 {
                break;
            }
            if deadline <= repeat {
                if self.pending.as_ref().unwrap().second.is_some() {
                    self.charts(&mut out);
                } else {
                    let taps = self.take();
                    self.moves(taps, &mut out);
                }
            } else {
                let taps = self.take();
                self.moves(taps, &mut out);
                let h = self.held.last_mut().unwrap();
                h.repeated = true;
                let tap = h.tap.clone();
                let t = ((repeat - h.tap.at - HOLD_DELAY) / RAMP).clamp(0.0, 1.0);
                let rate = BASE_RATE + (MAX_RATE - BASE_RATE) * t * t * (3.0 - 2.0 * t);
                h.next = repeat + 1.0 / rate;
                repeats += 1;
                if repeats >= 16 {
                    h.next = now + 1.0 / rate;
                }
                self.moves(vec![tap], &mut out);
                self.last = now;
                if repeats >= 16 {
                    break;
                }
            }
        }
        if self.pending.is_none() && self.held.is_empty() && now - self.last + 1e-9 >= BEFORE_QUIET
        {
            self.browsing = false;
        }
        out
    }
    pub fn down(
        &mut self,
        action: MenuAction,
        source: Source,
        now: f64,
        charts: bool,
    ) -> Vec<Event> {
        if self.held.iter().any(|h| h.tap.source == source) {
            return Vec::new();
        }
        let mut out = self.advance(now);
        let isolated = charts
            && !self.browsing
            && self.held.is_empty()
            && (self
                .difficulty
                .as_ref()
                .is_some_and(|s| same_device(s, &source))
                || now - self.last + 1e-9 >= BEFORE_QUIET);
        let tap = Tap {
            action,
            source,
            at: now,
            released: None,
        };
        if let Some(mut p) = self.pending.take() {
            if p.second.is_none()
                && p.first.action == action
                && same_device(&p.first.source, &tap.source)
                && p.first.released.is_some()
                && self.held.is_empty()
                && now - p.first.at < TAP_WINDOW
            {
                p.second = Some(tap.clone());
                self.pending = Some(p);
                out.push(Event::Freeze);
            } else if p.second.as_ref().is_some_and(|s| s.released.is_some())
                && self.held.is_empty()
                && same_device(&p.first.source, &tap.source)
                && direction(p.first.action) != direction(action)
            {
                p.pairs.push(p.first);
                p.pairs.push(p.second.unwrap());
                self.pending = Some(Pending {
                    first: tap.clone(),
                    second: None,
                    pairs: p.pairs,
                });
            } else {
                let mut taps = p.pairs;
                taps.push(p.first);
                taps.extend(p.second);
                taps.push(tap.clone());
                self.moves(taps, &mut out);
            }
        } else if isolated {
            self.pending = Some(Pending {
                first: tap.clone(),
                second: None,
                pairs: Vec::new(),
            });
            out.push(Event::Preview(action));
        } else {
            self.moves(vec![tap.clone()], &mut out);
        }
        self.held.push(Held {
            tap,
            next: now + HOLD_DELAY,
            repeated: false,
        });
        self.last = now;
        out
    }
    pub fn up(&mut self, source: &Source, now: f64) -> Vec<Event> {
        let Some(i) = self.held.iter().position(|h| &h.tap.source == source) else {
            return Vec::new();
        };
        let mut out = self.advance(now);
        let latest = i + 1 == self.held.len();
        let h = self.held.remove(i);
        if let Some(p) = &mut self.pending {
            for tap in std::iter::once(&mut p.first).chain(p.second.as_mut()) {
                if &tap.source == source && tap.released.is_none() {
                    tap.released = Some(now);
                }
            }
        }
        self.last = now;
        if latest {
            if let Some(h) = self.held.last_mut() {
                h.tap.at = now;
                h.next = now + HOLD_DELAY;
                h.repeated = false;
            }
            if h.repeated {
                out.push(Event::Stop);
            }
        }
        out.extend(self.advance(now));
        out
    }
}

#[derive(Clone, Copy, Debug)]
struct Segment {
    from: f64,
    to: f64,
    at: f64,
    duration: f64,
    v0: f64,
    v1: f64,
}
#[derive(Debug)]
pub(super) struct Motion {
    position: f64,
    segments: Vec<Segment>,
    preview: Option<f64>,
}
impl Motion {
    pub fn new(position: f64) -> Self {
        Self {
            position,
            segments: Vec::new(),
            preview: None,
        }
    }
    pub fn sample(&self, now: f64) -> (f64, f64) {
        for s in &self.segments {
            if now < s.at + s.duration {
                let u = ((now - s.at) / s.duration).clamp(0.0, 1.0);
                let d = s.to - s.from;
                let m0 = s.v0 * s.duration;
                let m1 = s.v1 * s.duration;
                let a = 10.0 * d - 6.0 * m0 - 4.0 * m1;
                let b = -15.0 * d + 8.0 * m0 + 7.0 * m1;
                let c = 6.0 * d - 3.0 * m0 - 3.0 * m1;
                return (
                    s.from + m0 * u + a * u.powi(3) + b * u.powi(4) + c * u.powi(5),
                    (m0 + 3.0 * a * u * u + 4.0 * b * u.powi(3) + 5.0 * c * u.powi(4)) / s.duration,
                );
            }
        }
        (self.segments.last().map_or(self.position, |s| s.to), 0.0)
    }
    pub fn animating(&self, now: f64) -> bool {
        self.segments
            .last()
            .is_some_and(|s| now < s.at + s.duration)
    }
    pub fn preview(&mut self, origin: f64, target: f64, now: f64, reduced: bool) {
        let (position, velocity) = self.sample(now);
        let knot = origin + (target - origin) * PREVIEW;
        let a = (knot - position) / TAP_WINDOW;
        let b = (target - knot) / SETTLE;
        let join = if a * b > 0.0 {
            a.signum() * a.abs().min(b.abs())
        } else {
            0.0
        };
        self.preview = Some(target);
        if reduced {
            self.position = knot;
            self.segments.clear();
            return;
        }
        self.segments = vec![
            Segment {
                from: position,
                to: knot,
                at: now,
                duration: TAP_WINDOW,
                v0: if velocity.abs() > 1e-8 { velocity } else { a },
                v1: join,
            },
            Segment {
                from: knot,
                to: target,
                at: now + TAP_WINDOW,
                duration: SETTLE,
                v0: join,
                v1: 0.0,
            },
        ];
    }
    pub fn commit(&mut self, target: f64, now: f64, reduced: bool) {
        if self.preview == Some(target) && !self.segments.is_empty() && !reduced {
            self.preview = None;
        } else {
            self.to(target, now, reduced);
        }
    }
    pub fn to(&mut self, target: f64, now: f64, reduced: bool) {
        let (position, velocity) = self.sample(now);
        self.preview = None;
        if reduced {
            self.stop(now, Some(target));
            return;
        }
        let mut duration: f64 = 0.120;
        let d = target - position;
        if d * velocity > 0.0 {
            duration = duration.min(2.0 * (d / velocity).abs());
        }
        if d.abs() < 1e-8 && velocity.abs() < 1e-8 {
            self.stop(now, Some(target));
            return;
        }
        self.segments = vec![Segment {
            from: position,
            to: target,
            at: now,
            duration: duration.max(0.001),
            v0: velocity,
            v1: 0.0,
        }];
    }
    pub fn stop(&mut self, now: f64, position: Option<f64>) {
        self.position = position.unwrap_or_else(|| self.sample(now).0);
        self.segments.clear();
        self.preview = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use MenuAction::{Down, Left, Right, Up};
    #[derive(Default)]
    struct Run {
        nav: Navigation,
        events: Vec<Event>,
    }
    impl Run {
        fn tap(&mut self, action: MenuAction, at: f64) {
            let source = Source::Key(format!("{action:?}"));
            self.events
                .extend(self.nav.down(action, source.clone(), at, true));
            self.events.extend(self.nav.up(&source, at + 0.02));
        }
        fn advance(&mut self, t: f64) {
            self.events.extend(self.nav.advance(t));
        }
        fn steps(&self) -> usize {
            self.events
                .iter()
                .map(|e| {
                    if let Event::Move(a, _) = e {
                        a.len()
                    } else {
                        0
                    }
                })
                .sum()
        }
        fn charts(&self) -> Vec<i32> {
            self.events
                .iter()
                .filter_map(|e| {
                    if let Event::Charts(c) = e {
                        Some(c.as_slice())
                    } else {
                        None
                    }
                })
                .flatten()
                .copied()
                .collect()
        }
    }
    #[test]
    fn singles_preview_without_committing_and_pairs_wait_for_release_and_quiet() {
        let mut r = Run::default();
        r.tap(Down, 0.0);
        r.advance(0.259);
        assert_eq!(r.steps(), 0);
        assert!(matches!(r.events[0], Event::Preview(Down)));
        r.advance(0.260);
        assert_eq!(r.steps(), 1);
        let mut r = Run::default();
        r.tap(Down, 0.0);
        r.tap(Down, 0.1);
        r.advance(0.299);
        assert!(r.charts().is_empty());
        r.advance(0.3);
        assert_eq!(r.charts(), [1]);
        assert_eq!(r.steps(), 0);
    }
    #[test]
    fn same_arrow_runs_and_mixed_directions_navigate_immediately() {
        let mut r = Run::default();
        for (i, a) in [Down, Down, Down, Down].into_iter().enumerate() {
            r.tap(a, i as f64 * 0.07);
        }
        assert_eq!(r.steps(), 4);
        assert!(r.charts().is_empty());
        let mut r = Run::default();
        r.tap(Down, 0.0);
        r.tap(Right, 0.1);
        assert_eq!(r.steps(), 2);
    }
    #[test]
    fn opposite_pairs_never_emit_a_song_move_even_before_first_pair_confirms() {
        for (a, b) in [(Up, Down), (Down, Up), (Left, Right), (Right, Left)] {
            let mut r = Run::default();
            r.tap(a, 0.0);
            r.tap(a, 0.07);
            r.tap(b, 0.14);
            assert_eq!(r.steps(), 0);
            assert_eq!(
                r.events
                    .iter()
                    .filter(|e| matches!(e, Event::Preview(_)))
                    .count(),
                1,
                "the opposite third tap must not restart the frozen preview"
            );
            r.tap(b, 0.21);
            r.advance(0.41);
            assert_eq!(r.steps(), 0);
            assert_eq!(r.charts(), [direction(a), direction(b)]);
        }
    }
    #[test]
    fn confirmed_pair_rearms_immediately_and_alternating_pairs_chain() {
        let mut r = Run::default();
        r.tap(Up, 0.0);
        r.tap(Up, 0.1);
        r.advance(0.3);
        r.tap(Down, 0.31);
        r.tap(Down, 0.4);
        r.advance(0.6);
        assert_eq!(r.charts(), [-1, 1]);
        assert_eq!(r.steps(), 0);
        let mut r = Run::default();
        for (i, a) in [Up, Up, Down, Down, Up, Up].into_iter().enumerate() {
            r.tap(a, i as f64 * 0.06);
        }
        r.advance(0.5);
        assert_eq!(r.charts(), [-1, 1, -1]);
        assert_eq!(r.steps(), 0);
    }
    #[test]
    fn missing_or_wrong_fourth_tap_becomes_navigation() {
        let mut r = Run::default();
        r.tap(Up, 0.0);
        r.tap(Up, 0.07);
        r.tap(Down, 0.14);
        r.advance(0.399);
        assert_eq!(r.steps(), 0);
        r.advance(0.4);
        assert_eq!(r.steps(), 3);
        assert!(r.charts().is_empty());
        let mut r = Run::default();
        r.tap(Up, 0.0);
        r.tap(Up, 0.07);
        r.tap(Down, 0.14);
        r.tap(Up, 0.21);
        assert_eq!(r.steps(), 4);
        assert!(r.charts().is_empty());
    }
    #[test]
    fn browsing_requires_quiet_but_does_not_reinterpret_committed_moves() {
        let mut r = Run::default();
        r.tap(Up, 0.0);
        r.advance(0.26);
        r.tap(Up, 0.3);
        r.tap(Down, 0.37);
        r.tap(Down, 0.44);
        r.advance(0.859);
        assert!(r.nav.browsing);
        r.advance(0.86);
        assert!(!r.nav.browsing);
        assert_eq!(r.steps(), 4);
        r.tap(Down, 0.87);
        assert_eq!(r.steps(), 4);
    }
    #[test]
    fn holds_accelerate_to_cap_stop_and_reset_when_reversed() {
        let mut r = Run::default();
        let source = Source::Key("ArrowDown".into());
        r.events.extend(r.nav.down(Down, source.clone(), 0.0, true));
        for i in 0..400 {
            r.advance(i as f64 * 0.005);
        }
        let steps = r.steps();
        assert!(steps > 25);
        assert!(r.charts().is_empty());
        r.events.extend(r.nav.up(&source, 2.0));
        let stopped = r.steps();
        r.advance(4.0);
        assert_eq!(r.steps(), stopped);
        assert!(stopped >= steps);
        assert!(matches!(r.events.last(), Some(Event::Stop)));
        let mut r = Run::default();
        r.events.extend(r.nav.down(Down, source, 0.0, true));
        r.advance(1.8);
        let other = Source::Key("ArrowUp".into());
        r.events.extend(r.nav.down(Up, other, 1.81, true));
        let n = r.steps();
        r.advance(2.08);
        assert_eq!(r.steps(), n);
        r.advance(2.085);
        assert_eq!(r.steps(), n + 1);
    }
    #[test]
    fn a_held_second_press_cannot_change_difficulty() {
        let mut r = Run::default();
        r.tap(Down, 0.0);
        r.events
            .extend(r.nav.down(Down, Source::Key("Down".into()), 0.1, true));
        r.advance(0.374);
        assert_eq!(r.steps(), 0);
        r.advance(0.375);
        assert_eq!(r.steps(), 3);
        assert!(r.charts().is_empty());
    }
    #[test]
    fn devices_stay_separate_but_opposite_pad_actions_can_pair() {
        let mut r = Run::default();
        r.tap(Down, 0.0);
        r.events
            .extend(r.nav.down(Down, Source::Pad(0, Down), 0.1, true));
        assert_eq!(r.steps(), 2);
        let mut r = Run::default();
        for (i, a) in [Up, Up, Down, Down].into_iter().enumerate() {
            let t = i as f64 * 0.07;
            let src = Source::Pad(3, a);
            r.events.extend(r.nav.down(a, src.clone(), t, true));
            r.events.extend(r.nav.up(&src, t + 0.02));
        }
        r.advance(0.41);
        assert_eq!(r.steps(), 0);
        assert_eq!(r.charts(), [-1, 1]);
        let mut r = Run::default();
        for (i, slot) in [1, 2].into_iter().enumerate() {
            let t = i as f64 * 0.07;
            let src = Source::Pad(slot, Down);
            r.events.extend(r.nav.down(Down, src.clone(), t, true));
            r.events.extend(r.nav.up(&src, t + 0.02));
        }
        assert_eq!(r.steps(), 2);
    }
    #[test]
    fn focus_loss_and_explicit_start_resolve_provisional_input() {
        let mut r = Run::default();
        r.tap(Down, 0.0);
        r.nav.cancel();
        r.advance(1.0);
        assert_eq!(r.steps(), 0);
        r.tap(Down, 1.0);
        r.events.extend(r.nav.finish());
        assert_eq!(r.steps(), 1);
        let mut r = Run::default();
        r.tap(Down, 0.0);
        r.tap(Down, 0.07);
        r.events.extend(r.nav.finish());
        assert_eq!(r.charts(), [1]);
        assert_eq!(r.steps(), 0);
    }
    #[test]
    fn motion_has_no_pause_or_easing_restart_at_commit() {
        for by in [-1.0, 1.0] {
            let mut m = Motion::new(0.0);
            m.preview(0.0, by * 66.0, 0.0, false);
            let before = m.sample(TAP_WINDOW - 1e-6);
            let at = m.sample(TAP_WINDOW);
            let after = m.sample(TAP_WINDOW + 1e-6);
            assert!((at.0 - by * 66.0 * PREVIEW).abs() < 1e-8);
            assert!(at.1 * by > 0.0);
            assert!((before.1 - after.1).abs() < 0.001);
            let start = m.segments[0].at;
            m.commit(by * 66.0, TAP_WINDOW + 0.01, false);
            assert_eq!(m.segments[0].at, start);
            let mut last = 0.0;
            for i in 0..=420 {
                let p = m.sample(i as f64 / 1000.0).0;
                assert!((p - last) * by >= -1e-8);
                last = p;
            }
            assert!((m.sample(TAP_WINDOW + SETTLE).0 - by * 66.0).abs() < 1e-8);
        }
    }
}
