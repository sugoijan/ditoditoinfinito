//! Opt-in contours joining adjacent, still-visible judgment instances.

use super::*;
use ddi_noteskin::component::{Clocks as ComponentClocks, Component};

#[derive(Clone, Copy)]
struct Endpoint {
    lane: usize,
    hit: Hit,
    at: [f32; 2],
    strength: f32,
}

pub(super) struct JudgmentLinks {
    pairs: Vec<[Endpoint; 2]>,
}

impl Endpoint {
    /// Whether this is the instance on lane `lane` of judgment `name`
    /// hit at `at` (song seconds).
    fn is(&self, lane: usize, name: &str, at: f64) -> bool {
        self.lane == lane && self.hit.name == name && self.hit.at == at
    }
}

impl JudgmentLinks {
    /// Whether the instance on lane `lane` of judgment `name` hit at `at`
    /// shows its part of `role`: never a link (drawn between the lanes),
    /// and a rim only on a side that is not linked. Other instances of the
    /// lane keep their rims.
    pub(super) fn shows(&self, lane: usize, name: &str, at: f64, role: Option<&str>) -> bool {
        match role {
            Some("link") => false,
            Some("rim-left") => !self.pairs.iter().any(|[_, r]| r.is(lane, name, at)),
            Some("rim-right") => !self.pairs.iter().any(|[l, _]| l.is(lane, name, at)),
            _ => true,
        }
    }
}

fn clocks(play: &NoteskinPlay, lane: usize, hit: Hit) -> ComponentClocks {
    ComponentClocks {
        hit: Some((play.frame_time.0 - hit.at) as f32),
        seed: hit_seed(lane, hit.at),
        ..play.clocks(lane)
    }
}

fn link_strength(c: &Component, hit: Hit, clocks: &ComponentClocks) -> f32 {
    // Only the link parts are posed: the rest are most of an explosion.
    c.parts
        .iter()
        .filter(|p| p.role.as_deref() == Some("link") && p.shows_for(hit.name, hit.bright))
        .filter_map(|p| p.pose(clocks))
        .map(|p| p.opacity * p.tint[3])
        .fold(0.0, f32::max)
}

impl NoteskinPlay {
    pub(super) fn judgment_links(
        &self,
        lanes: &[Lane],
        layout: &Layout,
        geo: &FieldGeometry,
    ) -> JudgmentLinks {
        let mut nodes = Vec::new();
        for (i, lane) in lanes.iter().enumerate() {
            let (Some(l), Element::Component(c)) = (layout.lanes.get(i), &lane.explosion) else {
                continue;
            };
            // All three roles opt in. An ordinary component is unaffected.
            if !["link", "rim-left", "rim-right"]
                .iter()
                .all(|role| c.parts.iter().any(|p| p.role.as_deref() == Some(role)))
            {
                continue;
            }
            let at = [geo.lane_x(l.column), geo.lane_receptor_y(l.scroll_sign)];
            if !at.iter().all(|n| n.is_finite()) {
                continue;
            }
            // One bridge per lane pair, even when repeated hits overlap.
            for &hit in lane.hits.iter().rev() {
                let age = self.frame_time.0 - hit.at;
                if age < 0.0 || age > f64::from(c.duration) {
                    continue;
                }
                let strength = link_strength(c, hit, &clocks(self, i, hit));
                if strength > 0.0 {
                    nodes.push(Endpoint {
                        lane: i,
                        hit,
                        at,
                        strength,
                    });
                    break;
                }
            }
        }
        // Each physical receptor row apart (layouts may put both scroll
        // directions at the same columns), and only the same judgment's.
        JudgmentLinks {
            pairs: adjacent_pairs(
                nodes,
                |n| n.at,
                |a, b| a.hit.name.cmp(b.hit.name),
                geo.arrow,
            ),
        }
    }

    pub(super) fn draw_judgment_links(
        &mut self,
        lanes: &[Lane],
        links: &JudgmentLinks,
        geo: &FieldGeometry,
        out: &mut Vec<TexQuad>,
    ) {
        for [left, right] in &links.pairs {
            let Element::Component(c) = &lanes[left.lane].explosion else {
                continue;
            };
            // Placed by its ends alone: not turned, zoomed or moved as the
            // left lane's explosion is.
            self.draw_parts(
                c,
                [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                bridge(left.at, right.at, geo.arrow),
                ((right.strength / left.strength).min(1.0), 1.0),
                &clocks(self, left.lane, left.hit),
                &|p| {
                    (p.role.as_deref() == Some("link")
                        && p.shows_for(left.hit.name, left.hit.bright))
                    .then_some(Mask::None)
                },
                out,
            );
        }
    }
}
