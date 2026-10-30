//! A shelf packer for atlas pages: rectangles of about one height laid
//! left to right on shelves, a new shelf under the last when none fits.
//! Text runs are its rectangles (`text_atlas`): one line high, of a few
//! sizes, so shelves waste little, and a page is cleared whole rather than
//! freed run by run (`docs/plans/ui-core.md`, P4).

/// A rectangle of a page, in texels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PageRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PageRect {
    pub fn right(&self) -> u32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> u32 {
        self.y + self.height
    }

    /// The smallest rectangle covering both.
    pub fn union(self, o: PageRect) -> PageRect {
        let (x, y) = (self.x.min(o.x), self.y.min(o.y));
        PageRect {
            x,
            y,
            width: self.right().max(o.right()) - x,
            height: self.bottom().max(o.bottom()) - y,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Shelf {
    y: u32,
    height: u32,
    /// Where the next rectangle goes.
    x: u32,
}

/// Places rectangles on one page.
#[derive(Clone, Debug)]
pub struct ShelfPacker {
    width: u32,
    height: u32,
    shelves: Vec<Shelf>,
    /// The top of the next shelf.
    next_y: u32,
    /// [`WASTE`], or the page's own ([`ShelfPacker::with_waste`]).
    waste: f32,
}

/// A shelf takes a rectangle whose height is within this fraction of its
/// own (taller ones open a new shelf; much shorter ones would waste it).
const WASTE: f32 = 0.3;

impl ShelfPacker {
    pub fn new(width: u32, height: u32) -> ShelfPacker {
        ShelfPacker {
            width,
            height,
            shelves: Vec::new(),
            next_y: 0,
            waste: WASTE,
        }
    }

    /// A shelf takes a rectangle up to `waste` of its height shorter than
    /// it (1: any shorter one; for rectangles placed tallest first, as a
    /// skin's images are).
    pub fn with_waste(mut self, waste: f32) -> ShelfPacker {
        self.waste = waste;
        self
    }

    /// A place for a `width × height` rectangle, or `None` when the page
    /// is full for it (or it is larger than the page).
    pub fn alloc(&mut self, width: u32, height: u32) -> Option<PageRect> {
        if width == 0 || height == 0 || width > self.width || height > self.height {
            return None;
        }
        // The lowest shelf it fits on without wasting too much of it.
        let best = self
            .shelves
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.height >= height
                    && (s.height - height) as f32 <= s.height as f32 * self.waste
                    && s.x + width <= self.width
            })
            .min_by_key(|(_, s)| s.height)
            .map(|(i, _)| i);
        let shelf = match best {
            Some(i) => &mut self.shelves[i],
            None => {
                if self.next_y + height > self.height {
                    return None;
                }
                self.shelves.push(Shelf {
                    y: self.next_y,
                    height,
                    x: 0,
                });
                self.next_y += height;
                self.shelves.last_mut().expect("just pushed")
            }
        };
        let rect = PageRect {
            x: shelf.x,
            y: shelf.y,
            width,
            height,
        };
        shelf.x += width;
        Some(rect)
    }

    /// Forgets every rectangle.
    pub fn clear(&mut self) {
        self.shelves.clear();
        self.next_y = 0;
    }

    /// The rows the shelves take, from the top.
    pub fn used_height(&self) -> u32 {
        self.next_y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangles_share_a_shelf_of_about_their_height() {
        let mut p = ShelfPacker::new(100, 100);
        let a = p.alloc(40, 20).unwrap();
        let b = p.alloc(40, 18).unwrap();
        assert_eq!((a.x, a.y, b.x, b.y), (0, 0, 40, 0));
        // Too short for the shelf (more than 30 % of it left empty): a
        // new one.
        let c = p.alloc(10, 10).unwrap();
        assert_eq!((c.x, c.y), (0, 20));
        // Too wide for what is left of the first: on a new shelf.
        let d = p.alloc(30, 20).unwrap();
        assert_eq!((d.x, d.y), (0, 30));
        assert_eq!(p.used_height(), 50);
        // The shortest shelf that fits wins.
        let e = p.alloc(10, 9).unwrap();
        assert_eq!((e.x, e.y), (10, 20));
    }

    /// A skin's images, tallest first: any shorter one shares a shelf,
    /// and room left on an earlier shelf is used before a new one opens.
    #[test]
    fn a_loose_page_takes_any_shorter_rectangle() {
        let mut p = ShelfPacker::new(100, 100).with_waste(1.0);
        let a = p.alloc(60, 40).unwrap();
        let b = p.alloc(70, 20).unwrap();
        assert_eq!((a.x, a.y, b.x, b.y), (0, 0, 0, 40));
        // Too wide for the second shelf's room, short enough for the
        // first's: there, not on a new shelf.
        let c = p.alloc(35, 10).unwrap();
        assert_eq!((c.x, c.y), (60, 0));
        assert_eq!(p.used_height(), 60);
    }

    #[test]
    fn a_full_page_refuses_and_clears() {
        let mut p = ShelfPacker::new(64, 32);
        assert!(p.alloc(65, 1).is_none());
        assert!(p.alloc(1, 33).is_none());
        assert!(p.alloc(0, 4).is_none());
        for _ in 0..4 {
            assert!(p.alloc(64, 8).is_some());
        }
        assert!(p.alloc(1, 8).is_none());
        p.clear();
        assert_eq!(p.alloc(64, 32).map(|r| (r.x, r.y)), Some((0, 0)));
    }

    #[test]
    fn rectangles_never_overlap() {
        let mut p = ShelfPacker::new(256, 256);
        let mut placed: Vec<PageRect> = Vec::new();
        // A deterministic spread of run-like sizes.
        let mut seed = 7u32;
        for _ in 0..500 {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let w = 8 + (seed >> 16) % 90;
            let h = 14 + (seed >> 8) % 12;
            let Some(r) = p.alloc(w, h) else { continue };
            assert!(r.right() <= 256 && r.bottom() <= 256);
            for o in &placed {
                let apart =
                    r.right() <= o.x || o.right() <= r.x || r.bottom() <= o.y || o.bottom() <= r.y;
                assert!(apart, "{r:?} overlaps {o:?}");
            }
            placed.push(r);
        }
        assert!(placed.len() > 30);
        let u = placed[0].union(placed[1]);
        assert!(u.width >= placed[0].width && u.height >= placed[0].height);
    }
}
