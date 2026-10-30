//! Images a skin's load drew, kept a while for the next load of the same
//! skin (`docs/plans/shell-sharing.md`, item 1): the start prompt's
//! diagram and the play ready one skin twice, for two layouts and two
//! Lua environments, from the same documents drawn at the same sizes.
//! Each shell keeps its own kind of image (desktop's RGBA, the web's
//! bitmaps) by the drawn document's hash and its raster size.
//!
//! Bounded twice: by pixels, one skin's whole budget ([`MAX_PIXELS`]),
//! and by time, an image unused for [`RECENT_SECONDS`] let go. Room is
//! made from other skins' images, least recently used first, never from
//! the skin loading's own: a skin larger than the cap has its first
//! images kept and the rest drawn each time, where evicting its own
//! would keep none of them for the next load (which asks in the same
//! order).

use std::collections::HashMap;

use crate::build::MAX_PIXELS;

/// The pixels kept at most.
pub const RECENT_PIXELS: u64 = MAX_PIXELS;

/// How long an image unused is kept, seconds.
pub const RECENT_SECONDS: f64 = 30.0;

/// A drawn image: the document's hash (`report::hash`) and the raster's
/// size.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageKey {
    pub hash: String,
    pub at: (u32, u32),
}

struct Recent<T> {
    image: T,
    pixels: u64,
    /// The skin that last used it.
    owner: String,
    used_at: f64,
    /// When it was last used, in uses.
    tick: u64,
}

/// Images kept by [`ImageKey`].
pub struct RecentImages<T> {
    cap: u64,
    age: f64,
    pixels: u64,
    tick: u64,
    entries: HashMap<ImageKey, Recent<T>>,
}

impl<T> Default for RecentImages<T> {
    fn default() -> RecentImages<T> {
        RecentImages::new(RECENT_PIXELS, RECENT_SECONDS)
    }
}

impl<T> RecentImages<T> {
    /// At most `cap` pixels, each kept `age` seconds after its last use.
    pub fn new(cap: u64, age: f64) -> RecentImages<T> {
        RecentImages {
            cap,
            age,
            pixels: 0,
            tick: 0,
            entries: HashMap::new(),
        }
    }

    /// The image kept for `key`, now used by skin `owner` at `now`
    /// (seconds, on any clock the shell keeps).
    pub fn get(&mut self, owner: &str, key: &ImageKey, now: f64) -> Option<&T> {
        self.tick += 1;
        let e = self.entries.get_mut(key)?;
        e.used_at = now;
        e.tick = self.tick;
        if e.owner != owner {
            e.owner = owner.to_string();
        }
        Some(&e.image)
    }

    /// Keeps `image` of `pixels` for `key`, drawn by skin `owner` at
    /// `now`; what was let go for it (too old, another skin's, one kept
    /// for the same key), or `image` itself when it is not kept (larger
    /// than the room other skins' images leave).
    pub fn insert(
        &mut self,
        owner: &str,
        key: ImageKey,
        image: T,
        pixels: u64,
        now: f64,
    ) -> Vec<T> {
        let mut out = self.trim(now);
        if let Some(old) = self.entries.remove(&key) {
            self.pixels -= old.pixels;
            out.push(old.image);
        }
        // The room other skins' images leave: none let go for nothing.
        let own: u64 = self
            .entries
            .values()
            .filter(|e| e.owner == owner)
            .map(|e| e.pixels)
            .sum();
        if own + pixels > self.cap {
            out.push(image);
            return out;
        }
        while self.pixels + pixels > self.cap {
            let oldest = self
                .entries
                .iter()
                .filter(|(_, e)| e.owner != owner)
                .min_by_key(|(_, e)| e.tick)
                .map(|(k, _)| k.clone());
            let Some(k) = oldest else {
                out.push(image);
                return out;
            };
            if let Some(e) = self.entries.remove(&k) {
                self.pixels -= e.pixels;
                out.push(e.image);
            }
        }
        self.tick += 1;
        self.pixels += pixels;
        self.entries.insert(
            key,
            Recent {
                image,
                pixels,
                owner: owner.to_string(),
                used_at: now,
                tick: self.tick,
            },
        );
        out
    }

    /// Lets go the images unused for longer than the age at `now`.
    pub fn trim(&mut self, now: f64) -> Vec<T> {
        let age = self.age;
        let old: Vec<ImageKey> = self
            .entries
            .iter()
            .filter(|(_, e)| now - e.used_at > age)
            .map(|(k, _)| k.clone())
            .collect();
        old.into_iter()
            .filter_map(|k| self.entries.remove(&k))
            .map(|e| {
                self.pixels -= e.pixels;
                e.image
            })
            .collect()
    }

    /// Lets every image go.
    pub fn clear(&mut self) -> Vec<T> {
        self.pixels = 0;
        self.entries.drain().map(|(_, e)| e.image).collect()
    }

    /// The pixels kept.
    pub fn pixels(&self) -> u64 {
        self.pixels
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(n: u32) -> ImageKey {
        ImageKey {
            hash: format!("{n:016x}"),
            at: (10, 10),
        }
    }

    fn get(r: &mut RecentImages<u32>, owner: &str, n: u32, now: f64) -> Option<u32> {
        r.get(owner, &key(n), now).copied()
    }

    #[test]
    fn an_image_is_kept_for_the_next_load() {
        let mut r = RecentImages::new(1000, 30.0);
        assert!(r.insert("a", key(1), 1, 100, 0.0).is_empty());
        assert_eq!(get(&mut r, "a", 1, 1.0), Some(1));
        assert_eq!(get(&mut r, "a", 2, 1.0), None);
        assert_eq!((r.len(), r.pixels()), (1, 100));
        // The same key again: the older let go.
        assert_eq!(r.insert("a", key(1), 11, 100, 2.0), [1]);
        assert_eq!((r.len(), r.pixels()), (1, 100));
    }

    #[test]
    fn other_skins_images_make_room_least_recently_used_first() {
        let mut r = RecentImages::new(300, 30.0);
        r.insert("a", key(1), 1, 100, 0.0);
        r.insert("a", key(2), 2, 100, 0.0);
        r.insert("a", key(3), 3, 100, 0.0);
        get(&mut r, "a", 1, 0.5);
        // Skin b: a's images go, 2 first (1 was used since).
        assert_eq!(r.insert("b", key(10), 10, 100, 1.0), [2]);
        assert_eq!(r.insert("b", key(11), 11, 100, 1.0), [3]);
        assert_eq!(r.pixels(), 300);
        assert_eq!(get(&mut r, "a", 1, 1.0), Some(1));
    }

    #[test]
    fn a_skin_larger_than_the_room_keeps_its_first_images() {
        let mut r = RecentImages::new(250, 30.0);
        r.insert("a", key(1), 1, 100, 0.0);
        r.insert("a", key(2), 2, 100, 0.0);
        // Its own are not let go for it: passed through.
        assert_eq!(r.insert("a", key(3), 3, 100, 0.0), [3]);
        // The next load finds the first ones, in the same order.
        assert_eq!(get(&mut r, "a", 1, 1.0), Some(1));
        assert_eq!(get(&mut r, "a", 2, 1.0), Some(2));
        assert_eq!(get(&mut r, "a", 3, 1.0), None);
        // An image larger than all the room.
        assert_eq!(r.insert("b", key(9), 9, 500, 1.0), [9]);
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn images_unused_a_while_go() {
        let mut r = RecentImages::new(1000, 30.0);
        r.insert("a", key(1), 1, 100, 0.0);
        r.insert("a", key(2), 2, 100, 20.0);
        assert_eq!(get(&mut r, "a", 1, 25.0), Some(1));
        // 2 last used at 20, 1 at 25.
        assert_eq!(r.trim(51.0), [2]);
        assert_eq!(r.trim(90.0), [1]);
        assert!(r.is_empty() && r.pixels() == 0);
        r.insert("a", key(3), 3, 100, 90.0);
        assert_eq!(r.clear(), [3]);
        assert_eq!(r.pixels(), 0);
    }
}
