//! Where a skin's actors live while its Lua can reach them: each loaded
//! element's tree, and where every actor with a Lua handle is in it.
//!
//! A play holds the trees it draws ([`Stage::reclaim`]); while Lua runs
//! it lends them back ([`Stage::lend`]), so there is one copy of each
//! actor and Lua's methods act on what is drawn. While a tree is out, Lua
//! cannot reach its actors ("the actor is gone"). An actor's place is the
//! path of child indices to it, recorded again when a frame sorts its
//! children ([`Stage::reindex`]).

use ddi_actor::Actor;

#[derive(Default)]
pub(crate) struct Stage {
    /// The elements' trees, by the index handed out by [`Stage::add`]
    /// (a placeholder while the play holds one). Boxed: lending swaps
    /// the boxes, not the trees.
    #[allow(clippy::vec_box)]
    roots: Vec<Box<Actor>>,
    /// Whether the play holds tree `i`.
    out: Vec<bool>,
    /// For actor id `i + 1`: its tree and the child indices down to it.
    places: Vec<(usize, Vec<u16>)>,
}

impl Stage {
    /// The id the next actor will get.
    pub fn next_id(&self) -> u32 {
        self.places.len() as u32 + 1
    }

    /// Records where actor `id` (the next one) will be.
    pub fn place(&mut self, id: u32, root: usize, path: Vec<u16>) {
        debug_assert_eq!(id, self.next_id());
        self.places.push((root, path));
    }

    /// The index the next tree will get.
    pub fn next_root(&self) -> usize {
        self.roots.len()
    }

    pub fn add(&mut self, tree: Actor) -> usize {
        self.roots.push(Box::new(tree));
        self.out.push(false);
        self.roots.len() - 1
    }

    /// The tree of `tree`'s root id, if `tree` is a root here and is
    /// `out` (held by the play) or not.
    fn slot(&self, tree: &Actor, out: bool) -> Option<usize> {
        let (root, path) = self.places.get(tree.id.checked_sub(1)? as usize)?;
        (path.is_empty() && self.out.get(*root) == Some(&out)).then_some(*root)
    }

    /// Takes the stage's tree for the play, giving `tree` (a copy, or a
    /// placeholder the play got back from [`Stage::lend`]) in exchange.
    /// False when `tree` is no root here or the play holds it already.
    pub fn reclaim(&mut self, tree: &mut Box<Actor>) -> bool {
        self.exchange(tree, false)
    }

    /// Lends the play's tree back to the stage while Lua runs. False when
    /// `tree` is no root here or the stage has it already.
    pub fn lend(&mut self, tree: &mut Box<Actor>) -> bool {
        self.exchange(tree, true)
    }

    fn exchange(&mut self, tree: &mut Box<Actor>, out: bool) -> bool {
        let Some(root) = self.slot(tree, out) else {
            return false;
        };
        match (self.roots.get_mut(root), self.out.get_mut(root)) {
            (Some(slot), Some(flag)) => {
                std::mem::swap(slot, tree);
                *flag = !out;
                true
            }
            _ => false,
        }
    }

    /// Whether tree `root` is on the stage (not held by the play).
    fn here(&self, root: usize) -> bool {
        self.out.get(root) == Some(&false)
    }

    fn walk<'s>(mut a: &'s Actor, path: &[u16]) -> Option<&'s Actor> {
        for &i in path {
            a = a.children().get(usize::from(i))?;
        }
        Some(a)
    }

    pub fn get(&self, id: u32) -> Option<&Actor> {
        let (root, path) = self.places.get(id.checked_sub(1)? as usize)?;
        if !self.here(*root) {
            return None;
        }
        Self::walk(self.roots.get(*root)?, path)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Actor> {
        let (root, path) = self.places.get(id.checked_sub(1)? as usize)?;
        if !self.here(*root) {
            return None;
        }
        let mut a: &mut Actor = self.roots.get_mut(*root)?;
        for &i in path {
            a = a.children_mut().get_mut(usize::from(i))?;
        }
        Some(a)
    }

    pub fn parent(&self, id: u32) -> Option<u32> {
        let (root, path) = self.places.get(id.checked_sub(1)? as usize)?;
        if !self.here(*root) {
            return None;
        }
        let (_, up) = path.split_last()?;
        let p = Self::walk(self.roots.get(*root)?, up)?;
        (p.id != 0).then_some(p.id)
    }

    /// Records again where the actors inside actor `id` are, after its
    /// children moved (a frame sorted them).
    pub fn reindex(&mut self, id: u32) {
        fn visit(a: &Actor, path: &mut Vec<u16>, found: &mut Vec<(u32, Vec<u16>)>) {
            for (i, c) in a.children().iter().enumerate() {
                path.push(u16::try_from(i).unwrap_or(u16::MAX));
                if c.id != 0 {
                    found.push((c.id, path.clone()));
                }
                visit(c, path, found);
                path.pop();
            }
        }
        let Some((root, path)) = id
            .checked_sub(1)
            .and_then(|i| self.places.get(i as usize))
            .cloned()
        else {
            return;
        };
        if !self.here(root) {
            return;
        }
        let mut found = Vec::new();
        if let Some(a) = self.roots.get(root).and_then(|r| Self::walk(r, &path)) {
            visit(a, &mut path.clone(), &mut found);
        }
        for (child, path) in found {
            if let Some(place) = self.places.get_mut(child as usize - 1) {
                *place = (root, path);
            }
        }
    }

    /// The ids of an actor's children, in order.
    pub fn children(&self, id: u32) -> Vec<u32> {
        self.get(id).map_or_else(Vec::new, |a| {
            a.children()
                .iter()
                .map(|c| c.id)
                .filter(|&c| c != 0)
                .collect()
        })
    }
}
