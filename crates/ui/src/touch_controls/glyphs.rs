//! The touch controls' glyphs (`docs/plans/touch-controls.md`, round
//! TC2): the start menu's rain outlines (`assets/ui/menu-rain.svg`, its
//! arrow and onigiri contours, `ddi_theme`'s rain), each drawn as ONE
//! outline from an atlas this module rasterizes, so a translucent glyph
//! never shows darker joints where strokes would overlap. A lane's glyph
//! is its cell of the atlas turned as its receptor is; the play's touch
//! controls (`ddi_shell::touch_controls`) and the prompt's diagram
//! (`crate::screens::prompt_controls::touch_cut`) both draw it.
//!
//! The atlas is made here, not read from a file: the shells hand it to a
//! painter that asks for [`SOURCE`] (`ddi_shell::touch_controls::serve`),
//! one picture for every screen, white with its coverage as alpha.
//!
//! Its columns are the shapes ([`Shape`]): the rain's arrow (pointing
//! up), the rain's onigiri, a rounded square for a five-panel pad's
//! center, a ring for any other glyph; its rows two weights ([`Weight`]):
//! the rain's heaviest line for the play's targets, a heavier one for the
//! prompt's small cut.

use ddi_chart::layout::Glyph;

/// The atlas's source as a painter asks for it.
pub const SOURCE: &str = "ddi:touch-glyphs";

/// A cell's side in the atlas, pixels (64 units of the rain's cells).
pub const CELL: u32 = 192;

/// The rain's arrow contour, pointing up, in a cell of 64 units
/// (`assets/ui/menu-rain.svg`, `#arrow`).
pub const ARROW: &str = "M32 4L53.813 25.813A8.75 8.75 0 0 1 41.438 38.187L40.75 37.499V47.626A8.75 8.75 0 0 1 23.25 47.626V37.499L22.562 38.187A8.75 8.75 0 0 1 10.187 25.813Z";

/// The rain's onigiri contour (`#onigiri`).
pub const ONIGIRI: &str = "M24.924 20.762A8.17 8.17 0 0 1 39.076 20.762L52.052 43.238A8.17 8.17 0 0 1 44.976 55.493L43.246 55.493A1.377 1.377 0 0 1 41.869 54.116L41.869 41.448A1.377 1.377 0 0 0 40.492 40.07L23.508 40.07A1.377 1.377 0 0 0 22.131 41.448L22.131 54.116A1.377 1.377 0 0 1 20.754 55.493L19.024 55.493A8.17 8.17 0 0 1 11.948 43.238Z";

/// The arrow's round ends' radius (its arcs'), which the center's square
/// takes for its corners.
const ROUND: f32 = 8.75;

/// A glyph's shape, the atlas's column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Arrow,
    Onigiri,
    /// A five-panel pad's middle panel: a square with the arrow's round
    /// corners.
    Center,
    /// Any other glyph (Dancing☆Onigiri's giko, iyo, a custom one).
    Ring,
}

impl Shape {
    pub const ALL: [Shape; 4] = [Shape::Arrow, Shape::Onigiri, Shape::Center, Shape::Ring];

    pub fn of(glyph: &Glyph) -> Shape {
        match glyph {
            Glyph::Arrow { .. } => Shape::Arrow,
            Glyph::Onigiri => Shape::Onigiri,
            Glyph::Center => Shape::Center,
            _ => Shape::Ring,
        }
    }

    fn column(self) -> u32 {
        self as u32
    }
}

/// An outline's weight, the atlas's row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weight {
    /// The rain's heaviest line (its nearest particles', 4.07 units of
    /// 64): the play's targets, drawn 40 CSS pixels and more.
    Regular,
    /// Heavier, for a glyph drawn about a dozen CSS pixels across (the
    /// prompt's cut), where the rain's line would fade.
    Bold,
}

impl Weight {
    /// The stroke's width in the cell's 64 units.
    pub fn units(self) -> f32 {
        match self {
            Weight::Regular => 4.0657,
            Weight::Bold => 7.0,
        }
    }

    fn row(self) -> u32 {
        match self {
            Weight::Regular => 0,
            Weight::Bold => 1,
        }
    }
}

/// The atlas's size in pixels.
pub const SIZE: [u32; 2] = [CELL * 4, CELL * 2];

/// Where a glyph is in the atlas (fractions: left, top, right, bottom)
/// and how far it turns, clockwise on the screen, in degrees: its cell
/// drawn as a square centered on the target and turned so.
pub fn cell(glyph: &Glyph, weight: Weight) -> ([f32; 4], f32) {
    let shape = Shape::of(glyph);
    let (c, r) = (shape.column() as f32, weight.row() as f32);
    let uv = [c / 4.0, r / 2.0, (c + 1.0) / 4.0, (r + 1.0) / 2.0];
    // The layout's arrows turn from pointing left (0), up at 90: the
    // atlas's points up.
    let turn = match glyph {
        Glyph::Arrow { rotation_deg } => rotation_deg - 90.0,
        _ => 0.0,
    };
    (uv, turn)
}

/// How much of a target's smaller side a glyph's cell is drawn across:
/// the arrow then about half the side long, as the strokes were.
pub const CELL_SHARE: f32 = 0.64;

/// A shape's contour: closed, in the cell's 64 units, centered on the
/// cell's middle by its bounds (the rain's own cells sit it a little
/// off).
pub fn contour(shape: Shape) -> Vec<[f32; 2]> {
    let mut points = match shape {
        Shape::Arrow => flatten(ARROW),
        Shape::Onigiri => flatten(ONIGIRI),
        Shape::Center => rounded_square(20.0, ROUND),
        Shape::Ring => circle([32.0, 32.0], 21.0, 96),
    };
    let (lo, hi) = points.iter().fold(
        ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]),
        |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])]),
    );
    let shift = [32.0 - (lo[0] + hi[0]) / 2.0, 32.0 - (lo[1] + hi[1]) / 2.0];
    for p in &mut points {
        p[0] += shift[0];
        p[1] += shift[1];
    }
    points
}

/// The atlas: [`SIZE`] pixels of RGBA, white, the outlines' coverage as
/// alpha (each pixel's from its distance to the contour: one shape, its
/// strokes never added to each other). Made once a process.
pub fn atlas() -> &'static [u8] {
    static ATLAS: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    ATLAS.get_or_init(make_atlas)
}

fn make_atlas() -> Vec<u8> {
    let [w, h] = SIZE;
    // Each pixel's distance to the nearest contour, in pixels.
    let mut distance = vec![f32::INFINITY; (w * h) as usize];
    let k = CELL as f32 / 64.0;
    let mut halves = vec![0.0f32; (w * h) as usize];
    for weight in [Weight::Regular, Weight::Bold] {
        let half = weight.units() / 2.0 * k;
        for shape in Shape::ALL {
            let ox = (shape.column() * CELL) as f32;
            let oy = (weight.row() * CELL) as f32;
            let points: Vec<[f32; 2]> = contour(shape)
                .into_iter()
                .map(|[x, y]| [ox + x * k, oy + y * k])
                .collect();
            for i in 0..points.len() {
                let (a, b) = (points[i], points[(i + 1) % points.len()]);
                let reach = half + 1.5;
                let x0 = (a[0].min(b[0]) - reach).floor().max(ox) as u32;
                let x1 = (a[0].max(b[0]) + reach).ceil().min(ox + CELL as f32) as u32;
                let y0 = (a[1].min(b[1]) - reach).floor().max(oy) as u32;
                let y1 = (a[1].max(b[1]) + reach).ceil().min(oy + CELL as f32) as u32;
                for y in y0..y1 {
                    for x in x0..x1 {
                        let d = to_segment([x as f32 + 0.5, y as f32 + 0.5], a, b);
                        let at = (y * w + x) as usize;
                        if d < distance[at] {
                            distance[at] = d;
                            halves[at] = half;
                        }
                    }
                }
            }
        }
    }
    let mut rgba = vec![255u8; (w * h * 4) as usize];
    for (i, (&d, &half)) in distance.iter().zip(&halves).enumerate() {
        let a = (half + 0.5 - d).clamp(0.0, 1.0);
        rgba[i * 4 + 3] = (a * 255.0).round() as u8;
    }
    rgba
}

fn to_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx * dx + dy * dy;
    let t = if len > 0.0 {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

/// A square `half` each way from the cell's middle, its corners round.
fn rounded_square(half: f32, r: f32) -> Vec<[f32; 2]> {
    let mut out = Vec::new();
    let inner = half - r;
    for (cx, cy, from) in [
        (inner, -inner, -90.0f32),
        (inner, inner, 0.0),
        (-inner, inner, 90.0),
        (-inner, -inner, 180.0),
    ] {
        for i in 0..=16 {
            let a = (from + 90.0 * i as f32 / 16.0).to_radians();
            out.push([32.0 + cx + r * a.cos(), 32.0 + cy + r * a.sin()]);
        }
    }
    out
}

fn circle(c: [f32; 2], r: f32, n: usize) -> Vec<[f32; 2]> {
    (0..n)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / n as f32;
            [c[0] + r * a.cos(), c[1] + r * a.sin()]
        })
        .collect()
}

/// An SVG path's points (absolute `M`, `L`, `H`, `V`, `A` with equal
/// radii and no rotation, `Z`: what the rain's contours use), its arcs
/// as short segments.
fn flatten(d: &str) -> Vec<[f32; 2]> {
    let mut tokens = Vec::new();
    let mut number = String::new();
    let flush = |number: &mut String, tokens: &mut Vec<Token>| {
        if !number.is_empty() {
            tokens.push(Token::Number(number.parse().expect("a number")));
            number.clear();
        }
    };
    for ch in d.chars() {
        match ch {
            'M' | 'L' | 'H' | 'V' | 'A' | 'Z' => {
                flush(&mut number, &mut tokens);
                tokens.push(Token::Command(ch));
            }
            ' ' | ',' => flush(&mut number, &mut tokens),
            '-' if !number.is_empty() => {
                flush(&mut number, &mut tokens);
                number.push(ch);
            }
            _ => number.push(ch),
        }
    }
    flush(&mut number, &mut tokens);
    let mut out: Vec<[f32; 2]> = Vec::new();
    let mut at = [0.0f32; 2];
    let mut command = 'M';
    let mut i = 0;
    let take = |i: &mut usize| -> f32 {
        let Token::Number(n) = tokens[*i] else {
            panic!("a number at {i}");
        };
        *i += 1;
        n
    };
    while i < tokens.len() {
        if let Token::Command(c) = tokens[i] {
            command = c;
            i += 1;
            if c == 'Z' {
                continue;
            }
        }
        match command {
            'M' | 'L' => {
                at = [take(&mut i), take(&mut i)];
                out.push(at);
            }
            'H' => {
                at[0] = take(&mut i);
                out.push(at);
            }
            'V' => {
                at[1] = take(&mut i);
                out.push(at);
            }
            'A' => {
                let r = take(&mut i);
                let _ry = take(&mut i);
                let _rotation = take(&mut i);
                let large = take(&mut i) != 0.0;
                let sweep = take(&mut i) != 0.0;
                let to = [take(&mut i), take(&mut i)];
                out.extend(arc(at, to, r, large, sweep));
                at = to;
            }
            _ => panic!("command {command}"),
        }
    }
    // The last point closes onto the first.
    if out.len() > 1 && out.first() == out.last() {
        out.pop();
    }
    out
}

#[derive(Clone, Copy)]
enum Token {
    Command(char),
    Number(f32),
}

/// A circular arc from `from` to `to` (SVG's endpoint form, F.6.5), its
/// points after `from`, `to` the last.
fn arc(from: [f32; 2], to: [f32; 2], r: f32, large: bool, sweep: bool) -> Vec<[f32; 2]> {
    let (x1, y1) = ((from[0] - to[0]) / 2.0, (from[1] - to[1]) / 2.0);
    let mut r = r.abs();
    let lambda = (x1 * x1 + y1 * y1) / (r * r);
    if lambda > 1.0 {
        r *= lambda.sqrt();
    }
    let num = (r * r * r * r - r * r * y1 * y1 - r * r * x1 * x1).max(0.0);
    let den = r * r * y1 * y1 + r * r * x1 * x1;
    let sign = if large != sweep { 1.0 } else { -1.0 };
    let coef = sign * (num / den).sqrt();
    let (cx1, cy1) = (coef * y1, -coef * x1);
    let c = [cx1 + (from[0] + to[0]) / 2.0, cy1 + (from[1] + to[1]) / 2.0];
    let start = ((y1 - cy1) / r).atan2((x1 - cx1) / r);
    let end = ((-y1 - cy1) / r).atan2((-x1 - cx1) / r);
    let mut delta = end - start;
    if sweep && delta < 0.0 {
        delta += std::f32::consts::TAU;
    } else if !sweep && delta > 0.0 {
        delta -= std::f32::consts::TAU;
    }
    let n = ((delta.abs() / std::f32::consts::PI * 32.0).ceil() as usize).max(2);
    let mut out: Vec<[f32; 2]> = (1..n)
        .map(|i| {
            let a = start + delta * i as f32 / n as f32;
            [c[0] + r * a.cos(), c[1] + r * a.sin()]
        })
        .collect();
    out.push(to);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_contours_are_the_rains_own() {
        let svg = include_str!("../../../../assets/ui/menu-rain.svg");
        assert!(svg.contains(&format!("id=\"arrow\" d=\"{ARROW}\"")));
        assert!(svg.contains(&format!("id=\"onigiri\" d=\"{ONIGIRI}\"")));
        // The rain's heaviest row is the play's weight.
        assert!(svg.contains("stroke-width=\"4.0657\""));
    }

    #[test]
    fn the_arrows_arcs_are_its_round_ends() {
        let points = flatten(ARROW);
        // The wing's end: the semicircle's far point, past its chord.
        let far = points.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max);
        assert!((far - (47.6255 + ROUND)).abs() < 0.05, "{far}");
        let bottom = points.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
        assert!((bottom - (47.626 + ROUND)).abs() < 0.05, "{bottom}");
        // Mirrored about the middle.
        let near = points.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
        assert!((near + far - 64.0).abs() < 0.05, "{near} {far}");
    }

    #[test]
    fn every_shape_sits_in_the_middle_of_its_cell() {
        for shape in Shape::ALL {
            let points = contour(shape);
            for p in &points {
                // Inside its cell with its stroke and room to turn.
                let d = (p[0] - 32.0).hypot(p[1] - 32.0);
                assert!(d < 31.0 - Weight::Bold.units() / 2.0, "{shape:?} {p:?}");
            }
        }
    }

    #[test]
    fn the_atlas_is_one_outline_a_glyph_never_doubled() {
        let rgba = atlas();
        let [w, _] = SIZE;
        assert_eq!(rgba.len(), (SIZE[0] * SIZE[1] * 4) as usize);
        let alpha = |x: u32, y: u32| rgba[((y * w + x) * 4 + 3) as usize];
        // White everywhere, so the mipmaps never darken an edge.
        assert!(rgba.chunks(4).all(|p| p[..3] == [255, 255, 255]));
        let k = CELL as f32 / 64.0;
        let px = |u: f32| (u * k) as u32;
        // The arrow (column 0, the regular row): its shaft hollow, its
        // outline opaque where the wings meet the shaft (no joint drawn
        // twice: an outline's coverage is its distance's alone).
        let shift = {
            let c = contour(Shape::Arrow);
            let raw = flatten(ARROW);
            [c[0][0] - raw[0][0], c[0][1] - raw[0][1]]
        };
        let at = |u: f32, v: f32| alpha(px(u + shift[0]), px(v + shift[1]));
        assert_eq!(at(32.0, 44.0), 0, "the shaft is hollow");
        assert_eq!(at(40.75, 44.0), 255, "the shaft's side is drawn");
        assert_eq!(at(32.0, 4.5), 255, "the tip is drawn");
        assert_eq!(at(2.0, 2.0), 0, "the corner is empty");
        // Nothing of the cell spills into its neighbors: each cell's
        // border is empty.
        for c in 0..8 {
            let (ox, oy) = ((c % 4) * CELL, (c / 4) * CELL);
            for i in 0..CELL {
                for (x, y) in [(ox + i, oy), (ox, oy + i), (ox + i, oy + CELL - 1), (ox + CELL - 1, oy + i)] {
                    assert_eq!(alpha(x, y), 0, "cell {c} at {x},{y}");
                }
            }
        }
    }

    #[test]
    fn each_glyph_has_its_cell_and_its_receptors_turn() {
        let arrow = |rotation_deg| Glyph::Arrow { rotation_deg };
        // Left, down, up, right as the layouts turn them.
        assert_eq!(cell(&arrow(0.0), Weight::Regular).1, -90.0);
        assert_eq!(cell(&arrow(-90.0), Weight::Regular).1, -180.0);
        assert_eq!(cell(&arrow(90.0), Weight::Regular).1, 0.0);
        assert_eq!(cell(&arrow(180.0), Weight::Regular).1, 90.0);
        // A diagonal (up-left) turns between them.
        assert_eq!(cell(&arrow(45.0), Weight::Regular).1, -45.0);
        assert_eq!(cell(&Glyph::Onigiri, Weight::Bold), ([0.25, 0.5, 0.5, 1.0], 0.0));
        assert_eq!(cell(&Glyph::Center, Weight::Regular).0, [0.5, 0.0, 0.75, 0.5]);
        assert_eq!(cell(&Glyph::Giko, Weight::Regular).0, [0.75, 0.0, 1.0, 0.5]);
    }
}
