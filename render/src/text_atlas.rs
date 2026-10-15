//! Text runs cached in atlas pages (`docs/plans/ui-core.md`, P4).
//!
//! A run (a string in a [`TextStyle`] at a scale) is drawn once, white,
//! into a staging page by whatever draws text on the platform (a
//! [`RunRasterizer`]: the browser's Canvas2D on the web, cosmic-text
//! natively), packed on shelves ([`crate::atlas`]), and copied to the
//! page's texture once per page and frame (step 0 measured a copy per run
//! at 0.2 ms; one per page and frame is what keeps a scroll's new runs
//! under 2 ms). Pages hold coverage alone (`R8Unorm`, a quarter of RGBA
//! and pixel-identical for white text); a run's colour is a tint.
//!
//! The pages cache what is shown, not the library: past a byte budget
//! the page drawn least recently is cleared whole and reused, and its
//! runs drawn again when they are shown again. A page with a run of the
//! current frame is never reused; the budget is exceeded instead.
//!
//! Cells are the run's ink box plus [`PAD`] texels on every side: ink
//! exceeds `measureText`'s bounding box by up to 1.7 px (step 0), and the
//! blank border keeps filtering from reaching a neighbour.

use std::collections::HashMap;
use std::sync::Arc;

use ddi_actor::text::{RunMetrics, TextMeasure, TextStyle};

use crate::atlas::{PageRect, ShelfPacker};

/// A page's side in texels: the largest every WebGL2 device takes.
pub const PAGE: u32 = 2048;

/// Blank texels around a run's measured ink.
pub const PAD: u32 = 2;

/// Draws runs for the cache. Coordinates and metrics are physical pixels:
/// a run is set at `style.size() × scale`.
pub trait RunRasterizer {
    /// What `text` occupies at `scale`; must not need it drawn.
    fn measure(&mut self, text: &str, style: &TextStyle, scale: f32) -> RunMetrics;
    /// Draws `text` white, coverage in alpha (or in its one channel),
    /// with its pen on the baseline at `pen` of the staging page (a
    /// [`PAGE`]-square surface of the rasterizer's own), within `cell`
    /// only: ink past it (an italic's overhang, a box measured short)
    /// would stay on the staging page, outside what is copied and
    /// cleared, and land in later runs' cells.
    fn draw(&mut self, text: &str, style: &TextStyle, scale: f32, pen: [f32; 2], cell: PageRect);
    /// Copies `rect` of the staging page into the same place of
    /// `texture` (an `R8Unorm` page), then clears it on the staging page.
    fn upload(&mut self, queue: &wgpu::Queue, texture: &wgpu::Texture, rect: PageRect);
    /// Changes when the fonts do (a bundled face finished loading): runs
    /// drawn and measured before are stale.
    fn fonts_version(&self) -> u64 {
        0
    }
}

/// What the cache asks of whoever draws runs and keeps the pages.
pub trait PageSink {
    fn measure(&mut self, text: &str, style: &TextStyle, scale: f32) -> RunMetrics;
    /// [`RunRasterizer::draw`].
    fn draw(&mut self, text: &str, style: &TextStyle, scale: f32, pen: [f32; 2], cell: PageRect);
    /// Copies `rect` of the staging page into page `page` and clears it.
    fn flush(&mut self, page: usize, rect: PageRect);
    /// Page `page` comes into use (a texture to make).
    fn add_page(&mut self, page: usize);
    /// Page `page` is no longer used (its texture can go).
    fn drop_page(&mut self, page: usize);
}

/// A run's key: equal keys draw alike.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct RunKey {
    text: Arc<str>,
    style: TextStyle,
    /// Physical pixels per unit, in 1/64.
    scale_64: u32,
}

impl RunKey {
    fn new(text: &Arc<str>, style: &TextStyle, scale: f32) -> RunKey {
        RunKey {
            text: text.clone(),
            style: style.clone(),
            scale_64: scale_64(scale),
        }
    }
}

fn scale_64(scale: f32) -> u32 {
    (scale * 64.0).round().max(1.0) as u32
}

/// Where a run is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub page: usize,
    pub cell: PageRect,
    /// The pen, on the baseline, from the cell's top left (texels).
    pub pen: [f32; 2],
    /// In physical pixels.
    pub metrics: RunMetrics,
}

struct Run {
    placed: Placed,
    last_used: u64,
}

struct Page {
    packer: ShelfPacker,
    last_used: u64,
    /// In use (its slot is reused after [`PageSink::drop_page`]).
    live: bool,
}

/// Counters for the debug state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextStats {
    /// Pages in use, and their bytes (one byte a texel).
    pub pages: usize,
    pub bytes: u64,
    pub budget: u64,
    /// Runs on the pages.
    pub runs: usize,
    /// Runs drawn, pages cleared for others and page copies, since the
    /// start.
    pub drawn: u64,
    pub evictions: u64,
    pub uploads: u64,
}

/// The pages' bookkeeping: which run is where, which page to reuse.
pub struct RunCache {
    page_size: u32,
    /// Pages kept before the least recently used is reused.
    budget_pages: usize,
    pages: Vec<Page>,
    runs: HashMap<RunKey, Run>,
    /// Measured runs (layout measures more than is drawn), by key.
    measured: HashMap<RunKey, RunMetrics>,
    frame: u64,
    /// The page the staging page holds runs of that are not copied yet,
    /// and where: a rectangle per stretch of a shelf. (One rectangle over
    /// them all would also copy the staging page's blank between them over
    /// runs copied before.)
    staged: Option<(usize, Vec<PageRect>)>,
    drawn: u64,
    evictions: u64,
    uploads: u64,
}

/// Measured runs kept at most; past it the measurements start over.
const MEASURED_MAX: usize = 16_384;

impl RunCache {
    /// Pages of `page_size` texels a side, about `budget` bytes of them
    /// (one page at least).
    pub fn new(page_size: u32, budget: u64) -> RunCache {
        let page_bytes = u64::from(page_size) * u64::from(page_size);
        RunCache {
            page_size,
            budget_pages: (budget / page_bytes.max(1)).max(1) as usize,
            pages: Vec::new(),
            runs: HashMap::new(),
            measured: HashMap::new(),
            frame: 0,
            staged: None,
            drawn: 0,
            evictions: 0,
            uploads: 0,
        }
    }

    pub fn set_budget(&mut self, budget: u64) {
        let page_bytes = u64::from(self.page_size) * u64::from(self.page_size);
        self.budget_pages = (budget / page_bytes.max(1)).max(1) as usize;
    }

    /// A new frame: runs placed from now on count as shown.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
    }

    /// Forgets every run and measurement (fonts changed, or the pages'
    /// textures are gone with the device); the pages' slots stay.
    pub fn clear(&mut self) {
        self.runs.clear();
        self.measured.clear();
        self.staged = None;
        for p in &mut self.pages {
            p.packer.clear();
        }
    }

    /// Forgets the pages too (their textures went with the device): the
    /// next runs start a new first page.
    pub fn reset(&mut self) {
        self.clear();
        self.pages.clear();
    }

    /// What `text` occupies at `scale` (physical pixels), measured by
    /// `measure` unless it is known.
    pub fn measure(
        &mut self,
        text: &Arc<str>,
        style: &TextStyle,
        scale: f32,
        measure: &mut dyn FnMut(&str, &TextStyle, f32) -> RunMetrics,
    ) -> RunMetrics {
        let key = RunKey::new(text, style, scale);
        if let Some(run) = self.runs.get(&key) {
            return run.placed.metrics;
        }
        if let Some(m) = self.measured.get(&key) {
            return *m;
        }
        let m = measure(text, style, scale);
        if self.measured.len() >= MEASURED_MAX {
            self.measured.clear();
        }
        self.measured.insert(key, m);
        m
    }

    /// Where `text` is on the pages for this frame, drawn there first if
    /// it is not yet. `None` for a run with no ink cell (empty).
    pub fn place(
        &mut self,
        text: &Arc<str>,
        style: &TextStyle,
        scale: f32,
        sink: &mut dyn PageSink,
    ) -> Option<Placed> {
        let frame = self.frame;
        let key = RunKey::new(text, style, scale);
        if let Some(run) = self.runs.get_mut(&key) {
            run.last_used = frame;
            self.pages[run.placed.page].last_used = frame;
            return Some(run.placed);
        }
        if text.trim().is_empty() {
            return None;
        }
        let metrics = match self.measured.remove(&key) {
            Some(m) => m,
            None => sink.measure(text, style, scale),
        };
        let left = (-metrics.ink[0]).max(0.0).ceil() as u32;
        let top = (-metrics.ink[1]).max(0.0).ceil() as u32;
        let right = metrics.ink[2].max(metrics.advance).max(0.0).ceil() as u32;
        let bottom = metrics.ink[3].max(0.0).ceil() as u32;
        // A run longer than a page is cut at the page's edge.
        let width = (left + right + 2 * PAD).min(self.page_size);
        let height = (top + bottom + 2 * PAD).min(self.page_size);
        let (page, cell) = self.alloc(width, height, sink)?;
        if self.staged.as_ref().is_some_and(|(p, _)| *p != page) {
            self.flush(sink);
        }
        let pen = [(PAD + left) as f32, (PAD + top) as f32];
        sink.draw(
            text,
            style,
            scale,
            [cell.x as f32 + pen[0], cell.y as f32 + pen[1]],
            cell,
        );
        self.drawn += 1;
        let (_, rects) = self.staged.get_or_insert_with(|| (page, Vec::new()));
        match rects
            .iter_mut()
            .find(|r| r.y == cell.y && r.height == cell.height && r.right() == cell.x)
        {
            // Next to the last run on its shelf.
            Some(r) => *r = r.union(cell),
            None => rects.push(cell),
        }
        let placed = Placed {
            page,
            cell,
            pen,
            metrics,
        };
        self.runs.insert(
            key,
            Run {
                placed,
                last_used: frame,
            },
        );
        self.pages[page].last_used = frame;
        Some(placed)
    }

    /// Copies what was drawn since the last copy to its page: once a
    /// frame, before the pages are sampled.
    pub fn flush(&mut self, sink: &mut dyn PageSink) {
        if let Some((page, rects)) = self.staged.take() {
            for rect in rects {
                sink.flush(page, rect);
                self.uploads += 1;
            }
        }
    }

    /// Past the budget, drops the pages holding the most runs not shown in
    /// the last frame, even pages with some that were, as long as what the
    /// last frame showed fits the budget at half fill: those runs are
    /// drawn again as they are shown, on the pages kept, which gathers
    /// what is shown on fewer pages. (A page with a run of the frame being
    /// drawn is never dropped: call this before anything is placed.)
    pub fn compact(&mut self, sink: &mut dyn PageSink) {
        let last = self.frame.saturating_sub(1);
        // Per page: the area of its runs, and of those shown.
        let mut area = vec![(0u64, 0u64); self.pages.len()];
        for r in self.runs.values() {
            let a = u64::from(r.placed.cell.width) * u64::from(r.placed.cell.height);
            let page = &mut area[r.placed.page];
            page.0 += a;
            if r.last_used >= last {
                page.1 += a;
            }
        }
        let shown: u64 = area.iter().map(|a| a.1).sum();
        let page_area = u64::from(self.page_size) * u64::from(self.page_size);
        let mut live = self.pages.iter().filter(|p| p.live).count();
        while live > self.budget_pages && shown * 2 <= self.budget_pages as u64 * page_area {
            let Some(i) = (0..self.pages.len())
                .filter(|&i| self.pages[i].live && self.pages[i].last_used < self.frame)
                .max_by_key(|&i| (area[i].0 - area[i].1, std::cmp::Reverse(area[i].1)))
            else {
                break;
            };
            self.empty(i, sink);
            self.pages[i].live = false;
            area[i] = (0, 0);
            sink.drop_page(i);
            self.evictions += 1;
            live -= 1;
        }
    }

    /// Drops pages past the budget not drawn from since `frames` frames
    /// (pages added while every one was in use).
    pub fn trim(&mut self, frames: u64, sink: &mut dyn PageSink) {
        let live = self.pages.iter().filter(|p| p.live).count();
        let mut excess = live.saturating_sub(self.budget_pages);
        while excess > 0 {
            let Some(i) = self.least_recent(self.frame.saturating_sub(frames)) else {
                break;
            };
            self.empty(i, sink);
            self.pages[i].live = false;
            sink.drop_page(i);
            excess -= 1;
        }
    }

    /// The live page drawn from least recently, before frame `before`.
    fn least_recent(&self, before: u64) -> Option<usize> {
        self.pages
            .iter()
            .enumerate()
            .filter(|(_, p)| p.live && p.last_used < before)
            .min_by_key(|(_, p)| p.last_used)
            .map(|(i, _)| i)
    }

    /// Forgets page `i`'s runs and clears it (what the staging page holds
    /// of it is copied, which clears the staging page too).
    fn empty(&mut self, i: usize, sink: &mut dyn PageSink) {
        if self.staged.as_ref().is_some_and(|(p, _)| *p == i) {
            self.flush(sink);
        }
        self.runs.retain(|_, r| r.placed.page != i);
        self.pages[i].packer.clear();
    }

    /// A cell of `width × height`: on a page with room, else a new page
    /// within the budget, else the least recently used page cleared, else
    /// (every page in use this frame) a new page past the budget.
    fn alloc(
        &mut self,
        width: u32,
        height: u32,
        sink: &mut dyn PageSink,
    ) -> Option<(usize, PageRect)> {
        // Newest pages first: older ones are mostly full.
        for i in (0..self.pages.len()).rev() {
            if self.pages[i].live
                && let Some(cell) = self.pages[i].packer.alloc(width, height)
            {
                return Some((i, cell));
            }
        }
        let live = self.pages.iter().filter(|p| p.live).count();
        if live >= self.budget_pages
            && let Some(i) = self.least_recent(self.frame)
        {
            self.empty(i, sink);
            self.evictions += 1;
            return self.pages[i].packer.alloc(width, height).map(|c| (i, c));
        }
        let i = match self.pages.iter().position(|p| !p.live) {
            Some(i) => i,
            None => {
                self.pages.push(Page {
                    packer: ShelfPacker::new(self.page_size, self.page_size),
                    last_used: self.frame,
                    live: false,
                });
                self.pages.len() - 1
            }
        };
        self.pages[i].live = true;
        self.pages[i].last_used = self.frame;
        sink.add_page(i);
        self.pages[i].packer.alloc(width, height).map(|c| (i, c))
    }

    pub fn stats(&self) -> TextStats {
        let pages = self.pages.iter().filter(|p| p.live).count();
        let page_bytes = u64::from(self.page_size) * u64::from(self.page_size);
        TextStats {
            pages,
            bytes: pages as u64 * page_bytes,
            budget: self.budget_pages as u64 * page_bytes,
            runs: self.runs.len(),
            drawn: self.drawn,
            evictions: self.evictions,
            uploads: self.uploads,
        }
    }
}

/// Measures through a rasterizer and the cache, at the scale the runs
/// will be drawn at, in units ([`TextMeasure`]: sizes and metrics in the
/// actors' units, `scale` physical pixels each).
pub struct Measurer<'a> {
    pub cache: &'a mut RunCache,
    pub measure: &'a mut dyn FnMut(&str, &TextStyle, f32) -> RunMetrics,
    pub scale: f32,
}

impl TextMeasure for Measurer<'_> {
    fn measure(&mut self, text: &str, style: &TextStyle) -> RunMetrics {
        let text: Arc<str> = text.into();
        let m = self.cache.measure(&text, style, self.scale, self.measure);
        let s = 1.0 / self.scale.max(1e-3);
        RunMetrics {
            advance: m.advance * s,
            ink: m.ink.map(|v| v * s),
            baseline: m.baseline * s,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `0.5 × size` a character wide, ink from the baseline up to
    /// the size; records what is asked of it.
    #[derive(Default)]
    struct Fake {
        measured: usize,
        drawn: Vec<(String, [f32; 2])>,
        flushed: Vec<(usize, PageRect)>,
        added: Vec<usize>,
        dropped: Vec<usize>,
    }

    impl PageSink for Fake {
        fn measure(&mut self, text: &str, style: &TextStyle, scale: f32) -> RunMetrics {
            self.measured += 1;
            let size = style.size() * scale;
            let advance = text.chars().count() as f32 * size * 0.5;
            RunMetrics {
                advance,
                ink: [0.0, -size, advance, size * 0.25],
                baseline: size,
            }
        }
        fn draw(&mut self, text: &str, _: &TextStyle, _: f32, pen: [f32; 2], _: PageRect) {
            self.drawn.push((text.to_string(), pen));
        }
        fn flush(&mut self, page: usize, rect: PageRect) {
            self.flushed.push((page, rect));
        }
        fn add_page(&mut self, page: usize) {
            self.added.push(page);
        }
        fn drop_page(&mut self, page: usize) {
            self.dropped.push(page);
        }
    }

    fn s(t: &str) -> Arc<str> {
        t.into()
    }

    #[test]
    fn a_run_is_drawn_once_in_a_padded_cell() {
        let mut c = RunCache::new(256, 256 * 256);
        let mut f = Fake::default();
        let style = TextStyle::new("sans-serif", 16.0);
        c.begin_frame();
        let a = c.place(&s("abcd"), &style, 1.0, &mut f).unwrap();
        // 4 × 8 wide, 16 + 4 high, 2 texels around.
        assert_eq!((a.cell.width, a.cell.height), (36, 24));
        assert_eq!(a.pen, [2.0, 18.0]);
        assert_eq!(f.drawn, vec![("abcd".to_string(), [2.0, 18.0])]);
        assert_eq!(f.added, vec![0]);
        // Again, and at another scale: drawn once more only for the scale.
        assert_eq!(c.place(&s("abcd"), &style, 1.0, &mut f), Some(a));
        let b = c.place(&s("abcd"), &style, 2.0, &mut f).unwrap();
        assert_eq!((b.cell.width, b.cell.height), (68, 44));
        assert_eq!(f.drawn.len(), 2);
        // Blank runs take no cell.
        assert_eq!(c.place(&s("  "), &style, 1.0, &mut f), None);
        // One copy a frame and shelf stretch: the runs are on two shelves.
        c.flush(&mut f);
        assert_eq!(f.flushed, vec![(0, a.cell), (0, b.cell)]);
        c.flush(&mut f);
        assert_eq!(f.flushed.len(), 2);
        assert_eq!(c.stats().runs, 2);
        // Runs side by side on a shelf go up in one copy.
        c.begin_frame();
        let d = c.place(&s("ab"), &style, 1.0, &mut f).unwrap();
        let e = c.place(&s("cd"), &style, 1.0, &mut f).unwrap();
        assert_eq!((d.cell.y, e.cell.x), (0, d.cell.right()));
        c.flush(&mut f);
        assert_eq!(f.flushed[2..], [(0, d.cell.union(e.cell))]);
    }

    #[test]
    fn measuring_is_cached_and_drawing_reuses_it() {
        let mut c = RunCache::new(256, 1 << 20);
        let mut f = Fake::default();
        let style = TextStyle::new("sans-serif", 10.0);
        let mut measure = |t: &str, st: &TextStyle, k: f32| f.measure(t, st, k);
        let mut m = Measurer {
            cache: &mut c,
            measure: &mut measure,
            scale: 2.0,
        };
        let metrics = m.measure("ab", &style);
        // In units: measured at 20 px, scaled back.
        assert_eq!(metrics.advance, 10.0);
        assert_eq!(metrics.ink, [0.0, -10.0, 10.0, 2.5]);
        m.measure("ab", &style);
        assert_eq!(f.measured, 1);
        c.begin_frame();
        c.place(&s("ab"), &style, 2.0, &mut f);
        assert_eq!(f.measured, 1);
    }

    #[test]
    fn the_least_recently_shown_page_is_reused_whole() {
        // Pages hold four runs of 64 × 64 cells: two pages in the budget.
        let mut c = RunCache::new(128, 2 * 128 * 128);
        let mut f = Fake::default();
        let style = TextStyle::new("mono", 48.0);
        // The ink is 48 + 12 high: 64 with the padding; 2 chars are 48 +
        // 4 wide; one char is 24 + 4 (two a shelf would be 56: fits).
        let runs: Vec<Arc<str>> = (0..8).map(|i| s(&format!("{i}{i}"))).collect();
        // Frame 1 fills page 0 with runs 0–3, frame 2 page 1 with 4–7.
        c.begin_frame();
        for r in &runs[..4] {
            assert_eq!(c.place(r, &style, 1.0, &mut f).unwrap().page, 0);
        }
        c.flush(&mut f);
        c.begin_frame();
        for r in &runs[4..] {
            assert_eq!(c.place(r, &style, 1.0, &mut f).unwrap().page, 1);
        }
        c.flush(&mut f);
        // Frame 3 shows run 0 (page 0) and a new run: page 1 is the least
        // recently used, and is cleared for it.
        c.begin_frame();
        c.place(&runs[0], &style, 1.0, &mut f);
        let new = c.place(&s("xx"), &style, 1.0, &mut f).unwrap();
        assert_eq!((new.page, new.cell.x, new.cell.y), (1, 0, 0));
        assert_eq!(c.stats().evictions, 1);
        assert_eq!(c.stats().runs, 5);
        // Run 4 was on it: drawn again when shown again.
        let drawn = f.drawn.len();
        c.flush(&mut f);
        c.begin_frame();
        c.place(&runs[4], &style, 1.0, &mut f);
        assert_eq!(f.drawn.len(), drawn + 1);
        assert_eq!(f.added, vec![0, 1]);
    }

    #[test]
    fn pages_in_use_are_never_reused_and_the_extra_ones_go_later() {
        let mut c = RunCache::new(128, 128 * 128);
        let mut f = Fake::default();
        let style = TextStyle::new("mono", 48.0);
        c.begin_frame();
        // Eight runs in one frame on a one-page budget: two pages.
        let pages: Vec<usize> = (0..8)
            .map(|i| {
                c.place(&s(&format!("{i}{i}")), &style, 1.0, &mut f)
                    .unwrap()
                    .page
            })
            .collect();
        assert_eq!(pages, [0, 0, 0, 0, 1, 1, 1, 1]);
        // A switch of page copies the first page's runs first (its two
        // shelves).
        assert_eq!(f.flushed.len(), 2);
        assert!(f.flushed.iter().all(|(p, _)| *p == 0));
        c.flush(&mut f);
        assert_eq!(c.stats().evictions, 0);
        assert_eq!(c.stats().pages, 2);
        // Later frames show page 1's runs only: page 0 is dropped once
        // it has gone unused for the frames given.
        for _ in 0..3 {
            c.begin_frame();
            c.place(&s("44"), &style, 1.0, &mut f);
            c.trim(2, &mut f);
        }
        assert_eq!(f.dropped, vec![0]);
        assert_eq!(c.stats().pages, 1);
        assert_eq!(c.stats().runs, 4);
        // Its slot is used again for the next page (page 1 is shown).
        c.begin_frame();
        c.place(&s("44"), &style, 1.0, &mut f);
        for i in 0..4 {
            c.place(&s(&format!("{i}{i}")), &style, 1.0, &mut f);
        }
        assert_eq!(f.added, vec![0, 1, 0]);
    }

    #[test]
    fn pages_past_the_budget_gather_what_is_shown() {
        let mut c = RunCache::new(128, 128 * 128);
        let mut f = Fake::default();
        let style = TextStyle::new("mono", 48.0);
        // Frame 1 shows eight runs: two pages, one past the budget.
        c.begin_frame();
        for i in 0..8 {
            c.place(&s(&format!("{i}{i}")), &style, 1.0, &mut f);
        }
        c.flush(&mut f);
        // Eight cells of 52 × 64 are more than half a page: kept.
        c.begin_frame();
        c.compact(&mut f);
        assert_eq!(c.stats().pages, 2);
        // Frame 2 shows two runs of page 1; frame 3 drops page 0 at its
        // start and draws nothing of it again.
        c.place(&s("44"), &style, 1.0, &mut f);
        c.place(&s("55"), &style, 1.0, &mut f);
        c.flush(&mut f);
        c.begin_frame();
        c.compact(&mut f);
        assert_eq!(f.dropped, vec![0]);
        assert_eq!((c.stats().pages, c.stats().evictions), (1, 1));
        // Run 0 was on the page dropped: drawn again when shown.
        c.begin_frame();
        c.place(&s("44"), &style, 1.0, &mut f);
        c.place(&s("55"), &style, 1.0, &mut f);
        let drawn = f.drawn.len();
        c.place(&s("00"), &style, 1.0, &mut f);
        assert_eq!(f.drawn.len(), drawn + 1);
    }

    #[test]
    fn a_reset_cache_makes_its_pages_again() {
        let mut c = RunCache::new(256, 1 << 20);
        let mut f = Fake::default();
        let style = TextStyle::new("sans-serif", 12.0);
        c.begin_frame();
        c.place(&s("hello"), &style, 1.0, &mut f);
        c.reset();
        c.begin_frame();
        c.place(&s("hello"), &style, 1.0, &mut f);
        assert_eq!(f.added, vec![0, 0]);
    }

    #[test]
    fn a_cleared_cache_draws_everything_again() {
        let mut c = RunCache::new(256, 1 << 20);
        let mut f = Fake::default();
        let style = TextStyle::new("sans-serif", 12.0);
        c.begin_frame();
        let a = c.place(&s("hello"), &style, 1.0, &mut f).unwrap();
        c.clear();
        c.begin_frame();
        let b = c.place(&s("hello"), &style, 1.0, &mut f).unwrap();
        assert_eq!(a, b);
        assert_eq!(f.drawn.len(), 2);
        assert_eq!(f.measured, 2);
    }
}
