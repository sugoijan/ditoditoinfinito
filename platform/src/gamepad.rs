//! Gamepad edge detection, shared by every shell that polls gamepads.
//!
//! Gamepads have no events: a shell polls the device state and hands each
//! reading to a [`PadTracker`] as a [`PadSnapshot`]. The tracker turns state
//! changes into [`RawInput`] press/release edges, decodes analog axes and
//! hat switches into digital controls, and picks the timestamp of each edge.
//!
//! Controls, as stored in bindings ([`Control`]'s string form):
//! - `button:N`: a button, pressed when the browser says so.
//! - `axis:N+` / `axis:N-`: one direction of an axis (analog stick, or a pad
//!   that reports its arrows on axes), with hysteresis.
//! - `hat:N:up|right|down|left`: a hat switch reported as a single axis. The
//!   eight directions are spread over −1…1 starting from up = −1 and going
//!   clockwise; neutral is a value outside ±1 (e.g. 1.2857, the HID "null"
//!   position 8 scaled the same way). An axis is treated as a hat once it
//!   has been seen outside ±1. A diagonal presses two directions.
//!
//! Timestamps: an edge is stamped with the pad's own `timestamp` (the time
//! its data was last updated, on the host timeline) when that value moved
//! forward since the previous poll and is plausible for this poll, else with
//! the poll time. Which one was used is counted in [`PadStats`].

use core::fmt;
use core::str::FromStr;

use crate::{DeviceId, HostTime, RawInput};

/// Axis magnitude that presses an axis direction.
pub const AXIS_PRESS: f64 = 0.6;
/// Axis magnitude below which a pressed axis direction releases.
pub const AXIS_RELEASE: f64 = 0.4;
/// Axis values beyond this magnitude only occur on hat switches at rest.
const HAT_NEUTRAL_MIN: f64 = 1.05;
/// How far before the previous poll a pad timestamp may lie and still be
/// trusted: the browser samples the device on its own schedule, so data can
/// become visible a few milliseconds after it was read.
const TIMESTAMP_SLACK: f64 = 0.008;
/// Smoothing of the report interval estimate.
const REPORT_EMA: f64 = 0.05;

/// One reading of a gamepad.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PadSnapshot {
    /// Slot of the pad in the browser's list (`Gamepad.index`).
    pub index: u32,
    /// `Gamepad.id`: a description of the device, the binding key.
    pub id: String,
    /// `Gamepad.mapping == "standard"`.
    pub standard: bool,
    /// `Gamepad.timestamp` on the host timeline.
    pub timestamp: HostTime,
    pub buttons: Vec<bool>,
    pub axes: Vec<f64>,
}

/// A direction of a hat switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HatDir {
    Up,
    Right,
    Down,
    Left,
}

impl HatDir {
    pub const ALL: [HatDir; 4] = [HatDir::Up, HatDir::Right, HatDir::Down, HatDir::Left];

    fn name(self) -> &'static str {
        match self {
            HatDir::Up => "up",
            HatDir::Right => "right",
            HatDir::Down => "down",
            HatDir::Left => "left",
        }
    }

    fn opposite(self) -> HatDir {
        match self {
            HatDir::Up => HatDir::Down,
            HatDir::Right => HatDir::Left,
            HatDir::Down => HatDir::Up,
            HatDir::Left => HatDir::Right,
        }
    }
}

/// A digital control on a gamepad.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Control {
    Button(u16),
    Axis { index: u16, positive: bool },
    Hat { index: u16, dir: HatDir },
}

impl Control {
    /// Whether two controls can never be pressed at the same time because
    /// they are opposite ends of one axis or hat (a hardware limit: an
    /// axis carries one value).
    pub fn exclusive_with(&self, other: &Control) -> bool {
        match (self, other) {
            (
                Control::Axis {
                    index: a,
                    positive: pa,
                },
                Control::Axis {
                    index: b,
                    positive: pb,
                },
            ) => a == b && pa != pb,
            (Control::Hat { index: a, dir: da }, Control::Hat { index: b, dir: db }) => {
                a == b && da.opposite() == *db
            }
            _ => false,
        }
    }

    /// Short human-readable name, e.g. "button 3", "axis 1 −", "hat 9 up".
    pub fn label(&self) -> String {
        match self {
            Control::Button(n) => format!("button {n}"),
            Control::Axis { index, positive } => {
                format!("axis {index} {}", if *positive { "+" } else { "−" })
            }
            Control::Hat { index, dir } => format!("hat {index} {}", dir.name()),
        }
    }
}

impl fmt::Display for Control {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Control::Button(n) => write!(f, "button:{n}"),
            Control::Axis { index, positive } => {
                write!(f, "axis:{index}{}", if *positive { '+' } else { '-' })
            }
            Control::Hat { index, dir } => write!(f, "hat:{index}:{}", dir.name()),
        }
    }
}

impl FromStr for Control {
    type Err = ();

    fn from_str(s: &str) -> Result<Control, ()> {
        let (kind, rest) = s.split_once(':').ok_or(())?;
        match kind {
            "button" => rest.parse().map(Control::Button).map_err(|_| ()),
            "axis" => {
                let positive = match rest.chars().last() {
                    Some('+') => true,
                    Some('-') => false,
                    _ => return Err(()),
                };
                let index = rest[..rest.len() - 1].parse().map_err(|_| ())?;
                Ok(Control::Axis { index, positive })
            }
            "hat" => {
                let (index, dir) = rest.split_once(':').ok_or(())?;
                let dir = HatDir::ALL
                    .into_iter()
                    .find(|d| d.name() == dir)
                    .ok_or(())?;
                Ok(Control::Hat {
                    index: index.parse().map_err(|_| ())?,
                    dir,
                })
            }
            _ => Err(()),
        }
    }
}

/// Directions pressed by a hat switch reading, or `None` at rest.
pub fn hat_directions(value: f64) -> Option<[bool; 4]> {
    if !value.is_finite() || value.abs() > HAT_NEUTRAL_MIN {
        return None;
    }
    // 0 = up, 1 = up-right, … 7 = up-left.
    let pos = (((value.clamp(-1.0, 1.0) + 1.0) * 3.5).round() as i32).rem_euclid(8);
    let up = matches!(pos, 7 | 0 | 1);
    let right = matches!(pos, 1..=3);
    let down = matches!(pos, 3..=5);
    let left = matches!(pos, 5..=7);
    Some([up, right, down, left])
}

/// How a pad's edges were timestamped, and how often it reports.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PadStats {
    /// Polls that saw a change stamped with the pad's own timestamp.
    pub stamped_by_pad: u32,
    /// Polls that saw a change stamped with the poll time.
    pub stamped_by_poll: u32,
    /// Smoothed interval between changes of the pad's timestamp, seconds.
    pub report_interval: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct AxisState {
    hat: bool,
    pos: bool,
    neg: bool,
    dirs: [bool; 4],
}

/// Edge detector for one gamepad slot.
#[derive(Clone, Debug)]
pub struct PadTracker {
    device: DeviceId,
    buttons: Vec<bool>,
    axes: Vec<AxisState>,
    last_timestamp: Option<HostTime>,
    last_poll: Option<HostTime>,
    last_edge: HostTime,
    stats: PadStats,
}

impl PadTracker {
    pub fn new(id: &str) -> PadTracker {
        PadTracker {
            device: DeviceId::Gamepad(id.to_string()),
            buttons: Vec::new(),
            axes: Vec::new(),
            last_timestamp: None,
            last_poll: None,
            last_edge: HostTime(f64::NEG_INFINITY),
            stats: PadStats::default(),
        }
    }

    pub fn device(&self) -> &DeviceId {
        &self.device
    }

    pub fn stats(&self) -> PadStats {
        self.stats
    }

    /// Controls currently held.
    pub fn active(&self) -> Vec<Control> {
        let mut out: Vec<Control> = self
            .buttons
            .iter()
            .enumerate()
            .filter(|(_, p)| **p)
            .map(|(i, _)| Control::Button(i as u16))
            .collect();
        for (i, a) in self.axes.iter().enumerate() {
            let index = i as u16;
            if a.hat {
                for (d, on) in HatDir::ALL.iter().zip(a.dirs) {
                    if on {
                        out.push(Control::Hat { index, dir: *d });
                    }
                }
            } else {
                if a.pos {
                    out.push(Control::Axis {
                        index,
                        positive: true,
                    });
                }
                if a.neg {
                    out.push(Control::Axis {
                        index,
                        positive: false,
                    });
                }
            }
        }
        out
    }

    /// Feeds one reading taken at `now`; appends the edges it implies.
    ///
    /// The first reading sets the resting state of the axes without edges
    /// (a trigger resting at −1 is not a press), but buttons already held
    /// are reported: browsers only reveal a pad once a button is pressed.
    pub fn update(&mut self, snap: &PadSnapshot, now: HostTime, out: &mut Vec<RawInput>) {
        let first = self.last_poll.is_none();
        let mut changes: Vec<(Control, bool)> = Vec::new();

        if self.buttons.len() < snap.buttons.len() {
            self.buttons.resize(snap.buttons.len(), false);
        }
        // Buttons missing from a shorter reading count as released.
        for (i, was) in self.buttons.iter_mut().enumerate() {
            let now_pressed = snap.buttons.get(i).copied().unwrap_or(false);
            if now_pressed != *was {
                *was = now_pressed;
                changes.push((Control::Button(i as u16), now_pressed));
            }
        }

        if self.axes.len() < snap.axes.len() {
            self.axes.resize(snap.axes.len(), AxisState::default());
        }
        for (i, (&v, a)) in snap.axes.iter().zip(self.axes.iter_mut()).enumerate() {
            let index = i as u16;
            let mut axis_changes: Vec<(Control, bool)> = Vec::new();
            if !a.hat && v.is_finite() && v.abs() > HAT_NEUTRAL_MIN {
                // Only a hat at rest reads outside ±1: release anything the
                // axis interpretation held and switch over.
                a.hat = true;
                if a.pos {
                    axis_changes.push((
                        Control::Axis {
                            index,
                            positive: true,
                        },
                        false,
                    ));
                }
                if a.neg {
                    axis_changes.push((
                        Control::Axis {
                            index,
                            positive: false,
                        },
                        false,
                    ));
                }
                a.pos = false;
                a.neg = false;
            }
            if a.hat {
                let dirs = hat_directions(v).unwrap_or([false; 4]);
                for (k, d) in HatDir::ALL.iter().enumerate() {
                    if dirs[k] != a.dirs[k] {
                        axis_changes.push((Control::Hat { index, dir: *d }, dirs[k]));
                    }
                }
                a.dirs = dirs;
            } else if v.is_finite() {
                let pos = if a.pos {
                    v > AXIS_RELEASE
                } else {
                    v >= AXIS_PRESS
                };
                let neg = if a.neg {
                    v < -AXIS_RELEASE
                } else {
                    v <= -AXIS_PRESS
                };
                if pos != a.pos {
                    axis_changes.push((
                        Control::Axis {
                            index,
                            positive: true,
                        },
                        pos,
                    ));
                }
                if neg != a.neg {
                    axis_changes.push((
                        Control::Axis {
                            index,
                            positive: false,
                        },
                        neg,
                    ));
                }
                a.pos = pos;
                a.neg = neg;
            }
            if !first {
                changes.extend(axis_changes);
            }
        }

        let ts = snap.timestamp;
        let moved = self.last_timestamp.is_some_and(|l| ts.0 > l.0);
        if moved && let Some(l) = self.last_timestamp {
            let dt = ts.0 - l.0;
            if dt < 0.1 {
                self.stats.report_interval = Some(match self.stats.report_interval {
                    Some(r) => r + (dt - r) * REPORT_EMA,
                    None => dt,
                });
            }
        }

        if !changes.is_empty() {
            let max_age = self.last_poll.map_or(0.0, |p| (now.0 - p.0).max(0.0)) + TIMESTAMP_SLACK;
            let plausible = moved && ts.0 <= now.0 + 0.0005 && now.0 - ts.0 <= max_age;
            let mut t = if plausible {
                self.stats.stamped_by_pad += 1;
                HostTime(ts.0.min(now.0))
            } else {
                self.stats.stamped_by_poll += 1;
                now
            };
            // Edges of one pad never go back in time.
            if t.0 < self.last_edge.0 {
                t = self.last_edge;
            }
            self.last_edge = t;
            // Releases first: a release and a press in one poll on the same
            // lane must leave it pressed.
            changes.sort_by_key(|(_, pressed)| *pressed);
            for (control, pressed) in changes {
                out.push(RawInput {
                    device: self.device.clone(),
                    control: control.to_string(),
                    pressed,
                    host_time: t,
                });
            }
        }

        if self.last_timestamp.is_none_or(|l| ts.0 > l.0) {
            self.last_timestamp = Some(ts);
        }
        self.last_poll = Some(now);
    }

    /// The pad went away: release everything it held, stamped `now`.
    pub fn release_all(&mut self, now: HostTime, out: &mut Vec<RawInput>) {
        let t = HostTime(now.0.max(self.last_edge.0));
        for control in self.active() {
            out.push(RawInput {
                device: self.device.clone(),
                control: control.to_string(),
                pressed: false,
                host_time: t,
            });
        }
        self.buttons.iter_mut().for_each(|b| *b = false);
        for a in &mut self.axes {
            a.pos = false;
            a.neg = false;
            a.dirs = [false; 4];
        }
    }
}

/// Standard-mapping buttons (W3C Gamepad "standard" layout).
pub mod standard {
    pub const A: u16 = 0;
    pub const B: u16 = 1;
    pub const X: u16 = 2;
    pub const Y: u16 = 3;
    pub const BACK: u16 = 8;
    pub const START: u16 = 9;
    pub const DPAD_UP: u16 = 12;
    pub const DPAD_DOWN: u16 = 13;
    pub const DPAD_LEFT: u16 = 14;
    pub const DPAD_RIGHT: u16 = 15;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(ts: f64, buttons: &[bool], axes: &[f64]) -> PadSnapshot {
        PadSnapshot {
            index: 0,
            id: "pad".into(),
            standard: false,
            timestamp: HostTime(ts),
            buttons: buttons.to_vec(),
            axes: axes.to_vec(),
        }
    }

    fn edges(out: &[RawInput]) -> Vec<(String, bool, f64)> {
        out.iter()
            .map(|r| (r.control.clone(), r.pressed, r.host_time.0))
            .collect()
    }

    #[test]
    fn control_strings_round_trip() {
        for s in [
            "button:0",
            "button:17",
            "axis:1+",
            "axis:0-",
            "hat:9:up",
            "hat:9:left",
        ] {
            let c: Control = s.parse().unwrap();
            assert_eq!(c.to_string(), s);
        }
        for bad in ["button:", "axis:1", "hat:9:north", "key:A", "button:-1"] {
            assert!(bad.parse::<Control>().is_err(), "{bad}");
        }
    }

    #[test]
    fn exclusive_controls() {
        let l: Control = "hat:9:left".parse().unwrap();
        let r: Control = "hat:9:right".parse().unwrap();
        let u: Control = "hat:9:up".parse().unwrap();
        assert!(l.exclusive_with(&r));
        assert!(!l.exclusive_with(&u));
        let p: Control = "axis:0+".parse().unwrap();
        let n: Control = "axis:0-".parse().unwrap();
        let other: Control = "axis:1-".parse().unwrap();
        assert!(p.exclusive_with(&n));
        assert!(!p.exclusive_with(&other));
        assert!(!Control::Button(1).exclusive_with(&Control::Button(1)));
    }

    #[test]
    fn hat_decoding() {
        let step = 2.0 / 7.0;
        assert_eq!(hat_directions(-1.0), Some([true, false, false, false]));
        assert_eq!(
            hat_directions(-1.0 + step),
            Some([true, true, false, false])
        );
        assert_eq!(
            hat_directions(-1.0 + 2.0 * step),
            Some([false, true, false, false])
        );
        assert_eq!(
            hat_directions(-1.0 + 4.0 * step),
            Some([false, false, true, false])
        );
        assert_eq!(
            hat_directions(-1.0 + 5.0 * step),
            Some([false, false, true, true])
        );
        assert_eq!(
            hat_directions(-1.0 + 6.0 * step),
            Some([false, false, false, true])
        );
        assert_eq!(hat_directions(1.0), Some([true, false, false, true]));
        assert_eq!(hat_directions(1.2857), None);
        assert_eq!(hat_directions(3.2857), None);
        assert_eq!(hat_directions(f64::NAN), None);
    }

    #[test]
    fn buttons_edges_with_pad_timestamps() {
        let mut t = PadTracker::new("pad");
        let mut out = Vec::new();
        t.update(
            &snap(0.990, &[false, false], &[]),
            HostTime(1.000),
            &mut out,
        );
        assert!(out.is_empty());
        // Changed at 1.0004 per the pad, seen at the poll at 1.001.
        t.update(
            &snap(1.0004, &[true, false], &[]),
            HostTime(1.001),
            &mut out,
        );
        t.update(
            &snap(1.0004, &[true, false], &[]),
            HostTime(1.002),
            &mut out,
        );
        t.update(
            &snap(1.0025, &[false, true], &[]),
            HostTime(1.003),
            &mut out,
        );
        assert_eq!(
            edges(&out),
            vec![
                ("button:0".into(), true, 1.0004),
                ("button:0".into(), false, 1.0025),
                ("button:1".into(), true, 1.0025),
            ]
        );
        assert_eq!(t.stats().stamped_by_pad, 2);
        assert_eq!(t.stats().stamped_by_poll, 0);
    }

    #[test]
    fn frozen_or_implausible_timestamps_fall_back_to_poll_time() {
        let mut t = PadTracker::new("pad");
        let mut out = Vec::new();
        t.update(&snap(0.0, &[false], &[]), HostTime(1.000), &mut out);
        // Timestamp never moves (a browser that does not update it).
        t.update(&snap(0.0, &[true], &[]), HostTime(1.001), &mut out);
        // Moves, but far in the past: another epoch.
        t.update(&snap(0.5, &[false], &[]), HostTime(1.002), &mut out);
        // Moves into the future.
        t.update(&snap(2.0, &[true], &[]), HostTime(1.003), &mut out);
        assert_eq!(
            edges(&out),
            vec![
                ("button:0".into(), true, 1.001),
                ("button:0".into(), false, 1.002),
                ("button:0".into(), true, 1.003),
            ]
        );
        assert_eq!(t.stats().stamped_by_poll, 3);
    }

    #[test]
    fn timestamp_may_predate_the_previous_poll_within_slack() {
        let mut t = PadTracker::new("pad");
        let mut out = Vec::new();
        t.update(&snap(0.990, &[false], &[]), HostTime(1.000), &mut out);
        // Per-frame polling: 16 ms between polls, the change 20 ms old.
        t.update(&snap(0.996, &[true], &[]), HostTime(1.016), &mut out);
        assert_eq!(edges(&out), vec![("button:0".into(), true, 0.996)]);
        // Older than the previous poll by more than the slack: poll time.
        out.clear();
        t.update(&snap(0.997, &[false], &[]), HostTime(1.032), &mut out);
        assert_eq!(edges(&out), vec![("button:0".into(), false, 1.032)]);
    }

    #[test]
    fn edges_never_go_back_in_time() {
        let mut t = PadTracker::new("pad");
        let mut out = Vec::new();
        t.update(&snap(0.990, &[false], &[]), HostTime(1.000), &mut out);
        t.update(&snap(0.990, &[true], &[]), HostTime(1.004), &mut out); // poll time
        t.update(&snap(0.999, &[false], &[]), HostTime(1.005), &mut out); // pad time < last edge
        assert_eq!(out[1].host_time, HostTime(1.004));
    }

    #[test]
    fn axes_have_hysteresis_and_rest_is_silent() {
        let mut t = PadTracker::new("pad");
        let mut out = Vec::new();
        // Trigger resting at -1: no edge on the first reading.
        t.update(&snap(0.0, &[], &[0.0, -1.0]), HostTime(1.0), &mut out);
        assert!(out.is_empty());
        t.update(&snap(0.0, &[], &[0.59, -1.0]), HostTime(1.01), &mut out);
        assert!(out.is_empty());
        t.update(&snap(0.0, &[], &[0.61, -1.0]), HostTime(1.02), &mut out);
        t.update(&snap(0.0, &[], &[0.45, -1.0]), HostTime(1.03), &mut out);
        t.update(&snap(0.0, &[], &[0.39, 1.0]), HostTime(1.04), &mut out);
        t.update(&snap(0.0, &[], &[-0.7, 1.0]), HostTime(1.05), &mut out);
        assert_eq!(
            edges(&out),
            vec![
                ("axis:0+".into(), true, 1.02),
                ("axis:0+".into(), false, 1.04),
                ("axis:1-".into(), false, 1.04),
                ("axis:1+".into(), true, 1.04),
                ("axis:0-".into(), true, 1.05),
            ]
        );
    }

    #[test]
    fn hat_axis_presses_directions_and_diagonals() {
        let step = 2.0 / 7.0;
        let mut t = PadTracker::new("pad");
        let mut out = Vec::new();
        t.update(&snap(0.0, &[], &[0.0, 1.2857]), HostTime(1.0), &mut out);
        assert!(out.is_empty());
        // Left, then up-left (a jump), then neutral.
        t.update(
            &snap(0.0, &[], &[0.0, -1.0 + 6.0 * step]),
            HostTime(1.01),
            &mut out,
        );
        t.update(&snap(0.0, &[], &[0.0, 1.0]), HostTime(1.02), &mut out);
        t.update(&snap(0.0, &[], &[0.0, 1.2857]), HostTime(1.03), &mut out);
        assert_eq!(
            edges(&out),
            vec![
                ("hat:1:left".into(), true, 1.01),
                ("hat:1:up".into(), true, 1.02),
                ("hat:1:up".into(), false, 1.03),
                ("hat:1:left".into(), false, 1.03),
            ]
        );
        assert!(t.active().is_empty());
    }

    #[test]
    fn axis_becomes_hat_when_seen_at_hat_rest() {
        let mut t = PadTracker::new("pad");
        let mut out = Vec::new();
        // First seen pressed "up" (-1): read as an axis.
        t.update(&snap(0.0, &[], &[-1.0]), HostTime(1.0), &mut out);
        assert_eq!(
            t.active(),
            vec![Control::Axis {
                index: 0,
                positive: false
            }]
        );
        t.update(&snap(0.0, &[], &[1.2857]), HostTime(1.01), &mut out);
        assert_eq!(edges(&out), vec![("axis:0-".into(), false, 1.01)]);
        out.clear();
        t.update(&snap(0.0, &[], &[-1.0]), HostTime(1.02), &mut out);
        assert_eq!(edges(&out), vec![("hat:0:up".into(), true, 1.02)]);
    }

    #[test]
    fn first_reading_reports_held_buttons_and_disconnect_releases() {
        let mut t = PadTracker::new("pad");
        let mut out = Vec::new();
        t.update(
            &snap(0.0, &[false, true], &[0.0, 1.2857]),
            HostTime(1.0),
            &mut out,
        );
        assert_eq!(edges(&out), vec![("button:1".into(), true, 1.0)]);
        out.clear();
        t.update(
            &snap(0.0, &[false, true], &[0.9, -1.0]),
            HostTime(1.01),
            &mut out,
        );
        out.clear();
        t.release_all(HostTime(1.02), &mut out);
        let mut released: Vec<String> = out.iter().map(|r| r.control.clone()).collect();
        released.sort();
        assert_eq!(released, vec!["axis:0+", "button:1", "hat:1:up"]);
        assert!(
            out.iter()
                .all(|r| !r.pressed && r.host_time == HostTime(1.02))
        );
        assert!(t.active().is_empty());
    }

    #[test]
    fn shorter_reading_releases_missing_buttons() {
        let mut t = PadTracker::new("pad");
        let mut out = Vec::new();
        t.update(
            &snap(0.0, &[false, false, true], &[]),
            HostTime(1.0),
            &mut out,
        );
        out.clear();
        t.update(&snap(0.0, &[false], &[]), HostTime(1.01), &mut out);
        assert_eq!(edges(&out), vec![("button:2".into(), false, 1.01)]);
        assert!(t.active().is_empty());
    }

    #[test]
    fn releases_come_before_presses_in_one_poll() {
        let mut t = PadTracker::new("pad");
        let mut out = Vec::new();
        t.update(&snap(0.0, &[false, true], &[]), HostTime(1.0), &mut out);
        out.clear();
        t.update(&snap(0.0, &[true, false], &[]), HostTime(1.01), &mut out);
        assert_eq!(
            edges(&out),
            vec![
                ("button:1".into(), false, 1.01),
                ("button:0".into(), true, 1.01)
            ]
        );
    }

    #[test]
    fn report_interval_is_estimated() {
        let mut t = PadTracker::new("pad");
        let mut out = Vec::new();
        for k in 0..50 {
            let ts = 1.0 + k as f64 * 0.004;
            t.update(&snap(ts, &[false], &[]), HostTime(ts + 0.0005), &mut out);
        }
        let r = t.stats().report_interval.unwrap();
        assert!((r - 0.004).abs() < 1e-9, "{r}");
    }
}
