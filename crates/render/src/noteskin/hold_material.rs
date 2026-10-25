//! Alpha-mask long notes: one continuous fill over a band, protected head
//! holes, exterior and overlapping shadows and a roll's pulse, drawn with
//! the stencil from named layers ([`HoldLayers`]): a skin's sprite sheets
//! under the `HoldStencilMaterial`/`RollStencilMaterial` metrics (frames by
//! index), or a `Def.LongNote`'s components (parts by role,
//! `docs/plans/noteskin-scene.md`).
use super::*;
use crate::material::{MaterialParams, MaterialQuad};
use ddi_noteskin::component::{Anchor, LongNote};
use ddi_noteskin::metrics::HoldMaterial;

pub(super) fn head_phase(element: &mut Element, percent: f32) {
    if let Element::Actor(actor) = element {
        set_note_animation(actor, percent, true);
    }
}

/// One image of a long note's layers: the skin's texture, its frame's
/// rectangle, its size in units and its middle's place across the note.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Layer {
    texture: u32,
    rect: [f32; 4],
    size: [f32; 2],
    across: f32,
}

fn layer(element: &Element, state: usize) -> Option<Layer> {
    let Element::Actor(actor) = element else {
        return None;
    };
    let Kind::Sprite(sprite) = &actor.kind else {
        return None;
    };
    Some(Layer {
        texture: sprite.texture,
        rect: sprite.states.get(state)?.rect,
        size: sprite.size,
        across: 0.0,
    })
}

/// The body's and cap's layers, by role.
const BAND: usize = 0;
const ENVELOPE: usize = 1;
const SHADOW: usize = 2;
const OVERLAP: usize = 3;
const PULSE: usize = 4;
/// Their roles' names (`data-role`).
const ROLES: [&str; 5] = ["band", "envelope", "shadow", "overlap-shadow", "pulse"];

/// What paints a long note's band.
#[derive(Clone, Copy, Debug)]
pub(super) enum Fill {
    /// The metrics' wave (`HoldMaterial*`).
    Wave { material: HoldMaterial, slot: u16 },
    /// A skin's material: its quad (all but its place), and where its
    /// coordinates are 0 along the note.
    Skin {
        quad: MaterialQuad,
        anchor: Anchor,
        origin: [f32; 2],
    },
    /// Nothing: the band left as the layers draw it.
    Nothing,
}

/// A long note's layers, as [`NoteskinPlay::material_hold`] draws them.
pub(super) struct HoldLayers {
    /// By role ([`BAND`] … [`PULSE`]): the body's tile, repeated from the
    /// join to the cap, and the cap's.
    body: [Option<Layer>; 5],
    cap: [Option<Layer>; 5],
    /// The cap's extent from the tail.
    cap_span: (f32, f32),
    /// The head's knockout and envelope, and their extent from the head.
    protect: Option<Layer>,
    outside: Option<Layer>,
    head_span: (f32, f32),
    pitch: f32,
    /// Where the body starts below the head.
    join: f32,
    fill: Fill,
    /// A roll's pulse's opacity now.
    pulse: Option<f32>,
}

impl HoldLayers {
    /// The metrics' layers: the body's and bottom cap's frames 0 to 2 (3
    /// and 4 for a moving roll), the top cap's two (its Y the join); `None`
    /// when one is missing.
    pub(super) fn of_sprites(
        parts: &HoldParts,
        state: usize,
        material: HoldMaterial,
        slot: u16,
        pulse: Option<f32>,
    ) -> Option<HoldLayers> {
        let count = if pulse.is_some() { 5 } else { 3 };
        let mut body = [None; 5];
        let mut cap = [None; 5];
        for i in 0..count {
            body[i] = Some(layer(&parts.body[state], i)?);
            cap[i] = Some(layer(&parts.bottomcap[state], i)?);
        }
        let protect = layer(&parts.topcap[state], 0)?;
        let outside = layer(&parts.topcap[state], 1)?;
        let cap_h = cap[BAND]?.size[1];
        let head_h = protect.size[1];
        // The topcap actor places the start of the repeated body relative
        // to the head. This follows the chosen head shape, including aliases.
        let join = match &parts.topcap[state] {
            Element::Actor(a) if a.current().pos[1].is_finite() => {
                a.current().pos[1].clamp(-head_h / 2.0, head_h / 2.0)
            }
            _ => 0.0,
        };
        Some(HoldLayers {
            body,
            cap,
            cap_span: (-cap_h / 2.0, cap_h / 2.0),
            protect: Some(protect),
            outside: Some(outside),
            head_span: (-head_h / 2.0, head_h / 2.0),
            pitch: material.pitch,
            join,
            fill: Fill::Wave { material, slot },
            pulse,
        })
    }

    /// A `Def.LongNote`'s layers at `clocks`: each role's part (its frame
    /// and, for the pulse, its opacity, from its bindings), the fill its
    /// material (`fill`, already given its slot and images).
    pub(super) fn of_long_note(
        note: &LongNote,
        clocks: &ddi_noteskin::component::Clocks,
        fill: Fill,
    ) -> HoldLayers {
        // Each component posed once: the first part shown with each of
        // `roles`, and its opacity.
        fn posed<const N: usize>(
            c: Option<&ddi_noteskin::component::Component>,
            roles: [&str; N],
            clocks: &ddi_noteskin::component::Clocks,
        ) -> [Option<(Layer, f32)>; N] {
            let mut out = [None; N];
            let Some(c) = c else {
                return out;
            };
            let [vx, _, vw, _] = c.view_box;
            for p in c.pose(clocks) {
                let Some(i) = roles
                    .iter()
                    .position(|r| p.part.role.as_deref() == Some(*r))
                else {
                    continue;
                };
                out[i].get_or_insert((
                    Layer {
                        texture: p.texture,
                        rect: [0.0, 0.0, 1.0, 1.0],
                        size: [c.view_box[2], c.view_box[3]],
                        across: vx + vw / 2.0,
                    },
                    p.opacity,
                ));
            }
            out
        }
        let body = posed(Some(&note.body), ROLES, clocks);
        let cap = posed(note.cap.as_ref(), ROLES, clocks);
        let [protect, outside] = posed(note.head.as_ref(), ["protect", "outside"], clocks);
        let pulse = body[PULSE].or(cap[PULSE]).map(|l| l.1);
        let span = |c: Option<&ddi_noteskin::component::Component>| {
            c.map_or((0.0, 0.0), |c| {
                (c.view_box[1], c.view_box[1] + c.view_box[3])
            })
        };
        HoldLayers {
            body: body.map(|l| l.map(|l| l.0)),
            cap: cap.map(|l| l.map(|l| l.0)),
            cap_span: span(note.cap.as_ref()),
            protect: protect.map(|l| l.0),
            outside: outside.map(|l| l.0),
            head_span: span(note.head.as_ref()),
            pitch: note.pitch,
            join: note.join,
            fill,
            pulse,
        }
    }

    /// The band's extent across the note.
    fn band_across(&self) -> (f32, f32) {
        match self.body[BAND].or(self.cap[BAND]) {
            Some(l) => (l.across - l.size[0] / 2.0, l.across + l.size[0] / 2.0),
            None => (-48.0, 48.0),
        }
    }
}

/// The wave's parameters ([`crate::material::HOLD_WAVE`],
/// `materials/hold_wave.wgsl`).
pub(super) fn wave_params(m: HoldMaterial) -> MaterialParams {
    let (sin, cos) = m.angle.sin_cos();
    let color = |i: usize| [m.colors[i][0], m.colors[i][1], m.colors[i][2], 0.0];
    [
        [sin, cos, m.period, m.wave[0]],
        [m.wave[1], m.wave[2], 0.0, 0.0],
        color(0),
        color(1),
        color(2),
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
    ]
}

/// The wave's phase at song `time`: its own, moved by its speed (the
/// motion's fraction taken in double precision, so a long song keeps it).
pub(super) fn wave_phase(m: HoldMaterial, time: f64) -> f32 {
    (m.phase + (time * f64::from(m.speed)).rem_euclid(1.0) as f32).rem_euclid(1.0)
}

/// A quad of a long note painted by `base`'s material: the note's own
/// coordinates `lo..hi` along it (units from the head) and `left..right`
/// across, at `x` and `head`, `k` screen pixels a unit, in direction
/// `dir`; its material's coordinates the same, less `origin`; its
/// corners' colors `colors`.
#[allow(clippy::too_many_arguments)]
pub(super) fn fill_quad(
    base: MaterialQuad,
    [x, head, k, dir]: [f32; 4],
    (lo, hi): (f32, f32),
    (left, right): (f32, f32),
    origin: [f32; 2],
    mask: Mask,
    colors: [[f32; 4]; 4],
) -> TexQuad {
    let (mid_x, mid_y) = ((left + right) * 0.5, (lo + hi) * 0.5);
    TexQuad {
        mask,
        gradient: colors.iter().any(|c| *c != colors[0]).then_some(colors),
        material: Some(MaterialQuad {
            axes: [right - left, 0.0, 0.0, hi - lo],
            origin: [mid_x - origin[0], mid_y - origin[1]],
            ..base
        }),
        ..TexQuad::new(
            Shape::Quad {
                transform: [
                    (right - left) * k,
                    0.0,
                    0.0,
                    (hi - lo) * k * dir,
                    x + mid_x * k,
                    head + mid_y * k * dir,
                ],
                uv: [0.0, 0.0, 1.0, 1.0],
            },
            colors[0],
            UNTEXTURED,
        )
    }
}

/// The wave's material quad (all but its place): `slot`'s parameters at
/// `phase`, its colors scaled by `scale`.
fn wave_base(slot: u16, phase: f32, scale: f32) -> MaterialQuad {
    MaterialQuad {
        material: crate::material::HOLD_WAVE,
        slot,
        axes: [0.0; 4],
        origin: [0.0; 2],
        phase,
        seed: 0.0,
        value: [scale, 0.0, 0.0, 0.0],
        images: [UNTEXTURED; 2],
    }
}

/// A quad of a long note painted with the wave, 96 units across (a lift's
/// rows and band).
#[allow(clippy::too_many_arguments)]
pub(super) fn wave_quad(
    slot: u16,
    phase: f32,
    scale: f32,
    place: [f32; 4],
    span: (f32, f32),
    mask: Mask,
    colors: [[f32; 4]; 4],
) -> TexQuad {
    fill_quad(
        wave_base(slot, phase, scale),
        place,
        span,
        (-48.0, 48.0),
        [0.0; 2],
        mask,
        colors,
    )
}

impl NoteskinPlay {
    /// Draws a long note from `layers`: its envelope's exterior shadow, the
    /// fill over its band (the head's holes protected), a roll's
    /// overlapping shading and pulse, then the head's own parts
    /// (`head_draws`: by mask, its shadow, its band, the rest on top).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn material_hold(
        &self,
        layers: &HoldLayers,
        note: &NoteSprite,
        frame: &Frame,
        geo: &FieldGeometry,
        [x, head, tail, k, color_scale]: [f32; 5],
        head_draws: &[TexQuad],
        out: &mut Vec<TexQuad>,
    ) {
        let dir = if tail >= head { 1.0 } else { -1.0 };
        let length = (tail - head).abs() / k;
        let (head_top, cap_bottom) = (layers.head_span.0.min(-1.0), layers.cap_span.1);
        let margin = (-head_top).max(cap_bottom).max(48.0);
        let screen0 = (0.0 - head) / (k * dir);
        let screen1 = (geo.height - head) / (k * dir);
        let screen = (screen0.min(screen1) - margin, screen0.max(screen1) + margin);
        let spans: Vec<(f32, f32)> = frame
            .appearance
            .visible_spans(
                note.y,
                match note.kind {
                    SpriteKind::HoldHead { tail_y, .. } | SpriteKind::RollHead { tail_y, .. } => {
                        tail_y
                    }
                    _ => note.y,
                },
            )
            .map(|(a, b)| {
                // Visibility is in engine arrow heights; convert relative to
                // this note so split-sign fields keep their own direction.
                let a = (a - note.y) * 64.0;
                let b = (b - note.y) * 64.0;
                (
                    (if a.min(b) <= 0.001 {
                        head_top
                    } else {
                        a.min(b)
                    })
                    .max(screen.0),
                    (if a.max(b) >= length - 0.001 {
                        length + cap_bottom
                    } else {
                        a.max(b)
                    })
                    .min(screen.1),
                )
            })
            .filter(|(a, b)| b > a)
            .collect();
        if spans.is_empty() {
            return;
        }
        let first = out.len();
        let draw = |layer: Layer,
                    y0: f32,
                    y1: f32,
                    v0: f32,
                    v1: f32,
                    wrap: bool,
                    mask: Mask,
                    out: &mut Vec<TexQuad>| {
            let Some(texture) = self
                .texture_ids
                .get(layer.texture as usize)
                .copied()
                .flatten()
            else {
                return;
            };
            let r = layer.rect;
            for &(lo, hi) in &spans {
                let a = y0.max(lo);
                let b = y1.min(hi);
                if b <= a {
                    continue;
                }
                let va = v0 + (v1 - v0) * (a - y0) / (y1 - y0);
                let vb = v0 + (v1 - v0) * (b - y0) / (y1 - y0);
                out.push(TexQuad {
                    mask,
                    wrap,
                    blend: mask_blend(mask),
                    ..TexQuad::new(
                        Shape::Quad {
                            transform: [
                                layer.size[0] * k,
                                0.0,
                                0.0,
                                (b - a) * k * dir,
                                x + layer.across * k,
                                head + (a + b) * 0.5 * k * dir,
                            ],
                            uv: [
                                r[0],
                                r[1] + (r[3] - r[1]) * va,
                                r[2],
                                r[1] + (r[3] - r[1]) * vb,
                            ],
                        },
                        [1.0; 4],
                        texture,
                    )
                });
            }
        };
        let (cap_top, cap_h) = (layers.cap_span.0, layers.cap_span.1 - layers.cap_span.0);
        let cap_start = length + cap_top;
        let join = layers.join;
        // Every repeated layer begins at the head's join, with its
        // tail-anchored phase preserved.
        let body_draw = |role: usize, mask, out: &mut Vec<TexQuad>| {
            let Some(l) = layers.body[role] else { return };
            if cap_start > join {
                draw(
                    l,
                    join,
                    cap_start,
                    (join - cap_start) / layers.pitch,
                    0.0,
                    true,
                    mask,
                    out,
                );
            }
        };
        let cap_draw = |role: usize, mask, out: &mut Vec<TexQuad>| {
            let Some(l) = layers.cap[role] else { return };
            let start = cap_start.max(join);
            if start >= length + cap_bottom || cap_h <= 0.0 {
                return;
            }
            draw(
                l,
                start,
                length + cap_bottom,
                (start - cap_start) / cap_h,
                1.0,
                false,
                mask,
                out,
            )
        };
        let head_layer = |l: Option<Layer>, mask, out: &mut Vec<TexQuad>| {
            if let Some(l) = l {
                let (a, b) = layers.head_span;
                draw(l, a, b, 0.0, 1.0, false, mask, out);
            }
        };
        let mirror = |q: &TexQuad| mirror_about(*q, head, dir);
        // The envelope, and the shadows outside it.
        out.push(clear_quad());
        body_draw(ENVELOPE, Mask::Write, out);
        cap_draw(ENVELOPE, Mask::Write, out);
        head_layer(layers.outside, Mask::Write, out);
        body_draw(SHADOW, Mask::Outside, out);
        cap_draw(SHADOW, Mask::Outside, out);
        out.extend(
            head_draws
                .iter()
                .filter(|q| q.mask == Mask::Outside)
                .map(mirror),
        );
        // The band, the head's holes taken out, and the fill over it.
        out.push(clear_quad());
        body_draw(BAND, Mask::Write, out);
        cap_draw(BAND, Mask::Write, out);
        head_layer(layers.protect, Mask::Erase, out);
        out.extend(
            head_draws
                .iter()
                .filter(|q| q.mask == Mask::Write)
                .map(mirror),
        );
        let place = [x, head, k, dir];
        let across = layers.band_across();
        match layers.fill {
            Fill::Wave { material, slot } => {
                let base = wave_base(slot, wave_phase(material, frame.song_time), color_scale);
                let white = to_target([1.0; 4], self.srgb);
                for &span in &spans {
                    out.push(fill_quad(
                        base,
                        place,
                        span,
                        across,
                        [0.0; 2],
                        Mask::Inside,
                        [white; 4],
                    ));
                }
            }
            Fill::Skin {
                quad,
                anchor,
                origin,
            } => {
                let along = match anchor {
                    Anchor::Head => 0.0,
                    Anchor::Tail => length,
                };
                // A hold let go greys: its fill tinted so.
                let tint = to_target([color_scale, color_scale, color_scale, 1.0], self.srgb);
                for &span in &spans {
                    out.push(fill_quad(
                        quad,
                        place,
                        span,
                        across,
                        [origin[0], origin[1] + along],
                        Mask::Inside,
                        [tint; 4],
                    ));
                }
            }
            Fill::Nothing => {}
        }
        if layers.body[OVERLAP].is_some() || layers.cap[OVERLAP].is_some() {
            // Body shading shares the body's tail-anchored UVs, right up to
            // the head. The head knocks out this stencil so moving shadows
            // cannot darken its band or charge holes.
            head_layer(layers.protect, Mask::Erase, out);
            body_draw(OVERLAP, Mask::Inside, out);
            cap_draw(OVERLAP, Mask::Inside, out);
            // Only the standalone head casts a head-anchored shadow onto
            // the body. Never freeze a crop of the repeated body here.
            out.extend(
                head_draws
                    .iter()
                    .filter(|q| q.mask == Mask::Outside)
                    .map(|q| {
                        let mut q = mirror(q);
                        q.mask = Mask::Inside;
                        q
                    }),
            );
        }
        out.push(clear_quad());
        if let Some(opacity) = layers.pulse
            && (layers.body[PULSE].is_some() || layers.cap[PULSE].is_some())
        {
            head_layer(layers.protect, Mask::Write, out);
            let start = out.len();
            body_draw(PULSE, Mask::Outside, out);
            cap_draw(PULSE, Mask::Outside, out);
            for q in &mut out[start..] {
                q.color = to_target([color_scale, color_scale, color_scale, opacity], self.srgb);
            }
            out.push(clear_quad());
        }
        out.extend(
            head_draws
                .iter()
                .filter(|q| q.mask == Mask::None)
                .map(mirror),
        );
        bound_clears(&mut out[first..]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_wave_loops_with_its_speed() {
        let m = HoldMaterial {
            pitch: 12.5,
            period: 121.0,
            angle: 60.0_f32.to_radians(),
            phase: 0.7,
            speed: 1.0,
            wave: [31.0, 209.0, 206.0],
            colors: [[0.2; 3], [0.5; 3], [0.8; 3]],
        };
        for t in [0.37, 125.0, 3600.25] {
            let a = wave_phase(m, t);
            let b = wave_phase(m, t + 1.0);
            assert!((a - b).abs() < 1e-5 && (0.0..1.0).contains(&a), "{a} {b}");
        }
        let p = wave_params(m);
        assert_eq!(p[0][2], 121.0);
        assert_eq!(p[4][..3], [0.8; 3]);
    }
}
