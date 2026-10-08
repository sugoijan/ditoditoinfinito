//! Dancing☆Onigiri key modes as [`Layout`]s.
//!
//! Every built-in mode of danoniplus (pattern 0, `danoni_table.rs`) and,
//! through [`KeyDef`], the modes a work defines in its header. Geometry
//! follows danoniplus (`js/lib/dataLoader.js` 2273–2289): lanes whose `pos`
//! is at most `div − 1` form the upper row, step zone at the top, scrolling up;
//! the others the lower row, step zone at the bottom, scrolling down. Within
//! a row a lane sits at `pos − (row offset + div − 1) / 2` lane widths from
//! the centre, the lower row's offset being `divMax` (else the largest
//! `pos` + 1). Lanes are one `blank` apart in danoniplus; here one lane
//! width.

use super::{Glyph, Lane, Layout, LayoutFamily};

/// How a lane is drawn: an arrow rotated from pointing left (positive =
/// clockwise, as CSS), or one of the ASCII-art characters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Rot(f32),
    Aa(&'static str),
}

/// One built-in key mode as danoniplus defines it.
pub(super) struct KeyMode {
    pub name: &'static str,
    pub chara: &'static [&'static str],
    pub color: &'static [u8],
    pub shuffle: &'static [u8],
    pub shape: &'static [Shape],
    pub pos: &'static [f32],
    pub div: u8,
    pub div_max: Option<f32>,
    /// Pixels between neighbouring lanes in danoniplus (informational).
    #[allow(dead_code)]
    pub blank: f32,
    /// Default keys per lane, as `KeyboardEvent.code`.
    pub keys: &'static [&'static [&'static str]],
}

/// A key mode definition, built in or from a work's header.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyDef {
    /// Mode name (`7`, `11L`, a header-defined name).
    pub name: String,
    /// Lane data names (`left`, `sleft`, `space`, …).
    pub chara: Vec<String>,
    pub color: Vec<u8>,
    pub shuffle: Vec<u8>,
    /// Arrow rotation in degrees, or an ASCII-art character name.
    pub shape: Vec<Result<f32, String>>,
    pub pos: Vec<f32>,
    /// Lanes with `pos <= div − 1` form the upper row.
    pub div: f32,
    pub div_max: Option<f32>,
    /// Default keys per lane, as `KeyboardEvent.code`.
    pub keys: Vec<Vec<String>>,
}

/// Layout id of a Dancing☆Onigiri key mode.
pub fn layout_id(mode: &str) -> String {
    format!("danoni-{mode}")
}

/// Names of the built-in key modes, in danoniplus order.
pub fn builtin_modes() -> impl Iterator<Item = &'static str> {
    super::danoni_table::KEY_MODES.iter().map(|k| k.name)
}

/// The built-in key mode `mode` (`5`, `7i`, `11L`, …).
pub fn builtin(mode: &str) -> Option<KeyDef> {
    let k = super::danoni_table::KEY_MODES
        .iter()
        .find(|k| k.name == mode)?;
    let mut keys: Vec<Vec<String>> = k
        .keys
        .iter()
        .map(|ks| ks.iter().map(|c| c.to_string()).collect())
        .collect();
    if k.name == "12i" {
        // Pattern 0 plays on F1–F12, which laptops often send as media
        // keys and browsers keep for themselves; pattern 1's row
        // (`keyCtrl12i_1`: Q … P, Ja-@, Ja-[) is added as a second key.
        for (lane, code) in [
            "KeyQ",
            "KeyW",
            "KeyE",
            "KeyR",
            "KeyT",
            "KeyY",
            "KeyU",
            "KeyI",
            "KeyO",
            "KeyP",
            "BracketLeft",
            "BracketRight",
        ]
        .into_iter()
        .enumerate()
        {
            keys[lane].push(code.to_string());
        }
    }
    Some(KeyDef {
        name: k.name.into(),
        chara: k.chara.iter().map(|c| c.to_string()).collect(),
        color: k.color.to_vec(),
        shuffle: k.shuffle.to_vec(),
        shape: k
            .shape
            .iter()
            .map(|s| match s {
                Shape::Rot(r) => Ok(*r),
                Shape::Aa(a) => Err(a.to_string()),
            })
            .collect(),
        pos: k.pos.to_vec(),
        div: f32::from(k.div),
        div_max: k.div_max,
        keys,
    })
}

impl KeyDef {
    /// The layout of this key mode, with id `id`.
    pub fn layout(&self, id: String) -> Layout {
        let n = self.chara.len();
        let divide = self.div - 1.0;
        let pos_max = self
            .div_max
            .unwrap_or_else(|| self.pos.iter().copied().fold(f32::NEG_INFINITY, f32::max) + 1.0);
        let lower = |p: f32| p > divide;
        // Offset from the field's centre, in lane widths.
        let x: Vec<f32> = self
            .pos
            .iter()
            .map(|&p| p - ((if lower(p) { pos_max } else { 0.0 }) + divide) / 2.0)
            .collect();
        // Columns from the field's left edge, the field centred on the
        // centre danoniplus uses (rows may be uneven: 9i, 14i).
        let half = x.iter().map(|x| x.abs()).fold(0.0, f32::max);
        let lanes = (0..n)
            .map(|i| {
                let glyph = match self.shape.get(i) {
                    Some(Ok(r)) => Glyph::Arrow { rotation_deg: *r },
                    Some(Err(aa)) => match aa.as_str() {
                        "onigiri" => Glyph::Onigiri,
                        "giko" => Glyph::Giko,
                        "iyo" => Glyph::Iyo,
                        other => Glyph::Custom(other.to_string()),
                    },
                    None => Glyph::Arrow { rotation_deg: 0.0 },
                };
                let low = self.pos.get(i).is_some_and(|&p| lower(p));
                Lane {
                    name: self.chara[i].clone(),
                    label: format!("{} · {}", i + 1, glyph_word(&glyph)),
                    glyph,
                    color_group: self.color.get(i).copied().unwrap_or(0),
                    shuffle_group: self.shuffle.get(i).copied().unwrap_or(0),
                    scroll_sign: if low { -1 } else { 1 },
                    column: x.get(i).map_or(i as f32, |x| x + half),
                    row: u8::from(low),
                    pad: 0,
                    default_keys: self.keys.get(i).cloned().unwrap_or_default(),
                }
            })
            .collect();
        Layout {
            id,
            name: format!("{} keys", self.name),
            players: 1,
            family: LayoutFamily::Danoni,
            lanes,
            turn_left: None,
        }
    }
}

/// A word for a lane's glyph: the nearest of eight directions for an arrow.
fn glyph_word(glyph: &Glyph) -> String {
    match glyph {
        Glyph::Arrow { rotation_deg } => {
            const WORDS: [&str; 8] = [
                "Left",
                "Up-left",
                "Up",
                "Up-right",
                "Right",
                "Down-right",
                "Down",
                "Down-left",
            ];
            let step = (rotation_deg / 45.0).round() as i64;
            WORDS[step.rem_euclid(8) as usize].to_string()
        }
        Glyph::Onigiri => "Onigiri".into(),
        Glyph::Giko => "Giko".into(),
        Glyph::Iyo => "Iyo".into(),
        Glyph::Custom(name) => {
            let mut c = name.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(mode: &str) -> Layout {
        builtin(mode).unwrap().layout(layout_id(mode))
    }

    #[test]
    fn every_builtin_mode_is_consistent() {
        let modes: Vec<&str> = builtin_modes().collect();
        assert_eq!(modes.len(), 24);
        for mode in modes {
            let l = layout(mode);
            let n: usize = mode
                .trim_end_matches(|c: char| c.is_ascii_alphabetic())
                .parse()
                .unwrap();
            assert_eq!(l.lane_count(), n, "{mode}");
            assert_eq!(l.family, LayoutFamily::Danoni);
            // Lane names and keys are unique, every lane has a key.
            let mut names: Vec<&str> = l.lanes.iter().map(|x| x.name.as_str()).collect();
            names.sort();
            names.dedup();
            assert_eq!(names.len(), n, "{mode}");
            let mut keys: Vec<&String> = l.lanes.iter().flat_map(|x| &x.default_keys).collect();
            let all = keys.len();
            keys.sort();
            keys.dedup();
            assert_eq!(keys.len(), all, "{mode}");
            assert!(l.lanes.iter().all(|x| !x.default_keys.is_empty()), "{mode}");
            // Lanes of one row never overlap.
            for row in [0, 1] {
                let mut cols: Vec<f32> = l
                    .lanes
                    .iter()
                    .filter(|x| x.row == row)
                    .map(|x| x.column)
                    .collect();
                cols.sort_by(f32::total_cmp);
                assert!(cols.windows(2).all(|w| w[1] - w[0] >= 0.99), "{mode}");
            }
            assert!(l.lanes.iter().all(|x| x.column >= 0.0));
        }
    }

    #[test]
    fn single_row_modes_follow_the_source() {
        let l = layout("7");
        let names: Vec<&str> = l.lanes.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "left", "leftdia", "down", "space", "up", "rightdia", "right"
            ]
        );
        assert_eq!(l.lanes[3].glyph, Glyph::Onigiri);
        assert_eq!(
            l.lanes[1].glyph,
            Glyph::Arrow {
                rotation_deg: -45.0
            }
        );
        assert_eq!(l.lanes[3].shuffle_group, 1);
        assert_eq!(l.lanes[3].default_keys, ["Space"]);
        assert_eq!(l.lanes[0].default_keys, ["KeyS"]);
        assert!(l.lanes.iter().all(|x| x.scroll_sign == 1 && x.row == 0));
        assert_eq!(
            l.lanes.iter().map(|x| x.column).collect::<Vec<_>>(),
            [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0]
        );
        assert_eq!(l.lanes[1].label, "2 · Down-left");
        let five = layout("5");
        assert_eq!(five.lanes[4].label, "5 · Onigiri");
        assert_eq!(five.lanes[0].default_keys, ["ArrowLeft"]);
        // 7i: giko, onigiri, iyo, then arrows.
        let seven_i = layout("7i");
        assert_eq!(seven_i.lanes[0].glyph, Glyph::Giko);
        assert_eq!(seven_i.lanes[2].glyph, Glyph::Iyo);
        // Key names convert like getKeyCtrlVal: `;` is Semicolon, D1 Digit1.
        assert_eq!(layout("9B").lanes[8].default_keys, ["Semicolon"]);
        assert_eq!(layout("9h").lanes[0].default_keys, ["Digit1"]);
    }

    #[test]
    fn two_row_modes_split_by_div() {
        // 11: pos 2..=12, div 6 → sleft..sright on top (pos 2..5, centred
        // on 2.5 like the source, so shifted right), the 7 lanes below.
        let l = layout("11");
        let top: Vec<&Lane> = l.lanes.iter().filter(|x| x.row == 0).collect();
        let bottom: Vec<&Lane> = l.lanes.iter().filter(|x| x.row == 1).collect();
        assert_eq!(top.len(), 4);
        assert_eq!(bottom.len(), 7);
        assert!(top.iter().all(|x| x.scroll_sign == 1));
        assert!(bottom.iter().all(|x| x.scroll_sign == -1));
        // x offsets: top −0.5, 0.5, 1.5, 2.5; bottom −3..=3; shifted by 3.
        let cols: Vec<f32> = l.lanes.iter().map(|x| x.column).collect();
        assert_eq!(
            cols,
            [2.5, 3.5, 4.5, 5.5, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0]
        );
        // 9h: pos 0, 4, 5, 6 on top, offsets pos − 3 (−3, 1, 2, 3); the
        // rest below, offsets pos − (divMax 15 + 6) / 2 (−2.5 … 2.5).
        let h = layout("9h");
        let cols: Vec<f32> = h.lanes.iter().map(|x| x.column).collect();
        assert_eq!(cols, [0.0, 4.0, 5.0, 6.0, 0.5, 1.5, 2.5, 4.25, 5.5]);
        // 9i: offsets −0.5 … 2.5 on top, −2 … 2 below (uneven rows), centred
        // on 0 like the source: the field spans −2.5 … 2.5.
        let i = layout("9i");
        let cols: Vec<f32> = i.lanes.iter().map(|x| x.column).collect();
        assert_eq!(cols, [2.0, 3.0, 4.0, 5.0, 0.5, 1.5, 2.5, 3.5, 4.5]);
        // 12i keeps F1–F12 and adds pattern 1's row.
        assert_eq!(
            layout("12i").lanes[11].default_keys,
            ["F12", "BracketRight"]
        );
        assert_eq!(h.lanes.iter().filter(|x| x.row == 1).count(), 5);
    }
}
