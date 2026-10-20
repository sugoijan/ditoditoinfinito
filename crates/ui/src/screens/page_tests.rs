//! Pages (credits, calibration's menu) and the app's notices, with the
//! plain look.

use std::cell::RefCell;
use std::rc::Rc;

use super::notices::{AppNotice, AppNotices, NOTICES_CHANGED, NoticeBar, NoticesFeed};
use super::page::{PageCall, PageFeed, PageScreen};
use super::{DESIGN, Env, SharedTheme, credits};
use crate::dev_widgets::FoldFn;
use crate::look::Plain;
use crate::testing::Harness;
use crate::{Message, PointerKind, TextMode, UiRequest};

fn env() -> Env {
    let fold: FoldFn =
        Rc::new(|s: &str| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s));
    Env::new(Rc::new(Plain), fold)
}

fn credits_page() -> (Harness, SharedTheme, Rc<RefCell<Vec<String>>>) {
    let env = env();
    let state = env.state.clone();
    let calls: Rc<RefCell<Vec<String>>> = Rc::default();
    let c = calls.clone();
    let feed: PageFeed = Rc::default();
    let page = PageScreen::new(env, "credits", credits::doc(&credits::sample())).with_shell(
        Rc::new(move |call, _cx| {
            c.borrow_mut().push(format!("{call:?}"));
            None
        }),
        feed,
    );
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
    h.push(page);
    h.run(0.3);
    (h, state, calls)
}

#[test]
fn credits_links_open_from_keys_and_taps() {
    let (mut h, state, calls) = credits_page();
    assert_eq!(state.borrow().screen, "credits");
    let items = state.borrow().page_items.clone();
    assert_eq!(items[0], "link:https://example.org/songs");
    assert!(items.iter().any(|i| i == "link:video/ddivideo.LICENSE.txt"));
    assert!(items.iter().any(|i| i == "fold:configure"));
    // Down to the first link, Enter opens it in the press.
    h.key("ArrowDown", "ArrowDown");
    assert_eq!(state.borrow().page_focus, 0);
    let r = h.key("Enter", "Enter");
    assert!(
        r.contains(&UiRequest::OpenUrl("https://example.org/songs".into())),
        "{r:?}"
    );
    // Down through the page: it scrolls with the focus.
    for _ in 0..40 {
        h.key("ArrowDown", "ArrowDown");
    }
    assert!(state.borrow().scroll > 0.0);
    // A fold opens, and its text shows.
    let fold = items.iter().position(|i| i == "fold:configure").unwrap();
    while state.borrow().page_focus != fold as i32 {
        h.key("ArrowUp", "ArrowUp");
    }
    h.key("Enter", "Enter");
    h.run(0.1);
    assert!(h.texts().iter().any(|t| t == "--enable-decoder=h264"));
    // Page Up and Home go back up; a tap opens a link.
    h.key("Home", "Home");
    assert_eq!(state.borrow().scroll, 0.0);
    h.run(0.1);
    let r = h.tap(PointerKind::Mouse, "Item0");
    assert!(
        r.contains(&UiRequest::OpenUrl("https://example.org/songs".into())),
        "{r:?}"
    );
    // The wheel scrolls, Back leaves through the shell.
    let at = h.centre("Column");
    h.wheel_lines(at, [0.0, 4.0]);
    assert!(state.borrow().scroll > 0.0);
    h.key("Escape", "Escape");
    assert_eq!(calls.borrow().last().map(String::as_str), Some("Leave"));
}

#[test]
fn calibrations_buttons_go_to_the_shell() {
    let env = env();
    let state = env.state.clone();
    let calls: Rc<RefCell<Vec<PageCall>>> = Rc::default();
    let c = calls.clone();
    let doc = super::calibration::doc(&crate::state::settings::Settings::default(), 250.0);
    let page = PageScreen::new(env, "calibration", doc).with_shell(
        Rc::new(move |call, _cx| {
            c.borrow_mut().push(call);
            None
        }),
        Rc::default(),
    );
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
    h.push(page);
    h.run(0.3);
    assert_eq!(state.borrow().page_items[0], "button:run:visual");
    h.key("ArrowDown", "ArrowDown");
    h.key("Enter", "Enter");
    h.key("ArrowDown", "ArrowDown");
    h.key("ArrowDown", "ArrowDown");
    h.key("Enter", "Enter");
    assert_eq!(
        *calls.borrow(),
        [
            PageCall::Button("run:visual".into()),
            PageCall::Button("run:combined".into())
        ]
    );
}

#[test]
fn notices_sit_over_the_screens_and_act() {
    let env = env();
    let state = env.state.clone();
    let feed: NoticesFeed = Rc::default();
    let acted: Rc<RefCell<Vec<String>>> = Rc::default();
    let a = acted.clone();
    let (mut h, _, _) = credits_page();
    h.ui.push_chrome(NoticeBar::new(
        env,
        feed.clone(),
        Rc::new(move |id, _cx| a.borrow_mut().push(id.into())),
    ));
    h.run(0.1);
    assert_eq!(state.borrow().app_notice, None);
    *feed.borrow_mut() = AppNotices {
        banner: Some(AppNotice {
            text: "First time here? Calibrate the timing for your audio and display before \
                   playing; it takes a minute."
                .into(),
            actions: vec![
                ("calibrate".into(), "calibrate".into(), true),
                ("later".into(), "later".into(), false),
            ],
        }),
        toast: None,
    };
    h.ui.broadcast(Message::new(NOTICES_CHANGED));
    h.run(0.3);
    assert!(state.borrow().app_notice.is_some());
    h.tap(PointerKind::Mouse, "AppNotice.later");
    assert_eq!(*acted.borrow(), ["later"]);
    // Keys still go to the page under it.
    h.key("ArrowDown", "ArrowDown");
    assert_eq!(state.borrow().page_focus, 0);
}

fn banner() -> AppNotices {
    AppNotices {
        banner: Some(AppNotice {
            text: "New audio path: 48 kHz. No calibration for it yet.".into(),
            actions: vec![
                ("calibrate".into(), "calibrate".into(), true),
                ("defaults".into(), "use defaults".into(), false),
                ("later".into(), "later".into(), false),
            ],
        }),
        toast: None,
    }
}

/// A page with the notices over it; what their actions did.
fn page_with_notices() -> (Harness, SharedTheme, NoticesFeed, Rc<RefCell<Vec<String>>>) {
    let env = env();
    let state = env.state.clone();
    let feed: NoticesFeed = Rc::default();
    let acted: Rc<RefCell<Vec<String>>> = Rc::default();
    let a = acted.clone();
    let (mut h, _, _) = credits_page();
    h.ui.push_chrome(NoticeBar::new(
        env,
        feed.clone(),
        Rc::new(move |id, _cx| a.borrow_mut().push(id.into())),
    ));
    h.run(0.1);
    (h, state, feed, acted)
}

#[test]
fn the_screens_make_room_for_the_notices() {
    let (mut h, _state, feed, _) = page_with_notices();
    let top = h.ui.bounds("Column").unwrap()[1];
    *feed.borrow_mut() = banner();
    h.ui.broadcast(Message::new(NOTICES_CHANGED));
    h.run(0.3);
    let plate = h.ui.bounds("AppNotice.Plate").unwrap();
    let below = h.ui.bounds("Column").unwrap()[1];
    assert!(below >= plate[3] - 0.5, "{below} under {plate:?}");
    assert!(below > top);
    // A tap on the page lands on the page.
    let r = h.tap(PointerKind::Mouse, "Item0");
    assert!(
        r.iter().any(|q| matches!(q, UiRequest::OpenUrl(_))),
        "{r:?}"
    );
    // Gone: the page goes back up.
    *feed.borrow_mut() = AppNotices::default();
    h.ui.broadcast(Message::new(NOTICES_CHANGED));
    h.run(0.3);
    assert_eq!(h.ui.bounds("Column").unwrap()[1], top);
}

#[test]
fn keys_and_pads_reach_the_notices() {
    let (mut h, state, feed, acted) = page_with_notices();
    *feed.borrow_mut() = banner();
    h.ui.broadcast(Message::new(NOTICES_CHANGED));
    h.run(0.3);
    // Down and Up on the page stay on the page.
    h.key("ArrowDown", "ArrowDown");
    assert_eq!(state.borrow().page_focus, 0);
    assert_eq!(h.ui.chrome_focused(), None);
    // A fresh Up at the top goes into the notices; Right, Start.
    h.key("ArrowUp", "ArrowUp");
    assert_eq!(h.ui.chrome_focused(), Some("AppNotice.calibrate"));
    h.key("ArrowRight", "ArrowRight");
    assert_eq!(h.ui.chrome_focused(), Some("AppNotice.defaults"));
    // Down goes back without doing anything.
    h.key("ArrowDown", "ArrowDown");
    assert_eq!(h.ui.chrome_focused(), None);
    assert!(acted.borrow().is_empty());
    // F2 goes there from anywhere; Enter presses.
    h.key("ArrowDown", "ArrowDown");
    h.key("F2", "F2");
    assert_eq!(h.ui.chrome_focused(), Some("AppNotice.calibrate"));
    h.key("ArrowRight", "ArrowRight");
    h.key("ArrowRight", "ArrowRight");
    h.key("Enter", "Enter");
    assert_eq!(*acted.borrow(), ["later"]);
    // A pad: its Up at the top, then A.
    // (Still in them after Enter: F2 goes back.)
    h.key("F2", "F2");
    assert_eq!(h.ui.chrome_focused(), None);
    while state.borrow().page_focus > 0 {
        h.pad(crate::MenuAction::Up, true);
        h.pad(crate::MenuAction::Up, false);
    }
    h.pad(crate::MenuAction::Up, true);
    h.pad(crate::MenuAction::Up, false);
    if h.ui.chrome_focused().is_none() {
        h.pad(crate::MenuAction::Up, true);
        h.pad(crate::MenuAction::Up, false);
    }
    assert_eq!(h.ui.chrome_focused(), Some("AppNotice.calibrate"));
    h.pad(crate::MenuAction::Start, true);
    h.pad(crate::MenuAction::Start, false);
    assert_eq!(acted.borrow().last().map(String::as_str), Some("calibrate"));
    // Back gives the focus back.
    h.pad(crate::MenuAction::Back, true);
    h.pad(crate::MenuAction::Back, false);
    assert_eq!(h.ui.chrome_focused(), None);
}

#[test]
fn song_select_reaches_the_notices_past_its_first_row() {
    let env = env();
    let feed: NoticesFeed = Rc::new(RefCell::new(banner()));
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
    h.push(super::sample::select_screen(&env, None));
    h.ui.push_chrome(NoticeBar::new(env, feed, Rc::new(|_, _| {})));
    h.run(0.5);
    // Held Up (repeats) never gets there; fresh presses past the top do.
    let mut presses = 0;
    while h.ui.chrome_focused().is_none() && presses < 40 {
        h.pad(crate::MenuAction::Up, true);
        h.pad(crate::MenuAction::Up, false);
        h.run(0.3);
        presses += 1;
    }
    assert_eq!(
        h.ui.chrome_focused(),
        Some("AppNotice.calibrate"),
        "{presses}"
    );
    h.pad(crate::MenuAction::Down, true);
    h.pad(crate::MenuAction::Down, false);
    assert_eq!(h.ui.chrome_focused(), None);
}

/// Fresh Ups from where `screen` starts, until the notices have the
/// focus (at most a few); then Down gives the screen back its focus as it
/// was before the last Up, and Enter there stays on the screen.
fn up_into_the_notices<S: crate::Screen + 'static>(screen: impl FnOnce(&Env) -> S, size: [f32; 2]) {
    let env = env();
    let state = env.state.clone();
    let feed: NoticesFeed = Rc::new(RefCell::new(banner()));
    let design = super::design_for(&Plain, size);
    let mut h = Harness::new(design, size, 1.0, TextMode::Mirror);
    h.push(screen(&env));
    h.ui.push_chrome(NoticeBar::new(env, feed, Rc::new(|_, _| {})));
    h.run(0.6);
    let name = state.borrow().screen.clone();
    let mut before = h.ui.focused().map(str::to_string);
    let mut presses = 0;
    while h.ui.chrome_focused().is_none() && presses < 40 {
        before = h.ui.focused().map(str::to_string);
        h.key("ArrowUp", "ArrowUp");
        h.run(0.3);
        presses += 1;
    }
    eprintln!("{name} {size:?}: {presses} Up, from {before:?}");
    assert_eq!(
        h.ui.chrome_focused(),
        Some("AppNotice.calibrate"),
        "{name} {size:?}"
    );
    h.key("ArrowDown", "ArrowDown");
    assert_eq!(h.ui.chrome_focused(), None);
    assert_eq!(
        h.ui.focused().map(str::to_string),
        before,
        "{name} {size:?}"
    );
    // A fresh Up from there (Down came back) reaches them again, at once.
    h.key("ArrowUp", "ArrowUp");
    assert_eq!(
        h.ui.chrome_focused(),
        Some("AppNotice.calibrate"),
        "{name} {size:?}"
    );
    // Back and F2 give the focus back too.
    h.key("Escape", "Escape");
    assert_eq!(h.ui.chrome_focused(), None);
    assert_eq!(
        h.ui.focused().map(str::to_string),
        before,
        "{name} {size:?}"
    );
    h.key("F2", "F2");
    h.key("F2", "F2");
    assert_eq!(
        h.ui.focused().map(str::to_string),
        before,
        "{name} {size:?}"
    );
    let r = h.key("Enter", "Enter");
    h.run(0.6);
    assert_eq!(state.borrow().screen, name, "{name} {size:?}: {r:?}");
}

#[test]
fn one_fresh_up_past_the_top_reaches_the_notices() {
    for size in [[1280.0, 720.0], [390.0, 844.0], [844.0, 390.0]] {
        up_into_the_notices(|env| super::sample::options_screen(env, None), size);
        up_into_the_notices(
            |env| super::sample::credits_screen(env, &credits::sample()),
            size,
        );
        up_into_the_notices(|env| super::sample::select_screen(env, None), size);
    }
}

#[test]
fn the_search_fields_caret_follows_the_notices() {
    let env = env();
    let feed: NoticesFeed = Rc::new(RefCell::new(banner()));
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
    h.push(super::sample::select_screen(&env, None));
    h.ui.push_chrome(NoticeBar::new(env, feed, Rc::new(|_, _| {})));
    h.run(0.5);
    let plate = h.ui.bounds("AppNotice.Plate").unwrap();
    let field = h.ui.bounds("Search").unwrap();
    assert!(field[1] >= plate[3] - 0.5);
    let r = h.tap(PointerKind::Mouse, "Search");
    let caret = r
        .iter()
        .find_map(|q| match q {
            UiRequest::StartTextInput { caret, .. } => Some(*caret),
            _ => None,
        })
        .expect("typing");
    // CSS pixels here are virtual ones (1280×720 at a ratio of 1).
    assert!(
        caret[1] >= field[1] - 1.0 && caret[1] <= field[3],
        "{caret:?} in {field:?}"
    );
    h.sync("ab", [2, 2], None);
    h.run(0.1);
    let ime: Vec<[f32; 4]> = h
        .requests
        .iter()
        .filter_map(|q| match q {
            UiRequest::SetImeCursorArea(c) => Some(*c),
            _ => None,
        })
        .collect();
    let last = ime.last().expect("the IME's caret");
    assert!(
        last[1] >= field[1] - 1.0 && last[1] <= field[3],
        "{last:?} in {field:?}"
    );
}

#[test]
fn a_link_a_controller_cannot_open_says_how() {
    let (mut h, state, _) = credits_page();
    h.key("ArrowDown", "ArrowDown");
    assert_eq!(state.borrow().page_focus, 0);
    let r = h.pad(crate::MenuAction::Start, true);
    h.pad(crate::MenuAction::Start, false);
    assert!(
        !r.iter().any(|q| matches!(q, UiRequest::OpenUrl(_))),
        "{r:?}"
    );
    assert_eq!(state.borrow().status, "press Enter or tap to open");
    h.run(0.05);
    assert!(
        h.texts()
            .iter()
            .any(|t| t.ends_with("press Enter or tap to open")),
        "{:?}",
        h.texts()
    );
    // Enter opens it; the hint goes.
    let r = h.key("Enter", "Enter");
    assert!(
        r.iter().any(|q| matches!(q, UiRequest::OpenUrl(_))),
        "{r:?}"
    );
    h.run(0.05);
    let left: Vec<String> = h
        .texts()
        .into_iter()
        .filter(|t| t.contains("press Enter"))
        .collect();
    assert!(left.is_empty(), "{left:?} {:?}", state.borrow().status);
}

#[test]
fn credits_take_any_commit_text() {
    let mut c = credits::sample();
    for s in &mut c.songs {
        s.source_commit = "コミットコミットコミット".into();
    }
    for k in &mut c.code {
        k.commit = "é".repeat(12);
    }
    let doc = credits::doc(&c);
    let shown = format!("{doc:?}");
    assert!(shown.contains("commit コミットコミットコミ,"), "{shown}");
}

#[test]
fn long_words_are_cut_wherever_they_fall() {
    let size = 12.0;
    let width = crate::look::mono_width("0123456789", size);
    let lines = super::select_dialogs::wrap_lines(
        "--enable-a --disable-everything-at-once-and-more x",
        width,
        size,
    );
    for l in &lines {
        assert!(
            crate::look::mono_width(l, size) <= width + 0.01,
            "{lines:?}"
        );
    }
    assert_eq!(
        lines.concat().replace(' ', ""),
        "--enable-a--disable-everything-at-once-and-morex"
    );
}
