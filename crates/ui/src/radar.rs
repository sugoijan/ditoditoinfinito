//! A chart's skill radar on song select (`docs/plans/msd-ratings.md`): the
//! seven skillsets of its MSD (Stream at the top, then clockwise
//! Jumpstream, Handstream, Stamina, JackSpeed, Chordjack, Technical), each
//! spoke's length its skillset as a share of the chart's overall rating
//! (the outer ring is the overall; the rings a quarter apart), so the
//! shapes of two charts compare as profiles while the number says how
//! hard.
//!
//! A vector shape of our own, as the stars and the heart are, drawn from
//! quads alone: the rings and spokes as thin rounded bars, the shape's
//! fill as seven triangles (each a bar along its outer edge, cut to its
//! wedge by two frames turned along its spokes: clips the 2D pass cuts
//! with its stencil), its outline as bars over the fill's edges. The
//! component ([`radar`]) is sized by its side and takes its colors from
//! the look (`Radar.Panel`, `Radar.Grid`, `Radar.Fill`, `Radar.Line`; a
//! look without them gets [`COLORS`]).

use std::f32::consts::{FRAC_PI_2, TAU};

use ddi_actor::{Actor, Rgba};

use crate::look::{call, quad};
use crate::screens::{Env, frame};

/// The seven skillsets.
pub const SKILLS: usize = 7;

/// A plain look's colors: the panel, the rings and spokes, the fill, the
/// outline.
pub const COLORS: [Rgba; 4] = [
    [0.04, 0.035, 0.05, 0.82],
    [0.23, 0.21, 0.26, 1.0],
    [0.37, 0.82, 1.0, 0.32],
    [0.37, 0.82, 1.0, 1.0],
];

/// The skillsets' shares of the overall (0 to 1), from the eight numbers
/// (overall first).
pub fn shares(values: &[f32; 8]) -> [f32; SKILLS] {
    let overall = values[0];
    let mut out = [0.0; SKILLS];
    if overall > 0.0 {
        for (k, s) in out.iter_mut().enumerate() {
            *s = (values[k + 1] / overall).clamp(0.0, 1.0);
        }
    }
    out
}

/// The direction of spoke `k`: the first straight up, then clockwise.
pub fn direction(k: usize) -> [f32; 2] {
    let a = -FRAC_PI_2 + TAU * k as f32 / SKILLS as f32;
    [a.cos(), a.sin()]
}

/// The shape's corners for `shares` on a radar of `radius` around the
/// origin.
pub fn corners(shares: &[f32; SKILLS], radius: f32) -> [[f32; 2]; SKILLS] {
    std::array::from_fn(|k| {
        let [x, y] = direction(k);
        let r = shares[k] * radius;
        [x * r, y * r]
    })
}

fn angle(v: [f32; 2]) -> f32 {
    v[1].atan2(v[0])
}

fn rotate(p: [f32; 2], a: f32) -> [f32; 2] {
    let (s, c) = a.sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
}

/// A bar from `a` to `b`, `width` thick with rounded ends.
fn bar(a: [f32; 2], b: [f32; 2], width: f32, color: Rgba) -> Actor {
    let d = [b[0] - a[0], b[1] - a[1]];
    let len = d[0].hypot(d[1]);
    let mut q = quad("", [len + width, width], color);
    call(&mut q, "cornerradius", &[width / 2.0]);
    call(&mut q, "rotationz", &[angle(d).to_degrees()]);
    call(&mut q, "xy", &[(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]);
    q
}

/// Room enough for any clip.
const FAR: f32 = 1.0e4;

/// The triangle (origin, `a`, `b`) filled in `color`: a bar along `ab`
/// toward the origin, cut by a frame turned along the origin's spoke to
/// `a` (keeping `b`'s side) holding one turned along the spoke to `b`
/// (keeping `a`'s side). `None` for a triangle too thin to draw.
fn sector(a: [f32; 2], b: [f32; 2], color: Rgba) -> Option<Actor> {
    let cross = a[0] * b[1] - a[1] * b[0];
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ab_len = ab[0].hypot(ab[1]);
    if cross.abs() < 1e-3 || ab_len < 1e-3 {
        return None;
    }
    // The bar: its far side on ab, as long as ab and more, reaching past
    // the origin (the wedge cuts the rest).
    let height = cross.abs() / ab_len;
    let n = {
        // The unit normal of ab toward the origin.
        let n = [-ab[1] / ab_len, ab[0] / ab_len];
        let toward = -(n[0] * a[0] + n[1] * a[1]);
        if toward >= 0.0 { n } else { [-n[0], -n[1]] }
    };
    let depth = height * 2.0 + 2.0;
    let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    let center = [mid[0] + n[0] * depth / 2.0, mid[1] + n[1] * depth / 2.0];
    let (aa, ab_angle, ba) = (angle(a), angle(ab), angle(b));
    // In the inner frame's space (turned by b's angle in all).
    let mut fill = quad("", [ab_len * 3.0 + 4.0, depth], color);
    call(&mut fill, "rotationz", &[(ab_angle - ba).to_degrees()]);
    let at = rotate(center, -ba);
    call(&mut fill, "xy", &at);
    // The inner frame: along the spoke to b, keeping a's side.
    let a_in_b = rotate(a, -ba);
    let mut inner = frame("", vec![fill]);
    inner.clip = Some(if a_in_b[1] > 0.0 {
        [-FAR, 0.0, FAR, FAR]
    } else {
        [-FAR, -FAR, FAR, 0.0]
    });
    call(&mut inner, "rotationz", &[(ba - aa).to_degrees()]);
    // The outer frame: along the spoke to a, keeping b's side.
    let b_in_a = rotate(b, -aa);
    let mut outer = frame("", vec![inner]);
    outer.clip = Some(if b_in_a[1] > 0.0 {
        [-FAR, 0.0, FAR, FAR]
    } else {
        [-FAR, -FAR, FAR, 0.0]
    });
    call(&mut outer, "rotationz", &[aa.to_degrees()]);
    Some(outer)
}

/// The radar's shape around the origin, `radius` long spokes, in
/// `colors` (panel unused here): rings, spokes, the filled shape and its
/// outline. A frame named `Shape`, never hit.
pub fn shape(values: &[f32; 8], radius: f32, colors: [Rgba; 4]) -> Actor {
    let [_, grid, fill, line] = colors;
    let thin = (radius / 60.0).max(1.0);
    let mut parts = Vec::new();
    // The rings, a quarter of the overall apart, and the spokes.
    for ring in 1..=4 {
        let r = radius * ring as f32 / 4.0;
        for k in 0..SKILLS {
            let [x0, y0] = direction(k);
            let [x1, y1] = direction((k + 1) % SKILLS);
            parts.push(bar([x0 * r, y0 * r], [x1 * r, y1 * r], thin, grid));
        }
    }
    for k in 0..SKILLS {
        let [x, y] = direction(k);
        parts.push(bar([0.0, 0.0], [x * radius, y * radius], thin, grid));
    }
    // The shape: filled, then outlined, with a dot at each corner.
    let points = corners(&shares(values), radius);
    for k in 0..SKILLS {
        if let Some(s) = sector(points[k], points[(k + 1) % SKILLS], fill) {
            parts.push(s);
        }
    }
    let stroke = (radius / 30.0).max(1.5);
    for k in 0..SKILLS {
        parts.push(bar(points[k], points[(k + 1) % SKILLS], stroke, line));
    }
    for p in points {
        let mut dot = quad("", [stroke * 2.2, stroke * 2.2], line);
        call(&mut dot, "cornerradius", &[stroke * 1.1]);
        call(&mut dot, "xy", &p);
        parts.push(dot);
    }
    let mut a = frame("Shape", parts);
    a.decoration = true;
    a
}

/// The look's radar colors, the plain ones where it has none.
pub fn colors(env: &Env) -> [Rgba; 4] {
    let mut out = COLORS;
    for (c, name) in out
        .iter_mut()
        .zip(["Radar.Panel", "Radar.Grid", "Radar.Fill", "Radar.Line"])
    {
        if let Some(v) = env.look.color(name) {
            *c = v;
        }
    }
    out
}

/// About how wide `s` is at `size` in the look's text face (Iosevka Term:
/// half an em a Latin letter, a whole em a CJK one), to keep labels in.
fn width_of(s: &str, size: f32) -> f32 {
    s.chars()
        .map(|c| if (c as u32) >= 0x2E80 { 1.0 } else { 0.55 })
        .sum::<f32>()
        * size
}

/// The radar component, `size` (width, height), named `name`, its middle
/// at the origin: a translucent panel, the shape, each spoke's short label
/// (`labels`, Stream first); with `list` (the overall's line, then a line
/// a skillset, each a name and its number) the numbers listed instead,
/// while the pointer is on it. The panel takes the pointer (the hover);
/// the rest is drawn only.
pub fn radar(
    env: &Env,
    name: &str,
    values: &[f32; 8],
    size: [f32; 2],
    labels: &[String; SKILLS],
    list: Option<&[(String, String)]>,
) -> Actor {
    let [w, h] = size;
    let colors = colors(env);
    let corner = h.min(w) * 0.06;
    let mut panel = quad("Panel", [w, h], colors[0]);
    call(&mut panel, "cornerradius", &[corner]);
    let mut parts = vec![panel];
    match list {
        None => {
            let label_size = (h / 14.0).clamp(9.0, 12.0);
            let widest = labels
                .iter()
                .map(|l| width_of(l, label_size))
                .fold(0.0, f32::max);
            // Room for the labels round the shape: above and below, and
            // beside it at the sides.
            let radius = (h / 2.0 - label_size * 1.9)
                .min(w / 2.0 - widest - label_size * 0.9)
                .max(8.0);
            parts.push(shape(values, radius, colors));
            for (k, label) in labels.iter().enumerate() {
                let [x, y] = direction(k);
                let r = radius + label_size * 0.8;
                let mut t = env.text("", label, label_size, 500, "Secondary");
                call(&mut t, "xy", &[x * r, y * r + y * label_size * 0.3]);
                // Right of the middle to the right, left of it to the left.
                let align = if x > 0.25 {
                    0.0
                } else if x < -0.25 {
                    1.0
                } else {
                    0.5
                };
                call(&mut t, "halign", &[align]);
                t.decoration = true;
                parts.push(t);
            }
        }
        Some(lines) => {
            let pad = (h / 18.0).clamp(6.0, 10.0);
            let line_h = (h - 2.0 * pad) / lines.len().max(1) as f32;
            let text = (line_h * 0.7).clamp(9.0, 13.0);
            for (i, (label, number)) in lines.iter().enumerate() {
                let y = -h / 2.0 + pad + line_h * (i as f32 + 0.5);
                let (color, weight) = if i == 0 { ("Text", 700) } else { ("Secondary", 500) };
                let mut t = env.text("", label, text, weight, color);
                crate::look::ellipsized(&mut t, w - 3.0 * pad - text * 2.5);
                call(&mut t, "xy", &[-w / 2.0 + pad * 1.5, y]);
                call(&mut t, "halign", &[0.0]);
                t.decoration = true;
                let mut n = env.numbers("", number, text, color);
                call(&mut n, "xy", &[w / 2.0 - pad * 1.5, y]);
                call(&mut n, "halign", &[1.0]);
                n.decoration = true;
                parts.extend([t, n]);
            }
        }
    }
    frame(name, parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_are_of_the_overall_and_spokes_go_clockwise_from_the_top() {
        let v = [20.0, 20.0, 10.0, 30.0, 0.0, 5.0, 15.0, 19.0];
        let s = shares(&v);
        assert_eq!(s[0], 1.0);
        assert_eq!(s[1], 0.5);
        assert_eq!(s[2], 1.0, "capped at the overall");
        assert_eq!(s[3], 0.0);
        assert_eq!(shares(&[0.0; 8]), [0.0; SKILLS]);
        let up = direction(0);
        assert!(up[0].abs() < 1e-6 && (up[1] + 1.0).abs() < 1e-6);
        // The second spoke to the right of the first (clockwise on a
        // screen, y down).
        assert!(direction(1)[0] > 0.0);
        let c = corners(&s, 10.0);
        assert!((c[0][1] + 10.0).abs() < 1e-4);
    }

    #[test]
    fn a_sector_is_a_bar_in_two_turned_clips() {
        let s = sector([0.0, -10.0], [8.0, -6.0], [1.0; 4]).expect("drawn");
        let outer_clip = s.clip.expect("outer clip");
        let ddi_actor::Kind::Frame(inner) = &s.kind else {
            panic!()
        };
        assert!(inner[0].clip.is_some());
        assert!(outer_clip[0] < 0.0 && outer_clip[2] > 0.0);
        // Too thin: none.
        assert!(sector([0.0, -10.0], [0.0, -5.0], [1.0; 4]).is_none());
    }
}
