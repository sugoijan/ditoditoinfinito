//! Presentation of effective controller bindings: [`Hints`], the value the
//! UI core keeps (`docs/plans/controller-input-core.md`), from the
//! bindings, the pads connected and the input the player last used. No
//! input is dispatched here.
use crate::keycaps::Keycap;
use ddi_engine::{ConnectedPad, ControlBindings, MenuInput, PadUse};
use ddi_platform::{DeviceId, RawInput, gamepad::Control};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
/// The input the hints follow: the keyboard, or the controller in `slot`
/// whose identity is `id` (`Gamepad.id`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Source {
    #[default]
    Keyboard,
    Pad {
        slot: u32,
        id: String,
    },
}

/// What hints are drawn from: the controller bindings and the styles the
/// player chose, the pads connected, and the source the player last used.
/// A plain value: the [`crate::Ui`] keeps one, which screens read through
/// [`crate::Cx::hints`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hints {
    controls: ControlBindings,
    styles: BTreeMap<String, Style>,
    pads: Vec<ConnectedPad>,
    source: Source,
}

impl Hints {
    pub fn new(
        controls: ControlBindings,
        styles: BTreeMap<String, Style>,
        pads: Vec<ConnectedPad>,
        source: Source,
    ) -> Hints {
        Hints {
            controls,
            styles,
            pads,
            source,
        }
    }

    /// The source's pad, while it is connected.
    fn pad(&self) -> Option<&ConnectedPad> {
        let Source::Pad { slot, id } = &self.source else {
            return None;
        };
        self.pads.iter().find(|p| p.index == *slot && p.id == *id)
    }

    /// Hints show a controller's controls: a pad is the source, and it is
    /// connected.
    pub fn active(&self) -> bool {
        self.pad().is_some()
    }

    pub fn source(&self) -> &Source {
        &self.source
    }

    pub fn pads(&self) -> &[ConnectedPad] {
        &self.pads
    }

    pub fn controls(&self) -> &ControlBindings {
        &self.controls
    }

    pub fn styles(&self) -> &BTreeMap<String, Style> {
        &self.styles
    }

    /// The connected pad in `slot` whose identity is `id`.
    pub fn pad_at(&self, slot: u32, id: &str) -> Option<&ConnectedPad> {
        self.pads.iter().find(|p| p.index == slot && p.id == id)
    }

    pub(crate) fn with_controls(
        &self,
        controls: ControlBindings,
        styles: BTreeMap<String, Style>,
    ) -> Hints {
        Hints {
            controls,
            styles,
            ..self.clone()
        }
    }

    pub(crate) fn with_pads(&self, pads: Vec<ConnectedPad>, source: Source) -> Hints {
        Hints {
            pads,
            source,
            ..self.clone()
        }
    }

    pub(crate) fn with_source(&self, source: Source) -> Hints {
        Hints {
            source,
            ..self.clone()
        }
    }

    /// One primary control per requested action, combining only D-pad
    /// directions.
    pub fn actions(&self, actions: &[MenuInput]) -> Vec<Keycap> {
        let Some(p) = self.pad() else {
            return Vec::new();
        };
        let f = family(&self.styles, p);
        let mut caps = Vec::new();
        let mut mask = 0;
        for action in actions {
            if let Some(c) = self
                .controls
                .menu_controls(&p.id, p.standard, *action)
                .first()
            {
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
    }

    /// Prefer Confirm; dedicated Start also activates an ordinary menu
    /// when South was consumed by a saved Back binding. The editor
    /// requests Confirm alone.
    pub fn activate(&self) -> Vec<Keycap> {
        let c = self.actions(&[MenuInput::Confirm]);
        if c.is_empty() {
            self.actions(&[MenuInput::Start])
        } else {
            c
        }
    }

    /// The controller's effective Start binding, which cover controls
    /// actually use.
    pub fn cover_instruction(&self) -> Option<String> {
        let p = self.pad()?;
        let b = self.controls.pad(&p.id, p.standard)?;
        let c = b.start.as_ref()?;
        if b.back.as_ref() == Some(c) {
            return None;
        }
        let label = Cap::control(p, family(&self.styles, p), c).label();
        Some(format!(
            "In play, hold {label} and press an up- or down-arrow lane binding to move it; tap {label} to hide or show it."
        ))
    }

    /// The upcoming play's actual assignment under `controls`, including
    /// split two-controller fields.
    pub fn lanes(
        &self,
        layout: &ddi_chart::Layout,
        controls: &ControlBindings,
    ) -> Option<Vec<Option<Keycap>>> {
        let slot = self.pad()?.index;
        let mut out = vec![None; layout.lane_count()];
        let plan = controls.plan(layout, &self.pads);
        let resolved = controls.resolve(layout, &self.pads);
        let split = plan
            .iter()
            .any(|(p, u)| p.index == slot && matches!(u, PadUse::OnePad { .. }));
        // The active controller wins when several can play the whole field.
        for (p, usage) in plan.iter().filter(|(p, _)| p.index == slot).chain(
            plan.iter()
                .filter(|(p, u)| split && p.index != slot && matches!(u, PadUse::OnePad { .. })),
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
                            let mut cap = Cap::control(p, family(&self.styles, p), c);
                            if let PadUse::OnePad { pad, .. } = usage {
                                cap.side = Some(pad + 1);
                            }
                            Keycap::Controller(cap)
                        });
                }
            }
        }
        Some(out)
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

fn family(styles: &BTreeMap<String, Style>, p: &ConnectedPad) -> Family {
    presentation(p, styles.get(&p.id).copied().unwrap_or_default())
}

/// Quit is shown after the actual cancel control was pressed (`raw`, among
/// `pads` drawn in `styles`). Unknown/raw identities use a truthful
/// reference to that same control, never guessed Back.
pub fn quit_hint(
    raw: &RawInput,
    pads: &[ConnectedPad],
    styles: &BTreeMap<String, Style>,
) -> &'static str {
    let Some(p) = pads
        .iter()
        .find(|p| p.index == raw.slot && raw.device == DeviceId::Gamepad(p.id.clone()))
    else {
        return "hold or double-tap this control to quit";
    };
    let cap = Cap::control(p, family(styles, p), &raw.control);
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
    fn quit_names_the_control_pressed_in_its_style() {
        let raw = |id: &str, control: &str| RawInput {
            device: DeviceId::Gamepad(id.into()),
            slot: 0,
            control: control.into(),
            pressed: true,
            host_time: ddi_platform::HostTime(0.0),
        };
        let pads = [pad("Xbox Wireless Controller", true)];
        let mut styles = BTreeMap::new();
        assert_eq!(
            quit_hint(&raw("Xbox Wireless Controller", "button:1"), &pads, &styles),
            "hold or double-tap B to quit"
        );
        // The player's style over the detected one.
        styles.insert("Xbox Wireless Controller".to_string(), Style::PlayStation);
        assert_eq!(
            quit_hint(&raw("Xbox Wireless Controller", "button:1"), &pads, &styles),
            "hold or double-tap Circle to quit"
        );
        // A pad not among those known: that control, unnamed.
        assert_eq!(
            quit_hint(&raw("Other", "button:1"), &pads, &styles),
            "hold or double-tap this control to quit"
        );
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
    fn source(p: &ConnectedPad) -> Source {
        Source::Pad {
            slot: p.index,
            id: p.id.clone(),
        }
    }

    #[test]
    fn effective_bindings_precedence_and_disconnect() {
        let p = pad("Xbox", true);
        let mut c = ControlBindings::default();
        let mut b = ddi_engine::PadBindings::standard(&p.id);
        b.back = Some("button:2".into());
        c.pads.push(b);
        let h = Hints::new(c.clone(), BTreeMap::new(), vec![p.clone()], source(&p));
        assert!(h.actions(&[MenuInput::More]).is_empty());
        assert_eq!(h.actions(&[MenuInput::Back])[0].label(), "B");
        let l = h.lanes(&ddi_chart::Layout::dance_single(), &c).unwrap();
        assert_eq!(l[0].as_ref().unwrap().label(), "LB");
        let keys = Hints::new(
            c.clone(),
            BTreeMap::new(),
            vec![p.clone()],
            Source::Keyboard,
        );
        assert!(!keys.active() && keys.actions(&[MenuInput::Back]).is_empty());
        // Its slot taken by another pad: not active.
        let gone = Hints::new(c, BTreeMap::new(), vec![pad("Other", true)], source(&p));
        assert!(!gone.active());
    }

    #[test]
    fn diagrams_use_effective_custom_raw_and_split_tables() {
        let p = pad("Xbox", true);
        let other = ConnectedPad {
            index: 1,
            ..pad("DualSense", true)
        };
        let mut c = ControlBindings::default();
        let mut b = ddi_engine::PadBindings::standard(&p.id);
        b.single[0] = vec!["button:14".into()];
        b.single[1].clear();
        b.single[2] = vec!["button:8".into(), "button:7".into()]; // Back consumes its own control.
        c.pads.push(b);
        let h = Hints::new(
            c.clone(),
            BTreeMap::new(),
            vec![p.clone(), other],
            source(&p),
        );
        let single = h.lanes(&ddi_chart::Layout::dance_single(), &c).unwrap();
        assert_eq!(single[0].as_ref().unwrap().label(), "←");
        assert!(
            single[1].is_none(),
            "another pad must not hide an explicitly unbound lane"
        );
        assert_eq!(single[2].as_ref().unwrap().label(), "RT");
        let double = h
            .lanes(&ddi_chart::Layout::builtin("dance-double").unwrap(), &c)
            .unwrap();
        assert_eq!(double[4].as_ref().unwrap().label(), "L1");
        assert!(matches!(
            &double[4],
            Some(Keycap::Controller(Cap { side: Some(2), .. }))
        ));
        let raw = pad("Raw adapter", false);
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
        let h = Hints::new(
            c.clone(),
            BTreeMap::from([(raw.id.clone(), Style::Nintendo)]),
            vec![raw.clone()],
            source(&raw),
        );
        let caps = h.lanes(&ddi_chart::Layout::dance_single(), &c).unwrap();
        assert!(caps[..3].iter().all(|c| matches!(
            c,
            Some(Keycap::Controller(Cap {
                glyph: Glyph::Raw(_),
                ..
            }))
        )));
        assert!(caps[3].is_none());
    }
}
