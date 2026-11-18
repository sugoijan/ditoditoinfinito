//! A song's heart: a favorite (filled) or not (an outline), on song
//! select (`docs/plans/song-navigation.md`, "Favorites").
//!
//! A vector shape of our own, never a glyph, as a CPU's stars are
//! ([`crate::stars`]). The classic heart is a square on its corner with a
//! disc on each of its upper sides; it is drawn here as two halves that
//! never overlap, so it reads the same drawn faintly (a wheel's far rows,
//! a screen fading) as whole: each half a stadium (a quad as long as two
//! sides of the square, rounded to half its width) turned an eighth, its
//! rounded end the lobe, its straight part the square, cut at the heart's
//! middle by its own clip (left half, right half). The outline is the
//! same two stadiums drawn as their edges alone. This module has its
//! geometry ([`shape`], [`width`]) and the screens' component: [`heart`]
//! (a look's `Heart` component, in its colors `Heart` and `Heart.Empty`),
//! sized by a height.

use ddi_actor::{Actor, Rgba};

use crate::look::{Spec, call, quad};
use crate::screens::{Env, frame};

/// A heart's width in its height.
pub const WIDTH: f32 = 1.093_8;

/// The square's half diagonal in the heart's height: its height is the
/// square's from its point to its side corners' line plus a lobe over
/// that line (`d + d/2 + d/√2`).
const HALF_DIAGONAL: f32 = 1.0 / 2.207_1;

/// An outline's thickness in the heart's height.
const STROKE: f32 = 0.1;

/// A heart's width at `height`.
pub fn width(height: f32) -> f32 {
    height * WIDTH
}

/// An outline's thickness at `height` (never under a pixel and a half).
pub fn stroke(height: f32) -> f32 {
    (height * STROKE).max(1.5)
}

/// A heart's shape: the square's middle and side, and its two lobes'
/// middles (the square's upper sides' middles) and radius (half the
/// side). Each half is a stadium as long as two sides and a side wide,
/// round its middle the square's, turned an eighth (the left half's long
/// way down to the right, the right half's up), cut at the middle's x.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub middle: [f32; 2],
    pub side: f32,
    pub lobes: [[f32; 2]; 2],
    pub radius: f32,
}

/// The heart `height` tall whose box's middle is `center`, its point down.
pub fn shape(center: [f32; 2], height: f32) -> Shape {
    let d = height * HALF_DIAGONAL;
    let r = d * std::f32::consts::FRAC_1_SQRT_2;
    // The box spans from a lobe's top (d/2 + r over the square's middle)
    // to the point (d under it): the square's middle a little under the
    // box's.
    let middle = [center[0], center[1] + (d / 2.0 + r - d) / 2.0];
    Shape {
        middle,
        side: d * std::f32::consts::SQRT_2,
        lobes: [
            [middle[0] - d / 2.0, middle[1] - d / 2.0],
            [middle[0] + d / 2.0, middle[1] - d / 2.0],
        ],
        radius: r,
    }
}

/// The plain look's colors: the filled heart's and the outline's (a
/// look's `Heart` and `Heart.Empty` otherwise).
pub const COLORS: [Rgba; 2] = [[0.95, 0.28, 0.42, 1.0], [0.70, 0.67, 0.65, 1.0]];

/// The heart as actors, `height` tall, around its middle: filled (in
/// `colors[0]`) or an outline (in `colors[1]`): a frame named `Heart`
/// holding one frame, `Filled` or `Outline`, of its two halves, each a
/// clipped frame of one stadium. An adornment: never hit (a button around
/// it takes the pointer).
pub fn actor(filled: bool, height: f32, colors: [Rgba; 2]) -> Actor {
    let s = shape([0.0, 0.0], height);
    let color = if filled { colors[0] } else { colors[1] };
    // Room enough each way: a clip cuts at the middle alone.
    let far = height * 2.0;
    let halves = [
        (45.0_f32, [-far, -far, 0.0, far]),
        (-45.0, [0.0, -far, far, far]),
    ]
    .into_iter()
    .map(|(turn, clip)| {
        let mut stadium = quad("", [2.0 * s.side, s.side], color);
        call(&mut stadium, "cornerradius", &[s.side / 2.0]);
        if !filled {
            call(&mut stadium, "outline", &[stroke(height)]);
        }
        call(&mut stadium, "rotationz", &[turn]);
        call(&mut stadium, "xy", &[0.0, s.middle[1]]);
        let mut half = frame("", vec![stadium]);
        half.clip = Some(clip);
        half
    })
    .collect();
    let mut a = frame(
        "Heart",
        vec![frame(if filled { "Filled" } else { "Outline" }, halves)],
    );
    a.decoration = true;
    a
}

/// The heart in the look's colors, `height` tall, named `name`, around
/// its middle: filled (a favorite) or an outline (the look's `Heart`
/// component, its spec's text `on` or `off`; a look without one gets
/// the plain colors).
pub fn heart(env: &Env, name: &str, filled: bool, height: f32) -> Actor {
    let spec = Spec::new([width(height), height], if filled { "on" } else { "off" });
    let mut a = env
        .look
        .component("Heart", &spec)
        .unwrap_or_else(|| actor(filled, height, COLORS));
    a.name = name.into();
    a
}

/// Whether `heart` (an actor [`heart`] made, or what holds one) shows
/// filled (`Some(true)`), as an outline (`Some(false)`) or not at all
/// (tests, and the debug state's check).
pub fn shown(heart: &Actor) -> Option<bool> {
    let mut walk = vec![heart];
    while let Some(a) = walk.pop() {
        if !a.visible {
            continue;
        }
        match a.name.as_str() {
            "Filled" => return Some(true),
            "Outline" => return Some(false),
            _ => {}
        }
        if let ddi_actor::Kind::Frame(children) = &a.kind {
            walk.extend(children.iter());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_heart_s_shape_stays_in_its_box() {
        let h = 24.0;
        let s = shape([0.0, 0.0], h);
        let d = s.side / std::f32::consts::SQRT_2;
        // The lobes' tops and outer sides, and the point, on the box.
        let top = s.lobes[0][1] - s.radius;
        let point = s.middle[1] + d;
        assert!((top + h / 2.0).abs() < 0.05, "{top}");
        assert!((point - h / 2.0).abs() < 0.05, "{point}");
        let (left, right) = (s.lobes[0][0] - s.radius, s.lobes[1][0] + s.radius);
        assert!((right - left - width(h)).abs() < 0.05, "{left} {right}");
        assert!((left + right).abs() < 1e-3);
        // Each lobe a disc on an upper side of the square: through its
        // side corner and its top corner.
        let [cx, cy] = s.middle;
        for (lobe, corner) in s.lobes.iter().zip([[cx - d, cy], [cx + d, cy]]) {
            for p in [corner, [cx, cy - d]] {
                assert!(((p[0] - lobe[0]).hypot(p[1] - lobe[1]) - s.radius).abs() < 1e-3);
            }
        }
        // The stadium's round end is the lobe: half its width, half a
        // side from the middle.
        assert!((s.radius - s.side / 2.0).abs() < 1e-3);
    }

    #[test]
    fn the_actor_is_two_halves_filled_or_outlined_and_never_hit() {
        let filled = actor(true, 12.0, COLORS);
        let outline = actor(false, 12.0, COLORS);
        assert_eq!(shown(&filled), Some(true));
        assert_eq!(shown(&outline), Some(false));
        assert!(filled.decoration && outline.decoration);
        let halves = |a: &Actor| match &a.kind {
            ddi_actor::Kind::Frame(c) => match &c[0].kind {
                ddi_actor::Kind::Frame(p) => p.iter().map(|h| h.clip).collect::<Vec<_>>(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
        // Two halves meeting at the middle, each cut there.
        for a in [&filled, &outline] {
            let clips = halves(a);
            assert_eq!(clips.len(), 2);
            assert_eq!(clips[0].unwrap()[2], 0.0);
            assert_eq!(clips[1].unwrap()[0], 0.0);
        }
        assert!(stroke(4.0) >= 1.5);
    }
}
