//! Who attacks whom, as the play shows it for the local player's own
//! attacks (`docs/plans/multiplayer-prototype-progress.md`, M8 round 3,
//! S5; `docs/plans/multiplayer-ui.md`, "Target connection and electrical
//! attack feedback"). CPU against CPU stays what it is: shocks in their
//! fields and the small lock look.
//!
//! - **Outgoing** (the player's full bar committed an attack): a line in
//!   the side space from the field's power bar's end to the recipient's
//!   miniature, at its own power bar; a pulse runs along it; the
//!   recipient's miniature is outlined while the attack is on its way
//!   and passing; the line fades.
//! - **Incoming**: the same the other way, from the sender's miniature to
//!   the warning by the receptors (`crate::battle_cue`), the sender's
//!   miniature outlined. The warning stands at the field's right edge: a
//!   sender on the left ends at the field's left edge on the receptors'
//!   row instead, never across the field.
//! - **Chained** (one waiting behind the one passing): its line and its
//!   outline quieter.
//!
//! The line keeps to the side space's own gaps, in straight legs: the
//! shortest way with the fewest corners ([`route`]) that stays out of
//! the field's band, the HUD, the judgment, the quit button's place, the
//! status line, the warning's place (unless it ends there) and every
//! miniature, typically along the top at the power bar's height and
//! down the gap beside the miniature's column, into it at its bar. Under Reverse the bars stay where they are and the
//! line with them; the warning's end follows the receptors.
//!
//! Reduced motion: no pulse and no fade; the line shown still for its
//! time, then gone; the outline as ever.

use ddi_render::FieldGeometry;
use ddi_render::draw2d::{Blend, Command, DrawList, Quad, Shape};
use ddi_session::battle::Link;

use crate::opponents::{Fields, Slot};

/// How a link looks over time. PROVISIONAL.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub id: &'static str,
    /// Seconds the pulse takes along the line.
    pub pulse: f32,
    /// Seconds the line shows from the commitment: whole while the pulse
    /// runs, fading after (whole throughout under reduced motion).
    pub line: f32,
    /// Seconds the outline takes to come in.
    pub outline_in: f32,
    /// How much quieter a chained attack's line and outline are.
    pub quiet: f32,
}

/// `attack-links-v0` (PROVISIONAL): a pulse of 0.6 s, the line gone after
/// 1.5 s, a chained attack's parts at 0.45.
pub const ATTACK_LINKS_V0: Look = Look {
    id: "attack-links-v0",
    pulse: 0.6,
    line: 1.5,
    outline_in: 0.2,
    quiet: 0.45,
};

/// Where a link is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// The pulse runs along the line.
    Pulse,
    /// The line fades.
    Fade,
    /// Reduced motion: the line stands still for its time.
    Still,
    /// The line is gone; the miniature stays outlined until the attack
    /// has passed.
    Outline,
}

impl Phase {
    pub fn id(self) -> &'static str {
        match self {
            Phase::Pulse => "pulse",
            Phase::Fade => "fade",
            Phase::Still => "still",
            Phase::Outline => "outline",
        }
    }
}

/// A link as a frame draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub attack: u32,
    /// The opponent's seat and the slot its miniature shows in.
    pub seat: usize,
    pub slot: usize,
    /// From the player to the opponent (else from the opponent to the
    /// player).
    pub outgoing: bool,
    pub chained: bool,
    pub phase: Phase,
    /// The line's opacity, `0..=1` (0 once gone).
    pub line: f32,
    /// The pulse's place along the line, `0..=1`, while it runs.
    pub pulse: Option<f32>,
    /// The outline's opacity, `0..=1`.
    pub outline: f32,
    /// The line's corners, target pixels, from where the attack comes to
    /// where it goes (empty where there is no clear way: the outline
    /// alone says it).
    pub path: Vec<[f32; 2]>,
    /// A watched stage's grid ([`shown_between`]): the sender's seat (the
    /// link is then between two competitors, `seat` its recipient).
    pub from: Option<usize>,
}

/// What a link's line needs of the play's layout: the field `geo`
/// (`span` lanes wide), the miniatures' `fields`, a logical pixel
/// `ratio` and what the line keeps clear of besides the band and the
/// miniatures (`[left, top, right, bottom]`: the HUD's areas and the
/// quit button's place; the warning's own place is where an incoming
/// line ends).
#[derive(Clone, Copy)]
pub struct Layout<'a> {
    pub geo: &'a FieldGeometry,
    pub span: f32,
    pub fields: &'a Fields,
    pub ratio: f32,
    pub keep_out: &'a [[f32; 4]],
}

/// A way from `start` to `end` in straight legs (never a diagonal) that
/// stays out of every box of `blocked` (`[left, top, right, bottom]`,
/// their edges included) and within `bounds`: the shortest, a corner
/// counting as `corner` more pixels, so the fewest corners among equals.
/// Its corners in order, both ends included; none where there is no way.
fn route(
    start: [f32; 2],
    end: [f32; 2],
    blocked: &[[f32; 4]],
    bounds: [f32; 4],
    corner: f32,
) -> Option<Vec<[f32; 2]>> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    // The lines a leg may run along: the ends' own, and just off each
    // box's edges.
    let off = 0.5;
    let mut xs = vec![start[0], end[0]];
    let mut ys = vec![start[1], end[1]];
    for b in blocked {
        xs.extend([b[0] - off, b[2] + off]);
        ys.extend([b[1] - off, b[3] + off]);
    }
    let tidy = |v: &mut Vec<f32>, lo: f32, hi: f32| {
        v.retain(|x| *x >= lo && *x <= hi);
        v.sort_by(f32::total_cmp);
        v.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    };
    tidy(&mut xs, bounds[0], bounds[2]);
    tidy(&mut ys, bounds[1], bounds[3]);
    let (nx, ny) = (xs.len(), ys.len());
    let index = |v: &[f32], x: f32| v.iter().position(|a| (*a - x).abs() < 0.01);
    let (sx, sy) = (index(&xs, start[0])?, index(&ys, start[1])?);
    let (ex, ey) = (index(&xs, end[0])?, index(&ys, end[1])?);
    // A leg between two neighbors on a line is free where it is in no
    // box.
    let free = |p: [f32; 2], q: [f32; 2]| {
        let seg = [p[0].min(q[0]), p[1].min(q[1]), p[0].max(q[0]), p[1].max(q[1])];
        !blocked
            .iter()
            .any(|b| seg[0] <= b[2] && seg[2] >= b[0] && seg[1] <= b[3] && seg[3] >= b[1])
    };
    // Dijkstra over (place, the way it was entered: across or down).
    let state = |x: usize, y: usize, d: usize| (y * nx + x) * 2 + d;
    let mut best = vec![f32::INFINITY; nx * ny * 2];
    let mut from: Vec<Option<usize>> = vec![None; nx * ny * 2];
    let mut heap = BinaryHeap::new();
    for d in 0..2 {
        best[state(sx, sy, d)] = 0.0;
        heap.push((Reverse(0u64), state(sx, sy, d)));
    }
    let key = |c: f32| (c * 64.0) as u64;
    let mut reached = None;
    while let Some((Reverse(k), at)) = heap.pop() {
        if k > key(best[at]) {
            continue;
        }
        let (place, d) = (at / 2, at % 2);
        let (x, y) = (place % nx, place / nx);
        if (x, y) == (ex, ey) {
            reached = Some(at);
            break;
        }
        let here = [xs[x], ys[y]];
        let mut go = |tx: usize, ty: usize, td: usize| {
            let there = [xs[tx], ys[ty]];
            if !free(here, there) {
                return;
            }
            let len = (there[0] - here[0]).abs() + (there[1] - here[1]).abs();
            let turn = if td != d && (x, y) != (sx, sy) { corner } else { 0.0 };
            let cost = best[at] + len + turn;
            let to = state(tx, ty, td);
            if cost < best[to] {
                best[to] = cost;
                from[to] = Some(at);
                heap.push((Reverse(key(cost)), to));
            }
        };
        if x > 0 {
            go(x - 1, y, 0);
        }
        if x + 1 < nx {
            go(x + 1, y, 0);
        }
        if y > 0 {
            go(x, y - 1, 1);
        }
        if y + 1 < ny {
            go(x, y + 1, 1);
        }
    }
    // Back along it, keeping the corners.
    let mut at = reached?;
    let mut pts = vec![[xs[(at / 2) % nx], ys[(at / 2) / nx]]];
    while let Some(prev) = from[at] {
        at = prev;
        pts.push([xs[(at / 2) % nx], ys[(at / 2) / nx]]);
    }
    pts.reverse();
    let mut out: Vec<[f32; 2]> = Vec::new();
    for p in pts {
        if let [.., a, b] = out[..]
            && (((a[0] - b[0]).abs() < 0.01 && (b[0] - p[0]).abs() < 0.01)
                || ((a[1] - b[1]).abs() < 0.01 && (b[1] - p[1]).abs() < 0.01))
        {
            out.pop();
        }
        out.push(p);
    }
    Some(out)
}

impl Layout<'_> {
    /// The band's left and right edges.
    fn band(&self) -> [f32; 2] {
        let [mid, band] = ddi_render::scene::filter_band(self.geo, self.span);
        [mid - band / 2.0, mid + band / 2.0]
    }

    /// What a line stays out of, each grown by `by`: the field's band
    /// (the HUD's bars and the judgment are in it), every miniature
    /// (its name line and its field; not in a large field's grid, whose
    /// gaps leave no way between them: there a line crosses the fields
    /// on its way, `crate::opponents::Fields::dense`), the keep-outs, the
    /// status line and, for a line that does not end there, the warning's
    /// place.
    fn blocked(&self, by: f32, warning: bool) -> Vec<[f32; 4]> {
        let [band_l, band_r] = self.band();
        let mut out = vec![[band_l, 0.0, band_r, self.geo.height]];
        let slots = if self.fields.dense {
            &[][..]
        } else {
            &self.fields.slots[..]
        };
        out.extend(slots.iter().map(|s| {
            [
                s.rect[0].min(s.panel[0]),
                s.rect[1].min(s.panel[1]),
                (s.rect[0] + s.rect[2]).max(s.panel[2]),
                (s.rect[1] + s.rect[3]).max(s.panel[3]),
            ]
        }));
        out.extend(self.keep_out.iter().copied());
        let [x, y, w, h] = self.fields.status;
        if w > 0.0 && h > 0.0 {
            out.push([x, y, x + w, y + h]);
        }
        if warning {
            out.push(crate::battle_cue::place(self.geo, self.span));
        }
        out.iter()
            .map(|b| [b[0] - by, b[1] - by, b[2] + by, b[3] + by])
            .collect()
    }

    /// The line of an attack between the player and the miniature in
    /// `slot`: from the field's power bar's end to the miniature's own
    /// bar (`outgoing`), or from the miniature to the warning by the
    /// receptors (a miniature on the right; one on the left ends at the
    /// field's left edge on the receptors' row); its corners in order.
    /// Empty where there is no clear way.
    pub fn path(&self, slot: &Slot, outgoing: bool) -> Vec<[f32; 2]> {
        let r = self.ratio;
        // Every box grown so that a line keeps this far from it, and a
        // line's end steps this far out of the box it touches.
        let grow = 1.5 * r;
        let step = grow + 0.5;
        let [band_l, band_r] = self.band();
        let (side, out) = if slot.left { (band_l, -1.0) } else { (band_r, 1.0) };
        let (power, _) = ddi_render::scene::power_and_life(self.geo, self.span);
        let (mini, _) =
            ddi_render::scene::miniature_power_and_life(&slot.field, slot.panel, self.span);
        let y_m = mini[1] + mini[3] / 2.0;
        // At the miniature: its field-facing edge at its power bar, and
        // the place just off it.
        let edge = if slot.left { slot.panel[2] } else { slot.panel[0] };
        let (at_mini, off_mini) = ([edge, y_m], [edge - out * step, y_m]);
        let bounds = [r, r, self.geo.width - r, self.geo.height - r];
        let corner = 24.0 * r;
        let cue = crate::battle_cue::place(self.geo, self.span);
        let ways: Vec<Vec<[f32; 2]>> = if outgoing {
            // From the band's edge at the power bar.
            let y = power[1] + power[3] / 2.0;
            let (at_bar, off_bar) = ([side, y], [side + out * step, y]);
            route(off_bar, off_mini, &self.blocked(grow, true), bounds, corner)
                .map(|mut way| {
                    way.insert(0, at_bar);
                    way.push(at_mini);
                    way
                })
                .into_iter()
                .collect()
        } else if slot.left {
            // To the field's left edge on the receptors' row.
            let y = self.geo.receptor_y;
            route(off_mini, [side - step, y], &self.blocked(grow, true), bounds, corner)
                .map(|mut way| {
                    way.insert(0, at_mini);
                    way.push([side, y]);
                    way
                })
                .into_iter()
                .collect()
        } else {
            // To the warning: onto its top, its bottom or its outer
            // side, whichever is nearest.
            let (cx, cy) = ((cue[0] + cue[2]) / 2.0, (cue[1] + cue[3]) / 2.0);
            let blocked = self.blocked(grow, true);
            [
                ([cx, cue[1]], [cx, cue[1] - step]),
                ([cx, cue[3]], [cx, cue[3] + step]),
                ([cue[2], cy], [cue[2] + step, cy]),
            ]
            .into_iter()
            .filter_map(|(on, off)| {
                let mut way = route(off_mini, off, &blocked, bounds, corner)?;
                way.insert(0, at_mini);
                way.push(on);
                Some(way)
            })
            .collect()
        };
        let cost = |way: &Vec<[f32; 2]>| {
            way.windows(2)
                .map(|w| (w[1][0] - w[0][0]).abs() + (w[1][1] - w[0][1]).abs())
                .sum::<f32>()
                + corner * way.len() as f32
        };
        let mut pts = ways
            .into_iter()
            .min_by(|a, b| cost(a).total_cmp(&cost(b)))
            .unwrap_or_default();
        pts.dedup_by(|a, b| (a[0] - b[0]).abs() < 0.25 && (a[1] - b[1]).abs() < 0.25);
        // A stub and the leg after it in one line are one leg.
        let mut out: Vec<[f32; 2]> = Vec::new();
        for p in pts {
            if let [.., a, b] = out[..]
                && (((a[0] - b[0]).abs() < 0.01 && (b[0] - p[0]).abs() < 0.01)
                    || ((a[1] - b[1]).abs() < 0.01 && (b[1] - p[1]).abs() < 0.01))
            {
                out.pop();
            }
            out.push(p);
        }
        if out.len() < 2 {
            return Vec::new();
        }
        out
    }
}

/// The local player's `links` as the frame at judged song second `t`
/// shows them over `layout`, under `look`; those whose opponent's
/// miniature is not in view (taking turns beyond the cap, before
/// [`crate::opponents::priority_seats`] brought it in) are left out.
pub fn shown(links: &[Link], layout: &Layout, t: f64, reduced: bool, look: Look) -> Vec<Shown> {
    links
        .iter()
        .filter_map(|l| {
            let slot = layout.fields.slots.iter().position(|s| s.seat == l.seat)?;
            let (phase, line, pulse, outline) = timing(l, t, reduced, look);
            let path = if line > 0.0 {
                layout.path(&layout.fields.slots[slot], l.outgoing)
            } else {
                Vec::new()
            };
            Some(Shown {
                attack: l.attack,
                seat: l.seat,
                slot,
                outgoing: l.outgoing,
                chained: l.chained,
                phase,
                line,
                pulse,
                outline,
                path,
                from: None,
            })
        })
        .collect()
}

/// What a line between two fields of a watched stage's grid needs
/// (`crate::opponents::arrange_grid`): the fields, the screen's `size`
/// (target pixels), the lanes' `span`, a logical pixel `ratio` and what
/// the line keeps clear of besides the fields (the quit button's place).
#[derive(Clone, Copy)]
pub struct GridLayout<'a> {
    pub fields: &'a Fields,
    pub size: [f32; 2],
    pub span: f32,
    pub ratio: f32,
    pub keep_out: &'a [[f32; 4]],
}

impl GridLayout<'_> {
    /// The line of an attack from the field in `from` to the field in
    /// `to`: from the sender's field at its power bar (the side that
    /// gives the shorter way) to the warning beside the recipient's
    /// field, in straight legs along the gaps between the fields. Empty
    /// where there is no clear way.
    pub fn path(&self, from: &Slot, to: &Slot) -> Vec<[f32; 2]> {
        let r = self.ratio;
        let grow = 1.5 * r;
        let step = grow + 0.5;
        let mut blocked: Vec<[f32; 4]> = self
            .fields
            .slots
            .iter()
            .map(|s| {
                [
                    s.rect[0].min(s.panel[0]),
                    s.rect[1].min(s.panel[1]),
                    (s.rect[0] + s.rect[2]).max(s.panel[2]),
                    (s.rect[1] + s.rect[3]).max(s.panel[3]),
                ]
            })
            .collect();
        blocked.extend(self.keep_out.iter().copied());
        let [x, y, w, h] = self.fields.status;
        if w > 0.0 && h > 0.0 {
            blocked.push([x, y, x + w, y + h]);
        }
        let blocked: Vec<[f32; 4]> = blocked
            .iter()
            .map(|b| [b[0] - grow, b[1] - grow, b[2] + grow, b[3] + grow])
            .collect();
        let (mini, _) =
            ddi_render::scene::miniature_power_and_life(&from.field, from.panel, self.span);
        let y_m = mini[1] + mini[3] / 2.0;
        let starts = [
            ([from.panel[0], y_m], [from.panel[0] - step, y_m]),
            ([from.panel[2], y_m], [from.panel[2] + step, y_m]),
        ];
        let cue = crate::battle_cue::place(&to.field, self.span);
        let (cx, cy) = ((cue[0] + cue[2]) / 2.0, (cue[1] + cue[3]) / 2.0);
        let ends = [
            ([cx, cue[1]], [cx, cue[1] - step]),
            ([cx, cue[3]], [cx, cue[3] + step]),
            ([cue[2], cy], [cue[2] + step, cy]),
        ];
        let bounds = [r, r, self.size[0] - r, self.size[1] - r];
        let corner = 24.0 * r;
        let cost = |way: &Vec<[f32; 2]>| {
            way.windows(2)
                .map(|w| (w[1][0] - w[0][0]).abs() + (w[1][1] - w[0][1]).abs())
                .sum::<f32>()
                + corner * way.len() as f32
        };
        let mut best: Option<Vec<[f32; 2]>> = None;
        for (at, off) in starts {
            for (on, before) in ends {
                let Some(mut way) = route(off, before, &blocked, bounds, corner) else {
                    continue;
                };
                way.insert(0, at);
                way.push(on);
                if best.as_ref().is_none_or(|b| cost(&way) < cost(b)) {
                    best = Some(way);
                }
            }
        }
        let mut pts = best.unwrap_or_default();
        pts.dedup_by(|a, b| (a[0] - b[0]).abs() < 0.25 && (a[1] - b[1]).abs() < 0.25);
        // A stub and the leg after it in one line are one leg.
        let mut out: Vec<[f32; 2]> = Vec::new();
        for p in pts {
            if let [.., a, b] = out[..]
                && (((a[0] - b[0]).abs() < 0.01 && (b[0] - p[0]).abs() < 0.01)
                    || ((a[1] - b[1]).abs() < 0.01 && (b[1] - p[1]).abs() < 0.01))
            {
                out.pop();
            }
            out.push(p);
        }
        if out.len() < 2 {
            return Vec::new();
        }
        out
    }
}

/// Who attacks whom among the fields of a watched stage's grid
/// (`watch-v0`): each of `links` (between two competitors) as the frame
/// at judged song second `t` shows it over `layout`, under `look`: its
/// line from the sender's field to the warning beside the recipient's,
/// the recipient's field outlined while the attack is on its way and
/// passing. Those with an end not in view are left out.
pub fn shown_between(
    links: &[ddi_session::battle::SeenLink],
    layout: &GridLayout,
    t: f64,
    reduced: bool,
    look: Look,
) -> Vec<Shown> {
    let slot_of = |seat: usize| layout.fields.slots.iter().position(|s| s.seat == seat);
    links
        .iter()
        .filter_map(|l| {
            let (from, to) = (slot_of(l.from)?, slot_of(l.to)?);
            let own = Link {
                attack: l.attack,
                seat: l.to,
                outgoing: true,
                shown_from: l.shown_from,
                first: l.first,
                clear: l.clear,
                chained: l.chained,
            };
            let (phase, line, pulse, outline) = timing(&own, t, reduced, look);
            let path = if line > 0.0 {
                layout.path(&layout.fields.slots[from], &layout.fields.slots[to])
            } else {
                Vec::new()
            };
            Some(Shown {
                attack: l.attack,
                seat: l.to,
                slot: to,
                outgoing: true,
                chained: l.chained,
                phase,
                line,
                pulse,
                outline,
                path,
                from: Some(l.from),
            })
        })
        .collect()
}

/// A link's phase, its line's opacity, its pulse's place and its
/// outline's opacity at judged song second `t` (a chained one quieter).
fn timing(l: &Link, t: f64, reduced: bool, look: Look) -> (Phase, f32, Option<f32>, f32) {
    let age = (t - l.shown_from).max(0.0) as f32;
    let quiet = if l.chained { look.quiet } else { 1.0 };
    let (phase, line, pulse) = if age >= look.line {
        (Phase::Outline, 0.0, None)
    } else if reduced {
        (Phase::Still, 1.0, None)
    } else if age < look.pulse {
        (Phase::Pulse, 1.0, Some(age / look.pulse.max(1e-3)))
    } else {
        let left = 1.0 - (age - look.pulse) / (look.line - look.pulse).max(1e-3);
        (Phase::Fade, left.clamp(0.0, 1.0), None)
    };
    let outline = if reduced {
        1.0
    } else {
        (age / look.outline_in.max(1e-3)).clamp(0.0, 1.0)
    };
    (phase, line * quiet, pulse, outline * quiet)
}

fn with_alpha(c: [f32; 4], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], c[3] * a]
}

/// The point `along` (`0..=1`) the polyline `path`.
fn point_along(path: &[[f32; 2]], along: f32) -> [f32; 2] {
    let lengths: Vec<f32> = path
        .windows(2)
        .map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt())
        .collect();
    let total: f32 = lengths.iter().sum();
    let mut left = total * along.clamp(0.0, 1.0);
    for (w, len) in path.windows(2).zip(&lengths) {
        if left <= *len || *len <= 0.0 {
            let k = if *len > 0.0 { left / len } else { 0.0 };
            return [
                w[0][0] + (w[1][0] - w[0][0]) * k,
                w[0][1] + (w[1][1] - w[0][1]) * k,
            ];
        }
        left -= len;
    }
    path.last().copied().unwrap_or([0.0; 2])
}

/// A dot of radius `r` at `at`.
fn dot(at: [f32; 2], r: f32, color: [f32; 4], soft: f32, blend: Blend) -> Command {
    let mut q = Quad::plain([at[0] - r, at[1] - r, at[0] + r, at[1] + r], color);
    q.shape = Shape {
        radius: [r; 2],
        soft: [soft; 2],
        ..Shape::NONE
    };
    q.blend = blend;
    Command::Quad(q)
}

/// Draws `shown` over the miniatures in `fields`: each link's line (a
/// soft glow under a thin stroke, the theme's attack colors), its pulse,
/// and the outline around its miniature's field; `ratio` a logical
/// pixel.
pub fn draw(list: &mut DrawList, shown: &[Shown], fields: &Fields, ratio: f32) {
    let [line, glow] = ddi_theme::attack_colors();
    for s in shown {
        // The outline around the opponent's field.
        if s.outline > 0.0
            && let Some(slot) = fields.slots.get(s.slot)
        {
            let grow = 1.5 * ratio;
            let [l, t, r, b] = slot.panel;
            let mut q = Quad::plain(
                [l - grow, t - grow, r + grow, b + grow],
                with_alpha(line, 0.9 * s.outline),
            );
            q.shape = Shape {
                radius: [3.0 * ratio; 2],
                outline: [1.5 * ratio; 2],
                ..Shape::NONE
            };
            list.push(Command::Quad(q));
        }
        if s.line <= 0.0 || s.path.len() < 2 {
            continue;
        }
        let thick = (1.5 * ratio).max(1.0);
        for w in s.path.windows(2) {
            let mut under = crate::battle_cue::segment(
                w[0],
                w[1],
                thick * 3.5,
                with_alpha(glow, 0.55 * s.line),
            );
            under.blend = Blend::Add;
            list.push(Command::Quad(under));
        }
        for w in s.path.windows(2) {
            list.push(Command::Quad(crate::battle_cue::segment(
                w[0],
                w[1],
                thick,
                with_alpha(line, 0.85 * s.line),
            )));
        }
        // Where it lands.
        if let Some(end) = s.path.last() {
            list.push(dot(
                *end,
                thick * 1.6,
                with_alpha(line, 0.9 * s.line),
                0.0,
                Blend::Normal,
            ));
        }
        if let Some(p) = s.pulse {
            let at = point_along(&s.path, p);
            let r = 7.0 * ratio;
            list.push(dot(at, r, with_alpha(glow, 1.6 * s.line), r * 0.6, Blend::Add));
            list.push(dot(
                at,
                2.5 * ratio,
                with_alpha([1.0, 1.0, 1.0, 1.0], s.line),
                0.0,
                Blend::Normal,
            ));
        }
    }
}

/// What the debug state says of the links drawn (`__DDI_DEBUG.battle
/// .attacks.links`): each one's attack, from whom to whom (`me`, or an
/// opponent's seat), its phase, and its line and outline as drawn.
pub fn debug(shown: &[Shown]) -> serde_json::Value {
    let end = |me: bool, seat: usize| {
        if me {
            serde_json::json!("me")
        } else {
            serde_json::json!(seat)
        }
    };
    // Between two competitors (a watched stage's grid): the sender's
    // seat in the player's place.
    let from = |s: &Shown| match s.from {
        Some(seat) => serde_json::json!(seat),
        None => end(s.outgoing, s.seat),
    };
    serde_json::Value::Array(
        shown
            .iter()
            .map(|s| {
                serde_json::json!({
                    "attack": s.attack,
                    "from": from(s),
                    "to": end(!s.outgoing, s.seat),
                    "seat": s.seat,
                    "slot": s.slot,
                    "phase": s.phase.id(),
                    "chained": s.chained,
                    "line": s.line,
                    "pulse": s.pulse,
                    "outline": s.outline,
                    "path": s.path,
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opponents::tests::{SIZES, keep};

    fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
        a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]
    }

    /// The box of the leg from `p` to `q`, a hair wide.
    fn leg(p: [f32; 2], q: [f32; 2]) -> [f32; 4] {
        [
            p[0].min(q[0]) - 0.5,
            p[1].min(q[1]) - 0.5,
            p[0].max(q[0]) + 0.5,
            p[1].max(q[1]) + 0.5,
        ]
    }

    fn link(seat: usize, outgoing: bool, chained: bool) -> Link {
        Link {
            attack: 1 + seat as u32,
            seat,
            outgoing,
            shown_from: 10.0,
            first: 14.0,
            clear: 18.0,
            chained,
        }
    }

    /// A watched stage's grid: at every size of the miniatures' table,
    /// with and without Reverse, for two to eight fields, the line of
    /// every attack from one field to another starts at the sender's
    /// field at its power bar, ends on the warning's place beside the
    /// recipient's field, goes in straight legs, and none of its legs is
    /// over a field, the status line or the quit button's place; and a
    /// link shows its line for its time and outlines the recipient.
    #[test]
    fn a_grid_s_lines_go_between_its_fields() {
        use crate::opponents::tests::keep_grid;
        let (mut lines, mut none) = (0, 0);
        for (w, h, ratio) in SIZES {
            for reverse in [false, true] {
                for n in 2..=8 {
                    let g = FieldGeometry::placed(
                        w * ratio,
                        h * ratio,
                        4.0,
                        reverse,
                        Default::default(),
                    );
                    let k = keep_grid(&g, ratio);
                    let f = crate::opponents::arrange_grid(n, &g, reverse, ratio, &k).unwrap();
                    let what = format!("{w}x{h}@{ratio} reverse {reverse} n {n}");
                    let layout = GridLayout {
                        fields: &f,
                        size: [g.width, g.height],
                        span: 4.0,
                        ratio,
                        keep_out: &k,
                    };
                    for (i, from) in f.slots.iter().enumerate() {
                        for (j, to) in f.slots.iter().enumerate() {
                            if i == j {
                                continue;
                            }
                            let p = layout.path(from, to);
                            if p.is_empty() {
                                none += 1;
                                continue;
                            }
                            lines += 1;
                            let (mini, _) = ddi_render::scene::miniature_power_and_life(
                                &from.field,
                                from.panel,
                                4.0,
                            );
                            let start = p[0];
                            assert!(
                                (start[0] - from.panel[0]).abs() < 0.01
                                    || (start[0] - from.panel[2]).abs() < 0.01,
                                "{what} {i}→{j}: {start:?}"
                            );
                            assert!(
                                start[1] >= mini[1] && start[1] <= mini[1] + mini[3],
                                "{what} {i}→{j}"
                            );
                            let cue = crate::battle_cue::place(&to.field, 4.0);
                            let end = p[p.len() - 1];
                            assert!(
                                end[0] >= cue[0] - 0.5
                                    && end[0] <= cue[2] + 0.5
                                    && end[1] >= cue[1] - 0.5
                                    && end[1] <= cue[3] + 0.5,
                                "{what} {i}→{j}: {end:?} {cue:?}"
                            );
                            for leg_ in p.windows(2) {
                                let (a, b) = (leg_[0], leg_[1]);
                                assert!(
                                    (a[0] - b[0]).abs() < 0.01 || (a[1] - b[1]).abs() < 0.01,
                                    "{what} {i}→{j}: a diagonal"
                                );
                                let b_ = leg(a, b);
                                for (m, s) in f.slots.iter().enumerate() {
                                    let r = [
                                        s.rect[0] + 0.6,
                                        s.rect[1] + 0.6,
                                        s.rect[0] + s.rect[2] - 0.6,
                                        s.rect[1] + s.rect[3] - 0.6,
                                    ];
                                    assert!(!overlaps(b_, r), "{what} {i}→{j} over field {m}");
                                }
                                for keep in &k {
                                    assert!(!overlaps(b_, *keep), "{what} {i}→{j} over {keep:?}");
                                }
                                let [x, y, sw, sh] = f.status;
                                assert!(
                                    !overlaps(b_, [x, y, x + sw, y + sh]),
                                    "{what} {i}→{j} over the status"
                                );
                            }
                        }
                    }
                }
            }
        }
        // Nearly every pair has a clear way (where it has none, the
        // outline alone says the attack).
        assert!(lines > 3000 && none * 50 < lines, "{lines} lines, {none} without");
        // A link over its time.
        let g = FieldGeometry::placed(1280.0, 720.0, 4.0, false, Default::default());
        let k = keep_grid(&g, 1.0);
        let f = crate::opponents::arrange_grid(4, &g, false, 1.0, &k).unwrap();
        let layout = GridLayout {
            fields: &f,
            size: [1280.0, 720.0],
            span: 4.0,
            ratio: 1.0,
            keep_out: &k,
        };
        let seen = [ddi_session::battle::SeenLink {
            attack: 5,
            from: 3,
            to: 1,
            shown_from: 10.0,
            first: 14.0,
            clear: 18.0,
            chained: false,
        }];
        let at = |t: f64, reduced: bool| shown_between(&seen, &layout, t, reduced, ATTACK_LINKS_V0);
        let pulse = &at(10.3, false)[0];
        assert_eq!((pulse.from, pulse.seat, pulse.slot), (Some(3), 1, 1));
        assert_eq!(pulse.phase, Phase::Pulse);
        assert!(pulse.path.len() >= 2 && pulse.line == 1.0);
        let later = &at(13.0, false)[0];
        assert_eq!(later.phase, Phase::Outline);
        assert!(later.path.is_empty() && later.outline == 1.0);
        assert_eq!(at(10.3, true)[0].phase, Phase::Still);
        // The debug state says who attacks whom by seat.
        let d = debug(&at(10.3, false));
        assert_eq!((d[0]["from"].clone(), d[0]["to"].clone()), (3.into(), 1.into()));
        // The same link as the followed competitor's own.
        assert_eq!(seen[0].of(1).map(|l| (l.seat, l.outgoing)), Some((3, false)));
        assert_eq!(seen[0].of(3).map(|l| (l.seat, l.outgoing)), Some((1, true)));
        assert_eq!(seen[0].of(0), None);
        let mut list = DrawList::new();
        draw(&mut list, &at(10.3, false), &f, 1.0);
        assert!(list.commands.len() > 3);
        // A grid's page shows only the lines between its own fields: an
        // attack from or to a field on another page is left out (no stub
        // at the edge; that field's own page shows it).
        let off = [
            ddi_session::battle::SeenLink { from: 14, ..seen[0] },
            ddi_session::battle::SeenLink { to: 20, ..seen[0] },
        ];
        assert!(shown_between(&off, &layout, 10.3, false, ATTACK_LINKS_V0).is_empty());
    }

    /// At every size the miniatures' table uses, with and without
    /// Reverse, for one to eight opponents: every outgoing and incoming
    /// line keeps to the side space. It starts at the band's edge by the
    /// power bar (outgoing) or ends at the warning's place or at the
    /// band's edge on the receptors' row (incoming), touches its own
    /// miniature at its bar, and none of its legs is over the field's
    /// band, the HUD's areas, the judgment, the quit button or any
    /// miniature.
    #[test]
    fn every_line_keeps_to_the_side_space() {
        let (mut lines, mut none) = (0, 0);
        for (w, h, ratio) in SIZES {
            for reverse in [false, true] {
                for n in [1, 2, 3, 4, 6, 8] {
                    let g = FieldGeometry::placed(
                        w * ratio,
                        h * ratio,
                        4.0,
                        reverse,
                        Default::default(),
                    );
                    let k = keep(&g, ratio, true);
                    let Some(f) = crate::opponents::arrange(n, &g, 4.0, reverse, ratio, &k) else {
                        continue;
                    };
                    let what = format!("{w}x{h}@{ratio} reverse {reverse} n {n}");
                    // The line's own keep-outs: the HUD and the quit
                    // button (the warning's place is where it ends).
                    let mut own = ddi_render::scene::hud_areas(&g, 4.0);
                    own.push(k[own.len()]);
                    let layout = Layout {
                        geo: &g,
                        span: 4.0,
                        fields: &f,
                        ratio,
                        keep_out: &own,
                    };
                    let [mid, band] = ddi_render::scene::filter_band(&g, 4.0);
                    let (bl, br) = (mid - band / 2.0, mid + band / 2.0);
                    let band_box = [bl + 0.6, 0.0, br - 0.6, g.height];
                    let cue = crate::battle_cue::place(&g, 4.0);
                    let (power, _) = ddi_render::scene::power_and_life(&g, 4.0);
                    for (i, slot) in f.slots.iter().enumerate() {
                        for outgoing in [true, false] {
                            let p = layout.path(slot, outgoing);
                            if p.is_empty() {
                                // The outline alone says it: only in the
                                // margins a phone held upright leaves.
                                assert!(w < h, "{what} slot {i} outgoing {outgoing}: no way");
                                none += 1;
                                continue;
                            }
                            lines += 1;
                            let (mini, _) = ddi_render::scene::miniature_power_and_life(
                                &slot.field,
                                slot.panel,
                                4.0,
                            );
                            let at_mini = if outgoing { p[p.len() - 1] } else { p[0] };
                            let other = if outgoing { p[0] } else { p[p.len() - 1] };
                            // At the miniature: its field-facing edge, at
                            // its power bar's height.
                            let edge = if slot.left { slot.panel[2] } else { slot.panel[0] };
                            assert!((at_mini[0] - edge).abs() < 0.01, "{what} slot {i}");
                            assert!(
                                at_mini[1] >= mini[1] && at_mini[1] <= mini[1] + mini[3],
                                "{what} slot {i}"
                            );
                            if outgoing {
                                // From the band's edge on that side, near
                                // the power bar.
                                let x = if slot.left { bl } else { br };
                                assert!((other[0] - x).abs() < 0.01, "{what} slot {i}");
                                assert!(
                                    (other[1] - (power[1] + power[3] / 2.0)).abs()
                                        <= g.arrow * 0.6,
                                    "{what} slot {i}: {other:?} {power:?}"
                                );
                            } else if slot.left {
                                assert!(
                                    (other[0] - bl).abs() < 0.01
                                        && (other[1] - g.receptor_y).abs() < 0.01,
                                    "{what} slot {i}: {other:?}"
                                );
                            } else {
                                // On the warning's place's edge.
                                assert!(
                                    other[0] >= cue[0] - 0.5
                                        && other[0] <= cue[2] + 0.5
                                        && other[1] >= cue[1] - 0.5
                                        && other[1] <= cue[3] + 0.5,
                                    "{what} slot {i}: {other:?} {cue:?}"
                                );
                            }
                            for (j, w2) in p.windows(2).enumerate() {
                                // Straight legs only.
                                assert!(
                                    (w2[0][0] - w2[1][0]).abs() < 0.01
                                        || (w2[0][1] - w2[1][1]).abs() < 0.01,
                                    "{what} slot {i}"
                                );
                                let b = leg(w2[0], w2[1]);
                                assert!(
                                    !overlaps(b, band_box),
                                    "{what} slot {i} outgoing {outgoing} leg {j} over the band: {p:?}"
                                );
                                for (m, ko) in own.iter().enumerate() {
                                    // (The life gauge is in the band.)
                                    assert!(
                                        !overlaps(b, *ko),
                                        "{what} slot {i} outgoing {outgoing} leg {j} over keep-out {m}: {p:?} {ko:?}"
                                    );
                                }
                                let judgment = ddi_render::scene::judgement_area(&g, 4.0);
                                assert!(!overlaps(b, judgment), "{what} slot {i}");
                                for (o, other_slot) in f.slots.iter().enumerate() {
                                    let r = other_slot.rect;
                                    // Shrunk a hair: a leg may run along
                                    // an edge, never inside.
                                    let inside = [
                                        r[0] + 0.75,
                                        r[1] + 0.75,
                                        r[0] + r[2] - 0.75,
                                        r[1] + r[3] - 0.75,
                                    ];
                                    let panel = other_slot.panel;
                                    let inside_panel = [
                                        panel[0] + 0.75,
                                        panel[1] + 0.75,
                                        panel[2] - 0.75,
                                        panel[3] - 0.75,
                                    ];
                                    assert!(
                                        !overlaps(b, inside) && !overlaps(b, inside_panel),
                                        "{what} slot {i} outgoing {outgoing} leg {j} over miniature {o}: {p:?} {r:?}"
                                    );
                                }
                                // Incoming lines end at the warning;
                                // outgoing ones never cross it.
                                if outgoing {
                                    assert!(
                                        !overlaps(b, cue),
                                        "{what} slot {i} leg {j} over the warning: {p:?} {cue:?}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        eprintln!("{lines} lines, {none} without a way");
        assert!(lines > 400 && none * 20 < lines, "{lines} lines, {none} without a way");
    }

    /// A link over time: the pulse runs for 0.6 s along a whole line,
    /// the line fades to nothing by 1.5 s, the outline stays until the
    /// attack has passed (the session drops the link then); a chained
    /// one is quieter; reduced motion has no pulse and no fade, the line
    /// whole for its time.
    #[test]
    fn a_link_pulses_fades_and_keeps_its_outline() {
        let g = FieldGeometry::placed(1280.0, 720.0, 4.0, false, Default::default());
        let k = keep(&g, 1.0, true);
        let f = crate::opponents::arrange(8, &g, 4.0, false, 1.0, &k).unwrap();
        let layout = Layout {
            geo: &g,
            span: 4.0,
            fields: &f,
            ratio: 1.0,
            keep_out: &k[..k.len() - 1],
        };
        let look = ATTACK_LINKS_V0;
        let at = |t: f64, reduced: bool, l: Link| {
            shown(&[l], &layout, t, reduced, look).remove(0)
        };
        let s = at(10.0, false, link(5, true, false));
        assert_eq!((s.phase, s.line, s.pulse), (Phase::Pulse, 1.0, Some(0.0)));
        assert_eq!((s.seat, s.slot, s.outgoing), (5, 5, true));
        assert_eq!(s.outline, 0.0, "it comes in");
        let s = at(10.3, false, link(5, true, false));
        assert_eq!(s.phase, Phase::Pulse);
        assert!((s.pulse.unwrap() - 0.5).abs() < 1e-3 && s.outline == 1.0);
        let s = at(11.05, false, link(5, true, false));
        assert_eq!((s.phase, s.pulse), (Phase::Fade, None));
        assert!((s.line - 0.5).abs() < 1e-3, "{}", s.line);
        let s = at(11.6, false, link(5, true, false));
        assert_eq!((s.phase, s.line, s.outline), (Phase::Outline, 0.0, 1.0));
        assert!(s.path.is_empty());
        // Chained: quieter throughout.
        let s = at(10.3, false, link(5, false, true));
        assert!((s.line - look.quiet).abs() < 1e-6 && (s.outline - look.quiet).abs() < 1e-6);
        assert!(!s.outgoing && s.chained);
        // Reduced motion: still, then gone.
        let s = at(10.0, true, link(2, true, false));
        assert_eq!((s.phase, s.line, s.pulse, s.outline), (Phase::Still, 1.0, None, 1.0));
        let s = at(11.4, true, link(2, true, false));
        assert_eq!((s.phase, s.line), (Phase::Still, 1.0));
        let s = at(11.5, true, link(2, true, false));
        assert_eq!((s.phase, s.line, s.outline), (Phase::Outline, 0.0, 1.0));
        // A seat not in view has no link.
        assert!(shown(&[link(9, true, false)], &layout, 10.0, false, look).is_empty());
        // Drawn: an outline, the line's glow and stroke a leg, where it
        // lands, the pulse's two dots.
        let s = at(10.3, false, link(5, true, false));
        let legs = s.path.len() - 1;
        let mut list = DrawList::new();
        draw(&mut list, std::slice::from_ref(&s), &f, 1.0);
        assert_eq!(list.commands.len(), 1 + 2 * legs + 1 + 2);
        // The outline alone once the line is gone.
        let mut list = DrawList::new();
        draw(&mut list, &[at(12.0, false, link(5, true, false))], &f, 1.0);
        assert_eq!(list.commands.len(), 1);
        // The debug state says from whom to whom and the phase.
        let d = debug(&[s]);
        assert_eq!(d[0]["from"], "me");
        assert_eq!(d[0]["to"], 5);
        assert_eq!(d[0]["phase"], "pulse");
        let d = debug(&[at(10.3, false, link(1, false, false))]);
        assert_eq!((d[0]["from"].clone(), d[0]["to"].clone()), (1.into(), "me".into()));
        // The pulse travels the whole line.
        let p = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 30.0]];
        assert_eq!(point_along(&p, 0.0), [0.0, 0.0]);
        assert_eq!(point_along(&p, 0.25), [10.0, 0.0]);
        assert_eq!(point_along(&p, 1.0), [10.0, 30.0]);
    }
}
