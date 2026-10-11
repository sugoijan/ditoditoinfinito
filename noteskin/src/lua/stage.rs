//! Where a skin's actors live while its Lua can reach them: each loaded
//! element's tree, and where every actor with a Lua handle is in it.

use crate::actor::{Actor, Kind};

#[derive(Default)]
pub(crate) struct Stage {
    /// The elements' trees, by the index handed out by [`Stage::add`].
    roots: Vec<Actor>,
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
        self.roots.push(tree);
        self.roots.len() - 1
    }

    pub fn root(&self, root: usize) -> Option<&Actor> {
        self.roots.get(root)
    }

    fn walk<'s>(mut a: &'s Actor, path: &[u16]) -> Option<&'s Actor> {
        for &i in path {
            a = a.children().get(usize::from(i))?;
        }
        Some(a)
    }

    pub fn get(&self, id: u32) -> Option<&Actor> {
        let (root, path) = self.places.get(id.checked_sub(1)? as usize)?;
        Self::walk(self.roots.get(*root)?, path)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Actor> {
        let (root, path) = self.places.get(id.checked_sub(1)? as usize)?;
        let mut a = self.roots.get_mut(*root)?;
        for &i in path {
            a = match &mut a.kind {
                Kind::Frame(c) => c.get_mut(usize::from(i))?,
                _ => return None,
            };
        }
        Some(a)
    }

    pub fn parent(&self, id: u32) -> Option<u32> {
        let (root, path) = self.places.get(id.checked_sub(1)? as usize)?;
        let (_, up) = path.split_last()?;
        let p = Self::walk(self.roots.get(*root)?, up)?;
        (p.id != 0).then_some(p.id)
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
