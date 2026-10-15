//! Images for actors, by source (`docs/plans/ui-core.md`, P9): a widget
//! asks for an image by where it comes from (a site path, `files:<key>`,
//! a file) and the longest side it is drawn at, and gets an actor
//! texture id to put on a sprite. The shell maps the id back to its
//! source when it draws ([`crate::Ui::texture_source`]) and hands that to
//! its texture cache, which loads only what is drawn, shows nothing (the
//! component's placeholder shows through) until the image is there, and
//! evicts what is not drawn any more. So a row far away loads nothing.

use std::collections::HashMap;
use std::sync::Arc;

/// Sources by id.
#[derive(Debug, Default)]
pub struct TextureTable {
    ids: HashMap<(Arc<str>, u32), u32>,
    sources: Vec<(Arc<str>, f32)>,
}

/// The sides sizes round up to, so a banner drawn at 300 and at 310
/// pixels is one texture.
const SIDES: [f32; 6] = [64.0, 128.0, 256.0, 512.0, 1024.0, 2048.0];

impl TextureTable {
    /// The id of `source` drawn at most `side` virtual pixels long.
    pub fn id(&mut self, source: &str, side: f32) -> u32 {
        let side = SIDES
            .iter()
            .copied()
            .find(|&s| s >= side)
            .unwrap_or(SIDES[SIDES.len() - 1]);
        let source: Arc<str> = source.into();
        let key = (source.clone(), side as u32);
        if let Some(&id) = self.ids.get(&key) {
            return id;
        }
        let id = self.sources.len() as u32 + 1;
        self.sources.push((source, side));
        self.ids.insert(key, id);
        id
    }

    /// The source and side of id `id`.
    pub fn source(&self, id: u32) -> Option<(Arc<str>, f32)> {
        let i = id.checked_sub(1)? as usize;
        self.sources.get(i).cloned()
    }

    pub fn len(&self) -> usize {
        self.sources.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_stable_by_source_and_size_bucket() {
        let mut t = TextureTable::default();
        let a = t.id("songs/a/bn.png", 200.0);
        assert_eq!(t.id("songs/a/bn.png", 250.0), a);
        assert_ne!(t.id("songs/a/bn.png", 300.0), a);
        assert_eq!(t.source(a).unwrap().1, 256.0);
        assert_eq!(t.source(0), None);
        assert_eq!(t.len(), 2);
    }
}
