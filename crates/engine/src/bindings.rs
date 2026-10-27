//! Saved control bindings, per device and layout, and how a play's
//! [`Bindings`] are built from them.
//!
//! The keyboard keeps one table per layout (a layout without one uses its
//! lanes' default keys). Each controller model (`Gamepad.id`) keeps its own
//! tables; controllers with the standard mapping get defaults for the
//! built-in layouts, every other controller is bound with the guided flow.
//! A layout spanning two pads (`dance-double`) is also played with two
//! controllers: each one's bindings of the one-pad layout
//! ([`Layout::one_pad_layout`], `dance-single`) drive one pad's lanes,
//! the lower `Gamepad.index` on the left, as StepMania gives each player
//! side its own controller.
//!
//! This is part of the player's settings: it deserializes from what earlier
//! versions saved (`keys_single`, `pads[].single`), and [`ControlBindings::sanitize`]
//! moves old fields into the new ones.

use std::collections::{BTreeMap, HashSet};

use ddi_chart::Layout;
use ddi_platform::DeviceId;
use ddi_platform::gamepad::Control;
use serde::{Deserialize, Serialize};

use crate::input::Bindings;

/// A press on the song list ([`ControlBindings::menu_input`]).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MenuInput {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    /// Dedicated, possibly rebound Start (distinct from the South face button).
    Start,
    More,
    Previous,
    Next,
    Back,
    /// StepMania's Select: a standard pad's Y (its Back/Select button is
    /// Back).
    Select,
}

/// Controls (or `KeyboardEvent.code` values) per lane, in lane order.
pub type LaneTable = Vec<Vec<String>>;

const SINGLE: &str = "dance-single";

/// Bindings of one controller model (`Gamepad.id`). Controls are
/// [`Control`] strings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PadBindings {
    pub id: String,
    /// `dance-single` controls per lane (L, D, U, R); also what this
    /// controller plays on one pad of a two-pad layout.
    pub single: LaneTable,
    /// Tables of every other layout, by layout id.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub layouts: BTreeMap<String, LaneTable>,
    /// Starts the song from the start prompt.
    pub start: Option<String>,
    /// Quits like Escape (double-tap or hold during play).
    pub back: Option<String>,
    /// Last time the pad was bound or used (ms since epoch).
    pub last_seen: f64,
}

impl Default for PadBindings {
    fn default() -> PadBindings {
        PadBindings {
            id: String::new(),
            single: vec![Vec::new(); 4],
            layouts: BTreeMap::new(),
            start: None,
            back: None,
            last_seen: 0.0,
        }
    }
}

impl PadBindings {
    /// Whether nothing at all is bound (a cleared entry).
    pub fn is_empty(&self) -> bool {
        let empty = |t: &LaneTable| t.iter().all(Vec::is_empty);
        empty(&self.single)
            && self.layouts.values().all(empty)
            && self.start.is_none()
            && self.back.is_none()
    }

    /// Defaults for a pad with the standard mapping: shoulders/triggers,
    /// d-pad and face alternatives, Start and Back. Pads without it have no defaults; their
    /// buttons are numbered arbitrarily.
    pub fn standard(id: &str) -> PadBindings {
        use ddi_platform::gamepad::standard::{BACK, START};
        PadBindings {
            id: id.to_string(),
            single: Self::standard_table(&Layout::dance_single()).unwrap_or_default(),
            start: Some(button(START)),
            back: Some(button(BACK)),
            ..PadBindings::default()
        }
    }

    /// Standard-mapping defaults for a built-in layout: `dance-single` on
    /// LB/LT/RT/RB, with d-pad and face alternatives; `dance-solo` on d-pad/face
    /// buttons with the
    /// shoulder buttons as diagonals, `dance-double` the left pad on the
    /// d-pad and the right pad on the face buttons; the pump layouts alike,
    /// their centres on the shoulders.
    pub fn standard_table(layout: &Layout) -> Option<LaneTable> {
        use ddi_platform::gamepad::standard::*;
        let t = |rows: &[&[u16]]| -> LaneTable {
            rows.iter()
                .map(|r| r.iter().map(|&b| button(b)).collect())
                .collect()
        };
        Some(match layout.id.as_str() {
            "dance-single" => t(&[
                &[LB, DPAD_LEFT, X],
                &[LT, DPAD_DOWN, A],
                &[RT, DPAD_UP, Y],
                &[RB, DPAD_RIGHT, B],
            ]),
            "dance-solo" => t(&[
                &[DPAD_LEFT, X],
                &[LB],
                &[DPAD_DOWN, A],
                &[DPAD_UP, Y],
                &[RB],
                &[DPAD_RIGHT, B],
            ]),
            "techno-single8" => t(&[
                &[LT],
                &[DPAD_LEFT, X],
                &[LB],
                &[DPAD_DOWN, A],
                &[DPAD_UP, Y],
                &[RB],
                &[DPAD_RIGHT, B],
                &[RT],
            ]),
            "dance-double" => t(&[
                &[DPAD_LEFT],
                &[DPAD_DOWN],
                &[DPAD_UP],
                &[DPAD_RIGHT],
                &[X],
                &[A],
                &[Y],
                &[B],
            ]),
            "pump-single" | "pump-halfdouble" | "pump-double" => {
                // Each panel keeps its control on every pump layout: the
                // pad turned an eighth onto the d-pad (left pad) and the
                // face buttons (right pad), the centre on a shoulder;
                // `pump-single` has both.
                let panel = |pad: u8, name: &str| -> Vec<u16> {
                    let sides = match name {
                        "downleft" => [DPAD_LEFT, X],
                        "upleft" => [DPAD_UP, Y],
                        "center" => [LB, RB],
                        "upright" => [DPAD_RIGHT, B],
                        _ => [DPAD_DOWN, A],
                    };
                    if layout.pads() < 2 {
                        sides.to_vec()
                    } else {
                        vec![sides[usize::from(pad)]]
                    }
                };
                layout
                    .lanes
                    .iter()
                    .map(|l| panel(l.pad, &l.name).into_iter().map(button).collect())
                    .collect()
            }
            _ => return None,
        })
    }

    /// The saved table of a layout. An empty table is a deliberate
    /// "nothing bound" (it hides the standard defaults); `None` means the
    /// layout was never bound. `dance-single` always has one, as saved
    /// entries always did.
    pub fn table(&self, layout_id: &str) -> Option<&LaneTable> {
        if layout_id == SINGLE {
            Some(&self.single)
        } else {
            self.layouts.get(layout_id)
        }
    }

    /// Replaces the table of a layout.
    pub fn set_table(&mut self, layout_id: &str, table: LaneTable) {
        if layout_id == SINGLE {
            self.single = table;
        } else {
            self.layouts.insert(layout_id.to_string(), table);
        }
    }

    /// Drops controls that do not parse, keeps each control on one lane of
    /// a table, gives built-in layouts their lane count.
    fn sanitize(&mut self) {
        let valid = |c: &String| c.parse::<Control>().is_ok();
        self.single.resize_with(4, Vec::new);
        for (id, table) in std::iter::once((SINGLE, &mut self.single))
            .chain(self.layouts.iter_mut().map(|(id, t)| (id.as_str(), t)))
        {
            if let Some(l) = Layout::builtin(id) {
                table.resize_with(l.lane_count(), Vec::new);
            }
            let mut seen = HashSet::new();
            for controls in table.iter_mut() {
                controls.retain(|c| valid(c) && seen.insert(c.clone()));
            }
        }
        // Start and Back act before any lane sees a press, so a control on a
        // lane of any table cannot also be one of them (the lane wins: it
        // was bound later, Start and Back are kept across layouts).
        let on_lane = |c: &String| {
            std::iter::once(&self.single)
                .chain(self.layouts.values())
                .flatten()
                .any(|controls| controls.contains(c))
        };
        let clear = |c: &Option<String>| c.as_ref().is_some_and(|c| !valid(c) || on_lane(c));
        if clear(&self.start) {
            self.start = None;
        }
        if clear(&self.back) {
            self.back = None;
        }
        if self.start.is_some() && self.start == self.back {
            self.back = None;
        }
        if !self.last_seen.is_finite() {
            self.last_seen = 0.0;
        }
    }
}

fn button(n: u16) -> String {
    format!("button:{n}")
}

/// A controller connected now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectedPad {
    /// `Gamepad.id`.
    pub id: String,
    /// `Gamepad.index`, the slot its edges carry.
    pub index: u32,
    /// `Gamepad.mapping == "standard"`.
    pub standard: bool,
    /// Structured USB identity, when the shell exposes it. Never part of binding keys.
    pub vendor_product: Option<[u16; 2]>,
}

/// Every saved binding: the keyboard's tables and each controller's.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ControlBindings {
    /// Keyboard codes per lane, by layout id. A layout without an entry
    /// plays on its default keys.
    pub keys: BTreeMap<String, LaneTable>,
    /// The `dance-single` keyboard table where versions before per-layout
    /// tables kept it. Read once into `keys` by [`ControlBindings::sanitize`]
    /// and written back as a mirror of the single table, so a tab of an
    /// older version still open after an update keeps the player's keys.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keys_single: Option<LaneTable>,
    /// Controller bindings, one entry per `Gamepad.id`.
    pub pads: Vec<PadBindings>,
}

impl ControlBindings {
    /// Moves old fields into the new ones and normalises the tables: one
    /// row per lane for built-in layouts, a key or control on one lane of a
    /// table only, tables equal to the defaults dropped, one entry per
    /// controller.
    pub fn sanitize(&mut self) {
        if let Some(single) = self.keys_single.take() {
            self.keys.entry(SINGLE.to_string()).or_insert(single);
        }
        self.keys.retain(|id, table| {
            if let Some(l) = Layout::builtin(id) {
                table.resize_with(l.lane_count(), Vec::new);
            }
            let mut seen = HashSet::new();
            for keys in table.iter_mut() {
                keys.retain(|k| !k.is_empty() && seen.insert(k.clone()));
            }
            Layout::builtin(id).is_none_or(|l| !is_default(id, table, &l))
        });
        let mut ids = HashSet::new();
        self.pads
            .retain(|p| !p.id.is_empty() && ids.insert(p.id.clone()));
        for p in &mut self.pads {
            p.sanitize();
        }
        self.mirror_single();
    }

    fn mirror_single(&mut self) {
        self.keys_single = Some(self.keyboard(&Layout::dance_single()));
    }

    /// The keyboard table of a layout: saved, else the default keys.
    pub fn keyboard(&self, layout: &Layout) -> LaneTable {
        match self.keys.get(&layout.id) {
            Some(t) => {
                let mut t = t.clone();
                t.resize_with(layout.lane_count(), Vec::new);
                t
            }
            None => default_keys(layout),
        }
    }

    /// Whether the keyboard plays a layout on its default keys.
    pub fn keyboard_is_default(&self, layout: &Layout) -> bool {
        !self.keys.contains_key(&layout.id)
    }

    /// Stores the keyboard table of a layout (dropping it when it equals
    /// the defaults).
    pub fn set_keyboard(&mut self, layout: &Layout, table: LaneTable) {
        if table == default_keys(layout) {
            self.keys.remove(&layout.id);
        } else {
            self.keys.insert(layout.id.clone(), table);
        }
        self.mirror_single();
    }

    /// Back to a layout's default keys.
    pub fn reset_keyboard(&mut self, layout: &Layout) {
        self.set_keyboard(layout, default_keys(layout));
    }

    /// Drops a controller's table of one layout, so it plays that layout
    /// on the defaults again (the standard ones, or one pad of a two-pad
    /// layout with its one-pad layout's bindings). The single table of a standard
    /// controller goes back to the standard defaults; of any other
    /// controller it is cleared.
    pub fn reset_pad_table(&mut self, id: &str, layout_id: &str, standard: bool) {
        if let Some(p) = self.pads.iter_mut().find(|p| p.id == id) {
            if layout_id == SINGLE {
                p.single = if standard {
                    PadBindings::standard(id).single
                } else {
                    vec![Vec::new(); 4]
                };
            } else {
                p.layouts.remove(layout_id);
            }
        }
    }

    /// Saved bindings of a controller, else the standard defaults when it
    /// has the standard mapping.
    pub fn pad(&self, id: &str, standard: bool) -> Option<PadBindings> {
        self.pads
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .or_else(|| standard.then(|| PadBindings::standard(id)))
    }

    /// What a controller press means on the song list. A controller with
    /// the standard mapping moves with the d-pad or the left stick,
    /// confirms with A or Start and goes back with B or Back; any other
    /// (a dance pad) moves with the arrows of its `dance-single` table and
    /// confirms and goes back with its Start and Back. Saved Start and Back
    /// controls count for both.
    pub fn menu_input(&self, id: &str, standard: bool, control: &str) -> Option<MenuInput> {
        self.menu_control(id, standard, control).map(|m| match m {
            MenuInput::Start => MenuInput::Confirm,
            other => other,
        })
    }

    /// Menu resolution preserving dedicated Start for contexts such as the editor.
    /// Saved Start/Back take precedence over every positional shortcut.
    pub fn menu_control(&self, id: &str, standard: bool, control: &str) -> Option<MenuInput> {
        use ddi_platform::gamepad::standard::*;
        let pad = self.pad(id, standard);
        if let Some(p) = &pad {
            if p.start.as_deref() == Some(control) {
                return Some(MenuInput::Start);
            }
            if p.back.as_deref() == Some(control) {
                return Some(MenuInput::Back);
            }
        }
        if standard {
            return match control {
                c if c == button(DPAD_UP) || c == "axis:1-" => Some(MenuInput::Up),
                c if c == button(DPAD_DOWN) || c == "axis:1+" => Some(MenuInput::Down),
                c if c == button(DPAD_LEFT) || c == "axis:0-" => Some(MenuInput::Left),
                c if c == button(DPAD_RIGHT) || c == "axis:0+" => Some(MenuInput::Right),
                c if c == button(A) => Some(MenuInput::Confirm),
                c if c == button(START) => Some(MenuInput::Start),
                c if c == button(X) => Some(MenuInput::More),
                c if c == button(LB) => Some(MenuInput::Previous),
                c if c == button(RB) => Some(MenuInput::Next),
                c if c == button(B) || c == button(BACK) => Some(MenuInput::Back),
                c if c == button(Y) => Some(MenuInput::Select),
                _ => None,
            };
        }
        let lanes = [
            MenuInput::Left,
            MenuInput::Down,
            MenuInput::Up,
            MenuInput::Right,
        ];
        pad?.single
            .iter()
            .zip(lanes)
            .find(|(controls, _)| controls.iter().any(|c| c == control))
            .map(|(_, input)| input)
    }

    /// Inverse of dispatch, including saved precedence. Candidates are checked
    /// through `menu_control`, so a stolen default cannot advertise an action.
    pub fn menu_controls(&self, id: &str, standard: bool, action: MenuInput) -> Vec<String> {
        let mut candidates: Vec<String> = if standard {
            (0..17)
                .map(button)
                .chain(["axis:0-", "axis:0+", "axis:1-", "axis:1+"].map(String::from))
                .collect()
        } else {
            Vec::new()
        };
        if let Some(pad) = self.pad(id, standard) {
            candidates.extend(pad.single.into_iter().flatten());
            candidates.extend(pad.start);
            candidates.extend(pad.back);
        }
        let mut seen = HashSet::new();
        candidates
            .into_iter()
            .filter(|c| {
                seen.insert(c.clone()) && self.menu_control(id, standard, c) == Some(action)
            })
            .collect()
    }

    /// Stores a controller's bindings, replacing earlier ones.
    pub fn set_pad(&mut self, mut pad: PadBindings, now_ms: f64) {
        pad.last_seen = now_ms;
        pad.sanitize();
        match self.pads.iter_mut().find(|p| p.id == pad.id) {
            Some(p) => *p = pad,
            None => self.pads.push(pad),
        }
    }

    /// Drops a controller's saved bindings (a controller with the standard
    /// mapping falls back to the defaults).
    pub fn forget_pad(&mut self, id: &str) {
        self.pads.retain(|p| p.id != id);
    }

    /// The table a controller plays a layout with when it has the whole
    /// field to itself: its saved table (possibly cleared), else the
    /// standard defaults.
    pub fn pad_table(&self, id: &str, standard: bool, layout: &Layout) -> Option<LaneTable> {
        match self.saved_table(id, &layout.id) {
            Some(t) => Some(t.clone()),
            None => standard
                .then(|| PadBindings::standard_table(layout))
                .flatten(),
        }
    }

    /// A saved controller's saved table of a layout.
    fn saved_table(&self, id: &str, layout_id: &str) -> Option<&LaneTable> {
        self.pads
            .iter()
            .find(|p| p.id == id)
            .and_then(|p| p.table(layout_id))
    }

    /// How each connected controller plays `layout`, in `Gamepad.index`
    /// order. One-pad layouts: its saved table, else the standard defaults.
    /// Two-pad layouts: a controller with a table of its own for the layout
    /// uses it; of the others, a lone standard controller plays the whole
    /// field on its defaults, otherwise each one with single bindings plays
    /// one pad, the lowest index on the left (controllers beyond the
    /// layout's pads are unused).
    pub fn plan<'a>(
        &self,
        layout: &Layout,
        connected: &'a [ConnectedPad],
    ) -> Vec<(&'a ConnectedPad, PadUse)> {
        let mut connected: Vec<&ConnectedPad> = connected.iter().collect();
        connected.sort_by_key(|c| c.index);
        let table_use = |c: &ConnectedPad| match self.saved_table(&c.id, &layout.id) {
            Some(t) => PadUse::Table {
                table: t.clone(),
                defaults: false,
            },
            None => match self.pad_table(&c.id, c.standard, layout) {
                Some(table) => PadUse::Table {
                    table,
                    defaults: true,
                },
                None => PadUse::Unused,
            },
        };
        if layout.pads() < 2 {
            return connected.into_iter().map(|c| (c, table_use(c))).collect();
        }
        let candidates: Vec<&ConnectedPad> = connected
            .iter()
            .copied()
            .filter(|c| self.saved_table(&c.id, &layout.id).is_none())
            .collect();
        let lone_standard = matches!(candidates.as_slice(), [c] if c.standard);
        let mut next_pad = 0u8;
        connected
            .into_iter()
            .map(|c| {
                let own = self.saved_table(&c.id, &layout.id).is_some();
                let u = if own || lone_standard {
                    table_use(c)
                } else {
                    let one_pad = layout.one_pad_layout();
                    let table = one_pad
                        .as_ref()
                        .and_then(|one| self.pad_table(&c.id, c.standard, one));
                    match (one_pad, table) {
                        (Some(one), Some(table))
                            if table.iter().any(|c| !c.is_empty()) && next_pad < layout.pads() =>
                        {
                            let pad = next_pad;
                            next_pad += 1;
                            // The one-pad table's rows of this pad's lanes.
                            let single = layout
                                .pad_lanes_in(pad, &one)
                                .into_iter()
                                .map(|l| l.and_then(|l| table.get(l).cloned()).unwrap_or_default())
                                .collect();
                            PadUse::OnePad { pad, single }
                        }
                        _ => PadUse::Unused,
                    }
                };
                (c, u)
            })
            .collect()
    }

    /// Bindings for a play of `layout`: the keyboard, every saved
    /// controller with a table for the layout (any slot, so a controller
    /// plugged in during play is ready) and the connected controllers as
    /// [`ControlBindings::plan`] uses them.
    pub fn resolve(&self, layout: &Layout, connected: &[ConnectedPad]) -> Bindings {
        let mut b = Bindings::default();
        let lane_of = |lane: usize| lane as u8;
        for (lane, codes) in self.keyboard(layout).iter().enumerate() {
            for code in codes {
                b.bind(&DeviceId::Keyboard, code, lane_of(lane));
            }
        }
        let bind_table = |b: &mut Bindings, id: &str, slot: Option<u32>, table: &LaneTable| {
            let device = DeviceId::Gamepad(id.to_string());
            for (lane, controls) in table.iter().enumerate() {
                for c in controls {
                    b.bind_slot(&device, slot, c, lane_of(lane));
                }
            }
        };
        for pad in &self.pads {
            if let Some(t) = pad.table(&layout.id) {
                bind_table(&mut b, &pad.id, None, t);
            }
        }
        for (c, u) in self.plan(layout, connected) {
            match u {
                PadUse::Table {
                    defaults: true,
                    table,
                } => {
                    bind_table(&mut b, &c.id, None, &table);
                }
                PadUse::OnePad { pad, single } => {
                    let lanes: Vec<usize> = layout.lanes_of_pad(pad).collect();
                    let device = DeviceId::Gamepad(c.id.clone());
                    for (i, controls) in single.iter().enumerate() {
                        let Some(&lane) = lanes.get(i) else { break };
                        for ctl in controls {
                            b.bind_slot(&device, Some(c.index), ctl, lane_of(lane));
                        }
                    }
                }
                // Saved tables are bound above.
                PadUse::Table { .. } | PadUse::Unused => {}
            }
        }
        b
    }
}

/// How a connected controller plays a layout ([`ControlBindings::plan`]).
#[derive(Clone, Debug, PartialEq)]
pub enum PadUse {
    /// The whole field with this table: saved, or the standard defaults.
    Table { table: LaneTable, defaults: bool },
    /// One pad of a two-pad layout, with the controller's bindings of the
    /// layout's one-pad layout ([`Layout::one_pad_layout`]): `single` has
    /// the rows of the pad's lanes, in order.
    OnePad { pad: u8, single: LaneTable },
    /// Nothing.
    Unused,
}

impl PadUse {
    /// Whether any control plays a lane.
    pub fn plays(&self) -> bool {
        match self {
            PadUse::Table { table, .. } => table.iter().any(|c| !c.is_empty()),
            PadUse::OnePad { .. } => true,
            PadUse::Unused => false,
        }
    }
}

/// Layouts whose default keys gained the number pad on 2026-10-27 (the
/// pump and techno layouts had it from the start). Every version writes
/// the single table (`keys_single`), so a player who never bound keys
/// comes with the defaults of the version that saved them: a saved table
/// equal to a layout's defaults without their numpad keys is the defaults
/// too, and takes the number pad.
const NUMPAD_ADDED: [&str; 6] = [
    "dance-single",
    "dance-solo",
    "dance-double",
    "danoni-5",
    "danoni-7",
    "danoni-9B",
];

/// Whether a saved keyboard table of layout `id` is its defaults, of this
/// version or of one before the number pad.
fn is_default(id: &str, table: &LaneTable, layout: &Layout) -> bool {
    let defaults = default_keys(layout);
    *table == defaults || NUMPAD_ADDED.contains(&id) && *table == without_numpad(&defaults)
}

/// A table less its number pad keys.
fn without_numpad(table: &LaneTable) -> LaneTable {
    table
        .iter()
        .map(|keys| {
            keys.iter()
                .filter(|k| !k.starts_with("Numpad"))
                .cloned()
                .collect()
        })
        .collect()
}

/// A layout's default keys, per lane.
pub fn default_keys(layout: &Layout) -> LaneTable {
    layout
        .lanes
        .iter()
        .map(|l| l.default_keys.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_inputs() {
        let mut b = ControlBindings::default();
        // A standard pad: d-pad, stick, A/Start, B/Back, whatever its lanes.
        assert_eq!(b.menu_input("x", true, "button:12"), Some(MenuInput::Up));
        assert_eq!(b.menu_input("x", true, "axis:0+"), Some(MenuInput::Right));
        assert_eq!(
            b.menu_input("x", true, "button:0"),
            Some(MenuInput::Confirm)
        );
        assert_eq!(b.menu_input("x", true, "button:8"), Some(MenuInput::Back));
        assert_eq!(b.menu_input("x", true, "button:3"), Some(MenuInput::Select));
        assert_eq!(b.menu_input("x", true, "button:2"), Some(MenuInput::More));
        assert_eq!(
            b.menu_control("x", true, "button:9"),
            Some(MenuInput::Start)
        );
        assert_eq!(
            b.menu_control("x", true, "button:4"),
            Some(MenuInput::Previous)
        );
        assert_eq!(b.menu_control("x", true, "button:5"), Some(MenuInput::Next));
        let mut overridden = PadBindings::standard("overridden");
        overridden.start = Some("button:2".into());
        overridden.back = Some("button:5".into());
        overridden.single.clear();
        overridden.layouts.clear();
        b.set_pad(overridden, 0.0);
        assert_eq!(
            b.menu_control("overridden", true, "button:2"),
            Some(MenuInput::Start)
        );
        assert_eq!(
            b.menu_control("overridden", true, "button:5"),
            Some(MenuInput::Back)
        );
        assert_eq!(
            b.menu_control("overridden", true, "button:12"),
            Some(MenuInput::Up)
        );
        // A dance pad: its arrows and its Start and Back.
        let mut pad = PadBindings {
            id: "mat".into(),
            single: vec![
                vec!["button:4".into()],
                vec!["button:6".into()],
                vec!["button:5".into()],
                vec!["button:7".into()],
            ],
            start: Some("button:9".into()),
            back: Some("button:2".into()),
            ..PadBindings::default()
        };
        b.set_pad(pad.clone(), 0.0);
        assert_eq!(b.menu_input("mat", false, "button:5"), Some(MenuInput::Up));
        assert_eq!(
            b.menu_input("mat", false, "button:7"),
            Some(MenuInput::Right)
        );
        assert_eq!(
            b.menu_input("mat", false, "button:9"),
            Some(MenuInput::Confirm)
        );
        assert_eq!(
            b.menu_input("mat", false, "button:2"),
            Some(MenuInput::Back)
        );
        assert_eq!(b.menu_input("mat", false, "button:0"), None);
        // Unbound pads do nothing.
        assert_eq!(b.menu_input("other", false, "button:5"), None);
        pad.back = None;
        b.set_pad(pad, 0.0);
        assert_eq!(b.menu_input("mat", false, "button:2"), None);
    }
    use ddi_platform::{HostTime, RawInput};

    fn pad(id: &str, index: u32, standard: bool) -> ConnectedPad {
        ConnectedPad {
            vendor_product: None,
            id: id.into(),
            index,
            standard,
        }
    }

    fn press(b: &Bindings, device: DeviceId, slot: u32, control: &str) -> Option<u8> {
        b.resolve(&RawInput {
            device,
            control: control.into(),
            pressed: true,
            host_time: HostTime(0.0),
            slot,
        })
        .map(|e| e.lane)
    }

    fn kb(b: &Bindings, code: &str) -> Option<u8> {
        press(b, DeviceId::Keyboard, 0, code)
    }

    /// Settings as the app saved them before per-layout tables: the
    /// `dance-single` keyboard table and pads with `single` only, next to
    /// unrelated fields.
    const OLD_SETTINGS: &str = r#"{
        "speed": 3.0,
        "keys_single": [["ArrowLeft","KeyD"],["ArrowDown","KeyF"],["ArrowUp","KeyJ"],["ArrowRight","KeyK","KeyL"]],
        "pads": [{"id":"Dance pad (Vendor: 0b43 Product: 0003)",
                  "single":[["button:2"],["button:1"],["button:0"],["button:3"]],
                  "start":"button:9","back":null,"last_seen":1.0}],
        "fast_pad_poll": true
    }"#;

    /// Mirrors how the app's settings embed the bindings.
    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    struct Outer {
        speed: f64,
        #[serde(deserialize_with = "crate::lenient")]
        turn: crate::Turn,
        #[serde(flatten)]
        controls: ControlBindings,
        fast_pad_poll: bool,
    }

    #[test]
    fn old_settings_load_and_migrate() {
        let mut o: Outer = serde_json::from_str(OLD_SETTINGS).unwrap();
        assert_eq!(o.speed, 3.0);
        assert!(o.fast_pad_poll);
        o.controls.sanitize();
        let single = Layout::dance_single();
        assert!(!o.controls.keyboard_is_default(&single));
        assert_eq!(
            o.controls.keyboard(&single)[3],
            ["ArrowRight", "KeyK", "KeyL"]
        );
        // Still written, mirroring the single table, for older versions.
        assert_eq!(
            o.controls.keys_single.as_ref(),
            Some(&o.controls.keyboard(&single))
        );
        let saved = serde_json::to_string(&o).unwrap();
        let mut again: Outer = serde_json::from_str(&saved).unwrap();
        assert_eq!(again, o);
        // Loading what we saved changes nothing (no save loop).
        again.controls.sanitize();
        assert_eq!(again, o);
        // An older version that kept only `keys_single` (it drops `keys`)
        // hands the player's single keys back.
        let old_tab = saved.replace(r#""keys":{"dance-single""#, r#""ignored":{"x""#);
        let mut back: Outer = serde_json::from_str(&old_tab).unwrap();
        back.controls.sanitize();
        assert_eq!(
            back.controls.keyboard(&single),
            o.controls.keyboard(&single)
        );
        // A pad saved with `single` only plays single as before.
        let id = "Dance pad (Vendor: 0b43 Product: 0003)";
        let b = o.controls.resolve(&single, &[pad(id, 0, false)]);
        assert_eq!(
            press(&b, DeviceId::Gamepad(id.into()), 0, "button:2"),
            Some(0)
        );
        // Old default keys migrate to "no table" (defaults).
        let mut d: ControlBindings = serde_json::from_str(
            r#"{"keys_single":[["ArrowLeft","KeyD"],["ArrowDown","KeyF"],["ArrowUp","KeyJ"],["ArrowRight","KeyK"]]}"#,
        )
        .unwrap();
        d.sanitize();
        assert!(d.keys.is_empty());
        // So do tables saved by versions before the number pad joined the
        // defaults, for every layout that gained it; a key added or
        // removed keeps the table.
        for id in NUMPAD_ADDED {
            let layout = Layout::builtin(id).unwrap();
            let before = without_numpad(&default_keys(&layout));
            assert_ne!(before, default_keys(&layout), "{id}");
            let mut c = ControlBindings::default();
            c.keys.insert(id.to_string(), before.clone());
            c.sanitize();
            assert!(c.keys.is_empty(), "{id}");
            assert_eq!(c.keyboard(&layout), default_keys(&layout), "{id}");
            let mut custom = before;
            custom[0].push("KeyZ".into());
            c.keys.insert(id.to_string(), custom.clone());
            c.sanitize();
            assert_eq!(c.keys.get(id), Some(&custom), "{id}");
        }
        // Pump's number pad was always there: a table without it is the
        // player's own.
        let pump = Layout::pump_single();
        let mut c = ControlBindings::default();
        let stripped = without_numpad(&default_keys(&pump));
        c.keys.insert(pump.id.clone(), stripped.clone());
        c.sanitize();
        assert_eq!(c.keys.get(&pump.id), Some(&stripped));
    }

    #[test]
    fn start_and_back_never_share_a_lane_control() {
        let mut c = ControlBindings::default();
        // A standard pad rebinds Right to its Back button (8): Back goes.
        let mut p = c.pad("x", true).unwrap();
        p.single[3] = vec!["button:8".into()];
        c.set_pad(p, 0.0);
        let p = c.pad("x", true).unwrap();
        assert_eq!(p.back, None);
        assert_eq!(p.start.as_deref(), Some("button:9"));
        // A lane of another layout counts too.
        let mut p = p;
        p.set_table("dance-solo", vec![vec!["button:9".into()]]);
        c.set_pad(p, 0.0);
        assert_eq!(c.pad("x", true).unwrap().start, None);
    }

    #[test]
    fn plan_matches_what_is_bound() {
        let mut c = ControlBindings::default();
        let double = Layout::dance_double();
        // A standard controller whose single table was cleared still plays
        // double alone on the standard defaults (the clearing hid single's
        // defaults only).
        let mut p = c.pad("x", true).unwrap();
        p.single = vec![Vec::new(); 4];
        c.set_pad(p, 0.0);
        let alone = [pad("x", 0, true)];
        let plan = c.plan(&double, &alone);
        assert!(matches!(&plan[0].1, PadUse::Table { defaults: true, .. }));
        let b = c.resolve(&double, &[pad("x", 0, true)]);
        assert_eq!(
            press(&b, DeviceId::Gamepad("x".into()), 0, "button:2"),
            Some(4)
        );
        // Two standard controllers: one pad each, by index.
        let c = ControlBindings::default();
        let two = [pad("b", 5, true), pad("a", 2, true)];
        let plan = c.plan(&double, &two);
        assert_eq!(plan[0].0.id, "a");
        assert!(matches!(plan[0].1, PadUse::OnePad { pad: 0, .. }));
        assert!(matches!(plan[1].1, PadUse::OnePad { pad: 1, .. }));
        // A third one is unused.
        let three = [pad("a", 0, true), pad("b", 1, true), pad("c", 2, true)];
        assert_eq!(c.plan(&double, &three)[2].1, PadUse::Unused);
        // A non-standard controller without bindings plays nothing.
        let unbound = [pad("n", 0, false)];
        assert!(!c.plan(&double, &unbound)[0].1.plays());
    }

    #[test]
    fn reset_pad_table_restores_the_defaults() {
        let mut c = ControlBindings::default();
        let solo = Layout::dance_solo();
        let mut p = c.pad("x", true).unwrap();
        p.set_table(&solo.id, vec![Vec::new(); 6]);
        p.single = vec![vec!["button:0".into()]; 1];
        c.set_pad(p, 0.0);
        c.reset_pad_table("x", &solo.id, true);
        c.reset_pad_table("x", "dance-single", true);
        let p = c.pad("x", true).unwrap();
        assert_eq!(p.table(&solo.id), None);
        assert_eq!(p.single, PadBindings::standard("x").single);
    }

    #[test]
    fn keyboard_tables_per_layout() {
        let mut c = ControlBindings::default();
        let solo = Layout::dance_solo();
        let b = c.resolve(&solo, &[]);
        assert_eq!(kb(&b, "KeyD"), Some(1));
        assert_eq!(kb(&b, "ArrowRight"), Some(5));
        let mut t = c.keyboard(&solo);
        t[1] = vec!["KeyE".into()];
        c.set_keyboard(&solo, t);
        let b = c.resolve(&solo, &[]);
        assert_eq!(kb(&b, "KeyE"), Some(1));
        assert_eq!(kb(&b, "KeyD"), None);
        // Single is untouched; resetting to the defaults drops the table.
        assert!(c.keyboard_is_default(&Layout::dance_single()));
        c.set_keyboard(&solo, default_keys(&solo));
        assert!(c.keys.is_empty());
        // Tables of unknown (song-defined) layouts are kept as saved.
        c.keys.insert("danoni-x".into(), vec![vec!["KeyQ".into()]]);
        c.sanitize();
        assert!(c.keys.contains_key("danoni-x"));
    }

    #[test]
    fn standard_controllers_get_defaults_per_layout() {
        let c = ControlBindings::default();
        let x = DeviceId::Gamepad("x".into());
        let b = c.resolve(&Layout::dance_solo(), &[pad("x", 0, true)]);
        assert_eq!(press(&b, x.clone(), 0, "button:4"), Some(1));
        assert_eq!(press(&b, x.clone(), 0, "button:5"), Some(4));
        assert_eq!(press(&b, x.clone(), 0, "button:14"), Some(0));
        // Non-standard controllers have no defaults.
        let b = c.resolve(&Layout::dance_solo(), &[pad("y", 1, false)]);
        assert!(
            b.bindings
                .iter()
                .all(|b| b.device == crate::BindingDevice::Keyboard)
        );
        // A lone standard controller on double: d-pad left pad, face right.
        let b = c.resolve(&Layout::dance_double(), &[pad("x", 0, true)]);
        assert_eq!(press(&b, x.clone(), 0, "button:14"), Some(0));
        assert_eq!(press(&b, x.clone(), 0, "button:2"), Some(4));
        assert_eq!(press(&b, x.clone(), 0, "button:0"), Some(5));
        assert_eq!(press(&b, x, 0, "button:1"), Some(7));
    }

    #[test]
    fn two_identical_pads_play_double_one_pad_each() {
        let id = "Dance pad";
        let dev = DeviceId::Gamepad(id.into());
        let mut c = ControlBindings::default();
        c.set_pad(
            PadBindings {
                id: id.into(),
                single: vec![
                    vec!["button:2".into()],
                    vec!["button:1".into()],
                    vec!["button:0".into()],
                    vec!["button:3".into()],
                ],
                ..PadBindings::default()
            },
            0.0,
        );
        let double = Layout::dance_double();
        let b = c.resolve(&double, &[pad(id, 3, false), pad(id, 1, false)]);
        // Lower index on the left.
        assert_eq!(press(&b, dev.clone(), 1, "button:2"), Some(0));
        assert_eq!(press(&b, dev.clone(), 3, "button:2"), Some(4));
        assert_eq!(press(&b, dev.clone(), 3, "button:3"), Some(7));
        // The same control on both pads at once holds two lanes.
        let mut li = crate::LaneInput::new(b);
        let edge = |slot, pressed| RawInput {
            device: dev.clone(),
            control: "button:0".into(),
            pressed,
            host_time: HostTime(0.0),
            slot,
        };
        assert_eq!(li.feed(&edge(1, true)).map(|e| e.lane), Some(2));
        assert_eq!(li.feed(&edge(3, true)).map(|e| e.lane), Some(6));
        assert_eq!(li.feed(&edge(1, false)).map(|e| e.lane), Some(2));
        assert!(li.lane_held(6));
        // One pad alone plays the left pad.
        let b = c.resolve(&double, &[pad(id, 0, false)]);
        assert_eq!(press(&b, dev.clone(), 0, "button:3"), Some(3));
        // A table saved for double itself wins over the split.
        let mut whole = c.pad(id, false).unwrap();
        whole.set_table(
            "dance-double",
            (0..8).map(|i| vec![format!("button:{}", 10 + i)]).collect(),
        );
        c.set_pad(whole, 1.0);
        let b = c.resolve(&double, &[pad(id, 3, false), pad(id, 1, false)]);
        assert_eq!(press(&b, dev.clone(), 3, "button:17"), Some(7));
        assert_eq!(press(&b, dev, 1, "button:2"), None);
    }

    #[test]
    fn pump_pads_split_by_panel_names() {
        use ddi_platform::gamepad::standard::*;
        let single = PadBindings::standard_table(&Layout::pump_single()).unwrap();
        assert_eq!(single[2], [button(LB), button(RB)]);
        let half = Layout::pump_halfdouble();
        let table = PadBindings::standard_table(&half).unwrap();
        // The left pad's centre and the right pad's: one shoulder each.
        assert_eq!(table[0], [button(LB)]);
        assert_eq!(table[5], [button(RB)]);
        assert_eq!(table[3], [button(X)]);
        // Two standard controllers: each plays one pad with its
        // `pump-single` rows of that pad's panels.
        let c = ControlBindings::default();
        let pads = [pad("a", 0, true), pad("b", 1, true)];
        let plan = c.plan(&half, &pads);
        assert_eq!(
            plan[0].1,
            PadUse::OnePad {
                pad: 0,
                single: vec![single[2].clone(), single[3].clone(), single[4].clone()],
            }
        );
        assert_eq!(
            plan[1].1,
            PadUse::OnePad {
                pad: 1,
                single: vec![single[0].clone(), single[1].clone(), single[2].clone()],
            }
        );
    }

    #[test]
    fn pad_tables_are_sanitized() {
        let mut c = ControlBindings::default();
        let mut p = PadBindings {
            id: "p".into(),
            ..PadBindings::default()
        };
        p.set_table(
            "dance-solo",
            vec![
                vec!["button:1".into(), "nonsense".into()],
                vec!["button:1".into()],
            ],
        );
        p.set_table("dance-double", vec![vec![]]);
        c.set_pad(p, 5.0);
        let p = c.pad("p", false).unwrap();
        let solo = p.table("dance-solo").unwrap();
        assert_eq!(solo.len(), 6);
        assert_eq!(solo[0], ["button:1"]);
        assert!(solo[1].is_empty());
        assert_eq!(p.table("dance-double").map(Vec::len), Some(8));
        assert_eq!(p.last_seen, 5.0);
        assert!(!p.is_empty());
        assert!(PadBindings::default().is_empty());
    }

    #[test]
    fn a_cleared_table_hides_the_standard_defaults() {
        let mut c = ControlBindings::default();
        let solo = Layout::dance_solo();
        let x = DeviceId::Gamepad("x".into());
        let mut p = c.pad("x", true).unwrap();
        p.set_table(&solo.id, vec![Vec::new(); 6]);
        c.set_pad(p, 0.0);
        let b = c.resolve(&solo, &[pad("x", 0, true)]);
        assert_eq!(press(&b, x.clone(), 0, "button:4"), None);
        // Other layouts keep their defaults; single keeps the saved table.
        let b = c.resolve(&Layout::dance_single(), &[pad("x", 0, true)]);
        assert_eq!(press(&b, x, 0, "button:14"), Some(0));
    }
}

#[cfg(test)]
mod controller_primary_tests {
    use super::*;
    #[test]
    fn primary_shoulders_leave_old_tables_and_explicit_empties_intact() {
        let layout = Layout::dance_single();
        let fresh = PadBindings::standard("pad");
        assert_eq!(
            fresh
                .single
                .iter()
                .map(|r| r[0].as_str())
                .collect::<Vec<_>>(),
            ["button:4", "button:6", "button:7", "button:5"]
        );
        let mut controls = ControlBindings::default();
        let old = vec![
            vec!["button:14".into(), "button:2".into()],
            vec![],
            vec!["axis:3+".into()],
            vec!["button:15".into()],
        ];
        controls.pads.push(PadBindings {
            single: old.clone(),
            ..fresh
        });
        let mut loaded: ControlBindings =
            serde_json::from_str(&serde_json::to_string(&controls).unwrap()).unwrap();
        loaded.sanitize();
        assert_eq!(loaded.pad_table("pad", true, &layout), Some(old));
        loaded.reset_pad_table("pad", &layout.id, true);
        assert_eq!(
            loaded.pad_table("pad", true, &layout),
            PadBindings::standard_table(&layout)
        );
    }
    #[test]
    fn every_advertised_menu_control_reaches_its_action_after_conflicts() {
        let mut bindings = ControlBindings::default();
        let mut pad = PadBindings::standard("pad");
        pad.start = Some("button:4".into());
        pad.back = Some("button:0".into());
        bindings.pads.push(pad);
        for action in [
            MenuInput::Up,
            MenuInput::Down,
            MenuInput::Left,
            MenuInput::Right,
            MenuInput::Confirm,
            MenuInput::Start,
            MenuInput::Back,
            MenuInput::Previous,
            MenuInput::Next,
            MenuInput::More,
            MenuInput::Select,
        ] {
            for c in bindings.menu_controls("pad", true, action) {
                assert_eq!(bindings.menu_control("pad", true, &c), Some(action));
            }
        }
        assert!(
            bindings
                .menu_controls("pad", true, MenuInput::Previous)
                .is_empty()
        );
        assert!(
            bindings
                .menu_controls("pad", true, MenuInput::Confirm)
                .is_empty()
        );
    }
}
