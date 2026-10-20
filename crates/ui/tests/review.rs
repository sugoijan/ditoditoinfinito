//! The step 4 review's cases: long lists whose glides must end, lists that
//! shrink mid-glide, short lists, the wheel style, catching taps, keys
//! during a flick, the selection kept by key across a reload, widgets
//! inside a moved frame, and images asked for during a long flick.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ddi_actor::{Actor, Drawn, Kind, UNTEXTURED};
use ddi_ui::event::{PointerKind, PointerPhase};
use ddi_ui::look::{Plain, call_for_tests as call};
use ddi_ui::testing::Harness;
use ddi_ui::widgets::controls::Slider;
use ddi_ui::widgets::list::{ItemKind, List, ListConfig, ListEvent, ListModel, ListStyle};
use ddi_ui::widgets::{set_image, set_text};
use ddi_ui::{Cx, Focusable, Handled, Input, Screen, TextMode};

/// `len` rows of 48, keyed by `key_base + i`, each with an image of its
/// own when `images`.
struct Rows {
    len: Rc<Cell<usize>>,
    key_base: Rc<Cell<u64>>,
    images: bool,
}

impl ListModel for Rows {
    fn len(&self) -> usize {
        self.len.get()
    }
    fn kind(&self, _: usize) -> ItemKind {
        ItemKind::Row
    }
    fn fill(&self, i: usize, item: &mut Actor, cx: &mut Cx) {
        set_text(item, "Title", &format!("Song {i}"));
        if self.images {
            let t = cx.texture(&format!("banners/{i}.png"), 128.0);
            set_image(item, "Banner", Some(t));
        }
    }
    fn key(&self, i: usize) -> Option<u64> {
        Some(self.key_base.get() + i as u64)
    }
}

#[derive(Default)]
struct Seen {
    events: Vec<ListEvent>,
    selected: usize,
    scroll: f64,
    animating: bool,
    reload: bool,
    volume: f32,
}

struct S {
    list: List,
    model: Rows,
    slider: Slider,
    seen: Rc<RefCell<Seen>>,
    /// Where the list and the slider are put: a frame moved by this.
    offset: [f32; 2],
}

impl Screen for S {
    fn build(&mut self, _cx: &mut Cx) -> Actor {
        let mut f = Actor::new(Kind::Frame(vec![
            self.list.build(&Plain),
            self.slider.build(&Plain),
        ]));
        call(&mut f, "xy", &self.offset);
        f.name = "Moved".into();
        Actor::new(Kind::Frame(vec![f]))
    }
    fn focusables(&self) -> Vec<Focusable> {
        vec![Focusable::new("L")]
    }
    fn animating(&self) -> bool {
        self.list.animating()
    }
    fn update(&mut self, dt: f32, root: &mut Actor, cx: &mut Cx) {
        let reload = std::mem::take(&mut self.seen.borrow_mut().reload);
        if reload {
            let e = self.list.reload(&self.model);
            self.seen.borrow_mut().events.extend(e);
        }
        let e = self.list.update(dt, root, &self.model, &Plain, cx);
        let mut s = self.seen.borrow_mut();
        s.events.extend(e);
        s.selected = self.list.selected().unwrap_or(0);
        s.scroll = self.list.scroll();
        s.animating = self.list.animating();
    }
    fn input(&mut self, input: &Input, root: &mut Actor, _cx: &mut Cx) -> Handled {
        let (h, v) = self.slider.input(input, root, false);
        if let Some(v) = v {
            self.seen.borrow_mut().volume = v;
        }
        if h == Handled::Yes {
            return h;
        }
        let (h, e) = self.list.input(input, root, &self.model);
        self.seen.borrow_mut().events.extend(e);
        h
    }
}

struct Made {
    h: Harness,
    seen: Rc<RefCell<Seen>>,
    len: Rc<Cell<usize>>,
    key_base: Rc<Cell<u64>>,
}

fn mk(n: usize, rect: [f32; 4], style: ListStyle, offset: [f32; 2], images: bool) -> Made {
    let len = Rc::new(Cell::new(n));
    let key_base = Rc::new(Cell::new(0));
    let seen = Rc::new(RefCell::new(Seen::default()));
    let mut c = ListConfig::new("L", rect);
    c.style = style;
    let model = Rows {
        len: len.clone(),
        key_base: key_base.clone(),
        images,
    };
    let list = List::new(c, &model);
    let slider = Slider::new("V", "Volume", [700.0, 40.0], [400.0, 34.0], 0.5);
    let mut h = Harness::new([960.0, 540.0], [960.0, 540.0], 1.0, TextMode::Mirror);
    h.push(S {
        list,
        model,
        slider,
        seen: seen.clone(),
        offset,
    });
    h.run(0.1);
    Made {
        h,
        seen,
        len,
        key_base,
    }
}

const RECT: [f32; 4] = [20.0, 20.0, 400.0, 480.0];

#[test]
fn glides_end_however_long_the_list() {
    for n in [20_000usize, 90_000, 200_000] {
        let Made { mut h, seen, .. } = mk(n, RECT, ListStyle::Plain, [0.0; 2], false);
        for k in 0..6 {
            h.wheel_lines([200.0, 200.0], [0.0, 1.0e6]);
            h.run(2.0);
            h.wheel_lines([200.0, 200.0], [0.0, -(k as f32) * 0.37 - 0.3]);
            let frames = h.run_on_demand(10.0);
            assert!(!seen.borrow().animating, "{n} rows: still gliding");
            assert!(frames < 200, "{n} rows: {frames} frames");
        }
    }
}

#[test]
fn a_list_shrinking_mid_glide_stays_within_its_ends() {
    let Made {
        mut h, seen, len, ..
    } = mk(1000, RECT, ListStyle::Plain, [0.0; 2], false);
    for _ in 0..20 {
        h.wheel_lines([200.0, 200.0], [0.0, 100.0]);
    }
    h.frame(1.0 / 60.0);
    assert!(seen.borrow().animating);
    len.set(20); // 960 high: at most 480 down.
    seen.borrow_mut().reload = true;
    h.run(3.0);
    let s = seen.borrow();
    assert!(s.scroll <= 480.0, "{}", s.scroll);
    assert!(!s.animating);
    // Rows show.
    assert!(h.texts().iter().any(|t| t.starts_with("Song 1")));
}

#[test]
fn short_lists_draw_their_bar() {
    let Made { mut h, seen, .. } = mk(
        100,
        [20.0, 20.0, 400.0, 20.0],
        ListStyle::Plain,
        [0.0; 2],
        false,
    );
    h.run(0.1);
    assert_eq!(seen.borrow().scroll, 0.0);
}

#[test]
fn a_wheel_lands_on_rows_and_selects_them() {
    let Made { mut h, seen, .. } = mk(1000, RECT, ListStyle::Wheel { focus: 0.5 }, [0.0; 2], false);
    // At the start the first row is on the line.
    assert_eq!(seen.borrow().scroll, 24.0 - 240.0);
    h.wheel_lines([200.0, 200.0], [0.0, 1.3]);
    h.run(3.0);
    let s = seen.borrow();
    let centre = s.selected as f64 * 48.0 + 24.0;
    assert!(
        (centre - s.scroll - 240.0).abs() < 1e-6,
        "{} {}",
        s.selected,
        s.scroll
    );
    assert!(s.selected > 0);
    assert_eq!(s.events.last(), Some(&ListEvent::Selected(s.selected)));
}

#[test]
fn a_tap_that_catches_a_moving_list_only_stops_it() {
    let Made { mut h, seen, .. } = mk(1000, RECT, ListStyle::Wheel { focus: 0.5 }, [0.0; 2], false);
    h.key("ArrowDown", "ArrowDown");
    h.frame(1.0 / 60.0);
    assert!(seen.borrow().animating);
    let before = seen.borrow().events.len();
    h.tap_at(PointerKind::Touch, [200.0, 90.0]);
    h.run(2.0);
    let s = seen.borrow();
    assert!(
        !s.events[before..]
            .iter()
            .any(|e| matches!(e, ListEvent::Activated(_))),
        "{:?}",
        s.events
    );
    // Settled with the selection on the line.
    let centre = s.selected as f64 * 48.0 + 24.0;
    assert!((centre - s.scroll - 240.0).abs() < 1e-6);
}

#[test]
fn keys_during_a_flick_land_on_a_row() {
    let Made { mut h, seen, .. } = mk(1000, RECT, ListStyle::Plain, [0.0; 2], false);
    h.drag(PointerKind::Touch, [200.0, 450.0], [200.0, 150.0], 0.1, 6);
    h.run(0.3);
    h.key("ArrowDown", "ArrowDown");
    h.run(3.0);
    assert_eq!(seen.borrow().scroll % 48.0, 0.0, "{}", seen.borrow().scroll);
}

#[test]
fn the_selection_follows_its_item_across_a_reload() {
    let Made {
        mut h,
        seen,
        len,
        key_base,
    } = mk(1000, RECT, ListStyle::Plain, [0.0; 2], false);
    // Keys from 1000.
    key_base.set(1000);
    seen.borrow_mut().reload = true;
    h.frame(0.0);
    for _ in 0..5 {
        h.key("ArrowDown", "ArrowDown");
    }
    h.frame(0.0);
    assert_eq!(seen.borrow().selected, 5);
    // Ten items come in front: the selected one (key 1005) is now at 15.
    len.set(1010);
    key_base.set(990);
    seen.borrow_mut().reload = true;
    h.run(0.5);
    assert_eq!(seen.borrow().selected, 15);
    assert!(seen.borrow().events.contains(&ListEvent::Selected(15)));
    // And in view.
    let s = seen.borrow();
    assert!(
        15.0 * 48.0 >= s.scroll && 16.0 * 48.0 <= s.scroll + 480.0,
        "{}",
        s.scroll
    );
}

#[test]
fn widgets_work_inside_a_moved_frame() {
    let Made { mut h, seen, .. } = mk(1000, RECT, ListStyle::Plain, [100.0, 50.0], false);
    // The slider's track (from its look, around its centre) is moved by
    // (100, 50): drag along it to its left end.
    h.drag(
        PointerKind::Mouse,
        [100.0 + 700.0 + 50.0, 50.0 + 40.0],
        [100.0 + 700.0 - 300.0, 50.0 + 40.0],
        0.2,
        4,
    );
    assert!(seen.borrow().volume < 0.01, "{}", seen.borrow().volume);
    // The list's scrollbar, moved too: drag its thumb to the middle.
    let x = 100.0 + 20.0 + 400.0 - 4.0;
    h.pointer(
        1,
        PointerKind::Mouse,
        PointerPhase::Down,
        [x, 50.0 + 20.0 + 10.0],
    );
    h.pointer(
        1,
        PointerKind::Mouse,
        PointerPhase::Move,
        [x, 50.0 + 20.0 + 240.0],
    );
    h.pointer(
        1,
        PointerKind::Mouse,
        PointerPhase::Up,
        [x, 50.0 + 20.0 + 240.0],
    );
    h.run(0.2);
    let mid = seen.borrow().scroll;
    assert!((20_000.0..28_000.0).contains(&mid), "{mid}");
    // A tap on the row under the pointer selects it.
    h.run(0.5);
    let first = (seen.borrow().scroll / 48.0) as usize;
    h.tap_at(PointerKind::Touch, [100.0 + 200.0, 50.0 + 20.0 + 24.0]);
    h.frame(0.0);
    assert_eq!(seen.borrow().selected, first);
}

#[test]
fn a_long_flick_asks_for_few_images() {
    let Made { mut h, seen, .. } = mk(10_000, RECT, ListStyle::Plain, [0.0; 2], true);
    let start = h.ui.texture_count();
    let mut drawn = std::collections::BTreeSet::new();
    // Hard flicks down the list, drawing every frame.
    for _ in 0..3 {
        h.drag(PointerKind::Touch, [200.0, 480.0], [200.0, 30.0], 0.05, 5);
        for _ in 0..240 {
            h.frame(1.0 / 60.0);
            for d in &h.drawn {
                if let Drawn::Quad(q) = d
                    && q.texture != UNTEXTURED
                {
                    drawn.insert(q.texture);
                }
            }
        }
    }
    let s = seen.borrow();
    assert!(s.scroll > 5_000.0, "{}", s.scroll);
    eprintln!(
        "flicked {:.0} px ({:.0} rows): {} images drawn (asked of the cache), {} ids handed out",
        s.scroll,
        s.scroll / 48.0,
        drawn.len(),
        h.ui.texture_count() - start
    );
    // Banners only while it is slow (during the drags and as each flick
    // runs out), not for the rows rushing past.
    assert!(drawn.len() < 80, "{} images drawn", drawn.len());
    assert!((drawn.len() as f64) < s.scroll / 48.0 / 2.0);
}
