//! Lane layouts ("steps types", key modes) as data.
//!
//! A [`Layout`] says everything the game needs to know about a chart's
//! lanes: how each lane is drawn and named, where it sits, how lanes may be
//! permuted, which physical pad drives it and which keys play it by
//! default. Charts refer to layouts by id; built-in ones come from
//! [`Layout::builtin`], and a song file may define its own
//! ([`Song::layouts`](crate::Song::layouts)), resolved with
//! [`Song::layout_of`](crate::Song::layout_of).
//!
//! Every played layout's default keys end with the number pad, a pad seen
//! from above: 4 2 8 6 for the four arrows, 7 and 9 for the up
//! diagonals, 1 and 3 for the down ones, 5 for a middle panel or the
//! onigiri (and, on a two-pad layout, the right pad). Its keys come after
//! each lane's main ones, as a second set. `Numpad*` codes are the number
//! pad's alone, never the number row's, on both shells.

use serde::{Deserialize, Serialize};

pub mod danoni;
mod danoni_table;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Glyph {
    /// Arrow pointing in `rotation_deg` where 0 = left, -90 = down, 90 = up, 180 = right.
    Arrow {
        rotation_deg: f32,
    },
    /// The middle panel of a five-panel pad (StepMania's pump `Center`).
    Center,
    Onigiri,
    Giko,
    Iyo,
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lane {
    /// Stable short name (`left`, `upleft`, a Dancing☆Onigiri data name).
    pub name: String,
    /// Name shown to players ("Up-left"); falls back to `name` when empty.
    #[serde(default)]
    pub label: String,
    pub glyph: Glyph,
    pub color_group: u8,
    /// Lanes are only permuted within their group (Mirror, Shuffle).
    pub shuffle_group: u8,
    /// +1 normal, -1 reversed relative to the field.
    pub scroll_sign: i8,
    /// Horizontal position in lane widths from the field's left edge.
    pub column: f32,
    pub row: u8,
    /// Physical pad driving the lane, from 0 (`dance-double`: the left
    /// pad's four lanes are 0, the right pad's are 1).
    #[serde(default)]
    pub pad: u8,
    /// Default bindings as `KeyboardEvent.code` values.
    pub default_keys: Vec<String>,
}

impl Lane {
    /// Name shown to players.
    pub fn label(&self) -> &str {
        if self.label.is_empty() {
            &self.name
        } else {
            &self.label
        }
    }
}

/// Which game a layout comes from; picks defaults that differ between them
/// (the ruleset under "each chart's own rules", the speed under per-family
/// speed settings).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LayoutFamily {
    #[default]
    StepMania,
    Danoni,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub id: String,
    pub name: String,
    pub players: u8,
    #[serde(default)]
    pub family: LayoutFamily,
    pub lanes: Vec<Lane>,
    /// The lane mapping of a quarter turn counter-clockwise ("Left"),
    /// `take_from[new_lane] = old_lane` as StepMania's `GetTrackMapping`;
    /// "Right" is its inverse. `None`: the layout has no such turn.
    #[serde(default)]
    pub turn_left: Option<Vec<u8>>,
}

/// Dancing☆Onigiri key modes the game plays: the ones close to four lanes
/// whose lanes are arrows and the onigiri (decision 27; 7i's giko and iyo
/// lanes left it on 2026-10-13). The rest of danoniplus's modes are kept
/// as data only.
pub const PLAYED_DANONI_MODES: [&str; 4] = ["5", "7", "9A", "9B"];

/// Key modes the game played once: their charts keep their places in a
/// work's songs, unplayable, since works stored before refer to charts by
/// position (`docs/plans/danoni-skins.md`).
pub const FORMER_DANONI_MODES: [&str; 1] = ["7i"];

/// Ids of the built-in StepMania layouts, in the order the game lists them
/// (the Dancing☆Onigiri key modes follow, see [`Layout::builtin_ids`]).
pub const BUILTIN_LAYOUTS: [&str; 7] = [
    "dance-single",
    "dance-solo",
    "dance-double",
    "pump-single",
    "pump-halfdouble",
    "pump-double",
    "techno-single8",
];

/// A five-panel pad's lanes in StepMania's track order (`pump`'s buttons,
/// `src/GameManager.cpp`): name, label, glyph.
const PUMP_PANELS: [(&str, &str, Glyph); 5] = [
    (
        "downleft",
        "Down-left",
        Glyph::Arrow {
            rotation_deg: -45.0,
        },
    ),
    ("upleft", "Up-left", Glyph::Arrow { rotation_deg: 45.0 }),
    ("center", "Centre", Glyph::Center),
    (
        "upright",
        "Up-right",
        Glyph::Arrow {
            rotation_deg: 135.0,
        },
    ),
    (
        "downright",
        "Down-right",
        Glyph::Arrow {
            rotation_deg: 225.0,
        },
    ),
];

/// Default keys of each pad's panels, in [`PUMP_PANELS`] order: a 3×3
/// block per hand (Q E / S / Z C, the corners and the middle), and the
/// number pad's as StepMania's second player has them.
const PUMP_KEYS: [[&[&str]; 5]; 2] = [
    [&["KeyZ"], &["KeyQ"], &["KeyS"], &["KeyE"], &["KeyC"]],
    [
        &["KeyM", "Numpad1"],
        &["KeyU", "Numpad7"],
        &["KeyK", "Numpad5"],
        &["KeyO", "Numpad9"],
        &["Period", "Numpad3"],
    ],
];

impl Layout {
    pub fn lane_count(&self) -> usize {
        self.lanes.len()
    }

    /// Number of physical pads the layout spans (at least 1).
    pub fn pads(&self) -> u8 {
        self.lanes.iter().map(|l| l.pad).max().map_or(1, |p| p + 1)
    }

    /// The one-pad layout whose bindings play each pad of this two-pad
    /// layout, as each player side has its own controller in StepMania:
    /// `dance-single` for `dance-double`, `pump-single` for the pump
    /// layouts on two pads. A pad's lanes are that layout's lanes of the
    /// same names ([`Layout::pad_lanes_in`]).
    pub fn one_pad_layout(&self) -> Option<Layout> {
        match self.id.as_str() {
            "dance-double" => Some(Self::dance_single()),
            "pump-halfdouble" | "pump-double" => Some(Self::pump_single()),
            _ => None,
        }
    }

    /// For each lane of `pad`, in order, the lane of `one_pad` with its
    /// name (`None` where it has none).
    pub fn pad_lanes_in(&self, pad: u8, one_pad: &Layout) -> Vec<Option<usize>> {
        self.lanes_of_pad(pad)
            .map(|i| {
                one_pad
                    .lanes
                    .iter()
                    .position(|l| l.name == self.lanes[i].name)
            })
            .collect()
    }

    /// Lanes of one pad, in order.
    pub fn lanes_of_pad(&self, pad: u8) -> impl Iterator<Item = usize> + '_ {
        (0..self.lanes.len()).filter(move |&i| self.lanes[i].pad == pad)
    }

    fn arrow(name: &str, label: &str, rot: f32, col: f32, keys: &[&str]) -> Lane {
        Lane {
            name: name.into(),
            label: label.into(),
            glyph: Glyph::Arrow { rotation_deg: rot },
            color_group: 0,
            shuffle_group: 0,
            scroll_sign: 1,
            column: col,
            row: 0,
            pad: 0,
            default_keys: keys.iter().map(|k| k.to_string()).collect(),
        }
    }

    /// A plain field of `lanes` left arrows for charts whose layout is not
    /// known: no default keys, no turns besides Mirror and Shuffle.
    pub fn generic(lanes: usize) -> Layout {
        Layout {
            id: format!("generic-{lanes}"),
            name: format!("{lanes} lanes"),
            players: 1,
            family: LayoutFamily::StepMania,
            lanes: (0..lanes)
                .map(|i| {
                    let name = format!("lane {}", i + 1);
                    Self::arrow(&name, &name, 0.0, i as f32, &[])
                })
                .collect(),
            turn_left: None,
        }
    }

    /// StepMania `dance-single`: L, D, U, R.
    pub fn dance_single() -> Layout {
        Layout {
            id: "dance-single".into(),
            name: "Single".into(),
            players: 1,
            family: LayoutFamily::StepMania,
            lanes: vec![
                Self::arrow("left", "Left", 0.0, 0.0, &["ArrowLeft", "KeyD", "Numpad4"]),
                Self::arrow(
                    "down",
                    "Down",
                    -90.0,
                    1.0,
                    &["ArrowDown", "KeyF", "Numpad2"],
                ),
                Self::arrow("up", "Up", 90.0, 2.0, &["ArrowUp", "KeyJ", "Numpad8"]),
                Self::arrow(
                    "right",
                    "Right",
                    180.0,
                    3.0,
                    &["ArrowRight", "KeyK", "Numpad6"],
                ),
            ],
            // StepMania `GetTrackMapping` (`src/Style.cpp`), `dance-single`.
            turn_left: Some(vec![2, 0, 3, 1]),
        }
    }

    /// StepMania `dance-solo`: L, UL, D, U, UR, R.
    pub fn dance_solo() -> Layout {
        Layout {
            id: "dance-solo".into(),
            name: "Solo".into(),
            players: 1,
            family: LayoutFamily::StepMania,
            lanes: vec![
                Self::arrow("left", "Left", 0.0, 0.0, &["ArrowLeft", "KeyS", "Numpad4"]),
                Self::arrow("upleft", "Up-left", 45.0, 1.0, &["KeyD", "Numpad7"]),
                Self::arrow(
                    "down",
                    "Down",
                    -90.0,
                    2.0,
                    &["ArrowDown", "KeyF", "Numpad2"],
                ),
                Self::arrow("up", "Up", 90.0, 3.0, &["ArrowUp", "KeyJ", "Numpad8"]),
                Self::arrow("upright", "Up-right", 135.0, 4.0, &["KeyK", "Numpad9"]),
                Self::arrow(
                    "right",
                    "Right",
                    180.0,
                    5.0,
                    &["ArrowRight", "KeyL", "Numpad6"],
                ),
            ],
            turn_left: Some(vec![5, 4, 0, 3, 1, 2]),
        }
    }

    /// StepMania `techno-single8`, in its track order (GameManager.cpp).
    pub fn techno_single8() -> Layout {
        Layout {
            id: "techno-single8".into(),
            name: "Techno 8-panel".into(),
            players: 1,
            family: LayoutFamily::StepMania,
            lanes: vec![
                Self::arrow("downleft", "Down-left", -45.0, 0.0, &["KeyZ", "Numpad1"]),
                Self::arrow("left", "Left", 0.0, 1.0, &["KeyA", "ArrowLeft", "Numpad4"]),
                Self::arrow("upleft", "Up-left", 45.0, 2.0, &["KeyQ", "Numpad7"]),
                Self::arrow(
                    "down",
                    "Down",
                    -90.0,
                    3.0,
                    &["KeyX", "ArrowDown", "Numpad2"],
                ),
                Self::arrow("up", "Up", 90.0, 4.0, &["KeyW", "ArrowUp", "Numpad8"]),
                Self::arrow("upright", "Up-right", 135.0, 5.0, &["KeyE", "Numpad9"]),
                Self::arrow(
                    "right",
                    "Right",
                    180.0,
                    6.0,
                    &["KeyD", "ArrowRight", "Numpad6"],
                ),
                Self::arrow("downright", "Down-right", 225.0, 7.0, &["KeyC", "Numpad3"]),
            ],
            turn_left: Some(vec![2, 4, 5, 1, 6, 7, 3, 0]),
        }
    }

    /// StepMania `dance-double`: the left pad's L, D, U, R, then the right
    /// pad's (one player on two pads).
    pub fn dance_double() -> Layout {
        let mut lanes = Vec::with_capacity(8);
        let keys: [[&[&str]; 4]; 2] = [
            [&["KeyA"], &["KeyS"], &["KeyW"], &["KeyD"]],
            [
                &["ArrowLeft", "Numpad4"],
                &["ArrowDown", "Numpad2"],
                &["ArrowUp", "Numpad8"],
                &["ArrowRight", "Numpad6"],
            ],
        ];
        for (pad, keys) in keys.iter().enumerate() {
            let base = pad as f32 * 4.0;
            let n = pad + 1;
            for (i, (name, label, rot)) in [
                ("left", "Left", 0.0),
                ("down", "Down", -90.0),
                ("up", "Up", 90.0),
                ("right", "Right", 180.0),
            ]
            .into_iter()
            .enumerate()
            {
                let mut lane = Self::arrow(
                    name,
                    &format!("{label} (pad {n})"),
                    rot,
                    base + i as f32,
                    keys[i],
                );
                lane.pad = pad as u8;
                lanes.push(lane);
            }
        }
        Layout {
            id: "dance-double".into(),
            name: "Double".into(),
            players: 1,
            family: LayoutFamily::StepMania,
            lanes,
            turn_left: Some(vec![2, 0, 3, 1, 6, 4, 7, 5]),
        }
    }

    /// The lanes of five-panel pads: `panels` of pad 0, then of pad 1, as
    /// `(pad, panel)` with panels in [`PUMP_PANELS`] order, side by side.
    fn pump_lanes(panels: &[(u8, usize)], keys: impl Fn(u8, usize) -> Vec<String>) -> Vec<Lane> {
        let two_pads = panels.iter().any(|&(pad, _)| pad > 0);
        panels
            .iter()
            .enumerate()
            .map(|(i, &(pad, panel))| {
                let (name, label, glyph) = &PUMP_PANELS[panel];
                let label = if two_pads {
                    format!("{label} (pad {})", pad + 1)
                } else {
                    label.to_string()
                };
                Lane {
                    name: name.to_string(),
                    label,
                    glyph: glyph.clone(),
                    color_group: 0,
                    shuffle_group: 0,
                    scroll_sign: 1,
                    column: i as f32,
                    row: 0,
                    pad,
                    default_keys: keys(pad, panel),
                }
            })
            .collect()
    }

    fn pump_keys(pad: u8, panel: usize) -> Vec<String> {
        PUMP_KEYS[usize::from(pad)][panel]
            .iter()
            .map(|k| k.to_string())
            .collect()
    }

    /// StepMania `pump-single`: DL, UL, C, UR, DR on one five-panel pad.
    pub fn pump_single() -> Layout {
        let panels: Vec<(u8, usize)> = (0..5).map(|p| (0, p)).collect();
        Layout {
            id: "pump-single".into(),
            name: "Pump single".into(),
            players: 1,
            family: LayoutFamily::StepMania,
            // The left hand's block and the number pad.
            lanes: Self::pump_lanes(&panels, |_, panel| {
                let mut keys = Self::pump_keys(0, panel);
                keys.push(PUMP_KEYS[1][panel][1].to_string());
                keys
            }),
            // StepMania `NoteDataUtil::Turn` (`src/NoteDataUtil.cpp`),
            // `pump_single`, as for every layout here.
            turn_left: Some(vec![1, 3, 2, 4, 0]),
        }
    }

    /// StepMania `pump-halfdouble`: the middle six panels of two pads, the
    /// left pad's C, UR, DR and the right pad's DL, UL, C.
    pub fn pump_halfdouble() -> Layout {
        Layout {
            id: "pump-halfdouble".into(),
            name: "Pump half-double".into(),
            players: 1,
            family: LayoutFamily::StepMania,
            lanes: Self::pump_lanes(
                &[(0, 2), (0, 3), (0, 4), (1, 0), (1, 1), (1, 2)],
                Self::pump_keys,
            ),
            turn_left: Some(vec![2, 0, 1, 3, 4, 5]),
        }
    }

    /// StepMania `pump-double`: the left pad's five panels, then the right
    /// pad's (one player on two pads).
    pub fn pump_double() -> Layout {
        let panels: Vec<(u8, usize)> = (0..2)
            .flat_map(|pad| (0..5).map(move |p| (pad, p)))
            .collect();
        Layout {
            id: "pump-double".into(),
            name: "Pump double".into(),
            players: 1,
            family: LayoutFamily::StepMania,
            lanes: Self::pump_lanes(&panels, Self::pump_keys),
            turn_left: Some(vec![8, 9, 7, 5, 6, 3, 4, 2, 0, 1]),
        }
    }

    /// Built-in layout by id: the StepMania ones and the
    /// Dancing☆Onigiri key modes the game plays ([`PLAYED_DANONI_MODES`]),
    /// as `danoni-<mode>`. Other key modes stay data
    /// ([`danoni::builtin`]) but are not played.
    pub fn builtin(id: &str) -> Option<Layout> {
        match id {
            "dance-single" => Some(Self::dance_single()),
            "dance-solo" => Some(Self::dance_solo()),
            "dance-double" => Some(Self::dance_double()),
            "pump-single" => Some(Self::pump_single()),
            "pump-halfdouble" => Some(Self::pump_halfdouble()),
            "pump-double" => Some(Self::pump_double()),
            "techno-single8" => Some(Self::techno_single8()),
            _ => {
                let mode = id.strip_prefix("danoni-")?;
                PLAYED_DANONI_MODES.contains(&mode).then_some(())?;
                danoni::builtin(mode).map(|k| {
                    let mut layout = k.layout(id.to_string());
                    for (lane, code) in danoni::numpad_keys(mode).iter().enumerate() {
                        layout.lanes[lane].default_keys.push(code.to_string());
                    }
                    layout
                })
            }
        }
    }

    /// Ids of every built-in layout, in the order the game lists them.
    pub fn builtin_ids() -> Vec<String> {
        BUILTIN_LAYOUTS
            .iter()
            .map(|s| s.to_string())
            .chain(PLAYED_DANONI_MODES.iter().map(|m| danoni::layout_id(m)))
            .collect()
    }

    /// The built-in StepMania layouts, in [`BUILTIN_LAYOUTS`] order.
    pub fn builtins() -> Vec<Layout> {
        BUILTIN_LAYOUTS
            .iter()
            .filter_map(|id| Self::builtin(id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_consistent() {
        for layout in Layout::builtins() {
            let n = layout.lane_count();
            assert!(n > 0, "{}", layout.id);
            if let Some(t) = &layout.turn_left {
                let mut sorted = t.clone();
                sorted.sort();
                assert_eq!(sorted, (0..n as u8).collect::<Vec<_>>(), "{}", layout.id);
            }
            // Keys are unique across lanes, labels are set.
            let mut keys: Vec<&String> =
                layout.lanes.iter().flat_map(|l| &l.default_keys).collect();
            let before = keys.len();
            keys.sort();
            keys.dedup();
            assert_eq!(keys.len(), before, "{}", layout.id);
            assert!(layout.lanes.iter().all(|l| !l.label.is_empty()));
        }
        let double = Layout::dance_double();
        assert_eq!(double.pads(), 2);
        // The number pad is each layout's second set: 4 2 8 6 for the
        // arrows, after the main keys, and the right pad of double.
        let numpad = |l: &Layout| -> Vec<Option<String>> {
            l.lanes
                .iter()
                .map(|l| {
                    assert!(!l.default_keys[0].starts_with("Numpad"), "{}", l.name);
                    l.default_keys
                        .iter()
                        .find(|k| k.starts_with("Numpad"))
                        .cloned()
                })
                .collect()
        };
        let pad = |n: u8| Some(format!("Numpad{n}"));
        assert_eq!(
            numpad(&Layout::dance_single()),
            [pad(4), pad(2), pad(8), pad(6)]
        );
        assert_eq!(
            numpad(&Layout::dance_solo()),
            [pad(4), pad(7), pad(2), pad(8), pad(9), pad(6)]
        );
        assert_eq!(
            numpad(&double),
            [None, None, None, None, pad(4), pad(2), pad(8), pad(6)]
        );
        assert_eq!(
            numpad(&Layout::techno_single8()),
            [
                pad(1),
                pad(4),
                pad(7),
                pad(2),
                pad(8),
                pad(9),
                pad(6),
                pad(3)
            ]
        );
        assert_eq!(double.lanes_of_pad(1).collect::<Vec<_>>(), [4, 5, 6, 7]);
        assert_eq!(Layout::dance_solo().pads(), 1);
        assert_eq!(Layout::builtins().len(), BUILTIN_LAYOUTS.len());
    }

    #[test]
    fn pump_layouts_follow_stepmania_track_order() {
        let names = |l: Layout| -> Vec<String> {
            l.lanes
                .iter()
                .map(|lane| format!("{}{}", lane.name, lane.pad))
                .collect()
        };
        assert_eq!(
            names(Layout::pump_single()),
            ["downleft0", "upleft0", "center0", "upright0", "downright0"]
        );
        // The middle six panels: StepMania's `m_iInputColumn` for
        // `halfdouble`.
        assert_eq!(
            names(Layout::pump_halfdouble()),
            [
                "center0",
                "upright0",
                "downright0",
                "downleft1",
                "upleft1",
                "center1"
            ]
        );
        let double = Layout::pump_double();
        assert_eq!(double.lane_count(), 10);
        assert_eq!(double.lanes_of_pad(1).collect::<Vec<_>>(), [5, 6, 7, 8, 9]);
        assert_eq!(double.lanes[7].glyph, Glyph::Center);
        assert_eq!(double.lanes[7].label(), "Centre (pad 2)");
        // Each panel keeps its keys across the three.
        let half = Layout::pump_halfdouble();
        assert_eq!(half.lanes[0].default_keys, double.lanes[2].default_keys);
        assert_eq!(half.lanes[5].default_keys, double.lanes[7].default_keys);
        assert_eq!(
            Layout::pump_single().lanes[0].default_keys,
            ["KeyZ", "Numpad1"]
        );
    }

    #[test]
    fn layouts_saved_before_the_new_fields_still_load() {
        let json = r#"{"id":"x","name":"X","players":1,"lanes":[{"name":"a",
            "glyph":"Onigiri","color_group":0,"shuffle_group":0,"scroll_sign":1,
            "column":0.0,"row":0,"default_keys":["Space"]}]}"#;
        let l: Layout = serde_json::from_str(json).unwrap();
        assert_eq!(l.family, LayoutFamily::StepMania);
        assert_eq!(l.turn_left, None);
        assert_eq!(l.lanes[0].pad, 0);
        assert_eq!(l.lanes[0].label(), "a");
    }
}
