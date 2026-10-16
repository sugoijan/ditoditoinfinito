//! Guided binding (the options screen's bind dialog): press each lane's
//! control in turn on the device being bound, for one layout.
//!
//! Works the same for the keyboard and any controller, whatever its layout.
//! Controllers without the standard mapping (dance pads, pad adapters, many
//! generic USB pads) number their buttons arbitrarily and may report arrows
//! on axes or on a hat switch, so the only reliable way to learn the layout
//! is to ask for each arrow. Each step records the first press edge of the
//! chosen device; the shells' controller edges already treat axes and hats
//! at rest as unpressed, so only a deliberate movement counts.
//!
//! Two rules keep stray input out: once a device is chosen, presses from
//! every other device are ignored (a drifting stick on another pad cannot
//! fill a step), and a recorded control must be released before the next
//! step listens (one press cannot fill two steps).

use std::collections::HashSet;

use ddi_chart::Layout;
use ddi_engine::{ControlBindings, MenuInput};
use ddi_platform::gamepad::Control;
use ddi_platform::{DeviceId, RawInput};

/// Steps after the lanes, for controllers only.
pub const PAD_STEPS: [&str; 2] = ["Start", "Back"];

/// Seconds a controller's button is held to cancel (a pad without a
/// Back of its own can always get out).
pub const HOLD_TO_CANCEL: f32 = 1.0;

/// Whether `edge` is a controller's Back as the menus read it (B on a
/// standard pad, or its saved Back).
pub fn pad_back(controls: &ControlBindings, standard: &[String], edge: &RawInput) -> bool {
    let DeviceId::Gamepad(id) = &edge.device else {
        return false;
    };
    controls.menu_input(id, standard.iter().any(|s| s == id), &edge.control)
        == Some(MenuInput::Back)
}

/// Controllers' buttons held, and for how long: holding one
/// [`HOLD_TO_CANCEL`] cancels. Only buttons pressed while the dialog is
/// open count, one at a time: an axis (a stick or a trigger resting past
/// its threshold) never does, nor do buttons a pad reports held in the
/// same reading as another (a pad revealed by a press while a panel is
/// stuck down reports both at once).
#[derive(Clone, Debug, Default)]
pub struct Holds {
    /// Device, control, seconds held, when pressed, and whether it counts.
    held: Vec<(DeviceId, String, f32, f64, bool)>,
}

impl Holds {
    /// A press or a release; keyboards are not counted (Escape cancels).
    pub fn edge(&mut self, edge: &RawInput) {
        if !matches!(edge.device, DeviceId::Gamepad(_)) {
            return;
        }
        let same =
            |h: &(DeviceId, String, f32, f64, bool)| h.0 == edge.device && h.1 == edge.control;
        if edge.pressed {
            if !self.held.iter().any(same) {
                let at = edge.host_time.0;
                let mut counts = !edge.control.starts_with("axis");
                for h in &mut self.held {
                    if h.0 == edge.device && h.3 == at {
                        h.4 = false;
                        counts = false;
                    }
                }
                self.held
                    .push((edge.device.clone(), edge.control.clone(), 0.0, at, counts));
            }
        } else {
            self.held.retain(|h| !same(h));
        }
    }

    /// Time passes; whether a button not `exempt` has been held long
    /// enough.
    pub fn tick(&mut self, dt: f32, exempt: impl Fn(&DeviceId, &str) -> bool) -> bool {
        let mut long = false;
        for h in &mut self.held {
            if !h.4 || exempt(&h.0, &h.1) {
                continue;
            }
            h.2 += dt;
            long |= h.2 >= HOLD_TO_CANCEL;
        }
        long
    }

    /// Whether a button that counts is held (the dialog's clock runs).
    pub fn any(&self) -> bool {
        self.held.iter().any(|h| h.4)
    }
}

/// What a finished flow binds; it replaces the device's table for the
/// layout (and a controller's Start and Back).
#[derive(Clone, Debug, PartialEq)]
pub enum Bound {
    /// One key per lane (`KeyboardEvent.code`).
    Keyboard(Vec<Vec<String>>),
    Pad {
        id: String,
        /// One control per lane.
        table: Vec<Vec<String>>,
        start: Option<String>,
        back: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Waiting for a press that picks the device.
    Choose,
    /// Waiting for the control of step `n`.
    Step(usize),
    /// Everything recorded: a live test, then save.
    Review,
}

/// What an edge did, for the dialog.
#[derive(Clone, Debug, PartialEq)]
pub enum Edge {
    /// Nothing to show.
    Ignored,
    /// The flow moved on or what is held changed.
    Changed,
    /// Escape (the keyboard's): leave without saving.
    Cancel,
    /// In review, the device's Start (Enter on the keyboard): save.
    Save,
}

pub struct BindFlow {
    pub device: Option<DeviceId>,
    pub phase: Phase,
    /// Step names: the layout's lane labels, then [`PAD_STEPS`].
    pub steps: Vec<String>,
    /// Lanes of the layout (the steps before [`PAD_STEPS`]).
    pub lanes: usize,
    /// Review grid rows: lanes of each pad (two-pad layouts) or each row.
    pub groups: Vec<Vec<usize>>,
    pub recorded: Vec<Option<String>>,
    /// A control that must be released before the next step listens.
    pub waiting: Option<String>,
    pub message: Option<String>,
    /// Controls of the chosen device held right now.
    pub held: HashSet<String>,
    layout: String,
    controls: ControlBindings,
    /// Controllers with the standard mapping.
    standard: Vec<String>,
}

impl BindFlow {
    /// A flow for `device` (`None`: the first press picks it) on
    /// `layout`; `controls` are the saved bindings (Start and Back cannot
    /// take a control that plays a lane in another layout of the same
    /// controller), `standard` the controllers connected with the standard
    /// mapping, `held` what the device holds already.
    pub fn new(
        device: Option<DeviceId>,
        layout: &Layout,
        controls: ControlBindings,
        standard: Vec<String>,
        held: HashSet<String>,
    ) -> BindFlow {
        let steps: Vec<String> = layout
            .lanes
            .iter()
            .map(|l| l.label().to_string())
            .chain(PAD_STEPS.iter().map(|s| s.to_string()))
            .collect();
        BindFlow {
            phase: if device.is_some() {
                Phase::Step(0)
            } else {
                Phase::Choose
            },
            device,
            recorded: vec![None; steps.len()],
            groups: review_groups(layout),
            lanes: layout.lane_count(),
            steps,
            waiting: None,
            message: None,
            held,
            layout: layout.id.clone(),
            controls,
            standard,
        }
    }

    pub fn is_pad(&self) -> bool {
        matches!(self.device, Some(DeviceId::Gamepad(_)))
    }

    /// How many steps the device has (Start and Back for controllers).
    pub fn step_count(&self) -> usize {
        if self.is_pad() {
            self.steps.len()
        } else {
            self.lanes
        }
    }

    /// A control as shown ("button 3", "KeyD").
    pub fn label(&self, control: &str) -> String {
        if self.is_pad() {
            control
                .parse::<Control>()
                .map(|c| c.label())
                .unwrap_or_else(|_| control.to_string())
        } else {
            control.to_string()
        }
    }

    /// What to press now.
    pub fn prompt(&self) -> String {
        match self.phase {
            Phase::Choose => {
                "Press any key or controller button on the device you want to bind".into()
            }
            Phase::Step(step) => match step.checked_sub(self.lanes) {
                Some(0) => "Press the button for Start (starts a song)".into(),
                Some(_) => "Press the button for Back (leaves a song)".into(),
                None => format!("Press {}", self.steps[step]),
            },
            Phase::Review => "Press each one to test it: it lights while held.".into(),
        }
    }

    /// What step `i` shows: its name, and what was recorded.
    pub fn step_value(&self, i: usize) -> String {
        let done = match self.phase {
            Phase::Step(s) => i < s,
            Phase::Review => true,
            Phase::Choose => false,
        };
        match (&self.recorded[i], done) {
            (Some(c), _) => self.label(c),
            (None, true) => "skipped".into(),
            (None, false) => String::new(),
        }
    }

    /// Whether Skip applies (Start and Back are optional).
    pub fn can_skip(&self) -> bool {
        matches!(self.phase, Phase::Step(s) if s >= self.lanes)
    }

    /// Handles one press or release.
    pub fn edge(&mut self, edge: &RawInput) -> Edge {
        if edge.device == DeviceId::Keyboard && edge.control == "Escape" {
            return if edge.pressed {
                Edge::Cancel
            } else {
                Edge::Ignored
            };
        }
        // Another controller's Back (or any one's while none is chosen).
        if edge.pressed
            && self.device.as_ref() != Some(&edge.device)
            && pad_back(&self.controls, &self.standard, edge)
        {
            return Edge::Cancel;
        }
        match &self.device {
            None => {
                // The first press picks the device; it still has to be
                // released before the first step listens.
                if !edge.pressed || self.phase != Phase::Choose {
                    return Edge::Ignored;
                }
                if !matches!(edge.device, DeviceId::Keyboard | DeviceId::Gamepad(_)) {
                    return Edge::Ignored;
                }
                self.device = Some(edge.device.clone());
                self.held = HashSet::from([edge.control.clone()]);
                self.waiting = Some(edge.control.clone());
                self.phase = Phase::Step(0);
                return Edge::Changed;
            }
            Some(d) if *d != edge.device => return Edge::Ignored,
            Some(_) => {}
        }
        if !edge.pressed {
            self.held.remove(&edge.control);
            if self.waiting.as_ref() == Some(&edge.control) {
                self.waiting = None;
                self.message = None;
            }
            return Edge::Changed;
        }
        self.held.insert(edge.control.clone());
        let step = match self.phase {
            Phase::Step(s) => s,
            Phase::Review => return self.review_press(&edge.control),
            Phase::Choose => return Edge::Changed,
        };
        if let Some(w) = &self.waiting {
            self.message = Some(format!("Release {} first.", self.label(w)));
            return Edge::Changed;
        }
        if edge.device == DeviceId::Keyboard && edge.control == "Backspace" {
            // Play treats it as quit before any lane sees it.
            self.message = Some("Backspace leaves a song; press a different key.".into());
            return Edge::Changed;
        }
        if let Some(i) = self
            .recorded
            .iter()
            .position(|r| r.as_deref() == Some(edge.control.as_str()))
        {
            self.message = Some(format!(
                "{} is already {}. Press a different one.",
                self.label(&edge.control),
                self.steps[i]
            ));
            return Edge::Changed;
        }
        if step >= self.lanes
            && let Some(DeviceId::Gamepad(id)) = &self.device
            && self.lane_elsewhere(id, &edge.control)
        {
            self.message = Some(format!(
                "{} plays a lane in another style's bindings. Press a different one.",
                self.label(&edge.control)
            ));
            return Edge::Changed;
        }
        self.recorded[step] = Some(edge.control.clone());
        self.waiting = Some(edge.control.clone());
        self.message = None;
        self.advance();
        Edge::Changed
    }

    /// A press while testing: the device's Start saves (the keyboard's
    /// Enter, unless it plays a lane), anything else lights.
    fn review_press(&mut self, control: &str) -> Edge {
        if self.is_lane(control) {
            return Edge::Changed;
        }
        let (start, back) = match &self.device {
            // Enter, or Space where Enter plays a lane.
            Some(DeviceId::Keyboard) => {
                (matches!(control, "Enter" | "NumpadEnter" | "Space"), false)
            }
            Some(DeviceId::Gamepad(id)) => {
                let saved = self.controls.pad(id, self.standard.contains(id));
                let is = |recorded: Option<&String>, saved: Option<String>| {
                    recorded
                        .map(String::as_str)
                        .or(saved.as_deref())
                        .is_some_and(|s| s == control)
                };
                (
                    is(
                        self.recorded[self.lanes].as_ref(),
                        saved.as_ref().and_then(|p| p.start.clone()),
                    ),
                    is(
                        self.recorded[self.lanes + 1].as_ref(),
                        saved.as_ref().and_then(|p| p.back.clone()),
                    ),
                )
            }
            _ => (false, false),
        };
        if start {
            Edge::Save
        } else if back {
            Edge::Cancel
        } else {
            Edge::Changed
        }
    }

    /// Whether `control` was recorded for a lane (of the device bound).
    pub fn is_lane(&self, control: &str) -> bool {
        self.recorded[..self.lanes]
            .iter()
            .any(|r| r.as_deref() == Some(control))
    }

    /// The controllers counted as standard, and the saved bindings (for a
    /// dialog's own reading of a Back).
    pub fn pads(&self) -> (&ControlBindings, &[String]) {
        (&self.controls, &self.standard)
    }

    /// Start and Back keep what they had.
    pub fn skip(&mut self) {
        if let Phase::Step(step) = self.phase
            && step >= self.lanes
        {
            self.recorded[step] = None;
            self.message = None;
            self.advance();
        }
    }

    /// From the first step again.
    pub fn redo(&mut self) {
        self.recorded = vec![None; self.steps.len()];
        self.message = None;
        self.phase = Phase::Step(0);
    }

    /// Whether `control` plays a lane of this controller in a layout other
    /// than the one being bound.
    fn lane_elsewhere(&self, id: &str, control: &str) -> bool {
        let Some(pad) = self.controls.pad(id, self.standard.iter().any(|s| s == id)) else {
            return false;
        };
        let single = (self.layout != "dance-single").then_some(&pad.single);
        single
            .into_iter()
            .chain(
                pad.layouts
                    .iter()
                    .filter(|(l, _)| **l != self.layout)
                    .map(|(_, t)| t),
            )
            .flatten()
            .any(|c| c.iter().any(|c| c == control))
    }

    fn advance(&mut self) {
        if let Phase::Step(step) = self.phase {
            self.phase = if step + 1 < self.step_count() {
                Phase::Step(step + 1)
            } else {
                Phase::Review
            };
        }
    }

    /// Pairs of lanes bound to two ends of one axis or hat: they cannot be
    /// pressed together.
    pub fn warnings(&self) -> Vec<String> {
        let controls: Vec<Option<Control>> = self.recorded[..self.lanes]
            .iter()
            .map(|c| c.as_ref().and_then(|c| c.parse().ok()))
            .collect();
        let mut out = Vec::new();
        for i in 0..self.lanes {
            for j in i + 1..self.lanes {
                if let (Some(a), Some(b)) = (&controls[i], &controls[j])
                    && a.exclusive_with(b)
                {
                    out.push(format!(
                        "{} and {} are opposite ends of one axis: they cannot be pressed together.",
                        self.steps[i], self.steps[j]
                    ));
                }
            }
        }
        out
    }

    /// What was bound, once a device is chosen.
    pub fn bound(&self) -> Option<Bound> {
        let table: Vec<Vec<String>> = self.recorded[..self.lanes]
            .iter()
            .map(|c| c.iter().cloned().collect())
            .collect();
        match &self.device {
            Some(DeviceId::Keyboard) => Some(Bound::Keyboard(table)),
            Some(DeviceId::Gamepad(id)) => Some(Bound::Pad {
                id: id.clone(),
                table,
                start: self.recorded[self.lanes].clone(),
                back: self.recorded[self.lanes + 1].clone(),
            }),
            _ => None,
        }
    }
}

/// Lanes of the review grid, one group per pad (two-pad layouts) or per
/// row of the field.
fn review_groups(layout: &Layout) -> Vec<Vec<usize>> {
    let key = |i: usize| {
        let l = &layout.lanes[i];
        if layout.pads() > 1 { l.pad } else { l.row }
    };
    let mut groups: Vec<(u8, Vec<usize>)> = Vec::new();
    for i in 0..layout.lane_count() {
        match groups.iter_mut().find(|(k, _)| *k == key(i)) {
            Some((_, g)) => g.push(i),
            None => groups.push((key(i), vec![i])),
        }
    }
    groups.into_iter().map(|(_, g)| g).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_platform::HostTime;

    fn key(code: &str, pressed: bool) -> RawInput {
        RawInput {
            device: DeviceId::Keyboard,
            control: code.into(),
            pressed,
            host_time: HostTime(0.0),
            slot: 0,
        }
    }

    fn pad(id: &str, control: &str, pressed: bool) -> RawInput {
        RawInput {
            device: DeviceId::Gamepad(id.into()),
            control: control.into(),
            pressed,
            host_time: HostTime(0.0),
            slot: 0,
        }
    }

    #[test]
    fn the_keyboard_is_bound_lane_by_lane() {
        let layout = Layout::dance_single();
        let mut f = BindFlow::new(
            Some(DeviceId::Keyboard),
            &layout,
            ControlBindings::default(),
            Vec::new(),
            HashSet::new(),
        );
        assert_eq!(f.prompt(), "Press Left");
        f.edge(&key("KeyD", true));
        // Not released: the next press waits.
        f.edge(&key("KeyF", true));
        assert_eq!(f.message.as_deref(), Some("Release KeyD first."));
        f.edge(&key("KeyD", false));
        f.edge(&key("KeyF", false));
        // A key already used, and Backspace, are refused.
        f.edge(&key("KeyD", true));
        assert!(f.message.as_deref().unwrap().contains("already Left"));
        f.edge(&key("KeyD", false));
        f.edge(&key("Backspace", true));
        assert!(f.message.as_deref().unwrap().contains("Backspace"));
        f.edge(&key("Backspace", false));
        for k in ["KeyF", "KeyJ", "KeyK"] {
            f.edge(&key(k, true));
            f.edge(&key(k, false));
        }
        assert_eq!(f.phase, Phase::Review);
        // Testing lights; Enter saves; Escape cancels.
        f.edge(&key("KeyJ", true));
        assert!(f.held.contains("KeyJ"));
        assert_eq!(f.edge(&key("Enter", true)), Edge::Save);
        assert_eq!(f.edge(&key("Escape", true)), Edge::Cancel);
        assert_eq!(
            f.bound(),
            Some(Bound::Keyboard(vec![
                vec!["KeyD".into()],
                vec!["KeyF".into()],
                vec!["KeyJ".into()],
                vec!["KeyK".into()]
            ]))
        );
    }

    #[test]
    fn a_controller_picked_by_a_press_binds_start_and_back_too() {
        let layout = Layout::dance_single();
        let mut f = BindFlow::new(
            None,
            &layout,
            ControlBindings::default(),
            Vec::new(),
            HashSet::new(),
        );
        assert_eq!(f.phase, Phase::Choose);
        f.edge(&pad("Pad", "button:0", true));
        assert!(f.is_pad());
        // Another device is ignored.
        assert_eq!(f.edge(&key("KeyA", true)), Edge::Ignored);
        f.edge(&pad("Pad", "button:0", false));
        for b in ["button:0", "button:1", "button:2", "button:3", "button:9"] {
            f.edge(&pad("Pad", b, true));
            f.edge(&pad("Pad", b, false));
        }
        assert!(f.can_skip());
        f.skip();
        assert_eq!(f.phase, Phase::Review);
        assert_eq!(f.step_value(5), "skipped");
        // The recorded Start saves.
        assert_eq!(f.edge(&pad("Pad", "button:9", true)), Edge::Save);
        let Some(Bound::Pad { start, back, .. }) = f.bound() else {
            panic!()
        };
        assert_eq!(start.as_deref(), Some("button:9"));
        assert_eq!(back, None);
        // Its recorded Back cancels; another (not standard) pad's B does not., or another pad's, cancels.
        f.recorded[5] = Some("button:8".into());
        assert_eq!(f.edge(&pad("Pad", "button:8", true)), Edge::Cancel);
        assert_eq!(
            f.edge(&pad("Other (STANDARD GAMEPAD)", "button:1", true)),
            Edge::Ignored
        );
    }

    #[test]
    fn a_controller_can_always_get_out() {
        let layout = Layout::dance_single();
        let std = "Std (STANDARD GAMEPAD)".to_string();
        // Binding the keyboard: a standard pad's B cancels.
        let mut f = BindFlow::new(
            Some(DeviceId::Keyboard),
            &layout,
            ControlBindings::default(),
            vec![std.clone()],
            HashSet::new(),
        );
        assert_eq!(f.edge(&pad(&std, "button:1", true)), Edge::Cancel);
        // While none is chosen too.
        let mut f = BindFlow::new(
            None,
            &layout,
            ControlBindings::default(),
            vec![std.clone()],
            HashSet::new(),
        );
        assert_eq!(f.edge(&pad(&std, "button:1", true)), Edge::Cancel);
        // Holding any controller's button a second.
        let mut h = Holds::default();
        h.edge(&pad("Odd pad", "button:3", true));
        assert!(!h.tick(0.6, |_, _| false));
        assert!(h.tick(0.5, |_, _| false));
        h.edge(&pad("Odd pad", "button:3", false));
        assert!(!h.any());
        // Not a key, nor what is exempt (a lane being tested).
        h.edge(&key("KeyA", true));
        assert!(!h.any());
        h.edge(&pad("Odd pad", "button:0", true));
        assert!(!h.tick(2.0, |_, c| c == "button:0"));
        // Space saves a keyboard binding whose lanes took Enter.
        let mut f = BindFlow::new(
            Some(DeviceId::Keyboard),
            &layout,
            ControlBindings::default(),
            Vec::new(),
            HashSet::new(),
        );
        for k in ["Enter", "KeyF", "KeyJ", "KeyK"] {
            f.edge(&key(k, true));
            f.edge(&key(k, false));
        }
        assert_eq!(f.edge(&key("Enter", true)), Edge::Changed);
        assert_eq!(f.edge(&key("Space", true)), Edge::Save);
    }
}

#[cfg(test)]
mod hold_tests {
    use super::*;
    use ddi_platform::HostTime;

    fn pad(control: &str, pressed: bool, t: f64) -> RawInput {
        RawInput {
            device: DeviceId::Gamepad("Pad".into()),
            control: control.into(),
            pressed,
            host_time: HostTime(t),
            slot: 0,
        }
    }

    #[test]
    fn only_a_button_pressed_alone_cancels_when_held() {
        let never = |_: &DeviceId, _: &str| false;
        // A button pressed and held.
        let mut h = Holds::default();
        h.edge(&pad("button:0", true, 1.0));
        assert!(!h.tick(0.5, never));
        assert!(h.tick(0.6, never));
        // An axis resting past its threshold never does.
        let mut h = Holds::default();
        h.edge(&pad("axis:1+", true, 1.0));
        assert!(!h.tick(5.0, never));
        assert!(!h.any());
        // A pad revealed by a press with a panel stuck down: both come in
        // one reading; neither counts, even once the press is let go.
        let mut h = Holds::default();
        h.edge(&pad("button:3", true, 2.0));
        h.edge(&pad("button:0", true, 2.0));
        h.edge(&pad("button:0", false, 2.1));
        assert!(!h.tick(5.0, never));
        // A button pressed afterwards still does.
        h.edge(&pad("button:1", true, 3.0));
        assert!(h.tick(1.1, never));
    }
}
