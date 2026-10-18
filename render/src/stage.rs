//! Dancing☆Onigiri's back and mask layers (`docs/plans/overlays.md`) as
//! textured quads: danoniplus's stage mapped onto the screen as the
//! lyrics' field is (decision 29), its height the screen's, centred on the
//! field; each content placed as its `<img>` or `<span>` is, then
//! transformed about its centre as CSS transforms it.

use ddi_engine::sprites::{SpriteDraw, SpriteSource, StageLayer};

use crate::color::to_target;
use crate::textured::{QuadBlend, Shape, TexQuad};

/// A content's texture: its registered id and its own size in the stage's
/// px (an image's natural size; text as laid out).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct StageTexture {
    pub id: u32,
    pub size: (f32, f32),
}

/// The back and mask quads of `draws`, on a `stage` (danoniplus's px)
/// shown `height` px tall with its centre at `center_x`; `texture` finds a
/// content's texture (none: not drawn yet). Colors as the target expects
/// them for skins' quads (`srgb`, see [`to_target`]).
pub fn quads(
    draws: &[SpriteDraw],
    stage: (f32, f32),
    center_x: f32,
    height: f32,
    texture: impl Fn(&SpriteSource) -> Option<StageTexture>,
    srgb: bool,
) -> (Vec<TexQuad>, Vec<TexQuad>) {
    let mut back = Vec::new();
    let mut mask = Vec::new();
    let s = height / stage.1.max(1.0);
    let left = center_x - stage.0 * s / 2.0;
    for d in draws {
        let Some(tex) = texture(&d.source) else {
            continue;
        };
        let opacity = d.opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 {
            continue;
        }
        let (w, h) = size(d.w, d.h, tex.size);
        if w <= 0.0 || h <= 0.0 {
            continue;
        }
        let (w, h) = (w * s, h * s);
        let cx = left + d.x * s + w / 2.0 + d.translate[0] * s;
        let cy = d.y * s + h / 2.0 + d.translate[1] * s;
        let (sin, cos) = d.rotate.to_radians().sin_cos();
        let (sx, sy) = (w * d.scale[0], h * d.scale[1]);
        // The stage clips its layers (`#canvas-frame { overflow: hidden }`):
        // a quad not turned is cropped to it, its texture with it; a turned
        // one (`spinZ`) is drawn whole.
        let shape = if d.rotate.rem_euclid(360.0) == 0.0 {
            let clip = [left, 0.0, left + stage.0 * s, stage.1 * s];
            match crop(cx, cy, sx, sy, clip) {
                Some(shape) => shape,
                None => continue,
            }
        } else {
            Shape::Quad {
                transform: [cos * sx, sin * sx, -sin * sy, cos * sy, cx, cy],
                uv: [0.0, 0.0, 1.0, 1.0],
            }
        };
        // Brightness can only darken here (a multiply).
        let b = d.brightness.clamp(0.0, 1.0);
        let quad = TexQuad {
            shape,
            color: to_target([b, b, b, opacity], srgb),
            glow: false,
            texture: tex.id,
            blend: QuadBlend::Normal,
            wrap: false,
            gradient: None,
            mask: Default::default(),
        };
        match d.layer {
            StageLayer::Back => back.push(quad),
            StageLayer::Mask => mask.push(quad),
        }
    }
    (back, mask)
}

/// A quad `|sx| × |sy|` px centred at `(cx, cy)`, mirrored where a side is
/// negative, cropped to `clip` (`[left, top, right, bottom]`) with its
/// texture coordinates; `None` when nothing of it is inside.
fn crop(cx: f32, cy: f32, sx: f32, sy: f32, clip: [f32; 4]) -> Option<Shape> {
    // One axis: the visible span and the texture's span over it.
    let axis = |c: f32, size: f32, lo: f32, hi: f32| -> Option<(f32, f32, f32, f32)> {
        let half = size.abs() / 2.0;
        let (a, b) = ((c - half).max(lo), (c + half).min(hi));
        if b <= a || half <= 0.0 {
            return None;
        }
        // Texture fraction at screen position p, mirrored when negative.
        let t = |p: f32| {
            let f = (p - (c - half)) / (2.0 * half);
            if size < 0.0 { 1.0 - f } else { f }
        };
        Some((a, b, t(a), t(b)))
    };
    let (x0, x1, u0, u1) = axis(cx, sx, clip[0], clip[2])?;
    let (y0, y1, v0, v1) = axis(cy, sy, clip[1], clip[3])?;
    // The quad runs left to right on screen; a mirror swaps the
    // texture's span instead.
    Some(Shape::Quad {
        transform: [x1 - x0, 0.0, 0.0, y1 - y0, (x0 + x1) / 2.0, (y0 + y1) / 2.0],
        uv: [u0, v0, u1, v1],
    })
}

/// An `<img>`'s size: as given, the other side keeping the image's shape
/// when one is 0, its own size when both are.
fn size(w: f32, h: f32, own: (f32, f32)) -> (f32, f32) {
    let (ow, oh) = own;
    match (w > 0.0, h > 0.0) {
        (true, true) => (w, h),
        (true, false) if ow > 0.0 => (w, oh * w / ow),
        (false, true) if oh > 0.0 => (ow * h / oh, h),
        _ => own,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    fn draw(layer: StageLayer, x: f32, w: f32, h: f32) -> SpriteDraw {
        SpriteDraw {
            layer,
            depth: 0,
            source: SpriteSource::Image(Rc::from("a.png")),
            x,
            y: 10.0,
            w,
            h,
            opacity: 1.0,
            translate: [0.0, 0.0],
            scale: [1.0, 1.0],
            rotate: 0.0,
            brightness: 1.0,
        }
    }

    #[test]
    fn the_stage_maps_onto_the_screen_around_the_field() {
        // A 600 × 500 stage on a 1000 px tall screen, centred at 800: 2 px
        // a stage px, the stage from 200 to 1400.
        let tex = |_: &SpriteSource| {
            Some(StageTexture {
                id: 3,
                size: (40.0, 20.0),
            })
        };
        let draws = [
            draw(StageLayer::Back, 0.0, 0.0, 0.0),
            draw(StageLayer::Mask, 100.0, 80.0, 0.0),
        ];
        let (back, mask) = quads(&draws, (600.0, 500.0), 800.0, 1000.0, tex, false);
        let t = |q: &TexQuad| match q.shape {
            Shape::Quad { transform, .. } => transform,
            _ => unreachable!(),
        };
        // Its own size, 40 × 20: 80 × 40 px, top left at (200, 20).
        assert_eq!(t(&back[0]), [80.0, 0.0, 0.0, 40.0, 240.0, 40.0]);
        // 80 wide, the height keeping its shape: 160 × 80 px.
        assert_eq!(
            t(&mask[0]),
            [160.0, 0.0, 0.0, 80.0, 200.0 + 200.0 + 80.0, 60.0]
        );
        assert_eq!(back[0].texture, 3);
    }

    #[test]
    fn layers_are_clipped_to_the_stage() {
        let tex = |_: &SpriteSource| {
            Some(StageTexture {
                id: 0,
                size: (100.0, 100.0),
            })
        };
        let uv = |q: &TexQuad| match q.shape {
            Shape::Quad { uv, transform } => (uv, transform),
            _ => unreachable!(),
        };
        // Half of it left of the stage (500 × 500 on 500 px at 250).
        let mut d = draw(StageLayer::Back, -50.0, 0.0, 0.0);
        let (back, _) = quads(
            std::slice::from_ref(&d),
            (500.0, 500.0),
            250.0,
            500.0,
            tex,
            false,
        );
        let (uv0, m) = uv(&back[0]);
        assert_eq!(uv0, [0.5, 0.0, 1.0, 1.0]);
        assert_eq!((m[0], m[4]), (50.0, 25.0));
        // Mirrored (spinY past a quarter turn): the texture's span swaps.
        d.scale = [-1.0, 1.0];
        let (back, _) = quads(
            std::slice::from_ref(&d),
            (500.0, 500.0),
            250.0,
            500.0,
            tex,
            false,
        );
        assert_eq!(uv(&back[0]).0, [0.5, 0.0, 0.0, 1.0]);
        // Outside the stage: nothing.
        let far = draw(StageLayer::Mask, 600.0, 0.0, 0.0);
        let (_, mask) = quads(&[far], (500.0, 500.0), 250.0, 500.0, tex, false);
        assert!(mask.is_empty());
    }
}
