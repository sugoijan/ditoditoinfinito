use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ddi_library::announcer::{Comment, Grade};
use ddi_platform::storage::{self, MemoryStore};
use ddi_ui::event::{MenuEvent, UiEvent};
use ddi_ui::look::Plain;
use ddi_ui::request::UiRequest;
use ddi_ui::screens::DESIGN;
use ddi_ui::testing::Harness;
use ddi_ui::{TextMode, screens::start};

use super::super::super::tests::Fake;
use super::*;

type Sound = f64;

/// A timer's handle: dropped, it is off.
struct Handle(Rc<Cell<bool>>);

impl Drop for Handle {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

type Pending = (f64, Rc<Cell<bool>>, Box<dyn FnOnce()>);
/// What the studio was told: its status, the line, what was heard, by whom.
type Told = (String, Line, Vec<String>, Said);

#[derive(Default)]
struct Inner {
    now: Cell<f64>,
    timers: RefCell<Vec<Pending>>,
    ui: RefCell<Vec<UiFn>>,
    presses: RefCell<Vec<MenuAction>>,
    previews: RefCell<Vec<String>>,
    voice: RefCell<Option<StagedVoice<Sound>>>,
    heard: RefCell<Option<Told>>,
    gave_up: Cell<bool>,
}

#[derive(Clone)]
struct Host(Rc<Inner>, Env);

impl SceneHost for Host {
    type Sound = Sound;

    fn now_ms(&self) -> f64 {
        self.0.now.get()
    }
    fn timer(&self, seconds: f64, f: Box<dyn FnOnce()>) -> Timer {
        let on = Rc::new(Cell::new(true));
        self.0
            .timers
            .borrow_mut()
            .push((self.now_ms() + seconds * 1000.0, on.clone(), f));
        Box::new(Handle(on))
    }
    fn env(&self) -> Option<Env> {
        Some(self.1.clone())
    }
    fn on_ui(&self, f: UiFn) {
        self.0.ui.borrow_mut().push(f);
    }
    fn show(&self, f: UiFn) -> bool {
        self.0.ui.borrow_mut().push(f);
        true
    }
    fn press(&self, action: MenuAction) {
        self.0.presses.borrow_mut().push(action);
    }
    fn preview_open(&self, _: f32) {
        self.0.previews.borrow_mut().push("open".into());
    }
    fn preview_select(&self, song: Option<String>) {
        self.0
            .previews
            .borrow_mut()
            .push(format!("select {}", song.unwrap_or_default()));
    }
    fn preview_stop(&self) {}
    fn preview_volume(&self, _: f32) {}
    fn preview_hush(&self) {
        self.0.previews.borrow_mut().push("hush".into());
    }
    fn preview_close(&self) {
        self.0.previews.borrow_mut().push("close".into());
    }
    fn stage_voice(&self, staged: Option<StagedVoice<Sound>>) {
        *self.0.voice.borrow_mut() = staged;
    }
    fn open_results_sound(&self, _: Line) -> bool {
        true
    }
    fn say_results(
        &self,
        scene: &ResultsScene,
        voice: Voice<Sound>,
    ) -> Option<(Vec<(String, f64)>, f64)> {
        let mut audio = Fake { voiced: Vec::new() };
        let now = self.now_ms() / 1000.0;
        Some((say_results(&mut audio, now, scene, voice, 1.0), now))
    }
    fn stop_sound(&self) {}
    fn changed(&self) {}
    fn publish(&self) {}
    fn gave_up(&self, _: UiText) {
        self.0.gave_up.set(true);
    }
    fn heard(&self, status: UiText, line: Line, spoken: Vec<String>, said: Said) {
        *self.0.heard.borrow_mut() = Some((status.to_string(), line, spoken, said));
    }
}

struct Test {
    h: Harness,
    host: Host,
    director: Rc<Director<Host>>,
}

fn test() -> Test {
    storage::install(Rc::new(MemoryStore::default()));
    let env = Env::new(Rc::new(Plain), Rc::new(|s: &str| s.to_lowercase()));
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, TextMode::Own);
    // What the scene comes over (the studio's place).
    h.push(start::StartScreen::new(
        env.clone(),
        false,
        Rc::new(|_, _| {}),
    ));
    h.run(1.0);
    let host = Host(Rc::default(), env);
    host.0.now.set(1_000_000.0);
    Test {
        h,
        director: Director::new(host.clone()),
        host,
    }
}

/// A voice saying `lines` for a second each.
fn voiced(lines: &[Line]) -> Voiced<Sound> {
    let mut voice = Voice::default();
    let mut lengths = HashMap::new();
    for &l in lines {
        voice.expect([l]);
        voice.add(l, 1.0);
        lengths.insert(l, 1.0);
    }
    (voice, lengths, HashMap::new())
}

impl Test {
    /// Frames for `seconds`: timers run as they come due, what they ask
    /// of the UI done, and what the screens ask of the voice answered.
    fn run(&mut self, seconds: f64) {
        let frames = (seconds * 60.0).round() as usize;
        for _ in 0..frames {
            self.host.0.now.set(self.host.0.now.get() + 1000.0 / 60.0);
            let due: Vec<_> = {
                let now = self.host.0.now.get();
                let mut t = self.host.0.timers.borrow_mut();
                let (due, later): (Vec<_>, Vec<_>) = t.drain(..).partition(|(at, ..)| *at <= now);
                *t = later;
                due
            };
            for (_, on, f) in due {
                if on.get() {
                    f();
                }
            }
            let todo: Vec<UiFn> = self.host.0.ui.borrow_mut().drain(..).collect();
            for f in todo {
                f(&mut self.h.ui);
            }
            let presses: Vec<MenuAction> = self.host.0.presses.borrow_mut().drain(..).collect();
            for action in presses {
                for down in [true, false] {
                    self.h.send(UiEvent::Menu(MenuEvent {
                        action,
                        down,
                        source: u32::MAX,
                        gesture: false,
                        time: 0.0,
                    }));
                }
            }
            self.h.frame(1.0 / 60.0);
            for r in std::mem::take(&mut self.h.requests) {
                if let UiRequest::Announce(line) = r {
                    self.announce(line);
                }
            }
        }
    }

    /// Song select's line, as a shell says it.
    fn announce(&self, line: Line) {
        let told = {
            let mut v = self.host.0.voice.borrow_mut();
            v.as_mut().map(|s| {
                let spoke = s.voice.next(line).map(|(found, b)| (found, *b));
                (s.told.clone(), spoke)
            })
        };
        if let Some((told, spoke)) = told {
            told(line, spoke);
        }
    }

    /// Runs until the scene is over, at most `seconds`.
    fn until_over(&mut self, seconds: f64) -> Told {
        let mut left = seconds;
        while left > 0.0 && self.host.0.heard.borrow().is_none() {
            self.run(0.1);
            left -= 0.1;
        }
        self.host
            .0
            .heard
            .borrow()
            .clone()
            .unwrap_or_else(|| panic!("never over: {:?}", self.director.state()))
    }

    fn top(&self) -> String {
        self.h
            .ui
            .layers()
            .last()
            .map(|l| l.name.clone())
            .unwrap_or_default()
    }
}

/// A comment's scene: the intro said as song select shows, the chart
/// pointed at and chosen after it, its comment said, the frame gone.
#[test]
fn a_comments_scene_goes_from_the_intro_to_its_comment_and_back() {
    let mut t = test();
    let line = Line::SelectMusic(Comment::Hard);
    let (n, scene) = t.director.begin_menu(line, Said::Draft).unwrap();
    assert!(t.director.busy());
    t.director
        .show_menu(n, scene, voiced(&[Line::SelectMusicIntro, line]));
    t.run(0.5);
    let st = t.director.state().unwrap();
    assert_eq!(st.screen, Shows::Select);
    assert_eq!(st.spoken, ["select music intro"]);
    assert!(!st.chosen && st.mark.is_none(), "{st:?}");
    t.run(1.0);
    let st = t.director.state().unwrap();
    assert!(
        st.mark.is_some(),
        "pointed at once the intro is said: {st:?}"
    );
    let (status, heard_line, spoken, said) = t.until_over(10.0);
    assert_eq!(heard_line, line);
    assert_eq!(spoken, ["select music intro", "select music comment hard"]);
    assert_eq!(said, Said::Draft);
    assert!(!status.is_empty());
    assert!(!t.director.busy());
    let previews = t.host.0.previews.borrow().clone();
    assert_eq!(previews.first().unwrap(), "open");
    assert!(previews.contains(&"hush".to_string()), "{previews:?}");
    assert_eq!(previews.last().unwrap(), "close");
    assert_eq!(t.director.turns.borrow()[&line], 1);
    assert_eq!(t.director.turns.borrow()[&Line::SelectMusicIntro], 1);
    t.h.run(1.0);
    assert!(
        t.host.0.voice.borrow().is_none(),
        "the announcer chosen again"
    );
    assert_eq!(t.top(), "start", "what it came over shows again");
}

/// The welcome's: from the start menu, Singleplayer chosen on it.
#[test]
fn the_welcomes_scene_comes_from_the_start_menu() {
    let mut t = test();
    let line = Line::SelectMusicIntro;
    let (n, scene) = t
        .director
        .begin_menu(line, Said::Example(Set::English))
        .unwrap();
    t.director.show_menu(n, scene, voiced(&[line]));
    t.run(0.5);
    let st = t.director.state().unwrap();
    assert_eq!(st.screen, Shows::Start);
    assert_eq!(st.mark, Some(Mark::Screen));
    t.run(2.5);
    assert_eq!(t.director.state().unwrap().screen, Shows::Select);
    let (_, _, spoken, said) = t.until_over(10.0);
    assert_eq!(spoken, ["select music intro"]);
    assert_eq!(said, Said::Example(Set::English));
    assert_eq!(t.director.example_turn(Set::English, line), 1);
    assert_eq!(t.director.example_turn(Set::Japanese, line), 0);
}

/// A draft with one take on each of `lines`.
fn draft_saying(lines: &[Line]) -> Draft {
    let mut draft = Draft::default();
    for (n, line) in lines.iter().enumerate() {
        let id = draft.add(&format!("{n}.wav"), 1, &[]);
        draft.take_mut(id).unwrap().line = Some(*line);
    }
    draft
}

/// A results line's: said as the results open, the cheer after an AA.
#[test]
fn a_results_scene_says_its_line_and_the_cheer() {
    let mut t = test();
    let line = Line::Grade(Grade::Aa);
    let (n, scene) = t
        .director
        .begin_results(line, Said::Draft, &draft_saying(&[line, Line::Cheer]))
        .unwrap();
    let lines = results_said_lines(&scene);
    assert!(lines.contains(&line) && lines.contains(&Line::Cheer));
    t.director
        .show_results(n, scene, voiced(&[line, Line::Cheer]));
    t.run(0.5);
    let st = t.director.state().unwrap();
    assert_eq!(st.screen, Shows::Results);
    assert!(st.mark.is_some());
    let (_, _, spoken, _) = t.until_over(12.0);
    assert_eq!(spoken, ["evaluation aa", "evaluation cheer"]);
}

/// A record's and a first clear's open on the results, like any line
/// said there: song select was a whole play ago.
#[test]
fn a_records_and_a_first_clears_scenes_open_on_the_results() {
    for line in [Line::NewRecord, Line::FirstClear] {
        let mut t = test();
        let (n, scene) = t
            .director
            .begin_results(line, Said::Draft, &draft_saying(&[line]))
            .unwrap();
        t.director.show_results(n, scene, voiced(&[line]));
        t.run(0.5);
        assert_eq!(t.director.state().unwrap().screen, Shows::Results);
        let (_, _, spoken, _) = t.until_over(10.0);
        assert_eq!(spoken, [line.folder()]);
    }
}

/// A results scene shows the words of its example alone: the cheer's,
/// not the grade's said before it.
#[test]
fn a_results_scene_shows_only_its_examples_words() {
    let mut t = test();
    let (grade, cheer) = (Line::Grade(Grade::Aa), Line::Cheer);
    let (n, scene) = t
        .director
        .begin_results(cheer, Said::Draft, &draft_saying(&[grade, cheer]))
        .unwrap();
    t.director.show_results(n, scene, voiced(&[grade, cheer]));
    let words = |l: Line| ms::stage_said(studio::name(l)).to_string();
    let (mut saw_grade, mut saw_cheer) = (false, false);
    for _ in 0..240 {
        t.run(1.0 / 60.0);
        let texts = t.h.texts();
        saw_grade |= texts.contains(&words(grade));
        saw_cheer |= texts.contains(&words(cheer));
    }
    assert!(saw_cheer, "the cheer's words never showed");
    assert!(!saw_grade, "the grade's words showed");
}

/// Its way back: the frame's button leaves at once, what was to be said
/// later not heard, and a scene that left does nothing more.
#[test]
fn leaving_a_scene_early_counts_only_what_was_said() {
    let mut t = test();
    let line = Line::SelectMusic(Comment::General);
    let (n, scene) = t.director.begin_menu(line, Said::Draft).unwrap();
    t.director
        .show_menu(n, scene, voiced(&[Line::SelectMusicIntro, line]));
    t.run(0.5);
    t.director.after(n, 0.0, Next::Leave);
    let (_, _, spoken, _) = t.until_over(2.0);
    assert_eq!(spoken, ["select music intro"]);
    t.run(6.0);
    assert!(!t.director.busy());
    assert!(!t.director.is(n));
}

/// A scene that cannot be said is given up; the studio says why.
#[test]
fn a_scene_given_up_leaves_nothing_behind() {
    let t = test();
    let line = Line::SelectMusic(Comment::New);
    let (n, _) = t.director.begin_menu(line, Said::Draft).unwrap();
    t.director.give_up(n, ms::status_no_select());
    assert!(t.host.0.gave_up.get());
    assert!(!t.director.busy());
    assert_eq!(t.host.0.previews.borrow().last().unwrap(), "close");
}

/// A scene in play comes back with what it said.
#[test]
fn what_a_scene_in_play_said_is_told_to_the_studio() {
    let t = test();
    let n = t.director.begin_play(Line::Ready, Said::Draft);
    assert!(t.director.busy());
    t.director.handed_over(n);
    assert!(!t.director.busy());
    t.director.heard(Heard {
        line: Line::Ready,
        said: Said::Draft,
        spoken: vec!["gameplay ready".into(), "gameplay here we go normal".into()],
    });
    assert_eq!(t.director.turns.borrow()[&Line::Ready], 1);
    assert_eq!(t.director.turns.borrow()[&Line::HereWeGo], 1);
    assert!(t.host.0.heard.borrow().is_some());
}

fn message_texts(m: &[Message], name: &str) -> Vec<String> {
    m.iter()
        .filter(|m| m.name == name)
        .filter_map(|m| match m.get("Text") {
            Some(Param::Str(t)) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

/// The words of what a play says, as each line starts and never twice.
#[test]
fn a_plays_lines_words_come_as_each_starts() {
    let mut words = SaidWords::new(
        Line::Ready,
        [(Line::Ready, "Ready?".to_string())].into(),
        [(Line::Ready, 0.8)].into(),
    );
    let spoken = vec![
        ("gameplay ready".to_string(), 10.0),
        ("gameplay cleared".to_string(), 12.0),
    ];
    assert!(words.next(&spoken, 9.0).is_empty());
    let first = words.next(&spoken, 10.5);
    assert_eq!(message_texts(&first, stage::SAY), ["Ready?"]);
    assert!(message_texts(&first, stage::CAPTION).is_empty());
    assert_eq!(words.told(), 1);
    assert_eq!(words.next(&spoken, 11.0).len(), 0);
    assert_eq!(words.told(), 1);
    // Said after the example: no words.
    assert!(message_texts(&words.next(&spoken, 12.5), stage::SAY).is_empty());
    assert_eq!(words.told(), 2);
    assert_eq!(words.next(&spoken, 20.0).len(), 0);
}

/// Only the example shows its words, whatever is said around it; what
/// speaks in its place is the example too.
#[test]
fn only_the_examples_words_are_shown() {
    let words_of = |asked, said: &[&str]| {
        let mut w = SaidWords::new(asked, HashMap::new(), HashMap::new());
        let spoken: Vec<_> = said.iter().map(|s| (s.to_string(), 1.0)).collect();
        message_texts(&w.next(&spoken, 1.0), stage::SAY)
    };
    let ready = "gameplay ready";
    let go = "gameplay here we go normal";
    assert_eq!(words_of(Line::HereWeGo, &[ready, go]).len(), 1);
    assert_eq!(words_of(Line::Ready, &[ready, go]).len(), 1);
    assert_eq!(words_of(Line::Combo(100), &[ready, go]).len(), 0);
    assert_eq!(words_of(Line::Restart, &[ready]).len(), 1);
    assert_eq!(
        words_of(Line::FullComboW(1), &["gameplay full combo"]).len(),
        1
    );
}

/// What comes before the line asked for says why, and the scene's own
/// caption comes back with the line.
#[test]
fn a_line_before_the_one_asked_for_says_why_it_came() {
    let mut words = SaidWords::new(Line::Combo(100), HashMap::new(), HashMap::new());
    words.set_caption("The combo reaches 100.");
    let spoken = vec![
        ("gameplay comment good".to_string(), 1.0),
        ("gameplay 100 combo".to_string(), 3.0),
        ("gameplay comment hot".to_string(), 5.0),
    ];
    let why = message_texts(&words.next(&spoken, 1.0), stage::CAPTION);
    assert_eq!(why.len(), 1);
    assert!(
        why[0].contains(&*studio::name(Line::CommentGood)),
        "{why:?}"
    );
    assert_eq!(
        message_texts(&words.next(&spoken, 3.0), stage::CAPTION),
        ["The combo reaches 100."]
    );
    // Said after it: nothing to explain.
    assert!(message_texts(&words.next(&spoken, 5.0), stage::CAPTION).is_empty());
}

/// Said in the place of a line with no recording: the caption says so.
#[test]
fn what_speaks_in_a_lines_place_is_said_to() {
    let mut words = SaidWords::new(Line::FullComboW(1), HashMap::new(), HashMap::new());
    let spoken = vec![("gameplay full combo".to_string(), 1.0)];
    let said = message_texts(&words.next(&spoken, 1.0), stage::CAPTION);
    assert!(
        said[0].contains(&*studio::name(Line::FullComboW(1))),
        "{said:?}"
    );
    assert!(
        said[0].contains(&*studio::name(Line::FullCombo)),
        "{said:?}"
    );
}

/// A restart's play begins its report again: ready, standing in for the
/// restart's line, is told again.
#[test]
fn a_restarted_plays_lines_are_told_again() {
    let mut words = SaidWords::new(Line::Restart, HashMap::new(), HashMap::new());
    let said = vec![("gameplay ready".to_string(), 1.0)];
    assert_eq!(message_texts(&words.next(&said, 1.0), stage::SAY).len(), 1);
    assert_eq!(words.told(), 1);
    words.restarted();
    assert_eq!(words.told(), 0);
    assert_eq!(message_texts(&words.next(&said, 1.0), stage::SAY).len(), 1);
}

/// Our pack says the first of a line's chain it has, once, the line's
/// next sound each scene; a sound not there is left out of the voice.
#[test]
fn an_example_plan_says_each_line_once_with_its_next_sound() {
    use ddi_library::announcer::{Bundled, Kept};
    use ddi_library::path::RelPathBuf;
    let t = test();
    let set = Set::ALL[0];
    let line = Line::Ready;
    let files: Vec<RelPathBuf> = vec!["ready/a.ogg".into(), "ready/b.ogg".into()];
    let mut kept = Kept::default();
    kept.lines.insert(line.folder(), files);
    let pack = Bundled {
        path: "Pack".into(),
        kept,
        hashes: Default::default(),
        said: [("ready/b.ogg".to_string(), "Ready!".to_string())].into(),
    };
    t.director
        .example_turns
        .borrow_mut()
        .insert((set.key(), line), 1);
    let plan = t
        .director
        .example_plan(&pack, set, &[line, line, Line::Cheer]);
    assert_eq!(
        plan,
        [Planned {
            line,
            path: "ready/b.ogg".into(),
            key: "Pack/ready/b.ogg".into(),
            words: Some("Ready!".into()),
        }]
    );
    let (_, lengths, words) = voiced_from(plan.clone(), |_| Some(2.0), |s: &Sound| *s);
    assert_eq!(lengths.get(&line), Some(&2.0));
    assert_eq!(words.get(&line).map(String::as_str), Some("Ready!"));
    let (_, lengths, _) = voiced_from(plan, |_| None, |s: &Sound| *s);
    assert!(lengths.is_empty());
    // An example pinned is said in place of the next one, and only on
    // its line; the turns stay as they were after its scene.
    let draft = Draft::default();
    let said = t
        .director
        .hearing(line, Some(Pin::Example("ready/a.ogg".into())), &draft, set);
    assert_eq!(said, Said::Example(set));
    assert_eq!(t.director.pin_key().as_deref(), Some("example:ready/a.ogg"));
    let plan = t.director.example_plan(&pack, set, &[line]);
    assert_eq!(plan[0].path, "ready/a.ogg");
    t.director.left(line, said, vec![line.folder()]);
    assert_eq!(t.director.pin_key(), None);
    assert_eq!(t.director.example_turn(set, line), 1);
    // Not pinned, as before.
    assert_eq!(
        t.director.hearing(line, None, &draft, set),
        Said::Example(set)
    );
    assert_eq!(
        t.director.example_plan(&pack, set, &[line])[0].path,
        "ready/b.ogg"
    );
}

/// The draft as a voice: the line's next recording first, the length its.
#[test]
fn the_draft_as_a_voice_takes_each_lines_next_recording_first() {
    let mut draft = Draft::default();
    let mut decoded = HashMap::new();
    for (name, len) in [("a.wav", 1.0), ("b.wav", 2.0)] {
        let id = draft.add(name, 10, b"RIFF");
        draft.take_mut(id).unwrap().line = Some(Line::Ready);
        decoded.insert(id, len);
    }
    let cleared = draft.add("c.wav", 10, b"RIFF");
    draft.take_mut(cleared).unwrap().line = Some(Line::Cleared);
    decoded.insert(cleared, 3.0);
    let turns = [(Line::Ready, 1)].into();
    let (mut voice, lengths, _) = draft_voice(
        &draft,
        |l| l == Line::Ready,
        &turns,
        None,
        &decoded,
        |_, s: &f64| Some(*s),
        |s| *s,
    );
    assert_eq!(lengths[&Line::Ready], 2.0, "the next one's");
    assert_eq!(lengths.get(&Line::Cleared), None);
    assert_eq!(voice.next(Line::Ready).unwrap().1, &2.0);
    assert_eq!(voice.next(Line::Ready).unwrap().1, &1.0);
    // A recording pinned: its line says it alone, the others as before.
    let first = draft.takes_of(Line::Ready).next().unwrap().id;
    let (mut voice, lengths, _) = draft_voice(
        &draft,
        |_| true,
        &turns,
        Some(first),
        &decoded,
        |_, s: &f64| Some(*s),
        |s| *s,
    );
    assert_eq!(lengths[&Line::Ready], 1.0);
    assert_eq!(voice.next(Line::Ready).unwrap().1, &1.0);
    assert_eq!(voice.next(Line::Ready).unwrap().1, &1.0);
    assert_eq!(lengths[&Line::Cleared], 3.0);
}
