//! Textures by source, cached (`docs/plans/ui-core.md`, P9): banners,
//! jackets, theme art and backgrounds are asked for by source (a bundled
//! path, an imported file's key, a theme's file) and the size they are
//! drawn at, decoded by the platform off the main thread (the web's
//! `createImageBitmap`, decision 16) and kept under a byte budget,
//! least recently used first out.
//!
//! The cache is bookkeeping only, over whatever the renderer keeps per
//! texture (`T`): a draw asks [`TextureCache::get`] for an id, which is
//! `None` (the caller draws a placeholder) until the platform has
//! answered [`TextureCache::take_requests`] with [`TextureCache::insert`].
//! Ids are handed out once per key and never change: an evicted texture,
//! or every texture after a lost device ([`TextureCache::unload_all`]),
//! is loaded again from its source the next time it is drawn.

use std::collections::HashMap;
use std::sync::Arc;

/// A texture's stable id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureId(pub u32);

/// What a texture shows, and how large it is kept.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextureKey {
    /// Where the platform reads it from (a site path, an imported file's
    /// key, …); the platform gives it meaning.
    pub source: Arc<str>,
    /// Its longest side is scaled down to this many pixels (0: as it is,
    /// within the device's limit).
    pub max_side: u32,
}

/// The sides [`TextureKey::drawn`] rounds up to: a texture is decoded at
/// a few sizes, not at every size it is drawn.
const SIDES: [u32; 6] = [64, 128, 256, 512, 1024, 2048];

impl TextureKey {
    /// `source` for drawing at most `side` physical pixels long: the next
    /// of a few sizes up, so a banner drawn at 300 px and at 310 px share
    /// one texture.
    pub fn drawn(source: impl Into<Arc<str>>, side: f32) -> TextureKey {
        let max_side = SIDES
            .iter()
            .copied()
            .find(|&s| s as f32 >= side)
            .unwrap_or(SIDES[SIDES.len() - 1]);
        TextureKey {
            source: source.into(),
            max_side,
        }
    }
}

/// Where a texture stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureState {
    /// Not loaded (never drawn, evicted, or the device was lost).
    Unloaded,
    /// Asked of the platform.
    Loading,
    Ready,
    /// The platform could not load it: never asked again.
    Failed,
}

enum Slot<T> {
    Unloaded,
    Loading,
    Ready { item: T, bytes: u64 },
    Failed,
}

struct Entry<T> {
    key: TextureKey,
    slot: Slot<T>,
    last_used: u64,
}

/// Counters for the debug state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextureStats {
    /// Bytes of the textures loaded.
    pub bytes: u64,
    pub budget: u64,
    /// Ids handed out.
    pub entries: usize,
    pub loaded: usize,
    pub loading: usize,
    pub failed: usize,
    /// Textures dropped for the budget, since the start.
    pub evictions: u64,
    /// Textures loaded, since the start.
    pub loads: u64,
}

pub struct TextureCache<T> {
    ids: HashMap<TextureKey, TextureId>,
    entries: Vec<Entry<T>>,
    requests: Vec<TextureId>,
    budget: u64,
    bytes: u64,
    frame: u64,
    evictions: u64,
    loads: u64,
}

impl<T> TextureCache<T> {
    /// A cache keeping about `budget` bytes of textures (more while
    /// everything over it is drawn in the current frame).
    pub fn new(budget: u64) -> TextureCache<T> {
        TextureCache {
            ids: HashMap::new(),
            entries: Vec::new(),
            requests: Vec::new(),
            budget,
            bytes: 0,
            frame: 0,
            evictions: 0,
            loads: 0,
        }
    }

    pub fn set_budget(&mut self, budget: u64) {
        self.budget = budget;
    }

    /// The id of `key`, the same every time.
    pub fn id(&mut self, key: TextureKey) -> TextureId {
        if let Some(&id) = self.ids.get(&key) {
            return id;
        }
        let id = TextureId(self.entries.len() as u32);
        self.entries.push(Entry {
            key: key.clone(),
            slot: Slot::Unloaded,
            last_used: 0,
        });
        self.ids.insert(key, id);
        id
    }

    pub fn key(&self, id: TextureId) -> Option<&TextureKey> {
        self.entries.get(id.0 as usize).map(|e| &e.key)
    }

    pub fn state(&self, id: TextureId) -> Option<TextureState> {
        self.entries.get(id.0 as usize).map(|e| match e.slot {
            Slot::Unloaded => TextureState::Unloaded,
            Slot::Loading => TextureState::Loading,
            Slot::Ready { .. } => TextureState::Ready,
            Slot::Failed => TextureState::Failed,
        })
    }

    /// A new frame: textures drawn from now on count as in use.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
    }

    /// The texture of `id` for drawing in this frame, or `None` while it
    /// loads (asked of the platform the first time).
    pub fn get(&mut self, id: TextureId) -> Option<&T> {
        let frame = self.frame;
        let entry = self.entries.get_mut(id.0 as usize)?;
        entry.last_used = frame;
        if matches!(entry.slot, Slot::Unloaded) {
            entry.slot = Slot::Loading;
            self.requests.push(id);
        }
        self.peek(id)
    }

    /// The texture of `id` if it is loaded, not counted as drawn.
    pub fn peek(&self, id: TextureId) -> Option<&T> {
        match &self.entries.get(id.0 as usize)?.slot {
            Slot::Ready { item, .. } => Some(item),
            _ => None,
        }
    }

    /// What the platform is to load: each id once until it answers.
    pub fn take_requests(&mut self) -> Vec<(TextureId, TextureKey)> {
        std::mem::take(&mut self.requests)
            .into_iter()
            .filter_map(|id| Some((id, self.key(id)?.clone())))
            .collect()
    }

    /// The platform loaded `id` into `item`, of `bytes`. Returns what it
    /// replaces and what the budget evicts, for the renderer to free.
    pub fn insert(&mut self, id: TextureId, item: T, bytes: u64) -> Vec<T> {
        let mut freed = Vec::new();
        let Some(entry) = self.entries.get_mut(id.0 as usize) else {
            freed.push(item);
            return freed;
        };
        if let Slot::Ready { item: old, bytes } =
            std::mem::replace(&mut entry.slot, Slot::Ready { item, bytes })
        {
            self.bytes -= bytes;
            freed.push(old);
        }
        // Loaded now: it stays at least through the next frame.
        entry.last_used = self.frame;
        self.bytes += bytes;
        self.loads += 1;
        freed.extend(self.evict());
        freed
    }

    /// The platform could not load `id`.
    pub fn fail(&mut self, id: TextureId) {
        if let Some(entry) = self.entries.get_mut(id.0 as usize)
            && matches!(entry.slot, Slot::Loading)
        {
            entry.slot = Slot::Failed;
        }
    }

    /// Drops the least recently used textures not drawn in this frame
    /// until the rest fit the budget; returns them, for the renderer to
    /// free.
    pub fn evict(&mut self) -> Vec<T> {
        let mut freed = Vec::new();
        if self.bytes <= self.budget {
            return freed;
        }
        let mut ready: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| matches!(e.slot, Slot::Ready { .. }) && e.last_used < self.frame)
            .map(|(i, _)| i)
            .collect();
        ready.sort_by_key(|&i| self.entries[i].last_used);
        for i in ready {
            if self.bytes <= self.budget {
                break;
            }
            if let Slot::Ready { item, bytes } =
                std::mem::replace(&mut self.entries[i].slot, Slot::Unloaded)
            {
                self.bytes -= bytes;
                self.evictions += 1;
                freed.push(item);
            }
        }
        freed
    }

    /// Every texture is gone (a lost device): all are loaded again when
    /// drawn, under the same ids. Loads in flight stay asked for (their
    /// decoded images go to the new device). Returns the textures.
    pub fn unload_all(&mut self) -> Vec<T> {
        let mut freed = Vec::new();
        for e in &mut self.entries {
            if matches!(e.slot, Slot::Ready { .. })
                && let Slot::Ready { item, .. } = std::mem::replace(&mut e.slot, Slot::Unloaded)
            {
                freed.push(item);
            }
        }
        self.bytes = 0;
        freed
    }

    pub fn stats(&self) -> TextureStats {
        let count = |f: fn(&Slot<T>) -> bool| self.entries.iter().filter(|e| f(&e.slot)).count();
        TextureStats {
            bytes: self.bytes,
            budget: self.budget,
            entries: self.entries.len(),
            loaded: count(|s| matches!(s, Slot::Ready { .. })),
            loading: count(|s| matches!(s, Slot::Loading)),
            failed: count(|s| matches!(s, Slot::Failed)),
            evictions: self.evictions,
            loads: self.loads,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(s: &str) -> TextureKey {
        TextureKey {
            source: s.into(),
            max_side: 0,
        }
    }

    #[test]
    fn ids_are_stable_per_source_and_size() {
        let mut c = TextureCache::<()>::new(1000);
        let a = c.id(key("a.png"));
        let b = c.id(key("b.png"));
        assert_ne!(a, b);
        assert_eq!(c.id(key("a.png")), a);
        // The same source at another size is another texture.
        let small = c.id(TextureKey::drawn("a.png", 100.0));
        let large = c.id(TextureKey::drawn("a.png", 300.0));
        assert_ne!(small, large);
        assert_eq!(c.key(small).unwrap().max_side, 128);
        assert_eq!(c.key(large).unwrap().max_side, 512);
        assert_eq!(TextureKey::drawn("x", 9000.0).max_side, 2048);
        assert_eq!(c.id(TextureKey::drawn("a.png", 120.0)), small);
    }

    #[test]
    fn a_texture_is_asked_for_once_and_shown_when_loaded() {
        let mut c = TextureCache::<&str>::new(1000);
        let a = c.id(key("a"));
        c.begin_frame();
        assert_eq!(c.get(a), None);
        assert_eq!(c.get(a), None);
        assert_eq!(c.take_requests(), vec![(a, key("a"))]);
        assert!(c.take_requests().is_empty());
        assert_eq!(c.state(a), Some(TextureState::Loading));
        assert!(c.insert(a, "A", 400).is_empty());
        c.begin_frame();
        assert_eq!(c.get(a), Some(&"A"));
        assert_eq!(c.stats().bytes, 400);
        // A failure is never asked again.
        let b = c.id(key("b"));
        assert_eq!(c.get(b), None);
        c.take_requests();
        c.fail(b);
        assert_eq!(c.get(b), None);
        assert!(c.take_requests().is_empty());
        assert_eq!(c.stats().failed, 1);
    }

    #[test]
    fn the_least_recently_drawn_go_first_and_never_this_frames() {
        let mut c = TextureCache::<u32>::new(250);
        let ids: Vec<TextureId> = (0..4).map(|i| c.id(key(&i.to_string()))).collect();
        // Frame 1: 0 and 1 drawn and loaded; frame 2: 2 drawn; frame 3: 0.
        c.begin_frame();
        c.get(ids[0]);
        c.get(ids[1]);
        assert!(c.insert(ids[0], 0, 100).is_empty());
        assert!(c.insert(ids[1], 1, 100).is_empty());
        c.begin_frame();
        c.get(ids[2]);
        // Over budget: the oldest (0 and 1 tie at frame 1; 0 is first) goes.
        assert_eq!(c.insert(ids[2], 2, 100), vec![0]);
        c.take_requests();
        assert_eq!(c.state(ids[0]), Some(TextureState::Unloaded));
        c.begin_frame();
        // Drawn again: asked again, under the same id.
        assert_eq!(c.get(ids[0]), None);
        assert_eq!(c.take_requests(), vec![(ids[0], key("0"))]);
        c.get(ids[2]);
        c.get(ids[1]);
        // Everything is drawn this frame: over budget rather than dropping
        // what is on screen.
        assert!(c.insert(ids[0], 10, 100).is_empty());
        assert_eq!(c.stats().bytes, 300);
        c.begin_frame();
        c.get(ids[0]);
        c.get(ids[2]);
        assert_eq!(c.evict(), vec![1]);
        let s = c.stats();
        assert_eq!((s.bytes, s.loaded, s.evictions, s.loads), (200, 2, 2, 4));
    }

    #[test]
    fn a_lost_device_unloads_everything_under_the_same_ids() {
        let mut c = TextureCache::<u32>::new(1 << 20);
        let a = c.id(key("a"));
        let b = c.id(key("b"));
        c.begin_frame();
        c.get(a);
        c.get(b);
        c.take_requests();
        c.insert(a, 1, 10);
        // b is still loading when the device goes: its image goes to the
        // new device.
        assert_eq!(c.unload_all(), vec![1]);
        assert_eq!(c.stats().bytes, 0);
        assert_eq!(c.state(b), Some(TextureState::Loading));
        c.begin_frame();
        assert_eq!(c.get(a), None);
        assert_eq!(c.take_requests(), vec![(a, key("a"))]);
        c.insert(b, 2, 10);
        assert_eq!(c.get(b), Some(&2));
        assert_eq!(c.id(key("a")), a);
    }
}
