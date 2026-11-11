//! A battle's attacks as the play shows them beside the field
//! (`docs/plans/multiplayer-prototype-progress.md`, M8b;
//! `docs/plans/multiplayer-ui.md`, "Incoming lightning"): no attack button,
//! no target to choose, nothing said by sound alone.
//!
//! - The incoming warning ([`Cue`]): an attack against the local player,
//!   committed. A shock glyph drawn here (whatever the skin) beside the
//!   field's outer receptor edge, at the receptors' row (the bottom under
//!   Reverse), with one dot a shock under it; it scales in over 0.2 s
//!   (at once under reduced motion), dims to 60% once the first shock is
//!   in sight, and becomes a small shield once the last has passed, until
//!   the player has recovered (they cannot be attacked meanwhile).
//! - The waiting line: a muted line under the power bar's right end
//!   while the player's full bar waits ("Spread · waiting for target").

use ddi_actor::text::{TextMeasure, TextStyle};
use ddi_render::FieldGeometry;
use ddi_render::draw2d::{Affine, Blend, Command, DrawList, Mask, Quad, Shape, TextRun};
use ddi_session::battle::Incoming;

const FACE: &str = "Iosevka Custom, Iosevka Term, monospace";
/// The glyph: the power bar's electric violet, lighter.
const BOLT: [f32; 4] = [0.84, 0.76, 1.0, 1.0];
const GLOW: [f32; 4] = [0.55, 0.36, 1.0, 0.45];
const SHIELD: [f32; 4] = [0.62, 0.66, 0.86, 0.85];
const MUTED: [f32; 4] = [0.80, 0.80, 0.86, 0.72];
/// How long the warning takes to scale in, seconds.
const SCALE_IN: f64 = 0.2;
/// Its opacity once the first shock is in sight.
const DIMMED: f32 = 0.6;

/// The warning as drawn this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cue {
    /// One dot a shock.
    pub shocks: u8,
    /// The last shock passed: a shield until recovered.
    pub shield: bool,
    /// The first shock is in sight.
    pub dim: bool,
    /// `0..=1`, as it scales in.
    pub scale: f32,
}

/// The warning for `incoming` at judged song second `t`; `in_sight`, the
/// first of its shocks is drawn on the field now.
pub fn cue(incoming: Option<Incoming>, t: f64, in_sight: bool, reduced: bool) -> Option<Cue> {
    let i = incoming?;
    let scale = if reduced {
        1.0
    } else {
        ((t - i.committed) / SCALE_IN).clamp(0.0, 1.0) as f32
    };
    Some(Cue {
        shocks: i.shocks,
        shield: t > i.clear,
        dim: in_sight || t >= i.first,
        scale,
    })
}

/// Where the warning goes, `[left, top, right, bottom]` in target pixels:
/// beside the field's right edge (`span` lanes wide), on the receptors'
/// row. The miniatures keep clear of it.
pub fn place(geo: &FieldGeometry, span: f32) -> [f32; 4] {
    let a = geo.arrow;
    let left = geo.left + a * span + a * 0.15;
    let y = geo.receptor_y;
    [left, y - a * 0.5, left + a * 0.8, y + a * 0.55]
}

/// A bar from `p` to `q`, `thick` across.
fn segment(p: [f32; 2], q: [f32; 2], thick: f32, color: [f32; 4]) -> Quad {
    let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
    let len = (dx * dx + dy * dy).sqrt();
    let mut quad = Quad::plain([-len / 2.0, -thick / 2.0, len / 2.0, thick / 2.0], color);
    quad.transform = Affine::rotate(dy.atan2(dx))
        .then(Affine::translate((p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0));
    quad.shape = Shape {
        radius: [thick / 2.0; 2],
        ..Shape::NONE
    };
    quad
}

/// Draws `cue` beside the field `geo` (`span` lanes wide).
pub fn draw(list: &mut DrawList, cue: &Cue, geo: &FieldGeometry, span: f32) {
    if cue.scale <= 0.0 {
        return;
    }
    let [l, t, r, b] = place(geo, span);
    let (w, h) = (r - l, b - t);
    let alpha = if cue.dim && !cue.shield { DIMMED } else { 1.0 };
    let fade = |c: [f32; 4]| [c[0], c[1], c[2], c[3] * alpha];
    // Scaled about the glyph's middle.
    let (cx, cy) = (l + w / 2.0, t + h * 0.32);
    let k = cue.scale;
    let at = |x: f32, y: f32| [cx + (l + x * w - cx) * k, cy + (t + y * h - cy) * k];
    if cue.shield {
        // A small shield: a rounded top and a point under it.
        let s = 0.42 * k;
        let (sx, sy) = (cx, cy);
        let half = w * s / 2.0;
        let mut top = Quad::plain([sx - half, sy - half, sx + half, sy + half * 0.4], SHIELD);
        top.shape = Shape {
            radius: [half * 0.35; 2],
            ..Shape::NONE
        };
        list.push(Command::Quad(top));
        let d = half * 0.72;
        let mut point = Quad::plain([-d, -d, d, d], SHIELD);
        point.transform = Affine::rotate(std::f32::consts::FRAC_PI_4)
            .then(Affine::translate(sx, sy + half * 0.25));
        list.push(Command::Quad(point));
        return;
    }
    // A soft glow behind the bolt.
    let g = w * 0.42 * k;
    let mut glow = Quad::plain([cx - g, cy - g, cx + g, cy + g], fade(GLOW));
    glow.shape = Shape {
        radius: [g; 2],
        soft: [g * 0.6; 2],
        ..Shape::NONE
    };
    glow.blend = Blend::Add;
    list.push(Command::Quad(glow));
    // The bolt: down-left, across, down-left again.
    let thick = w * 0.13 * k;
    for (p, q) in [
        (at(0.66, 0.0), at(0.34, 0.33)),
        (at(0.30, 0.33), at(0.70, 0.33)),
        (at(0.66, 0.33), at(0.34, 0.66)),
    ] {
        list.push(Command::Quad(segment(p, q, thick, fade(BOLT))));
    }
    // One dot a shock, in rows of four under it.
    let n = usize::from(cue.shocks).min(16);
    let d = w * 0.13 * k;
    let gap = w * 0.22 * k;
    for i in 0..n {
        let (row, col) = (i / 4, i % 4);
        let x = cx + (col as f32 - 1.5) * gap;
        let y = t + h * 0.80 + row as f32 * gap * 0.9;
        let y = cy + (y - cy) * k;
        let mut dot = Quad::plain(
            [x - d / 2.0, y - d / 2.0, x + d / 2.0, y + d / 2.0],
            fade(BOLT),
        );
        dot.shape = Shape {
            radius: [d / 2.0; 2],
            ..Shape::NONE
        };
        list.push(Command::Quad(dot));
    }
}

/// Draws `text`, the waiting line, under the power bar's right end over
/// the field `geo` (`span` lanes wide), right-aligned to it, under the
/// life gauge; `ratio` a logical pixel.
pub fn draw_waiting(
    list: &mut DrawList,
    text: &str,
    geo: &FieldGeometry,
    span: f32,
    ratio: f32,
    measure: &mut dyn TextMeasure,
) {
    let (power, life) = ddi_render::scene::power_and_life(geo, span);
    let face = ddi_l10n::font_stack(FACE);
    let mut style = TextStyle::new(face.as_str(), 12.0 * ratio);
    style.weight = 500;
    let width = measure.measure(text, &style).advance;
    let right = power[0] + power[2];
    let x = (right - width).max(4.0 * ratio);
    let y = life[1] + life[3] + 3.0 * ratio;
    list.push(Command::Text(TextRun {
        curve: None,
        text: text.into(),
        style,
        transform: Affine::translate(x, y),
        color: MUTED,
        blend: Blend::Normal,
        mask: Mask::None,
        perspective: None,
        blur: [0.0; 2],
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn incoming() -> Incoming {
        Incoming {
            shocks: 8,
            committed: 10.0,
            first: 14.0,
            clear: 18.0,
        }
    }

    #[test]
    fn it_scales_in_dims_in_sight_and_becomes_a_shield() {
        assert_eq!(cue(None, 10.0, false, false), None);
        let c = cue(Some(incoming()), 10.1, false, false).unwrap();
        assert!((c.scale - 0.5).abs() < 1e-3 && !c.dim && !c.shield);
        assert_eq!(cue(Some(incoming()), 10.0, false, true).unwrap().scale, 1.0);
        assert!(cue(Some(incoming()), 12.0, true, false).unwrap().dim);
        assert!(cue(Some(incoming()), 14.5, false, false).unwrap().dim);
        assert!(cue(Some(incoming()), 18.5, false, false).unwrap().shield);
    }

    #[test]
    fn it_sits_beside_the_receptors_and_follows_reverse() {
        let geo = FieldGeometry::new(1280.0, 720.0, 4.0, false);
        let [l, t, _, b] = place(&geo, 4.0);
        assert!(l > geo.left + geo.arrow * 4.0);
        assert!(t < geo.receptor_y && b > geo.receptor_y);
        let rev = FieldGeometry::new(1280.0, 720.0, 4.0, true);
        let [_, t2, _, _] = place(&rev, 4.0);
        assert!(t2 > 720.0 / 2.0, "at the bottom under Reverse");
        let mut list = DrawList::new();
        let c = cue(Some(incoming()), 11.0, false, false).unwrap();
        draw(&mut list, &c, &geo, 4.0);
        // A glow, three strokes and eight dots.
        assert_eq!(list.commands.len(), 1 + 3 + 8);
        list.clear();
        draw(&mut list, &Cue { shield: true, ..c }, &geo, 4.0);
        assert_eq!(list.commands.len(), 2);
    }
}
