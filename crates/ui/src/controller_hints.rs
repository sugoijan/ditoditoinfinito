//! Presentation of effective controller bindings: [`Hints`], the value the
//! UI core keeps (`docs/plans/controller-input-core.md`), from the
//! bindings, the pads connected and the input the player last used. No
//! input is dispatched here.
use crate::keycaps::Keycap;
use ddi_engine::{ConnectedPad, ControlBindings, MenuInput, PadUse};
use ddi_l10n::UiText;
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
    /// A control of a pad without the standard mapping, by its kind,
    /// number and direction (named in words, [`control_name`]).
    Raw(Control),
    /// A binding that is no control's, as stored.
    Unknown(String),
}

/// A control by its kind, number and direction, in words ("button 3",
/// "axis 1 −", "hat 0 up"): a raw pad's, whose legends the game knows
/// not.
pub fn control_name(control: &Control) -> UiText {
    use ddi_l10n::msg::devices as d;
    use ddi_platform::gamepad::HatDir;
    match *control {
        Control::Button(n) => d::button(n.to_string()),
        Control::Axis { index, positive } => {
            d::axis(index.to_string(), if positive { "plus" } else { "minus" })
        }
        Control::Hat { index, dir } => d::hat(
            index.to_string(),
            match dir {
                HatDir::Up => "up",
                HatDir::Right => "right",
                HatDir::Down => "down",
                HatDir::Left => "left",
            },
        ),
    }
}

/// A pad's binding as stored (`button:3`), named by its kind and number
/// ([`control_name`]); one that is no control's as it is stored (data).
pub fn binding_name(control: &str) -> UiText {
    control
        .parse::<Control>()
        .map_or_else(|_| UiText::data(control), |c| control_name(&c))
}

/// Controls' names as a list (a lane's), joined as the language joins
/// one: the first and the rest, the rest joined so in turn. `None` for
/// none.
pub fn control_list(names: impl IntoIterator<Item = UiText>) -> Option<UiText> {
    let names: Vec<UiText> = names.into_iter().collect();
    names
        .into_iter()
        .rev()
        .reduce(|rest, first| ddi_l10n::msg::devices::list(first, rest))
}
/// What a cap is named by: what the controller prints, or words.
enum Naming {
    Printed(String),
    Words(UiText),
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
            Ok(c) => Glyph::Raw(c),
            Err(_) => Glyph::Unknown(control.into()),
        };
        Self {
            family,
            glyph,
            side: None,
        }
    }

    /// The control as the screens name it: its legend as the controller
    /// prints it (`A`, `LB`, `Cross`, an arrow: data), else in words (a
    /// generic pad's face buttons by their places, a raw control by its
    /// number: messages).
    pub fn name(&self) -> UiText {
        match self.naming() {
            Naming::Printed(s) => UiText::data(s),
            Naming::Words(t) => t,
        }
    }

    /// As [`Self::name`], plain: a cap's legend, or the text beside it.
    pub fn label(&self) -> String {
        match self.naming() {
            Naming::Printed(s) => s,
            Naming::Words(t) => t.into(),
        }
    }

    /// Whether the control is a standard pad's (a face, shoulder or
    /// menu button, a D-pad direction), named after its cap; a raw one
    /// is named by its number.
    pub fn standard(&self) -> bool {
        matches!(self.glyph, Glyph::Button(_) | Glyph::Dpad(_))
    }

    /// A raw button's number, which its round cap prints.
    pub fn raw_button(&self) -> Option<u16> {
        match self.glyph {
            Glyph::Raw(Control::Button(n)) => Some(n),
            _ => None,
        }
    }

    /// A raw control drawn as a dot with its name beside the cap (an
    /// axis's or a hat's direction, a binding that is no control's).
    pub fn named_beside(&self) -> bool {
        matches!(&self.glyph, Glyph::Raw(_) | Glyph::Unknown(_)) && self.raw_button().is_none()
    }

    fn naming(&self) -> Naming {
        match &self.glyph {
            Glyph::Raw(c) => Naming::Words(control_name(c)),
            Glyph::Unknown(s) => Naming::Printed(s.clone()),
            Glyph::Dpad(mask) => Naming::Printed(
                [(1, "↑"), (2, "↓"), (4, "←"), (8, "→")]
                    .into_iter()
                    .filter(|(bit, _)| mask & bit != 0)
                    .map(|(_, s)| s)
                    .collect(),
            ),
            Glyph::Button(n) => {
                use ddi_l10n::msg::devices as d;
                // A generic pad's face buttons, by their places.
                if self.family == Family::Generic {
                    match n {
                        0 => return Naming::Words(d::face_south()),
                        1 => return Naming::Words(d::face_east()),
                        2 => return Naming::Words(d::face_west()),
                        3 => return Naming::Words(d::face_north()),
                        _ => {}
                    }
                }
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
                    // The face buttons (words) are named above.
                    Family::Generic => [
                        "", "", "", "", "LB", "RB", "LT", "RT", "SELECT", "START", "LS", "RS",
                    ],
                };
                if self.family == Family::DualSense && *n == 8 {
                    Naming::Printed("Create".into())
                } else {
                    match labels.get(*n as usize) {
                        Some(label) => Naming::Printed(label.to_string()),
                        None => Naming::Words(control_name(&Control::Button(*n))),
                    }
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
    /// actually use, as its button is named (Options' lane cover row
    /// says how it moves the cover).
    pub fn cover_start(&self) -> Option<UiText> {
        let p = self.pad()?;
        let b = self.controls.pad(&p.id, p.standard)?;
        let c = b.start.as_ref()?;
        if b.back.as_ref() == Some(c) {
            return None;
        }
        Some(Cap::control(p, family(&self.styles, p), c).name())
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

/// A hint the play shows, kept for the run: the play's view and the
/// renderer's options hold it as `&'static str` (both `Copy`, the
/// renderer's shared with SkinCraft), so each text is made once, in the
/// language spoken, and kept; a few dozen at most (a standard pad's
/// controls in each style, the restart's keys, a language or two).
fn kept(text: ddi_l10n::UiText) -> &'static str {
    thread_local! {
        static KEPT: std::cell::RefCell<std::collections::HashSet<&'static str>> =
            Default::default();
    }
    KEPT.with_borrow_mut(|kept| {
        if let Some(s) = kept.get(text.as_str()) {
            return *s;
        }
        let s: &'static str = Box::leak(String::from(text).into_boxed_str());
        kept.insert(s);
        s
    })
}

/// Quit is shown after the actual cancel control was pressed (`raw`, among
/// `pads` drawn in `styles`), named as its cap is (a standard pad's
/// control in its style). Unknown/raw identities use a truthful
/// reference to that same control, never guessed Back.
pub fn quit_hint(
    raw: &RawInput,
    pads: &[ConnectedPad],
    styles: &BTreeMap<String, Style>,
) -> &'static str {
    use ddi_l10n::msg::prompt;
    let named = pads
        .iter()
        .find(|p| p.index == raw.slot && raw.device == DeviceId::Gamepad(p.id.clone()))
        .map(|p| Cap::control(p, family(styles, p), &raw.control))
        .filter(Cap::standard)
        .map(|cap| cap.name());
    kept(match named {
        Some(control) => prompt::play_quit(control),
        None => prompt::play_quit_this(),
    })
}

/// The hint while a pad's Back is held in a multiplayer stage with
/// attacks, where a short press of it changes the attack policy and only
/// holding it quits (`ddi_session::play`).
pub fn quit_hold_hint(
    raw: &RawInput,
    pads: &[ConnectedPad],
    styles: &BTreeMap<String, Style>,
) -> &'static str {
    use ddi_l10n::msg::prompt;
    let named = pads
        .iter()
        .find(|p| p.index == raw.slot && raw.device == DeviceId::Gamepad(p.id.clone()))
        .map(|p| Cap::control(p, family(styles, p), &raw.control))
        .filter(Cap::standard)
        .map(|cap| cap.name());
    kept(match named {
        Some(control) => prompt::play_quit_hold(control),
        None => prompt::play_quit_hold_this(),
    })
}

/// The keys that cycle a stage's attack policy during play
/// (`ddi_session::play`, D8 and the maintainer's decision of 2026-11-13):
/// either Shift, by physical code, and Tab, each unless the player's
/// bindings give it to a lane (it then plays the lane). In the order a
/// hint names them.
pub const POLICY_KEYS: [&str; 3] = ["ShiftLeft", "ShiftRight", "Tab"];

/// The key a hint names for cycling the attack policy: the first of
/// [`POLICY_KEYS`] that `bound` (whether a lane has the code) leaves free;
/// none where all three are a lane's.
pub fn policy_key(bound: impl Fn(&str) -> bool) -> Option<&'static str> {
    POLICY_KEYS.into_iter().find(|k| !bound(k))
}

/// A policy key's cap ([`POLICY_KEYS`]): Shift for either Shift, else Tab.
pub fn policy_keycap(code: &str) -> crate::keycaps::KeyboardKey {
    if code.starts_with("Shift") {
        crate::keycaps::KeyboardKey::Shift
    } else {
        crate::keycaps::KeyboardKey::Tab
    }
}

/// The name of pad `id`'s `control` (in `slot`, among `pads` drawn in
/// `styles`) as its cap shows it, where it is a standard pad's: the
/// attack policy's pad control (its Back) named beside the label in play.
pub fn pad_control_name(
    pads: &[ConnectedPad],
    styles: &BTreeMap<String, Style>,
    id: &str,
    control: &str,
) -> Option<&'static str> {
    pads.iter()
        .find(|p| p.id == id)
        .map(|p| Cap::control(p, family(styles, p), control))
        .filter(Cap::standard)
        .map(|cap| kept(cap.name()))
}

/// The hint after the touch screen's quit button was pressed.
pub fn quit_button_hint() -> &'static str {
    kept(ddi_l10n::msg::prompt::play_quit_button())
}

/// The restart's hint, after its key (`R`, `\`, as its cap prints it) was
/// pressed.
pub fn restart_hint(key: &str) -> &'static str {
    kept(ddi_l10n::msg::prompt::play_restart(key))
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
    fn the_plays_hints_speak_the_language_and_are_made_once() {
        let raw = RawInput {
            device: DeviceId::Gamepad("Xbox Wireless Controller".into()),
            slot: 0,
            control: "button:1".into(),
            pressed: true,
            host_time: ddi_platform::HostTime(0.0),
        };
        let pads = [pad("Xbox Wireless Controller", true)];
        let styles = BTreeMap::new();
        ddi_l10n::set_language("en");
        assert_eq!(quit_button_hint(), "hold or double-tap the quit button");
        assert_eq!(restart_hint("\\"), "hold or double-tap \\ to restart");
        let first = quit_hint(&raw, &pads, &styles);
        assert!(std::ptr::eq(first, quit_hint(&raw, &pads, &styles)));
        ddi_l10n::set_language("ja");
        assert_eq!(
            quit_hint(&raw, &pads, &styles),
            "Bを長押しまたは2回押しで終了"
        );
        assert_eq!(restart_hint("R"), "Rを長押しまたは2回押しでリスタート");
        // A generic pad's face button by its place, in the language; a
        // raw pad's control unnamed.
        let generic = [
            pad("Wireless Controller", true),
            ConnectedPad {
                index: 1,
                ..pad("Raw adapter", false)
            },
        ];
        let press = |id: &str, slot: u32| RawInput {
            device: DeviceId::Gamepad(id.into()),
            slot,
            ..raw.clone()
        };
        assert_eq!(
            quit_hint(&press("Wireless Controller", 0), &generic, &styles),
            "右ボタンを長押しまたは2回押しで終了"
        );
        assert_eq!(
            quit_hint(&press("Raw adapter", 1), &generic, &styles),
            "このボタンを長押しまたは2回押しで終了"
        );
        ddi_l10n::set_language("en");
        assert_eq!(
            quit_hint(&press("Wireless Controller", 0), &generic, &styles),
            "hold or double-tap East to quit"
        );
    }

    #[test]
    fn controls_are_named_in_words_in_the_language() {
        let raw = pad("Raw adapter", false);
        let name = |control: &str| Cap::control(&raw, Family::Xbox, control).label();
        let generic = |n: u16| {
            Cap {
                family: Family::Generic,
                glyph: Glyph::Button(n),
                side: None,
            }
            .label()
        };
        ddi_l10n::set_language("en");
        assert_eq!(name("button:3"), "button 3");
        assert_eq!(name("axis:1-"), "axis 1 −");
        assert_eq!(name("axis:1+"), "axis 1 +");
        assert_eq!(name("hat:0:up"), "hat 0 up");
        assert_eq!(name("hat:0:left"), "hat 0 left");
        // A binding that is no control's, as stored.
        assert_eq!(name("pedal"), "pedal");
        assert_eq!(binding_name("button:1000").as_str(), "button 1000");
        assert_eq!(
            (0..4).map(generic).collect::<Vec<_>>(),
            ["South", "East", "West", "North"]
        );
        // What a generic pad prints, as it prints it.
        assert_eq!(generic(8), "SELECT");
        let list =
            |names: &[&str]| control_list(names.iter().map(|c| binding_name(c))).map(String::from);
        assert_eq!(list(&[]), None);
        assert_eq!(list(&["button:1"]).as_deref(), Some("button 1"));
        assert_eq!(
            list(&["button:1", "axis:0+", "hat:2:down"]).as_deref(),
            Some("button 1, axis 0 +, hat 2 down")
        );
        ddi_l10n::set_language("ja");
        assert_eq!(name("button:3"), "ボタン3");
        assert_eq!(name("axis:1-"), "軸1 −");
        assert_eq!(name("hat:0:up"), "ハット0の上");
        assert_eq!(generic(0), "下ボタン");
        assert_eq!(
            list(&["button:1", "axis:0+"]).as_deref(),
            Some("ボタン1、軸0 +")
        );
        ddi_l10n::set_language("en");
        // The round cap prints a raw button's number; another raw
        // control is a dot with its name beside it.
        let cap = |control: &str| Cap::control(&raw, Family::Xbox, control);
        assert_eq!(cap("button:7").raw_button(), Some(7));
        assert!(!cap("button:7").named_beside());
        assert!(cap("axis:2-").named_beside() && cap("pedal").named_beside());
        let standard = Cap::control(&pad("Xbox", true), Family::Xbox, "button:0");
        assert!(standard.standard() && !standard.named_beside());
        assert_eq!(standard.raw_button(), None);
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
