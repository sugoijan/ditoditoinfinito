//! Actors on the 2D pass: what actors draw ([`ddi_actor::Drawn`]) as a
//! [`DrawList`] (`docs/plans/ui-core.md`, "Draw list"). Units are the
//! list's: the actors are drawn into them (a screen draws its actors at
//! the transform of its virtual screen over the painter's scale).
//!
//! A quad's diffuse pass, then its glow pass (`Sprite::DrawTexture`);
//! corners as the list orders them; a quad seen in perspective by its
//! corners; StepMania's masks and clips that are not rectangles through
//! the stencil. Models are not drawn here.

use ddi_actor::{Blend as ActorBlend, Depth, Drawn, UNTEXTURED, ZTest};

use crate::draw2d::{Affine, Blend, Command, DrawList, Mask, Projected, Quad, Shape, TextRun};
use crate::texture_cache::TextureId;

/// The unit square an actor's quad transform places.
const UNIT: [f32; 4] = [-0.5, -0.5, 0.5, 0.5];

/// StepMania's blend modes as the list has them; one StepMania does not
/// have ([`ActorBlend::Other`]) draws as normal.
pub fn blend(b: ActorBlend) -> Blend {
    match b {
        ActorBlend::Normal | ActorBlend::Other => Blend::Normal,
        ActorBlend::Add => Blend::Add,
        ActorBlend::Subtract => Blend::Subtract,
        ActorBlend::Modulate => Blend::Modulate,
        ActorBlend::NoEffect => Blend::NoEffect,
        ActorBlend::CopySrc => Blend::CopySrc,
        ActorBlend::AlphaMask => Blend::AlphaMask,
        ActorBlend::AlphaKnockOut => Blend::AlphaKnockOut,
        ActorBlend::AlphaMultiply => Blend::AlphaMultiply,
        ActorBlend::WeightedMultiply => Blend::WeightedMultiply,
        ActorBlend::InvertDest => Blend::InvertDest,
    }
}

/// What an actor's depth state does with the mask
/// ([`crate::stencil`]). Only a `BlendMode_NoEffect` actor's depth is
/// nudged nearer (`SetZBias(1)`, `src/Actor.cpp:772-775`): it writes the
/// mask (tested or not, `MaskSource` and `MaskDest` on one actor). Any
/// other writes the depth everything else lies at: untested
/// (`ZTEST_OFF` is `GL_ALWAYS`, `src/RageDisplay_OGL.cpp:1971-1982`) it
/// takes the mask away where drawn; tested, it changes nothing.
pub fn mask(depth: Depth, blend: ActorBlend) -> Mask {
    match (depth.write, blend == ActorBlend::NoEffect, depth.test) {
        (true, true, _) => Mask::Write,
        (true, false, ZTest::Off) => Mask::Erase,
        (_, _, ZTest::WriteOnPass) => Mask::Outside,
        (_, _, ZTest::WriteOnFail) => Mask::Inside,
        (false, _, ZTest::Off) => Mask::None,
    }
}

/// [`mask`], or `None` when the actor draws nothing: a mask's source
/// (`BlendMode_NoEffect`) that writes no mask.
pub fn drawn_mask(depth: Depth, blend: ActorBlend) -> Option<Mask> {
    let mask = mask(depth, blend);
    (blend != ActorBlend::NoEffect || mask == Mask::Write).then_some(mask)
}

/// A quad's texture rectangle (`[left, top, right, bottom]`) moved by
/// `offset`.
pub(crate) fn offset_uv(uv: [f32; 4], offset: [f32; 2]) -> [f32; 4] {
    [
        uv[0] + offset[0],
        uv[1] + offset[1],
        uv[2] + offset[0],
        uv[3] + offset[1],
    ]
}

/// Upper left, upper right, lower left, lower right (an actor's) as top
/// left, top right, bottom right, bottom left (the list's).
fn corners<T: Copy>(c: [T; 4]) -> [T; 4] {
    [c[0], c[1], c[3], c[2]]
}

/// A motion blur on the target in the space `transform` places.
fn local_blur(blur: [f32; 2], transform: &Affine) -> [f32; 2] {
    if blur == [0.0; 2] {
        return blur;
    }
    transform.unapply_vector(blur)
}

/// Appends `drawn` to `list`; `texture` gives the list's texture of an
/// actor's (`None`: not drawn).
pub fn append(
    drawn: &[Drawn],
    list: &mut DrawList,
    texture: &mut dyn FnMut(u32) -> Option<TextureId>,
) {
    for d in drawn {
        match d {
            Drawn::Quad(q) => {
                let mask = drawn_mask(q.depth, q.blend);
                if q.depth.clear {
                    // `clearzbuffer`: the mask cleared everywhere.
                    list.quad(Quad {
                        blend: Blend::NoEffect,
                        mask: Mask::Clear,
                        // Opaque, so the alpha test keeps it; it draws no
                        // color.
                        ..Quad::plain([-1e6, -1e6, 1e6, 1e6], [1.0; 4])
                    });
                }
                let Some(mask) = mask else {
                    continue;
                };
                let tex = if q.texture == UNTEXTURED {
                    None
                } else {
                    match texture(q.texture) {
                        Some(t) => Some(t),
                        None => continue,
                    }
                };
                let quad = Quad {
                    rect: UNIT,
                    transform: Affine(q.transform),
                    texture: tex,
                    cover: q.cover,
                    uv: offset_uv(q.uv, q.texcoord_offset),
                    colors: corners(q.diffuse),
                    blend: blend(q.blend),
                    glow: false,
                    mask,
                    projected: q.projected.map(|p| Projected {
                        corners: corners(p.corners),
                        w: corners(p.w),
                    }),
                    shape: if q.shape.is_plain() {
                        Shape::NONE
                    } else {
                        Shape::of(&q.shape, q.size)
                    },
                    blur: local_blur(q.blur, &Affine(q.transform)),
                };
                if q.diffuse.iter().any(|c| c[3] > 0.0) {
                    list.quad(quad.clone());
                }
                if q.glow[3] > 0.0 {
                    list.quad(Quad {
                        colors: [q.glow; 4],
                        glow: true,
                        ..quad
                    });
                }
            }
            Drawn::Text(t) => {
                let mask = mask(t.depth, t.blend);
                let blend = blend(t.blend);
                // From the line box's top left, as the list places runs.
                let transform = Affine(t.line_box_transform());
                let run = TextRun {
                    text: t.text.clone(),
                    style: t.style.clone(),
                    transform,
                    // White text tinted: the upper left corner's color.
                    color: t.diffuse[0],
                    blend,
                    mask,
                    perspective: t.line_box_perspective(),
                    blur: local_blur(t.blur, &transform),
                };
                if t.diffuse[0][3] > 0.0 {
                    list.push(Command::Text(run.clone()));
                }
                // Glowing white text is the glow's color.
                if t.glow[3] > 0.0 {
                    list.push(Command::Text(TextRun {
                        color: t.glow,
                        ..run
                    }));
                }
            }
            Drawn::ClipPush(c) => list.push(if c.exact {
                Command::PushClip(c.scissor)
            } else {
                Command::PushClipQuad {
                    corners: corners(c.corners),
                    bound: c.scissor,
                }
            }),
            Drawn::ClipPop => list.pop_clip(),
            // Models need a depth buffer and the field's camera: the play
            // field's renderer draws them.
            Drawn::Model(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_actor::text::{FixedMeasure, TextStyle};
    use ddi_actor::{Actor, Kind, Sprite, TRANSFORM_IDENTITY, Text};

    fn draw(a: &mut Actor) -> DrawList {
        let mut out = Vec::new();
        a.draw(TRANSFORM_IDENTITY, [1.0; 4], [1.0, 1.0, 1.0, 0.0], &mut out);
        let mut list = DrawList::new();
        append(&out, &mut list, &mut |t| Some(TextureId(t)));
        list
    }

    fn run(a: &mut Actor, commands: &str) {
        let mut p = ddi_actor::Problems::default();
        for c in ddi_actor::commands::parse(commands).unwrap() {
            a.call(&c, &mut p, 0).unwrap();
        }
    }

    #[test]
    fn quads_keep_their_corners_and_glow() {
        let mut q = Actor::new(Kind::Sprite(Sprite::quad([1.0, 1.0])));
        run(
            &mut q,
            "zoomto,20,10;x,50;diffuse,1,0,0,1;diffuselowerleft,0,0,1,1;glow,1,1,1,0.5",
        );
        let l = draw(&mut q);
        let [Command::Quad(d), Command::Quad(g)] = l.commands.as_slice() else {
            panic!("{:?}", l.commands)
        };
        assert_eq!(d.texture, None);
        assert_eq!(d.rect, UNIT);
        assert_eq!(d.transform.apply([0.5, 0.5]), [60.0, 5.0]);
        // Lower left is the list's last corner.
        assert_eq!(d.colors[3], [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(d.colors[2], [1.0, 0.0, 0.0, 1.0]);
        assert!(g.glow && g.colors[0] == [1.0, 1.0, 1.0, 0.5]);
        // A textured sprite's texture through the map; one it lacks is not
        // drawn.
        let mut s = Actor::new(Kind::Sprite(Sprite::new(7, vec![], [4.0, 4.0])));
        let l = draw(&mut s);
        assert!(matches!(&l.commands[0], Command::Quad(q) if q.texture == Some(TextureId(7))));
        let mut out = Vec::new();
        s.draw(TRANSFORM_IDENTITY, [1.0; 4], [1.0, 1.0, 1.0, 0.0], &mut out);
        let mut none = DrawList::new();
        append(&out, &mut none, &mut |_| None);
        assert!(none.commands.is_empty());
    }

    #[test]
    fn masks_clips_and_perspective() {
        let mut source = Actor::new(Kind::Sprite(Sprite::quad([10.0, 10.0])));
        run(&mut source, "MaskSource,true");
        let mut dest = Actor::new(Kind::Sprite(Sprite::quad([10.0, 10.0])));
        run(&mut dest, "MaskDest");
        let mut f = Actor::new(Kind::Frame(vec![source, dest]));
        f.clip = Some([0.0, 0.0, 10.0, 10.0]);
        run(&mut f, "rotationz,30;fov,90");
        let l = draw(&mut f);
        let [
            Command::PushClipQuad { corners: c, bound },
            Command::Quad(clear),
            Command::Quad(write),
            Command::Quad(tested),
            Command::PopClip,
        ] = l.commands.as_slice()
        else {
            panic!("{:?}", l.commands)
        };
        assert!(bound[2] > bound[0] && c[0] != c[1]);
        assert_eq!((clear.mask, clear.blend), (Mask::Clear, Blend::NoEffect));
        assert_eq!((write.mask, write.blend), (Mask::Write, Blend::NoEffect));
        assert_eq!(tested.mask, Mask::Outside);
        // In perspective, by its corners.
        assert!(tested.projected.is_some());
        // An exact clip is a scissor.
        let mut g = Actor::new(Kind::Frame(vec![]));
        g.clip = Some([1.0, 2.0, 3.0, 4.0]);
        let mut inner = Actor::new(Kind::Sprite(Sprite::quad([1.0, 1.0])));
        run(&mut inner, "x,2;y,3");
        if let Kind::Frame(c) = &mut g.kind {
            c.push(inner);
        }
        let l = draw(&mut g);
        assert_eq!(l.commands[0], Command::PushClip([1.0, 2.0, 3.0, 4.0]));
    }

    #[test]
    fn text_runs_from_their_line_box() {
        let style = TextStyle::new("sans-serif", 20.0);
        let mut t = Actor::new(Kind::Text(Text::new("abc", style)));
        t.measure(&mut FixedMeasure);
        run(&mut t, "x,100;y,50;diffuse,1,0.5,0,1");
        let l = draw(&mut t);
        let [Command::Text(r)] = l.commands.as_slice() else {
            panic!("{:?}", l.commands)
        };
        // 30 wide, a line of 25, centred on (100, 50).
        assert_eq!(r.transform.apply([0.0, 0.0]), [85.0, 37.5]);
        assert_eq!(r.color, [1.0, 0.5, 0.0, 1.0]);
    }

    #[test]
    fn a_frames_motion_blur_reaches_what_is_inside_in_its_units() {
        let mut q = Actor::new(Kind::Sprite(Sprite::quad([1.0, 1.0])));
        run(&mut q, "zoomto,40,20");
        let mut t = Actor::new(Kind::Text(Text::new(
            "abc",
            TextStyle::new("sans-serif", 20.0),
        )));
        t.measure(&mut FixedMeasure);
        run(&mut t, "zoom,0.5");
        let mut f = Actor::new(Kind::Frame(vec![q, t]));
        // 10 of the frame's units along its y: 20 on the target.
        run(&mut f, "zoom,2;motionblur,10");
        let l = draw(&mut f);
        let [Command::Quad(q), Command::Text(r)] = l.commands.as_slice() else {
            panic!("{:?}", l.commands)
        };
        // The quad is 80 by 40 on the target: half its height.
        assert_eq!(q.blur, [0.0, 0.5]);
        // The run is drawn at a zoom of 1 on the target: 20 of its units.
        assert_eq!(r.blur, [0.0, 20.0]);
        // None: none at all.
        run(&mut f, "motionblur,0");
        let l = draw(&mut f);
        assert!(l.commands.iter().all(|c| match c {
            Command::Quad(q) => q.blur == [0.0; 2],
            Command::Text(r) => r.blur == [0.0; 2],
            _ => true,
        }));
    }

    #[test]
    fn text_in_perspective_from_its_line_box_too() {
        let style = TextStyle::new("sans-serif", 20.0);
        let mut t = Actor::new(Kind::Text(Text::new("abc", style)));
        t.measure(&mut FixedMeasure);
        run(&mut t, "x,100;y,50;rotationz,20;z,30");
        let mut f = Actor::new(Kind::Frame(vec![t]));
        run(&mut f, "fov,90");
        let l = draw(&mut f);
        let [Command::Text(r)] = l.commands.as_slice() else {
            panic!("{:?}", l.commands)
        };
        let (m, camera) = r.perspective.expect("in perspective");
        // The 3D placement starts where the flat one does, at the line
        // box's top left (not at the pen, a baseline lower), z aside.
        for p in [[0.0, 0.0], [30.0, 0.0], [0.0, 25.0], [30.0, 25.0]] {
            let a = ddi_actor::transform_point(&m, [p[0], p[1], 0.0]);
            let b = r.transform.apply(p);
            assert!(
                (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3,
                "{a:?} {b:?}"
            );
            assert!((a[2] - 30.0).abs() < 1e-3);
        }
        // Nearer the eye than the screen: seen larger.
        assert!(camera.project([0.0, 0.0, 30.0]).unwrap().1 < 1.0);
    }

    #[test]
    fn only_a_mask_writes_the_mask() {
        use ddi_actor::Depth;
        let write = Depth {
            write: true,
            ..Depth::default()
        };
        let both = Depth {
            test: ZTest::WriteOnPass,
            ..write
        };
        assert_eq!(mask(write, ActorBlend::NoEffect), Mask::Write);
        assert_eq!(mask(both, ActorBlend::NoEffect), Mask::Write);
        // Another blend mode writes the depth the rest lies at: untested,
        // over the mask; tested, as a test alone.
        assert_eq!(mask(write, ActorBlend::Normal), Mask::Erase);
        assert_eq!(mask(both, ActorBlend::Normal), Mask::Outside);
        assert_eq!(mask(Depth::default(), ActorBlend::Add), Mask::None);
    }
}
