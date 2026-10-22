//! The 2D pass's draw list (`docs/plans/ui-core.md`, "Draw list" and
//! P11): what a screen draws, in order, in its units (virtual pixels;
//! [`crate::painter::Painter::draw`] scales them to physical pixels).
//!
//! Quads (a texture's rectangle or a plain color, under a 2D affine
//! transform or seen in perspective, one color a corner, StepMania's
//! blend modes, glow and masks), text runs (a string in a style, its line
//! box's top left at the transform's origin, tinted), clips pushed and
//! popped (scissor rectangles, or any quad through the stencil), and
//! external slots, where the shell draws
//! something of its own between the list's layers (the play field, so a
//! theme's layers can lie under and over it). Colors are encoded values
//! with straight alpha: the 2D pass blends as StepMania and the browser
//! do, in encoded values (P11).

use std::sync::Arc;

pub use ddi_actor::Fit;
use ddi_actor::Perspective;
use ddi_actor::text::TextStyle;

pub use crate::stencil::Mask;
use crate::texture_cache::TextureId;
pub use crate::textured::QuadBlend as Blend;

/// A 2D affine transform: `x' = m0 x + m2 y + m4`, `y' = m1 x + m3 y + m5`
/// (the column order of [`crate::textured::Shape::Quad`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine(pub [f32; 6]);

impl Default for Affine {
    fn default() -> Affine {
        Affine::IDENTITY
    }
}

impl Affine {
    pub const IDENTITY: Affine = Affine([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    pub fn translate(x: f32, y: f32) -> Affine {
        Affine([1.0, 0.0, 0.0, 1.0, x, y])
    }

    pub fn scale(x: f32, y: f32) -> Affine {
        Affine([x, 0.0, 0.0, y, 0.0, 0.0])
    }

    /// Turned clockwise on the screen (y grows downwards) by `radians`.
    pub fn rotate(radians: f32) -> Affine {
        let (s, c) = radians.sin_cos();
        Affine([c, s, -s, c, 0.0, 0.0])
    }

    /// This transform, then `outer`.
    pub fn then(self, outer: Affine) -> Affine {
        Affine(ddi_actor::mul(&outer.0, &self.0))
    }

    pub fn apply(&self, p: [f32; 2]) -> [f32; 2] {
        ddi_actor::apply(&self.0, p)
    }

    /// A move alone (no scaling, turning or skewing).
    pub fn is_translation(&self) -> bool {
        let m = self.0;
        m[0] == 1.0 && m[1] == 0.0 && m[2] == 0.0 && m[3] == 1.0
    }

    /// A vector of the target in this transform's own space (its linear
    /// part undone); `[0, 0]` when it flattens everything.
    pub fn unapply_vector(&self, [x, y]: [f32; 2]) -> [f32; 2] {
        let m = self.0;
        let det = m[0] * m[3] - m[1] * m[2];
        if det == 0.0 || !det.is_finite() {
            return [0.0; 2];
        }
        [(m[3] * x - m[2] * y) / det, (m[0] * y - m[1] * x) / det]
    }
}

/// A rectangle `[left, top, right, bottom]`.
pub type Rect = [f32; 4];

/// A quad: `rect` in its own space, placed by `transform`.
#[derive(Clone, Debug, PartialEq)]
pub struct Quad {
    pub rect: Rect,
    pub transform: Affine,
    /// `None`: the colors alone.
    pub texture: Option<TextureId>,
    /// How the texture fits `rect`: stretched, center-cropped (cover), or
    /// whole and centered in it (contain, letterboxed).
    pub fit: Fit,
    /// The texture's rectangle, in fractions of it.
    pub uv: Rect,
    /// Top left, top right, bottom right, bottom left; encoded, straight
    /// alpha.
    pub colors: [[f32; 4]; 4],
    pub blend: Blend,
    /// StepMania's glow pass: the colors with the texture's alpha.
    pub glow: bool,
    pub mask: Mask,
    /// Seen in perspective: `rect` and `transform` then give way to these
    /// corners.
    pub projected: Option<Projected>,
    /// Rounded corners, a soft edge, an outline, saturation (our
    /// extension; not drawn in perspective).
    pub shape: Shape,
    /// A motion blur: the quad smeared along this vector, in its rect's
    /// units, centered (our extension; not drawn in perspective). `[0, 0]`
    /// draws exactly as before.
    pub blur: [f32; 2],
}

/// How a quad is cut and colored beyond StepMania's (actors'
/// `ddi_actor::Shape`), in its rectangle's units, per axis. [`Shape::NONE`]
/// draws exactly as before.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub radius: [f32; 2],
    /// A soft edge each way of the shape, which is the rectangle inset by
    /// as much.
    pub soft: [f32; 2],
    /// Only an outline this thick (0: filled).
    pub outline: [f32; 2],
    /// 1 as it is, 0 grey.
    pub saturation: f32,
}

impl Shape {
    pub const NONE: Shape = Shape {
        radius: [0.0; 2],
        soft: [0.0; 2],
        outline: [0.0; 2],
        saturation: 1.0,
    };

    /// Cut (corners, edge or outline).
    pub fn is_cut(&self) -> bool {
        self.radius != [0.0; 2] || self.soft != [0.0; 2] || self.outline != [0.0; 2]
    }

    /// From an actor's, for a quad of `size` units drawn as the unit
    /// square.
    pub fn of(s: &ddi_actor::Shape, size: [f32; 2]) -> Shape {
        let per = |v: f32| {
            [
                if size[0] > 0.0 { v / size[0] } else { 0.0 },
                if size[1] > 0.0 { v / size[1] } else { 0.0 },
            ]
        };
        Shape {
            radius: per(s.radius),
            soft: per(s.soft),
            outline: per(s.outline),
            saturation: s.saturation,
        }
    }
}

impl Default for Shape {
    fn default() -> Shape {
        Shape::NONE
    }
}

/// A quad seen in perspective (an actor under an `fov` frame).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projected {
    /// Top left, top right, bottom right, bottom left, in units.
    pub corners: [[f32; 2]; 4],
    /// Each corner's `w`, for texturing in perspective.
    pub w: [f32; 4],
}

impl Quad {
    /// `rect` in one color, untextured.
    pub fn plain(rect: Rect, color: [f32; 4]) -> Quad {
        Quad {
            rect,
            transform: Affine::IDENTITY,
            texture: None,
            fit: Fit::Stretch,
            uv: [0.0, 0.0, 1.0, 1.0],
            colors: [color; 4],
            blend: Blend::Normal,
            glow: false,
            mask: Mask::None,
            projected: None,
            shape: Shape::NONE,
            blur: [0.0; 2],
        }
    }
}

/// A run of text on one line.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub text: Arc<str>,
    pub style: TextStyle,
    /// Places the line box, whose top left is the origin.
    pub transform: Affine,
    /// The tint (the run is drawn white).
    pub color: [f32; 4],
    pub blend: Blend,
    pub mask: Mask,
    /// Seen in perspective: the run's space (its line box's top left at
    /// the origin, z toward the viewer) in units, a column-major 4×4, and
    /// the camera; `transform` then gives the size it is drawn at.
    pub perspective: Option<([f32; 16], Perspective)>,
    /// A motion blur along this vector in the run's units, as
    /// [`Quad::blur`].
    pub blur: [f32; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Quad(Quad),
    Text(TextRun),
    /// Clips what follows to this rectangle of the screen (within the
    /// clip already in force) until the matching [`Command::PopClip`].
    PushClip(Rect),
    /// Clips what follows to this quad (top left, top right, bottom
    /// right, bottom left, in units), through the stencil, and to its
    /// `bound`, a scissor rectangle.
    PushClipQuad {
        corners: [[f32; 2]; 4],
        bound: Rect,
    },
    PopClip,
    /// The shell draws slot `n` here (`Painter::draw`'s `external`).
    External(u32),
}

/// What a screen draws in a frame, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DrawList {
    pub commands: Vec<Command>,
}

impl DrawList {
    pub fn new() -> DrawList {
        DrawList::default()
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }

    pub fn push(&mut self, command: Command) {
        self.commands.push(command);
    }

    /// A rectangle in one color.
    pub fn rect(&mut self, rect: Rect, color: [f32; 4]) {
        self.push(Command::Quad(Quad::plain(rect, color)));
    }

    /// A texture's whole picture in `rect`, tinted.
    pub fn image(&mut self, rect: Rect, texture: TextureId, tint: [f32; 4]) {
        self.push(Command::Quad(Quad {
            texture: Some(texture),
            ..Quad::plain(rect, tint)
        }));
    }

    pub fn quad(&mut self, quad: Quad) {
        self.push(Command::Quad(quad));
    }

    /// `text` with its line box's top left at `(x, y)`.
    pub fn text(
        &mut self,
        text: impl Into<Arc<str>>,
        style: &TextStyle,
        x: f32,
        y: f32,
        color: [f32; 4],
    ) {
        self.push(Command::Text(TextRun {
            text: text.into(),
            style: style.clone(),
            transform: Affine::translate(x, y),
            color,
            blend: Blend::Normal,
            mask: Mask::None,
            perspective: None,
            blur: [0.0; 2],
        }));
    }

    pub fn push_clip(&mut self, rect: Rect) {
        self.push(Command::PushClip(rect));
    }

    pub fn pop_clip(&mut self) {
        self.push(Command::PopClip);
    }

    pub fn external(&mut self, slot: u32) {
        self.push(Command::External(slot));
    }
}

/// The clips in force: each pushed rectangle within the one before.
#[derive(Clone, Debug, Default)]
pub struct ClipStack {
    stack: Vec<Rect>,
}

impl ClipStack {
    pub fn push(&mut self, rect: Rect) {
        let r = match self.stack.last() {
            Some(o) => [
                rect[0].max(o[0]),
                rect[1].max(o[1]),
                rect[2].min(o[2]),
                rect[3].min(o[3]),
            ],
            None => rect,
        };
        self.stack.push(r);
    }

    /// An unmatched pop is ignored.
    pub fn pop(&mut self) {
        self.stack.pop();
    }

    /// The clip in force (`None`: none).
    pub fn current(&self) -> Option<Rect> {
        self.stack.last().copied()
    }

    pub fn clear(&mut self) {
        self.stack.clear();
    }
}

/// The scissor rectangle `[x, y, width, height]` of `clip` (units) on a
/// `size` target at `scale` pixels a unit; `None` when nothing of it is
/// left.
pub fn scissor(clip: Option<Rect>, scale: f32, size: [u32; 2]) -> Option<[u32; 4]> {
    let Some(c) = clip else {
        return Some([0, 0, size[0], size[1]]);
    };
    let px = |v: f32, max: u32| ((v * scale).round().max(0.0) as u32).min(max);
    let (x0, y0) = (px(c[0], size[0]), px(c[1], size[1]));
    let (x1, y1) = (px(c[2], size[0]), px(c[3], size[1]));
    (x1 > x0 && y1 > y0).then(|| [x0, y0, x1 - x0, y1 - y0])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5
    }

    #[test]
    fn transforms_compose_in_order() {
        // Scaled, then turned a quarter, then moved.
        let t = Affine::scale(2.0, 3.0)
            .then(Affine::rotate(std::f32::consts::FRAC_PI_2))
            .then(Affine::translate(10.0, 20.0));
        // (1, 0) → (2, 0) → (0, 2) → (10, 22).
        assert!(close(t.apply([1.0, 0.0]), [10.0, 22.0]));
        // (0, 1) → (0, 3) → (-3, 0) → (7, 20).
        assert!(close(t.apply([0.0, 1.0]), [7.0, 20.0]));
        assert!(Affine::translate(3.0, 4.0).is_translation());
        assert!(!t.is_translation());
        assert_eq!(
            Affine::IDENTITY.then(Affine::translate(1.0, 2.0)),
            Affine::translate(1.0, 2.0)
        );
    }

    #[test]
    fn clips_nest_and_become_scissors() {
        let mut c = ClipStack::default();
        assert_eq!(scissor(c.current(), 2.0, [100, 80]), Some([0, 0, 100, 80]));
        c.push([10.0, 10.0, 40.0, 30.0]);
        c.push([20.0, 0.0, 60.0, 20.0]);
        assert_eq!(c.current(), Some([20.0, 10.0, 40.0, 20.0]));
        // At 2 px a unit, clamped to the target.
        assert_eq!(scissor(c.current(), 2.0, [100, 80]), Some([40, 20, 40, 20]));
        assert_eq!(scissor(c.current(), 2.0, [60, 80]), Some([40, 20, 20, 20]));
        // Disjoint: nothing is drawn.
        c.push([100.0, 100.0, 120.0, 120.0]);
        assert_eq!(scissor(c.current(), 1.0, [500, 500]), None);
        c.pop();
        c.pop();
        assert_eq!(c.current(), Some([10.0, 10.0, 40.0, 30.0]));
        c.pop();
        c.pop();
        assert_eq!(c.current(), None);
    }

    #[test]
    fn the_list_keeps_its_order() {
        let mut l = DrawList::new();
        let style = TextStyle::new("sans-serif", 16.0);
        l.rect([0.0, 0.0, 10.0, 10.0], [1.0; 4]);
        l.push_clip([0.0, 0.0, 5.0, 5.0]);
        l.text("革命", &style, 1.0, 2.0, [1.0; 4]);
        l.pop_clip();
        l.external(0);
        assert_eq!(l.commands.len(), 5);
        assert!(
            matches!(&l.commands[2], Command::Text(t) if &*t.text == "革命" && t.transform == Affine::translate(1.0, 2.0))
        );
        assert_eq!(l.commands[4], Command::External(0));
    }
}
