//! Lane layouts ("steps types", key modes) as data.
//!
//! A [`Layout`] says everything the game needs to know about a chart's
//! lanes: how each lane is drawn and named, where it sits, how lanes may be
//! permuted, which physical pad drives it and which keys play it by
//! default. Charts refer to layouts by id; built-in ones come from
//! [`Layout::builtin`], and a song file may define its own
//! ([`Song::layouts`](crate::Song::layouts)), resolved with
//! [`Song::layout_of`](crate::Song::layout_of).

use serde::{Deserialize, Serialize};

pub mod danoni;
mod danoni_table;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Glyph {
    /// Arrow pointing in `rotation_deg` where 0 = left, -90 = down, 90 = up, 180 = right.
    Arrow {
        rotation_deg: f32,
    },
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
/// (decision 27). The rest of danoniplus's modes are kept as data only.
pub const PLAYED_DANONI_MODES: [&str; 5] = ["5", "7", "7i", "9A", "9B"];

/// Ids of the built-in StepMania layouts, in the order the game lists them
/// (the Dancing☆Onigiri key modes follow, see [`Layout::builtin_ids`]).
pub const BUILTIN_LAYOUTS: [&str; 3] = ["dance-single", "dance-solo", "dance-double"];

impl Layout {
    pub fn lane_count(&self) -> usize {
        self.lanes.len()
    }

    /// Number of physical pads the layout spans (at least 1).
    pub fn pads(&self) -> u8 {
        self.lanes.iter().map(|l| l.pad).max().map_or(1, |p| p + 1)
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
                Self::arrow("left", "Left", 0.0, 0.0, &["ArrowLeft", "KeyD"]),
                Self::arrow("down", "Down", -90.0, 1.0, &["ArrowDown", "KeyF"]),
                Self::arrow("up", "Up", 90.0, 2.0, &["ArrowUp", "KeyJ"]),
                Self::arrow("right", "Right", 180.0, 3.0, &["ArrowRight", "KeyK"]),
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
                Self::arrow("left", "Left", 0.0, 0.0, &["ArrowLeft", "KeyS"]),
                Self::arrow("upleft", "Up-left", 45.0, 1.0, &["KeyD"]),
                Self::arrow("down", "Down", -90.0, 2.0, &["ArrowDown", "KeyF"]),
                Self::arrow("up", "Up", 90.0, 3.0, &["ArrowUp", "KeyJ"]),
                Self::arrow("upright", "Up-right", 135.0, 4.0, &["KeyK"]),
                Self::arrow("right", "Right", 180.0, 5.0, &["ArrowRight", "KeyL"]),
            ],
            turn_left: Some(vec![5, 4, 0, 3, 1, 2]),
        }
    }

    /// StepMania `dance-double`: the left pad's L, D, U, R, then the right
    /// pad's (one player on two pads).
    pub fn dance_double() -> Layout {
        let mut lanes = Vec::with_capacity(8);
        let keys: [[&[&str]; 4]; 2] = [
            [&["KeyA"], &["KeyS"], &["KeyW"], &["KeyD"]],
            [
                &["ArrowLeft"],
                &["ArrowDown"],
                &["ArrowUp"],
                &["ArrowRight"],
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

    /// Built-in layout by id: the StepMania ones and the
    /// Dancing☆Onigiri key modes the game plays ([`PLAYED_DANONI_MODES`]),
    /// as `danoni-<mode>`. Other key modes stay data
    /// ([`danoni::builtin`]) but are not played.
    pub fn builtin(id: &str) -> Option<Layout> {
        match id {
            "dance-single" => Some(Self::dance_single()),
            "dance-solo" => Some(Self::dance_solo()),
            "dance-double" => Some(Self::dance_double()),
            _ => {
                let mode = id.strip_prefix("danoni-")?;
                PLAYED_DANONI_MODES.contains(&mode).then_some(())?;
                danoni::builtin(mode).map(|k| k.layout(id.to_string()))
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
        assert_eq!(double.lanes_of_pad(1).collect::<Vec<_>>(), [4, 5, 6, 7]);
        assert_eq!(Layout::dance_solo().pads(), 1);
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
