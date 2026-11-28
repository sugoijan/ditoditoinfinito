//! The experimental settings screen (`docs/plans/ui-levels.md`, section 5
//! and stage 4): a page of its own, as `about:config` is a browser's, for
//! the switches that are not for players (`crate::state::experimental`).
//! Reached by its address on the web (`#/experimental`) and a flag on
//! desktop (`--screen experimental`), then from Options → Interface →
//! Troubleshooting at Advanced once it was opened ([`in_options`]); never
//! from a place a newcomer lands on.
//!
//! The page says first what these are (for testing, not needed to play,
//! may break things or go), then each switch under its group: its name,
//! its key, what it does (English data from `experimental.toml`), what it
//! is now against its default (in gold where it differs, and where the
//! page's address gave it for the page alone), how the flags and
//! addresses set it, and Turn on or Turn off with Reset; then Reset all
//! and Forget this screen. A change is kept at once, as Options' are.
//!
//! [`in_options`]: crate::state::experimental::in_options

use std::cell::RefCell;
use std::rc::Rc;

use ddi_l10n::UiText;
use ddi_l10n::msg::{common, pages as pm};

use super::Env;
use super::data::Platform;
use super::dialog::ConfirmDialog;
use super::page::{
    Action, Block, PAGE_CHANGED, PageCall, PageDoc, PageFeed, PageScreen, PageShell, PageTitle,
};
use crate::message::Message;
use crate::screen::Cx;
use crate::state::experimental::{self as exp, Entry};
use crate::state::settings::Settings;

/// The screen's name (the debug state's `screen`).
pub const NAME: &str = "experimental";

/// How the screen keeps a change: the edit made on the settings, which
/// are then kept. [`stored`] for the stored settings; desktop's Options,
/// under which the screen opens, gives its own model's, so that a change
/// here is not undone by the next one made there.
pub type Store = Rc<dyn Fn(&mut dyn FnMut(&mut Settings)) -> Settings>;

/// The stored settings: loaded, edited, saved. Returns them as kept.
pub fn stored() -> Store {
    Rc::new(|edit| {
        let mut s = Settings::load();
        edit(&mut s);
        s.save();
        s
    })
}

/// The settings as `store` has them now.
fn current(store: &Store) -> Settings {
    store(&mut |_| {})
}

/// What a button on the page does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Press {
    /// The switch `key` turned on (`true`) or off.
    Turn(String, bool),
    /// The switch `key` back to its default.
    Reset(String),
    ResetAll,
    /// Everything back, and the screen out of Options.
    Forget,
}

/// The press of button `id` on `settings`' page.
pub fn press(id: &str, settings: &Settings) -> Option<Press> {
    if let Some(key) = id.strip_prefix("turn:") {
        let e = exp::ENTRIES.iter().find(|e| e.key == key)?;
        return Some(Press::Turn(key.into(), !exp::value(settings, e)));
    }
    if let Some(key) = id.strip_prefix("reset:") {
        return Some(Press::Reset(key.into()));
    }
    match id {
        "reset-all" => Some(Press::ResetAll),
        "forget" => Some(Press::Forget),
        _ => None,
    }
}

/// `press` made on `s`.
pub fn apply(press: &Press, s: &mut Settings) {
    match press {
        Press::Turn(key, on) => exp::set(s, key, *on),
        Press::Reset(key) => exp::reset(s, key),
        Press::ResetAll => exp::reset_all(s),
        Press::Forget => {
            exp::reset_all(s);
            s.experimental_seen = false;
        }
    }
}

/// How this platform opens the page again (Forget this screen's line).
fn way_back(platform: Platform) -> &'static str {
    match platform {
        Platform::Web => "#/experimental",
        Platform::Desktop => "--screen experimental",
    }
}

/// The page on `s` for `platform`; `notice`: a line after a press.
pub fn doc(s: &Settings, platform: Platform, notice: Option<&str>) -> PageDoc {
    let mut b = vec![Block::Note(pm::experimental_warning().into())];
    // The switches' own words are English data (open question 11).
    if ddi_l10n::current().tag != "en" {
        b.push(Block::Muted(pm::experimental_english().into()));
    }
    if let Some(n) = notice {
        b.push(Block::Text(n.into()));
    }
    let mut group = None;
    for e in exp::entries(platform) {
        if group != Some(e.group) {
            b.push(Block::Heading(UiText::data(e.group).into()));
            group = Some(e.group);
        }
        entry(&mut b, s, e, platform);
    }
    b.push(Block::Gap);
    // What Forget does above its button, so that the focus on the page's
    // last button shows it too.
    b.push(Block::Muted(
        pm::experimental_forget_about(UiText::data(way_back(platform))).into(),
    ));
    let any = exp::changed_count(s, platform) > 0;
    b.push(Block::Actions(vec![
        Action::new("reset-all", String::from(pm::experimental_reset_all())).off(!any),
        Action::new("forget", String::from(pm::experimental_forget())),
    ]));
    PageDoc {
        title: PageTitle::new(pm::experimental_title(), pm::experimental_heading()),
        blocks: b,
    }
}

/// Entry `e`'s blocks.
fn entry(b: &mut Vec<Block>, s: &Settings, e: &Entry, platform: Platform) {
    let switch = exp::switch(e.key);
    let on = exp::value(s, e);
    let given = switch.and_then(|w| w.given());
    let stored = switch.is_some_and(|w| w.stored(s));
    b.push(Block::Subheading(UiText::data(e.name).into()));
    b.push(Block::Mono(e.key.into()));
    b.push(Block::Text(UiText::data(e.line).into()));
    if e.play {
        b.push(Block::Note(pm::experimental_plays().into()));
    }
    // What it is now: the page's own where an address gave it otherwise
    // than kept, else against its default.
    b.push(match (given.filter(|g| *g != stored), on, on == e.default) {
        (Some(true), ..) => Block::Note(pm::experimental_given_on().into()),
        (Some(false), ..) => Block::Note(pm::experimental_given_off().into()),
        (None, true, true) => Block::Muted(pm::experimental_on_default().into()),
        (None, false, true) => Block::Muted(pm::experimental_off_default().into()),
        (None, true, false) => Block::Note(pm::experimental_on_changed().into()),
        (None, false, false) => Block::Note(pm::experimental_off_changed().into()),
    });
    // How else it is set, on this platform.
    match platform {
        Platform::Desktop if !e.desktop_flag.is_empty() => b.push(Block::Muted(
            pm::experimental_flag(UiText::data(e.desktop_flag)).into(),
        )),
        Platform::Web if !e.web_address.is_empty() => b.push(Block::Muted(
            pm::experimental_address(UiText::data(e.web_address)).into(),
        )),
        _ => {}
    }
    let turn = if on {
        pm::experimental_turn_off()
    } else {
        pm::experimental_turn_on()
    };
    b.push(Block::Actions(vec![
        Action::new(format!("turn:{}", e.key), String::from(turn)),
        Action::new(format!("reset:{}", e.key), String::from(common::reset()))
            .off(on == e.default && given.is_none()),
    ]));
}

/// The screen on `store`'s settings; `leave`: what Back does (the web's
/// history, desktop's stack). Opening it marks it seen: Options lists it
/// from now (`crate::state::experimental::in_options`).
pub fn screen(env: &Env, store: Store, leave: Rc<dyn Fn(&mut Cx)>) -> PageScreen {
    let platform = env.platform;
    let settings = store(&mut |s| s.experimental_seen = true);
    let feed = PageFeed::default();
    let notice: Rc<RefCell<Option<String>>> = Rc::default();
    // A press made: kept, and the page as it is after it.
    let made: Made = {
        let (store, notice) = (store.clone(), notice.clone());
        Rc::new(move |p, cx| {
            let after = store(&mut |s| apply(p, s));
            *notice.borrow_mut() = (*p == Press::Forget).then(|| pm::experimental_forgotten().into());
            // Options under the screen (desktop's stack) shows the change
            // on the way back (`options_shell::experimental_store`).
            cx.broadcast(Message::new(super::options::OPTIONS_CHANGED));
            doc(&after, platform, notice.borrow().as_deref())
        })
    };
    let shell: PageShell = {
        let (env, feed) = (env.clone(), feed.clone());
        Rc::new(move |call, cx| match call {
            PageCall::Leave => {
                leave(cx);
                None
            }
            PageCall::Button(id) => {
                let p = press(&id, &current(&store))?;
                if matches!(p, Press::ResetAll | Press::Forget) {
                    confirm(&env, &feed, made.clone(), p, cx);
                    return None;
                }
                Some(made(&p, cx))
            }
            PageCall::Field { .. } => None,
        })
    };
    PageScreen::new(env.clone(), NAME, doc(&settings, platform, None)).with_shell(shell, feed)
}

/// A press made: kept, and the page as it is after it.
type Made = Rc<dyn Fn(&Press, &mut Cx) -> PageDoc>;

/// Asks once before `p` (Reset all, Forget this screen), Cancel chosen
/// first, as the game's other resets do; once made, the page in `feed`.
fn confirm(
    env: &Env,
    feed: &PageFeed,
    made: Made,
    p: Press,
    cx: &mut Cx,
) {
    let (title, body, yes) = confirm_texts(&p);
    let feed = feed.clone();
    cx.overlay(
        ConfirmDialog::new(
            env.clone(),
            &title,
            &body,
            &yes,
            Rc::new(move |cx| {
                *feed.borrow_mut() = Some(made(&p, cx));
                cx.broadcast(Message::new(PAGE_CHANGED));
            }),
        ),
        true,
    );
}

/// The question before `p`: its title, text and button.
pub fn confirm_texts(p: &Press) -> (String, String, String) {
    match p {
        Press::Forget => (
            pm::experimental_forget_title().into(),
            pm::experimental_forget_body().into(),
            pm::experimental_forget().into(),
        ),
        _ => (
            pm::experimental_reset_all_title().into(),
            pm::experimental_reset_all_body().into(),
            common::reset().into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::settings;
    use ddi_platform::storage::{self, MemoryStore};

    fn texts(doc: &PageDoc) -> Vec<String> {
        doc.blocks
            .iter()
            .flat_map(|b| match b {
                Block::Heading(t)
                | Block::Subheading(t)
                | Block::Text(t)
                | Block::Muted(t)
                | Block::Note(t)
                | Block::Mono(t) => vec![t.clone()],
                Block::Actions(a) => a.iter().map(|a| a.label.clone()).collect(),
                _ => vec![],
            })
            .collect()
    }

    fn buttons(doc: &PageDoc) -> Vec<(String, bool)> {
        doc.blocks
            .iter()
            .flat_map(|b| match b {
                Block::Actions(a) => a.iter().map(|a| (a.id.clone(), a.off)).collect(),
                _ => vec![],
            })
            .collect()
    }

    /// The page lists every entry with its key, what it does, its value
    /// against its default and its buttons; the warning first.
    #[test]
    fn the_page_lists_each_switch_with_its_value_and_default() {
        settings::forget_given(true, true, true);
        let s = Settings::default();
        for platform in [Platform::Web, Platform::Desktop] {
            let d = doc(&s, platform, None);
            assert_eq!(d.blocks[0], Block::Note(pm::experimental_warning().into()));
            let t = texts(&d);
            for e in exp::ENTRIES {
                assert!(t.contains(&e.key.to_string()), "{}", e.key);
                assert!(t.contains(&e.line.to_string()), "{}", e.key);
            }
            let off: String = pm::experimental_off_default().into();
            assert_eq!(t.iter().filter(|x| **x == off).count(), exp::ENTRIES.len());
            // Each switch's Reset off at its default, Reset all too.
            let b = buttons(&d);
            for e in exp::ENTRIES {
                assert!(b.contains(&(format!("turn:{}", e.key), false)));
                assert!(b.contains(&(format!("reset:{}", e.key), true)));
            }
            assert!(b.contains(&("reset-all".into(), true)));
            let share = exp::ENTRIES.iter().find(|e| e.key == "share.uploader").unwrap();
            let flag: String = pm::experimental_flag(UiText::data(share.desktop_flag)).into();
            assert_eq!(t.contains(&flag), platform == Platform::Desktop);
        }
    }

    /// Turned on, a switch is kept, says it differs from its default, and
    /// its Reset and Reset all come on; Reset puts it back.
    #[test]
    fn a_switch_turned_on_is_kept_marked_and_reset() {
        storage::install(Rc::new(MemoryStore::default()));
        settings::forget_given(true, true, true);
        let store = stored();
        let p = press("turn:rooms.rules_first", &current(&store)).unwrap();
        assert_eq!(p, Press::Turn("rooms.rules_first".into(), true));
        store(&mut |s| apply(&p, s));
        let s = Settings::load();
        assert!(s.multiplayer_rules_first && s.rules_first());
        let d = doc(&s, Platform::Web, None);
        let changed: String = pm::experimental_on_changed().into();
        assert!(texts(&d).contains(&changed));
        let b = buttons(&d);
        assert!(b.contains(&("reset:rooms.rules_first".into(), false)));
        assert!(b.contains(&("reset-all".into(), false)));
        let p = press("reset:rooms.rules_first", &s).unwrap();
        store(&mut |s| apply(&p, s));
        assert!(!Settings::load().multiplayer_rules_first);
    }

    /// A value the page's address gave is shown as the page's alone, the
    /// kept one beside it; turning the switch is the player's own, kept.
    #[test]
    fn an_address_value_is_shown_as_the_page_s_alone() {
        storage::install(Rc::new(MemoryStore::default()));
        settings::forget_given(true, true, true);
        settings::give_sharing(Some(true));
        let store = stored();
        let d = doc(&current(&store), Platform::Web, None);
        let given: String = pm::experimental_given_on().into();
        assert!(texts(&d).contains(&given));
        assert!(buttons(&d).contains(&("reset:share.uploader".into(), false)));
        // Turn off: the address's let go, Off kept.
        let p = press("turn:share.uploader", &current(&store)).unwrap();
        assert_eq!(p, Press::Turn("share.uploader".into(), false));
        store(&mut |s| apply(&p, s));
        let s = Settings::load();
        assert!(!s.sharing_on());
        assert!(!texts(&doc(&s, Platform::Web, None)).contains(&given));
    }

    /// Opening the screen marks it seen; Forget this screen sets
    /// everything back and clears the mark.
    #[test]
    fn forget_sets_everything_back_and_takes_it_out_of_options() {
        storage::install(Rc::new(MemoryStore::default()));
        settings::forget_given(true, true, true);
        let store = stored();
        store(&mut |s| s.experimental_seen = true);
        store(&mut |s| apply(&Press::Turn("rooms.watch_link".into(), true), s));
        assert!(exp::in_options(&Settings::load(), Platform::Desktop));
        store(&mut |s| apply(&Press::Forget, s));
        let s = Settings::load();
        assert!(!s.multiplayer_watch_link && !s.experimental_seen);
        assert!(!exp::in_options(&s, Platform::Desktop));
    }

    /// The screen by keys: a switch turned on by Enter is kept; Reset all
    /// asks first, Cancel focused, then resets; Back leaves.
    #[test]
    fn the_screen_turns_a_switch_and_asks_before_reset_all() {
        use crate::TextMode;
        use crate::testing::Harness;
        storage::install(Rc::new(MemoryStore::default()));
        settings::forget_given(true, true, true);
        let env = super::super::test_kit::env();
        let state = env.state.clone();
        let left: Rc<std::cell::Cell<bool>> = Rc::default();
        let l = left.clone();
        let mut h = Harness::new(super::super::DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
        h.push(screen(&env, stored(), Rc::new(move |_| l.set(true))));
        h.run(0.3);
        assert_eq!(state.borrow().screen, NAME);
        assert!(Settings::load().experimental_seen, "marked seen");
        let go_to = |h: &mut Harness, item: &str| {
            let items = state.borrow().page_items.clone();
            let at = items.iter().position(|i| i == item).unwrap_or_else(|| panic!("{items:?}"));
            let mut moves = 0;
            while state.borrow().page_focus != at as i32 {
                h.key("ArrowDown", "ArrowDown");
                moves += 1;
                assert!(moves <= items.len() + 1, "{item}");
            }
        };
        go_to(&mut h, "button:turn:rooms.watch_link");
        h.key("Enter", "Enter");
        h.run(0.1);
        assert!(Settings::load().multiplayer_watch_link);
        go_to(&mut h, "button:reset-all");
        h.key("Enter", "Enter");
        h.run(0.3);
        assert_eq!(h.ui.focused(), Some("Cancel"), "Cancel first");
        // Cancel: nothing changes.
        h.key("Enter", "Enter");
        h.run(0.3);
        assert!(Settings::load().multiplayer_watch_link);
        go_to(&mut h, "button:reset-all");
        h.key("Enter", "Enter");
        h.run(0.3);
        h.key("ArrowRight", "ArrowRight");
        assert_eq!(h.ui.focused(), Some("Confirm"));
        h.key("Enter", "Enter");
        h.run(0.3);
        assert!(!Settings::load().multiplayer_watch_link);
        assert!(Settings::load().experimental_seen, "Reset all keeps the screen listed");
        h.key("Escape", "Escape");
        h.run(0.3);
        assert!(left.get());
    }
}
