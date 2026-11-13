//! Skins' quads put in fewer draws without changing a pixel: a run of
//! quads is drawn in batches of consecutive quads that share a texture, a
//! material and a blend (`textured::build`), and a run that alternates
//! between them (each note's body and outline, each miniature's receptors,
//! holds, notes and hit effects in turn) is drawn in many small batches,
//! each a draw call with its state set again. On WebGL2 every one of them
//! is several GL calls the browser checks on the main thread: eight
//! miniatures' runs were about a hundred batches a frame against the
//! play's own field's ten (`docs/plans/multiplayer-prototype-progress.md`,
//! "Frame cost in the browser").
//!
//! [`Batcher::order`] moves a quad back into an earlier batch of its own
//! kind where nothing drawn between that batch and it touches the same
//! pixels: two quads that touch no pixel in common are drawn in either
//! order to the same result (blending is per pixel), so the frame is the
//! same, pixel for pixel, whatever is moved. What a quad may touch is its
//! corners' box two pixels wider (`renderer::quad_rect`, as the
//! conversions' scissors), and a quad with no such box, or that uses the
//! mask (whose stencil may reach the whole target, `Mask::Clear`), moves
//! nothing past it.

use crate::atlas::PageRect;
use crate::stencil::Mask;
use crate::textured::{QuadBlend, TexQuad};

/// How far back a quad looks for a batch of its kind: past this many
/// batches it starts its own (bounding the work a frame; the result is
/// exact either way, only less merged).
const LOOK_BACK: usize = 48;

/// Boxes a batch keeps of what its quads may touch: each quad's box lies
/// in one of them, so a quad clear of all of them is clear of every quad
/// of the batch (a few boxes bound the checks; their being larger than
/// the quads only merges less).
const CLUSTERS: usize = 8;

/// What makes two quads drawable in one batch (`textured::build`'s
/// condition), a packed texture standing for its atlas page.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Key {
    pub texture: u32,
    pub blend: QuadBlend,
    pub wrap: bool,
    pub material: Option<u32>,
    pub images: [u32; 2],
}

struct Group {
    key: Key,
    /// A quad of it may touch anything: nothing moves past it.
    barrier: bool,
    /// What its quads may touch: boxes holding each quad's
    /// ([`CLUSTERS`] at most, `count` of them used).
    boxes: [PageRect; CLUSTERS],
    count: usize,
    /// Its quads, by index into the run, in their order.
    items: Vec<u32>,
}

fn area(r: PageRect) -> u64 {
    u64::from(r.width) * u64::from(r.height)
}

impl Group {
    fn new(key: Key, r: Option<PageRect>, items: Vec<u32>) -> Group {
        let mut g = Group {
            key,
            barrier: r.is_none(),
            boxes: [PageRect::default(); CLUSTERS],
            count: 0,
            items,
        };
        if let Some(r) = r {
            g.add(r);
        }
        g
    }

    /// Whether nothing of it may touch what `r` may.
    fn clear_of(&self, r: PageRect) -> bool {
        !self.barrier && self.boxes[..self.count].iter().all(|&b| !overlaps(b, r))
    }

    /// `r` among its boxes: a box of its own while there is room, else
    /// into the box it grows least.
    fn add(&mut self, r: PageRect) {
        if self.count < CLUSTERS {
            self.boxes[self.count] = r;
            self.count += 1;
            return;
        }
        let grow = |b: PageRect| area(b.union(r)) - area(b);
        if let Some(b) = self.boxes.iter_mut().min_by_key(|b| grow(**b)) {
            *b = b.union(r);
        }
    }
}

/// Reorders runs of quads for fewer batches, its room kept between
/// frames.
#[derive(Default)]
pub(crate) struct Batcher {
    groups: Vec<Group>,
    /// Groups emptied, their room reused.
    spare: Vec<Vec<u32>>,
    rects: Vec<Option<PageRect>>,
    out: Vec<TexQuad>,
}

fn overlaps(a: PageRect, b: PageRect) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

impl Batcher {
    /// Puts `quads` (one run, drawn in one pass) in an order drawing the
    /// same pixels in fewer batches: each quad after every earlier quad
    /// that may touch a pixel it touches. `key` is what batches a quad,
    /// `rect` what it may touch on the target (`None`: anything).
    pub(crate) fn order(
        &mut self,
        quads: &mut [TexQuad],
        key: impl Fn(&TexQuad) -> Key,
        rect: impl Fn(&TexQuad) -> Option<PageRect>,
    ) {
        if quads.len() < 3 {
            return;
        }
        for mut g in self.groups.drain(..) {
            g.items.clear();
            self.spare.push(g.items);
        }
        self.rects.clear();
        self.rects.extend(quads.iter().map(|q| {
            if q.mask == Mask::None {
                rect(q)
            } else {
                None
            }
        }));
        for (i, q) in quads.iter().enumerate() {
            let k = key(q);
            let r = self.rects[i];
            // The earliest batch of its kind it may join: scanning back,
            // each batch passed must touch nothing it touches.
            let mut join = None;
            if let Some(r) = r {
                for (n, g) in self.groups.iter().enumerate().rev().take(LOOK_BACK) {
                    if g.key == k {
                        join = Some(n);
                    }
                    if !g.clear_of(r) {
                        break;
                    }
                }
            } else if self.groups.last().is_some_and(|last| last.key == k) {
                // (At the end of the last batch: where it was.)
                join = Some(self.groups.len() - 1);
            }
            match join {
                Some(n) => {
                    let g = &mut self.groups[n];
                    g.items.push(i as u32);
                    match r {
                        Some(r) => g.add(r),
                        None => g.barrier = true,
                    }
                }
                None => {
                    let mut items = self.spare.pop().unwrap_or_default();
                    items.push(i as u32);
                    self.groups.push(Group::new(k, r, items));
                }
            }
        }
        // Nothing moved: as it was.
        let moved = self
            .groups
            .iter()
            .flat_map(|g| g.items.iter())
            .enumerate()
            .any(|(at, &i)| at as u32 != i);
        if !moved {
            return;
        }
        self.out.clear();
        self.out.extend(
            self.groups
                .iter()
                .flat_map(|g| g.items.iter().map(|&i| quads[i as usize])),
        );
        quads.copy_from_slice(&self.out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::textured::{Shape, UNTEXTURED};

    fn quad(x: f32, y: f32, texture: u32) -> TexQuad {
        TexQuad::new(
            Shape::Quad {
                transform: [10.0, 0.0, 0.0, 10.0, x, y],
                uv: [0.0, 0.0, 1.0, 1.0],
            },
            [1.0; 4],
            texture,
        )
    }

    fn key(q: &TexQuad) -> Key {
        Key {
            texture: q.texture,
            blend: q.blend,
            wrap: q.wrap,
            material: None,
            images: [UNTEXTURED; 2],
        }
    }

    fn rect(q: &TexQuad) -> Option<PageRect> {
        match q.shape {
            Shape::Quad { transform: m, .. } => Some(PageRect {
                x: (m[4] - 7.0) as u32,
                y: (m[5] - 7.0) as u32,
                width: 14,
                height: 14,
            }),
            _ => None,
        }
    }

    fn batches(quads: &[TexQuad]) -> usize {
        1 + quads.windows(2).filter(|w| w[0].texture != w[1].texture).count()
    }

    /// Every pair that may touch the same pixels keeps its order.
    fn keeps_overlapping_order(before: &[TexQuad], after: &[TexQuad]) -> bool {
        let at = |q: &TexQuad| after.iter().position(|a| a == q).unwrap();
        before.iter().enumerate().all(|(i, a)| {
            before[i + 1..].iter().all(|b| {
                let overlap = match (rect(a), rect(b)) {
                    (Some(x), Some(y)) => overlaps(x, y),
                    _ => true,
                };
                !overlap || at(a) < at(b)
            })
        })
    }

    #[test]
    fn apart_fields_alternating_kinds_are_drawn_in_two_batches() {
        // Eight small fields, each a receptor (texture 1) and a note over
        // it (texture 2): sixteen batches in their order, two reordered.
        let mut quads: Vec<TexQuad> = (0..8)
            .flat_map(|i| {
                let x = 20.0 + 40.0 * i as f32;
                [quad(x, 20.0, 1), quad(x, 22.0, 2)]
            })
            .collect();
        let before = quads.clone();
        assert_eq!(batches(&quads), 16);
        Batcher::default().order(&mut quads, key, rect);
        assert_eq!(batches(&quads), 2);
        assert!(keeps_overlapping_order(&before, &quads));
    }

    #[test]
    fn overlapping_quads_keep_their_order() {
        // Texture 1, 2, 1 over one place: nothing moves.
        let mut quads = vec![quad(20.0, 20.0, 1), quad(21.0, 20.0, 2), quad(22.0, 20.0, 1)];
        let before = quads.clone();
        Batcher::default().order(&mut quads, key, rect);
        assert_eq!(quads, before);
    }

    #[test]
    fn a_masked_quad_is_a_barrier() {
        let mut masked = quad(200.0, 200.0, 2);
        masked.mask = Mask::Clear;
        let mut quads = vec![quad(20.0, 20.0, 1), masked, quad(100.0, 20.0, 1)];
        let before = quads.clone();
        Batcher::default().order(&mut quads, key, rect);
        assert_eq!(quads, before);
    }

    #[test]
    fn many_runs_keep_every_overlapping_pair_in_order() {
        // A pseudo-random scatter of quads of three kinds.
        let mut seed = 7u32;
        let mut next = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            seed >> 8
        };
        let mut b = Batcher::default();
        for _ in 0..50 {
            let mut quads: Vec<TexQuad> = (0..60)
                .map(|_| {
                    quad(
                        10.0 + (next() % 300) as f32,
                        10.0 + (next() % 100) as f32,
                        next() % 3,
                    )
                })
                .collect();
            let before = quads.clone();
            b.order(&mut quads, key, rect);
            assert!(keeps_overlapping_order(&before, &quads));
            assert!(batches(&quads) <= batches(&before));
            let mut a = before.clone();
            let mut c = quads.clone();
            let id = |q: &TexQuad| match q.shape {
                Shape::Quad { transform: m, .. } => (m[4] as i32, m[5] as i32, q.texture),
                _ => (0, 0, 0),
            };
            a.sort_by_key(id);
            c.sort_by_key(id);
            assert_eq!(a, c, "the same quads");
        }
    }
}
