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
    Gamepad(u32),
    Touch,
    Other(String),
}

impl From<&DeviceId> for BindingDevice {
    fn from(d: &DeviceId) -> BindingDevice {
        match d {
            DeviceId::Keyboard => BindingDevice::Keyboard,
            DeviceId::Gamepad(n) => BindingDevice::Gamepad(*n),
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
    /// `KeyboardEvent.code`, `button:<n>`, etc.
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

#[cfg(test)]
mod tests {
    use super::*;

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
        b.rebind(2, DeviceId::Gamepad(0), "button:3");
        assert_eq!(b.resolve(&raw), None);
        assert_eq!(b.lane_of(&DeviceId::Gamepad(0), "button:3"), Some(2));
        // Binding a control already used elsewhere moves it.
        b.bind(BindingDevice::Keyboard, "ArrowLeft", 3);
        assert_eq!(b.lane_of(&DeviceId::Keyboard, "ArrowLeft"), Some(3));
        assert_eq!(b.controls_of(0).count(), 1);
    }
}
