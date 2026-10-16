//! The widgets driven through the harness: the virtual list (bounded
//! actors, text and images over ten thousand items; inertia, snapping,
//! keys, the scrollbar, the wheel style), the controls, the search field
//! with its popup, the panels.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_actor::{Actor, Drawn, Kind, UNTEXTURED};
use ddi_ui::dev_widgets::{FoldFn, LIBRARY_MESSAGE, LibrarySong, SharedWidgets, WidgetScreen};
use ddi_ui::event::{PointerKind, PointerPhase, TextEvent, TextSync, WheelSource};
use ddi_ui::look::Plain;
use ddi_ui::testing::Harness;
use ddi_ui::widgets::list::{ItemKind, List, ListConfig, ListEvent, ListModel, ListStyle};
use ddi_ui::widgets::{set_image, set_text};
use ddi_ui::{Cx, Focusable, Handled, Input, Message, Screen, TextMode, UiEvent, UiRequest};

/// `n` rows of 48, each with an image of its own.
struct Rows(usize);

impl ListModel for Rows {
    fn len(&self) -> usize {
        self.0
    }
    fn kind(&self, _: usize) -> ItemKind {
        ItemKind::Row
    }
    fn fill(&self, i: usize, item: &mut Actor, cx: &mut Cx) {
        set_text(item, "Title", &format!("Song {i}"));
        set_text(item, "Subtitle", "Artist");
        let t = cx.texture(&format!("banners/{i}.png"), 128.0);
        set_image(item, "Banner", Some(t));
    }
}

#[derive(Default)]
struct Seen {
    events: Vec<ListEvent>,
    selected: usize,
    scroll: f32,
    pool: usize,
    animating: bool,
    shown: Vec<usize>,
    blur: f32,
    /// The fastest the list was seen moving, and its most blur.
    top_speed: f64,
    top_blur: f32,
}

struct ListScreen {
    list: List,
    model: Rows,
    seen: Rc<RefCell<Seen>>,
}

impl ListScreen {
    fn new(style: ListStyle, seen: Rc<RefCell<Seen>>) -> ListScreen {
        let model = Rows(10_000);
        let mut config = ListConfig::new("L", [20.0, 20.0, 400.0, 480.0]);
        config.style = style;
        ListScreen {
            list: List::new(config, &model),
            model,
            seen,
        }
    }

    fn note(&self, e: Option<ListEvent>) {
        let mut s = self.seen.borrow_mut();
        s.events.extend(e);
        s.selected = self.list.selected().unwrap_or(0);
        s.scroll = self.list.scroll() as f32;
        s.pool = self.list.pool();
        s.animating = self.list.animating();
        s.shown = self.list.shown();
        s.blur = self.list.blur();
        s.top_speed = s.top_speed.max(self.list.speed());
        s.top_blur = s.top_blur.max(self.list.blur());
    }
}

impl Screen for ListScreen {
    fn build(&mut self, _cx: &mut Cx) -> Actor {
        Actor::new(Kind::Frame(vec![self.list.build(&Plain)]))
    }
    fn focusables(&self) -> Vec<Focusable> {
        vec![Focusable::new("L")]
    }
    fn animating(&self) -> bool {
        self.list.animating()
    }
    fn update(&mut self, dt: f32, root: &mut Actor, cx: &mut Cx) {
        let e = self.list.update(dt, root, &self.model, &Plain, cx);
        self.note(e);
    }
    fn input(&mut self, input: &Input, root: &mut Actor, _cx: &mut Cx) -> Handled {
        let (h, e) = self.list.input(input, root, &self.model);
        self.note(e);
        h
    }
}

fn list(style: ListStyle) -> (Harness, Rc<RefCell<Seen>>) {
    let seen = Rc::new(RefCell::new(Seen::default()));
    let mut h = Harness::new([960.0, 540.0], [960.0, 540.0], 1.0, TextMode::Mirror);
    h.push(ListScreen::new(style, seen.clone()));
    h.run(0.1);
    (h, seen)
}

fn texts(h: &Harness) -> usize {
    h.drawn
        .iter()
        .filter(|d| matches!(d, Drawn::Text(_)))
        .count()
}

fn images(h: &Harness) -> usize {
    let mut ids: Vec<u32> = h
        .drawn
        .iter()
        .filter_map(|d| match d {
            Drawn::Quad(q) if q.texture != UNTEXTURED => Some(q.texture),
            _ => None,
        })
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids.len()
}

#[test]
fn ten_thousand_rows_with_bounded_actors_text_and_images() {
    let (mut h, seen) = list(ListStyle::Plain);
    // 480 shown + 96 a side of margin: about 14 rows of 48.
    let pool = seen.borrow().pool;
    assert!((10..=16).contains(&pool), "{pool}");
    assert!(texts(&h) <= 2 * 12, "{}", texts(&h));
    assert!(images(&h) <= 12);
    // Down the list with the wheel, a flick and the scrollbar.
    for _ in 0..40 {
        h.wheel_lines([200.0, 200.0], [0.0, 10.0]);
        h.frame(1.0 / 60.0);
    }
    h.run(1.0);
    assert!(seen.borrow().scroll > 10_000.0, "{}", seen.borrow().scroll);
    h.drag(PointerKind::Touch, [200.0, 400.0], [200.0, 100.0], 0.1, 6);
    h.run(4.0);
    // To the middle by the scrollbar's thumb (at the list's right edge).
    let thumb_x = 20.0 + 400.0 - 4.0;
    let s = seen.borrow().scroll;
    let thumb_y = 20.0 + s / (10_000.0 * 48.0 - 480.0) * (480.0 - 24.0) + 10.0;
    h.pointer(
        1,
        PointerKind::Mouse,
        PointerPhase::Down,
        [thumb_x, thumb_y],
    );
    h.pointer(1, PointerKind::Mouse, PointerPhase::Move, [thumb_x, 270.0]);
    h.pointer(1, PointerKind::Mouse, PointerPhase::Up, [thumb_x, 270.0]);
    h.run(0.5);
    let mid = seen.borrow().scroll;
    assert!((200_000.0..280_000.0).contains(&mid), "{mid}");
    // Still a handful of actors, runs and images, and only the images of
    // rows that were shown were ever asked for.
    assert!(seen.borrow().pool <= 16, "{}", seen.borrow().pool);
    assert!(texts(&h) <= 2 * 12);
    assert!(images(&h) <= 12);
    let asked = h.ui.texture_count();
    assert!(asked < 400, "{asked} images asked for");
    // The rows shown are those around the scroll.
    let shown = seen.borrow().shown.clone();
    let first = (mid / 48.0) as usize;
    assert!(
        shown.iter().all(|&i| i + 3 >= first && i <= first + 13),
        "{shown:?} at {first}"
    );
    assert!(
        h.texts()
            .iter()
            .any(|t| *t == format!("Song {}", first + 1))
    );
}

#[test]
fn flicks_slow_down_stop_and_snap_to_a_row() {
    let (mut h, seen) = list(ListStyle::Plain);
    h.drag(PointerKind::Touch, [200.0, 450.0], [200.0, 150.0], 0.1, 6);
    assert!(seen.borrow().animating);
    let after_flick = seen.borrow().scroll;
    h.run(0.5);
    let later = seen.borrow().scroll;
    assert!(later > after_flick + 300.0, "{after_flick} {later}");
    h.run(4.0);
    let s = seen.borrow();
    assert!(!s.animating);
    assert!(!h.ui.needs_redraw());
    assert_eq!(s.scroll % 48.0, 0.0, "{}", s.scroll);
    // A press catches a moving list.
    drop(s);
    h.drag(PointerKind::Touch, [200.0, 450.0], [200.0, 150.0], 0.1, 6);
    h.run(0.1);
    h.pointer(2, PointerKind::Touch, PointerPhase::Down, [200.0, 300.0]);
    let caught = seen.borrow().scroll;
    h.run(0.3);
    assert_eq!(seen.borrow().scroll, caught);
    h.pointer(2, PointerKind::Touch, PointerPhase::Up, [200.0, 300.0]);
}

#[test]
fn keys_and_pads_move_the_selection_and_the_list_follows() {
    let (mut h, seen) = list(ListStyle::Plain);
    assert_eq!(h.ui.focused(), Some("L"));
    for _ in 0..3 {
        h.key("ArrowDown", "ArrowDown");
    }
    h.run(0.5);
    assert_eq!(seen.borrow().selected, 3);
    assert_eq!(seen.borrow().scroll, 0.0, "no need to move yet");
    // Held: repeats; the list keeps the selection in view.
    h.pad(ddi_ui::MenuAction::Down, true);
    h.run(2.0);
    h.pad(ddi_ui::MenuAction::Down, false);
    h.run(1.0);
    let s = seen.borrow();
    assert!(s.selected > 15, "{}", s.selected);
    let top = s.selected as f32 * 48.0;
    assert!(
        top >= s.scroll && top + 48.0 <= s.scroll + 480.0,
        "{} {}",
        s.selected,
        s.scroll
    );
    // Start activates the selected row.
    drop(s);
    h.key("Enter", "Enter");
    let sel = seen.borrow().selected;
    assert_eq!(
        seen.borrow().events.last(),
        Some(&ListEvent::Activated(sel))
    );
}

#[test]
fn taps_select_then_activate() {
    let (mut h, seen) = list(ListStyle::Plain);
    // Row 2 is at 20 + 96 .. 20 + 144.
    h.tap_at(PointerKind::Touch, [200.0, 140.0]);
    h.run(0.3);
    assert_eq!(seen.borrow().selected, 2);
    assert_eq!(seen.borrow().events.last(), Some(&ListEvent::Selected(2)));
    h.tap_at(PointerKind::Touch, [200.0, 140.0]);
    assert_eq!(seen.borrow().events.last(), Some(&ListEvent::Activated(2)));
}

#[test]
fn wheel_style_keeps_the_selection_on_its_line() {
    let (mut h, seen) = list(ListStyle::Wheel { focus: 0.5 });
    h.drag(PointerKind::Touch, [200.0, 450.0], [200.0, 250.0], 0.1, 6);
    h.run(4.0);
    let s = seen.borrow();
    assert!(!s.animating);
    assert!(s.selected > 5, "{}", s.selected);
    // The selected row's centre on the focus line (240 from the top).
    let centre = s.selected as f32 * 48.0 + 24.0;
    assert!(
        (centre - s.scroll - 240.0).abs() < 0.6,
        "{} {}",
        centre,
        s.scroll
    );
    assert!(matches!(s.events.last(), Some(ListEvent::Selected(_))));
}

/// The wheel style's selected row centred on its line (240 down).
fn on_its_line(seen: &Seen) -> bool {
    let centre = seen.selected as f32 * 48.0 + 24.0;
    (centre - seen.scroll - 240.0).abs() < 0.6
}

const AT: [f32; 2] = [200.0, 200.0];

#[test]
fn a_slow_trackpad_scroll_follows_the_fingers_then_snaps_to_a_row() {
    for style in [ListStyle::Plain, ListStyle::Wheel { focus: 0.5 }] {
        let (mut h, seen) = list(style);
        let start = seen.borrow().scroll;
        // A slow drift: 1.5 px a frame for a second (90 px), then still.
        h.wheel_stream(AT, &[1.5; 60], WheelSource::Pixels, 1.0 / 60.0);
        let moved = seen.borrow().scroll - start;
        assert!((moved - 90.0).abs() < 0.01, "{style:?}: followed {moved}");
        assert!(seen.borrow().animating, "the stream is still open");
        assert_eq!(seen.borrow().blur, 0.0, "slow: no blur");
        h.run(1.0);
        let s = seen.borrow();
        assert!(!s.animating);
        match style {
            // 90 down: two rows of 48 are nearer than one.
            ListStyle::Plain => assert_eq!(s.scroll, 96.0),
            ListStyle::Wheel { .. } => {
                assert_eq!(s.selected, 2);
                assert!(on_its_line(&s), "{} {}", s.selected, s.scroll);
                assert_eq!(s.events.last(), Some(&ListEvent::Selected(2)));
            }
        }
    }
}

#[test]
fn a_fast_trackpad_flick_and_its_momentum_stay_one_stream() {
    let (mut h, seen) = list(ListStyle::Wheel { focus: 0.5 });
    let start = seen.borrow().scroll;
    // Fingers speeding up, then the platform's momentum dying away.
    let mut deltas = vec![3.0, 12.0, 40.0, 80.0, 120.0];
    deltas.extend((0..90).map(|i| 120.0 * 0.93f32.powi(i)));
    let total: f32 = deltas.iter().sum();
    h.wheel_stream(AT, &deltas, WheelSource::Pixels, 1.0 / 60.0);
    let moved = seen.borrow().scroll - start;
    assert!((moved - total).abs() < 0.5, "{moved} of {total}");
    assert!(seen.borrow().top_blur > 0.0, "fast: blurred");
    h.run(1.0);
    let s = seen.borrow();
    assert!(!s.animating);
    assert!(on_its_line(&s), "{} {}", s.selected, s.scroll);
    assert_eq!(s.blur, 0.0);
    // Rows are sharp again.
    assert!(h.drawn.iter().all(|d| match d {
        Drawn::Quad(q) => q.blur == [0.0; 2],
        Drawn::Text(t) => t.blur == [0.0; 2],
        _ => true,
    }));
}

#[test]
fn wheel_notches_move_a_row_each() {
    // Firefox's 3 lines, Chrome's 100 px on Windows, macOS's
    // 4.000244140625 in Chrome and Safari.
    for (delta, source) in [
        (120.0, WheelSource::Lines),
        (100.0, WheelSource::Pixels),
        (ddi_ui::gesture::MAC_NOTCH as f32, WheelSource::Pixels),
    ] {
        let (mut h, seen) = list(ListStyle::Wheel { focus: 0.5 });
        for k in 1..=3 {
            h.wheel(AT, [0.0, delta], source);
            h.run(0.6);
            let s = seen.borrow();
            assert_eq!(s.selected, k, "{delta} {source:?}");
            assert!(on_its_line(&s));
        }
        // Turned fast (one a frame): still a row each.
        h.wheel_stream(AT, &[delta; 5], source, 1.0 / 60.0);
        h.wheel(AT, [0.0, -delta], source);
        h.run(1.0);
        assert_eq!(seen.borrow().selected, 7, "{delta} {source:?}");
    }
    // A plain list glides a notch's pixels.
    let (mut h, seen) = list(ListStyle::Plain);
    h.wheel(AT, [0.0, 120.0], WheelSource::Lines);
    h.run(1.0);
    assert_eq!(seen.borrow().scroll, 120.0);
}

#[test]
fn a_notch_reported_as_a_few_pixels_still_moves_a_row() {
    // One small event and nothing after: no trackpad gesture is so short.
    let (mut h, seen) = list(ListStyle::Wheel { focus: 0.5 });
    h.wheel(AT, [0.0, 6.0], WheelSource::Pixels);
    h.run(1.0);
    assert_eq!(seen.borrow().selected, 1);
    assert!(on_its_line(&seen.borrow()));
    h.wheel(AT, [0.0, -6.0], WheelSource::Pixels);
    h.run(1.0);
    assert_eq!(seen.borrow().selected, 0);
}

#[test]
fn sideways_wheels_leave_a_vertical_list_alone() {
    let (mut h, seen) = list(ListStyle::Wheel { focus: 0.5 });
    h.wheel(AT, [0.0, 120.0], WheelSource::Lines);
    h.run(1.0);
    assert_eq!(seen.borrow().selected, 1);
    for dx in [100.0, -100.0, 120.0] {
        h.wheel(AT, [dx, 0.0], WheelSource::Pixels);
        h.run(1.0);
        assert_eq!(seen.borrow().selected, 1, "{dx}");
    }
}

#[test]
fn keys_during_a_stream_are_kept_when_it_ends() {
    let (mut h, seen) = list(ListStyle::Wheel { focus: 0.5 });
    h.wheel_stream(AT, &[2.0; 20], WheelSource::Pixels, 1.0 / 60.0);
    for _ in 0..3 {
        h.key("ArrowDown", "ArrowDown");
    }
    // The fingers go on a little (20 px, under half a row), then lift.
    h.wheel_stream(AT, &[0.5; 40], WheelSource::Pixels, 1.0 / 60.0);
    h.run(1.5);
    let s = seen.borrow();
    assert_eq!(s.selected, 3);
    assert!(on_its_line(&s), "{} {}", s.selected, s.scroll);
    drop(s);
    // Further (80 px): from where the keys went.
    for _ in 0..3 {
        h.key("ArrowDown", "ArrowDown");
    }
    h.wheel_stream(AT, &[2.0; 40], WheelSource::Pixels, 1.0 / 60.0);
    h.run(1.5);
    assert_eq!(seen.borrow().selected, 8);
}

#[test]
fn a_stream_carries_a_glide_on() {
    // A notch, then a trackpad at once: the notch's row is not lost.
    let (mut h, seen) = list(ListStyle::Wheel { focus: 0.5 });
    h.wheel(AT, [0.0, 120.0], WheelSource::Lines);
    h.frame(1.0 / 60.0);
    h.wheel_stream(AT, &[1.0; 10], WheelSource::Precise, 1.0 / 60.0);
    h.run(1.5);
    assert_eq!(seen.borrow().selected, 1);
    assert!(on_its_line(&seen.borrow()));
    // A plain list's notch glides as far as a notch the platform
    // reports small (a stream of one event).
    let (mut h, seen) = list(ListStyle::Plain);
    h.wheel(AT, [0.0, 75.0], WheelSource::Pixels);
    h.run(1.0);
    let notch = seen.borrow().scroll;
    h.wheel(AT, [0.0, 6.0], WheelSource::Pixels);
    h.run(1.0);
    assert_eq!(seen.borrow().scroll, 2.0 * notch);
}

#[test]
fn past_the_ends_it_stretches_and_springs_back() {
    let (mut h, seen) = list(ListStyle::Plain);
    // Scrolled up at the top: it gives, less than the fingers move.
    h.wheel_stream(AT, &[-10.0; 30], WheelSource::Precise, 1.0 / 60.0);
    let over = seen.borrow().scroll;
    assert!(over < -40.0 && over > -240.0, "{over}");
    let more = {
        h.wheel_stream(AT, &[-10.0; 30], WheelSource::Precise, 1.0 / 60.0);
        seen.borrow().scroll
    };
    assert!(more < over && more - over > -150.0, "{over} {more}");
    h.run(1.5);
    assert_eq!(seen.borrow().scroll, 0.0);
    assert!(!seen.borrow().animating);
    // Dragged past the end the same way, and back when let go.
    h.drag(PointerKind::Touch, [200.0, 150.0], [200.0, 450.0], 0.5, 30);
    let dragged = seen.borrow().scroll;
    assert!(dragged < -40.0 && dragged > -240.0, "{dragged}");
    h.run(1.5);
    assert_eq!(seen.borrow().scroll, 0.0);
    // At the bottom too.
    let end = 10_000.0 * 48.0 - 480.0;
    h.wheel(AT, [0.0, 1.0e7], WheelSource::Lines);
    h.run(2.0);
    assert_eq!(seen.borrow().scroll, end);
    h.wheel_stream(AT, &[10.0; 20], WheelSource::Pixels, 1.0 / 60.0);
    assert!(seen.borrow().scroll > end + 20.0);
    h.run(1.5);
    assert_eq!(seen.borrow().scroll, end);
}

#[test]
fn rows_blur_with_speed_and_never_under_reduced_motion() {
    let (mut h, seen) = list(ListStyle::Plain);
    h.drag(PointerKind::Touch, [200.0, 450.0], [200.0, 150.0], 0.05, 4);
    h.frame(1.0 / 60.0);
    let blur = seen.borrow().blur;
    assert!(blur > 4.0 && blur <= 48.0, "{blur}");
    // Each row's quads and runs carry it, along the list.
    let blurred = h
        .drawn
        .iter()
        .filter_map(|d| match d {
            Drawn::Quad(q) => Some(q.blur),
            Drawn::Text(t) => Some(t.blur),
            _ => None,
        })
        .filter(|b| *b != [0.0; 2])
        .collect::<Vec<_>>();
    assert!(blurred.len() > 10);
    assert!(
        blurred
            .iter()
            .all(|b| b[0] == 0.0 && (b[1] - blur).abs() < 1e-3)
    );
    // It fades as the flick slows, and is gone at rest.
    h.run(0.5);
    assert!(seen.borrow().blur < blur);
    h.run(4.0);
    assert_eq!(seen.borrow().blur, 0.0);
    assert!(!h.ui.needs_redraw());
    // Reduced motion: none, however fast.
    let seen = Rc::new(RefCell::new(Seen::default()));
    let mut h = Harness::new([960.0, 540.0], [960.0, 540.0], 1.0, TextMode::Mirror);
    let mut screen = ListScreen::new(ListStyle::Plain, seen.clone());
    screen.list.config.glide = false;
    h.push(screen);
    h.drag(PointerKind::Touch, [200.0, 450.0], [200.0, 150.0], 0.05, 4);
    h.run(2.0);
    let s = seen.borrow();
    assert!(s.top_speed > 2000.0, "{}", s.top_speed);
    assert_eq!(s.top_blur, 0.0);
}

fn widgets(library: Vec<LibrarySong>) -> (Harness, SharedWidgets) {
    let state = SharedWidgets::default();
    state.borrow_mut().library = library;
    let fold: FoldFn = Rc::new(|s: &str| s.to_string());
    let mut h = Harness::new([960.0, 540.0], [960.0, 540.0], 1.0, TextMode::Mirror);
    h.push(WidgetScreen::new(state.clone(), fold));
    h.run(0.1);
    (h, state)
}

#[test]
fn the_widget_screen_lists_its_library_with_headers() {
    let lib = vec![LibrarySong {
        title: "Beatucada".into(),
        artist: "Sevish".into(),
        pack: None,
        banner: Some("songs/beatucada/bn.png".into()),
    }];
    let (mut h, state) = widgets(Vec::new());
    assert_eq!(state.borrow().items, 200 * 51);
    state.borrow_mut().library = lib;
    h.ui.broadcast(Message::new(LIBRARY_MESSAGE));
    h.run(0.1);
    assert!(h.texts().iter().any(|t| t == "Beatucada"));
    assert!(h.texts().iter().any(|t| t.contains("Library (50)")));
    // The library's banner is asked for, by its source.
    assert!(
        (1..=h.ui.texture_count() as u32)
            .filter_map(|i| h.ui.texture_source(i))
            .any(|(s, _)| &*s == "songs/beatucada/bn.png")
    );
    // Start on the first header closes its pack.
    h.key("Enter", "Enter");
    h.run(0.2);
    assert_eq!(state.borrow().items, 200 * 51 - 50);
    assert!(h.texts().iter().any(|t| t.contains("▸ Library")));
}

#[test]
fn option_rows_toggles_and_sliders() {
    let (mut h, state) = widgets(Vec::new());
    // From the list, right to the search field, then down the rows.
    h.key("ArrowRight", "ArrowRight");
    assert_eq!(h.ui.focused(), Some("Search"));
    h.key("ArrowDown", "ArrowDown");
    assert_eq!(h.ui.focused(), Some("Style"));
    h.key("ArrowRight", "ArrowRight");
    h.key("ArrowRight", "ArrowRight");
    h.key("ArrowRight", "ArrowRight");
    assert_eq!(state.borrow().style, 2, "clamped at the last");
    h.key("ArrowLeft", "ArrowLeft");
    assert_eq!(state.borrow().style, 1);
    // A tap picks a choice (and focuses the row).
    h.tap(PointerKind::Mouse, "Speed.Choice0");
    assert_eq!(state.borrow().speed, 0);
    assert_eq!(h.ui.focused(), Some("Speed"));
    // The toggle by Start, the slider by keys and by a drag.
    h.key("ArrowDown", "ArrowDown");
    h.key("ArrowDown", "ArrowDown");
    assert_eq!(h.ui.focused(), Some("Reverse"));
    h.key("Enter", "Enter");
    assert!(state.borrow().reverse);
    h.key("ArrowDown", "ArrowDown");
    h.key("ArrowRight", "ArrowRight");
    assert!((state.borrow().volume - 0.85).abs() < 1e-4);
    // The track runs from 676 + 260 - 70 - 260 = 606 to 866.
    h.drag(PointerKind::Mouse, [700.0, 246.0], [600.0, 246.0], 0.2, 4);
    assert!(state.borrow().volume < 0.01, "{}", state.borrow().volume);
    // The panels row changes the grid.
    assert!(h.ui.actor("Panel.Judgements").is_some());
    h.tap(PointerKind::Mouse, "Panels.Choice1");
    assert!(h.ui.actor("Panel.Judgements").is_none());
    assert!(h.ui.actor("Panel.Timing").is_some());
    h.frame(0.0);
    assert!(h.texts().iter().any(|t| t == "Timing"));
}

fn sync(h: &mut Harness, text: &str, composing: bool) {
    let n = text.encode_utf16().count();
    h.send(UiEvent::Text(TextEvent::Sync(TextSync {
        text: text.into(),
        selection: [n, n],
        composition: composing.then_some([0, n]),
    })));
}

#[test]
fn search_suggests_under_the_field() {
    let (mut h, state) = widgets(Vec::new());
    let r = h.tap(PointerKind::Touch, "Search");
    assert!(
        matches!(r.first(), Some(UiRequest::StartTextInput { .. })),
        "{r:?}"
    );
    // While composing, no popup.
    sync(&mut h, "sil", true);
    assert!(!state.borrow().popup);
    h.key("ArrowDown", "ArrowDown");
    assert!(!state.borrow().popup);
    sync(&mut h, "sil", false);
    assert!(state.borrow().popup);
    let first = state.borrow().suggestions[0].clone();
    assert!(first.starts_with("Silver"), "{first}");
    h.frame(0.0);
    assert!(h.texts().contains(&first));
    // Down, Enter: taken, typed into the textarea, submitted, found.
    h.key_down("ArrowDown", "ArrowDown");
    assert!(h.consumed, "the textarea's caret must not move");
    h.key_up("ArrowDown", "ArrowDown");
    let r = h.key("Enter", "Enter");
    assert!(
        r.contains(&UiRequest::SetText {
            text: first.clone(),
            selection: [first.encode_utf16().count(); 2]
        }),
        "{r:?}"
    );
    assert_eq!(state.borrow().submitted, vec![first.clone()]);
    assert!(
        state.borrow().status.starts_with("found"),
        "{}",
        state.borrow().status
    );
    // Escape closes the popup first, then stops typing.
    h.tap(PointerKind::Touch, "Search");
    sync(&mut h, "pack 00", false);
    assert!(state.borrow().popup);
    h.key("Escape", "Escape");
    assert!(!state.borrow().popup);
    assert!(h.ui.text_field().is_some());
    h.key("Escape", "Escape");
    assert!(h.ui.text_field().is_none());
    // A tap on a suggestion takes it.
    h.tap(PointerKind::Touch, "Search");
    sync(&mut h, "sakura", false);
    let second = state.borrow().suggestions[1].clone();
    h.tap(PointerKind::Mouse, "Search.Item1");
    assert_eq!(state.borrow().search, second);
    assert!(!state.borrow().popup);
}

/// A field near the bottom of the screen, its popup flipping over it.
struct Low {
    field: ddi_ui::widgets::field::SearchField,
}

impl Screen for Low {
    fn build(&mut self, _cx: &mut Cx) -> Actor {
        Actor::new(Kind::Frame(vec![self.field.build(&Plain)]))
    }
    fn focusables(&self) -> Vec<Focusable> {
        vec![Focusable::new("F")]
    }
    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        let suggest = |t: &str| {
            (0..4)
                .map(|i| (format!("{t} {i}"), "title".to_string()))
                .collect::<Vec<_>>()
        };
        self.field.input(input, root, cx, &suggest).0
    }
}

#[test]
fn the_popup_is_an_overlay_and_flips_above_near_the_bottom() {
    let mut h = Harness::new([960.0, 540.0], [960.0, 540.0], 1.0, TextMode::Mirror);
    h.push(Low {
        field: ddi_ui::widgets::field::SearchField::new("F", "", [480.0, 500.0], [400.0, 34.0]),
    });
    h.run(0.1);
    h.tap(PointerKind::Touch, "F");
    sync(&mut h, "dito", false);
    h.frame(0.0);
    let layers: Vec<String> = h.ui.layers().into_iter().map(|l| l.name).collect();
    assert_eq!(layers, ["", "popup"]);
    // Over the field (whose top is 483).
    let b = h.ui.bounds("F.Item3").unwrap();
    assert!(b[3] <= 483.0, "{b:?}");
    let b0 = h.ui.bounds("F.Item0").unwrap();
    assert!(b0[1] > 300.0 && b0[1] < b[1], "{b0:?}");
    // When the visible area grows (a keyboard closed), and it fits below,
    // it opens under the field.
    h.send(UiEvent::Resize {
        size: [960.0, 900.0],
        ratio: 1.0,
    });
    sync(&mut h, "dito!", false);
    h.frame(0.0);
    let v = h.ui.viewport().screen;
    let field = h.ui.bounds("F").unwrap();
    let b = h.ui.bounds("F.Item0").unwrap();
    assert!(b[1] >= field[3], "{b:?} {field:?} {v:?}");
    // Escape closes the popup: its layer goes.
    h.key("Escape", "Escape");
    h.frame(0.0);
    assert_eq!(h.ui.layers().len(), 1);
}
