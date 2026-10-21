//! A song's scripted backgrounds and foregrounds
//! (`docs/plans/scripted-backgrounds.md`) as textured quads: their actors
//! drawn on StepMania's screen (480 pixels tall, as wide as the scripts
//! were told) mapped onto the background frame, its height the frame's,
//! centred in it; their textures registered as a noteskin's are.
//!
//! Quads not turned are cut to the frame, as StepMania's screen ends at
//! its edges (their texture with them, the corners' colors kept); turned
//! ones and quads seen in perspective are drawn whole. Models and text are
//! not drawn here.

use ddi_actor::{Actor, Drawn, UNTEXTURED, View};

use crate::actors2d;
use crate::color::to_target;
use crate::textured::{Shape, TexQuad, UNTEXTURED as NO_TEXTURE};

/// The transform from StepMania's `screen` (its width and height) to a
/// `frame` (`x, y, width, height` in target pixels): the frame's height,
/// centred across it.
pub fn screen_transform(frame: [f32; 4], screen: [f32; 2]) -> [f32; 6] {
    let [x, y, w, h] = frame;
    let k = h / screen[1].max(1.0);
    [k, 0.0, 0.0, k, x + (w - screen[0] * k) / 2.0, y]
}

/// The quads of `draws` (each an actor tree, its opacity, and whether it
/// is a foreground), back to front, as `(backgrounds, foregrounds)`;
/// `textures` gives each texture id the actors carry its registered one
/// (`None`: not loaded yet, not drawn). Colors as the target expects them
/// for skins' quads (`srgb`, see [`to_target`]).
pub fn quads<'a>(
    draws: impl IntoIterator<Item = (&'a mut Actor, f32, bool)>,
    frame: [f32; 4],
    screen: [f32; 2],
    textures: &[Option<u32>],
    srgb: bool,
) -> (Vec<TexQuad>, Vec<TexQuad>) {
    let transform = screen_transform(frame, screen);
    let view = View {
        transform,
        size: screen,
    };
    let clip = [frame[0], frame[1], frame[0] + frame[2], frame[1] + frame[3]];
    let mut back = Vec::new();
    let mut front = Vec::new();
    let mut drawn = Vec::new();
    for (actor, alpha, foreground) in draws {
        if alpha <= 0.0 {
            continue;
        }
        drawn.clear();
        actor.draw_in(
            &view,
            transform,
            [1.0, 1.0, 1.0, alpha],
            [1.0, 1.0, 1.0, 0.0],
            &mut drawn,
        );
        let out = if foreground { &mut front } else { &mut back };
        for d in drawn.drain(..) {
            if let Drawn::Quad(q) = d {
                push(&q, clip, textures, srgb, out);
            }
        }
    }
    (back, front)
}

/// A quad's diffuse and glow passes (`Sprite::DrawTexture`).
fn push(
    q: &ddi_actor::Quad,
    clip: [f32; 4],
    textures: &[Option<u32>],
    srgb: bool,
    out: &mut Vec<TexQuad>,
) {
    let Some(mask) = actors2d::drawn_mask(q.depth, q.blend) else {
        return;
    };
    let texture = if q.texture == UNTEXTURED {
        NO_TEXTURE
    } else {
        match textures.get(q.texture as usize) {
            Some(Some(t)) => *t,
            _ => return,
        }
    };
    let uv = actors2d::offset_uv(q.uv, q.texcoord_offset);
    let shape = match q.projected {
        Some(p) => Shape::Projected {
            corners: p.corners,
            w: p.w,
            uv,
        },
        None => match cut(q.transform, uv, clip) {
            Some(shape) => shape,
            None => return,
        },
    };
    let colors = q.diffuse.map(|c| to_target(c, srgb));
    let base = TexQuad {
        blend: actors2d::blend(q.blend),
        gradient: colors.iter().any(|c| *c != colors[0]).then_some(colors),
        mask,
        ..TexQuad::new(shape, colors[0], texture)
    };
    if q.diffuse.iter().any(|c| c[3] > 0.0) {
        out.push(base);
    }
    if q.glow[3] > 0.0 {
        out.push(TexQuad {
            color: to_target(q.glow, srgb),
            glow: true,
            gradient: None,
            ..base
        });
    }
}

/// A quad (`transform` of the unit square, texture rectangle `uv`) cut to
/// `clip` (`[left, top, right, bottom]`) when it is not turned; `None`
/// when nothing of it is inside.
fn cut(m: [f32; 6], uv: [f32; 4], clip: [f32; 4]) -> Option<Shape> {
    let whole = Shape::Quad { transform: m, uv };
    if m[1] != 0.0 || m[2] != 0.0 {
        return Some(whole);
    }
    // One axis: centre `c`, signed size `s`, texture span `t0..t1`.
    let axis = |c: f32, s: f32, t0: f32, t1: f32, lo: f32, hi: f32| {
        let half = s.abs() / 2.0;
        let (a, b) = ((c - half).max(lo), (c + half).min(hi));
        if b <= a || half <= 0.0 {
            return None;
        }
        // The texture at screen position p (a negative size mirrors).
        let at = |p: f32| {
            let f = (p - (c - half)) / (2.0 * half);
            let f = if s < 0.0 { 1.0 - f } else { f };
            t0 + (t1 - t0) * f
        };
        Some((a, b, at(a), at(b)))
    };
    let (x0, x1, u0, u1) = axis(m[4], m[0], uv[0], uv[2], clip[0], clip[2])?;
    let (y0, y1, v0, v1) = axis(m[5], m[3], uv[1], uv[3], clip[1], clip[3])?;
    Some(Shape::Quad {
        transform: [x1 - x0, 0.0, 0.0, y1 - y0, (x0 + x1) / 2.0, (y0 + y1) / 2.0],
        uv: [u0, v0, u1, v1],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_actor::{Kind, Sprite};

    #[test]
    fn the_screen_fills_the_frame_height_centred() {
        // StepMania's 640 × 480 in a 1280 × 720 frame: 1.5 to the pixel,
        // centred (160 px each side).
        let m = screen_transform([0.0, 0.0, 1280.0, 720.0], [640.0, 480.0]);
        assert_eq!(m, [1.5, 0.0, 0.0, 1.5, 160.0, 0.0]);
    }

    #[test]
    fn quads_are_cut_to_the_frame() {
        // A quad twice the screen's width, centred: cut to the frame, its
        // texture with it.
        let mut quad = Actor::new(Kind::Sprite(Sprite::quad([1.0, 1.0])));
        let calls = ddi_actor::commands::parse("zoomto,1280,480;xy,320,240").unwrap();
        let mut p = ddi_actor::Problems::default();
        for c in &calls {
            quad.call(c, &mut p, 0).unwrap();
        }
        quad.update(0.0, &ddi_actor::Clocks::default(), &mut p);
        let (back, front) = quads(
            [(&mut quad, 0.5, false)],
            [0.0, 0.0, 640.0, 480.0],
            [640.0, 480.0],
            &[],
            false,
        );
        assert!(front.is_empty());
        let Shape::Quad { transform, uv } = back[0].shape else {
            panic!()
        };
        assert_eq!(transform, [640.0, 0.0, 0.0, 480.0, 320.0, 240.0]);
        assert_eq!(uv, [0.25, 0.0, 0.75, 1.0]);
        assert_eq!(back[0].color[3], 0.5);
    }
}
