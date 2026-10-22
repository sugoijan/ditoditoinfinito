//! A song's scripted backgrounds and foregrounds
//! (`docs/plans/scripted-backgrounds.md`) as textured quads: their actors
//! drawn on StepMania's screen (480 pixels tall, as wide as the scripts
//! were told) mapped onto the background frame, its height the frame's,
//! centered in it; their textures registered as a noteskin's are.
//!
//! Quads not turned are cut to the frame, as StepMania's screen ends at
//! its edges (their texture with them, the corners' colors kept); turned
//! ones and quads seen in perspective are drawn whole. Text is drawn by
//! the shell into a texture per run (white, tinted here); models are not
//! drawn here.

use ddi_actor::{Actor, Drawn, View};

use crate::actors2d;
use crate::textured::{self, Shape, TexQuad};

/// A run's drawing by its text: its texture and size in StepMania's
/// pixels, `None` while not drawn.
pub type Texts<'a> = dyn Fn(&str) -> Option<(u32, [f32; 2])> + 'a;

/// The transform from StepMania's `screen` (its width and height) to a
/// `frame` (`x, y, width, height` in target pixels): the frame's height,
/// centered across it.
pub fn screen_transform(frame: [f32; 4], screen: [f32; 2]) -> [f32; 6] {
    let [x, y, w, h] = frame;
    let k = h / screen[1].max(1.0);
    [k, 0.0, 0.0, k, x + (w - screen[0] * k) / 2.0, y]
}

/// The quads of `draws` (each an actor tree, its opacity, and whether it
/// is a foreground), back to front, as `(backgrounds, foregrounds)`;
/// `textures` gives each texture id the actors carry its registered one
/// (`None`: not loaded yet, not drawn), `texts` a run's drawing (its
/// texture and size in StepMania's pixels; `None`: not drawn yet). Colors
/// as the target expects them for skins' quads (`srgb`, see
/// [`crate::color::to_target`]).
pub fn quads<'a>(
    draws: impl IntoIterator<Item = (&'a mut Actor, f32, bool)>,
    frame: [f32; 4],
    screen: [f32; 2],
    textures: &[Option<u32>],
    texts: &Texts,
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
            match d {
                Drawn::Quad(q) => push(&q, clip, textures, srgb, out),
                Drawn::Text(t) => push_text(&t, texts, srgb, out),
                Drawn::Model(_) | Drawn::ClipPush(_) | Drawn::ClipPop => {}
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
    let Some((shape, texture)) = textured::actor_quad(q, textures, [0.0; 2]) else {
        return;
    };
    // Texture coordinates past the image (a scrolling texture,
    // `customtexturerect` past it) repeat it: StepMania wraps a sprite's
    // texture once its coordinates are set (`Sprite::SetCustomTextureRect`).
    let wrap = match &shape {
        Shape::Quad { uv, .. } | Shape::Projected { uv, .. } => {
            uv.iter().any(|c| !(0.0..=1.0).contains(c))
        }
        _ => false,
    };
    let shape = match shape {
        Shape::Quad { transform, uv } => match cut(transform, uv, clip) {
            Some(shape) => shape,
            None => return,
        },
        projected => projected,
    };
    let base = TexQuad {
        blend: actors2d::blend(q.blend),
        mask,
        wrap,
        ..TexQuad::new(shape, [1.0; 4], texture)
    };
    textured::push_passes(base, q.diffuse, q.glow, srgb, out);
}

/// A run's drawing as a quad over its line box (its top the drawing's),
/// in its colors: the diffuse pass and the glow's.
fn push_text(t: &ddi_actor::TextDraw, texts: &Texts, srgb: bool, out: &mut Vec<TexQuad>) {
    let Some(mask) = actors2d::drawn_mask(t.depth, t.blend) else {
        return;
    };
    let Some((texture, [w, h])) = texts(&t.text) else {
        return;
    };
    let top = -t.metrics.baseline;
    let local = [w, 0.0, 0.0, h, w / 2.0, top + h / 2.0];
    let shape = Shape::Quad {
        transform: ddi_actor::mul(&t.transform, &local),
        uv: [0.0, 0.0, 1.0, 1.0],
    };
    let base = TexQuad {
        blend: actors2d::blend(t.blend),
        mask,
        ..TexQuad::new(shape, [1.0; 4], texture)
    };
    textured::push_passes(base, t.diffuse, t.glow, srgb, out);
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
    fn the_screen_fills_the_frame_height_centered() {
        // StepMania's 640 × 480 in a 1280 × 720 frame: 1.5 to the pixel,
        // centered (160 px each side).
        let m = screen_transform([0.0, 0.0, 1280.0, 720.0], [640.0, 480.0]);
        assert_eq!(m, [1.5, 0.0, 0.0, 1.5, 160.0, 0.0]);
    }

    #[test]
    fn quads_are_cut_to_the_frame() {
        // A quad twice the screen's width, centered: cut to the frame, its
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
            &|_| None,
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

    #[test]
    fn scrolled_textures_repeat() {
        let mut quad = Actor::new(Kind::Sprite(Sprite::new(0, Vec::new(), [64.0, 64.0])));
        let calls = ddi_actor::commands::parse("xy,320,240;customtexturerect,0,0,1,1").unwrap();
        let mut p = ddi_actor::Problems::default();
        for c in &calls {
            quad.call(c, &mut p, 0).unwrap();
        }
        let screen = [640.0, 480.0];
        let frame = [0.0, 0.0, 640.0, 480.0];
        let (back, _) = quads(
            [(&mut quad, 1.0, false)],
            frame,
            screen,
            &[Some(3)],
            &|_| None,
            false,
        );
        assert!(!back[0].wrap);
        let calls = ddi_actor::commands::parse("texcoordvelocity,0.5,0").unwrap();
        for c in &calls {
            quad.call(c, &mut p, 0).unwrap();
        }
        quad.update(0.5, &ddi_actor::Clocks::default(), &mut p);
        let (back, _) = quads(
            [(&mut quad, 1.0, false)],
            frame,
            screen,
            &[Some(3)],
            &|_| None,
            false,
        );
        assert!(back[0].wrap);
    }

    #[test]
    fn text_is_its_drawing_over_its_line_box() {
        use ddi_actor::text::{FixedMeasure, TextStyle};
        let style = TextStyle::new("sans-serif", 20.0);
        let mut text = Actor::new(Kind::Text(ddi_actor::Text::new("hi", style)));
        let mut p = ddi_actor::Problems::default();
        for c in &ddi_actor::commands::parse("xy,100,100;diffuse,1,1,0,1").unwrap() {
            text.call(c, &mut p, 0).unwrap();
        }
        text.update(0.0, &ddi_actor::Clocks::default(), &mut p);
        text.measure(&mut FixedMeasure);
        let screen = [640.0, 480.0];
        let frame = [0.0, 0.0, 640.0, 480.0];
        // Not drawn yet: nothing.
        let (back, _) = quads(
            [(&mut text, 1.0, false)],
            frame,
            screen,
            &[],
            &|_| None,
            false,
        );
        assert!(back.is_empty());
        // Drawn (20 × 25 in StepMania's pixels, as measured): a quad that
        // size, centred where the run's box is (centred on the actor), in
        // its color.
        let texts = |t: &str| (t == "hi").then_some((7, [20.0, 25.0]));
        let (_, front) = quads([(&mut text, 1.0, true)], frame, screen, &[], &texts, false);
        assert_eq!(front.len(), 1);
        assert_eq!(front[0].texture, 7);
        assert_eq!(front[0].color, [1.0, 1.0, 0.0, 1.0]);
        let Shape::Quad { transform, .. } = front[0].shape else {
            panic!()
        };
        assert_eq!(transform, [20.0, 0.0, 0.0, 25.0, 100.0, 100.0]);
    }
}
