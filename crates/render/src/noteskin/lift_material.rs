//! Time-sized leading lift masks. Ordinary lift actors remain unchanged unless
//! the skin declares LiftStencilMaterial and supplies the four named children.
use super::*;
use ddi_actor::Actor;

type ImageLayer = (u32, [f32; 4], [f32; 2]);
fn layer(actor: &Actor, name: &str, state: usize) -> Option<ImageLayer> {
    let child = actor.children().iter().find(|c| c.name == name)?;
    let Kind::Sprite(sprite) = &child.kind else {
        return None;
    };
    Some((sprite.texture, sprite.states.get(state)?.rect, sprite.size))
}

pub(super) fn lead_length(
    m: ddi_noteskin::metrics::LiftMaterial,
    motion: ddi_engine::frame::NoteMotion,
) -> f32 {
    lead(m.seconds, motion)
}

/// How far ahead of a lift its trail leads: the distance the note travels
/// in `seconds`, a quarter of a beat at least, in units (64 an arrow).
fn lead(seconds: f32, motion: ddi_engine::frame::NoteMotion) -> f32 {
    (motion.arrows_per_second.abs() * seconds.max(motion.seconds_per_beat * 0.25) * 64.0)
        .clamp(1.0, 256.0)
}

impl NoteskinPlay {
    /// A `Def.Lift` (`docs/plans/noteskin-scene.md`): its trail's shadow
    /// and fill over the lead, faded toward its far end (in 4-unit rows,
    /// each faded from its top to its bottom), then its head as a tap's
    /// component.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn component_lift(
        &mut self,
        lift: &ddi_noteskin::component::Lift,
        lane: usize,
        note: &NoteSprite,
        frame: &Frame,
        [x, head]: [f32; 2],
        k: f32,
        dir: f32,
        out: &mut Vec<TexQuad>,
    ) {
        let clocks = self.clocks(lane, Some(note.beat));
        let seed = note_seed(note.note_index);
        // The head's `protect` and `outside` parts, as it is drawn (upright
        // whichever way the notes scroll): kept out of the trail's fill and
        // its shadow, as a long note's head keeps them out of its band.
        let mut head_mask = |role: &str, mask: Mask| {
            let mut quads = Vec::new();
            self.draw_component(
                &lift.head,
                [x, head],
                k,
                (1.0, 1.0),
                seed,
                &clocks,
                &|p| (p.role.as_deref() == Some(role)).then_some(mask),
                &mut quads,
            );
            quads
        };
        let protect = head_mask("protect", Mask::Erase);
        let outside = head_mask("outside", Mask::Write);
        if let Some(trail) = &lift.trail {
            let length = lead(lift.seconds, frame.note_motion);
            let [vx, vy, vw, vh] = trail.view_box;
            let (far, near) = ((-length).max(vy), vy + vh);
            let fade = |y: f32| {
                ((y + length) / length).clamp(0.0, 1.0).powf(lift.fade)
                    * lift.opacity
                    * frame.appearance.visibility(note.y + y / 64.0).0
            };
            let rows = |from: f32, to: f32| {
                let mut out = Vec::new();
                let mut y = from;
                while y < to {
                    let end = (y + 4.0).min(to);
                    out.push((y, end));
                    y = end;
                }
                out
            };
            let posed = |role: &str| {
                trail
                    .pose(&clocks)
                    .find(|p| p.part.role.as_deref() == Some(role))
                    .and_then(|p| self.texture_id(p.texture))
            };
            let srgb = self.srgb;
            let quad = |texture: u32, a: f32, b: f32, mask: Mask, alphas: [f32; 2]| {
                let color = |a| to_target([1.0, 1.0, 1.0, a], srgb);
                let (ca, cb) = (color(alphas[0]), color(alphas[1]));
                TexQuad {
                    mask,
                    gradient: (ca != cb).then_some([ca, ca, cb, cb]),
                    blend: mask_blend(mask),
                    ..TexQuad::new(
                        Shape::Quad {
                            transform: [
                                vw * k,
                                0.0,
                                0.0,
                                (b - a) * k * dir,
                                x + (vx + vw / 2.0) * k,
                                head + (a + b) * 0.5 * k * dir,
                            ],
                            uv: [0.0, (a - vy) / vh, 1.0, (b - vy) / vh],
                        },
                        ca,
                        texture,
                    )
                }
            };
            let first = out.len();
            if far < near {
                if let Some(shadow) = posed("shadow") {
                    let mask = if outside.is_empty() {
                        Mask::None
                    } else {
                        out.push(clear_quad());
                        out.extend(&outside);
                        Mask::Outside
                    };
                    for (a, b) in rows(far, near) {
                        out.push(quad(shadow, a, b, mask, [fade(a), fade(b)]));
                    }
                    if mask == Mask::Outside {
                        out.push(clear_quad());
                    }
                }
                let material = lift
                    .fill
                    .as_ref()
                    .and_then(|f| Some((f, f.material.as_deref()?)));
                if let (Some(mask), Some((fill, m))) = (posed("trail"), material)
                    && let (Some(index), Some(slot)) = (m.compiled, self.material_slot(m.params))
                {
                    let image_id = |n: usize| {
                        m.images
                            .get(n)
                            .and_then(|&t| self.texture_id(t))
                            .unwrap_or(UNTEXTURED)
                    };
                    let base = crate::material::MaterialQuad {
                        material: crate::material::LOCAL | index,
                        slot,
                        axes: [0.0; 4],
                        origin: [0.0; 2],
                        phase: fill.value(ddi_noteskin::component::Property::Phase, &clocks),
                        seed,
                        value: [0.0; 4],
                        images: [image_id(0), image_id(1)],
                    };
                    out.push(clear_quad());
                    out.push(quad(mask, far, near, Mask::Write, [1.0; 2]));
                    out.extend(&protect);
                    for (a, b) in rows(far, near) {
                        let (ca, cb) = (
                            to_target([1.0, 1.0, 1.0, fade(a)], self.srgb),
                            to_target([1.0, 1.0, 1.0, fade(b)], self.srgb),
                        );
                        out.push(hold_material::fill_quad(
                            base,
                            [x, head, k, dir],
                            (a, b),
                            (vx, vx + vw),
                            m.origin,
                            Mask::Inside,
                            [ca, ca, cb, cb],
                        ));
                    }
                    out.push(clear_quad());
                }
            }
            bound_clears(&mut out[first..]);
        }
        self.draw_component(
            &lift.head,
            [x, head],
            k,
            (note.alpha, 1.0),
            seed,
            &clocks,
            &|p| (!matches!(p.role.as_deref(), Some("protect" | "outside"))).then_some(Mask::None),
            out,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn material_lift(
        &self,
        element: &mut Element,
        metrics: &NoteDisplay,
        note: &NoteSprite,
        frame: &Frame,
        geo: &FieldGeometry,
        [x, head]: [f32; 2],
        k: f32,
        dir: f32,
        slot: Option<u16>,
        out: &mut Vec<TexQuad>,
    ) -> bool {
        let (Some(m), Some(slot)) = (metrics.lift_material, slot) else {
            return false;
        };
        let Element::Actor(actor) = element else {
            return false;
        };
        let phase = animation_percent(
            metrics.part(Part::TapLift),
            metrics.animation_is_beat_based,
            frame.beat as f32,
            frame.song_time as f32,
            note.beat,
        );
        let state = (phase.rem_euclid(1.0) * 16.0).floor() as usize;
        let (Some(trail), Some(shadow), Some(band), Some(charge)) = (
            layer(actor, "LiftTrail", 0),
            layer(actor, "LiftTrail", 1),
            layer(actor, "LiftBand", 0),
            layer(actor, "LiftCharge", state),
        ) else {
            return false;
        };
        let Some(origin) = actor
            .children()
            .iter()
            .find(|c| c.name == "LiftTrail")
            .map(|c| c.current().pos[1])
        else {
            return false;
        };
        if !actor.children().iter().any(|c| c.name == "LiftColors") {
            return false;
        }
        let visibility: Vec<_> = actor.children().iter().map(|c| c.visible).collect();
        for child in actor.children_mut() {
            child.visible &= child.name == "LiftColors";
        }
        let mut colors = Vec::new();
        self.note_actor(
            element,
            metrics,
            Part::TapLift,
            note.beat,
            (note.alpha, note.glow),
            frame,
            [x, head],
            k,
            1.0,
            &mut colors,
        );
        let Element::Actor(actor) = element else {
            unreachable!()
        };
        for (child, visible) in actor.children_mut().iter_mut().zip(visibility) {
            child.visible = visible;
        }
        let length = lead_length(m, frame.note_motion);
        let first = out.len();
        let fade = |y: f32| ((y + length) / length).clamp(0.0, 1.0).powf(m.fade) * m.opacity;
        let alpha = |y: f32| frame.appearance.visibility(note.y + y / 64.0).0;
        let image = |asset: ImageLayer,
                     y0: f32,
                     y1: f32,
                     source_y: f32,
                     mask: Mask,
                     opacity: [f32; 2],
                     out: &mut Vec<TexQuad>| {
            let Some(texture) = self.texture_id(asset.0) else {
                return;
            };
            let screen0 = (0.0 - head) / (k * dir);
            let screen1 = (geo.height - head) / (k * dir);
            let a = y0.max(screen0.min(screen1));
            let b = y1.min(screen0.max(screen1));
            if b <= a {
                return;
            }
            let uv_y =
                |y: f32| asset.1[1] + (asset.1[3] - asset.1[1]) * (y + source_y) / asset.2[1];
            let v = |y: f32| opacity[0] + (opacity[1] - opacity[0]) * (y - y0) / (y1 - y0);
            let color = |a| to_target([1.0, 1.0, 1.0, a], self.srgb);
            out.push(TexQuad {
                mask,
                gradient: Some([color(v(a)), color(v(a)), color(v(b)), color(v(b))]),
                blend: mask_blend(mask),
                ..TexQuad::new(
                    Shape::Quad {
                        transform: [
                            asset.2[0] * k,
                            0.0,
                            0.0,
                            (b - a) * k * dir,
                            x,
                            head + (a + b) * 0.5 * k * dir,
                        ],
                        uv: [asset.1[0], uv_y(a), asset.1[2], uv_y(b)],
                    },
                    [1.0; 4],
                    texture,
                )
            });
        };
        out.push(clear_quad());
        // The exported shadow surrounds the combined trail and head, so there
        // is no second stationary head shadow darkening their overlap.
        let mut y = -length;
        while y < 48.0 {
            let end = (y + 4.0).min(48.0);
            image(
                shadow,
                y,
                end,
                origin,
                Mask::None,
                [fade(y) * alpha(y), fade(end) * alpha(end)],
                out,
            );
            y = end;
        }
        out.push(clear_quad());
        image(trail, -length, 48.0, origin, Mask::Write, [1.0; 2], out);
        // The wave over the trail in four-pixel rows, each faded from its
        // top to its bottom; over the head's band, one quad.
        let phase = hold_material::wave_phase(m.material, frame.song_time);
        let white = |a| to_target([1.0, 1.0, 1.0, a], self.srgb);
        let wave = |span, colors| {
            hold_material::wave_quad(
                slot,
                phase,
                1.0,
                [x, head, k, dir],
                span,
                Mask::Inside,
                colors,
            )
        };
        let paint = |lo: f32, hi: f32, head_band: bool, out: &mut Vec<TexQuad>| {
            if head_band {
                out.push(wave((lo, hi), [white(note.alpha); 4]));
                return;
            }
            let mut y = lo;
            while y < hi {
                let end = (y + 4.0).min(hi);
                let (a, b) = (white(fade(y) * alpha(y)), white(fade(end) * alpha(end)));
                out.push(wave((y, end), [a, a, b, b]));
                y = end;
            }
        };
        paint(-length, 48.0, false, out);
        out.push(clear_quad());
        image(band, -48.0, 48.0, 48.0, Mask::Write, [1.0; 2], out);
        paint(-48.0, 48.0, true, out);
        out.push(TexQuad {
            mask: Mask::Inside,
            ..TexQuad::new(
                Shape::Quad {
                    transform: [96.0 * k, 0.0, 0.0, 96.0 * k, x, head],
                    uv: [0.0, 0.0, 1.0, 1.0],
                },
                to_target([1.0, 1.0, 1.0, m.white * note.alpha], self.srgb),
                UNTEXTURED,
            )
        });
        out.push(clear_quad());
        image(charge, -48.0, 48.0, 48.0, Mask::Write, [1.0; 2], out);
        for mut q in colors {
            q.mask = Mask::Inside;
            out.push(mirror_about(q, head, dir));
        }
        out.push(clear_quad());
        bound_clears(&mut out[first..]);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preparation_is_time_sized_with_a_quarter_beat_minimum_and_bounded_extent() {
        let skin = ddi_noteskin::skin::Skin::load(&[], "dance", "default").unwrap();
        let material = NoteDisplay::read(&skin, "Down").hold_material.unwrap_or(
            ddi_noteskin::metrics::HoldMaterial {
                pitch: 12.5,
                period: 121.0,
                angle: 0.0,
                phase: 0.7,
                speed: 1.0,
                wave: [0.0, 209.0, 206.0],
                colors: [[0.5; 3]; 3],
            },
        );
        let m = ddi_noteskin::metrics::LiftMaterial {
            material,
            seconds: 0.28,
            fade: 2.0,
            opacity: 1.0,
            white: 0.7,
        };
        let motion = |speed, seconds_per_beat| ddi_engine::frame::NoteMotion {
            arrows_per_second: speed,
            seconds_per_beat,
        };
        assert!((lead_length(m, motion(3.75, 0.5)) - 67.2).abs() < 1e-4);
        assert!((lead_length(m, motion(3.75, 2.0)) - 120.0).abs() < 1e-4);
        assert_eq!(
            lead_length(m, motion(-3.75, 0.5)),
            lead_length(m, motion(3.75, 0.5))
        );
        assert_eq!(lead_length(m, motion(2000.0, 0.5)), 256.0);
    }
}
