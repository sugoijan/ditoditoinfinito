//! Finding actors under a point, and where actors are on the screen.
//!
//! An actor is placed as [`ddi_actor::Actor::draw`] places it
//! ([`ddi_actor::Actor::local_transform`]: the current tween state, no
//! effects, no `fov` camera). A point hits an actor with a box (a sprite,
//! a quad, a run of text) when, taken back through the actor's transform,
//! it lies in the box; and a clipping frame's rectangle, so a list's empty
//! space is a target too. What draw skips is never hit: invisible and
//! hibernating actors, and those wholly transparent with no glow (their
//! alpha times what their frames hand down, as `Actor::draw`), with all
//! they hold; nor anything outside the clips around it. A hit area must
//! be drawn, however faintly. The
//! topmost hit is the last drawn: children in draw order (by z under
//! `SetDrawByZPosition`), later siblings over earlier ones.
//!
//! Named actors are what input goes to: a hit's [`Target`] is the
//! nearest named actor at or above the box hit, the screen's root aside.
//! Boxes with no named actor above them are decoration and let the point
//! through to what is under them.
//!
//! Actors are addressed by path (child indices from the root), valid
//! until the tree changes, or by name ([`find`], depth first).

use ddi_actor::{Actor, Kind};

/// A 2D affine transform, `x' = m[0]x + m[2]y + m[4]`,
/// `y' = m[1]x + m[3]y + m[5]` (the actor crate's).
pub type Affine = [f32; 6];

/// The identity, and `parent · local` (`local` first): the actor crate's.
pub use ddi_actor::{TRANSFORM_IDENTITY as IDENTITY, mul};

pub fn apply(m: &Affine, p: [f32; 2]) -> [f32; 2] {
    [
        m[0] * p[0] + m[2] * p[1] + m[4],
        m[1] * p[0] + m[3] * p[1] + m[5],
    ]
}

/// The inverse; `None` when the transform squashes the plane (a zoom of
/// 0, a quarter turn about x or y).
pub fn invert(m: &Affine) -> Option<Affine> {
    let det = m[0] * m[3] - m[1] * m[2];
    if !det.is_finite() || det.abs() < 1e-9 {
        return None;
    }
    let (a, b, c, d) = (m[3] / det, -m[1] / det, -m[2] / det, m[0] / det);
    Some([a, b, c, d, -(a * m[4] + c * m[5]), -(b * m[4] + d * m[5])])
}

/// An actor a point is over.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    /// Child indices from the root to the actor hit.
    pub path: Vec<usize>,
    /// The point in the box, from its top left, in the actor's units.
    pub local: [f32; 2],
}

/// Where input goes: a named actor, and the named actors around it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// Child indices from the root to the named actor.
    pub path: Vec<usize>,
    pub name: String,
    /// Its name, then every named ancestor's below the root, nearest
    /// first: a row's target knows it is in the list.
    pub chain: Vec<String>,
}

impl Target {
    /// Whether the target is the actor named `name` or inside it.
    pub fn within(&self, name: &str) -> bool {
        self.chain.iter().any(|n| n == name)
    }
}

/// Every box under `point` (in the space `transform` maps the root's
/// parent space to; the root's own placement applies), topmost first.
pub fn hits(root: &Actor, transform: &Affine, point: [f32; 2]) -> Vec<Hit> {
    let mut out = Vec::new();
    let mut path = Vec::new();
    walk(root, transform, [1.0, 0.0], point, &mut path, &mut out, 0);
    out.reverse();
    out
}

/// The topmost target under `point` ([`hits`] resolved to named actors).
pub fn target_at(root: &Actor, transform: &Affine, point: [f32; 2]) -> Option<Target> {
    hits(root, transform, point)
        .iter()
        .find_map(|h| target_of(root, &h.path))
}

/// The nearest named actor at or above `path`, the root aside.
pub fn target_of(root: &Actor, path: &[usize]) -> Option<Target> {
    let mut chain = Vec::new();
    let mut found: Option<(usize, String)> = None;
    for depth in (1..=path.len()).rev() {
        let a = actor_at(root, &path[..depth])?;
        if !a.name.is_empty() {
            if found.is_none() {
                found = Some((depth, a.name.clone()));
            }
            chain.push(a.name.clone());
        }
    }
    let (depth, name) = found?;
    Some(Target {
        path: path[..depth].to_vec(),
        name,
        chain,
    })
}

const MAX_DEPTH: usize = 40;

/// `under` is the alpha and the glow's alpha a frame hands its children.
fn walk(
    a: &Actor,
    parent: &Affine,
    under: [f32; 2],
    point: [f32; 2],
    path: &mut Vec<usize>,
    out: &mut Vec<Hit>,
    depth: usize,
) {
    if !a.visible || a.hibernating() || depth > MAX_DEPTH {
        return;
    }
    // `Actor::draw`'s `PartiallyOpaque`: the corners' alphas times what
    // is handed down; the glow screened.
    let s = a.current();
    let k = under[0] * a.base_alpha;
    let alpha = s.diffuse.map(|c| c[3] * k);
    let g = s.glow[3];
    let glow = if under[1] > 0.0 {
        g + under[1] - g * under[1]
    } else {
        g
    };
    if !(alpha.iter().any(|&c| c > 0.0) || glow > 0.0) {
        return;
    }
    let m = mul(parent, &a.local_transform());
    let Some(inv) = invert(&m) else { return };
    let p = apply(&inv, point);
    let size = a.size();
    if matches!(a.kind, Kind::Sprite(_) | Kind::Text(_))
        && size[0] > 0.0
        && size[1] > 0.0
        && p[0].abs() <= size[0] / 2.0
        && p[1].abs() <= size[1] / 2.0
    {
        out.push(Hit {
            path: path.clone(),
            local: [p[0] + size[0] / 2.0, p[1] + size[1] / 2.0],
        });
    }
    let Kind::Frame(children) = &a.kind else {
        return;
    };
    if let Some(r) = a.clip {
        if !(p[0] >= r[0] && p[0] <= r[2] && p[1] >= r[1] && p[1] <= r[3]) {
            // Nothing inside can be hit here.
            return;
        }
        out.push(Hit {
            path: path.clone(),
            local: [p[0] - r[0], p[1] - r[1]],
        });
    }
    for i in draw_order(a, children) {
        path.push(i);
        walk(
            &children[i],
            &m,
            [alpha[0], glow],
            point,
            path,
            out,
            depth + 1,
        );
        path.pop();
    }
}

/// The children's indices in the order they are drawn.
fn draw_order(frame: &Actor, children: &[Actor]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..children.len()).collect();
    if frame.draw_by_z {
        order.sort_by(|&a, &b| {
            let (za, zb) = (children[a].current().pos[2], children[b].current().pos[2]);
            za.partial_cmp(&zb).unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    order
}

pub fn actor_at<'a>(root: &'a Actor, path: &[usize]) -> Option<&'a Actor> {
    let mut a = root;
    for &i in path {
        a = a.children().get(i)?;
    }
    Some(a)
}

pub fn actor_at_mut<'a>(root: &'a mut Actor, path: &[usize]) -> Option<&'a mut Actor> {
    let mut a = root;
    for &i in path {
        a = a.children_mut().get_mut(i)?;
    }
    Some(a)
}

/// The path of the first actor named `name`, depth first (the root
/// included).
pub fn find(root: &Actor, name: &str) -> Option<Vec<usize>> {
    fn go(a: &Actor, name: &str, path: &mut Vec<usize>, depth: usize) -> bool {
        if a.name == name {
            return true;
        }
        if depth > MAX_DEPTH {
            return false;
        }
        for (i, c) in a.children().iter().enumerate() {
            path.push(i);
            if go(c, name, path, depth + 1) {
                return true;
            }
            path.pop();
        }
        false
    }
    let mut path = Vec::new();
    go(root, name, &mut path, 0).then_some(path)
}

/// The first actor named `name`.
pub fn find_mut<'a>(root: &'a mut Actor, name: &str) -> Option<&'a mut Actor> {
    let path = find(root, name)?;
    actor_at_mut(root, &path)
}

/// The transform from the actor at `path`'s own space to the space
/// `transform` maps the root's parent space to.
pub fn transform_of(root: &Actor, transform: &Affine, path: &[usize]) -> Option<Affine> {
    let mut m = mul(transform, &root.local_transform());
    let mut a = root;
    for &i in path {
        a = a.children().get(i)?;
        m = mul(&m, &a.local_transform());
    }
    Some(m)
}

/// `point` (in the space `transform` maps the root's parent space to) in
/// the own space of the first actor named `name`: where a press is along
/// a slider's track or down a list, wherever they are placed.
pub fn local_point(
    root: &Actor,
    transform: &Affine,
    name: &str,
    point: [f32; 2],
) -> Option<[f32; 2]> {
    let path = find(root, name)?;
    let m = transform_of(root, transform, &path)?;
    Some(apply(&invert(&m)?, point))
}

/// What of the actor at `path` (its box, or its children's) is on view:
/// cut by the clipping frames around it; `None` when it is hidden or
/// clipped away.
pub fn visible_rect(root: &Actor, transform: &Affine, path: &[usize]) -> Option<[f32; 4]> {
    if !shown(root, path) {
        return None;
    }
    let mut r = bounds_deep(root, transform, path)?;
    for d in 0..path.len() {
        if actor_at(root, &path[..d]).is_some_and(|a| a.clip.is_some())
            && let Some(c) = bounds(root, transform, &path[..d])
        {
            r = [
                r[0].max(c[0]),
                r[1].max(c[1]),
                r[2].min(c[2]),
                r[3].min(c[3]),
            ];
        }
    }
    (r[2] > r[0] && r[3] > r[1]).then_some(r)
}

/// Whether the actor at `path` and everything above it is shown.
pub fn shown(root: &Actor, path: &[usize]) -> bool {
    let mut a = root;
    if !a.visible || a.hibernating() {
        return false;
    }
    for &i in path {
        match a.children().get(i) {
            Some(c) if c.visible && !c.hibernating() => a = c,
            _ => return false,
        }
    }
    true
}

/// The rectangle an actor's box covers (a clipping frame's: its clip),
/// `[left, top, right, bottom]` around its corners; `None` for an actor
/// with no box.
pub fn bounds(root: &Actor, transform: &Affine, path: &[usize]) -> Option<[f32; 4]> {
    let a = actor_at(root, path)?;
    let m = transform_of(root, transform, path)?;
    let rect = match (a.clip, a.size()) {
        (Some(r), _) => r,
        (None, [w, h]) if w > 0.0 && h > 0.0 => [-w / 2.0, -h / 2.0, w / 2.0, h / 2.0],
        _ => return None,
    };
    let corners = [
        [rect[0], rect[1]],
        [rect[2], rect[1]],
        [rect[0], rect[3]],
        [rect[2], rect[3]],
    ]
    .map(|p| apply(&m, p));
    let mut b = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for c in corners {
        b = [
            b[0].min(c[0]),
            b[1].min(c[1]),
            b[2].max(c[0]),
            b[3].max(c[1]),
        ];
    }
    Some(b)
}

/// The rectangle a named actor covers, or the union of what its
/// children cover when it has no box of its own (a frame holding a
/// button's art and label).
pub fn bounds_deep(root: &Actor, transform: &Affine, path: &[usize]) -> Option<[f32; 4]> {
    if let Some(b) = bounds(root, transform, path) {
        return Some(b);
    }
    let a = actor_at(root, path)?;
    let mut out: Option<[f32; 4]> = None;
    let mut child = path.to_vec();
    for i in 0..a.children().len() {
        child.push(i);
        if shown(root, &child)
            && let Some(b) = bounds_deep(root, transform, &child)
        {
            out = Some(match out {
                None => b,
                Some(o) => [
                    o[0].min(b[0]),
                    o[1].min(b[1]),
                    o[2].max(b[2]),
                    o[3].max(b[3]),
                ],
            });
        }
        child.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_actor::Sprite;
    use ddi_actor::commands::parse;

    fn quad(name: &str, size: [f32; 2], commands: &str) -> Actor {
        let mut a = Actor::new(Kind::Sprite(Sprite::quad(size)));
        a.name = name.into();
        run(&mut a, commands);
        a
    }

    fn run(a: &mut Actor, commands: &str) {
        let mut p = ddi_actor::Problems::default();
        if !commands.is_empty() {
            for c in parse(commands).unwrap() {
                a.call(&c, &mut p, 0).unwrap();
            }
        }
    }

    fn frame(name: &str, children: Vec<Actor>, commands: &str) -> Actor {
        let mut a = Actor::new(Kind::Frame(children));
        a.name = name.into();
        run(&mut a, commands);
        a
    }

    #[test]
    fn inverse_undoes() {
        let m = [2.0, 0.5, -0.25, 3.0, 10.0, -4.0];
        let inv = invert(&m).unwrap();
        let p = apply(&inv, apply(&m, [3.0, 7.0]));
        assert!((p[0] - 3.0).abs() < 1e-4 && (p[1] - 7.0).abs() < 1e-4);
        assert!(invert(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0]).is_none());
    }

    #[test]
    fn hits_follow_transforms_topmost_first() {
        // A 100×50 button at (200, 100) turned 90°: it covers 175–225 by
        // 50–150. A smaller label over its centre.
        let root = frame(
            "",
            vec![
                frame(
                    "Button",
                    vec![quad("", [100.0, 50.0], ""), quad("Label", [20.0, 10.0], "")],
                    "xy,200,100;rotationz,90",
                ),
                quad("Far", [10.0, 10.0], "xy,400,400"),
            ],
            "",
        );
        let hs = hits(&root, &IDENTITY, [200.0, 100.0]);
        assert_eq!(hs.len(), 2);
        assert_eq!(hs[0].path, vec![0, 1]);
        let t = target_at(&root, &IDENTITY, [200.0, 100.0]).unwrap();
        assert_eq!(t.name, "Label");
        assert_eq!(t.chain, vec!["Label".to_string(), "Button".to_string()]);
        assert!(t.within("Button"));
        // Turned: (220, 60) is inside, (240, 100) is not.
        let t = target_at(&root, &IDENTITY, [220.0, 60.0]).unwrap();
        assert_eq!((t.name.as_str(), t.path.as_slice()), ("Button", &[0][..]));
        let h = &hits(&root, &IDENTITY, [220.0, 60.0])[0];
        // Local: x along the quad's width, which now runs down the screen.
        assert!((h.local[0] - 10.0).abs() < 1e-3 && (h.local[1] - 5.0).abs() < 1e-3);
        assert!(target_at(&root, &IDENTITY, [240.0, 100.0]).is_none());
        // Bounds of the turned button.
        let b = bounds_deep(&root, &IDENTITY, &[0]).unwrap();
        assert!((b[0] - 175.0).abs() < 1e-3 && (b[3] - 150.0).abs() < 1e-3);
    }

    #[test]
    fn hidden_and_clipped_actors_are_not_hit() {
        let mut list = frame(
            "List",
            vec![quad("Row", [100.0, 20.0], "xy,0,25")],
            "xy,100,100",
        );
        list.clip = Some([-50.0, -30.0, 50.0, 30.0]);
        let root = frame(
            "",
            vec![
                list,
                quad("Hidden", [500.0, 500.0], "visible,0"),
                // Transparent, or inside a transparent frame: not drawn,
                // not hit.
                quad("Clear", [500.0, 500.0], "diffusealpha,0"),
                frame(
                    "Faded",
                    vec![quad("Inner", [500.0, 500.0], "")],
                    "diffusealpha,0",
                ),
            ],
            "",
        );
        // The row (115–135 down the screen) is cut at the clip's 130:
        // (100, 125) shows it, (100, 133) does not, and the list's own clip
        // is a target between rows.
        assert_eq!(
            target_at(&root, &IDENTITY, [100.0, 125.0]).unwrap().name,
            "Row"
        );
        assert!(target_at(&root, &IDENTITY, [100.0, 133.0]).is_none());
        assert_eq!(
            target_at(&root, &IDENTITY, [100.0, 100.0]).unwrap().name,
            "List"
        );
        assert_eq!(
            bounds(&root, &IDENTITY, &[0]),
            Some([50.0, 70.0, 150.0, 130.0])
        );
    }

    #[test]
    fn decoration_lets_the_point_through() {
        let root = frame(
            "",
            vec![
                quad("Button", [50.0, 50.0], "xy,100,100"),
                // An unnamed glow over it.
                quad("", [80.0, 80.0], "xy,100,100;diffusealpha,0.5"),
            ],
            "",
        );
        assert_eq!(hits(&root, &IDENTITY, [100.0, 100.0]).len(), 2);
        assert_eq!(
            target_at(&root, &IDENTITY, [100.0, 100.0]).unwrap().name,
            "Button"
        );
    }

    #[test]
    fn draw_by_z_puts_the_nearest_on_top() {
        let mut root = frame(
            "",
            vec![
                quad("Near", [50.0, 50.0], "z,10"),
                quad("Far", [50.0, 50.0], "z,-10"),
            ],
            "",
        );
        assert_eq!(target_at(&root, &IDENTITY, [0.0, 0.0]).unwrap().name, "Far");
        root.draw_by_z = true;
        assert_eq!(
            target_at(&root, &IDENTITY, [0.0, 0.0]).unwrap().name,
            "Near"
        );
    }

    #[test]
    fn finds_by_name() {
        let mut root = frame(
            "",
            vec![frame("A", vec![quad("B", [1.0, 1.0], "")], "")],
            "",
        );
        assert_eq!(find(&root, "B"), Some(vec![0, 0]));
        assert_eq!(find(&root, "C"), None);
        find_mut(&mut root, "B").unwrap().visible = false;
        assert!(!shown(&root, &[0, 0]));
        assert!(shown(&root, &[0]));
    }
}
