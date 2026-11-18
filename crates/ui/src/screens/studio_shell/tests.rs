//! The studio's page on a fake platform: what each press does to the
//! draft, and what it asks of the platform, once for both shells.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_library::announcer::draft::Draft;
use ddi_library::announcer::examples::Set;
use ddi_library::announcer::{Bundled, Line};

use super::*;
use crate::screens::page::PageScreen;
use crate::screens::DESIGN;
use crate::screens::test_kit::env;
use crate::testing::Harness;
use crate::{PointerKind, TextMode};

const MP3: &[u8] = b"ID3\x04\x00";

/// A platform that says what it was asked, in order.
struct Fake {
    state: StudioState,
    env: Env,
    asked: RefCell<Vec<String>>,
}

impl Fake {
    fn new() -> Rc<Fake> {
        Rc::new(Fake {
            state: StudioState::new(Draft::default(), String::new()),
            env: env(),
            asked: RefCell::default(),
        })
    }

    fn tell(&self, what: impl Into<String>) {
        self.asked.borrow_mut().push(what.into());
    }

    /// What it was asked since last taken.
    fn asked(&self) -> Vec<String> {
        std::mem::take(&mut *self.asked.borrow_mut())
    }
}

impl StudioHost for Fake {
    fn state(&self) -> &StudioState {
        &self.state
    }
    fn env(&self) -> Option<Env> {
        Some(self.env.clone())
    }
    fn example_pack(&self, set: Set) -> Option<Bundled> {
        self.tell(format!("pack {}", set.key()));
        None
    }
    fn save(&self) {
        self.tell("save");
    }
    fn show(&self) {
        self.tell("show");
    }
    fn publish(&self) {
        self.tell("publish");
    }
    fn forget_export(&self) {
        self.tell("forget");
    }
    fn stop(&self) {
        self.tell("stop");
    }
    fn leave(&self, _cx: &mut Cx) {
        self.tell("leave");
    }
    fn pick_recordings(self: &Rc<Self>, line: Line, script: Option<u64>) {
        self.tell(format!("pick {} {script:?}", line.folder()));
    }
    fn pick_pack(self: &Rc<Self>, folder: bool) {
        self.tell(format!("pick pack {folder}"));
    }
    fn play_take(self: &Rc<Self>, id: u64) -> bool {
        self.tell(format!("play {id}"));
        false
    }
    fn play_example(self: &Rc<Self>, line: Line, i: usize) -> bool {
        self.tell(format!("example {} {i}", line.folder()));
        true
    }
    fn removed(&self, id: u64) {
        self.tell(format!("removed {id}"));
    }
    fn export(self: &Rc<Self>) -> bool {
        self.tell("export");
        false
    }
    fn save_zip(self: &Rc<Self>) -> bool {
        self.tell("save zip");
        true
    }
    fn use_in_game(self: &Rc<Self>) -> bool {
        self.tell("use");
        false
    }
    fn hear(self: &Rc<Self>, act: Act) {
        self.tell(format!("hear {act:?}"));
    }
}

/// Each press asks the platform the same of both shells: a change of the
/// draft is kept (and an export made before let go), what only a
/// platform does is its, and the page shows at once where it said so.
#[test]
fn each_press_changes_the_draft_or_asks_the_platform() {
    let host = Fake::new();
    let ready = host.state.draft.borrow_mut().add("gameplay ready.mp3", 1, MP3);
    // A switch of the draft: kept, what plays stopped, shown at once.
    assert!(act(&host, Act::Tidy).is_some());
    assert!(!host.state.draft.borrow().tidy);
    assert_eq!(host.asked(), ["stop", "forget", "save", "publish"]);
    // The examples' set: its pack asked for.
    let before = host.state.set();
    assert!(act(&host, Act::Examples).is_some());
    assert_ne!(host.state.set(), before);
    assert_eq!(
        host.asked(),
        [format!("pack {}", host.state.set().key()), "publish".into()]
    );
    // How the page shows: nothing asked.
    assert!(act(&host, Act::Details).is_some());
    assert!(host.asked().is_empty());
    // The platform's, each shown at once only where it says so.
    assert!(act(&host, Act::Add(Line::Cheer)).is_none());
    assert!(act(&host, Act::ImportFolder).is_none());
    assert!(act(&host, Act::Play(ready)).is_none());
    assert!(act(&host, Act::Example(Line::Cheer, 1)).is_some());
    assert!(act(&host, Act::Export).is_none());
    assert!(act(&host, Act::Save).is_some());
    assert!(act(&host, Act::UseInGame).is_none());
    assert!(act(&host, Act::Hear(Line::Cheer)).is_none());
    assert_eq!(
        host.asked(),
        [
            "pick evaluation cheer None",
            "pick pack true",
            &format!("play {ready}"),
            "example evaluation cheer 1",
            "export",
            "save zip",
            "use",
            "hear Hear(Cheer)",
        ]
    );
    // A text line's recording is picked for its line.
    let script = host.state.draft.borrow_mut().write(Line::Cleared);
    act(&host, Act::AddTo(script));
    assert_eq!(host.asked(), [format!("pick gameplay cleared Some({script})")]);
    assert!(act(&host, Act::AddTo(999)).is_none());
    // A take removed: off the draft, kept so, then its file let go.
    assert!(act(&host, Act::Remove(ready)).is_some());
    assert!(host.state.draft.borrow().take(ready).is_none());
    assert_eq!(
        host.asked(),
        ["forget", "save", &format!("removed {ready}"), "publish"]
    );
}

/// Picked recordings go on the line asked, the first in its text line's
/// place, as long as the draft has room; what the page says after.
#[test]
fn recordings_are_taken_in_on_their_line() {
    ddi_l10n::set_language("en");
    let state = StudioState::new(Draft::default(), String::new());
    let script = state.draft.borrow_mut().write(Line::Ready);
    state.draft.borrow_mut().scripts[0].words = "Get ready!".into();
    let mut first = Some(script);
    let a = state
        .take_in("New Recording 1.m4a", 10, b"", Some(Line::Ready), &mut first, None)
        .unwrap();
    let b = state
        .take_in("New Recording 2.m4a", 10, b"", Some(Line::Ready), &mut first, None)
        .unwrap();
    let named = state
        .take_in("evaluation aa.mp3", 10, MP3, None, &mut None, None)
        .unwrap();
    let d = state.draft.borrow();
    assert_eq!(d.take(a).unwrap().words.as_deref(), Some("Get ready!"));
    assert_eq!(d.take(b).unwrap().words, None);
    assert_eq!(d.take(b).unwrap().line, Some(Line::Ready));
    assert_eq!(
        d.take(named).unwrap().line,
        Some(Line::Grade(ddi_library::announcer::Grade::Aa))
    );
    assert!(d.scripts.is_empty());
    drop(d);
    // A file too large to be a recording is left out.
    assert_eq!(
        state.take_in("song.wav", u64::MAX, b"RIFF", None, &mut None, None),
        None
    );
    assert_eq!(added_status(3, 1, 0, None).to_string(), m::status_added(3, 1).to_string());
    assert!(added_status(2, 0, 1, None).contains("1 was left out"));
    assert!(
        added_status(0, 0, 0, Some("nope")).to_string()
            == m::status_files_unreadable(crate::screens::errors::error_text("nope")).to_string()
    );
}

/// A field typed into changes the draft and keeps it; Move… and Add a
/// combo line… open their dialogs over the page, and what is chosen
/// there is kept and shown.
#[test]
fn fields_and_dialogs_change_the_draft() {
    ddi_l10n::set_language("en");
    let host = Fake::new();
    let take = host.state.draft.borrow_mut().add("New Recording.mp3", 1, MP3);
    let shell: super::super::page::PageShell = {
        let weak = Rc::downgrade(&host);
        Rc::new(move |c, cx| call(&weak.upgrade()?, c, cx))
    };
    let feed: super::super::page::PageFeed = Rc::default();
    let page = PageScreen::new(host.env.clone(), studio::NAME, doc_of(&*host))
        .with_shell(shell, feed.clone());
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
    h.push(page);
    h.run(0.3);
    h.tap(PointerKind::Mouse, "Field.name");
    h.sync("My Voice", [8, 8], None);
    assert_eq!(host.state.draft.borrow().name, "My Voice");
    assert_eq!(host.asked(), ["forget", "save", "publish"]);
    // Which line? (Move… unsorted): heard as it is chosen, then put on
    // the line chosen (the first, song select's welcome).
    let items = host.env.state.borrow().page_items.clone();
    let at = items
        .iter()
        .position(|i| *i == format!("button:move:{take}"))
        .expect("Move…");
    h.tap(PointerKind::Mouse, &format!("Item{at}"));
    assert_eq!(host.asked(), [format!("play {take}")]);
    h.run(0.3);
    h.key("Enter", "Enter");
    h.run(0.3);
    assert_eq!(
        host.state.draft.borrow().take(take).unwrap().line,
        Some(Line::SelectMusicIntro)
    );
    assert_eq!(host.asked(), ["stop", "forget", "save", "show"]);
    // Name its language…: the first offered (English) taken.
    h.run(0.5);
    *feed.borrow_mut() = Some(doc_of(&*host));
    h.ui.broadcast(crate::message::Message::new(super::super::page::PAGE_CHANGED));
    h.run(0.3);
    let items = host.env.state.borrow().page_items.clone();
    let at = items
        .iter()
        .position(|i| i == "button:add-language")
        .unwrap_or_else(|| panic!("{items:?}"));
    h.tap(PointerKind::Mouse, &format!("Item{at}"));
    h.run(0.3);
    h.key("Enter", "Enter");
    h.run(0.3);
    assert_eq!(host.state.draft.borrow().languages, ["en"]);
    assert_eq!(host.asked(), ["stop", "forget", "save", "show"]);
    // Tags…: the first tag offered (a suggestion) put on the take, then
    // a tag typed (not a word first).
    *feed.borrow_mut() = Some(doc_of(&*host));
    h.ui.broadcast(crate::message::Message::new(super::super::page::PAGE_CHANGED));
    h.run(0.3);
    let items = host.env.state.borrow().page_items.clone();
    let at = items
        .iter()
        .position(|i| *i == format!("button:tags:{take}"))
        .unwrap_or_else(|| panic!("{items:?}"));
    h.tap(PointerKind::Mouse, &format!("Item{at}"));
    h.run(0.3);
    h.key("Enter", "Enter");
    h.run(0.3);
    assert_eq!(host.state.draft.borrow().take(take).unwrap().tags, ["calm"]);
    assert_eq!(host.asked(), ["forget", "save", "show"]);
}

/// A pack made before with a script, picked: its pool's places taken by
/// its sounds, its languages and switches by the draft, and what it says
/// that the draft cannot carry over shown under the status.
#[test]
fn a_scripted_pack_picked_goes_where_its_pool_puts_it() {
    use ddi_library::announcer::draft::{Adopted, Lost, PackPlan, Planned, Switch};
    ddi_l10n::set_language("en");
    let state = StudioState::new(Draft::default(), String::new());
    let mut plan = PackPlan {
        languages: vec!["en".into(), "ja".into()],
        mixed: true,
        switches: vec![Switch {
            tag: "rude".into(),
            label: "Rude".into(),
            default: false,
        }],
        lost: vec![Lost::Script, Lost::Weights],
        ..PackPlan::default()
    };
    plan.takes.insert(
        "P/shared/a.ogg".into(),
        Planned {
            line: Line::Combo(100),
            also: vec![Line::Cheer],
            lang: Some("ja".into()),
            tags: vec!["rude".into()],
        },
    );
    let got = state.adopt(Adopted::new("P".into(), None).with_plan(Some(plan.clone())));
    assert_eq!(got.as_ref(), Some(&plan));
    {
        let d = state.draft.borrow();
        assert_eq!(d.name, "P");
        assert_eq!(d.languages, ["en", "ja"]);
        assert!(d.mixed && d.switch_of("rude").is_some());
    }
    assert_eq!(
        state.view.borrow().lost,
        [
            m::lost_script().to_string(),
            m::lost_weights().to_string()
        ]
    );
    assert!(format!("{:?}", state.doc(Default::default()).blocks).contains(&m::lost_weights().to_string()));
    let id = state
        .take_in("P/shared/a.ogg", 9, b"OggS", None, &mut None, got.as_ref())
        .unwrap();
    let other = state
        .take_in("P/evaluation cheer/b.ogg", 9, b"OggS", None, &mut None, got.as_ref())
        .unwrap();
    let d = state.draft.borrow();
    let t = d.take(id).unwrap();
    assert_eq!((t.line, t.also.clone()), (Some(Line::Combo(100)), vec![Line::Cheer]));
    assert_eq!((t.lang.as_deref(), t.tags.clone()), (Some("ja"), vec!["rude".to_string()]));
    // Not in its pool: by its folder, as before.
    assert_eq!(d.take(other).unwrap().line, Some(Line::Cheer));
}
