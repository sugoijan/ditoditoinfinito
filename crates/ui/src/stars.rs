//! A CPU's level as five stars (the maintainer, 2026-11-12: "silver/gold
//! star ratings", less text; `docs/plans/multiplayer-prototype-progress.md`,
//! feedback round A).
//!
//! A level is always five stars: levels 1 to 5 fill them silver from the
//! left (level 3: three silver, two empty outlines), levels 6 to 10 turn
//! them gold from the left (level 7: two gold, three silver; level 10:
//! five gold). Never a number, never "of 5".
//!
//! The stars are vector shapes, not glyphs (no face has to carry them):
//! each filled star five rhombi (a tip to the far side of the middle) and
//! a disc, each empty one its outline's ten strokes. This module has
//! their geometry ([`shape`], [`width`]), which the play's miniatures draw
//! as quads (`ddi_shell::opponents`), and the screens' component:
//! [`stars`] (a look's `Stars` component, in its colors: `Star.Gold`,
//! `Star.Silver`, `Star.Empty`), sized by a height, placed with
//! [`stars_left`] or [`stars_right`].

use ddi_actor::{Actor, Rgba};

use crate::look::{self, Spec, call, quad};
use crate::screens::{Env, frame};

/// How a star of a level is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Star {
    Empty,
    Silver,
    Gold,
}

impl Star {
    /// Its part's and its color's name (`Star.Gold`).
    pub fn name(self) -> &'static str {
        match self {
            Star::Empty => "Empty",
            Star::Silver => "Silver",
            Star::Gold => "Gold",
        }
    }
}

/// How many stars a level is.
pub const STARS: usize = 5;
/// The highest level (`ddi_engine::cpu::LEVELS`).
pub const LEVELS: u8 = 10;

/// Level `level` (from 1 to [`LEVELS`]) as its five stars from the left.
pub fn fills(level: u8) -> [Star; STARS] {
    let level = usize::from(level.min(LEVELS));
    std::array::from_fn(|i| {
        if level > STARS {
            if i < level - STARS {
                Star::Gold
            } else {
                Star::Silver
            }
        } else if i < level {
            Star::Silver
        } else {
            Star::Empty
        }
    })
}

/// A star's width, the room between two and an outline's thickness, in
/// its height.
const STAR_WIDTH: f32 = 1.051_5;
const GAP: f32 = 0.14;
const STROKE: f32 = 0.11;
/// Its inner corners' distance from its middle, in its tips' (a little
/// plumper than a pentagram's 0.382: it reads at ten pixels).
const INNER: f32 = 0.46;
/// A star's height in its tips' distance from its middle (a tip up).
const HEIGHT: f32 = 1.809;

/// Five stars' width at `height`.
pub fn width(height: f32) -> f32 {
    height * (STARS as f32 * STAR_WIDTH + (STARS as f32 - 1.0) * GAP)
}

/// The stars' middles, the first's left edge at `left`, all on `y`.
pub fn centers(left: f32, y: f32, height: f32) -> [[f32; 2]; STARS] {
    std::array::from_fn(|i| {
        [
            left + height * (STAR_WIDTH / 2.0 + i as f32 * (STAR_WIDTH + GAP)),
            y,
        ]
    })
}

/// An outline's thickness at `height` (never under a pixel).
pub fn stroke(height: f32) -> f32 {
    (height * STROKE).max(1.0)
}

/// One of a filled star's five rhombi.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rhombus {
    pub center: [f32; 2],
    /// Its long diagonal's direction (to its tip), radians clockwise on
    /// the screen from the x axis.
    pub angle: f32,
    /// Its diagonals: along its tip's direction, and across.
    pub diagonals: [f32; 2],
    /// Its two sides from its corner opposite the tip.
    pub sides: [[f32; 2]; 2],
}

/// A star's shape: its five rhombi and the disc at its middle (filled),
/// and its outline's ten corners in order (a tip first).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub rhombi: [Rhombus; 5],
    pub disc: ([f32; 2], f32),
    pub outline: [[f32; 2]; 10],
}

/// The star `height` tall whose box's middle is `center`, a tip up.
pub fn shape(center: [f32; 2], height: f32) -> Shape {
    let tip = height / HEIGHT;
    let inner = tip * INNER;
    // Its tips' middle sits a little under its box's.
    let mid = [center[0], center[1] + tip * (1.0 - 0.809) / 2.0];
    let (s36, c36) = (36.0_f32.to_radians().sin(), 36.0_f32.to_radians().cos());
    let at =
        |radius: f32, angle: f32| [mid[0] + radius * angle.cos(), mid[1] + radius * angle.sin()];
    let angle = |k: usize| (-90.0 + 72.0 * k as f32).to_radians();
    let turn = 36.0_f32.to_radians();
    // Each rhombus: a tip, the inner corners beside it and their sum less
    // the tip, on the tip's line past the middle.
    let far = 2.0 * inner * c36 - tip;
    let rhombi = std::array::from_fn(|k| {
        let a = angle(k);
        let (t, f) = (at(tip, a), at(far, a));
        let (p, q) = (at(inner, a + turn), at(inner, a - turn));
        Rhombus {
            center: [(t[0] + f[0]) / 2.0, (t[1] + f[1]) / 2.0],
            angle: a,
            diagonals: [tip - far, 2.0 * inner * s36],
            sides: [[p[0] - f[0], p[1] - f[1]], [q[0] - f[0], q[1] - f[1]]],
        }
    });
    let outline = std::array::from_fn(|i| {
        let a = angle(i / 2);
        if i % 2 == 0 {
            at(tip, a)
        } else {
            at(inner, a + turn)
        }
    });
    Shape {
        rhombi,
        // Inside the inner corners' pentagon.
        disc: (mid, inner * c36),
        outline,
    }
}

/// The plain look's colors, gold, silver and the empty outline (a look's
/// `Star.Gold`, `Star.Silver` and `Star.Empty` otherwise).
pub const COLORS: [Rgba; 3] = [
    [1.0, 0.82, 0.30, 1.0],
    [0.80, 0.83, 0.88, 1.0],
    [0.50, 0.48, 0.55, 1.0],
];

/// The stars of `level` as actors, `height` tall, around their middle, in
/// `colors` (gold, silver, the empty outline): a frame of `Star0` to
/// `Star4`, each with one part named `Gold`, `Silver` or `Empty`.
pub fn actor(level: u8, height: f32, colors: [Rgba; 3]) -> Actor {
    let left = -width(height) / 2.0;
    let children = fills(level)
        .into_iter()
        .zip(centers(left, 0.0, height))
        .enumerate()
        .map(|(i, (star, center))| {
            let parts = match star {
                // The strokes inside the star's box.
                Star::Empty => {
                    let thick = stroke(height);
                    outline_parts(&shape(center, height - thick), thick, colors[2])
                }
                Star::Silver => filled_parts(&shape(center, height), colors[1]),
                Star::Gold => filled_parts(&shape(center, height), colors[0]),
            };
            frame(&format!("Star{i}"), vec![frame(star.name(), parts)])
        })
        .collect();
    let mut a = frame("Stars", children);
    a.decoration = true;
    a
}

/// A quad cut (a hair of corner radius), so its edges are drawn smooth.
fn smooth(size: [f32; 2], color: Rgba, radius: f32) -> Actor {
    let mut q = quad("", size, color);
    call(&mut q, "cornerradius", &[radius]);
    q
}

fn filled_parts(s: &Shape, color: Rgba) -> Vec<Actor> {
    let mut parts: Vec<Actor> = s
        .rhombi
        .iter()
        .map(|r| {
            // A square on its corner, stretched to the diagonals and
            // turned to the tip.
            let mut square = smooth([1.0, 1.0], color, 0.02);
            call(&mut square, "rotationz", &[45.0]);
            let mut f = frame("", vec![square]);
            call(&mut f, "xy", &r.center);
            call(&mut f, "rotationz", &[r.angle.to_degrees()]);
            let k = std::f32::consts::SQRT_2;
            call(&mut f, "zoomx", &[r.diagonals[0] / k]);
            call(&mut f, "zoomy", &[r.diagonals[1] / k]);
            f
        })
        .collect();
    let (mid, radius) = s.disc;
    let mut disc = smooth([2.0 * radius, 2.0 * radius], color, radius);
    call(&mut disc, "xy", &mid);
    parts.push(disc);
    parts
}

fn outline_parts(s: &Shape, thick: f32, color: Rgba) -> Vec<Actor> {
    (0..10)
        .map(|i| {
            let (a, b) = (s.outline[i], s.outline[(i + 1) % 10]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let mut q = smooth([dx.hypot(dy) + thick, thick], color, thick / 2.0);
            call(&mut q, "rotationz", &[dy.atan2(dx).to_degrees()]);
            call(&mut q, "xy", &[(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]);
            q
        })
        .collect()
}

/// The stars of `level` in the look's colors, `height` tall, named
/// `name`, around their middle (the look's `Stars` component; a look
/// without one gets the plain colors).
pub fn stars(env: &Env, name: &str, level: u8, height: f32) -> Actor {
    let spec = Spec::new([width(height), height], level.to_string());
    let mut a = env
        .look
        .component("Stars", &spec)
        .unwrap_or_else(|| actor(level, height, COLORS));
    a.name = name.into();
    a
}

/// [`stars`] with their left edge at `x`, their middle on `y`.
pub fn stars_left(env: &Env, name: &str, level: u8, height: f32, x: f32, y: f32) -> Actor {
    let mut a = stars(env, name, level, height);
    look::call(&mut a, "xy", &[x + width(height) / 2.0, y]);
    a
}

/// [`stars`] with their right edge at `x`, their middle on `y`.
pub fn stars_right(env: &Env, name: &str, level: u8, height: f32, x: f32, y: f32) -> Actor {
    let mut a = stars(env, name, level, height);
    look::call(&mut a, "xy", &[x - width(height) / 2.0, y]);
    a
}

/// What `stars` (an actor [`stars`] made, or its parent) shows: each
/// star's fill, from the left (tests, and the debug state's check).
pub fn shown(stars: &Actor) -> Vec<Star> {
    let mut out = Vec::new();
    let mut walk = vec![stars];
    while let Some(a) = walk.pop() {
        if let ddi_actor::Kind::Frame(children) = &a.kind {
            if a.name.starts_with("Star") && a.name[4..].parse::<usize>().is_ok() {
                let i: usize = a.name[4..].parse().unwrap_or(0);
                let fill = children.first().map(|c| match c.name.as_str() {
                    "Gold" => Star::Gold,
                    "Silver" => Star::Silver,
                    _ => Star::Empty,
                });
                if let Some(fill) = fill {
                    out.push((i, fill));
                }
                continue;
            }
            walk.extend(children.iter());
        }
    }
    out.sort_by_key(|(i, _)| *i);
    out.into_iter().map(|(_, f)| f).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_level_s_five_stars() {
        use Star::{Empty as E, Gold as G, Silver as S};
        let all: Vec<[Star; 5]> = (0..=11).map(fills).collect();
        assert_eq!(
            all,
            [
                [E, E, E, E, E],
                [S, E, E, E, E],
                [S, S, E, E, E],
                [S, S, S, E, E],
                [S, S, S, S, E],
                [S, S, S, S, S],
                [G, S, S, S, S],
                [G, G, S, S, S],
                [G, G, G, S, S],
                [G, G, G, G, S],
                [G, G, G, G, G],
                [G, G, G, G, G],
            ]
        );
    }

    #[test]
    fn the_actor_shows_its_level_s_fills_inside_its_size() {
        for level in 1..=LEVELS {
            let a = actor(level, 12.0, COLORS);
            assert_eq!(shown(&a), fills(level), "level {level}");
        }
        // A filled star: five rhombi and the disc; an empty one ten
        // strokes.
        let a = actor(1, 12.0, COLORS);
        let ddi_actor::Kind::Frame(stars) = &a.kind else {
            panic!()
        };
        let parts = |i: usize| {
            let ddi_actor::Kind::Frame(star) = &stars[i].kind else {
                panic!()
            };
            let ddi_actor::Kind::Frame(parts) = &star[0].kind else {
                panic!()
            };
            parts.len()
        };
        assert_eq!((parts(0), parts(4)), (6, 10));
        // Never hit, never counted in bounds: an adornment.
        assert!(a.decoration);
    }

    #[test]
    fn a_star_s_shape_stays_in_its_box() {
        let h = 20.0;
        let s = shape([0.0, 0.0], h);
        for [x, y] in s.outline {
            assert!(x.abs() <= h * STAR_WIDTH / 2.0 + 0.01, "{x}");
            assert!(y.abs() <= h / 2.0 + 0.01, "{y}");
        }
        // A tip up, at the box's top; the lowest tips at its bottom.
        assert!((s.outline[0][1] + h / 2.0).abs() < 0.01);
        let low = s.outline.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
        assert!((low - h / 2.0).abs() < 0.05, "{low}");
        // Each rhombus from its tip to past the middle, its sides equal.
        for r in s.rhombi {
            let [a, b] = r.sides;
            assert!((a[0].hypot(a[1]) - b[0].hypot(b[1])).abs() < 1e-3);
            assert!(r.diagonals[0] > r.diagonals[1]);
        }
        // Five of them side by side.
        let c = centers(10.0, 0.0, h);
        assert!((c[0][0] - (10.0 + h * STAR_WIDTH / 2.0)).abs() < 1e-3);
        assert!((c[4][0] + h * STAR_WIDTH / 2.0 - (10.0 + width(h))).abs() < 1e-3);
        assert!(stroke(4.0) >= 1.0);
    }
}
