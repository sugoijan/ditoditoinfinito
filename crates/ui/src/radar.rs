//! A chart's skill radar on song select (`docs/plans/msd-ratings.md`): the
//! seven skillsets of its MSD (Stream at the top, then clockwise
//! Jumpstream, Handstream, Stamina, JackSpeed, Chordjack, Technical), each
//! spoke's length its skillset as a share of the chart's overall rating
//! (the outer ring is the overall; the rings a quarter apart), so the
//! shapes of two charts compare as profiles while the numbers say how
//! hard: each spoke's name and number round it, the overall's in its
//! corner (since round Z1, in the ladder's place, [`place`]'s label rule).
//!
//! A vector shape of our own, as the stars and the heart are, drawn from
//! quads alone: the rings and spokes as thin rounded bars, the shape's
//! fill as seven triangles (each a bar along its outer edge, cut to its
//! wedge by two frames turned along its spokes: clips the 2D pass cuts
//! with its stencil), its outline as bars over the fill's edges. The
//! component ([`radar`]) is sized by its rectangle and takes its colors
//! from the look (`Radar.Panel`, `Radar.Grid`, `Radar.Fill`, `Radar.Line`;
//! a look without them gets [`COLORS`]); [`note`] is its place's quiet
//! line where a chart has none.

use std::f32::consts::{FRAC_PI_2, TAU};

use ddi_actor::{Actor, Rgba};

use crate::look::{call, quad};
use crate::screens::{Env, frame};

/// MSD's seven skillsets (a radar takes as many spokes as it is given:
/// the DDI estimate's eight, `ddi_engine::cpu::SKILL_NAMES`).
pub const SKILLS: usize = 7;

/// A plain look's colors: the panel, the rings and spokes, the fill, the
/// outline.
pub const COLORS: [Rgba; 4] = [
    [0.04, 0.035, 0.05, 0.82],
    [0.23, 0.21, 0.26, 1.0],
    [0.37, 0.82, 1.0, 0.32],
    [0.37, 0.82, 1.0, 1.0],
];

/// The DDI estimate's radar (round Z3, the maintainer: "set a minimum
/// value of 1 for each skill and grow everything proportionally to the
/// difficulty"): each skill's value `1 + e`, `e = cost / (cost +`
/// [`COST_HALF`]`)`, so nothing below the floor and a standout toward
/// twice it; a spoke `base(decimal) × (1 + e ×` [`GROWTH`]`)` of the
/// radius, `base` rising linearly from [`BASE_AT_1`] at 1 to
/// [`BASE_AT_19`] at 19. A chart with no costs is a regular octagon whose
/// size says its difficulty; the longest spoke there can be (19, a cost
/// without end) reaches the outer ring.
///
/// The cost, in points of Accuracy, at which a skill is halfway to its
/// most: a typical chart's Stream costs about 2.
pub const COST_HALF: f32 = 2.0;
/// How far past the base a skill's spoke reaches at its most: six tenths
/// of it, a bulge rather than a spike, and room enough for the base to
/// grow 2.8 times from 1 to 19.
pub const GROWTH: f32 = 0.6;
/// The base octagon at 1 and at 19, as fractions of the radius: 0.22
/// keeps the easiest chart's octagon a clear octagon (14 px at the 1280 ×
/// 720 radar's 64) inside its labels; at 19, `1 / (1 + GROWTH)`, so the
/// longest spoke ends at the outer ring.
pub const BASE_AT_1: f32 = 0.22;
pub const BASE_AT_19: f32 = 1.0 / (1.0 + GROWTH);

/// The base octagon's size for an estimate of `decimal` (1 to 19), as a
/// fraction of the radius.
pub fn estimate_base(decimal: f32) -> f32 {
    let t = ((decimal - 1.0) / 18.0).clamp(0.0, 1.0);
    BASE_AT_1 + (BASE_AT_19 - BASE_AT_1) * t
}

/// The DDI estimate's spokes, as fractions of the radius, for a chart of
/// `decimal` whose skills cost `costs` points of Accuracy.
pub fn estimate_spokes(decimal: f32, costs: &[f32; 8]) -> Vec<f32> {
    let base = estimate_base(decimal);
    costs
        .iter()
        .map(|c| {
            let c = c.max(0.0);
            base * (1.0 + c / (c + COST_HALF) * GROWTH)
        })
        .collect()
}

/// The DDI estimate's rings: the base octagon at 5, 10 and 15 (so two
/// charts' sizes compare), and the outer ring.
pub fn estimate_rings() -> [f32; 4] {
    [estimate_base(5.0), estimate_base(10.0), estimate_base(15.0), 1.0]
}

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

/// The direction of spoke `k` of `n`: the first straight up, then
/// clockwise.
pub fn direction(k: usize, n: usize) -> [f32; 2] {
    let a = -FRAC_PI_2 + TAU * k as f32 / n.max(1) as f32;
    [a.cos(), a.sin()]
}

/// The shape's corners for `shares` (a spoke each) on a radar of `radius`
/// around the origin.
pub fn corners(shares: &[f32], radius: f32) -> Vec<[f32; 2]> {
    let n = shares.len();
    (0..n)
        .map(|k| {
            let [x, y] = direction(k, n);
            let r = shares[k].clamp(0.0, 1.0) * radius;
            [x * r, y * r]
        })
        .collect()
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
/// `colors` (panel unused here): rings, spokes, the filled shape of
/// `shares` (each skillset's share of the overall, [`shares`]; a morph's
/// on its way) and its outline. A frame named `Shape`, never hit.
pub fn shape(shares: &[f32], radius: f32, colors: [Rgba; 4]) -> Actor {
    shape_with(shares, radius, colors, None)
}

/// [`shape`] with its rings where `rings` says (fractions of the radius:
/// the DDI estimate's difficulty marks, [`estimate_rings`]), else a
/// quarter of the outer ring apart.
pub fn shape_with(shares: &[f32], radius: f32, colors: [Rgba; 4], rings: Option<&[f32]>) -> Actor {
    let [_, grid, fill, line] = colors;
    let n = shares.len();
    let thin = (radius / 60.0).max(1.0);
    let mut parts = Vec::new();
    // The rings, a quarter of the outer ring apart (or where asked), and
    // the spokes.
    let at: Vec<f32> = match rings {
        Some(r) => r.iter().map(|f| radius * f).collect(),
        None => (1..=4).map(|ring| radius * ring as f32 / 4.0).collect(),
    };
    for r in at {
        for k in 0..n {
            let [x0, y0] = direction(k, n);
            let [x1, y1] = direction((k + 1) % n, n);
            parts.push(bar([x0 * r, y0 * r], [x1 * r, y1 * r], thin, grid));
        }
    }
    for k in 0..n {
        let [x, y] = direction(k, n);
        parts.push(bar([0.0, 0.0], [x * radius, y * radius], thin, grid));
    }
    // The shape: filled, then outlined, with a dot at each corner.
    let points = corners(shares, radius);
    for k in 0..n {
        if let Some(s) = sector(points[k], points[(k + 1) % n], fill) {
            parts.push(s);
        }
    }
    let stroke = (radius / 30.0).max(1.5);
    for k in 0..n {
        parts.push(bar(points[k], points[(k + 1) % n], stroke, line));
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
/// half an em a Latin letter, a whole em a CJK one; a little over, so a
/// label kept in by it is kept in), to keep labels in and apart.
pub fn width_of(s: &str, size: f32) -> f32 {
    s.chars()
        .map(|c| if (c as u32) >= 0x2E80 { 1.0 } else { 0.55 })
        .sum::<f32>()
        * size
}

/// A text's line height at `size`.
fn line(size: f32) -> f32 {
    size * 1.3
}

/// What the radar says round its shape: each spoke's full and short name
/// (a name each, as many as the spokes, Stream first), and the word before
/// the caption's number ("MSD").
#[derive(Clone, Copy, Debug)]
pub struct Words<'a> {
    pub full: &'a [String],
    pub short: &'a [String],
    pub overall: &'a str,
}

/// A rectangle, `[x0, y0, x1, y1]` from the radar's middle.
pub type Rect = [f32; 4];

/// Whether two rectangles share any area.
pub fn overlap(a: Rect, b: Rect) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

/// Where everything of a radar `size` large goes ([`place`]): the shape's
/// radius, the labels' text size and names, each spoke's label (its name
/// over its number) and the overall's caption in the upper left corner.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub radius: f32,
    pub text: f32,
    /// The full names (else the short ones).
    pub full: bool,
    pub labels: Vec<Rect>,
    pub caption: Rect,
}

/// The label rule (`docs/plans/msd-ratings.md`, round Z1): the labels'
/// text is the radar's size (a sixteenth of its height, 9 to 13 px; the
/// numbers 2 px larger); a
/// spoke's label is its skillset's name over its number, put beyond the
/// spoke's end, away from the middle (right of a spoke going right, left
/// of one going left, above one going up, below one going down, centered
/// otherwise); the full names where the shape keeps at least four-fifths
/// of the radius the short names leave it, else the short names; where
/// neither fits inside the radar without two labels (or a label and the
/// caption) overlapping, the text one pixel smaller, down to 9 px; the
/// shape's radius is whatever the labels leave it, never under
/// [`MIN_RADIUS`]. Nothing is cut and nothing overlaps.
/// `numbers` is the caption's number, then a spoke's each.
pub fn place(size: [f32; 2], words: &Words, numbers: &[String]) -> Placed {
    let mut text = (size[1] / 16.0).clamp(9.0, 13.0).floor();
    while text >= 9.0 {
        let full = fit(size, text, words.full, words.overall, numbers);
        let short = fit(size, text, words.short, words.overall, numbers);
        match (full, short) {
            (Some(f), Some(s)) if f.radius >= (s.radius * 0.8).floor() => return f.named(true),
            (_, Some(s)) => return s.named(false),
            (Some(f), None) => return f.named(true),
            (None, None) => text -= 1.0,
        }
    }
    // Too small for any (no screen lays one out so small): the short
    // names at the least size, the shape at the least radius.
    layout_at(size, 9.0, words.short, words.overall, numbers, MIN_RADIUS).named(false)
}

/// How much larger a label's number is than its name.
const NUMBER_UP: f32 = 2.0;

/// The least radius the shape is drawn at.
pub const MIN_RADIUS: f32 = 12.0;

impl Placed {
    fn named(mut self, full: bool) -> Placed {
        self.full = full;
        self
    }
}

/// The largest radius at which `names` at `text` fit inside `size`
/// without overlapping: `None` where none does.
fn fit(
    size: [f32; 2],
    text: f32,
    names: &[String],
    overall: &str,
    numbers: &[String],
) -> Option<Placed> {
    let mut r = (size[0].min(size[1]) / 2.0).floor();
    while r >= MIN_RADIUS {
        let p = layout_at(size, text, names, overall, numbers, r);
        if fits(size, &p) {
            return Some(p);
        }
        r -= 1.0;
    }
    None
}

/// The room kept free round the radar's edges.
fn pad(size: [f32; 2]) -> f32 {
    (size[1] / 24.0).clamp(4.0, 10.0)
}

fn fits(size: [f32; 2], p: &Placed) -> bool {
    let [w, h] = size;
    let m = pad(size);
    let inside = |r: &Rect| {
        r[0] >= -w / 2.0 + m && r[2] <= w / 2.0 - m && r[1] >= -h / 2.0 + m && r[3] <= h / 2.0 - m
    };
    let mut all: Vec<Rect> = p.labels.to_vec();
    all.push(p.caption);
    all.iter().enumerate().all(|(i, a)| {
        inside(a)
            && !meets_shape(*a, p.radius, p.labels.len())
            && all[i + 1..].iter().all(|b| !overlap(*a, *b))
    })
}

/// Whether a rectangle reaches into the shape's polygon of `n` corners
/// (at the radius).
pub fn meets_shape(r: Rect, radius: f32, n: usize) -> bool {
    // The polygon's corners; a rectangle meets it where a corner of the
    // rectangle is inside it or a corner of it is inside the rectangle.
    let pts: Vec<[f32; 2]> = (0..n)
        .map(|k| {
            let [x, y] = direction(k, n);
            [x * radius, y * radius]
        })
        .collect();
    let inside_poly = |p: [f32; 2]| {
        (0..n).all(|k| {
            let a = pts[k];
            let b = pts[(k + 1) % n];
            (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]) >= 0.0
        })
    };
    let corners = [[r[0], r[1]], [r[2], r[1]], [r[0], r[3]], [r[2], r[3]]];
    corners.iter().any(|&c| inside_poly(c))
        || pts
            .iter()
            .any(|p| p[0] > r[0] && p[0] < r[2] && p[1] > r[1] && p[1] < r[3])
}

/// Everything at `radius`.
fn layout_at(
    size: [f32; 2],
    text: f32,
    names: &[String],
    overall: &str,
    numbers: &[String],
    radius: f32,
) -> Placed {
    let gap = text * 0.5;
    let n = names.len();
    let labels = (0..n).map(|k| {
        let [dx, dy] = direction(k, n);
        let tw = width_of(&names[k], text).max(width_of(&numbers[k + 1], text + NUMBER_UP));
        let th = line(text) + line(text + NUMBER_UP);
        let [ax, ay] = [dx * (radius + gap), dy * (radius + gap)];
        let x0 = if dx > 0.25 {
            ax
        } else if dx < -0.25 {
            ax - tw
        } else {
            ax - tw / 2.0
        };
        let y0 = if dy < -0.5 {
            ay - th
        } else if dy > 0.5 {
            ay
        } else {
            ay - th / 2.0
        };
        [x0, y0, x0 + tw, y0 + th]
    }).collect();
    let m = pad(size);
    let [w, h] = size;
    let cap_w = width_of(overall, text) + text * 0.5 + width_of(&numbers[0], text + 4.0);
    let cap = [-w / 2.0 + m, -h / 2.0 + m, -w / 2.0 + m + cap_w, -h / 2.0 + m + line(text + 4.0)];
    Placed {
        radius,
        text,
        full: true,
        labels,
        caption: cap,
    }
}

/// The radar component, `size` (width, height), named `name`, its middle
/// at the origin: a translucent panel, the shape (`shares`, a spoke each,
/// which may be a morph's on its way), each spoke's name and number round
/// it (`numbers[k + 1]`), the caption ("MSD 24.3": `words.overall` and
/// `numbers[0]`) in its upper left corner; laid out by [`place`]. Drawn only: the panel is named `Panel`, each
/// label `Label<k>` (its parts `Name` and `Number`), the caption `Caption`.
pub fn radar(
    env: &Env,
    name: &str,
    numbers: &[String],
    shares: &[f32],
    rings: Option<&[f32]>,
    size: [f32; 2],
    words: &Words,
) -> Actor {
    let [w, h] = size;
    let colors = colors(env);
    let n = shares.len();
    let p = place(size, words, numbers);
    let mut panel = quad("Panel", [w, h], colors[0]);
    call(&mut panel, "cornerradius", &[h.min(w) * 0.06]);
    let mut parts = vec![panel, shape_with(shares, p.radius, colors, rings)];
    let names = if p.full { words.full } else { words.short };
    for (k, rect) in p.labels.iter().enumerate() {
        let [dx, _] = direction(k, n);
        let (align, x) = if dx > 0.25 {
            (0.0, rect[0])
        } else if dx < -0.25 {
            (1.0, rect[2])
        } else {
            (0.5, (rect[0] + rect[2]) / 2.0)
        };
        let mut n = env.text("Name", &names[k], p.text, 500, "Secondary");
        call(&mut n, "halign", &[align]);
        call(&mut n, "xy", &[x, rect[1] + line(p.text) / 2.0]);
        let mut v = env.numbers("Number", &numbers[k + 1], p.text + NUMBER_UP, "Text");
        call(&mut v, "halign", &[align]);
        call(&mut v, "xy", &[x, rect[1] + line(p.text) + line(p.text + NUMBER_UP) / 2.0]);
        let mut label = frame(&format!("Label{k}"), vec![n, v]);
        label.decoration = true;
        parts.push(label);
    }
    let c = p.caption;
    let mut word = env.text("Word", words.overall, p.text, 600, "Muted");
    call(&mut word, "halign", &[0.0]);
    call(&mut word, "xy", &[c[0], (c[1] + c[3]) / 2.0]);
    let mut overall = env.numbers("Overall", &numbers[0], p.text + 4.0, "Text");
    call(&mut overall, "halign", &[0.0]);
    call(&mut overall, "xy", &[c[0] + width_of(words.overall, p.text) + p.text * 0.5, (c[1] + c[3]) / 2.0]);
    let mut caption = frame("Caption", vec![word, overall]);
    caption.decoration = true;
    parts.push(caption);
    frame(name, parts)
}

/// A rating as the radar writes it: to a tenth.
pub fn number(x: f32) -> String {
    format!("{x:.1}")
}

/// The radar's place where the chart has no radar: the panel, `size`
/// large, its middle at the origin, with `line` quietly in its middle (a
/// rating on its way, or none for the chart). Named `name`; the line is
/// its `Line`.
pub fn note(env: &Env, name: &str, size: [f32; 2], line: &str) -> Actor {
    let [w, h] = size;
    let colors = colors(env);
    let mut panel = quad("Panel", [w, h], colors[0]);
    call(&mut panel, "cornerradius", &[h.min(w) * 0.06]);
    let mut t = env.text("Line", line, 13.0, 500, "Muted");
    crate::look::ellipsized(&mut t, w - 24.0);
    t.decoration = true;
    frame(name, vec![panel, t])
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
        let up = direction(0, SKILLS);
        assert!(up[0].abs() < 1e-6 && (up[1] + 1.0).abs() < 1e-6);
        // The second spoke to the right of the first (clockwise on a
        // screen, y down).
        assert!(direction(1, SKILLS)[0] > 0.0);
        // Eight spokes: the third straight to the right.
        assert!((direction(2, 8)[0] - 1.0).abs() < 1e-6);
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

/// MSD's radar draws as it did before the DDI estimate's floor and
/// difficulty marks (round Z3): its shape's geometry for fixed numbers,
/// pinned on the code before (`crate::testing::rounded`).
#[cfg(test)]
#[test]
fn msd_s_shape_draws_as_pinned() {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let v = [24.3f32, 22.1, 18.0, 9.5, 23.0, 14.2, 11.0, 20.7];
    let mut s = DefaultHasher::new();
    crate::testing::rounded(&format!("{:?}", shape(&shares(&v), 64.0, COLORS))).hash(&mut s);
    assert_eq!(s.finish(), 1930352012017716584);
}

/// Round Z3: the DDI estimate's radar: a chart with no costs is a regular
/// octagon whose size rises with its estimate (1, 10, 19); a standout
/// skill bulges past the base, every other spoke at least the base; the
/// longest spoke there can be ends inside the outer ring.
#[cfg(test)]
#[test]
fn the_estimate_s_octagon_grows_with_difficulty() {
    let flat = |d: f32| estimate_spokes(d, &[0.0; 8]);
    let sizes: Vec<f32> = [1.0, 10.0, 19.0]
        .iter()
        .map(|&d| {
            let s = flat(d);
            assert!(s.iter().all(|x| (x - s[0]).abs() < 1e-6), "regular at {d}: {s:?}");
            s[0]
        })
        .collect();
    assert!(sizes[0] < sizes[1] && sizes[1] < sizes[2], "{sizes:?}");
    assert!((sizes[0] - BASE_AT_1).abs() < 1e-6);
    // A standout: a bulge, the others at the base.
    let s = estimate_spokes(11.3, &[1.65, 0.0, 0.0, 0.32, 16.9, 0.0, 0.01, 0.0]);
    let base = estimate_base(11.3);
    assert!(s.iter().all(|x| *x >= base - 1e-6), "{s:?}");
    let jack = s[4];
    assert!(s.iter().all(|x| *x <= jack), "{s:?}");
    assert!(jack > base * 1.4, "a bulge: {jack} against {base}");
    // The longest there can be: inside the outer ring.
    let most = estimate_spokes(19.0, &[1.0e9; 8]);
    assert!(most.iter().all(|x| *x <= 1.0 + 1e-6), "{most:?}");
    assert!(estimate_spokes(30.0, &[1.0e9; 8]).iter().all(|x| *x <= 1.0 + 1e-6));
    // Its rings: the base at 5, 10, 15, then the outer ring.
    let r = estimate_rings();
    assert!(r[0] < r[1] && r[1] < r[2] && r[2] < r[3] && r[3] == 1.0);
}
