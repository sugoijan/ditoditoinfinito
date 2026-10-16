//! The banner cache over ten thousand rows, most with a banner of its
//! own: the widget screen's list driven by the harness, its draws asked
//! of the painter's texture cache as a shell asks (by source and the
//! size drawn, P9), every image "decoded" at once at a banner's bytes.
//! What stays loaded is bounded by bytes and count however far the list
//! goes, and a long flick asks for nothing it passes.

use std::rc::Rc;

use ddi_actor::{Drawn, UNTEXTURED};
use ddi_render::texture_cache::{MAX_LOADED, TextureCache, TextureKey};
use ddi_ui::dev_widgets::{FoldFn, LIBRARY_MESSAGE, LibrarySong, SharedWidgets, WidgetScreen};
use ddi_ui::event::{PointerKind, WheelSource};
use ddi_ui::testing::Harness;
use ddi_ui::{Message, TextMode};

/// A banner's bytes at the longest side `side`: 3.2 to 1 (StepMania's
/// 256×80), RGBA, with its mipmaps.
fn bytes(side: u32) -> u64 {
    let (w, h) = (u64::from(side), u64::from(side) * 10 / 32);
    w * h * 4 * 4 / 3
}

struct Shell {
    h: Harness,
    cache: TextureCache<()>,
    ratio: f32,
    /// Loads and the most loaded and bytes seen.
    loads: usize,
    most: (usize, u64),
}

impl Shell {
    /// A frame: drawn, its images asked for and loaded.
    fn frame(&mut self) {
        self.h.frame(1.0 / 60.0);
        self.cache.begin_frame();
        for d in &self.h.drawn {
            if let Drawn::Quad(q) = d
                && q.texture != UNTEXTURED
                && let Some((source, side)) = self.h.ui.texture_source(q.texture)
            {
                let id = self.cache.id(TextureKey::drawn(source, side * self.ratio));
                self.cache.resolve(id);
            }
        }
        for (id, key) in self.cache.take_requests() {
            self.cache.insert(id, (), bytes(key.max_side));
            self.loads += 1;
        }
        self.cache.evict();
        let s = self.cache.stats();
        self.most = (self.most.0.max(s.loaded), self.most.1.max(s.bytes));
    }

    fn run(&mut self, seconds: f32) {
        for _ in 0..(seconds * 60.0) as usize {
            self.frame();
        }
    }
}

#[test]
fn ten_thousand_banners_stay_bounded() {
    let library: Vec<LibrarySong> = (0..10_000)
        .map(|i| LibrarySong {
            title: format!("Song {i}"),
            artist: "Artist".into(),
            pack: None,
            banner: Some(format!("songs/s{i}/bn.png")),
        })
        .collect();
    let state = SharedWidgets::default();
    state.borrow_mut().library = library;
    let fold: FoldFn = Rc::new(|s: &str| s.to_string());
    let ratio = 2.0;
    let mut h = Harness::new([960.0, 540.0], [960.0, 540.0], ratio, TextMode::Own);
    h.push(WidgetScreen::new(state.clone(), fold));
    h.ui.broadcast(Message::new(LIBRARY_MESSAGE));
    // A budget of a 100 banners or so, to see it hold.
    let mut cache = TextureCache::new(100 * bytes(256));
    cache.set_max_loaded(MAX_LOADED);
    let mut s = Shell {
        h,
        cache,
        ratio,
        loads: 0,
        most: (0, 0),
    };
    s.run(0.5);
    let first = s.loads;
    assert!((5..=14).contains(&first), "a screenful: {first}");
    // Down with a trackpad, slowly enough to show banners (720 px a
    // second): every banner shown loads once, what left goes.
    for _ in 0..1200 {
        s.h.wheel([200.0, 260.0], [0.0, 12.0], WheelSource::Precise);
        s.frame();
    }
    s.run(1.0);
    let stats = s.cache.stats();
    eprintln!(
        "wheel: {} loads, {} evictions, at most {} loaded, {} KB",
        s.loads,
        stats.evictions,
        s.most.0,
        s.most.1 / 1024
    );
    assert!(s.loads > 150, "{}", s.loads);
    assert!(stats.bytes <= 100 * bytes(256) + 16 * bytes(256));
    assert!(s.most.0 <= 120, "{}", s.most.0);
    // A long flick: nothing loads while it is fast.
    let before = s.loads;
    s.h.drag(PointerKind::Touch, [200.0, 500.0], [200.0, 40.0], 0.05, 4);
    let mut fast_loads = 0;
    for _ in 0..60 {
        let l = s.loads;
        s.frame();
        if s.h.ui.needs_redraw() && s.loads > l {
            fast_loads += s.loads - l;
        }
        if !s.h.ui.needs_redraw() {
            break;
        }
    }
    s.run(4.0);
    let after = s.loads - before;
    eprintln!(
        "flick: {after} loads ({fast_loads} while moving), scroll {}",
        state.borrow().scroll
    );
    assert!(after <= 3 * 14, "{after}");
    // The same banners again: loaded, nothing asked.
    let loads = s.loads;
    s.run(1.0);
    assert_eq!(s.loads, loads);
}
