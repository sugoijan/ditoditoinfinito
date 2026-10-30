//! One active outline per UI input layer. Widget selections stay independent;
//! only the screen's primary target receives the animated decoration.
use ddi_actor::{Actor, Kind, Sprite, TRANSFORM_IDENTITY, Vivid, mul};
use ddi_ui::hit;
use ddi_ui::look::named;

use super::Infinito;

pub(super) const NAME: &str = "ActiveOutline";

fn clear(actor: &mut Actor) {
    if actor.name == NAME {
        actor.visible = false;
    }
    for child in actor.children_mut() {
        clear(child);
    }
}

struct Body {
    bounds: [f32; 4],
    radius: f32,
}

fn sprite_body(s: &Sprite, m: [f32; 6]) -> Body {
    let [w, h] = s.size;
    Body {
        bounds: hit::rect_bounds(&m, [-w / 2.0, -h / 2.0, w / 2.0, h / 2.0]),
        radius: s.shape.radius * m[0].hypot(m[1]).min(m[2].hypot(m[3])),
    }
}

/// Geometry in the control's parent space, including animated plates,
/// the wheel row's inset wrapper. Frame controls inherit their press and
/// transition transforms; visual-only decorations do not change their bounds.
fn find_body(actor: &Actor, parent: [f32; 6], named: bool) -> Option<Body> {
    for child in actor.children() {
        if !child.visible {
            continue;
        }
        let m = mul(&parent, &child.local_transform());
        if (!named || matches!(child.name.as_str(), "Plate" | "Box"))
            && !child.decoration
            && let Kind::Sprite(s) = &child.kind
        {
            return Some(sprite_body(s, m));
        }
        if let Some(b) = find_body(child, m, named) {
            return Some(b);
        }
    }
    None
}

fn body(actor: &Actor, parent: [f32; 6]) -> Option<Body> {
    find_body(actor, parent, true).or_else(|| find_body(actor, parent, false))
}

pub(super) fn update(
    root: &mut Actor,
    target: Option<&str>,
    seconds: f64,
    theme: &Infinito,
) -> bool {
    clear(root);
    let Some(name) = target else { return false };
    let Some(mut path) = hit::find(root, name) else {
        return false;
    };
    if path.is_empty() || !hit::shown(root, &path) {
        return false;
    }
    let Some(control) = hit::actor_at(root, &path) else {
        return false;
    };
    let geometry = match &control.kind {
        Kind::Frame(_) => body(control, TRANSFORM_IDENTITY),
        Kind::Sprite(s) => {
            let geometry = sprite_body(s, control.local_transform());
            path.pop();
            Some(geometry)
        }
        _ => None,
    };
    let Some(Body {
        bounds: [x0, y0, x1, y1],
        radius,
    }) = geometry
    else {
        return false;
    };
    // Frame controls own their decoration. In particular, never append an
    // extra sibling to the list's slot array. Primitive page hit areas use
    // their manually-built parent instead.
    let Some(parent) = hit::actor_at_mut(root, &path) else {
        return false;
    };
    let Kind::Frame(children) = &mut parent.kind else {
        return false;
    };
    let width = theme.m("Active.Width");
    let out = width + theme.m("Active.Gap");
    let size = [x1 - x0 + out * 2.0, y1 - y0 + out * 2.0];
    let index = children
        .iter()
        .position(|a| a.name == NAME)
        .unwrap_or_else(|| {
            let mut a = named(Kind::Sprite(Sprite::quad(size)), NAME);
            a.decoration = true;
            children.push(a);
            children.len() - 1
        });
    let outline = &mut children[index];
    outline.visible = true;
    outline.set_position([(x0 + x1) / 2.0, (y0 + y1) / 2.0, 0.0]);
    if let Kind::Sprite(s) = &mut outline.kind {
        s.size = size;
        s.shape.radius = radius + out;
        s.shape.outline = width;
        s.shape.vivid = Some(Vivid {
            time: seconds as f32,
            phase_offset: 0.0,
            evolution_speed: theme.m("Active.EvolutionSpeed"),
            screen_space: true,
            rect: [-64.0, -64.0, 64.0, 64.0],
        });
    }
    true
}
