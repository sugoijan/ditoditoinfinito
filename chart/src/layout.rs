//! Lane layouts ("steps types") as data.

use serde::{Deserialize, Serialize};

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
    pub name: String,
    pub glyph: Glyph,
    pub color_group: u8,
    pub shuffle_group: u8,
    /// +1 normal, -1 reversed relative to the field.
    pub scroll_sign: i8,
    /// Horizontal position in lane widths from the field's left edge.
    pub column: f32,
    pub row: u8,
    /// Default bindings as `KeyboardEvent.code` values.
    pub default_keys: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub id: String,
    pub name: String,
    pub players: u8,
    pub lanes: Vec<Lane>,
}

impl Layout {
    pub fn lane_count(&self) -> usize {
        self.lanes.len()
    }

    fn arrow(name: &str, rot: f32, col: f32, keys: &[&str]) -> Lane {
        Lane {
            name: name.into(),
            glyph: Glyph::Arrow { rotation_deg: rot },
            color_group: 0,
            shuffle_group: 0,
            scroll_sign: 1,
            column: col,
            row: 0,
            default_keys: keys.iter().map(|k| k.to_string()).collect(),
        }
    }

    /// StepMania `dance-single`: L, D, U, R.
    pub fn dance_single() -> Layout {
        Layout {
            id: "dance-single".into(),
            name: "Single (4 panels)".into(),
            players: 1,
            lanes: vec![
                Self::arrow("left", 0.0, 0.0, &["ArrowLeft", "KeyD"]),
                Self::arrow("down", -90.0, 1.0, &["ArrowDown", "KeyF"]),
                Self::arrow("up", 90.0, 2.0, &["ArrowUp", "KeyJ"]),
                Self::arrow("right", 180.0, 3.0, &["ArrowRight", "KeyK"]),
            ],
        }
    }

    /// StepMania `dance-solo`: L, UL, D, U, UR, R.
    pub fn dance_solo() -> Layout {
        Layout {
            id: "dance-solo".into(),
            name: "Solo (6 panels)".into(),
            players: 1,
            lanes: vec![
                Self::arrow("left", 0.0, 0.0, &["ArrowLeft", "KeyS"]),
                Self::arrow("upleft", 45.0, 1.0, &["KeyD"]),
                Self::arrow("down", -90.0, 2.0, &["ArrowDown", "KeyF"]),
                Self::arrow("up", 90.0, 3.0, &["ArrowUp", "KeyJ"]),
                Self::arrow("upright", 135.0, 4.0, &["KeyK"]),
                Self::arrow("right", 180.0, 5.0, &["ArrowRight", "KeyL"]),
            ],
        }
    }

    /// StepMania `dance-double`: P1 L,D,U,R then P2 L,D,U,R (one player).
    pub fn dance_double() -> Layout {
        let mut lanes = Vec::with_capacity(8);
        for (p, keys) in [
            [&["KeyA"][..], &["KeyS"], &["KeyW"], &["KeyD"]],
            [
                &["ArrowLeft"][..],
                &["ArrowDown"],
                &["ArrowUp"],
                &["ArrowRight"],
            ],
        ]
        .iter()
        .enumerate()
        {
            let base = p as f32 * 4.0;
            lanes.push(Self::arrow("left", 0.0, base, keys[0]));
            lanes.push(Self::arrow("down", -90.0, base + 1.0, keys[1]));
            lanes.push(Self::arrow("up", 90.0, base + 2.0, keys[2]));
            lanes.push(Self::arrow("right", 180.0, base + 3.0, keys[3]));
        }
        Layout {
            id: "dance-double".into(),
            name: "Double (8 panels)".into(),
            players: 1,
            lanes,
        }
    }

    /// Built-in layout by StepMania steps-type id.
    pub fn builtin(id: &str) -> Option<Layout> {
        match id {
            "dance-single" => Some(Self::dance_single()),
            "dance-solo" => Some(Self::dance_solo()),
            "dance-double" => Some(Self::dance_double()),
            _ => None,
        }
    }
}
