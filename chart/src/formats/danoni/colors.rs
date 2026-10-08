//! A work's own note colours.
//!
//! danoniplus colours a lane by its colour group (`setColor`, per chart
//! `setColor2`, …; `resetBaseColorList`, `js/lib/dosConverter.js`), a hold
//! head by its group's freeze colours (`frzColor`: normal arrow, normal bar,
//! hit arrow, hit bar per group), and changes them over time with
//! `color_data`/`acolor_data` (`frame,target,colour`: 0–19 or 1000+ a lane,
//! 20–29 a colour group, 30–61 freeze parts) and `ncolor_data`
//! (`frame,target[:part],colour[,all]`: `0...7`, `1/3/5`, `g2`, `all`; parts
//! `Arrow`, `Normal`, …) (`pushColors`, `js/lib/dataLoader.js` 1900–2020). A
//! change applies to notes arriving at or after its frame.
//!
//! Here each note gets the colour it would be drawn with: taps the arrow
//! colour, hold heads the freeze normal-arrow colour. Hold bodies keep the
//! skin's colours, gradients use their first colour, and changes marked to
//! recolour notes already on screen (`acolor_data`, the `all` flag) are
//! treated like the others (only notes already on screen at that frame
//! differ).

use crate::model::Color;

use super::dos::{Dos, js_round, parse_int, split_lf, split_lf2};
use super::expr;

/// danoniplus's default arrow colours (`setColorInit`), one per group.
const SET_COLOR_INIT: [&str; 10] = [
    "#6666ff", "#99ffff", "#ffffff", "#ffff99", "#ff9966", "#6666ff", "#99ffff", "#ffffff",
    "#ffff99", "#ff9966",
];

/// Its default freeze colours (`frzColorInit`): normal arrow, normal bar,
/// hit arrow, hit bar.
const FRZ_COLOR_INIT: [&str; 4] = ["#66ffff", "#6600ff", "#cccc33", "#999933"];

/// Most colour changes read per chart.
const MAX_CHANGES: usize = 100_000;

/// Most lane targets read from one `ncolor_data` entry.
const MAX_TARGETS: usize = 1024;

/// What a colour change applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    /// Taps (and the arrow of anything not a hold).
    Arrow,
    /// Hold heads.
    HoldHead,
}

/// The colours of a chart's notes.
pub(super) struct Palette {
    arrow: Vec<Color>,
    hold: Vec<Color>,
    /// Per lane and part (`lane * 2 + part`), `(frame, colour)` sorted by
    /// frame (stably, so a later change on the same frame wins).
    changes: Vec<Vec<(i64, Color)>>,
    /// Changes read so far.
    count: usize,
    /// Lanes of each colour group.
    by_group: Vec<Vec<usize>>,
}

impl Palette {
    /// The colours of chart `suffix` (`""`, `"2"`, …) for lanes in colour
    /// groups `groups`.
    pub(super) fn new(dos: &Dos, suffix: &str, groups: &[u8]) -> Palette {
        let header = |name: &str| {
            dos.val(&format!("{name}{suffix}"))
                .or_else(|| dos.val(name))
                .map(str::to_string)
        };
        let set: Vec<Color> = match header("setColor") {
            Some(list) => {
                let given: Vec<Option<Color>> = list.split(',').map(parse_color).collect();
                // The list repeats over the ten groups.
                (0..SET_COLOR_INIT.len())
                    .map(|j| {
                        given[j % given.len()]
                            .or_else(|| parse_color(SET_COLOR_INIT[j]))
                            .unwrap_or(WHITE)
                    })
                    .collect()
            }
            None => SET_COLOR_INIT
                .iter()
                .filter_map(|c| parse_color(c))
                .collect(),
        };
        // One row of freeze colours per group; missing ones from the
        // defaults.
        let frz_rows: Vec<String> = header("frzColor")
            .map(|f| split_lf2(&f))
            .unwrap_or_default();
        let frz_normal: Vec<Color> = (0..SET_COLOR_INIT.len())
            .map(|j| {
                frz_rows
                    .get(j)
                    .or(frz_rows.first())
                    .and_then(|row| row.split(',').next())
                    .and_then(parse_color)
                    .or_else(|| parse_color(FRZ_COLOR_INIT[0]))
                    .unwrap_or(WHITE)
            })
            .collect();
        let of = |list: &[Color], g: u8| list[usize::from(g) % list.len()];
        let mut p = Palette {
            arrow: groups.iter().map(|&g| of(&set, g)).collect(),
            hold: groups.iter().map(|&g| of(&frz_normal, g)).collect(),
            changes: vec![Vec::new(); groups.len() * 2],
            count: 0,
            by_group: {
                let mut g = vec![Vec::new(); 256];
                for (lane, &group) in groups.iter().enumerate() {
                    g[usize::from(group)].push(lane);
                }
                g
            },
        };
        for name in ["color", "acolor"] {
            if let Some(data) = dos.val(&format!("{name}{suffix}_data")) {
                p.legacy(data);
            }
        }
        if let Some(data) = dos.val(&format!("ncolor{suffix}_data")) {
            p.modern(data);
        }
        for c in &mut p.changes {
            c.sort_by_key(|c| c.0);
        }
        p
    }

    fn slot(lane: usize, part: Part) -> usize {
        lane * 2 + usize::from(part == Part::HoldHead)
    }

    fn full(&self) -> bool {
        self.count >= MAX_CHANGES
    }

    /// The colour of a note of `lane` arriving at `frame`.
    pub(super) fn color(&self, lane: usize, frame: i64, hold: bool) -> Option<Color> {
        let part = if hold { Part::HoldHead } else { Part::Arrow };
        let base = if hold { &self.hold } else { &self.arrow };
        self.changes
            .get(Self::slot(lane, part))
            .and_then(|c| {
                let end = c.partition_point(|c| c.0 <= frame);
                end.checked_sub(1).map(|k| c[k].1)
            })
            .or_else(|| base.get(lane).copied())
    }

    fn push(&mut self, frame: i64, lane: usize, part: Part, color: Color) {
        if !self.full() && lane < self.arrow.len() {
            self.changes[Self::slot(lane, part)].push((frame, color));
            self.count += 1;
        }
    }

    /// `color_data` / `acolor_data`: `frame,target,colour` triples.
    fn legacy(&mut self, data: &str) {
        for line in split_lf(data).filter(|l| !l.is_empty()) {
            let items: Vec<&str> = line.split(',').collect();
            for k in (0..items.len()).step_by(3) {
                if self.full() {
                    return;
                }
                if items[k].is_empty() {
                    continue;
                }
                if items.get(k + 1) == Some(&"-") {
                    break;
                }
                let (Some(frame), Some(target), Some(color)) = (
                    frame_of(items[k]),
                    items
                        .get(k + 1)
                        .and_then(|t| expr::eval(t))
                        .map(|t| t as i64),
                    items.get(k + 2).and_then(|c| parse_color(c)),
                ) else {
                    continue;
                };
                match target {
                    t if !(20..1000).contains(&t) && t >= 0 => {
                        self.push(frame, (t % 1000) as usize, Part::Arrow, color)
                    }
                    20..=29 => self.groups(frame, (target - 20) as u8, Part::Arrow, color),
                    30..=61 => {
                        // Freeze parts (`pushColors`): only the normal arrow
                        // colours a hold head here.
                        let parts: Vec<i64> = match target {
                            30..=49 => vec![target % 30],
                            50..=59 => vec![(target % 50) * 2, (target % 50) * 2 + 1],
                            60 => (0..8).collect(),
                            _ => (10..18).collect(),
                        };
                        for t in parts {
                            let normal_arrow = t < 10 && t % 2 == 0;
                            if normal_arrow {
                                let group = ((t % 10 - 1) as f64 / 2.0).ceil() as i64;
                                self.groups(frame, group.max(0) as u8, Part::HoldHead, color);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// `ncolor_data`: `frame,target[:part],colour[,all]` per line.
    fn modern(&mut self, data: &str) {
        for line in split_lf(data).filter(|l| !l.is_empty()) {
            if self.full() {
                return;
            }
            let items: Vec<&str> = line.split(',').map(str::trim).collect();
            if items.first().is_none_or(|f| f.is_empty()) || items.get(1) == Some(&"-") {
                continue;
            }
            let (Some(frame), Some(color)) = (
                frame_of(items[0]),
                items.get(2).and_then(|c| parse_color(c)),
            ) else {
                continue;
            };
            let target = items.get(1).copied().unwrap_or_default();
            let (lanes_spec, parts_spec) = match target.split_once(':') {
                Some((l, p)) if !l.is_empty() => (l.trim(), p.trim()),
                _ => (target, "Arrow"),
            };
            // The two parts drawn here, each at most once.
            let named = expand_parts(parts_spec);
            let parts: Vec<Part> = [("Arrow", Part::Arrow), ("Normal", Part::HoldHead)]
                .into_iter()
                .filter(|(n, _)| named.iter().any(|p| p == n))
                .map(|(_, p)| p)
                .collect();
            let lanes_spec = if lanes_spec == "all" {
                (0..50)
                    .map(|g| format!("g{g}"))
                    .collect::<Vec<_>>()
                    .join("/")
            } else {
                lanes_spec.to_string()
            };
            for spec in lanes_spec.split('/').take(MAX_TARGETS) {
                if self.full() {
                    return;
                }
                for part in &parts {
                    if let Some(g) = spec.strip_prefix('g') {
                        if let Some(g) = parse_int(g).filter(|g| (0..=255).contains(g)) {
                            self.groups(frame, g as u8, *part, color);
                        }
                    } else if let Some((a, b)) = spec.split_once("...") {
                        if let (Some(a), Some(b)) = (parse_int(a), parse_int(b)) {
                            let last = b.min(a.max(0) + 255).min(self.arrow.len() as i64 - 1);
                            for lane in a.max(0)..=last {
                                self.push(frame, lane as usize, *part, color);
                            }
                        }
                    } else if let Some(lane) = parse_int(spec).filter(|l| *l >= 0) {
                        self.push(frame, lane as usize, *part, color);
                    }
                }
            }
        }
    }

    fn groups(&mut self, frame: i64, group: u8, part: Part, color: Color) {
        for k in 0..self.by_group[usize::from(group)].len() {
            let lane = self.by_group[usize::from(group)][k];
            self.push(frame, lane, part, color);
        }
    }
}

/// The parts an `ncolor_data` target names, short names written out
/// (`g_escapeStr.colorPatternName`).
fn expand_parts(spec: &str) -> Vec<String> {
    let short = [
        ("AR", "Arrow"),
        ("AS", "ArrowShadow"),
        ("NA", "Normal"),
        ("NB", "NormalBar"),
        ("NS", "NormalShadow"),
        ("HA", "Hit"),
        ("HB", "HitBar"),
        ("HS", "HitShadow"),
        ("FN", "Normal/NormalBar"),
        ("FH", "Hit/HitBar"),
        ("FS", "NormalShadow/HitShadow"),
    ];
    let full = short
        .iter()
        .find(|(s, _)| *s == spec)
        .map_or(spec, |(_, f)| f);
    full.split('/').map(str::to_string).collect()
}

/// A frame of a colour change (evaluated, rounded, bounded like note
/// frames).
fn frame_of(s: &str) -> Option<i64> {
    expr::eval(s)
        .filter(|f| f.abs() <= super::MAX_FRAME)
        .map(|f| js_round(f) as i64)
}

const WHITE: Color = Color {
    r: 1.0,
    g: 1.0,
    b: 1.0,
    a: 1.0,
};

/// The first colour of a danoniplus colour value: `#rgb`, `#rrggbb`,
/// `#rrggbbaa` or `0xrrggbb`, possibly the first stop of a gradient
/// (`45deg:#ffff99:#ffffff@linear-gradient`).
pub(super) fn parse_color(s: &str) -> Option<Color> {
    let stops = s.trim().split("@").next()?.split(';').next()?;
    stops.split(':').find_map(|t| {
        let t = t.trim();
        let hex = t.strip_prefix('#').or_else(|| t.strip_prefix("0x"))?;
        let v = |i: usize, w: usize| {
            u8::from_str_radix(hex.get(i..i + w)?, 16)
                .ok()
                .map(|x| if w == 1 { x * 17 } else { x })
        };
        let (r, g, b, a) = match hex.len() {
            3 => (v(0, 1)?, v(1, 1)?, v(2, 1)?, 255),
            6 => (v(0, 2)?, v(2, 2)?, v(4, 2)?, 255),
            8 => (v(0, 2)?, v(2, 2)?, v(4, 2)?, v(6, 2)?),
            _ => return None,
        };
        let c = |x: u8| f32::from(x) / 255.0;
        Some(Color {
            r: c(r),
            g: c(g),
            b: c(b),
            a: c(a),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::danoni::dos::DosFlags;

    fn hex(c: Color) -> String {
        let b = |x: f32| (x * 255.0).round() as u8;
        format!("#{:02x}{:02x}{:02x}", b(c.r), b(c.g), b(c.b))
    }

    #[test]
    fn colours_parse() {
        assert_eq!(parse_color("#ff9966").map(hex).as_deref(), Some("#ff9966"));
        assert_eq!(parse_color("0x80FFFF").map(hex).as_deref(), Some("#80ffff"));
        assert_eq!(parse_color("#fff").map(hex).as_deref(), Some("#ffffff"));
        assert_eq!(
            parse_color("45deg:#ffff99:#ffffff@linear-gradient")
                .map(hex)
                .as_deref(),
            Some("#ffff99")
        );
        assert_eq!(parse_color("Default"), None);
    }

    #[test]
    fn many_targets_stay_bounded() {
        // Thousands of lane ranges and parts in one entry: each target is
        // read once per part, at most `MAX_TARGETS` of them.
        let lanes = vec!["0...255"; 5000].join("/");
        let parts = vec!["Arrow"; 5000].join("/");
        let dos = Dos::parse(
            &format!("|ncolor_data=0,{lanes}:{parts},#ff0000|"),
            DosFlags::default(),
        );
        let p = Palette::new(&dos, "", &[0, 1, 2]);
        assert_eq!(p.count, MAX_TARGETS * 3);
        assert_eq!(p.color(2, 10, false).map(hex).as_deref(), Some("#ff0000"));
    }

    #[test]
    fn groups_defaults_and_changes() {
        // 5 keys: arrows in group 0, the onigiri in group 2.
        let groups = [0, 0, 0, 0, 2];
        let p = Palette::new(&Dos::default(), "", &groups);
        assert_eq!(hex(p.color(0, 0, false).unwrap()), "#6666ff");
        assert_eq!(hex(p.color(4, 0, false).unwrap()), "#ffffff");
        assert_eq!(hex(p.color(0, 0, true).unwrap()), "#66ffff");

        let dos = Dos::parse(
            "|setColor=#111111,#222222,#333333|setColor2=#999999|frzColor=#aa0000,#bb0000|\
             |color_data=300,22,#00ff00,400,1,#0000ff\n500,30,#ff00ff|\
             |ncolor_data=600,0...1,#123456\n700,g2:Normal,#654321\n800,3:NA,#abcdef,all|",
            DosFlags::default(),
        );
        let p = Palette::new(&dos, "", &groups);
        assert_eq!(hex(p.color(0, 0, false).unwrap()), "#111111");
        assert_eq!(hex(p.color(4, 0, false).unwrap()), "#333333");
        assert_eq!(hex(p.color(0, 0, true).unwrap()), "#aa0000");
        // Group 2 (the onigiri) from 300; lane 1 from 400.
        assert_eq!(hex(p.color(4, 299, false).unwrap()), "#333333");
        assert_eq!(hex(p.color(4, 300, false).unwrap()), "#00ff00");
        assert_eq!(hex(p.color(1, 450, false).unwrap()), "#0000ff");
        // 30: group 0's freeze normal arrow, for hold heads only.
        assert_eq!(hex(p.color(2, 500, true).unwrap()), "#ff00ff");
        assert_eq!(hex(p.color(2, 500, false).unwrap()), "#111111");
        assert_eq!(hex(p.color(0, 650, false).unwrap()), "#123456");
        assert_eq!(hex(p.color(2, 650, false).unwrap()), "#111111");
        assert_eq!(hex(p.color(4, 700, true).unwrap()), "#654321");
        assert_eq!(hex(p.color(3, 800, true).unwrap()), "#abcdef");
        // Chart 2 has its own list, repeated over the groups.
        let p2 = Palette::new(&dos, "2", &groups);
        assert_eq!(hex(p2.color(4, 0, false).unwrap()), "#999999");
    }
}
