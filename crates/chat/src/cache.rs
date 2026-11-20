//! The emote images a device keeps (C4, C9): at most [`KEPT`] of them,
//! [`AT_ONCE`] fetched at once, each asked directly first and through our
//! server's proxy once after a failure (a host that needed the proxy is
//! asked through it for the rest of the session), and their places in one
//! atlas the layer draws from, so the emotes are one batch, not one per
//! emote. The shells fetch and decode (off the frame: the browser's
//! `createImageBitmap`, a thread on desktop) and copy the pixels into the
//! cell they are given; this keeps the books.
//!
//! The atlas is [`ATLAS`] pixels in rows [`CELL`] tall, each row in
//! [`CELL`]-wide units; an image is scaled to the row's height and takes
//! as many units as its width needs, [`WIDEST_UNITS`] at most. When it is
//! full, the images drawn least recently go.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::emotes::{Emote, EmoteSets, Image};
use crate::overlay::Readiness;

/// A cell's height and a unit's width, pixels.
pub const CELL: u32 = 64;
/// The atlas's size, pixels: 32 units by 16 rows.
pub const ATLAS: [u32; 2] = [2048, 1024];
/// Units an image takes at most (an emote four times as wide as tall).
pub const WIDEST_UNITS: u32 = 4;
/// Images kept at most.
pub const KEPT: usize = 256;
/// Fetches in flight at most.
pub const AT_ONCE: usize = 4;

const UNITS: usize = (ATLAS[0] / CELL) as usize;
const ROWS: usize = (ATLAS[1] / CELL) as usize;

/// How an image was asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Via {
    Direct,
    /// Through our server's proxy route.
    Proxy,
}

/// A fetch for a shell to make.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fetch {
    pub emote: Emote,
    pub url: String,
    pub via: Via,
}

/// An image's place in the atlas: `[x, y, width, height]` pixels.
pub type Cell = [u32; 4];

#[derive(Clone, Debug, PartialEq)]
enum State {
    Queued(Via),
    Fetching(Via),
    Ready { cell: Cell },
    Failed,
}

#[derive(Clone, Debug)]
struct Entry {
    image: Option<Image>,
    state: State,
    /// The frame it was last wanted or drawn in.
    used: u64,
}

/// What a fetch's end means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// Copy the pixels (scaled to `size`, [`fit`]) into `cell`.
    Place { cell: Cell },
    /// Asked again through the proxy (on the next [`EmoteCache::start`]).
    Retry,
    /// Never drawn this session.
    Failed,
}

/// The size an image of `size` pixels is drawn into the atlas at: the
/// cell's height, its width in proportion, [`WIDEST_UNITS`] at most.
pub fn fit(size: [u32; 2]) -> [u32; 2] {
    let [w, h] = size.map(|v| v.max(1));
    let width = (u64::from(w) * u64::from(CELL)).div_ceil(u64::from(h)) as u32;
    [width.clamp(1, CELL * WIDEST_UNITS), CELL]
}

/// The emote images of a session.
pub struct EmoteCache {
    entries: HashMap<Emote, Entry>,
    queue: VecDeque<Emote>,
    frame: u64,
    /// Hosts that needed the proxy this session.
    proxied: HashSet<String>,
    /// Our server's API base (`None`: no proxy, as in the scripted chat).
    api: Option<String>,
    units: [[bool; UNITS]; ROWS],
}

impl EmoteCache {
    pub fn new(api: Option<String>) -> EmoteCache {
        EmoteCache {
            entries: HashMap::new(),
            queue: VecDeque::new(),
            frame: 0,
            proxied: HashSet::new(),
            api,
            units: [[false; UNITS]; ROWS],
        }
    }

    /// A new frame: what is wanted or drawn from now on counts as recent.
    pub fn next_frame(&mut self) {
        self.frame += 1;
    }

    /// `emote` is wanted now (an item waits for it) or drawn: kept recent,
    /// and asked for if it is new. Without an image to ask for (none
    /// offline, or none known) it fails at once.
    pub fn want(&mut self, emote: &Emote, sets: &EmoteSets) {
        let frame = self.frame;
        if let Some(e) = self.entries.get_mut(emote) {
            e.used = frame;
            return;
        }
        let image = sets.image(emote);
        let state = if image.is_some() {
            self.queue.push_back(emote.clone());
            State::Queued(self.via_for(image.as_ref()))
        } else {
            State::Failed
        };
        self.entries.insert(
            emote.clone(),
            Entry {
                image,
                state,
                used: frame,
            },
        );
    }

    fn via_for(&self, image: Option<&Image>) -> Via {
        match (image, &self.api) {
            (Some(i), Some(_)) if i.proxy.is_some() && self.proxied.contains(i.host()) => Via::Proxy,
            _ => Via::Direct,
        }
    }

    fn address(&self, image: &Image, via: Via) -> Option<String> {
        match via {
            Via::Direct => Some(image.url.clone()),
            Via::Proxy => image.proxied(self.api.as_deref()?),
        }
    }

    /// The fetches to start now, so that no more than [`AT_ONCE`] are in
    /// flight.
    pub fn start(&mut self) -> Vec<Fetch> {
        let flying = self
            .entries
            .values()
            .filter(|e| matches!(e.state, State::Fetching(_)))
            .count();
        let mut out = Vec::new();
        while flying + out.len() < AT_ONCE {
            let Some(emote) = self.queue.pop_front() else {
                break;
            };
            let Some(entry) = self.entries.get(&emote) else {
                continue;
            };
            let State::Queued(via) = entry.state else {
                continue;
            };
            let Some(url) = entry.image.as_ref().and_then(|i| self.address(i, via)) else {
                self.entries.get_mut(&emote).expect("there").state = State::Failed;
                continue;
            };
            self.entries.get_mut(&emote).expect("there").state = State::Fetching(via);
            out.push(Fetch { emote, url, via });
        }
        out
    }

    /// A fetch of `emote` ended: decoded to an image of `size` pixels, or
    /// failed (`None`).
    pub fn fetched(&mut self, emote: &Emote, via: Via, size: Option<[u32; 2]>) -> Answer {
        let Some(entry) = self.entries.get(emote) else {
            return Answer::Failed;
        };
        if entry.state != State::Fetching(via) {
            return Answer::Failed;
        }
        let Some(size) = size else {
            // Once through the proxy after a direct failure; its host
            // through the proxy for the rest of the session.
            let retry = via == Via::Direct
                && self.api.is_some()
                && entry.image.as_ref().is_some_and(|i| i.proxy.is_some());
            let host = entry.image.as_ref().map(|i| i.host().to_string());
            let entry = self.entries.get_mut(emote).expect("there");
            if retry {
                entry.state = State::Queued(Via::Proxy);
                self.queue.push_front(emote.clone());
                if let Some(h) = host {
                    self.proxied.insert(h);
                }
                return Answer::Retry;
            }
            entry.state = State::Failed;
            return Answer::Failed;
        };
        let [w, h] = fit(size);
        let units = w.div_ceil(CELL) as usize;
        let place = loop {
            if self.ready_count() < KEPT
                && let Some(p) = self.free_units(units)
            {
                break Some(p);
            }
            if !self.evict() {
                break None;
            }
        };
        let entry = self.entries.get_mut(emote).expect("there");
        let Some((row, unit)) = place else {
            entry.state = State::Failed;
            return Answer::Failed;
        };
        for u in &mut self.units[row][unit..unit + units] {
            *u = true;
        }
        let cell = [unit as u32 * CELL, row as u32 * CELL, w, h];
        entry.state = State::Ready { cell };
        Answer::Place { cell }
    }

    fn ready_count(&self) -> usize {
        self.entries
            .values()
            .filter(|e| matches!(e.state, State::Ready { .. }))
            .count()
    }

    fn free_units(&self, n: usize) -> Option<(usize, usize)> {
        (0..ROWS).find_map(|r| {
            (0..=UNITS - n)
                .find(|&u| self.units[r][u..u + n].iter().all(|t| !t))
                .map(|u| (r, u))
        })
    }

    /// Lets the image drawn least recently go (not one of this frame);
    /// whether one went.
    fn evict(&mut self) -> bool {
        let frame = self.frame;
        let oldest = self
            .entries
            .iter()
            .filter(|(_, e)| matches!(e.state, State::Ready { .. }) && e.used < frame)
            .min_by(|a, b| a.1.used.cmp(&b.1.used).then_with(|| a.0.cmp(b.0)))
            .map(|(k, _)| k.clone());
        let Some(k) = oldest else {
            return false;
        };
        if let Some(Entry {
            state: State::Ready { cell },
            ..
        }) = self.entries.remove(&k)
        {
            let row = (cell[1] / CELL) as usize;
            let unit = (cell[0] / CELL) as usize;
            let n = cell[2].div_ceil(CELL) as usize;
            for u in &mut self.units[row][unit..unit + n] {
                *u = false;
            }
        }
        true
    }

    /// Whether `emote` can be drawn, and at what shape.
    pub fn readiness(&self, emote: &Emote) -> Readiness {
        match self.entries.get(emote).map(|e| &e.state) {
            Some(State::Ready { cell }) => Readiness::Ready {
                aspect: cell[2] as f32 / cell[3].max(1) as f32,
            },
            Some(State::Failed) => Readiness::Failed,
            _ => Readiness::Waiting,
        }
    }

    /// `emote`'s place in the atlas, once ready.
    pub fn cell(&self, emote: &Emote) -> Option<Cell> {
        match self.entries.get(emote).map(|e| &e.state) {
            Some(State::Ready { cell }) => Some(*cell),
            _ => None,
        }
    }

    /// Images ready, fetches in flight, failed; hosts through the proxy.
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        let n = |f: fn(&State) -> bool| self.entries.values().filter(|e| f(&e.state)).count();
        (
            n(|s| matches!(s, State::Ready { .. })),
            n(|s| matches!(s, State::Fetching(_))),
            n(|s| matches!(s, State::Failed)),
            self.proxied.len(),
        )
    }

    /// Where a provider's set is read (C4's rule for the sets too): its
    /// address, through the proxy where its host needed it.
    pub fn set_address(&self, image: &Image) -> (String, Via) {
        let via = self.via_for(Some(image));
        match self.address(image, via) {
            Some(a) => (a, via),
            None => (image.url.clone(), Via::Direct),
        }
    }

    /// A set's direct read failed: its host goes through the proxy for
    /// the session, and the proxy's address to ask now (none without a
    /// proxy for it).
    pub fn set_failed(&mut self, image: &Image) -> Option<String> {
        let api = self.api.as_deref()?;
        let at = image.proxied(api)?;
        self.proxied.insert(image.host().to_string());
        Some(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emotes::{Provider, twitch_image};

    fn sets() -> EmoteSets {
        EmoteSets::default()
    }

    fn e(i: u32) -> Emote {
        Emote::twitch(i.to_string())
    }

    #[test]
    fn four_at_once() {
        let mut c = EmoteCache::new(None);
        for i in 0..10 {
            c.want(&e(i), &sets());
        }
        assert_eq!(c.start().len(), 4);
        assert!(c.start().is_empty());
        assert!(matches!(
            c.fetched(&e(0), Via::Direct, Some([56, 56])),
            Answer::Place { .. }
        ));
        let next = c.start();
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].emote, e(4));
        assert_eq!(
            next[0].url,
            "https://static-cdn.jtvnw.net/emoticons/v2/4/static/dark/2.0"
        );
    }

    #[test]
    fn a_failed_direct_fetch_goes_through_the_proxy_once_and_its_host_after() {
        let mut c = EmoteCache::new(Some("https://api.test/x".into()));
        c.want(&e(1), &sets());
        let f = c.start();
        assert_eq!(f[0].via, Via::Direct);
        assert_eq!(c.fetched(&e(1), Via::Direct, None), Answer::Retry);
        let f = c.start();
        assert_eq!(f[0].via, Via::Proxy);
        assert_eq!(
            f[0].url,
            "https://api.test/x/emotes/twitch/emoticons/v2/1/static/dark/2.0"
        );
        assert_eq!(c.fetched(&e(1), Via::Proxy, None), Answer::Failed);
        assert_eq!(c.readiness(&e(1)), Readiness::Failed);
        // The host goes through the proxy from now on.
        c.want(&e(2), &sets());
        assert_eq!(c.start()[0].via, Via::Proxy);
        // The sets' reads follow the same rule.
        let (at, via) = c.set_address(&twitch_image("9"));
        assert_eq!(via, Via::Proxy);
        assert!(at.starts_with("https://api.test/x/emotes/twitch/"));
    }

    #[test]
    fn without_a_proxy_a_failure_is_final() {
        let mut c = EmoteCache::new(None);
        c.want(&e(1), &sets());
        c.start();
        assert_eq!(c.fetched(&e(1), Via::Direct, None), Answer::Failed);
    }

    #[test]
    fn offline_sets_ask_for_nothing() {
        let mut c = EmoteCache::new(Some("https://api.test".into()));
        c.want(&e(1), &EmoteSets::offline());
        assert!(c.start().is_empty());
        assert_eq!(c.readiness(&e(1)), Readiness::Failed);
        // The scripted chat's own pictures are asked for.
        let mut s = EmoteSets::offline();
        s.set_image(
            Emote::new(Provider::Test, "a"),
            Image {
                url: "shape:circle:ff8800".into(),
                proxy: None,
            },
        );
        c.want(&Emote::new(Provider::Test, "a"), &s);
        assert_eq!(c.start()[0].url, "shape:circle:ff8800");
    }

    #[test]
    fn images_fit_the_row_and_wide_ones_take_more_units() {
        assert_eq!(fit([56, 56]), [64, 64]);
        assert_eq!(fit([168, 56]), [192, 64]);
        assert_eq!(fit([1000, 50]), [256, 64]);
        let mut c = EmoteCache::new(None);
        c.want(&e(1), &sets());
        c.want(&e(2), &sets());
        c.start();
        assert_eq!(
            c.fetched(&e(1), Via::Direct, Some([168, 56])),
            Answer::Place {
                cell: [0, 0, 192, 64]
            }
        );
        assert_eq!(
            c.fetched(&e(2), Via::Direct, Some([28, 28])),
            Answer::Place {
                cell: [192, 0, 64, 64]
            }
        );
        assert_eq!(c.readiness(&e(1)), Readiness::Ready { aspect: 3.0 });
    }

    #[test]
    fn at_most_256_kept_the_least_recent_going() {
        let mut c = EmoteCache::new(None);
        for i in 0..300u32 {
            c.next_frame();
            c.want(&e(i), &sets());
            let f = c.start();
            for f in f {
                let a = c.fetched(&f.emote, f.via, Some([56, 56]));
                assert!(matches!(a, Answer::Place { .. }), "{i}: {a:?}");
            }
        }
        let (ready, ..) = c.counts();
        assert_eq!(ready, KEPT);
        // The oldest went, the newest stayed.
        assert_eq!(c.readiness(&e(0)), Readiness::Waiting);
        assert!(matches!(c.readiness(&e(299)), Readiness::Ready { .. }));
        // An image drawn this frame is never the one to go.
        c.next_frame();
        c.want(&e(100), &sets());
        c.want(&e(1000), &sets());
        for f in c.start() {
            c.fetched(&f.emote, f.via, Some([56, 56]));
        }
        assert!(matches!(c.readiness(&e(100)), Readiness::Ready { .. }));
    }
}
