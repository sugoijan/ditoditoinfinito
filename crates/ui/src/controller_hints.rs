//! Presentation of effective controller bindings. No input is dispatched here.
//! The shell feeds physical edges and connected identities; the shared UI
//! retains the last deliberate source across screens (and web route hosts).
use crate::keycaps::Keycap;
use ddi_engine::{ConnectedPad, ControlBindings, MenuInput, PadUse};
use ddi_platform::{DeviceId, RawInput, gamepad::Control};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashSet},
};

pub const CHANGED: &str = "ControllerHintsChanged";
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    #[default]
    Auto,
    Xbox,
    PlayStation,
    Nintendo,
    Generic,
}
impl Style {
    pub const ALL: [Self; 5] = [
        Self::Auto,
        Self::Xbox,
        Self::PlayStation,
        Self::Nintendo,
        Self::Generic,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Xbox => "Xbox",
            Self::PlayStation => "PlayStation",
            Self::Nintendo => "Nintendo",
            Self::Generic => "Generic",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Xbox,
    PlayStation,
    DualSense,
    Nintendo,
    Generic,
}
impl Family {
    pub fn label(self) -> &'static str {
        match self {
            Self::Xbox => "Xbox",
            Self::PlayStation => "PlayStation",
            Self::DualSense => "PlayStation (DualSense)",
            Self::Nintendo => "Nintendo",
            Self::Generic => "Generic",
        }
    }
    pub fn row(self) -> usize {
        match self {
            Self::Xbox => 0,
            Self::PlayStation => 1,
            Self::DualSense => 2,
            Self::Nintendo => 3,
            Self::Generic => 4,
        }
    }
}
/// Conservative browser USB formats: Chrome's Vendor/Product suffix or Firefox's
/// leading hex vendor-product pair. Generic names alone establish no brand.
fn browser_usb(id: &str) -> Option<[u16; 2]> {
    let id = id.to_ascii_lowercase();
    if let (Some((_, v)), Some((_, p))) = (id.split_once("vendor: "), id.split_once("product: ")) {
        return Some([
            u16::from_str_radix(v.get(..4)?, 16).ok()?,
            u16::from_str_radix(p.get(..4)?, 16).ok()?,
        ]);
    }
    let mut parts = id.split('-');
    let (v, p) = (parts.next()?, parts.next()?);
    (v.len() == 4 && p.len() == 4).then(|| {
        Some([
            u16::from_str_radix(v, 16).ok()?,
            u16::from_str_radix(p, 16).ok()?,
        ])
    })?
}
pub fn detect(pad: &ConnectedPad) -> (Family, &'static str) {
    use Family::*;
    if let Some([vendor, product]) = pad.vendor_product.or_else(|| browser_usb(&pad.id)) {
        let family = match (vendor, product) {
            (0x054c, 0x0ce6 | 0x0df2) => Some(DualSense),
            (0x054c, 0x05c4 | 0x09cc | 0x0ba0) => Some(PlayStation),
            (0x057e, 0x2009 | 0x200e) => Some(Nintendo),
            (
                0x045e,
                0x028e | 0x02d1 | 0x02dd | 0x02e0 | 0x02ea | 0x02fd | 0x0b00 | 0x0b05 | 0x0b12
                | 0x0b13 | 0x0b20 | 0x0b22,
            ) => Some(Xbox),
            _ => None,
        };
        if let Some(f) = family {
            return (f, "USB vendor/product");
        }
        // Unsupported Nintendo/Sony models must not inherit another model's art.
        if matches!(vendor, 0x054c | 0x057e) {
            return (Generic, "unknown model");
        }
    }
    let name = pad.id.to_ascii_lowercase();
    let f = if name.contains("dualsense") {
        DualSense
    } else if name.contains("dualshock 4")
        || name.contains("playstation 4")
        || name.contains("ps4 controller")
    {
        PlayStation
    } else if name.contains("xbox") {
        Xbox
    } else if name.contains("switch pro") || name.contains("joy-con (l+r)") {
        Nintendo
    } else {
        return (Generic, "unrecognized identity");
    };
    (f, "specific reported name")
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Glyph {
    Button(u16),
    Dpad(u8),
    Raw(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cap {
    pub family: Family,
    pub glyph: Glyph,
    pub side: Option<u8>,
}
impl Cap {
    pub fn control(pad: &ConnectedPad, family: Family, control: &str) -> Self {
        let parsed = control.parse::<Control>();
        let glyph = match parsed {
            Ok(Control::Button(n @ 0..=15)) if pad.standard => match n {
                12 => Glyph::Dpad(1),
                13 => Glyph::Dpad(2),
                14 => Glyph::Dpad(4),
                15 => Glyph::Dpad(8),
                _ => Glyph::Button(n),
            },
            Ok(c) => Glyph::Raw(c.label()),
            Err(_) => Glyph::Raw(control.into()),
        };
        Self {
            family,
            glyph,
            side: None,
        }
    }
    pub fn label(&self) -> String {
        match &self.glyph {
            Glyph::Raw(s) => s.clone(),
            Glyph::Dpad(mask) => [(1, "↑"), (2, "↓"), (4, "←"), (8, "→")]
                .into_iter()
                .filter(|(bit, _)| mask & bit != 0)
                .map(|(_, s)| s)
                .collect(),
            Glyph::Button(n) => {
                let labels = match self.family {
                    Family::Xbox => [
                        "A", "B", "X", "Y", "LB", "RB", "LT", "RT", "View", "Menu", "LS", "RS",
                    ],
                    Family::PlayStation | Family::DualSense => [
                        "Cross", "Circle", "Square", "Triangle", "L1", "R1", "L2", "R2", "Share",
                        "Options", "L3", "R3",
                    ],
                    Family::Nintendo => [
                        "B", "A", "Y", "X", "L", "R", "ZL", "ZR", "−", "+", "LS", "RS",
                    ],
                    Family::Generic => [
                        "South", "East", "West", "North", "LB", "RB", "LT", "RT", "SELECT",
                        "START", "LS", "RS",
                    ],
                };
                if self.family == Family::DualSense && *n == 8 {
                    "Create".into()
                } else {
                    labels.get(*n as usize).copied().unwrap_or("button").into()
                }
            }
        }
    }
    /// Atlas column and optical size. D-pad masks occupy 12..27; raw uses 28.
    pub fn art(&self) -> (usize, [f32; 2]) {
        match &self.glyph {
            Glyph::Dpad(mask) => (12 + usize::from(*mask), [35.1, 35.1]),
            Glyph::Button(n @ 0..=3) => (
                *n as usize,
                if self.family == Family::Generic {
                    [35.1, 35.1]
                } else {
                    [22.1, 22.1]
                },
            ),
            Glyph::Button(n @ 4..=5) => (*n as usize, [41.6, 26.0]),
            Glyph::Button(n @ 6..=7) => (
                *n as usize,
                [
                    if self.family == Family::Nintendo {
                        29.9
                    } else {
                        26.0
                    },
                    26.0,
                ],
            ),
            Glyph::Button(n @ 8..=9) if self.family == Family::Generic => {
                (*n as usize, [35.49, 16.9])
            }
            Glyph::Button(n @ 8..=11) => (*n as usize, [26.0, 26.0]),
            _ => (28, [26.0, 26.0]),
        }
    }
}
#[derive(Default)]
struct State {
    pads: Vec<ConnectedPad>,
    active: Option<(u32, String)>,
    controls: ControlBindings,
    styles: BTreeMap<String, Style>,
    held: HashSet<(u32, String)>,
    revision: u64,
    devices_revision: u64,
}
thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }
/// Starts an independent session (used by the headless harness).
pub fn reset() {
    STATE.with(|s| *s.borrow_mut() = State::default());
}
pub(crate) fn devices_revision() -> u64 {
    STATE.with(|s| s.borrow().devices_revision)
}
pub(crate) fn connected_identities() -> Vec<(u32, String)> {
    STATE.with(|s| {
        s.borrow()
            .pads
            .iter()
            .map(|p| (p.index, p.id.clone()))
            .collect()
    })
}
pub fn revision() -> u64 {
    STATE.with(|s| s.borrow().revision)
}
pub fn configure(controls: &ControlBindings, styles: &BTreeMap<String, Style>) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if &s.controls != controls || &s.styles != styles {
            s.controls = controls.clone();
            s.styles = styles.clone();
            if s.active.is_some() {
                s.revision += 1;
            }
        }
    });
}
pub fn devices(pads: Vec<ConnectedPad>) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.pads == pads {
            return;
        }
        let was_active = s.active.is_some();
        if s.active
            .as_ref()
            .is_some_and(|(slot, id)| !pads.iter().any(|p| p.index == *slot && p.id == *id))
        {
            s.active = None;
        }
        let removed: Vec<_> = s
            .pads
            .iter()
            .filter(|old| !pads.iter().any(|p| p.index == old.index && p.id == old.id))
            .map(|p| p.index)
            .collect();
        s.held.retain(|(slot, _)| !removed.contains(slot));
        s.pads = pads;
        s.devices_revision += 1;
        if was_active {
            s.revision += 1;
        }
    });
}
pub fn keyboard() {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.active.take().is_some() {
            s.revision += 1;
        }
    });
}
pub fn focus_lost() {
    STATE.with(|s| s.borrow_mut().held.clear());
}
pub fn pad_press(slot: u32) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let next = s
            .pads
            .iter()
            .find(|p| p.index == slot)
            .map(|p| (p.index, p.id.clone()));
        if next.is_some() && s.active != next {
            s.active = next;
            s.revision += 1;
        }
    });
}
pub fn edge(e: &RawInput) {
    if !matches!(e.device, DeviceId::Gamepad(_)) {
        return;
    }
    let fresh = STATE.with(|s| {
        let mut s = s.borrow_mut();
        if !s
            .pads
            .iter()
            .any(|p| p.index == e.slot && e.device == DeviceId::Gamepad(p.id.clone()))
        {
            return false;
        }
        let k = (e.slot, e.control.clone());
        if e.pressed {
            s.held.insert(k)
        } else {
            s.held.remove(&k);
            false
        }
    });
    if fresh {
        pad_press(e.slot);
    }
}
pub fn presentation(p: &ConnectedPad, style: Style) -> Family {
    match style {
        Style::Auto => detect(p).0,
        Style::Xbox => Family::Xbox,
        Style::PlayStation => Family::PlayStation,
        Style::Nintendo => Family::Nintendo,
        Style::Generic => Family::Generic,
    }
}
fn family(s: &State, p: &ConnectedPad) -> Family {
    presentation(p, s.styles.get(&p.id).copied().unwrap_or_default())
}

pub fn active() -> bool {
    STATE.with(|s| s.borrow().active.is_some())
}
/// One primary control per requested action, combining only D-pad directions.
pub fn actions(actions: &[MenuInput]) -> Vec<Keycap> {
    STATE.with(|s| {
        let s = s.borrow();
        let Some((slot, id)) = &s.active else {
            return Vec::new();
        };
        let Some(p) = s.pads.iter().find(|p| p.index == *slot && p.id == *id) else {
            return Vec::new();
        };
        let f = family(&s, p);
        let mut caps = Vec::new();
        let mut mask = 0;
        for action in actions {
            if let Some(c) = s.controls.menu_controls(id, p.standard, *action).first() {
                let cap = Cap::control(p, f, c);
                if let Glyph::Dpad(m) = cap.glyph {
                    mask |= m;
                } else if !caps.contains(&Keycap::Controller(cap.clone())) {
                    caps.push(Keycap::Controller(cap));
                }
            }
        }
        if mask != 0 {
            caps.insert(
                0,
                Keycap::Controller(Cap {
                    family: f,
                    glyph: Glyph::Dpad(mask),
                    side: None,
                }),
            );
        }
        caps
    })
}
/// Prefer Confirm; dedicated Start also activates an ordinary menu when South
/// was consumed by a saved Back binding. The editor requests Confirm alone.
pub fn activate() -> Vec<Keycap> {
    let c = actions(&[MenuInput::Confirm]);
    if c.is_empty() {
        actions(&[MenuInput::Start])
    } else {
        c
    }
}
/// Quit is shown after the actual cancel control was pressed. Unknown/raw
/// identities use a truthful reference to that same control, never guessed Back.
pub fn quit_hint(raw: &RawInput) -> &'static str {
    STATE.with(|s| {
        let s = s.borrow();
        let Some(p) = s
            .pads
            .iter()
            .find(|p| p.index == raw.slot && raw.device == DeviceId::Gamepad(p.id.clone()))
        else {
            return "hold or double-tap this control to quit";
        };
        let cap = Cap::control(p, family(&s, p), &raw.control);
        match cap.label().as_str() {
            "A" => "hold or double-tap A to quit",
            "B" => "hold or double-tap B to quit",
            "X" => "hold or double-tap X to quit",
            "Y" => "hold or double-tap Y to quit",
            "LB" => "hold or double-tap LB to quit",
            "RB" => "hold or double-tap RB to quit",
            "LT" => "hold or double-tap LT to quit",
            "RT" => "hold or double-tap RT to quit",
            "View" => "hold or double-tap View to quit",
            "Menu" => "hold or double-tap Menu to quit",
            "LS" => "hold or double-tap LS to quit",
            "RS" => "hold or double-tap RS to quit",
            "Cross" => "hold or double-tap Cross to quit",
            "Circle" => "hold or double-tap Circle to quit",
            "Square" => "hold or double-tap Square to quit",
            "Triangle" => "hold or double-tap Triangle to quit",
            "L1" => "hold or double-tap L1 to quit",
            "R1" => "hold or double-tap R1 to quit",
            "L2" => "hold or double-tap L2 to quit",
            "R2" => "hold or double-tap R2 to quit",
            "Share" => "hold or double-tap Share to quit",
            "Create" => "hold or double-tap Create to quit",
            "Options" => "hold or double-tap Options to quit",
            "L3" => "hold or double-tap L3 to quit",
            "R3" => "hold or double-tap R3 to quit",
            "L" => "hold or double-tap L to quit",
            "R" => "hold or double-tap R to quit",
            "ZL" => "hold or double-tap ZL to quit",
            "ZR" => "hold or double-tap ZR to quit",
            "−" => "hold or double-tap − to quit",
            "+" => "hold or double-tap + to quit",
            "South" => "hold or double-tap South to quit",
            "East" => "hold or double-tap East to quit",
            "West" => "hold or double-tap West to quit",
            "North" => "hold or double-tap North to quit",
            "SELECT" => "hold or double-tap SELECT to quit",
            "START" => "hold or double-tap START to quit",
            "↑" => "hold or double-tap ↑ to quit",
            "↓" => "hold or double-tap ↓ to quit",
            "←" => "hold or double-tap ← to quit",
            "→" => "hold or double-tap → to quit",
            _ => "hold or double-tap this control to quit",
        }
    })
}

/// The controller's effective Start binding, which cover controls actually use.
pub fn cover_instruction() -> Option<String> {
    STATE.with(|s| {
        let s=s.borrow();let (slot,id)=s.active.as_ref()?;
        let p=s.pads.iter().find(|p|p.index==*slot && p.id==*id)?;
        let b=s.controls.pad(id,p.standard)?;
        let c=b.start.as_ref()?;
        if b.back.as_ref()==Some(c) {return None;}
        let label=Cap::control(p,family(&s,p),c).label();
        Some(format!("In play, hold {label} and press an up- or down-arrow lane binding to move it; tap {label} to hide or show it."))
    })
}

/// The upcoming play's actual assignment, including split two-controller fields.
pub fn lanes(
    layout: &ddi_chart::Layout,
    controls: &ControlBindings,
) -> Option<Vec<Option<Keycap>>> {
    STATE.with(|s| {
        let s = s.borrow();
        let (slot, _) = s.active.as_ref()?;
        let mut out = vec![None; layout.lane_count()];
        let plan = controls.plan(layout, &s.pads);
        let resolved = controls.resolve(layout, &s.pads);
        let split = plan
            .iter()
            .any(|(p, u)| p.index == *slot && matches!(u, PadUse::OnePad { .. }));
        // The active controller wins when several can play the whole field.
        for (p, usage) in plan.iter().filter(|(p, _)| p.index == *slot).chain(
            plan.iter()
                .filter(|(p, u)| split && p.index != *slot && matches!(u, PadUse::OnePad { .. })),
        ) {
            let pairs: Vec<(usize, &Vec<String>)> = match usage {
                PadUse::Table { table, .. } => table.iter().enumerate().collect(),
                PadUse::OnePad { pad, single } => {
                    layout.lanes_of_pad(*pad).zip(single.iter()).collect()
                }
                PadUse::Unused => Vec::new(),
            };
            let b = controls.pad(&p.id, p.standard);
            for (lane, bindings) in pairs {
                if let Some(target) = out.get_mut(lane)
                    && target.is_none()
                {
                    *target = bindings
                        .iter()
                        .find(|c| {
                            b.as_ref().is_none_or(|b| b.back.as_ref() != Some(c))
                                && resolved.lane_of_slot(
                                    &DeviceId::Gamepad(p.id.clone()),
                                    p.index,
                                    c,
                                ) == Some(lane as u8)
                        })
                        .map(|c| {
                            let mut cap = Cap::control(p, family(&s, p), c);
                            if let PadUse::OnePad { pad, .. } = usage {
                                cap.side = Some(pad + 1);
                            }
                            Keycap::Controller(cap)
                        });
                }
            }
        }
        Some(out)
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn pad(id: &str, standard: bool) -> ConnectedPad {
        ConnectedPad {
            id: id.into(),
            index: 0,
            standard,
            vendor_product: None,
        }
    }
    #[test]
    fn conservative_detection_and_raw_override() {
        assert_eq!(detect(&pad("Wireless Controller", true)).0, Family::Generic);
        assert_eq!(detect(&pad("Pro Controller", true)).0, Family::Generic);
        assert_eq!(detect(&pad("PS3 Controller", true)).0, Family::Generic);
        assert_eq!(
            detect(&pad("054c-0268-Sony Controller", true)).0,
            Family::Generic
        );
        assert_eq!(
            detect(&pad("054c-0ce6-Wireless Controller", true)).0,
            Family::DualSense
        );
        assert_eq!(
            detect(&pad("Nintendo (Vendor: 057e Product: 2009)", true)).0,
            Family::Nintendo
        );
        assert_eq!(
            detect(&pad("Nintendo (Vendor: 057e Product: 2006)", true)).0,
            Family::Generic
        );
        assert!(matches!(
            Cap::control(&pad("Xbox", false), Family::Xbox, "button:0").glyph,
            Glyph::Raw(_)
        ));
    }
    #[test]
    fn effective_bindings_precedence_and_disconnect() {
        STATE.with(|s| *s.borrow_mut() = State::default());
        let p = pad("Xbox", true);
        devices(vec![p.clone()]);
        pad_press(0);
        let mut c = ControlBindings::default();
        let mut b = ddi_engine::PadBindings::standard(&p.id);
        b.back = Some("button:2".into());
        c.pads.push(b);
        configure(&c, &BTreeMap::new());
        assert!(actions(&[MenuInput::More]).is_empty());
        assert_eq!(actions(&[MenuInput::Back])[0].label(), "B");
        let l = lanes(&ddi_chart::Layout::dance_single(), &c).unwrap();
        assert_eq!(l[0].as_ref().unwrap().label(), "LB");
        keyboard();
        assert!(!active());
        pad_press(0);
        devices(vec![pad("Other", true)]);
        assert!(!active());
        STATE.with(|s| *s.borrow_mut() = State::default());
    }
    #[test]
    fn only_fresh_edges_change_source_and_new_ui_hosts_keep_it() {
        reset();
        let p = pad("Xbox", true);
        devices(vec![p.clone()]);
        let mut e = RawInput {
            device: DeviceId::Gamepad(p.id.clone()),
            control: "button:6".into(),
            slot: 0,
            pressed: true,
            host_time: ddi_platform::HostTime(1.0),
        };
        edge(&e);
        assert!(active());
        let mut ui = crate::Ui::new([1280.0, 720.0], crate::TextMode::Mirror, false);
        assert!(active());
        let mut repeat = crate::event::KeyEvent::down("KeyA", "a");
        repeat.repeat = true;
        ui.handle(crate::UiEvent::Key(repeat));
        assert!(active());
        ui.handle(crate::UiEvent::Key(crate::event::KeyEvent::down(
            "KeyA", "a",
        )));
        assert!(!active());
        edge(&e);
        assert!(!active(), "held pad must not reclaim hints");
        e.pressed = false;
        edge(&e);
        assert!(!active());
        e.pressed = true;
        edge(&e);
        assert!(active());
        ui.handle(crate::UiEvent::Focus(false));
        keyboard();
        edge(&e);
        assert!(active());
        devices(vec![pad("DualSense", true)]);
        assert!(!active());
        // A disconnected slot's stale edge cannot claim its replacement.
        edge(&e);
        assert!(!active());
        e.device = DeviceId::Gamepad("DualSense".into());
        e.pressed = false;
        edge(&e);
        e.pressed = true;
        edge(&e);
        assert!(active());
        assert_eq!(actions(&[MenuInput::Confirm])[0].label(), "Cross");
        reset();
    }

    #[test]
    fn diagrams_use_effective_custom_raw_and_split_tables() {
        reset();
        let p = pad("Xbox", true);
        let other = ConnectedPad {
            index: 1,
            ..pad("DualSense", true)
        };
        devices(vec![p.clone(), other]);
        pad_press(0);
        let mut c = ControlBindings::default();
        let mut b = ddi_engine::PadBindings::standard(&p.id);
        b.single[0] = vec!["button:14".into()];
        b.single[1].clear();
        b.single[2] = vec!["button:8".into(), "button:7".into()]; // Back consumes its own control.
        c.pads.push(b);
        configure(&c, &BTreeMap::new());
        let single = lanes(&ddi_chart::Layout::dance_single(), &c).unwrap();
        assert_eq!(single[0].as_ref().unwrap().label(), "←");
        assert!(
            single[1].is_none(),
            "another pad must not hide an explicitly unbound lane"
        );
        assert_eq!(single[2].as_ref().unwrap().label(), "RT");
        let double = lanes(&ddi_chart::Layout::builtin("dance-double").unwrap(), &c).unwrap();
        assert_eq!(double[4].as_ref().unwrap().label(), "L1");
        assert!(matches!(
            &double[4],
            Some(Keycap::Controller(Cap { side: Some(2), .. }))
        ));
        let raw = pad("Raw adapter", false);
        devices(vec![raw.clone()]);
        pad_press(0);
        let b = ddi_engine::PadBindings {
            id: raw.id.clone(),
            single: vec![
                vec!["button:40".into()],
                vec!["axis:3-".into()],
                vec!["hat:0:up".into()],
                vec![],
            ],
            ..Default::default()
        };
        let c = ControlBindings {
            pads: vec![b],
            ..Default::default()
        };
        configure(&c, &BTreeMap::from([(raw.id, Style::Nintendo)]));
        let caps = lanes(&ddi_chart::Layout::dance_single(), &c).unwrap();
        assert!(caps[..3].iter().all(|c| matches!(
            c,
            Some(Keycap::Controller(Cap {
                glyph: Glyph::Raw(_),
                ..
            }))
        )));
        assert!(caps[3].is_none());
        reset();
    }
}
