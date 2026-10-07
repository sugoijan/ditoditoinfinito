//! Lane-level input events and device bindings.

use ddi_chart::Layout;
use ddi_platform::{DeviceId, HostTime, RawInput};
use serde::{Deserialize, Serialize};

/// A lane going down or up.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InputEvent {
    pub lane: u8,
    pub pressed: bool,
    #[serde(with = "host_time_serde")]
    pub host_time: HostTime,
}

/// Serde shim for [`HostTime`], which the platform crate leaves plain.
pub mod host_time_serde {
    use ddi_platform::HostTime;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(t: &HostTime, s: S) -> Result<S::Ok, S::Error> {
        t.0.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<HostTime, D::Error> {
        f64::deserialize(d).map(HostTime)
    }
}

/// Serializable mirror of [`DeviceId`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BindingDevice {
    Keyboard,
    Gamepad(String),
    Touch,
    Other(String),
}

impl From<&DeviceId> for BindingDevice {
    fn from(d: &DeviceId) -> BindingDevice {
        match d {
            DeviceId::Keyboard => BindingDevice::Keyboard,
            DeviceId::Gamepad(id) => BindingDevice::Gamepad(id.clone()),
            DeviceId::Touch => BindingDevice::Touch,
            DeviceId::Other(s) => BindingDevice::Other(s.clone()),
        }
    }
}

impl From<DeviceId> for BindingDevice {
    fn from(d: DeviceId) -> BindingDevice {
        BindingDevice::from(&d)
    }
}

/// One control → lane mapping.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub device: BindingDevice,
    /// `KeyboardEvent.code`, a gamepad control (`button:<n>`, `axis:<n>+`,
    /// `hat:<n>:up`), `lane:<n>` for touch.
    pub control: String,
    pub lane: u8,
}

/// `(device, control)` → lane table. Several controls may map to one lane;
/// a control maps to at most one lane.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bindings {
    pub bindings: Vec<Binding>,
}

impl Bindings {
    /// Keyboard bindings from a layout's `default_keys`.
    pub fn from_layout(layout: &Layout) -> Bindings {
        let mut b = Bindings::default();
        for (lane, l) in layout.lanes.iter().enumerate() {
            for key in &l.default_keys {
                b.bind(BindingDevice::Keyboard, key, lane as u8);
            }
        }
        b
    }

    /// Maps `control` on `device` to `lane`, replacing any previous mapping
    /// of that control.
    pub fn bind(&mut self, device: impl Into<BindingDevice>, control: &str, lane: u8) {
        let device = device.into();
        self.bindings
            .retain(|b| !(b.device == device && b.control == control));
        self.bindings.push(Binding {
            device,
            control: control.to_string(),
            lane,
        });
    }

    /// Replaces every mapping of `lane` with the single given control.
    pub fn rebind(&mut self, lane: u8, device: impl Into<BindingDevice>, control: &str) {
        self.bindings.retain(|b| b.lane != lane);
        self.bind(device, control, lane);
    }

    /// Removes every mapping of `lane`.
    pub fn unbind_lane(&mut self, lane: u8) {
        self.bindings.retain(|b| b.lane != lane);
    }

    /// Lane for a control, if bound.
    pub fn lane_of(&self, device: &DeviceId, control: &str) -> Option<u8> {
        let device = BindingDevice::from(device);
        self.bindings
            .iter()
            .find(|b| b.device == device && b.control == control)
            .map(|b| b.lane)
    }

    /// Controls bound to a lane, as `(device, control)`.
    pub fn controls_of(&self, lane: u8) -> impl Iterator<Item = (&BindingDevice, &str)> {
        self.bindings
            .iter()
            .filter(move |b| b.lane == lane)
            .map(|b| (&b.device, b.control.as_str()))
    }

    /// Turns a device edge into a lane event, or `None` if unbound.
    pub fn resolve(&self, raw: &RawInput) -> Option<InputEvent> {
        self.lane_of(&raw.device, &raw.control)
            .map(|lane| InputEvent {
                lane,
                pressed: raw.pressed,
                host_time: raw.host_time,
            })
    }
}

/// Turns device edges into lane events through [`Bindings`].
///
/// Every press of a bound control is a step, even while another control
/// already holds the lane (two keys on one lane can alternate on a stream,
/// as in StepMania). A lane is released only when the last control holding
/// it is let go, so a hold survives pressing a second control on its lane.
#[derive(Clone, Debug, Default)]
pub struct LaneInput {
    pub bindings: Bindings,
    /// Controls currently down, with the lane each one pressed.
    held: Vec<(DeviceId, String, u8)>,
}

impl LaneInput {
    pub fn new(bindings: Bindings) -> LaneInput {
        LaneInput {
            bindings,
            held: Vec::new(),
        }
    }

    /// The lane event for a device edge, if any. Repeated presses of a held
    /// control and releases of controls never seen pressed are dropped.
    pub fn feed(&mut self, raw: &RawInput) -> Option<InputEvent> {
        let found = self
            .held
            .iter()
            .position(|(d, c, _)| *d == raw.device && *c == raw.control);
        let lane = if raw.pressed {
            if found.is_some() {
                return None;
            }
            let lane = self.bindings.lane_of(&raw.device, &raw.control)?;
            self.held
                .push((raw.device.clone(), raw.control.clone(), lane));
            lane
        } else {
            let (_, _, lane) = self.held.swap_remove(found?);
            if self.lane_held(lane) {
                return None;
            }
            lane
        };
        Some(InputEvent {
            lane,
            pressed: raw.pressed,
            host_time: raw.host_time,
        })
    }

    /// Whether any control holds `lane`.
    pub fn lane_held(&self, lane: u8) -> bool {
        self.held.iter().any(|(_, _, l)| *l == lane)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(device: DeviceId, control: &str, pressed: bool, t: f64) -> RawInput {
        RawInput {
            device,
            control: control.into(),
            pressed,
            host_time: HostTime(t),
        }
    }

    #[test]
    fn lane_input_presses_always_releases_on_last() {
        let mut b = Bindings::from_layout(&Layout::dance_single());
        let pad = DeviceId::Gamepad("Pad (Vendor: 1234 Product: 5678)".into());
        b.bind(&pad, "hat:9:left", 0);
        let mut li = LaneInput::new(b);
        let kb = DeviceId::Keyboard;
        let ev = |pressed, t| {
            Some(InputEvent {
                lane: 0,
                pressed,
                host_time: HostTime(t),
            })
        };
        assert_eq!(
            li.feed(&raw(kb.clone(), "ArrowLeft", true, 1.0)),
            ev(true, 1.0)
        );
        // A second control on the lane steps again while the first holds it.
        assert_eq!(
            li.feed(&raw(pad.clone(), "hat:9:left", true, 1.1)),
            ev(true, 1.1)
        );
        assert_eq!(li.feed(&raw(pad.clone(), "hat:9:left", true, 1.15)), None);
        // Letting go of one keeps the lane held.
        assert_eq!(li.feed(&raw(kb.clone(), "ArrowLeft", false, 1.2)), None);
        assert!(li.lane_held(0));
        assert_eq!(
            li.feed(&raw(pad.clone(), "hat:9:left", false, 1.3)),
            ev(false, 1.3)
        );
        assert!(!li.lane_held(0));
        // Unknown releases and unbound controls do nothing.
        assert_eq!(li.feed(&raw(kb.clone(), "KeyD", false, 1.4)), None);
        assert_eq!(li.feed(&raw(pad.clone(), "button:7", true, 1.5)), None);
        assert_eq!(li.feed(&raw(pad, "button:7", false, 1.6)), None);
    }

    #[test]
    fn layout_defaults_resolve_and_rebind() {
        let mut b = Bindings::from_layout(&Layout::dance_single());
        let raw = RawInput {
            device: DeviceId::Keyboard,
            control: "KeyJ".into(),
            pressed: true,
            host_time: HostTime(1.0),
        };
        assert_eq!(
            b.resolve(&raw),
            Some(InputEvent {
                lane: 2,
                pressed: true,
                host_time: HostTime(1.0),
            })
        );
        let pad = DeviceId::Gamepad("pad".into());
        b.rebind(2, &pad, "button:3");
        assert_eq!(b.resolve(&raw), None);
        assert_eq!(b.lane_of(&pad, "button:3"), Some(2));
        // Binding a control already used elsewhere moves it.
        b.bind(BindingDevice::Keyboard, "ArrowLeft", 3);
        assert_eq!(b.lane_of(&DeviceId::Keyboard, "ArrowLeft"), Some(3));
        assert_eq!(b.controls_of(0).count(), 1);
    }
}
