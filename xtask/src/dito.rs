//! `gen-dito`: the art of Dito, the bundled skin that draws the built-in
//! arrows through a skin's Lua (`docs/plans/bundled-skins.md`, step 2), as
//! SVG drawn from the shader's own shapes (`crates/render/src/shader.wgsl`) and
//! the color schemes' own colors (`ddi_render::note_colors`). Writes the
//! `.svg` files of `assets/noteskins/dance/Dito*/`; the Lua and metrics
//! next to them are written by hand.
//!
//! A skin's lane is 64 pixels; the built-in arrows fill 0.92 of it, so the
//! shader's local unit (the quad's half size) is 29.44 pixels. The art
//! points down, as StepMania's `Down` art does; the skin turns it for the
//! other buttons. The onigiri (Dancing☆Onigiri's lane, `Onigiri` elements,
//! `docs/plans/danoni-skins.md`) is the shader's star, a point up.

use std::f64::consts::TAU;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use ddi_chart::Quantization;
use ddi_render::note_colors::{ColorScheme, Gradient, quantization_color};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in the workspace root")
        .to_path_buf()
}

/// A lane, in pixels.
const LANE: f64 = 64.0;
/// The shader's local unit for arrows and receptors (`0.92 * lane / 2`).
const UNIT: f64 = 0.46 * LANE;
/// Its stroke's half width (`sd_arrow`'s `w`).
const HALF_STROKE: f64 = 0.22;
/// How far in from the edge the darker rim reaches.
const RIM: f64 = 0.16;
/// The receptor's ring: from this far inside the arrow's skeleton…
const RING_INNER: f64 = 0.04;
/// Frames of the Vivid and Flat cycle (four beats).
pub(crate) const VIVID_FRAMES: usize = 32;
/// Frames of Rainbow's flow (one beat).
pub(crate) const RAINBOW_FRAMES: usize = 16;
/// Rows of the Quantized sheet: nine colors, padded to a power of two.
const QUANTIZED_ROWS: usize = 16;

/// The single color (`crates/web/src/settings.rs`, `NoteColors::Single`).
const SINGLE: [f32; 4] = [0.35, 0.85, 1.0, 1.0];
/// The mine (`crates/render/src/scene.rs`).
const MINE: [f32; 3] = [0.35, 0.3, 0.45];
const MINE_CORE: [f32; 3] = [1.0, 0.9, 0.8];
/// Hold and roll bodies, held and not (`crates/render/src/scene.rs`).
const BODY_ACTIVE: [f32; 4] = [0.55, 0.95, 0.65, 0.95];
const BODY_INACTIVE: [f32; 4] = [0.35, 0.65, 0.45, 0.85];

/// The point `(x, y)` of the shader's local space (the arrow pointing
/// left) for an arrow pointing down, in a frame's pixels.
fn down(x: f64, y: f64) -> (f64, f64) {
    (LANE / 2.0 - y * UNIT, LANE / 2.0 - x * UNIT)
}

/// `sd_arrow`'s skeleton: the head's two arms from the tip and the shaft.
fn arrow_path() -> String {
    let p = |x, y| {
        let (px, py) = down(x, y);
        format!("{} {}", n(px), n(py))
    };
    format!(
        "M{}L{}L{}M{}L{}",
        p(-0.05, 0.73),
        p(-0.78, 0.0),
        p(-0.05, -0.73),
        p(-0.62, 0.0),
        p(0.72, 0.0)
    )
}

/// A number in the files: two decimals, no trailing zeros.
fn n(v: f64) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn hex(c: [f32; 3]) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}

fn rgb(c: [f32; 4]) -> [f32; 3] {
    [c[0], c[1], c[2]]
}

/// The rim: the shader scales the color by `factor(0.25)`, 0.25^2.2 in
/// linear light on the sRGB targets the game draws to.
fn darker(c: [f32; 3]) -> [f32; 3] {
    let linear = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let encode = |v: f32| {
        if v <= 0.003_130_8 {
            v * 12.92
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        }
    };
    let f = 0.25f32.powf(2.2);
    c.map(|v| encode(linear(v) * f))
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// The shader's star (`sd_star`, iq's sdStar5 in quad space, y down).
fn sd_star(x: f64, y: f64) -> f64 {
    let (r, rf) = (0.98, 0.5);
    let k1 = (0.809016994375, -0.587785252292);
    let k2 = (-k1.0, k1.1);
    let (mut px, mut py) = (x.abs(), -y + 0.08);
    for k in [k1, k2] {
        let d = 2.0 * (k.0 * px + k.1 * py).max(0.0);
        px -= d * k.0;
        py -= d * k.1;
    }
    px = px.abs();
    py -= r;
    let ba = (rf * -k1.1, rf * k1.0 - 1.0);
    let h = ((px * ba.0 + py * ba.1) / (ba.0 * ba.0 + ba.1 * ba.1)).clamp(0.0, r);
    let (dx, dy) = (px - ba.0 * h, py - ba.1 * h);
    (dx * dx + dy * dy).sqrt() * (py * ba.0 - px * ba.1).signum() - 0.04
}

/// The shader's square (`sd_symbol`'s `SYMBOL_SQUARE`, a rounded box).
fn sd_square(x: f64, y: f64) -> f64 {
    let (b, r) = (0.74 - 0.16, 0.16);
    let (dx, dy) = (x.abs() - b, y.abs() - b);
    let outside = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt();
    outside + dx.max(dy).min(0.0) - r
}

/// The star's contour where its distance is `level` (inside it below 0),
/// in a frame's pixels: a ray from its centre per half degree, the star's
/// points and notches among them.
fn star_path(level: f64) -> String {
    contour(sd_star, (0.0, 0.08), level)
}

/// The square's contour where its distance is `level`; its corners lie on
/// the rays at 45°.
fn square_path(level: f64) -> String {
    contour(sd_square, (0.0, 0.0), level)
}

/// The contour of `sdf` where it is `level`, from `(cx, cy)` (inside), in
/// a frame's pixels: a ray per half degree.
fn contour(sdf: fn(f64, f64) -> f64, (cx, cy): (f64, f64), level: f64) -> String {
    let mut d = String::new();
    for i in 0..720 {
        let a = TAU * f64::from(i) / 720.0;
        let (dx, dy) = (a.sin(), -a.cos());
        let (mut lo, mut hi) = (0.0, 1.5);
        for _ in 0..48 {
            let mid = (lo + hi) / 2.0;
            if sdf(cx + mid * dx, cy + mid * dy) < level {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let (x, y) = (cx + lo * dx, cy + lo * dy);
        let _ = write!(
            d,
            "{}{} {}",
            if i == 0 { "M" } else { "L" },
            n(LANE / 2.0 + x * UNIT),
            n(LANE / 2.0 + y * UNIT)
        );
    }
    d.push('Z');
    d
}

/// The shape a sheet's frames stroke: the arrow's skeleton, or a symbol's
/// band's middle (the band is the arrow's stroke, inside the outline): the
/// onigiri's star, the centre's square.
#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Arrow,
    Star,
    Square,
}

impl Shape {
    fn def(self) -> String {
        match self {
            Shape::Arrow => format!(
                "<path id=\"a\" d=\"{}\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
                arrow_path()
            ),
            Shape::Star => format!(
                "<path id=\"a\" d=\"{}\" fill=\"none\" stroke-linejoin=\"miter\" stroke-miterlimit=\"8\"/>",
                star_path(-HALF_STROKE)
            ),
            // The band's middle has sharp corners; its outer edge is
            // rounded (by 0.16 in the shader, the stroke's half here).
            Shape::Square => format!(
                "<path id=\"a\" d=\"{}\" fill=\"none\" stroke-linejoin=\"round\"/>",
                square_path(-HALF_STROKE)
            ),
        }
    }

    /// The prefix of its files (`_down tap note`, `_onigiri tap note`).
    fn prefix(self) -> &'static str {
        match self {
            Shape::Arrow => "_down",
            Shape::Star => "_onigiri",
            Shape::Square => "_center",
        }
    }
}

/// An SVG of `cols` × `rows` frames of a lane each, frame by frame.
fn sheet(cols: usize, rows: usize, defs: &str, frames: &[String]) -> String {
    sheet_of(Shape::Arrow, cols, rows, defs, frames)
}

/// [`sheet`] with `shape` as the frames' `#a`.
fn sheet_of(shape: Shape, cols: usize, rows: usize, defs: &str, frames: &[String]) -> String {
    let (w, h) = (cols as f64 * LANE, rows as f64 * LANE);
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n",
        n(w),
        n(h),
        n(w),
        n(h)
    );
    s.push_str("<!-- Generated by xtask gen-dito; do not edit by hand. -->\n");
    let _ = writeln!(s, "<defs>{}{defs}</defs>", shape.def());
    for (i, f) in frames.iter().enumerate() {
        if f.is_empty() {
            continue;
        }
        let (x, y) = ((i % cols) as f64 * LANE, (i / cols) as f64 * LANE);
        if x == 0.0 && y == 0.0 {
            let _ = writeln!(s, "<g>{f}</g>");
        } else {
            let _ = writeln!(s, "<g transform=\"translate({} {})\">{f}</g>", n(x), n(y));
        }
    }
    s.push_str("</svg>\n");
    s
}

/// A tap arrow: the darker rim, then the core, in flat colors or along
/// gradients `#b{id}` (core) and `#r{id}` (rim).
fn arrow(core: &str, rim: &str) -> String {
    let wide = n(2.0 * HALF_STROKE * UNIT);
    let thin = n(2.0 * (HALF_STROKE - RIM) * UNIT);
    format!(
        "<use xlink:href=\"#a\" stroke=\"{rim}\" stroke-width=\"{wide}\"/><use xlink:href=\"#a\" stroke=\"{core}\" stroke-width=\"{thin}\"/>"
    )
}

fn flat_arrow(c: [f32; 3]) -> String {
    arrow(&hex(c), &hex(darker(c)))
}

/// Vivid's ramp from `tail` (the base color) to `tip`, along the arrow.
fn ramp_arrow(id: usize, tail: [f32; 3], tip: [f32; 3], defs: &mut String) -> String {
    let (_, tip_y) = down(-1.0, 0.0);
    let (_, tail_y) = down(1.0, 0.0);
    for (prefix, (a, b)) in [("b", (tip, tail)), ("r", (darker(tip), darker(tail)))] {
        let _ = write!(
            defs,
            "<linearGradient id=\"{prefix}{id}\" gradientUnits=\"userSpaceOnUse\" x1=\"0\" y1=\"{}\" x2=\"0\" y2=\"{}\"><stop offset=\"0\" stop-color=\"{}\"/><stop offset=\"1\" stop-color=\"{}\"/></linearGradient>",
            n(tip_y),
            n(tail_y),
            hex(a),
            hex(b)
        );
    }
    arrow(&format!("url(#b{id})"), &format!("url(#r{id})"))
}

/// The files of one skin folder: `(name, text)`.
type Files = Vec<(String, String)>;

/// How one frame of a color sheet looks.
#[derive(Clone, Copy)]
struct Look {
    /// A tap's color, at its tail when it has a ramp.
    base: [f32; 3],
    /// Vivid's and Flat's color at the tip.
    tip: Option<[f32; 3]>,
    /// A fake's: the scheme's color, flat (the built-in fake has no
    /// gradient), drawn at 60 %.
    plain: [f32; 3],
}

/// A tap or lift arrow for `look`.
fn arrow_of(look: Look, id: usize, defs: &mut String) -> String {
    match look.tip {
        Some(tip) => ramp_arrow(id, look.base, tip, defs),
        None => flat_arrow(look.base),
    }
}

/// A symbol's ramp, made radial as the shader makes it for symbols
/// (`MODE_RAMP`, from the quad's centre): `tip` at the centre, `base` at
/// the rim.
fn ramp_symbol(id: usize, base: [f32; 3], tip: [f32; 3], defs: &mut String) -> String {
    for (prefix, (a, b)) in [("b", (tip, base)), ("r", (darker(tip), darker(base)))] {
        let _ = write!(
            defs,
            "<radialGradient id=\"{prefix}{id}\" gradientUnits=\"userSpaceOnUse\" cx=\"{c}\" cy=\"{c}\" r=\"{}\"><stop offset=\"0\" stop-color=\"{}\"/><stop offset=\"1\" stop-color=\"{}\"/></radialGradient>",
            n(UNIT),
            hex(a),
            hex(b),
            c = n(LANE / 2.0)
        );
    }
    arrow(&format!("url(#b{id})"), &format!("url(#r{id})"))
}

/// A tap of `shape` for `look`.
fn note_of(shape: Shape, look: Look, id: usize, defs: &mut String) -> String {
    match (shape, look.tip) {
        (Shape::Star | Shape::Square, Some(tip)) => ramp_symbol(id, look.base, tip, defs),
        _ => arrow_of(look, id, defs),
    }
}

/// The three sheets of a color scheme, `cols` × `rows` frames (`None`:
/// an empty frame): taps, lifts (a tap with a white dot of 0.3 of the
/// lane at 80 %), fakes. Lifts and fakes have sheets of their own rather
/// than Lua around a tap, because the engine sets a note's color and its
/// frame on the note's own sprite, as StepMania does.
fn sheets(cols: usize, rows: usize, looks: &[Option<Look>]) -> Files {
    [Shape::Arrow, Shape::Star, Shape::Square]
        .into_iter()
        .flat_map(|shape| sheets_of(shape, cols, rows, looks))
        .collect()
}

/// [`sheets`] of one shape.
fn sheets_of(shape: Shape, cols: usize, rows: usize, looks: &[Option<Look>]) -> Files {
    let grid = if cols == 1 && rows == 1 {
        String::new()
    } else {
        format!(" {cols}x{rows}")
    };
    let dot = format!(
        "<circle cx=\"{c}\" cy=\"{c}\" r=\"{}\" fill=\"#fff\" fill-opacity=\"0.8\"/>",
        n(0.15 * LANE),
        c = n(LANE / 2.0)
    );
    let mut out = Vec::new();
    for part in ["note", "lift", "fake"] {
        let mut defs = String::new();
        let frames: Vec<String> = looks
            .iter()
            .enumerate()
            .map(|(k, look)| match (look, part) {
                (None, _) => String::new(),
                (Some(l), "note") => note_of(shape, *l, k, &mut defs),
                (Some(l), "lift") => note_of(shape, *l, k, &mut defs) + &dot,
                (Some(l), _) => format!("<g opacity=\"0.6\">{}</g>", flat_arrow(l.plain)),
            })
            .collect();
        out.push((
            format!("{} tap {part}{grid}.svg", shape.prefix()),
            sheet_of(shape, cols, rows, &defs, &frames),
        ));
    }
    out
}

/// Quantized (Dito): a color a row, 4ths to 192nds.
fn quantized() -> Files {
    use Quantization::*;
    let mut looks = vec![None; QUANTIZED_ROWS];
    for (row, q) in [N4, N8, N12, N16, N24, N32, N48, N64, N192]
        .into_iter()
        .enumerate()
    {
        let c = rgb(quantization_color(q));
        looks[row] = Some(Look {
            base: c,
            tip: None,
            plain: c,
        });
    }
    sheets(1, QUANTIZED_ROWS, &looks)
}

/// Vivid and Flat: the cycle over four beats, each frame with its ramp,
/// taken at the middle of the time it is shown.
fn vivid() -> Files {
    let looks: Vec<Option<Look>> = (0..VIVID_FRAMES)
        .map(|k| {
            let beat = 4.0 * (k as f64 + 0.5) / VIVID_FRAMES as f64;
            let base = rgb(ColorScheme::Flat.note_color(Quantization::N4, 0.0, beat));
            let tip = match ColorScheme::Flat.note_gradient(0.0, beat) {
                Some(Gradient::Ramp { tip }) => tip,
                _ => base,
            };
            Some(Look {
                base,
                tip: Some(tip),
                plain: base,
            })
        })
        .collect();
    sheets(8, VIVID_FRAMES / 8, &looks)
}

/// Rainbow: a band a row (StepMania's `ProgressAlternate` puts a beat's
/// first quarter's notes in row 0 … and the beat's own in row 3, as
/// `rainbow_band` does), its flow over one beat across the row's frames.
/// The built-in flow also runs down the screen; a frame is one color.
fn rainbow() -> Files {
    let mut looks = Vec::new();
    for band in 0..4 {
        // A beat fraction in the band.
        let frac = [0.25, 0.5, 0.75, 0.0][band];
        for f in 0..RAINBOW_FRAMES {
            let beat = (f as f64 + 0.5) / RAINBOW_FRAMES as f64;
            let plain = rgb(ColorScheme::Rainbow.note_color(Quantization::N4, frac, beat));
            let base = match ColorScheme::Rainbow.note_gradient(frac, beat) {
                Some(Gradient::Flow { to, phase }) => {
                    let wave = 0.5 + 0.5 * (TAU * f64::from(phase)).cos();
                    mix(plain, to, wave as f32)
                }
                _ => plain,
            };
            looks.push(Some(Look {
                base,
                tip: None,
                plain,
            }));
        }
    }
    sheets(RAINBOW_FRAMES, 4, &looks)
}

/// Single: one color.
fn single() -> Files {
    let c = rgb(SINGLE);
    let look = Look {
        base: c,
        tip: None,
        plain: c,
    };
    sheets(1, 1, &[Some(look)])
}

/// The receptor: the arrow's outline as a ring (`SHAPE_ARROW_OUTLINE`),
/// white (the Lua colors it).
fn receptor() -> String {
    let defs = format!(
        "<mask id=\"m\" maskUnits=\"userSpaceOnUse\" x=\"0\" y=\"0\" width=\"{l}\" height=\"{l}\"><use xlink:href=\"#a\" stroke=\"#fff\" stroke-width=\"{}\"/><use xlink:href=\"#a\" stroke=\"#000\" stroke-width=\"{}\"/></mask>",
        n(2.0 * HALF_STROKE * UNIT),
        n(2.0 * RING_INNER * UNIT),
        l = n(LANE)
    );
    let frame = format!(
        "<rect width=\"{l}\" height=\"{l}\" fill=\"#fff\" mask=\"url(#m)\"/>",
        l = n(LANE)
    );
    sheet(1, 1, &defs, &[frame])
}

/// A symbol's receptor: its ring (`MODE_OUTLINE`), white.
fn symbol_receptor(shape: Shape) -> String {
    let (path, join) = match shape {
        Shape::Square => (square_path(-0.09), "round"),
        _ => (star_path(-0.09), "miter\" stroke-miterlimit=\"8"),
    };
    let frame = format!(
        "<path d=\"{path}\" fill=\"none\" stroke=\"#fff\" stroke-width=\"{}\" stroke-linejoin=\"{join}\"/>",
        n(0.18 * UNIT)
    );
    sheet_of(shape, 1, 1, "", &[frame])
}

/// A mine (`SHAPE_MINE` at 0.8 of the lane): a disc with eight spikes and
/// a light core. The Lua turns it with the song, as the shader does.
fn mine() -> String {
    let unit = 0.4 * LANE;
    let c = LANE / 2.0;
    let mut d = String::new();
    let steps = 256;
    for i in 0..steps {
        let a = TAU * i as f64 / steps as f64;
        let r = (0.62 + 0.12 * (8.0 * a).cos().max(0.0)) * unit;
        let _ = write!(
            d,
            "{}{} {}",
            if i == 0 { "M" } else { "L" },
            n(c + r * a.cos()),
            n(c - r * a.sin())
        );
    }
    d.push('Z');
    let defs = format!(
        "<radialGradient id=\"g\" gradientUnits=\"userSpaceOnUse\" cx=\"{c}\" cy=\"{c}\" r=\"{}\"><stop offset=\"{}\" stop-color=\"{}\"/><stop offset=\"1\" stop-color=\"{}\"/></radialGradient>",
        n(0.34 * unit),
        n(0.30 / 0.34),
        hex(MINE_CORE),
        hex(MINE),
        c = n(c)
    );
    sheet(
        1,
        1,
        &defs,
        &[format!("<path d=\"{d}\" fill=\"url(#g)\"/>")],
    )
}

/// A soft white glow for the explosions (the Lua colors it).
fn glow() -> String {
    let c = n(LANE / 2.0);
    let defs = format!(
        "<radialGradient id=\"g\" gradientUnits=\"userSpaceOnUse\" cx=\"{c}\" cy=\"{c}\" r=\"{c}\"><stop offset=\"0\" stop-color=\"#fff\"/><stop offset=\"0.45\" stop-color=\"#fff\" stop-opacity=\"0.5\"/><stop offset=\"1\" stop-color=\"#fff\" stop-opacity=\"0\"/></radialGradient>"
    );
    sheet(
        1,
        1,
        &defs,
        &[format!(
            "<rect width=\"{l}\" height=\"{l}\" fill=\"url(#g)\"/>",
            l = n(LANE)
        )],
    )
}

/// A hold's or roll's body (half or 0.38 of the lane wide), tiling down.
fn body(width: f64, c: [f32; 4]) -> String {
    let w = width * LANE;
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{l}\" height=\"{l}\" viewBox=\"0 0 {l} {l}\">\n<!-- Generated by xtask gen-dito; do not edit by hand. -->\n<rect x=\"{}\" width=\"{}\" height=\"{l}\" fill=\"{}\" fill-opacity=\"{}\"/>\n</svg>\n",
        n((LANE - w) / 2.0),
        n(w),
        hex(rgb(c)),
        n(f64::from(c[3])),
        l = n(LANE)
    )
}

/// Its end, a quarter of a lane long (the body stops that far before the
/// tail, `StopDrawingHoldBodyOffsetFromTail`): the body, then a half disc
/// ending at the tail.
fn cap(width: f64, c: [f32; 4]) -> String {
    let (w, h) = (width * LANE, LANE / 4.0);
    let x = (LANE - w) / 2.0;
    let r = w / 2.0;
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{l}\" height=\"{}\" viewBox=\"0 0 {l} {}\">\n<!-- Generated by xtask gen-dito; do not edit by hand. -->\n<path d=\"M{} 0V{}A{} {} 0 0 0 {} {}V0Z\" fill=\"{}\" fill-opacity=\"{}\"/>\n</svg>\n",
        n(h),
        n(h),
        n(x),
        n(h - r),
        n(r),
        n(r),
        n(x + w),
        n(h - r),
        hex(rgb(c)),
        n(f64::from(c[3])),
        l = n(LANE)
    )
}

/// Every generated file, by skin folder.
pub(crate) fn files() -> Vec<(&'static str, Files)> {
    let mut base: Files = quantized();
    base.extend([
        ("_down receptor.svg".into(), receptor()),
        ("_onigiri receptor.svg".into(), symbol_receptor(Shape::Star)),
        (
            "_center receptor.svg".into(),
            symbol_receptor(Shape::Square),
        ),
        ("_down mine.svg".into(), mine()),
        ("_down glow.svg".into(), glow()),
    ]);
    for (part, width) in [("Hold", 0.5), ("Roll", 0.38)] {
        for (state, c) in [("Active", BODY_ACTIVE), ("Inactive", BODY_INACTIVE)] {
            base.push((format!("Down {part} Body {state}.svg"), body(width, c)));
            base.push((format!("Down {part} BottomCap {state}.svg"), cap(width, c)));
        }
    }
    vec![
        ("Dito", base),
        ("Dito Vivid", vivid()),
        ("Dito Rainbow", rainbow()),
        ("Dito Single", single()),
    ]
}

pub(crate) fn gen_dito(check: bool) -> Result<()> {
    let root = repo_root().join("assets/noteskins/dance");
    let mut stale = Vec::new();
    for (skin, files) in files() {
        let dir = root.join(skin);
        // Generated files no longer generated.
        let wanted: Vec<&str> = files.iter().map(|(f, _)| f.as_str()).collect();
        if dir.is_dir() {
            for e in fs::read_dir(&dir)? {
                let name = e?.file_name().to_string_lossy().into_owned();
                if name.ends_with(".svg") && !wanted.contains(&name.as_str()) {
                    stale.push(format!("{skin}/{name} (no longer generated)"));
                    if !check {
                        fs::remove_file(dir.join(&name))?;
                    }
                }
            }
        }
        for (name, text) in files {
            let path = dir.join(&name);
            if fs::read_to_string(&path).is_ok_and(|t| t == text) {
                continue;
            }
            stale.push(format!("{skin}/{name}"));
            if !check {
                fs::create_dir_all(&dir)?;
                fs::write(&path, text)?;
            }
        }
    }
    if check && !stale.is_empty() {
        bail!(
            "Dito's art is stale ({}); run `cargo run -p xtask -- gen-dito`",
            stale.join(", ")
        );
    }
    if !check && !stale.is_empty() {
        println!("wrote {} of Dito's files", stale.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_is_in_the_subset_and_a_whole_number_of_lanes() {
        for (skin, files) in files() {
            for (name, text) in files {
                let (w, h) = ddi_noteskin::svg::check(&text)
                    .unwrap_or_else(|e| panic!("{skin}/{name}: {e}"));
                assert_eq!(w % 64, 0, "{skin}/{name}");
                assert!(h == 16 || h % 64 == 0, "{skin}/{name}");
            }
        }
    }

    #[test]
    fn the_star_is_the_shaders() {
        // Its top point and its centre, as `sd_star` has them.
        assert!(sd_star(0.0, -0.94).abs() < 0.01);
        assert!(sd_star(0.0, 0.08) < -0.4);
        assert!(sd_star(0.0, -0.98) > 0.0);
        // The band's middle, inside the outline, up first.
        let d = star_path(-HALF_STROKE);
        assert!(d.starts_with("M32 "), "{}", &d[..20]);
    }

    #[test]
    fn the_square_is_the_shaders() {
        // Its edge's middle, its rounded corner, its centre.
        assert!(sd_square(0.74, 0.0).abs() < 1e-9);
        assert!((sd_square(0.74, 0.74) - (0.16 * 2f64.sqrt() - 0.16)).abs() < 1e-9);
        assert!((sd_square(0.0, 0.0) + 0.74).abs() < 1e-9);
        // The band's middle: a square 0.52 across from the centre, its
        // corners sharp; up first.
        let d = square_path(-HALF_STROKE);
        let top = 32.0 - 0.52 * UNIT;
        assert!(d.starts_with(&format!("M32 {}", n(top))), "{}", &d[..20]);
        let corner = format!("L{} {}", n(32.0 + 0.52 * UNIT), n(top));
        assert!(d.contains(&corner), "{corner}");
    }

    #[test]
    fn the_arrow_points_down_from_the_shaders_skeleton() {
        // The tip, low in the frame, centred.
        assert_eq!(down(-0.78, 0.0), (32.0, 32.0 + 0.78 * UNIT));
        assert!(arrow_path().starts_with("M10.51 33.47L32 54.96L53.49 33.47"));
    }
}
