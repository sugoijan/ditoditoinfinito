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
//! drawn here. An `ActorFrameTexture`'s children are drawn into its
//! texture ([`TargetQuads`], drawn by `Renderer::set_target_quads`
//! before the frame), which sprites then show.

use ddi_actor::{Actor, Drawn, TARGET_TEXTURES, View};

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

/// The registered textures quads draw with: by the texture ids the
/// actors carry (`None`: not loaded yet, not drawn), the render targets'
/// by their index ([`ddi_actor::Target::index`]) and the movies' by
/// theirs ([`ddi_actor::movie_texture`]); with `movies_drawn`, the
/// movies a quad drew marked in it (bit `index`), whether their texture
/// holds a frame yet or not: what the shell decodes.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ids<'a> {
    pub textures: &'a [Option<u32>],
    pub targets: &'a [Option<u32>],
    pub movies: &'a [Option<u32>],
    pub movies_drawn: Option<&'a std::cell::Cell<u32>>,
}

impl Ids<'_> {
    /// The registered texture of id `texture` an actor carries.
    fn of(&self, texture: u32) -> Option<u32> {
        if let Some(i) = ddi_actor::movie_index(texture) {
            if let Some(drawn) = self.movies_drawn {
                drawn.set(drawn.get() | 1u32.checked_shl(i).unwrap_or(0));
            }
            return *self.movies.get(i as usize)?;
        }
        if texture != ddi_actor::UNTEXTURED && texture >= TARGET_TEXTURES {
            return *self.targets.get((texture - TARGET_TEXTURES) as usize)?;
        }
        *self.textures.get(texture as usize)?
    }
}

/// What a render target is drawn with this frame, in its pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct TargetQuads {
    /// Its index among the song's targets.
    pub index: u32,
    pub size: [u32; 2],
    pub alpha: bool,
    pub preserve: bool,
    pub quads: Vec<TexQuad>,
}

/// The quads of `draws` (each an actor tree, its opacity, and whether it
/// is a foreground), back to front, as `(backgrounds, foregrounds)`;
/// `ids` gives each texture id the actors carry its registered one,
/// `texts` a run's drawing (its texture and size in StepMania's pixels;
/// `None`: not drawn yet). Colors as the target expects them for skins'
/// quads (`srgb`, see [`crate::color::to_target`]). What render targets
/// are drawn with goes to `targets`, a target inside another first.
#[allow(clippy::too_many_arguments)]
pub fn quads<'a>(
    draws: impl IntoIterator<Item = (&'a mut Actor, f32, bool)>,
    frame: [f32; 4],
    screen: [f32; 2],
    ids: Ids,
    texts: &Texts,
    srgb: bool,
    targets: &mut Vec<TargetQuads>,
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
        push_all(&mut drawn, clip, (ids, texts), srgb, out, targets, 0);
    }
    (back, front)
}

/// `drawn` as quads in `out` (cut to `clip`), and the render targets in
/// it as theirs in `targets`.
fn push_all(
    drawn: &mut Vec<Drawn>,
    clip: [f32; 4],
    (ids, texts): (Ids, &Texts),
    srgb: bool,
    out: &mut Vec<TexQuad>,
    targets: &mut Vec<TargetQuads>,
    depth: u32,
) {
    for d in drawn.drain(..) {
        match d {
            Drawn::Quad(q) => push(&q, clip, ids, srgb, out),
            Drawn::Text(t) => push_text(&t, texts, srgb, out),
            Drawn::Target(mut t) if depth < 8 => {
                let mut quads = Vec::new();
                let [w, h] = t.size.map(|v| v as f32);
                let clip = [0.0, 0.0, w, h];
                push_all(
                    &mut t.drawn,
                    clip,
                    (ids, texts),
                    srgb,
                    &mut quads,
                    targets,
                    depth + 1,
                );
                targets.push(TargetQuads {
                    index: t.index,
                    size: t.size,
                    alpha: t.alpha,
                    preserve: t.preserve,
                    quads,
                });
            }
            Drawn::Target(_) | Drawn::Model(_) | Drawn::ClipPush(_) | Drawn::ClipPop => {}
        }
    }
}

/// A quad's diffuse and glow passes (`Sprite::DrawTexture`).
fn push(q: &ddi_actor::Quad, clip: [f32; 4], ids: Ids, srgb: bool, out: &mut Vec<TexQuad>) {
    let Some(mask) = actors2d::drawn_mask(q.depth, q.blend) else {
        return;
    };
    let registered = [ids.of(q.texture)];
    let mut one = *q;
    if q.texture != ddi_actor::UNTEXTURED {
        one.texture = 0;
    }
    let Some((shape, texture)) = textured::actor_quad(&one, &registered, [0.0; 2]) else {
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
        Shape::Quad { transform, uv } => match textured::cut(transform, uv, clip) {
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
            Ids::default(),
            &|_| None,
            false,
            &mut Vec::new(),
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
        let ids = Ids {
            textures: &[Some(3)],
            ..Ids::default()
        };
        let (back, _) = quads(
            [(&mut quad, 1.0, false)],
            frame,
            screen,
            ids,
            &|_| None,
            false,
            &mut Vec::new(),
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
            ids,
            &|_| None,
            false,
            &mut Vec::new(),
        );
        assert!(back[0].wrap);
    }

    #[test]
    fn a_movies_sprite_draws_its_texture_and_is_marked_drawn() {
        let sprite = Sprite::new(ddi_actor::movie_texture(1), Vec::new(), [320.0, 240.0]);
        let mut actor = Actor::new(Kind::Sprite(sprite));
        let drawn = std::cell::Cell::new(0);
        let frame = [0.0, 0.0, 640.0, 480.0];
        // Its texture not made yet: nothing drawn, but marked.
        let ids = Ids {
            movies: &[None, None],
            movies_drawn: Some(&drawn),
            ..Ids::default()
        };
        let mut targets = Vec::new();
        let (back, _) = quads(
            [(&mut actor, 1.0, false)],
            frame,
            [640.0, 480.0],
            ids,
            &|_| None,
            false,
            &mut targets,
        );
        assert!(back.is_empty());
        assert_eq!(drawn.get(), 0b10);
        // Made: drawn with it.
        let ids = Ids {
            movies: &[None, Some(5)],
            ..Ids::default()
        };
        let (back, _) = quads(
            [(&mut actor, 1.0, false)],
            frame,
            [640.0, 480.0],
            ids,
            &|_| None,
            false,
            &mut targets,
        );
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].texture, 5);
    }

    #[test]
    fn render_targets_are_drawn_in_their_pixels_and_shown_by_sprites() {
        // A 64 × 32 target holding a quad over its right half, and a
        // sprite showing it on the screen.
        let mut quad = Actor::new(Kind::Sprite(Sprite::quad([1.0, 1.0])));
        let mut p = ddi_actor::Problems::default();
        for c in &ddi_actor::commands::parse("zoomto,32,32;xy,48,16").unwrap() {
            quad.call(c, &mut p, 0).unwrap();
        }
        let mut aft = Actor::new(Kind::Frame(vec![quad]));
        aft.target = Some(Box::new(ddi_actor::Target {
            index: Some(0),
            size: [64.0, 32.0],
            alpha: false,
            preserve: false,
        }));
        let shown = Sprite::new(ddi_actor::target_texture(0), Vec::new(), [64.0, 32.0]);
        let mut shown = Actor::new(Kind::Sprite(shown));
        for c in &ddi_actor::commands::parse("xy,320,240").unwrap() {
            shown.call(c, &mut p, 0).unwrap();
        }
        let mut tree = Actor::new(Kind::Frame(vec![aft, shown]));
        let ids = Ids {
            targets: &[Some(9)],
            ..Ids::default()
        };
        let mut targets = Vec::new();
        let (back, _) = quads(
            [(&mut tree, 1.0, false)],
            [0.0, 0.0, 640.0, 480.0],
            [640.0, 480.0],
            ids,
            &|_| None,
            false,
            &mut targets,
        );
        assert_eq!(targets.len(), 1);
        let t = &targets[0];
        assert_eq!((t.index, t.size, t.alpha), (0, [64, 32], false));
        let Shape::Quad { transform, .. } = t.quads[0].shape else {
            panic!()
        };
        assert_eq!(transform, [32.0, 0.0, 0.0, 32.0, 48.0, 16.0]);
        // The sprite draws the target's registered texture.
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].texture, 9);
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
            Ids::default(),
            &|_| None,
            false,
            &mut Vec::new(),
        );
        assert!(back.is_empty());
        // Drawn (20 × 25 in StepMania's pixels, as measured): a quad that
        // size, centred where the run's box is (centred on the actor), in
        // its color.
        let texts = |t: &str| (t == "hi").then_some((7, [20.0, 25.0]));
        let (_, front) = quads(
            [(&mut text, 1.0, true)],
            frame,
            screen,
            Ids::default(),
            &texts,
            false,
            &mut Vec::new(),
        );
        assert_eq!(front.len(), 1);
        assert_eq!(front[0].texture, 7);
        assert_eq!(front[0].color, [1.0, 1.0, 0.0, 1.0]);
        let Shape::Quad { transform, .. } = front[0].shape else {
            panic!()
        };
        assert_eq!(transform, [20.0, 0.0, 0.0, 25.0, 100.0, 100.0]);
    }
}
