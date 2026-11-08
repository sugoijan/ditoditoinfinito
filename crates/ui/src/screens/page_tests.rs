//! Pages (credits, calibration's menu), with the plain look.

use std::cell::RefCell;
use std::rc::Rc;

use super::page::{PageCall, PageFeed, PageScreen, PageTitle};
use super::{DESIGN, Env, SharedTheme, credits};
use crate::dev_widgets::FoldFn;
use crate::look::Plain;
use crate::testing::Harness;
use crate::{PointerKind, TextMode, UiRequest};

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
    // The game's own source first.
    assert_eq!(items[0], format!("link:{}", credits::SOURCE));
    assert_eq!(items[1], "link:https://example.org/songs");
    assert!(items.iter().any(|i| i == "link:video/ddivideo.LICENSE.txt"));
    assert!(items.iter().any(|i| i == "fold:configure"));
    // Down to the first link, Enter opens it in the press.
    h.key("ArrowDown", "ArrowDown");
    assert_eq!(state.borrow().page_focus, 0);
    let r = h.key("Enter", "Enter");
    assert!(
        r.contains(&UiRequest::OpenUrl(credits::SOURCE.into())),
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
        r.contains(&UiRequest::OpenUrl(credits::SOURCE.into())),
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
    let doc = super::calibration::doc(&crate::state::settings::Settings::default(), 250.0, &[]);
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

/// The column's scrollbar: shown beside it while there is more than
/// fits, a press on its track jumps there, its thumb drags, and the
/// wheel moves it.
#[test]
fn the_scrollbar_jumps_and_drags() {
    let (mut h, state, _) = credits_page();
    let column = h.ui.bounds("Column").unwrap();
    let bar = h.ui.bounds("Column.Bar").unwrap();
    assert!(bar[0] >= column[2], "{bar:?} beside {column:?}");
    let thumb = h.ui.bounds("Thumb").unwrap();
    assert!(
        thumb[3] - thumb[1] < (bar[3] - bar[1]) / 2.0,
        "{thumb:?} in {bar:?}"
    );
    assert_eq!(state.borrow().scroll, 0.0);
    // A press at the track's end jumps to the end.
    let x = (bar[0] + bar[2]) / 2.0;
    h.tap_at(PointerKind::Mouse, [x, bar[3] - 2.0]);
    h.run(0.2);
    let end = state.borrow().scroll;
    assert!(end > 100.0, "{end}");
    let thumb = h.ui.bounds("Thumb").unwrap();
    assert!(
        (thumb[3] - bar[3]).abs() < 1.0,
        "{thumb:?} at the end of {bar:?}"
    );
    // Dragging the thumb up scrolls up by the same share.
    h.drag(
        PointerKind::Mouse,
        [x, (thumb[1] + thumb[3]) / 2.0],
        [x, bar[1]],
        0.3,
        6,
    );
    h.run(0.2);
    assert_eq!(state.borrow().scroll, 0.0);
    // The wheel over the column moves the thumb.
    h.wheel_lines(
        [(column[0] + column[2]) / 2.0, (column[1] + column[3]) / 2.0],
        [0.0, 5.0],
    );
    h.run(0.2);
    let wheeled = state.borrow().scroll;
    assert!(wheeled > 0.0 && wheeled < end, "{wheeled}");
    let thumb = h.ui.bounds("Thumb").unwrap();
    assert!(
        thumb[1] > bar[1] && thumb[3] < bar[3],
        "{thumb:?} within {bar:?}"
    );
}

/// The studio's blocks: a field typed into tells the shell, buttons sit
/// side by side and Right moves along them, a zone is an actor the shell
/// lights.
#[test]
fn fields_buttons_and_zones() {
    use super::page::{Action, Block, PAGE_TARGET, PageDoc};
    use crate::message::{Message, Param};
    let env = env();
    let state = env.state.clone();
    let calls: Rc<RefCell<Vec<PageCall>>> = Rc::default();
    let c = calls.clone();
    let doc = PageDoc {
        title: PageTitle::new("Studio", "STUDIO"),
        blocks: vec![
            Block::Field {
                id: "name".into(),
                label: "Name".into(),
                value: "Voice".into(),
                hint: "Its name".into(),
            },
            Block::Actions(vec![Action::new("a", "One"), Action::new("b", "Two")]),
            Block::Zone {
                id: "z".into(),
                body: vec![
                    Block::Text("Inside".into()),
                    Block::Action {
                        id: "c".into(),
                        label: "Three".into(),
                    },
                ],
            },
            Block::Note("Heed this".into()),
        ],
    };
    let page = PageScreen::new(env, "studio", doc).with_shell(
        Rc::new(move |call, _cx| {
            c.borrow_mut().push(call);
            None
        }),
        Rc::default(),
    );
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
    h.push(page);
    h.run(0.3);
    assert_eq!(
        state.borrow().page_items,
        ["field:name", "button:a", "button:b", "button:c"]
    );
    let (a, b) = (h.centre("Item1"), h.centre("Item2"));
    assert!((a[1] - b[1]).abs() < 0.5 && b[0] > a[0], "{a:?} {b:?}");
    assert!(h.texts().iter().any(|t| t == "Heed this"));
    assert!(h.texts().iter().any(|t| t == "Voice"));
    h.key("ArrowDown", "ArrowDown");
    h.key("ArrowRight", "ArrowRight");
    assert_eq!(state.borrow().page_focus, 1);
    h.key("Enter", "Enter");
    assert_eq!(calls.borrow().last(), Some(&PageCall::Button("a".into())));
    // A tap on the field types into it; what is typed goes to the shell.
    let r = h.tap(PointerKind::Mouse, "Field.name");
    assert!(
        r.iter()
            .any(|q| matches!(q, UiRequest::StartTextInput { text, .. } if text == "Voice")),
        "{r:?}"
    );
    h.sync("Voice 2", [7, 7], None);
    assert_eq!(
        calls.borrow().last(),
        Some(&PageCall::Field {
            id: "name".into(),
            text: "Voice 2".into()
        })
    );
    // Enter ends the typing: the caret goes, the field keeps its text.
    let r = h.key("Enter", "Enter");
    assert!(r.contains(&UiRequest::StopTextInput), "{r:?}");
    assert!(h.ui.text_field().is_none());
    h.run(0.05);
    assert!(h.texts().iter().any(|t| t == "Voice 2"), "{:?}", h.texts());
    // So does a press elsewhere, or the focus moving on.
    h.tap(PointerKind::Mouse, "Field.name");
    assert!(h.ui.text_field().is_some());
    let r = h.tap(PointerKind::Mouse, "Item1");
    assert!(r.contains(&UiRequest::StopTextInput), "{r:?}");
    assert_eq!(calls.borrow().last(), Some(&PageCall::Button("a".into())));
    // A controller's Down (the keyboard's arrows edit the text).
    h.tap(PointerKind::Mouse, "Field.name");
    h.pad(crate::nav::MenuAction::Down, true);
    h.pad(crate::nav::MenuAction::Down, false);
    assert!(h.ui.text_field().is_none());
    // The zone, lit by the shell.
    let plain = h.actor("Zone.z").clone();
    h.ui.broadcast(Message::new(PAGE_TARGET).with("Zone", Param::Str("z".into())));
    h.run(0.05);
    assert_ne!(format!("{plain:?}"), format!("{:?}", h.actor("Zone.z")));
    // Lit, it is still found under a drop.
    assert!(h.ui.visible_bounds("Zone.z").is_some());
}
