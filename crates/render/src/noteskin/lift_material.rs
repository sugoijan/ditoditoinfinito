//! A skin's `Def.Lift`: a time-sized leading trail under its head
//! (`docs/plans/noteskin-scene.md`).
use super::*;

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
        let clocks = self.note_clocks(lane, note.beat, frame);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preparation_is_time_sized_with_a_quarter_beat_minimum_and_bounded_extent() {
        let motion = |speed, seconds_per_beat| ddi_engine::frame::NoteMotion {
            arrows_per_second: speed,
            seconds_per_beat,
        };
        assert!((lead(0.28, motion(3.75, 0.5)) - 67.2).abs() < 1e-4);
        assert!((lead(0.28, motion(3.75, 2.0)) - 120.0).abs() < 1e-4);
        assert_eq!(
            lead(0.28, motion(-3.75, 0.5)),
            lead(0.28, motion(3.75, 0.5))
        );
        assert_eq!(lead(0.28, motion(2000.0, 0.5)), 256.0);
    }
}
