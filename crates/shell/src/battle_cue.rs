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
//!   in sight, and goes once the last has passed: there is no recovery
//!   since `attacks-v1`, so no shield. Where more attacks are placed
//!   behind it (a chain: under `attacks-v2` any number), smaller glyphs
//!   stand in a column at its right, one an attack up to [`BEHIND_GLYPHS`],
//!   past that one glyph and the count ("×5"), never dimmed (they are still
//!   to come); once the first has passed, the warning is the next one's.
//! - The waiting line: a muted line under the life gauge's right end
//!   while the player's full bar waits ("Waiting for target").
//! - The policy label ([`draw_policy`], M8 round 3, D8): the player's
//!   attack policy, one small muted word under the life gauge's left end,
//!   as steady as the waiting line (nothing flashes, nothing moves). A
//!   tap or a click on it or on the bars above it ([`policy_area`])
//!   cycles the policy, as Tab and a pad's View do
//!   (`ddi_session::play`).

use ddi_actor::text::{TextMeasure, TextStyle};
use ddi_render::FieldGeometry;
use ddi_render::draw2d::{Affine, Blend, Command, DrawList, Mask, Quad, Shape, TextRun};
use ddi_session::battle::Incoming;

const FACE: &str = "Iosevka Custom, Iosevka Term, monospace";
/// The glyph: the power bar's electric violet, lighter.
const BOLT: [f32; 4] = [0.84, 0.76, 1.0, 1.0];
const GLOW: [f32; 4] = [0.55, 0.36, 1.0, 0.45];
const MUTED: [f32; 4] = [0.80, 0.80, 0.86, 0.72];
/// How long the warning takes to scale in, seconds.
const SCALE_IN: f64 = 0.2;
/// Its opacity once the first shock is in sight.
const DIMMED: f32 = 0.6;
/// The most attacks behind the nearest drawn one glyph each; past it, one
/// glyph and the count.
pub const BEHIND_GLYPHS: u16 = 3;

/// The warning as drawn this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cue {
    /// One dot a shock.
    pub shocks: u8,
    /// Attacks placed behind this one (a chain).
    pub behind: u16,
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
        behind: i.behind,
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

/// A bar from `p` to `q`, `thick` across, its ends round.
pub(crate) fn segment(p: [f32; 2], q: [f32; 2], thick: f32, color: [f32; 4]) -> Quad {
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
    let alpha = if cue.dim { DIMMED } else { 1.0 };
    let fade = |c: [f32; 4]| [c[0], c[1], c[2], c[3] * alpha];
    // Scaled about the glyph's middle.
    let (cx, cy) = (l + w / 2.0, t + h * 0.32);
    let k = cue.scale;
    let at = |x: f32, y: f32| [cx + (l + x * w - cx) * k, cy + (t + y * h - cy) * k];
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
    // Those placed behind it: small bolts in a column at its right, one
    // an attack up to three, else one and the count; never dimmed (they
    // are still to come). Inside the warning's place, above the dots.
    let glyphs = if cue.behind > BEHIND_GLYPHS { 1 } else { cue.behind };
    let s = 0.34;
    let small_bolt = |list: &mut DrawList, oy: f32| {
        let ox = 0.70;
        let thick = w * 0.13 * k * s;
        for (p, q) in [
            ((0.66, 0.0), (0.34, 0.33)),
            ((0.30, 0.33), (0.70, 0.33)),
            ((0.66, 0.33), (0.34, 0.66)),
        ] {
            let small = |(x, y): (f32, f32)| at(ox + x * s, oy + y * s);
            list.push(Command::Quad(segment(small(p), small(q), thick, BOLT)));
        }
    };
    for i in 0..glyphs {
        small_bolt(list, -0.04 + f32::from(i) * 0.24);
    }
    if cue.behind > BEHIND_GLYPHS {
        let face = ddi_l10n::font_stack(FACE);
        let mut style = TextStyle::new(face.as_str(), h * 0.24 * k);
        style.weight = 700;
        let [x, y] = at(0.76, 0.22);
        list.push(Command::Text(TextRun {
            curve: None,
            text: format!("×{}", cue.behind).into(),
            style,
            transform: Affine::translate(x, y),
            color: BOLT,
            blend: Blend::Normal,
            mask: Mask::None,
            perspective: None,
            blur: [0.0; 2],
        }));
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

/// Where a tap or a click cycles the attack policy, `[left, top, right,
/// bottom]` in target pixels: the label's place under the life gauge's
/// left end and the left of the two bars above it, over the field `geo`
/// (`span` lanes wide); `ratio` a logical pixel. Inside the field's own
/// width: clear of the quit button, the warning and the miniatures,
/// which all stand beside the field.
pub fn policy_area(geo: &FieldGeometry, span: f32, ratio: f32) -> [f32; 4] {
    let (power, life) = ddi_render::scene::power_and_life(geo, span);
    let pad = 4.0 * ratio;
    [
        (power[0] - pad).max(0.0),
        (power[1] - pad).max(0.0),
        power[0] + power[2] * 0.45,
        life[1] + life[3] + 3.0 * ratio + POLICY_SIZE * ratio * 1.3 + pad,
    ]
}

/// The policy label's size, logical pixels (the waiting line's).
const POLICY_SIZE: f32 = 12.0;

/// Draws `text`, the player's attack policy, under the life gauge's left
/// end over the field `geo` (`span` lanes wide), left-aligned to it;
/// `ratio` a logical pixel.
pub fn draw_policy(list: &mut DrawList, text: &str, geo: &FieldGeometry, span: f32, ratio: f32) {
    let (_, life) = ddi_render::scene::power_and_life(geo, span);
    let face = ddi_l10n::font_stack(FACE);
    let mut style = TextStyle::new(face.as_str(), POLICY_SIZE * ratio);
    style.weight = 500;
    list.push(Command::Text(TextRun {
        curve: None,
        text: text.into(),
        style,
        transform: Affine::translate(life[0], life[1] + life[3] + 3.0 * ratio),
        color: MUTED,
        blend: Blend::Normal,
        mask: Mask::None,
        perspective: None,
        blur: [0.0; 2],
    }));
}

/// An attack policy's word, as the label and the rooms say it.
pub fn policy_word(t: ddi_session::battle::Targeting) -> String {
    use ddi_l10n::msg::room as m;
    use ddi_session::battle::Targeting;
    match t {
        Targeting::Spread => m::attack_policy_spread(),
        Targeting::Retaliate => m::attack_policy_retaliate(),
        Targeting::Leader => m::attack_policy_leader(),
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The policy label: under the life gauge's left end, inside the
    /// field's width at every size the miniatures' table uses, so the
    /// quit button, the warning and the miniatures never meet it; its
    /// tap area covers it and the left of the bars, and never reaches
    /// the waiting line's half.
    #[test]
    fn the_policy_label_is_under_the_gauge_inside_the_field() {
        for (w, h, ratio) in [
            (1280.0, 720.0, 1.0),
            (1920.0, 1080.0, 1.0),
            (844.0, 390.0, 1.0),
            (390.0, 844.0, 1.0),
            (2560.0, 1440.0, 2.0),
        ] {
            for reverse in [false, true] {
                let geo = FieldGeometry::new(w, h, 4.0, reverse);
                let (power, life) = ddi_render::scene::power_and_life(&geo, 4.0);
                let [l, t, r, b] = policy_area(&geo, 4.0, ratio);
                assert!(l >= 0.0 && t >= 0.0, "{w}x{h}");
                assert!(t <= power[1] && b > life[1] + life[3], "{w}x{h}");
                // Within the field's band, left of the bars' middle.
                assert!(l >= geo.left - 4.0 * ratio - 0.5, "{w}x{h}: {l} {}", geo.left);
                assert!(r < power[0] + power[2] / 2.0, "{w}x{h}");
                // Clear of the warning's place and the quit button's.
                let cue = place(&geo, 4.0);
                assert!(r < cue[0] || b < cue[1] || t > cue[3], "{w}x{h}");
                let layout = ddi_chart::Layout::dance_single();
                let [qx, qy, qr] = ddi_render::scene::quit_button(&geo, &layout, ratio, [0.0; 4]);
                let clear = qx + qr <= l || qx - qr >= r || qy + qr <= t || qy - qr >= b;
                assert!(clear, "{w}x{h} reverse {reverse}: quit {qx} {qy} {qr}, label {l} {t} {r} {b}");
                let mut list = DrawList::new();
                draw_policy(&mut list, "Spread", &geo, 4.0, ratio);
                let Command::Text(run) = &list.commands[0] else {
                    panic!("a text");
                };
                assert_eq!(run.text.as_ref(), "Spread");
                assert_eq!(run.color, MUTED);
            }
        }
        assert_eq!(policy_word(ddi_session::battle::Targeting::Leader), "Leader");
    }

    fn incoming() -> Incoming {
        Incoming {
            shocks: 8,
            committed: 10.0,
            first: 14.0,
            clear: 18.0,
            behind: 0,
        }
    }

    #[test]
    fn it_scales_in_dims_in_sight_and_marks_those_placed_behind() {
        assert_eq!(cue(None, 10.0, false, false), None);
        let c = cue(Some(incoming()), 10.1, false, false).unwrap();
        assert!((c.scale - 0.5).abs() < 1e-3 && !c.dim && c.behind == 0);
        assert_eq!(cue(Some(incoming()), 10.0, false, true).unwrap().scale, 1.0);
        assert!(cue(Some(incoming()), 12.0, true, false).unwrap().dim);
        assert!(cue(Some(incoming()), 14.5, false, false).unwrap().dim);
        // Two more placed behind it (a chain): said by the cue.
        let chained = Incoming {
            behind: 2,
            ..incoming()
        };
        assert_eq!(cue(Some(chained), 12.0, false, false).unwrap().behind, 2);
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
        // More placed behind it: a smaller bolt each, up to three, all
        // inside the warning's place (the miniatures keep clear of it).
        let [pl, pt, pr, pb] = place(&geo, 4.0);
        for n in 1..=3u16 {
            list.clear();
            draw(&mut list, &Cue { behind: n, scale: 1.0, ..c }, &geo, 4.0);
            assert_eq!(list.commands.len(), 1 + 3 + 3 * usize::from(n) + 8, "{n}");
            for cmd in &list.commands[4..4 + 3 * usize::from(n)] {
                let Command::Quad(q) = cmd else { panic!("a stroke") };
                let [x, y] = q.transform.apply([0.0, 0.0]);
                assert!(x > pl && x < pr && y > pt - 1.0 && y < pb, "{n}: {x} {y}");
            }
        }
        // Past three: one bolt and the count.
        list.clear();
        draw(&mut list, &Cue { behind: 5, scale: 1.0, ..c }, &geo, 4.0);
        assert_eq!(list.commands.len(), 1 + 3 + 3 + 1 + 8);
        assert!(list.commands.iter().any(
            |cmd| matches!(cmd, Command::Text(t) if t.text.as_ref() == "×5")
        ));
    }
}
