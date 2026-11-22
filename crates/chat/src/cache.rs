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
//!
//! An animated emote's frames (round TW4, [`crate::anim`]) are each a cell
//! of their own, all animated emotes' frames within [`ANIMATED_UNITS`]: a
//! new animation that does not fit sends the least recently drawn
//! animated emote away (never one drawn this frame), and where none can
//! go it is placed as a still, its first frame in one cell.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::emotes::{Emote, EmoteSets, Image};
use crate::overlay::Readiness;

/// A cell's height and a unit's width, pixels.
pub const CELL: u32 = 64;
/// The atlas's size, pixels: 32 units by 32 rows (16 MiB, RGBA).
pub const ATLAS: [u32; 2] = [2048, 2048];
/// Units an image takes at most (an emote four times as wide as tall).
pub const WIDEST_UNITS: u32 = 4;
/// Images kept at most.
pub const KEPT: usize = 256;
/// Fetches in flight at most.
pub const AT_ONCE: usize = 4;
/// The units all animated emotes' frames take at most: three quarters of
/// the atlas (12 MiB).
pub const ANIMATED_UNITS: usize = 768;

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
    /// Its frames' cells (one for a still image) and how long each
    /// shows, milliseconds.
    Ready { cells: Vec<Cell>, durations: Vec<u32> },
    Failed,
}

#[derive(Clone, Debug)]
struct Entry {
    image: Option<Image>,
    state: State,
    /// The frame it was last wanted or drawn in.
    used: u64,
}

/// A fetch decoded: the size of its frames (scaled to [`fit`]) and each
/// frame's duration, milliseconds (one frame: a still image).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Got {
    pub size: [u32; 2],
    pub durations: Vec<u32>,
}

impl Got {
    /// A still image of `size`.
    pub fn still(size: [u32; 2]) -> Got {
        Got {
            size,
            durations: vec![0],
        }
    }
}

/// What a fetch's end means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// Copy frame `k`'s pixels (scaled to `size`, [`fit`]) into `cells[k]`:
    /// every frame, or only the first where the animation had no room
    /// (one cell).
    Place { cells: Vec<Cell> },
    /// Asked again through the proxy (on the next [`EmoteCache::start`]).
    Retry,
    /// Never drawn this session.
    Failed,
}

/// The host an emote's image is counted by (C4's "a host that needed the
/// proxy"): its provider's image host, else its address's (round TW5:
/// so that a test's local addresses keep the providers apart).
fn host(emote: &Emote, image: &Image) -> String {
    match emote.provider.image_host() {
        "" => image.host().to_string(),
        h => h.to_string(),
    }
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
            State::Queued(self.via_for(emote, image.as_ref()))
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

    fn via_for(&self, emote: &Emote, image: Option<&Image>) -> Via {
        match (image, &self.api) {
            (Some(i), Some(_)) if i.proxy.is_some() && self.proxied.contains(&host(emote, i)) => Via::Proxy,
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

    /// A fetch of `emote` ended: decoded to frames of a size (`got`), or
    /// failed (`None`).
    pub fn fetched(&mut self, emote: &Emote, via: Via, got: Option<Got>) -> Answer {
        let Some(entry) = self.entries.get(emote) else {
            return Answer::Failed;
        };
        if entry.state != State::Fetching(via) {
            return Answer::Failed;
        }
        let Some(got) = got else {
            // Once through the proxy after a direct failure; its host
            // through the proxy for the rest of the session.
            let retry = via == Via::Direct
                && self.api.is_some()
                && entry.image.as_ref().is_some_and(|i| i.proxy.is_some());
            let host = entry.image.as_ref().map(|i| host(emote, i));
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
        let [w, h] = fit(got.size);
        let units = w.div_ceil(CELL) as usize;
        let frames = got.durations.len().max(1);
        // The animation's frames where they fit its budget (making room
        // from the least recently drawn), else its first frame still.
        let mut places = None;
        if frames > 1 {
            loop {
                let room = self.animated_units() + frames * units <= ANIMATED_UNITS;
                if room
                    && self.ready_count() < KEPT
                    && let Some(p) = self.free_runs(units, frames)
                {
                    places = Some(p);
                    break;
                }
                if !self.evict(!room) {
                    break;
                }
            }
        }
        let (places, durations) = match places {
            Some(p) => (p, got.durations),
            None => {
                let one = loop {
                    if self.ready_count() < KEPT
                        && let Some(p) = self.free_units(units)
                    {
                        break Some(p);
                    }
                    if !self.evict(false) {
                        break None;
                    }
                };
                (one.into_iter().collect(), vec![0])
            }
        };
        let entry = self.entries.get_mut(emote).expect("there");
        if places.is_empty() {
            entry.state = State::Failed;
            return Answer::Failed;
        }
        let mut cells = Vec::with_capacity(places.len());
        for (row, unit) in places {
            for u in &mut self.units[row][unit..unit + units] {
                *u = true;
            }
            cells.push([unit as u32 * CELL, row as u32 * CELL, w, h]);
        }
        entry.state = State::Ready {
            cells: cells.clone(),
            durations,
        };
        Answer::Place { cells }
    }

    /// The units animated emotes' frames take now.
    fn animated_units(&self) -> usize {
        self.entries
            .values()
            .filter_map(|e| match &e.state {
                State::Ready { cells, .. } if cells.len() > 1 => {
                    Some(cells.iter().map(|c| c[2].div_ceil(CELL) as usize).sum::<usize>())
                }
                _ => None,
            })
            .sum()
    }

    /// `count` free runs of `n` units, none overlapping.
    fn free_runs(&self, n: usize, count: usize) -> Option<Vec<(usize, usize)>> {
        let mut taken = self.units;
        let mut out = Vec::with_capacity(count);
        for (r, row) in taken.iter_mut().enumerate() {
            let mut u = 0;
            while u + n <= UNITS && out.len() < count {
                if row[u..u + n].iter().all(|t| !t) {
                    for t in &mut row[u..u + n] {
                        *t = true;
                    }
                    out.push((r, u));
                    u += n;
                } else {
                    u += 1;
                }
            }
        }
        (out.len() == count).then_some(out)
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

    /// Lets the image drawn least recently go (not one of this frame; an
    /// animated one where `animated`); whether one went.
    fn evict(&mut self, animated: bool) -> bool {
        let frame = self.frame;
        let oldest = self
            .entries
            .iter()
            .filter(|(_, e)| match &e.state {
                State::Ready { cells, .. } => e.used < frame && (!animated || cells.len() > 1),
                _ => false,
            })
            .min_by(|a, b| a.1.used.cmp(&b.1.used).then_with(|| a.0.cmp(b.0)))
            .map(|(k, _)| k.clone());
        let Some(k) = oldest else {
            return false;
        };
        if let Some(Entry {
            state: State::Ready { cells, .. },
            ..
        }) = self.entries.remove(&k)
        {
            for cell in cells {
                let row = (cell[1] / CELL) as usize;
                let unit = (cell[0] / CELL) as usize;
                let n = cell[2].div_ceil(CELL) as usize;
                for u in &mut self.units[row][unit..unit + n] {
                    *u = false;
                }
            }
        }
        true
    }

    /// Whether `emote` can be drawn, and at what shape.
    pub fn readiness(&self, emote: &Emote) -> Readiness {
        match self.entries.get(emote).map(|e| &e.state) {
            Some(State::Ready { cells, .. }) => Readiness::Ready {
                aspect: cells[0][2] as f32 / cells[0][3].max(1) as f32,
            },
            Some(State::Failed) => Readiness::Failed,
            _ => Readiness::Waiting,
        }
    }

    /// `emote`'s place in the atlas, once ready: its first frame's.
    pub fn cell(&self, emote: &Emote) -> Option<Cell> {
        self.cell_at(emote, 0)
    }

    /// `emote`'s frame's place at `ms` of the layer's animation clock
    /// ([`crate::anim::Clock`]): the same for every copy of it.
    pub fn cell_at(&self, emote: &Emote, ms: u64) -> Option<Cell> {
        match self.entries.get(emote).map(|e| &e.state) {
            Some(State::Ready { cells, durations }) => {
                cells.get(crate::anim::frame_at(durations, ms)).or(cells.first()).copied()
            }
            _ => None,
        }
    }

    /// Animated emotes ready, and the atlas units their frames take.
    pub fn animated(&self) -> (usize, usize) {
        let n = self
            .entries
            .values()
            .filter(|e| matches!(&e.state, State::Ready { cells, .. } if cells.len() > 1))
            .count();
        (n, self.animated_units())
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

    /// Images ready, fetches in flight, failed, and animated ones ready,
    /// by provider (the global-emotes demo's report, round TW5).
    pub fn counts_by(&self, provider: crate::emotes::Provider) -> [usize; 4] {
        let mut out = [0; 4];
        for (_, entry) in self.entries.iter().filter(|(e, _)| e.provider == provider) {
            match &entry.state {
                State::Ready { cells, .. } => {
                    out[0] += 1;
                    out[3] += usize::from(cells.len() > 1);
                }
                State::Fetching(_) | State::Queued(_) => out[1] += 1,
                State::Failed => out[2] += 1,
            }
        }
        out
    }

    /// The hosts that needed the proxy this session, sorted.
    pub fn proxied_hosts(&self) -> Vec<String> {
        let mut v: Vec<String> = self.proxied.iter().cloned().collect();
        v.sort();
        v
    }

    /// Where a provider's set is read (C4's rule for the sets too): its
    /// address, through the proxy where its host needed it.
    pub fn set_address(&self, image: &Image) -> (String, Via) {
        let via = match &self.api {
            Some(_) if image.proxy.is_some() && self.proxied.contains(image.host()) => Via::Proxy,
            _ => Via::Direct,
        };
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
            c.fetched(&e(0), Via::Direct, Some(Got::still([56, 56]))),
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
        let (at, via) = c.set_address(&twitch_image("9").unwrap());
        assert_eq!(via, Via::Proxy);
        assert!(at.starts_with("https://api.test/x/emotes/twitch/"));
    }

    /// Round TW5: the counts by provider and the hosts through the proxy,
    /// for the global-emotes demo's report.
    #[test]
    fn counts_by_provider_and_the_proxied_hosts() {
        let mut c = EmoteCache::new(Some("https://api.test".into()));
        c.want(&e(1), &sets());
        c.want(&e(2), &sets());
        let f = c.start();
        c.fetched(&f[0].emote, f[0].via, Some(moving(3, 50)));
        assert_eq!(c.fetched(&f[1].emote, f[1].via, None), Answer::Retry);
        assert_eq!(c.counts_by(Provider::Twitch), [1, 1, 0, 1]);
        assert_eq!(c.counts_by(Provider::Bttv), [0; 4]);
        assert_eq!(c.proxied_hosts(), ["static-cdn.jtvnw.net"]);
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
            c.fetched(&e(1), Via::Direct, Some(Got::still([168, 56]))),
            Answer::Place {
                cells: vec![[0, 0, 192, 64]]
            }
        );
        assert_eq!(
            c.fetched(&e(2), Via::Direct, Some(Got::still([28, 28]))),
            Answer::Place {
                cells: vec![[192, 0, 64, 64]]
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
                let a = c.fetched(&f.emote, f.via, Some(Got::still([56, 56])));
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
            c.fetched(&f.emote, f.via, Some(Got::still([56, 56])));
        }
        assert!(matches!(c.readiness(&e(100)), Readiness::Ready { .. }));
    }

    /// Round DE: `chat-overlay-v2` lets 480 emotes on screen, the atlas
    /// keeps 256 images. Where more distinct emotes than that are drawn in
    /// one frame (none of them may go), those past 256 are not placed:
    /// their items go without them (`no_image` after the wait), and the
    /// image stays failed for the play. In the bursts measured the most
    /// emotes drawn at once was about 150, of a few dozen distinct ones.
    #[test]
    fn past_256_distinct_images_drawn_at_once_the_rest_go_without() {
        let mut c = EmoteCache::new(None);
        c.next_frame();
        let mut placed = 0;
        let mut failed = 0;
        for i in 0..480u32 {
            c.want(&e(i), &sets());
            for f in c.start() {
                match c.fetched(&f.emote, f.via, Some(Got::still([56, 56]))) {
                    Answer::Place { .. } => placed += 1,
                    Answer::Failed => failed += 1,
                    other => panic!("{other:?}"),
                }
            }
        }
        assert_eq!((placed, failed), (KEPT, 480 - KEPT));
    }

    fn moving(frames: usize, ms: u32) -> Got {
        Got {
            size: [56, 56],
            durations: vec![ms; frames],
        }
    }

    /// Round TW4: an animated emote's frames each a cell, the frame drawn
    /// chosen by the layer's clock alone (every copy alike).
    #[test]
    fn an_animated_emotes_frames_are_cells_drawn_in_turn() {
        let mut c = EmoteCache::new(None);
        c.want(&e(1), &sets());
        c.start();
        let Answer::Place { cells } = c.fetched(&e(1), Via::Direct, Some(moving(4, 100))) else {
            panic!()
        };
        assert_eq!(cells.len(), 4);
        assert_eq!(cells[1], [64, 0, 64, 64]);
        assert_eq!(c.cell_at(&e(1), 0), Some(cells[0]));
        assert_eq!(c.cell_at(&e(1), 250), Some(cells[2]));
        assert_eq!(c.cell_at(&e(1), 450), Some(cells[0]), "it loops");
        assert_eq!(c.cell(&e(1)), Some(cells[0]));
        assert_eq!(c.readiness(&e(1)), Readiness::Ready { aspect: 1.0 });
        assert_eq!(c.animated(), (1, 4));
    }

    /// The animated frames' budget: past it the least recently drawn
    /// animation goes; where every one is drawn this frame, a new one is
    /// placed as a still, its first frame (never nothing).
    #[test]
    fn animations_keep_to_their_budget_and_a_still_first_frame_beyond_it() {
        let mut c = EmoteCache::new(None);
        // Twelve animations of 64 frames fill the budget (768 units).
        for i in 0..12 {
            c.next_frame();
            c.want(&e(i), &sets());
            for f in c.start() {
                let a = c.fetched(&f.emote, f.via, Some(moving(64, 40)));
                assert!(matches!(&a, Answer::Place { cells } if cells.len() == 64), "{i}");
            }
        }
        assert_eq!(c.animated(), (12, ANIMATED_UNITS));
        // A thirteenth sends the least recently drawn (0) away.
        c.next_frame();
        c.want(&e(12), &sets());
        for f in c.start() {
            let a = c.fetched(&f.emote, f.via, Some(moving(64, 40)));
            assert!(matches!(&a, Answer::Place { cells } if cells.len() == 64));
        }
        assert_eq!(c.readiness(&e(0)), Readiness::Waiting);
        assert_eq!(c.animated(), (12, ANIMATED_UNITS));
        // All twelve drawn this frame: the next is still, one cell.
        c.next_frame();
        for i in 1..=12 {
            c.want(&e(i), &sets());
        }
        c.want(&e(13), &sets());
        for f in c.start() {
            let a = c.fetched(&f.emote, f.via, Some(moving(64, 40)));
            assert!(matches!(&a, Answer::Place { cells } if cells.len() == 1), "{a:?}");
        }
        assert_eq!(c.cell_at(&e(13), 0), c.cell_at(&e(13), 1000));
        assert_eq!(c.animated(), (12, ANIMATED_UNITS));
        // Stills still have the rest of the atlas.
        c.want(&e(14), &sets());
        for f in c.start() {
            assert!(matches!(
                c.fetched(&f.emote, f.via, Some(Got::still([56, 56]))),
                Answer::Place { .. }
            ));
        }
    }
}
