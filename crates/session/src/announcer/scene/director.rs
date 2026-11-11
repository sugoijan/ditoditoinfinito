//! A scene as the announcer studio runs it (`docs/plans/announcer-studio.md`,
//! steps 3 and 4), the same on both shells: what comes after what, when,
//! and what the frame around it says. A shell gives it a clock, its UI,
//! song select's sound and the voice ([`SceneHost`]); the [`Director`]
//! brings on the screens the game speaks a line on, presses what a player
//! would press, and says what was heard once the scene is over.
//!
//! A scene is begun in the press ([`Director::begin_menu`],
//! [`Director::begin_results`], [`Director::begin_play`]), for what must
//! start there, and shown once the shell has its voice
//! ([`Director::show_menu`], [`Director::show_results`]); a scene in play
//! goes on to the shell's play ([`StagedPlay`]) and comes back with what
//! it said ([`Heard`], [`Director::heard`]).

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use ddi_l10n::UiText;
use ddi_l10n::msg::studio as ms;
use ddi_library::announcer::draft::{Draft, Take};
use ddi_library::announcer::examples::Set;
use ddi_library::announcer::{Bundled, Line};
use ddi_platform::AudioBackend;
use ddi_platform::storage;
use ddi_ui::message::{Message, Param};
use ddi_ui::nav::MenuAction;
use ddi_ui::screens::Env;
use ddi_ui::screens::results::{ActionFn, ResultsActions, ResultsScreen};
use ddi_ui::screens::select::{OpenFn, PlayFn, SelectCall, SelectScreen, ShellFn};
use ddi_ui::screens::stage::{self, BackFn, StageFrame};
use ddi_ui::screens::start;
use ddi_ui::screens::studio;
use ddi_ui::state::settings::Settings;
use ddi_ui::{Cx, Mark, Screen, Ui};

use super::super::{Announcer, Timing, Voice, cheers, results_lines};
use super::{MenuScene, ResultsScene, example_volume, is_example};

/// A scene lasts this long at least, seconds: time to see its screen.
const SCENE_SECONDS: f64 = 3.5;
/// It ends this long after its last line, seconds.
const SCENE_TAIL: f64 = 1.5;
/// The start menu shows this long before Singleplayer is chosen (the
/// welcome's scene), seconds.
const START_SECONDS: f64 = 1.8;
/// What a comment is said for is pointed at this long before the chart
/// is chosen, seconds.
const MARK_LEAD: f64 = 1.2;
/// A screen's way in, about, seconds: the results' line waits for it.
const ENTRANCE: f64 = 0.4;

/// A voice, each line's length (seconds) and words (where known).
pub type Voiced<S> = (Voice<S>, HashMap<Line, f64>, HashMap<Line, String>);

/// A line as our pack says it in a scene.
#[derive(Clone, Debug, PartialEq)]
pub struct Planned {
    pub line: Line,
    /// The sound that says it this time, by its path in the pack.
    pub path: String,
    /// What the sound is kept under, decoded.
    pub key: String,
    pub words: Option<String>,
}

/// The voice `plan` says, each sound as `sound` has it (one it has not
/// is left out), each line's length by `duration`.
pub fn voiced_from<S: Clone>(
    plan: Vec<Planned>,
    sound: impl Fn(&str) -> Option<S>,
    duration: impl Fn(&S) -> f64,
) -> Voiced<S> {
    let mut voice = Voice::default();
    let mut lengths = HashMap::new();
    let mut words = HashMap::new();
    for p in plan {
        let Some(s) = sound(&p.key) else {
            continue;
        };
        voice.expect([p.line]);
        lengths.insert(p.line, duration(&s));
        voice.add(p.line, s);
        if let Some(w) = p.words {
            words.insert(p.line, w);
        }
    }
    (voice, lengths, words)
}

/// A timer of the shell's: dropped, it does not run.
pub type Timer = Box<dyn Any>;

/// Something to do on the shell's UI.
pub type UiFn = Box<dyn FnOnce(&mut Ui)>;

/// What a staged line did: the line asked for, and what spoke for it and
/// for how long.
pub type Told = Rc<dyn Fn(Line, Option<(Line, f64)>)>;

/// The studio's voice while its scene shows song select: its lines are
/// spoken from it in place of the announcer chosen, at `share` of the
/// music's volume, and each told to the director.
pub struct StagedVoice<S> {
    pub voice: Voice<S>,
    pub share: f32,
    pub told: Told,
    /// The line the scene is for: what is not it is said quieter.
    pub example: Line,
}

impl<S> StagedVoice<S> {
    /// The volume `found` is spoken at.
    pub fn volume(&self, found: Line) -> f32 {
        example_volume(self.example, found, self.share)
    }
}

/// Who says a scene's lines.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Said {
    /// The draft: the line is recorded (or what speaks in its place).
    Draft,
    /// Our pack of this set of examples (Mr. Clank, ガキンさん): it is
    /// not yet.
    Example(Set),
}

impl Said {
    /// Who says `line` in its scene: the draft where it has something
    /// to say for the line, else our pack of the examples chosen.
    pub fn of(line: Line, draft: &Draft, examples: Set) -> Said {
        if studio::recorded(line, draft) {
            Said::Draft
        } else {
            Said::Example(examples)
        }
    }

    /// As the debug state says it: `draft`, `example:en`.
    pub fn key(self) -> String {
        match self {
            Said::Draft => "draft".into(),
            Said::Example(set) => format!("example:{}", set.key()),
        }
    }

    /// As the frame says it.
    pub fn label(self) -> UiText {
        match self {
            Said::Draft => ms::stage_voice_recording(),
            Said::Example(set) => ms::stage_voice_example(set.pack()),
        }
    }
}

/// The one sound a scene says for its line, chosen on the studio's page
/// (Hear in the game beside it): a recording, or our pack's sound of an
/// example, by its path in the pack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pin {
    Take(u64),
    Example(String),
}

impl Pin {
    /// As the debug state says it: `take:3`, `example:<path>`.
    pub fn key(&self) -> String {
        match self {
            Pin::Take(id) => format!("take:{id}"),
            Pin::Example(path) => format!("example:{path}"),
        }
    }
}

/// The line a press of Hear in the game shows, and the sound pinned
/// for it: the recording pressed beside (on its line), our example
/// pressed beside (`examples`, the page's; one our announcer says), or
/// none for the line's own button.
pub fn hear_of(
    act: studio::Act,
    draft: &Draft,
    examples: &BTreeMap<Line, Vec<studio::Example>>,
) -> Option<(Line, Option<Pin>)> {
    match act {
        studio::Act::Hear(line) => Some((line, None)),
        studio::Act::HearTake(id) => Some((draft.take(id)?.line?, Some(Pin::Take(id)))),
        studio::Act::HearExample(line, i) => {
            let path = examples.get(&line)?.get(i)?.sound.clone()?;
            Some((line, Some(Pin::Example(path))))
        }
        _ => None,
    }
}

/// The screen a scene shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shows {
    /// A line said in play, on the shell's play.
    Play,
    Start,
    Select,
    Results,
}

impl Shows {
    /// As the debug state says it: `start`.
    pub fn key(self) -> &'static str {
        match self {
            Shows::Play => "play",
            Shows::Start => "start",
            Shows::Select => "select",
            Shows::Results => "results",
        }
    }
}

/// What a staged play is said with, from the studio.
pub struct StagedPlay<S> {
    pub line: Line,
    pub said: Said,
    /// Who says it, as the frame shows it ("Your recording").
    pub voice_label: String,
    /// The line as the frame names it.
    pub name: String,
    /// The play's and its results' lines (the draft's, or our pack's).
    pub voice: Voice<S>,
    /// Each line's length (its next sound's), seconds.
    pub lengths: HashMap<Line, f64>,
    /// Each line's words, where known (an example's, a recording's made
    /// for a text line).
    pub words: HashMap<Line, String>,
}

/// What a staged play said, for the studio.
#[derive(Clone, Debug, PartialEq)]
pub struct Heard {
    pub line: Line,
    pub said: Said,
    /// The lines said, by folder, in order.
    pub spoken: Vec<String>,
}

/// The frame's caption for a line other than the one asked for, said
/// before it: why the game says it there (`docs/plans/announcer-studio.md`,
/// step 4).
fn why_first(found: Line) -> UiText {
    let line = studio::name(found);
    match found {
        Line::Ready | Line::HereWeGo | Line::Restart => ms::why_opening(line),
        Line::CommentHot | Line::CommentGood | Line::CommentDanger | Line::CommentOni => {
            ms::why_comment(line)
        }
        Line::Combo(_) | Line::ComboOverflow => ms::why_combo(line),
        _ => ms::why_on_the_way(line),
    }
}

/// The frame's caption while `found` is said in the place of `asked`,
/// which has no recording.
fn in_its_place(asked: Line, found: Line) -> UiText {
    ms::why_in_its_place(studio::name(asked), studio::name(found))
}

/// The words of the lines a staged play says, as each starts, and why
/// one came before the line asked for.
pub struct SaidWords {
    /// The line the scene is for.
    asked: Line,
    /// The lines said so far, told to the frame.
    told: usize,
    words: HashMap<Line, String>,
    lengths: HashMap<Line, f64>,
    /// What the frame says happens, when no line needs explaining.
    caption: String,
    /// The frame says why a line came first, for now.
    explaining: bool,
    /// The line asked for was said (or what speaks in its place).
    arrived: bool,
}

impl SaidWords {
    pub fn new(
        asked: Line,
        words: HashMap<Line, String>,
        lengths: HashMap<Line, f64>,
    ) -> SaidWords {
        SaidWords {
            asked,
            told: 0,
            words,
            lengths,
            caption: String::new(),
            explaining: false,
            arrived: false,
        }
    }

    /// How many lines the frame has been told.
    pub fn told(&self) -> usize {
        self.told
    }

    /// What the frame says happens in this scene.
    pub fn set_caption(&mut self, caption: &str) {
        self.caption = caption.into();
    }

    /// The play began again (its report of what was said begins again).
    pub fn restarted(&mut self) {
        self.told = 0;
    }

    /// The frame's caption, as it should be with `found` said now.
    fn caption_for(&mut self, found: Line) -> Option<String> {
        if self.arrived {
            return None;
        }
        let asked = self.asked;
        if found == asked {
            self.arrived = true;
            return std::mem::take(&mut self.explaining).then(|| self.caption.clone());
        }
        if is_example(asked, found) {
            self.arrived = true;
            self.explaining = true;
            return Some(in_its_place(asked, found).to_string());
        }
        self.explaining = true;
        Some(why_first(found).to_string())
    }

    /// For each of the lines said so far (`spoken`: folder, context time)
    /// begun by `now` and not told yet: the frame's messages showing its
    /// words, if it is the example, and, where it is not the line asked
    /// for and came before it, why.
    pub fn next(&mut self, spoken: &[(String, f64)], now: f64) -> Vec<Message> {
        let mut out = Vec::new();
        while let Some((folder, at)) = spoken.get(self.told) {
            if *at > now {
                break;
            }
            self.told += 1;
            let Some(line) = Line::parse(folder) else {
                continue;
            };
            if let Some(caption) = self.caption_for(line) {
                out.push(Message::new(stage::CAPTION).with("Text", Param::Str(caption)));
            }
            if !is_example(self.asked, line) {
                continue;
            }
            let text = self
                .words
                .get(&line)
                .cloned()
                .unwrap_or_else(|| ms::stage_said(studio::name(line)).to_string());
            let seconds = self.lengths.get(&line).copied().unwrap_or(1.5);
            out.push(say(text, seconds));
        }
        out
    }
}

/// The frame's message for words said now.
fn say(text: String, seconds: f64) -> Message {
    Message::new(stage::SAY)
        .with("Text", Param::Str(text))
        .with("Seconds", Param::Num(seconds))
}

/// The draft as a voice for the lines `which` keeps: each line's
/// recordings decoded so far (`decoded`, by take), as the pack has them,
/// the line's next one (`turns`) first, each made a sound by `make`; with
/// the first one's length and words (where it has them) of each line.
/// The line of take `pinned` says it alone.
pub fn draft_voice<S>(
    draft: &Draft,
    which: impl Fn(Line) -> bool,
    turns: &HashMap<Line, usize>,
    pinned: Option<u64>,
    decoded: &HashMap<u64, S>,
    make: impl Fn(&Take, &S) -> Option<S>,
    length: impl Fn(&S) -> f64,
) -> Voiced<S> {
    let mut voice = Voice::default();
    let mut lengths = HashMap::new();
    let mut words = HashMap::new();
    for line in draft.lines().into_iter().filter(|l| which(*l)) {
        let mut takes: Vec<&Take> = draft
            .takes_of(line)
            .filter(|t| decoded.contains_key(&t.id))
            .collect();
        if let Some(id) = pinned
            && takes.iter().any(|t| t.id == id)
        {
            takes.retain(|t| t.id == id);
        }
        if takes.is_empty() {
            continue;
        }
        voice.expect([line]);
        let first = turns.get(&line).copied().unwrap_or(0) % takes.len();
        // The words of the one said first, where it was recorded for a
        // text line; else its line by name.
        if let Some(w) = takes[first].words.as_deref().map(str::trim)
            && !w.is_empty()
        {
            words.insert(line, w.to_string());
        }
        for k in 0..takes.len() {
            let t = takes[(first + k) % takes.len()];
            if let Some(b) = make(t, &decoded[&t.id]) {
                lengths.entry(line).or_insert_with(|| length(&b));
                voice.add(line, b);
            }
        }
    }
    (voice, lengths, words)
}

/// The lines the results of `scene` may say, the cheer too.
pub fn results_said_lines(scene: &ResultsScene) -> Vec<Line> {
    let ending = scene.ending();
    let mut lines = results_lines(Some(scene.prior), &ending, &scene.grades, scene.combo_w4);
    if cheers(&ending, &scene.grades) {
        lines.push(Line::Cheer);
    }
    lines
}

/// The results' line of `scene`, said now by `voice` on `audio` at
/// `share` of the volume, as [`Announcer::results`] chooses it: the lines
/// said and when each starts, and the context time now.
pub fn say_results<A: AudioBackend>(
    audio: &mut A,
    now: f64,
    scene: &ResultsScene,
    voice: Voice<A::Buffer>,
    share: f32,
) -> Vec<(String, f64)> {
    let mut announcer = Announcer::new(
        true,
        share,
        Some(scene.prior),
        Timing {
            start: 0.0,
            offset: 0.0,
            rate: 1.0,
        },
        None,
    )
    .for_example(Some(scene.line));
    announcer.set_voice(Rc::new(RefCell::new(voice)));
    announcer.results(audio, now, &scene.ending(), &scene.grades, scene.combo_w4);
    announcer.report.spoken
}

/// What a shell does for a scene.
pub trait SceneHost: 'static {
    /// A decoded sound.
    type Sound: Clone + 'static;

    /// Milliseconds on the wall clock.
    fn now_ms(&self) -> f64;
    /// `f` in `seconds`, unless the timer is dropped first.
    fn timer(&self, seconds: f64, f: Box<dyn FnOnce()>) -> Timer;
    /// The screens' environment, once there is one.
    fn env(&self) -> Option<Env>;
    /// `f` on the UI: now, or just after the event it is handling.
    fn on_ui(&self, f: UiFn);
    /// `f` on the UI, for a scene's screens to come in; whether it could.
    fn show(&self, f: UiFn) -> bool;
    /// Presses `action` as a player would: down, then up.
    fn press(&self, action: MenuAction);

    /// Song select's sound, opened in the press at `volume`.
    fn preview_open(&self, volume: f32);
    /// The song song select shows.
    fn preview_select(&self, song: Option<String>);
    fn preview_stop(&self);
    fn preview_volume(&self, volume: f32);
    /// The preview and what is said stop, as a play opening stops them.
    fn preview_hush(&self);
    fn preview_close(&self);
    /// Who says song select's lines (`None`: the announcer chosen again).
    fn stage_voice(&self, staged: Option<StagedVoice<Self::Sound>>);

    /// The sound the results' line is said on, made in the press; whether
    /// it could be.
    fn open_results_sound(&self, line: Line) -> bool;
    /// `voice` says `scene`'s line now on that sound: the lines said and
    /// when each starts, and the context time now.
    fn say_results(
        &self,
        scene: &ResultsScene,
        voice: Voice<Self::Sound>,
    ) -> Option<(Vec<(String, f64)>, f64)>;

    /// What the scene plays stops.
    fn stop_sound(&self);
    /// The scene changed (the shell's debug state).
    fn changed(&self);
    /// A scene's step is done: the shell's debug state made again.
    fn publish(&self);
    /// The scene could not show: the studio says why.
    fn gave_up(&self, why: UiText);
    /// The scene is over, the studio showing again: what it says, the
    /// line asked for, the lines heard (by folder), and who said them.
    fn heard(&self, status: UiText, line: Line, spoken: Vec<String>, said: Said);
}

/// A scene showing: a line heard on the screen the game speaks it on, in
/// a frame over the studio ([`StageFrame`]), what it is said for pointed
/// at.
struct Scene {
    line: Line,
    said: Said,
    /// When it began, milliseconds.
    began: f64,
    /// The screen shown now.
    screen: Shows,
    /// What happens now (the frame's caption).
    caption: String,
    /// Song select's chart is to be chosen (a comment's scene, a results
    /// line's song select), and was (by the scene, or by the player
    /// first).
    choose: bool,
    chosen: bool,
    /// What it points at now (a spot of the screen's, `Ui::mark`), and
    /// whether it does yet.
    mark: Option<Mark>,
    marked: bool,
    /// Its way out was asked for on the start menu (which Back does not
    /// leave): it leaves as Singleplayer is chosen.
    leaving: bool,
    /// It plays song select's preview (on song select's own sound).
    preview: bool,
    /// The lines said, by folder, and when each starts (milliseconds):
    /// one cut by the scene's end was not heard.
    spoken: Vec<(String, f64)>,
    /// Each line's words (an example's, a recording's made for a text
    /// line) and length, seconds.
    words: HashMap<Line, String>,
    lengths: HashMap<Line, f64>,
    /// Its next step, and the steps under way (the mark, the words shown).
    timer: Option<Timer>,
    later: Vec<Timer>,
    /// Tells this scene's calls and timers from a later one's.
    n: u64,
}

/// A scene's next step.
#[derive(Clone, Copy)]
enum Next {
    /// Start: Singleplayer on the start menu, the chart on song select.
    Start,
    /// Back to the studio (Back), once the scene has shown long enough.
    End,
    /// Back to the studio at once (the frame's way back).
    Leave,
}

/// A scene showing, for a shell's debug state.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneState {
    pub line: Line,
    pub said: Said,
    pub screen: Shows,
    pub caption: String,
    pub chosen: bool,
    /// What is pointed at now, if anything yet.
    pub mark: Option<Mark>,
    /// The lines said so far, by folder.
    pub spoken: Vec<String>,
}

/// Runs the studio's scenes on a shell.
pub struct Director<H: SceneHost> {
    host: H,
    scene: RefCell<Option<Scene>>,
    scenes: Cell<u64>,
    /// Each line's recording a scene says first: the next each time one
    /// was said.
    pub turns: RefCell<HashMap<Line, usize>>,
    /// Each line's next example, by set.
    pub example_turns: RefCell<HashMap<(&'static str, Line), usize>>,
    /// The sound the scene says for its line, where one was chosen
    /// ([`Director::hearing`]).
    pin: RefCell<Option<Pin>>,
}

impl<H: SceneHost> Director<H> {
    pub fn new(host: H) -> Rc<Director<H>> {
        Rc::new(Director {
            host,
            scene: RefCell::new(None),
            scenes: Cell::new(0),
            turns: RefCell::default(),
            example_turns: RefCell::default(),
            pin: RefCell::default(),
        })
    }

    /// Who says the scene of `line` about to begin: the draft for a
    /// recording pinned (`pin`), our pack of `set` for an example pinned,
    /// else as [`Said::of`]; the pin kept for its voice.
    pub fn hearing(&self, line: Line, pin: Option<Pin>, draft: &Draft, set: Set) -> Said {
        let said = match &pin {
            Some(Pin::Take(_)) => Said::Draft,
            Some(Pin::Example(_)) => Said::Example(set),
            None => Said::of(line, draft, set),
        };
        *self.pin.borrow_mut() = pin;
        said
    }

    /// The recording pinned for the scene, if one is.
    pub fn pinned_take(&self) -> Option<u64> {
        match *self.pin.borrow() {
            Some(Pin::Take(id)) => Some(id),
            _ => None,
        }
    }

    /// What is pinned for the scene, as the debug state says it.
    pub fn pin_key(&self) -> Option<String> {
        self.pin.borrow().as_ref().map(Pin::key)
    }

    /// A scene is under way.
    pub fn busy(&self) -> bool {
        self.scene.borrow().is_some()
    }

    /// The scene showing, for a shell's debug state.
    pub fn state(&self) -> Option<SceneState> {
        self.scene.borrow().as_ref().map(|sc| SceneState {
            line: sc.line,
            said: sc.said,
            screen: sc.screen,
            caption: sc.caption.clone(),
            chosen: sc.chosen,
            mark: sc.mark.filter(|_| sc.marked),
            spoken: sc.spoken.iter().map(|(f, _)| f.clone()).collect(),
        })
    }

    /// Who says `line`'s next scene: the turn of `said`'s sounds of it.
    pub fn example_turn(&self, set: Set, line: Line) -> usize {
        self.example_turns
            .borrow()
            .get(&(set.key(), line))
            .copied()
            .unwrap_or(0)
    }

    /// What `pack` (our pack of `set`) says for `lines`: one sound of
    /// each (as the first of its chain the pack has, once), the line's
    /// next each scene, or the example pinned where it is the line's.
    pub fn example_plan(&self, pack: &Bundled, set: Set, lines: &[Line]) -> Vec<Planned> {
        let pinned = match &*self.pin.borrow() {
            Some(Pin::Example(path)) => Some(path.clone()),
            _ => None,
        };
        let mut plan = Vec::new();
        let mut seen = BTreeSet::new();
        for &line in lines {
            let Some((found, files)) = line.chain().into_iter().find_map(|l| {
                let files = pack.kept.lines.get(&l.folder())?;
                (!files.is_empty()).then_some((l, files))
            }) else {
                continue;
            };
            if !seen.insert(found) {
                continue;
            }
            let path = match pinned
                .as_ref()
                .filter(|p| files.iter().any(|f| f.as_str() == p.as_str()))
            {
                Some(p) => p.clone(),
                None => files[self.example_turn(set, found) % files.len()].to_string(),
            };
            plan.push(Planned {
                line: found,
                key: format!("{}/{path}", pack.path),
                words: pack.said.get(&path).cloned(),
                path,
            });
        }
        plan
    }

    #[allow(clippy::too_many_arguments)]
    fn begin(
        &self,
        line: Line,
        said: Said,
        screen: Shows,
        choose: bool,
        caption: &str,
        mark: Option<Mark>,
        preview: bool,
    ) -> u64 {
        let n = self.scenes.get() + 1;
        self.scenes.set(n);
        self.host.changed();
        *self.scene.borrow_mut() = Some(Scene {
            line,
            said,
            began: self.host.now_ms(),
            screen,
            caption: caption.into(),
            choose,
            chosen: false,
            mark,
            marked: false,
            leaving: false,
            preview,
            spoken: Vec::new(),
            words: HashMap::new(),
            lengths: HashMap::new(),
            timer: None,
            later: Vec::new(),
            n,
        });
        n
    }

    /// Whether scene `n` is still the one under way.
    pub fn is(&self, n: u64) -> bool {
        self.scene.borrow().as_ref().is_some_and(|s| s.n == n)
    }

    /// `f` on scene `n` (while it is the one under way).
    fn with_scene<T>(&self, n: u64, f: impl FnOnce(&mut Scene) -> T) -> Option<T> {
        self.host.changed();
        self.scene.borrow_mut().as_mut().filter(|s| s.n == n).map(f)
    }

    /// Scene `n` given up before it showed (its example could not be
    /// read): the studio says why.
    pub fn give_up(&self, n: u64, why: UiText) {
        let Some(sc) = self.scene.borrow_mut().take_if(|s| s.n == n) else {
            return;
        };
        self.host.stage_voice(None);
        if sc.preview {
            self.host.preview_close();
        }
        self.host.stop_sound();
        self.pin.borrow_mut().take();
        self.host.gave_up(why);
    }

    /// The frame scene `n` is shown in ([`StageFrame`]): its line, who
    /// says it, what happens first; its way back leaves the scene.
    fn frame(self: &Rc<Self>, env: Env, n: u64, caption: &str) -> StageFrame {
        let (line, said) = self
            .with_scene(n, |sc| (sc.line, sc.said))
            .unwrap_or((Line::Ready, Said::Draft));
        let weak = Rc::downgrade(self);
        let back: BackFn = Rc::new(move |_| {
            if let Some(s) = weak.upgrade() {
                s.after(n, 0.0, Next::Leave);
            }
        });
        StageFrame::new(env, &studio::name(line), &said.label(), caption, back)
    }

    /// The scene's frame over the studio and its first screen in it, the
    /// studio leaving first (out of the press: what it says is ready),
    /// `mark` pointed at; whether they were.
    fn show(&self, frame: StageFrame, screen: impl Screen + 'static, mark: Option<Mark>) -> bool {
        self.host.show(Box::new(move |ui| {
            ui.push_chrome(frame);
            ui.push_over(screen);
            ui.mark(mark);
        }))
    }

    /// Scene `n` points at what it is for (its spot), now.
    fn mark(&self, n: u64) {
        let Some(Some(mark)) = self.with_scene(n, |sc| {
            sc.marked = true;
            sc.mark
        }) else {
            return;
        };
        self.host.on_ui(Box::new(move |ui| ui.mark(Some(mark))));
        self.host.publish();
    }

    /// The frame's caption: what happens now.
    fn caption_now(&self, n: u64, caption: &str) {
        self.with_scene(n, |sc| sc.caption = caption.into());
        let m = Message::new(stage::CAPTION).with("Text", Param::Str(caption.into()));
        self.host.on_ui(Box::new(move |ui| ui.broadcast(m)));
    }

    /// The frame's bar: the scene ends in about `seconds`.
    fn until(&self, n: u64, seconds: f64) {
        if !self.is(n) {
            return;
        }
        let m = Message::new(stage::UNTIL).with("Seconds", Param::Num(seconds));
        self.host.on_ui(Box::new(move |ui| ui.broadcast(m)));
    }

    /// The frame shows the words of `line` (said now) for `seconds`, if
    /// it is the scene's example: as its pack says them, a recording's
    /// line by name.
    fn say(&self, n: u64, line: Line, seconds: f64) {
        let Some((words, asked)) = self.with_scene(n, |sc| (sc.words.get(&line).cloned(), sc.line))
        else {
            return;
        };
        if !is_example(asked, line) {
            return;
        }
        if line != asked {
            self.caption_now(n, &in_its_place(asked, line));
        }
        let text = match words {
            Some(w) => UiText::data(w),
            None => ms::stage_said(studio::name(line)),
        };
        let m = say(text.into(), seconds);
        self.host.on_ui(Box::new(move |ui| ui.broadcast(m)));
    }

    /// `f` in `seconds`, while scene `n` lasts.
    fn later(self: &Rc<Self>, n: u64, seconds: f64, f: impl FnOnce(&Rc<Self>) + 'static) {
        let weak = Rc::downgrade(self);
        let t = self.host.timer(
            seconds.max(0.0),
            Box::new(move || {
                if let Some(s) = weak.upgrade()
                    && s.is(n)
                {
                    f(&s);
                }
            }),
        );
        self.with_scene(n, |sc| sc.later.push(t));
    }

    /// Scene `n`'s words and lengths, kept for its steps.
    fn keep(&self, n: u64, lengths: &HashMap<Line, f64>, words: HashMap<Line, String>) {
        self.with_scene(n, |sc| {
            sc.lengths = lengths.clone();
            sc.words = words;
        });
    }

    /// How long `line` is said for in scene `n` (as the first of its
    /// chain the voice has), about.
    fn length(&self, n: u64, line: Line) -> f64 {
        self.with_scene(n, |sc| {
            line.chain()
                .into_iter()
                .find_map(|l| sc.lengths.get(&l).copied())
        })
        .flatten()
        .unwrap_or(1.5)
    }

    /// Song select's scene of `line`, begun in the press (`None`: not one
    /// of its lines): its scene, and its number. Song select's sound
    /// opens here.
    pub fn begin_menu(&self, line: Line, said: Said) -> Option<(u64, MenuScene)> {
        let scene = super::menu(line, storage::started_ms(), self.host.now_ms())?;
        self.host.env()?;
        self.host.preview_open(Settings::load().volume);
        let (first, caption) = match &scene.start {
            Some(c) => (Shows::Start, c.text()),
            None => (Shows::Select, scene.caption.text()),
        };
        let n = self.begin(
            line,
            said,
            first,
            scene.choose,
            &caption,
            Some(scene.mark),
            true,
        );
        Some((n, scene))
    }

    /// Song select's scene `n`, its voice ready: the bundled songs, the
    /// preview playing, the intro said as it shows (the welcome's from
    /// the start menu, Singleplayer chosen on it); a comment's chart
    /// pointed at and chosen once the intro is said.
    pub fn show_menu(self: &Rc<Self>, n: u64, scene: MenuScene, voiced: Voiced<H::Sound>) {
        if !self.is(n) {
            return;
        }
        let Some(env) = self.host.env() else {
            return self.give_up(n, ms::status_no_select());
        };
        let (voice, lengths, words) = voiced;
        self.keep(n, &lengths, words);
        let example = self.with_scene(n, |sc| sc.line).unwrap_or(Line::Ready);
        let told: Told = {
            let weak = Rc::downgrade(self);
            Rc::new(move |asked, spoke| {
                if let Some(s) = weak.upgrade() {
                    s.told(n, asked, spoke);
                }
            })
        };
        self.host.stage_voice(Some(StagedVoice {
            voice,
            share: Settings::load().announcer_volume,
            told,
            example,
        }));
        let play: PlayFn = {
            let weak = Rc::downgrade(self);
            Rc::new(move |_, _, _, _| {
                if let Some(s) = weak.upgrade() {
                    s.chosen(n);
                }
            })
        };
        let options: OpenFn = Rc::new(|_, _| {});
        let select = SelectScreen::new(env.clone(), scene.data, Some(&scene.song), play, options)
            .with_shell(self.select_shell(n), Rc::default());
        let caption = match &scene.start {
            Some(c) => c.text(),
            None => scene.caption.text(),
        };
        let frame = self.frame(env.clone(), n, &caption);
        let intro = self.length(n, Line::SelectMusicIntro);
        let line = self.with_scene(n, |sc| sc.line).unwrap_or(Line::Ready);
        let shown = match &scene.start {
            Some(_) => {
                // Singleplayer brings song select in, as on the start menu.
                let next = Rc::new(RefCell::new(Some(select)));
                let weak = Rc::downgrade(self);
                let caption = scene.caption.text().to_string();
                let action: start::Action = Rc::new(move |choice, cx| {
                    if choice == start::Choice::Singleplayer
                        && let Some(s) = weak.upgrade()
                    {
                        s.singleplayer(n, &next, &caption, cx);
                    }
                });
                let start = start::StartScreen::new(env, false, action)
                    .with_focus(Some(start::Choice::Singleplayer.id().into()));
                self.show(frame, start, Some(scene.mark))
            }
            None => self.show(frame, select, None),
        };
        if !shown {
            return self.give_up(n, ms::status_no_select());
        }
        self.with_scene(n, |sc| sc.marked = scene.start.is_some());
        if scene.start.is_some() {
            self.after(n, START_SECONDS, Next::Start);
            self.until(n, START_SECONDS + ENTRANCE + intro + SCENE_TAIL);
        } else if scene.choose {
            // Should nothing be said as it shows: on regardless.
            let comment = self.length(n, line);
            self.later(n, 3.0 - MARK_LEAD, move |s| s.mark(n));
            self.after(n, 3.0, Next::Start);
            self.until(
                n,
                ENTRANCE + (intro + MARK_LEAD).max(3.0) + comment + SCENE_TAIL,
            );
        } else {
            self.after(n, 3.0, Next::End);
        }
        self.host.publish();
    }

    /// What song select's calls do in scene `n`.
    fn select_shell(self: &Rc<Self>, n: u64) -> ShellFn {
        let weak = Rc::downgrade(self);
        Rc::new(move |call, cx| {
            if let Some(s) = weak.upgrade() {
                s.select_call(n, call, cx);
            }
        })
    }

    /// The welcome's start menu chose Singleplayer (scene `n`, in `cx`):
    /// song select `next` comes in, as on the start menu; the scene
    /// leaves instead where its way out was asked for meanwhile.
    fn singleplayer(
        self: &Rc<Self>,
        n: u64,
        next: &RefCell<Option<SelectScreen>>,
        caption: &str,
        cx: &mut Cx,
    ) {
        let leaving = self.with_scene(n, |sc| sc.leaving).unwrap_or(true);
        if leaving {
            return self.leave(n, cx);
        }
        let Some(select) = next.borrow_mut().take() else {
            return;
        };
        cx.replace(select);
        self.with_scene(n, |sc| {
            sc.screen = Shows::Select;
            sc.timer = None;
        });
        self.caption_now(n, caption);
        // Should nothing be said as it shows: on regardless.
        self.after(n, 3.0 + ENTRANCE, Next::End);
        self.host.publish();
    }

    /// What song select's scene `n` asks.
    fn select_call(self: &Rc<Self>, n: u64, call: SelectCall, cx: &mut Cx) {
        match call {
            SelectCall::Selected { song, .. } => self.host.preview_select(song),
            SelectCall::Shown(false) => self.host.preview_stop(),
            SelectCall::Volume(v) => self.host.preview_volume(v),
            SelectCall::Leave => self.leave(n, cx),
            _ => {}
        }
    }

    /// Scene `n`'s chart chosen (song select's Start): as a play opening
    /// takes song select's sound, the preview and the intro stop; the
    /// comment comes next (`UiRequest::Announce`, [`Director::told`]).
    fn chosen(&self, n: u64) {
        self.with_scene(n, |sc| {
            sc.chosen = true;
            sc.timer = None;
        });
        self.host.preview_hush();
        self.host.publish();
    }

    /// A line of song select's scene `n` asked for (`asked`), and what
    /// was said for it and how long: the scene's next step after it (a
    /// comment's chart pointed at once the intro is said, then chosen).
    fn told(self: &Rc<Self>, n: u64, asked: Line, spoke: Option<(Line, f64)>) {
        let now = self.host.now_ms();
        let Some((chosen, choose)) = self.with_scene(n, |sc| {
            if let Some((found, _)) = spoke {
                sc.spoken.push((found.folder(), now));
            }
            (sc.chosen, sc.choose)
        }) else {
            return;
        };
        if let Some((found, len)) = spoke {
            self.say(n, found, len);
        }
        let len = spoke.map_or(0.0, |(_, l)| l);
        match asked {
            Line::SelectMusicIntro if chosen => {}
            Line::SelectMusicIntro if choose => {
                // The steps in case nothing was said give way.
                let line = self
                    .with_scene(n, |sc| {
                        sc.later.clear();
                        sc.line
                    })
                    .unwrap_or(asked);
                let at = (len + MARK_LEAD).max(1.8);
                self.later(n, at - MARK_LEAD, move |s| s.mark(n));
                self.after(n, at, Next::Start);
                self.until(n, at + self.length(n, line) + SCENE_TAIL);
            }
            _ => self.after(n, len + SCENE_TAIL, Next::End),
        }
        self.host.publish();
    }

    /// The results' scene of `line`, begun in the press (`None`: not one
    /// of its lines, or no sound): a play staged for the line, judged by
    /// the player's rules where they give it (`recorded`: the lines the
    /// draft has, where it says them).
    pub fn begin_results(
        &self,
        line: Line,
        said: Said,
        draft: &Draft,
    ) -> Option<(u64, ResultsScene)> {
        let settings = Settings::load();
        // Our packs say every line.
        let has = |l: Line| said != Said::Draft || draft.takes_of(l).next().is_some();
        let scene = super::results(line, &settings.ruleset, &has)?;
        self.host.env()?;
        if !self.host.open_results_sound(line) {
            return None;
        }
        let n = self.begin(
            line,
            said,
            Shows::Results,
            false,
            &scene.caption.text(),
            Some(scene.mark),
            false,
        );
        Some((n, scene))
    }

    /// The results' scene `n`, its voice ready: its line said on its own
    /// sound as the results open, as after a play.
    pub fn show_results(self: &Rc<Self>, n: u64, scene: ResultsScene, voiced: Voiced<H::Sound>) {
        if !self.is(n) {
            return;
        }
        let Some(env) = self.host.env() else {
            return self.give_up(n, ms::status_no_results());
        };
        let (voice, lengths, words) = voiced;
        self.keep(n, &lengths, words);
        let back: ActionFn = {
            let weak = Rc::downgrade(self);
            Rc::new(move |_, cx| {
                if let Some(s) = weak.upgrade() {
                    s.leave(n, cx);
                }
            })
        };
        let actions = ResultsActions {
            retry: Some(back.clone()),
            song_list: Some(back),
            replay: None,
            offset: None,
        };
        let results = ResultsScreen::new(env.clone(), scene.data.clone(), actions);
        let frame = self.frame(env, n, &scene.caption.text());
        if !self.show(frame, results, Some(scene.mark)) {
            return self.give_up(n, ms::status_no_results());
        }
        self.with_scene(n, |sc| sc.marked = true);
        self.results_said(n, &scene, voice);
    }

    /// The results' line of scene `n`, said now by `voice`, as
    /// [`Announcer::results`] chooses it; the scene ends after it.
    fn results_said(self: &Rc<Self>, n: u64, scene: &ResultsScene, voice: Voice<H::Sound>) {
        let (spoken, now) = self.host.say_results(scene, voice).unwrap_or_default();
        let mut end = now;
        for (f, at) in &spoken {
            let Some(l) = Line::parse(f) else {
                continue;
            };
            let len = self.length(n, l);
            end = end.max(at + len);
            // Its words as it is said.
            self.later(n, at - now, move |s| s.say(n, l, len));
        }
        let wall = self.host.now_ms();
        self.with_scene(n, |sc| {
            sc.spoken = spoken
                .into_iter()
                .map(|(f, at)| (f, wall + (at - now) * 1000.0))
                .collect();
        });
        self.after(n, end - now + SCENE_TAIL, Next::End);
        self.host.publish();
    }

    /// A line said in play, begun in the press: the shell's play shows
    /// its scene. Taken up with [`Director::handed_over`] once that play
    /// has what it needs.
    pub fn begin_play(&self, line: Line, said: Said) -> u64 {
        self.begin(line, said, Shows::Play, false, "", None, false)
    }

    /// Scene `n` goes on on the shell's play.
    pub fn handed_over(&self, n: u64) {
        self.scene.borrow_mut().take_if(|s| s.n == n);
        self.host.changed();
    }

    /// What the last scene in play said, as the studio shows again: the
    /// status, and each line's next recording (or example) for the next.
    pub fn heard(&self, h: Heard) {
        self.left(h.line, h.said, h.spoken);
    }

    /// Scene `n`'s next step in `seconds`, by the input a player would
    /// give: Start (Singleplayer on the start menu, the chart on song
    /// select), Back to leave (the start menu, which Back does not leave,
    /// leaves as Singleplayer is chosen).
    fn after(self: &Rc<Self>, n: u64, seconds: f64, next: Next) {
        // Long enough to see the screen.
        let now = self.host.now_ms();
        let seconds = match (next, self.scene.borrow().as_ref()) {
            (Next::End, Some(sc)) => seconds.max(SCENE_SECONDS - (now - sc.began) / 1000.0),
            _ => seconds,
        };
        if matches!(next, Next::End) {
            self.until(n, seconds);
        }
        let weak = Rc::downgrade(self);
        let timer = self.host.timer(
            seconds.max(0.0),
            Box::new(move || {
                let Some(s) = weak.upgrade() else {
                    return;
                };
                let action = s.with_scene(n, |sc| match next {
                    Next::Start if sc.chosen => None,
                    Next::Start => Some(MenuAction::Start),
                    Next::End | Next::Leave if sc.screen == Shows::Start => {
                        sc.leaving = true;
                        Some(MenuAction::Start)
                    }
                    Next::End | Next::Leave => Some(MenuAction::Back),
                });
                if let Some(Some(action)) = action {
                    s.host.press(action);
                }
            }),
        );
        self.with_scene(n, |sc| sc.timer = Some(timer));
    }

    /// Scene `n` leaves (its screen closed in `cx`, the frame going):
    /// back in the studio, which says what was heard.
    fn leave(&self, n: u64, cx: &mut Cx) {
        let Some(sc) = self.scene.borrow_mut().take_if(|s| s.n == n) else {
            return;
        };
        cx.close(true);
        cx.broadcast(Message::new(stage::END));
        self.host.on_ui(Box::new(|ui| ui.mark(None)));
        self.host.stage_voice(None);
        if sc.preview {
            self.host.preview_close();
        }
        self.host.stop_sound();
        // What was heard: the lines begun by now (one scheduled later was
        // cut).
        let wall = self.host.now_ms();
        let heard: Vec<String> = sc
            .spoken
            .into_iter()
            .filter(|(_, at)| *at <= wall)
            .map(|(f, _)| f)
            .collect();
        self.left(sc.line, sc.said, heard);
    }

    /// A scene is over: each line heard takes its next sound next time,
    /// and the studio says what was heard.
    fn left(&self, line: Line, said: Said, heard: Vec<String>) {
        let lines: Vec<Line> = heard.iter().filter_map(|f| Line::parse(f)).collect();
        // A scene of a sound chosen leaves the turns as they were.
        let pinned = self.pin.borrow_mut().take().is_some();
        for &l in lines.iter().filter(|_| !pinned) {
            match said {
                Said::Draft => *self.turns.borrow_mut().entry(l).or_insert(0) += 1,
                Said::Example(set) => {
                    *self
                        .example_turns
                        .borrow_mut()
                        .entry((set.key(), l))
                        .or_insert(0) += 1
                }
            }
        }
        let example = match said {
            Said::Draft => None,
            Said::Example(set) => Some(set.pack()),
        };
        let status = studio::heard(line, &lines, example);
        self.host.heard(status, line, heard, said);
    }
}

#[cfg(test)]
mod tests;
