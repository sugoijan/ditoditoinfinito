//! The start prompt (`docs/plans/ui-core.md`, step 6.4), one screen for
//! both shells: the song, its chart and the mods it will play with over
//! its picture, until the player starts or goes back; or the linked folder
//! the browser asks about again, the devices not calibrated yet, an
//! interrupted play or why the song cannot play. Start never waits for
//! what the play needs: the shell readies it behind the prompt, and a
//! Start that comes first shows the loading screen ([`PromptStep::Loading`]:
//! "Loading" over the play's own background, whose entrance runs there)
//! until the play begins (`docs/plans/start-prompt.md`).
//!
//! The shell says what to show through [`PromptState`] (shared, its
//! `version` bumped on every change: the screen builds itself again) and
//! hears what the player chose through a [`PromptFn`], called inside the
//! input's handler: on the web the key, click or tap that starts the song
//! is still the user gesture the audio context must be made in
//! (`AGENTS.md`), and asking for a linked folder needs one too
//! ([`Cx::in_gesture`] says whether this one is: a pad's press is not in
//! WebKit).
//!
//! The cached song-list base stays visible while loading. A second copy
//! contracts into the background frame over 450 ms, easing toward the
//! configured brightness and a softer Gaussian blur. Start samples its
//! position so the play renderer can finish the unblur over 300 ms, even
//! when started early. Reduced motion shows the settled states. Shared
//! geometry and progress live in `ddi_actor::background`; this is visual
//! time only and never delays the synchronous audio gesture.
//!
//! Components: `Dialog` (the card, `Panel`'s fallback), `PlayButton` and
//! `Button`; colors `Play.Field` (the play's own, under the picture),
//! `Text`, `Muted`, `Faint`, `Accent`, `Danger`,
//! `Difficulty.<name>`; metric `Prompt.Width` (the card's), and
//! `Backdrop.Side` for the cached Gaussian-blurred copy.

use ddi_engine::MenuInput as HintAction;
use std::cell::RefCell;
use std::rc::Rc;

use ddi_actor::{Actor, Kind, Sprite};

use super::select_dialogs::{wrap, wrap_lines};
use super::{Env, footer, frame, left, right};
use crate::gesture::Gesture;
use crate::look::{self, Spec, call, named, quad};
use crate::nav::{Focusable, MenuAction};
use crate::screen::{Cx, Handled, Input, Screen};

/// What the prompt names: the song and its chart.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PromptInfo {
    pub title: String,
    /// The artist (or a calibration's hint).
    pub artist: String,
    /// "Hard 13 · Single" ([`chart_line`]); empty for none.
    pub chart: String,
    /// The difficulty's name for its color (`Difficulty.Hard`); empty:
    /// the text's.
    pub difficulty: String,
    /// The mods it will be played with ([`mods_line`]); empty for none.
    pub mods: String,
    /// "AUTOPLAY" when autoplayed, else empty.
    pub badge: String,
    /// The quiet suggestion to calibrate ([`timing_hint`]): `None`, no
    /// row (a replay, a test); `Some`, a row below the modifiers kept
    /// at one height, empty or with the line and its Calibrate link.
    pub timing: Option<String>,
}

/// The suggestion's line for `verdict`, what the kept plays say of the
/// timing on the devices in use, and `calibrated`, whether both have a
/// calibrated profile (`docs/plans/quiet-calibration.md`): a consistent
/// shift names its direction, calibrated or not; an uncalibrated setup
/// with nothing to go on gets the neutral line; empty otherwise (noise
/// around zero, or calibrated with nothing to go on), the row left
/// blank. The shells read the devices from the play's probe, else from
/// the profiles seen last ([`crate::state::settings::Settings::last_devices`]);
/// with none known at all, the setup is not calibrated.
pub fn timing_hint(verdict: &crate::state::history::timing::Verdict, calibrated: bool) -> String {
    use crate::state::history::timing::Verdict;
    match verdict {
        Verdict::Shift { ms, .. } => format!(
            "Hits run {:.0} ms {}",
            ms.abs(),
            if *ms > 0.0 { "late" } else { "early" }
        ),
        Verdict::Inconclusive { .. } if !calibrated => "Timing not calibrated".to_string(),
        _ => String::new(),
    }
}

/// The song's picture behind the prompt, as the play shows it before the
/// first change.
#[derive(Clone, Debug, PartialEq)]
pub struct PromptPicture {
    /// The image's texture source (the shell's: a file it keeps).
    pub source: String,
    /// Its width over its height.
    pub aspect: f32,
    /// The background frame's shape (width over height), `None`: the
    /// whole screen.
    pub frame: Option<f32>,
    /// The backgrounds' perceived brightness, `0..=1`.
    pub brightness: f32,
    /// The field's filter: its band's middle and width (fractions of the
    /// screen's width) and how dark, `0..=1` of the brightness removed.
    pub filter: Option<([f32; 2], f32)>,
}

/// What the prompt is at.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum PromptStep {
    /// The loading screen: Start was pressed before everything the play
    /// needs was ready ("loading…", "preparing audio…", for tests); the
    /// card gives way to "Loading" over the play's background, whose
    /// entrance runs here. Back leaves.
    Loading(String),
    /// Start plays (or shows the loading screen first: the shell says),
    /// whatever still loads in the background ([`PromptState::waiting`]).
    #[default]
    Ready,
    /// A linked song's folder needs the browser's permission again
    /// (asked from a gesture: [`PromptChoice::Allow`]); why the last ask
    /// failed.
    Access {
        folder: String,
        refused: Option<String>,
    },
    /// The play was cut off (the tab hidden, the audio suspended).
    Interrupted,
    /// What came of it (a calibration's outcome): a heading, lines with
    /// their colors' names, and actions (id, label), the first focused.
    Card {
        heading: String,
        lines: Vec<(String, String)>,
        actions: Vec<(String, String)>,
    },
    /// The song cannot play, and why.
    Error(String),
}

/// What the player chose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptChoice {
    /// Start the song (also while loading: a press meant to start it).
    Start,
    Back,
    /// Ask for the linked folder.
    Allow,
    /// After an interruption.
    Retry,
    /// The timing line's link: calibration's menu.
    Calibrate,
    /// A card's action, by its id.
    Action(String),
    /// A play option to value `index`, for this play
    /// (`super::prompt_options::PlayOptions::change`).
    Option {
        id: String,
        index: usize,
    },
    /// Keep the play options as the defaults.
    SaveOptions,
}

/// What the prompt shows, for tests (`ThemeState::prompt`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PromptView {
    /// The loading screen is up, and says "Loading" (past its delay).
    pub loading: bool,
    pub label: bool,
    /// The countdown's seconds left while it runs, and where it is
    /// (`off`, `running`, `cancelled`, `done`).
    pub countdown: Option<f32>,
    pub countdown_state: String,
    /// The play options (`id=value`), whether they differ from the
    /// stored settings, and whether they were just saved.
    pub options: Vec<String>,
    pub options_differ: bool,
    pub options_saved: bool,
    /// What the shell said still loads ([`PromptState::waiting`]).
    pub waiting: Vec<String>,
    pub bindings: Vec<String>,
    pub active_lane: Option<usize>,
}

/// What the shell shows; `version` goes up with every change.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PromptState {
    /// The song (its name may come after the prompt is up).
    pub info: PromptInfo,
    pub step: PromptStep,
    /// Lines under the song (the video decoder's download, "this browser
    /// needs a tap…").
    pub notes: Vec<String>,
    /// A touch screen: taps, not keys, and the ✕ button to quit.
    pub touch: bool,
    pub picture: Option<PromptPicture>,
    /// Cached song-list base, carried over before the play's pictures finish loading.
    pub base: Option<String>,
    /// Current visual position, sampled by Start to continue an early entrance.
    pub reveal: f32,
    /// How far the loading screen took the last unblur (0 → 1): the play
    /// continues from here with [`PromptState::entrance`].
    pub clarity: f32,
    /// What still loads, by the shell's names (`song`, `noteskin`,
    /// `audio`, `pictures`, `scripts`), for tests; empty: all is ready.
    /// Not shown, so a change does not bump the version.
    pub waiting: Vec<String>,
    /// The shell started the play: the screen closes itself at its next
    /// frame (desktop, where the prompt is a screen over song select).
    pub done: bool,
    /// Seconds the prompt counts down from when it offers Start, then
    /// presses it for the player (a play chosen in song select,
    /// `Settings::start_countdown`); `None`: it waits for Start. Enter
    /// skips it, any other input cancels it ([`countdown_input`]).
    pub countdown: Option<f32>,
    /// The play options this play runs with (`super::prompt_options`),
    /// whether they differ from the stored settings, and whether they were
    /// just saved as the defaults; none: not offered (a replay, a test).
    pub bindings: Option<Rc<super::prompt_controls::Controls>>,
    pub options: Vec<super::data::OptionDef>,
    pub options_differ: bool,
    pub options_saved: bool,
    pub version: u64,
}

impl PromptState {
    pub fn set_bindings(
        &mut self,
        layout: Option<&ddi_chart::Layout>,
        settings: &crate::state::settings::Settings,
    ) {
        if let (Some(layout), Some(old)) = (layout, self.bindings.as_ref())
            && old.layout == *layout
            && old.settings == *settings
        {
            return;
        }
        let fresh = layout.map(|l| Rc::new(super::prompt_controls::Controls::new(l, settings)));
        if self.bindings != fresh {
            self.bindings = fresh;
            self.version += 1;
        }
    }

    pub fn set_info(&mut self, info: PromptInfo) {
        if self.info != info {
            self.info = info;
            self.version += 1;
        }
    }

    pub fn set_step(&mut self, step: PromptStep) {
        if self.step != step {
            self.step = step;
            self.version += 1;
        }
    }

    pub fn set_notes(&mut self, notes: Vec<String>) {
        if self.notes != notes {
            self.notes = notes;
            self.version += 1;
        }
    }

    pub fn set_touch(&mut self, touch: bool) {
        if self.touch != touch {
            self.touch = touch;
            self.version += 1;
        }
    }

    pub fn set_base(&mut self, source: Option<String>) {
        if self.base != source {
            self.base = source;
            self.version += 1;
        }
    }

    pub fn set_options(&mut self, rows: Vec<super::data::OptionDef>, differ: bool, saved: bool) {
        if self.options != rows || self.options_differ != differ || self.options_saved != saved {
            self.options = rows;
            self.options_differ = differ;
            self.options_saved = saved;
            self.version += 1;
        }
    }

    pub fn set_countdown(&mut self, seconds: Option<f32>) {
        let seconds = seconds.filter(|s| *s > 0.0);
        if self.countdown != seconds {
            self.countdown = seconds;
            self.version += 1;
        }
    }

    pub fn set_waiting(&mut self, waiting: Vec<String>) {
        self.waiting = waiting;
    }

    /// The play started: the screen goes.
    pub fn finish(&mut self) {
        if !self.done {
            self.done = true;
            self.version += 1;
        }
    }

    /// Where the backdrop's entrance is, for the play to continue it
    /// (`ddi_actor::background::Entrance::resume`).
    pub fn entrance(&self) -> ddi_actor::background::Reveal {
        ddi_actor::background::Reveal {
            position: self.reveal,
            clarity: self.clarity,
        }
    }

    pub fn set_picture(&mut self, picture: Option<PromptPicture>) {
        if self.picture != picture {
            if self.base.is_none() {
                self.base = picture.as_ref().map(|p| p.source.clone());
            }
            self.picture = picture;
            self.version += 1;
        }
    }
}

pub type SharedPrompt = Rc<RefCell<PromptState>>;

/// The shell's answer to a choice, inside the input's handler (the
/// gesture, when it is one: [`Cx::in_gesture`]). It may close the screen
/// ([`Cx::close`]) or leave it to show what comes next.
pub type PromptFn = Rc<dyn Fn(PromptChoice, &mut Cx)>;

/// "Hard 13 · Single": a chart's difficulty, meter and style.
pub fn chart_line(difficulty: ddi_chart::Difficulty, meter: u32, style: &str) -> String {
    format!(
        "{} {meter} · {style}",
        super::data::difficulty_name(difficulty)
    )
}

/// The mods a play will run with, as the settings have them: the speed,
/// Reverse, then every option off its default ([`super::play::options_line`]).
pub fn mods_line(settings: &crate::state::settings::Settings) -> String {
    super::play::options_line(&settings.play_options(None))
}

/// The prompt's screen.
pub struct PromptScreen {
    env: Env,
    state: SharedPrompt,
    act: PromptFn,
    /// The state's version built.
    built: u64,
    /// The kind of step built.
    built_step: Option<std::mem::Discriminant<PromptStep>>,
    screen: [f32; 2],
    /// The viewport it was built for, and the zoom onto it (a phone
    /// upright: laid out 480 wide, zoomed to the width).
    vp: [f32; 2],
    zoom: f32,
    /// The buttons of the step built, the one focused first first.
    buttons: Vec<String>,
    /// Over song select (desktop): counted in [`super::ThemeState::over`].
    over: bool,
    /// The fade in started (at the first frame).
    faded: bool,
    reveal_time: f32,
    picture_source: Option<String>,
    /// The loading screen: how long it has shown, and its backdrop's
    /// entrance (from where the prompt's was, again from the start for a
    /// picture that comes meanwhile).
    loading: Option<Loading>,
    /// The last card built, which the loading screen fades out.
    card_shown: Option<Actor>,
    countdown: Countdown,
    /// A press that cancelled the countdown: its tap does not start.
    swallow_tap: bool,
    /// The play options' rows (`Opt.<id>`) and their `SaveOptions`, as
    /// built.
    controls: Vec<String>,
    demo_time: f32,
    preview_ready: Rc<std::cell::Cell<bool>>,
    demo_config: Option<Rc<super::prompt_controls::Controls>>,
    preview_size: std::cell::Cell<Option<[f32; 2]>>,
    preview_told: Option<super::prompt_controls::Picture>,
    /// When the diagram's picture came in (the screen's time): it fades
    /// in from then, over [`RECEPTORS_FADE`] s, through rebuilds.
    receptors_fade_at: Option<f64>,
}

/// Where the countdown is (`docs/plans/start-prompt.md`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Countdown {
    /// Not begun (begins when Start is first offered, if asked for).
    Off,
    /// Seconds left, of how many.
    Running(f32, f32),
    /// Something other than Start was pressed: Start waits.
    Cancelled,
    /// It pressed Start, or Start was pressed.
    Done,
}

/// What an input does to a running countdown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CountdownInput {
    /// Starts now (Enter, a pad's Start, Start tapped).
    Skip,
    /// Start waits (any other fresh press, a tap elsewhere, navigation).
    Cancel,
    /// Not the player's choice (releases, repeats, hover, the wheel,
    /// focus changes, Back, which leaves anyway).
    Ignore,
}

/// Keys that alone are no choice (switching windows, a shortcut begun).
fn modifier_only(key: &str) -> bool {
    matches!(
        key,
        "Shift"
            | "Control"
            | "Alt"
            | "AltGraph"
            | "Meta"
            | "OS"
            | "Super"
            | "Hyper"
            | "Fn"
            | "FnLock"
            | "CapsLock"
            | "NumLock"
            | "ScrollLock"
    )
}

/// How `input` counts for a running countdown: Enter, Space, a pad's
/// confirm or Start (the menu's Start) and a tap or click on Start skip it;
/// any other fresh key (but a modifier alone), a menu direction or Select,
/// a press, tap, long press or drag anywhere but on Start, and any other
/// button cancel it; releases, the OS's and our own repeats, hover, the
/// wheel (a trackpad's inertia from song select's list), focus changes and
/// Back (which leaves) do neither. The press that chose the song was song
/// select's: only its release and repeats can reach the prompt.
pub fn countdown_input(input: &Input) -> CountdownInput {
    use CountdownInput::*;
    match input {
        Input::Menu { repeat: true, .. } => Ignore,
        Input::Menu {
            action: MenuAction::Start,
            ..
        } => Skip,
        Input::Menu {
            action: MenuAction::Back,
            ..
        } => Ignore,
        Input::Menu { .. } => Cancel,
        Input::Key(k) if k.down && !k.repeat && !modifier_only(&k.key) => Cancel,
        Input::Key(_) => Ignore,
        Input::Pointer {
            target,
            gesture: Gesture::Press | Gesture::Tap | Gesture::LongPress | Gesture::DragStart,
            ..
        } => {
            if target.as_ref().is_some_and(|t| t.within("Start")) {
                Ignore
            } else {
                Cancel
            }
        }
        Input::Pointer { .. } => Ignore,
        Input::Activate { name } if name == "Start" => Skip,
        Input::Activate { name } if name == "Back" => Ignore,
        Input::Activate { .. } => Cancel,
        Input::Raw(e) if e.pressed => Cancel,
        _ => Ignore,
    }
}

/// The loading screen's clocks.
#[derive(Clone, Copy, Debug)]
struct Loading {
    shown: f32,
    entrance: f32,
    from: f32,
}

/// The card's text sizes.
const TITLE: f32 = 28.0;
const BODY: f32 = 15.0;
const SMALL: f32 = 13.0;
/// A play option's row's height.
const OPTION_ROW: f32 = 28.0;

/// The prompt's play options as tests read them (`id=value`).
fn st_options(st: &PromptState) -> Vec<String> {
    st.options
        .iter()
        .map(|o| format!("{}={}", o.id, o.value()))
        .collect()
}

impl PromptScreen {
    pub fn new(env: Env, state: SharedPrompt, act: PromptFn) -> PromptScreen {
        state.borrow_mut().reveal = if env.reduced() { 1.0 } else { 0.0 };
        PromptScreen {
            env,
            state,
            act,
            built: u64::MAX,
            built_step: None,
            screen: super::DESIGN,
            vp: [0.0, 0.0],
            zoom: 1.0,
            buttons: Vec::new(),
            over: false,
            faded: false,
            reveal_time: 0.0,
            picture_source: None,
            loading: None,
            card_shown: None,
            countdown: Countdown::Off,
            swallow_tap: false,
            controls: Vec::new(),
            demo_time: 0.0,
            preview_ready: Rc::default(),
            demo_config: None,
            preview_size: std::cell::Cell::new(None),
            preview_told: None,
            receptors_fade_at: None,
        }
    }

    /// The focused play option's row lit, the others not.
    fn light(&self, root: &mut Actor, focused: Option<&str>) {
        let [r, g, b, _] = self.env.color("Text");
        for name in self.controls.iter().filter(|n| n.starts_with("Opt.")) {
            let a = if focused == Some(name.as_str()) {
                0.08
            } else {
                0.004
            };
            if let Some(l) = crate::hit::find_mut(root, &format!("{name}.Light")) {
                call(l, "diffuse", &[r, g, b, a]);
            }
        }
    }

    /// Whether a target is on one of the prompt's controls (a button, a
    /// row, Save).
    fn on_control(&self, target: Option<&crate::hit::Target>) -> bool {
        target.is_some_and(|t| {
            self.buttons
                .iter()
                .chain(&self.controls)
                .any(|b| t.within(b))
        })
    }

    /// A play option changed by `by` (Left, Right, ◀ ▶), or cycled (a tap
    /// on its row); clamped as Options' rows are.
    fn change_option(&self, id: &str, by: i32, wrap: bool, cx: &mut Cx) {
        let Some(o) = self
            .state
            .borrow()
            .options
            .iter()
            .find(|o| o.id == id)
            .cloned()
        else {
            return;
        };
        let n = o.values().len() as i32;
        if n == 0 {
            return;
        }
        let to = o.index as i32 + by;
        let to = if wrap {
            to.rem_euclid(n)
        } else {
            to.clamp(0, n - 1)
        } as usize;
        if to != o.index {
            self.choose(
                PromptChoice::Option {
                    id: id.to_string(),
                    index: to,
                },
                cx,
            );
        }
    }

    /// The countdown's seconds left, while it runs.
    fn counting(&self) -> Option<(f32, f32)> {
        match self.countdown {
            Countdown::Running(left, of) => Some((left, of)),
            _ => None,
        }
    }

    /// Fill Start's background as the countdown advances. Reduced motion
    /// uses whole-second steps instead of a continuous wipe.
    fn update_countdown_fill(&self, root: &mut Actor) {
        let Some(fill) = crate::hit::find_mut(root, "CountdownFill") else {
            return;
        };
        let Some((left, total)) = self.counting() else {
            fill.visible = false;
            return;
        };
        let left = if self.env.reduced() {
            left.ceil().min(total)
        } else {
            left
        };
        let fraction = (1.0 - left / total).clamp(0.0, 1.0);
        let Some(ink) = fill.children().first() else {
            return;
        };
        let [w, h] = ink.size();
        fill.visible = true;
        fill.clip = Some([-w / 2.0, -h / 2.0, -w / 2.0 + w * fraction, h / 2.0]);
    }
    /// Shown over song select, as its other screens are (it says what it
    /// does again once none is).
    pub fn over_select(mut self) -> PromptScreen {
        self.over = true;
        self
    }

    fn choose(&self, choice: PromptChoice, cx: &mut Cx) {
        if matches!(
            choice,
            PromptChoice::Start | PromptChoice::Back | PromptChoice::Retry
        ) {
            cx.request(crate::UiRequest::ReceptorPreview(None));
        }
        (self.act)(choice, cx);
    }

    fn tree(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        self.vp = [vp.width, vp.height];
        // A phone upright: 480 wide (as options, results and song select
        // are), zoomed to the width; else the screen as it is.
        let narrow = self.env.metric("Options.Narrow.Width", 480.0);
        (self.screen, self.zoom) = if super::is_compact(&*self.env.look, vp) && vp.height > vp.width
        {
            let z = (vp.width / narrow).max(1.0);
            ([vp.width / z, vp.height / z], z)
        } else {
            ([vp.width, vp.height], 1.0)
        };
        self.preview_size.set(None);
        let state = self.state.borrow().clone();
        self.built = state.version;
        self.built_step = Some(std::mem::discriminant(&state.step));
        let env = self.env.clone();
        let screen = self.screen;
        // The play's own field color (`Play.Field`): a song without a
        // picture opens on it.
        let mut field = quad(
            "Ground",
            [screen[0] + 8.0, screen[1] + 8.0],
            env.color("Play.Field"),
        );
        call(&mut field, "xy", &[screen[0] / 2.0, screen[1] / 2.0]);
        let mut children = vec![field];
        children.push(picture(&env, screen, &state, cx));
        if matches!(state.step, PromptStep::Loading(_)) {
            // The loading screen: the card fading out (while it does),
            // then the label alone.
            self.buttons = Vec::new();
            self.controls = Vec::new();
            let shown = self.loading.map_or(0.0, |l| l.shown);
            if let Some(card) = &self.card_shown
                && shown < LOADING_FADE
                && !self.env.reduced()
            {
                children.push(card.clone());
            }
            children.push(loading_label(&env, screen));
            env.state.borrow_mut().hints = Vec::new();
        } else {
            let (card, buttons, controls) = self.card(&state, cx);
            self.buttons = buttons;
            self.controls = controls;
            self.card_shown = Some(card.clone());
            children.push(card);
        }
        if !state.touch && state.step == PromptStep::Ready {
            let hints: &[(&[&str], &str)] = &[(&["ENTER"], "Start"), (&["ESC"], "Back")];
            let (f, texts) = footer(
                &env,
                screen,
                hints,
                &[
                    (&[HintAction::Confirm, HintAction::Start], "Activate"),
                    (&[HintAction::Back], "Back"),
                ],
                "",
                &[],
                cx,
            );
            env.state.borrow_mut().hints = texts;
            children.push(f);
        } else if !matches!(state.step, PromptStep::Loading(_)) {
            env.state.borrow_mut().hints = Vec::new();
        }
        let mut root = Actor::new(Kind::Frame(children));
        if self.zoom != 1.0 {
            call(&mut root, "zoom", &[self.zoom]);
        }
        root
    }

    /// The card in the middle, its buttons (the first focused), and the
    /// play options' rows and their Save.
    fn card(&self, state: &PromptState, cx: &mut Cx) -> (Actor, Vec<String>, Vec<String>) {
        let env = &self.env;
        let [sw, sh] = self.screen;
        // A phone sideways: wider, so fewer lines and buttons lie on it.
        let w = if sh < 480.0 {
            (sw - 32.0).min(780.0)
        } else {
            env.metric("Prompt.Width", 560.0).min(sw - 32.0)
        };
        let inner = w - 48.0;
        let mut c: Vec<Actor> = Vec::new();
        let mut y = 24.0;
        let line = |c: &mut Vec<Actor>,
                    name: &str,
                    text: &str,
                    size: f32,
                    weight: u16,
                    color: &str,
                    y: &mut f32| {
            for (i, l) in wrap_lines(text, inner, size).into_iter().enumerate() {
                let mut t = env.text(&format!("{name}{i}"), &l, size, weight, color);
                wrap(&mut t, inner);
                *y += size * 0.65;
                c.push(left(t, 24.0, *y));
                *y += size * 0.65;
            }
        };
        let i = &state.info;
        // The song, on every step (once its name is known; asking for its
        // folder before it is read, the folder's name).
        let title = match &state.step {
            PromptStep::Access { folder, .. } if i.title.is_empty() => folder.clone(),
            _ => i.title.clone(),
        };
        if !title.is_empty() {
            let mut title = env.text("SongTitle", &title, TITLE, 700, "Text");
            wrap(&mut title, inner);
            y += TITLE * 0.6;
            c.push(left(title, 24.0, y));
            y += TITLE * 0.6 + 4.0;
        }
        if !i.artist.is_empty() {
            line(&mut c, "Artist", &i.artist, BODY, 400, "Muted", &mut y);
        }
        if !i.chart.is_empty() {
            y += 6.0;
            let color = if i.difficulty.is_empty() {
                "Text".to_string()
            } else {
                format!("Difficulty.{}", i.difficulty)
            };
            line(&mut c, "Chart", &i.chart, BODY, 700, &color, &mut y);
        }
        if !i.badge.is_empty() {
            line(&mut c, "Badge", &i.badge, SMALL, 700, "Accent", &mut y);
        }
        if !i.mods.is_empty() && (state.step != PromptStep::Ready || state.options.is_empty()) {
            line(&mut c, "Mods", &i.mods, SMALL, 400, "Muted", &mut y);
        }
        // The play options (while Start is offered): this play's,
        // Options' own rows.
        let mut controls: Vec<String> = Vec::new();
        if state.step == PromptStep::Ready
            && let Some(binding) = &state.bindings
        {
            y += 12.0;
            let (actors, height, size) =
                super::prompt_controls::draw(env, binding, inner, y, sh < 480.0, cx);
            c.extend(actors);
            y += height;
            self.preview_size.set(Some(size));
        }
        if state.step == PromptStep::Ready && !state.options.is_empty() {
            y += 14.0;
            let head = look::tracked(
                env.text("OptionsHeading", "MODIFIERS", 11.0, 600, "Faint"),
                120,
            );
            c.push(left(head, 24.0, y + 9.0));
            let save_y = y + 9.0;
            if state.options_differ {
                let label = "Save as default";
                let bw = look::mono_width(label, 13.0) + 24.0;
                let mut b = look::component_or(
                    &*env.look,
                    "Button",
                    "Button",
                    &Spec::new([bw, 26.0], label),
                );
                b.name = "SaveOptions".into();
                call(&mut b, "xy", &[w - 24.0 - bw / 2.0, save_y]);
                c.push(b);
                controls.push("SaveOptions".into());
            } else if state.options_saved {
                c.push(right(
                    env.text("OptionsSaved", "Saved as default", 12.0, 400, "Muted"),
                    w - 24.0,
                    save_y,
                ));
            }
            y += 22.0;
            // Two columns where the card is wide (read down the first,
            // then the second, as Up and Down go), else one.
            let cols = if inner >= 470.0 { 2 } else { 1 };
            let gap = 20.0;
            let colw = (inner - gap * (cols - 1) as f32) / cols as f32;
            let per = state.options.len().div_ceil(cols);
            let label_w = state
                .options
                .iter()
                .map(|o| look::mono_width(&o.label, 14.0))
                .fold(0.0, f32::max)
                .min(colw * 0.45);
            let vw = (colw - label_w - 52.0).clamp(60.0, 200.0);
            let top = y;
            for (k, o) in state.options.iter().enumerate() {
                let name = format!("Opt.{}", o.id);
                let (col, at) = (k / per, k % per);
                let x0 = 24.0 + col as f32 * (colw + gap);
                let vx = x0 + colw - vw - 44.0;
                let mid = OPTION_ROW / 2.0;
                let mut light = quad(
                    &format!("{name}.Light"),
                    [colw + 12.0, OPTION_ROW - 2.0],
                    [1.0, 1.0, 1.0, 0.004],
                );
                call(&mut light, "cornerradius", &[6.0]);
                call(&mut light, "xy", &[x0 + colw / 2.0, mid]);
                let mut label = env.text(&format!("{name}.Label"), &o.label, 14.0, 400, "Muted");
                call(&mut label, "maxwidth", &[(vx - x0 - 4.0).max(40.0)]);
                let mut value = env.text(&format!("{name}.Value"), o.value(), 14.0, 600, "Text");
                call(&mut value, "maxwidth", &[vw]);
                call(&mut value, "xy", &[vx + 22.0 + vw / 2.0, mid]);
                let arrow = |part: &str, glyph: &str, x: f32| {
                    let mut hit = quad(&format!("{name}.{part}"), [26.0, OPTION_ROW], [0.0; 4]);
                    call(&mut hit, "xy", &[x, mid]);
                    let mut g = env.text("", glyph, 12.0, 400, "Accent");
                    call(&mut g, "xy", &[x, mid]);
                    frame("", vec![hit, g])
                };
                let mut row = frame(
                    &name,
                    vec![
                        light,
                        left(label, x0, mid),
                        arrow("Down", "◀", vx + 9.0),
                        value,
                        arrow("Up", "▶", vx + 22.0 + vw + 13.0),
                    ],
                );
                call(&mut row, "y", &[top + at as f32 * OPTION_ROW]);
                c.push(row);
                controls.push(name);
            }
            y = top + per as f32 * OPTION_ROW;
        }
        y += 14.0;
        // The step.
        let mut buttons: Vec<(String, String, &str)> = Vec::new();
        match &state.step {
            // Not a card: the loading screen (`tree`).
            PromptStep::Loading(_) => {}
            PromptStep::Ready => {
                buttons.push(("Start".into(), "Start".into(), "Prompt.Start"));
                buttons.push(("Back".into(), "Back".into(), "Button"));
            }
            PromptStep::Access { folder, refused } => {
                let text = format!(
                    "This song plays from the folder “{folder}”, which the browser asks about again."
                );
                line(&mut c, "Status", &text, BODY, 400, "Secondary", &mut y);
                if let Some(r) = refused {
                    line(&mut c, "Refused", r, BODY, 600, "Danger", &mut y);
                }
                buttons.push((
                    "Allow".into(),
                    "Allow reading the folder".into(),
                    "PlayButton",
                ));
                buttons.push(("Back".into(), "Back".into(), "Button"));
            }
            PromptStep::Interrupted => {
                line(
                    &mut c,
                    "Status",
                    "Interrupted",
                    BODY + 3.0,
                    700,
                    "Text",
                    &mut y,
                );
                line(
                    &mut c,
                    "Why",
                    "The tab was hidden or audio was suspended.",
                    BODY,
                    400,
                    "Muted",
                    &mut y,
                );
                buttons.push(("Retry".into(), "Retry".into(), "PlayButton"));
                buttons.push(("Back".into(), "Back".into(), "Button"));
            }
            PromptStep::Error(e) => {
                line(&mut c, "Status", e, BODY, 600, "Danger", &mut y);
                buttons.push(("Back".into(), "Back".into(), "Button"));
            }
            PromptStep::Card {
                heading,
                lines,
                actions,
            } => {
                line(&mut c, "Status", heading, BODY + 3.0, 700, "Text", &mut y);
                for (k, (l, color)) in lines.iter().enumerate() {
                    line(&mut c, &format!("Line{k}."), l, BODY, 400, color, &mut y);
                }
                for (k, (id, label)) in actions.iter().enumerate() {
                    let kind = if k == 0 { "PlayButton" } else { "Button" };
                    buttons.push((format!("Action.{id}"), label.clone(), kind));
                }
            }
        }
        if state.step != PromptStep::Ready {
            for (k, n) in state.notes.iter().enumerate() {
                line(&mut c, &format!("Note{k}."), n, SMALL, 400, "Muted", &mut y);
            }
        }
        y += 18.0;
        // Taller on a phone upright (a thumb's).
        let bh = if self.zoom > 1.0 { 48.0 } else { 40.0 };
        // The buttons: in one row from the right when they fit, else one
        // a row (a new device's offers).
        let widths: Vec<f32> = buttons
            .iter()
            .map(|(_, l, kind)| {
                // The play button's label is larger (the theme's 18).
                let size = if matches!(*kind, "PlayButton" | "Prompt.Start") {
                    18.0
                } else {
                    15.0
                };
                // Its letter spacing too (the play button's is 0.08 em).
                let spacing = if *kind == "PlayButton" {
                    l.chars().count() as f32 * 0.08 * size
                } else {
                    0.0
                };
                (look::mono_width(l, size) + spacing + 36.0).max(104.0)
            })
            .collect();
        let one_row = widths.iter().sum::<f32>() + 10.0 * (buttons.len() as f32 - 1.0) <= inner;
        // The quiet suggestion to calibrate (`PromptInfo::timing`): its
        // line and link in the buttons' row where they fit beside the
        // buttons, else a row of their own above them, kept with or
        // without a line, so the card never moves for it.
        let timing = (state.step == PromptStep::Ready)
            .then_some(i.timing.as_deref())
            .flatten();
        let link_w = look::mono_width("Calibrate", 13.0) + 16.0;
        let timing_w = timing.map_or(0.0, |t| {
            if t.is_empty() {
                0.0
            } else {
                look::mono_width(t, SMALL) + 8.0 + link_w
            }
        });
        let free = inner - widths.iter().sum::<f32>() - 10.0 * (buttons.len() as f32 - 1.0);
        let inline = one_row && timing_w <= free - 16.0;
        // A short screen (a phone sideways) has no room for a row of its
        // own: the link alone beside the buttons says it.
        let link_only = !inline && sh < 480.0 && one_row && link_w <= free - 16.0;
        let place_timing = |c: &mut Vec<Actor>, controls: &mut Vec<String>, mid: f32| {
            let Some(hint) = timing.filter(|t| !t.is_empty()) else {
                return;
            };
            let tw = if link_only {
                0.0
            } else {
                look::mono_width(hint, SMALL).min(inner - link_w - 8.0)
            };
            if !link_only {
                let mut text = env.text("TimingHint", hint, SMALL, 400, "Muted");
                call(&mut text, "maxwidth", &[tw]);
                c.push(left(text, 24.0, mid));
            }
            let mut link = look::component_or(
                &*env.look,
                "Link",
                "Button",
                &Spec::new([link_w, 26.0], "Calibrate"),
            );
            link.name = "Calibrate".into();
            let gap = if link_only { 0.0 } else { 8.0 };
            call(&mut link, "xy", &[24.0 + tw + gap + link_w / 2.0, mid]);
            c.push(link);
            controls.push("Calibrate".into());
        };
        if timing.is_some() && !inline && !link_only {
            place_timing(&mut c, &mut controls, y + TIMING_ROW / 2.0);
            y += TIMING_ROW + 10.0;
        }
        if one_row {
            if inline || link_only {
                place_timing(&mut c, &mut controls, y + bh / 2.0);
            }
            // The first (focused) rightmost, as results put Retry.
            let mut x = w - 24.0;
            for ((name, label, kind), bw) in buttons.iter().zip(&widths) {
                c.push(pbutton(
                    env,
                    bh,
                    name,
                    label,
                    [x - bw / 2.0, y + bh / 2.0],
                    *bw,
                    kind,
                ));
                x -= bw + 10.0;
            }
            y += bh;
        } else {
            // In rows: the short ones two a row (the first, focused, on
            // top), then the long ones one a row.
            let half = (inner - 10.0) / 2.0;
            let (short, long): (Vec<usize>, Vec<usize>) =
                (0..buttons.len()).partition(|&k| widths[k] <= half);
            let mut rows: Vec<Vec<usize>> = short.chunks(2).map(<[usize]>::to_vec).collect();
            rows.extend(long.into_iter().map(|k| vec![k]));
            for row in rows {
                if let [a, b] = row[..] {
                    for (j, x) in [(a, 24.0 + half / 2.0), (b, w - 24.0 - half / 2.0)] {
                        let (name, label, kind) = &buttons[j];
                        c.push(pbutton(env, bh, name, label, [x, y + bh / 2.0], half, kind));
                    }
                } else {
                    let (name, label, kind) = &buttons[row[0]];
                    let bw = if widths[row[0]] <= half { half } else { inner };
                    let x = if bw < inner {
                        24.0 + half / 2.0
                    } else {
                        w / 2.0
                    };
                    c.push(pbutton(env, bh, name, label, [x, y + bh / 2.0], bw, kind));
                }
                y += bh + 10.0;
            }
            y -= 10.0;
        }
        let h = y + 24.0;
        let mut children = vec![env.component(
            "Dialog",
            "Panel",
            &Spec::new([w, h], ""),
            "Card",
            [0.0, 0.0],
        )];
        children.extend(c);
        let mut card = frame("Prompt", children);
        self.update_countdown_fill(&mut card);
        // Too tall for the screen (a phone sideways, many devices):
        // smaller.
        let room = sh - 16.0 - if state.touch { 0.0 } else { 40.0 };
        let zoom = (room / h).min(1.0);
        call(&mut card, "zoom", &[zoom]);
        if let Some(size) = self.preview_size.get() {
            let ratio = zoom * self.zoom;
            self.preview_size
                .set(Some([size[0] * ratio, size[1] * ratio]));
        }
        let top = ((sh - if state.touch { 0.0 } else { 40.0 }) - h * zoom) / 2.0;
        call(&mut card, "xy", &[(sw - w * zoom) / 2.0, top.max(8.0)]);
        (
            card,
            buttons.into_iter().map(|(n, _, _)| n).collect(),
            controls,
        )
    }

    fn rebuild(&mut self, root: &mut Actor, cx: &mut Cx) {
        let focused = cx.focused().map(str::to_string);
        let before = std::mem::discriminant(&self.state.borrow().step);
        let step = self.built_step;
        *root = self.tree(cx);
        root.finish_tweening();
        // The loading screen's card fades out from where it is.
        if let Some(l) = self.loading
            && l.shown < LOADING_FADE
            && let Some(card) = crate::hit::find_mut(root, "Prompt")
        {
            let a = 1.0 - l.shown / LOADING_FADE;
            look::run(
                card,
                &format!(
                    "diffusealpha,{a};linear,{};diffusealpha,0",
                    LOADING_FADE - l.shown
                ),
            );
        }
        self.show_loading(root);
        // The same step (a resize, a line changed): the focus stays; a new
        // one focuses its own first button.
        let keep = focused.filter(|f| {
            step == Some(before) && (self.buttons.contains(f) || self.controls.contains(f))
        });
        if let Some(f) = keep.or_else(|| self.buttons.first().cloned()) {
            cx.focus(f);
        }
    }

    /// The loading screen's label, shown once [`LOADING_DELAY`] passed
    /// (fading in, but under reduced motion), its dots stepping.
    fn show_loading(&self, root: &mut Actor) {
        let Some(l) = self.loading else {
            return;
        };
        let reduced = self.env.reduced();
        let alpha = if l.shown < LOADING_DELAY {
            0.0
        } else if reduced {
            1.0
        } else {
            ((l.shown - LOADING_DELAY) / LOADING_FADE).min(1.0)
        };
        if let Some(f) = crate::hit::find_mut(root, "Loading") {
            call(f, "diffusealpha", &[alpha]);
        }
        let step = if reduced { DOT_STEP_REDUCED } else { DOT_STEP };
        let dots = (((l.shown - LOADING_DELAY).max(0.0) / step) as usize) % 4;
        for i in 0..3 {
            if let Some(d) = crate::hit::find_mut(root, &format!("LoadingDot{i}")) {
                call(d, "diffusealpha", &[if i < dots { 1.0 } else { 0.0 }]);
            }
        }
    }

    /// The countdown: begun the first time Start is offered (when the
    /// shell asks for one), counted while it is, Start pressed at zero
    /// (outside any gesture: the shell knows what it may start then).
    fn tick_countdown(&mut self, dt: f32, root: &mut Actor, cx: &mut Cx) {
        let (ready, asked) = {
            let st = self.state.borrow();
            (st.step == PromptStep::Ready, st.countdown)
        };
        if !ready {
            return;
        }
        match self.countdown {
            Countdown::Off => {
                if let Some(s) = asked {
                    self.countdown = Countdown::Running(s, s);
                    self.rebuild(root, cx);
                }
            }
            Countdown::Running(left, of) => {
                let left = left - dt;
                if left <= 0.0 {
                    self.countdown = Countdown::Done;
                    self.rebuild(root, cx);
                    self.choose(PromptChoice::Start, cx);
                    return;
                }
                self.countdown = Countdown::Running(left, of);
                self.update_countdown_fill(root);
            }
            Countdown::Cancelled | Countdown::Done => {}
        }
    }

    /// What tests read of the prompt (`__DDI_DEBUG.ui.theme.prompt`).
    fn active_lane(&self) -> Option<usize> {
        if self.env.reduced() || self.state.borrow().step != PromptStep::Ready {
            return None;
        }
        self.state
            .borrow()
            .bindings
            .as_ref()
            .and_then(|c| c.active(self.demo_time))
    }
    fn draw_controls(&mut self, root: &mut Actor, cx: &mut Cx) {
        let active = self.active_lane();
        let st = self.state.borrow();
        let picture = self
            .preview_size
            .get()
            .zip(st.bindings.clone())
            .filter(|_| st.step == PromptStep::Ready)
            .map(|(size, controls)| super::prompt_controls::Picture {
                controls,
                size: size.map(|v| v * cx.viewport().scale),
                ready: self.preview_ready.clone(),
                time: self.demo_time,
                active,
            });
        if let Some(p) = &picture {
            for (i, cap) in p.controls.caps().iter().enumerate() {
                if let Some(key) = crate::hit::find_mut(root, &format!("KeyMotion{i}")) {
                    let pressed = active == Some(i) && cap.is_some();
                    let controller = matches!(cap, Some(crate::keycaps::Keycap::Controller(_)));
                    call(
                        key,
                        "y",
                        &[if pressed && !controller {
                            2.271 * 26.0 / 32.0
                        } else {
                            0.0
                        }],
                    );
                    if controller && let Some(body) = crate::hit::find_mut(key, "Cap0") {
                        call(body, "setstate", &[if pressed { 1.0 } else { 0.0 }]);
                    }
                    if controller && let Some(ink) = crate::hit::find_mut(key, "Key0") {
                        call(ink, "y", &[if pressed { -1.2 } else { -2.0 }]);
                    }
                }
            }
        }
        if self.preview_told != picture {
            cx.request(crate::UiRequest::ReceptorPreview(picture.clone()));
            self.preview_told = picture;
        }
        // The picture fades in once it is there (its texture drawn by the
        // host): set each frame, since the card is built again as the
        // state changes.
        if self.preview_ready.get() && self.receptors_fade_at.is_none() {
            self.receptors_fade_at = Some(cx.time());
        }
        if let Some(at) = self.receptors_fade_at {
            let alpha = if self.env.reduced() {
                1.0
            } else {
                ((cx.time() - at) / RECEPTORS_FADE).clamp(0.0, 1.0) as f32
            };
            for row in 0..4 {
                if let Some(image) = crate::hit::find_mut(root, &format!("Receptors{row}")) {
                    call(image, "diffusealpha", &[alpha]);
                }
            }
        }
    }

    fn publish_view(&self) {
        let view = PromptView {
            loading: self.loading.is_some(),
            label: self.loading.is_some_and(|l| l.shown >= LOADING_DELAY),
            countdown: self.counting().map(|(left, _)| left),
            countdown_state: match self.countdown {
                Countdown::Off => "off",
                Countdown::Running(..) => "running",
                Countdown::Cancelled => "cancelled",
                Countdown::Done => "done",
            }
            .into(),
            options: st_options(&self.state.borrow()),
            options_differ: self.state.borrow().options_differ,
            options_saved: self.state.borrow().options_saved,
            waiting: self.state.borrow().waiting.clone(),
            bindings: self
                .state
                .borrow()
                .bindings
                .as_ref()
                .map(|b| {
                    if crate::controller_hints::active() {
                        b.caps()
                            .iter()
                            .map(|k| {
                                k.as_ref()
                                    .map(crate::keycaps::Keycap::label)
                                    .unwrap_or_default()
                            })
                            .collect()
                    } else {
                        b.keys
                            .iter()
                            .map(|k| k.clone().unwrap_or_default())
                            .collect()
                    }
                })
                .unwrap_or_default(),
            active_lane: self.active_lane(),
        };
        let mut st = self.env.state.borrow_mut();
        if st.prompt.as_ref() != Some(&view) {
            st.prompt = Some(view);
        }
    }

    fn publish(&self) {
        let mut st = self.env.state.borrow_mut();
        st.screen = "prompt".into();
        let info = self.state.borrow().info.clone();
        st.song = info.title.clone();
        st.chart = (!info.chart.is_empty()).then_some(info.chart);
        st.status = match &self.state.borrow().step {
            PromptStep::Loading(s) => s.clone(),
            PromptStep::Ready => "ready".into(),
            PromptStep::Access { .. } => "access".into(),
            PromptStep::Interrupted => "interrupted".into(),
            PromptStep::Error(e) => format!("error: {e}"),
            PromptStep::Card { heading, .. } => format!("card: {heading}"),
        };
        st.backdrop = self
            .state
            .borrow()
            .picture
            .as_ref()
            .map(|p| p.source.clone());
        st.page_items = self.buttons.clone();
    }
}

impl Drop for PromptScreen {
    /// Gone (closed by the shell, or the stack let go): song select says
    /// what it does again once nothing is over it.
    fn drop(&mut self) {
        let mut st = self.env.state.borrow_mut();
        st.prompt = None;
        if self.over && self.built != u64::MAX {
            st.over = st.over.saturating_sub(1);
        }
    }
}

/// A button `h` high (the dialogs' are 40).
fn pbutton(
    env: &Env,
    h: f32,
    name: &str,
    label: &str,
    centre: [f32; 2],
    w: f32,
    kind: &str,
) -> Actor {
    let mut b = look::component_or(&*env.look, kind, "Button", &Spec::new([w, h], label));
    b.name = name.into();
    call(&mut b, "xy", &centre);
    b
}

/// A continuous blurred base, with a copy emerging into the content frame;
/// on the loading screen its last unblur too (a sharp copy under the
/// blurred ones, as the play's renderer draws it), so the play opens on
/// what is already there.
fn picture(env: &Env, [w, h]: [f32; 2], state: &PromptState, cx: &mut Cx) -> Actor {
    use ddi_actor::background::{BASE_SIDE, CONTENT_SIDE, Reveal, frame_rect};
    let image = |name: &str, rect: [f32; 4], texture: u32, tint: f32, alpha: f32| {
        let [x, y, w, h] = rect;
        let mut s = named(Kind::Sprite(Sprite::quad([w, h])), name);
        if let Kind::Sprite(sp) = &mut s.kind {
            sp.texture = texture;
            sp.fit = ddi_actor::Fit::Cover;
        }
        call(&mut s, "xy", &[x + w / 2.0, y + h / 2.0]);
        call(&mut s, "diffuse", &[tint, tint, tint, alpha]);
        s
    };
    let clarity = state.clarity.clamp(0.0, 1.0);
    let mut children = Vec::new();
    if let Some(base) = &state.base {
        // A song without a picture plays on its ground alone: the loading
        // screen lets the song list's base go as the play would.
        let fade = if state.picture.is_none() {
            1.0 - clarity
        } else {
            1.0
        };
        if fade > 0.0 {
            let texture = cx.blurred_texture(base, BASE_SIDE);
            children.push(image(
                "Base",
                [0.0, 0.0, w, h],
                texture,
                1.0,
                env.metric("Backdrop.Alpha", ddi_actor::background::BASE_ALPHA) * fade,
            ));
        }
    }
    if let Some(p) = &state.picture {
        let progress = state.reveal.clamp(0.0, 1.0);
        let reveal = Reveal {
            position: progress,
            clarity,
        };
        let rect = reveal.rect(w, h, p.frame);
        let b = ddi_actor::background::BASE_ALPHA
            + (p.brightness.clamp(0.0, 1.0) - ddi_actor::background::BASE_ALPHA) * progress;
        // The three opacities compose to the copy's opacity over the base.
        let [sharp, strong_alpha, soft_alpha] = reveal.alphas();
        if sharp > 0.0 {
            // Decoded once for the frame's size, wherever the copy is.
            let [_, _, fw, fh] = frame_rect(w, h, p.frame);
            let texture = cx.texture(&p.source, fw.max(fh));
            children.push(image("Sharp", rect, texture, b, sharp));
        }
        let texture = cx.blurred_texture(&p.source, BASE_SIDE);
        children.push(image("ContentStrong", rect, texture, b, strong_alpha));
        let texture = cx.blurred_texture(&p.source, CONTENT_SIDE);
        children.push(image("Picture", rect, texture, b, soft_alpha));
        if let Some(([mid, width], dark)) = p.filter
            && dark > 0.0
        {
            let mut filter = quad(
                "Filter",
                [width * w, h],
                [0.0, 0.0, 0.0, dark.clamp(0.0, 1.0) * progress],
            );
            call(&mut filter, "xy", &[mid * w, h / 2.0]);
            children.push(filter);
        }
    }
    frame("Backdrop", children)
}

/// The loading screen's "Loading" and its dots, in the lower right corner
/// (`Prompt.Loading`), hidden until [`LOADING_DELAY`] has passed.
fn loading_label(env: &Env, [w, h]: [f32; 2]) -> Actor {
    let size = env.metric("Prompt.Loading.Size", 15.0);
    let margin = env.metric("Margin", 32.0);
    let label = "Loading";
    let dot = look::mono_width(".", size);
    let x = w - margin - look::mono_width(label, size) - 3.0 * dot;
    let y = h - margin;
    let mut children = vec![left(
        env.text("LoadingText", label, size, 600, "Muted"),
        x,
        y,
    )];
    for i in 0..3 {
        children.push(left(
            env.text(&format!("LoadingDot{i}"), ".", size, 600, "Muted"),
            x + look::mono_width(label, size) + dot * i as f32,
            y,
        ));
    }
    let mut f = frame("Loading", children);
    call(&mut f, "diffusealpha", &[0.0]);
    f
}

/// How long the diagram's picture fades in, s, once its texture is
/// there.
const RECEPTORS_FADE: f64 = 0.15;
/// The timing line's row, virtual pixels.
const TIMING_ROW: f32 = 26.0;

/// How long the loading screen waits before it says "Loading" (the
/// backdrop's unblur), seconds: a short wait looks like the entrance alone.
const LOADING_DELAY: f32 = ddi_actor::background::PLAY_SECONDS;
/// The label's fade in, and the card's fade out, seconds.
const LOADING_FADE: f32 = 0.15;
/// How often the dots step, seconds (reduced motion: slower).
const DOT_STEP: f32 = 0.4;
const DOT_STEP_REDUCED: f32 = 1.0;

impl Screen for PromptScreen {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn name(&self) -> &str {
        "play-prompt"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        if self.over {
            self.env.state.borrow_mut().over += 1;
        }
        let root = self.tree(cx);
        self.publish();
        root
    }

    /// The buttons (Start first, focused first), then the play options'
    /// rows top to bottom and their Save; Up from the buttons goes up
    /// through Save and the rows, Down back.
    fn focusables(&self) -> Vec<Focusable> {
        let rows: Vec<&String> = self
            .controls
            .iter()
            .filter(|n| n.starts_with("Opt."))
            .collect();
        let save = self.controls.iter().find(|n| *n == "SaveOptions");
        let link = self.controls.iter().find(|n| *n == "Calibrate");
        let above_buttons = link.or(save).or(rows.last().copied()).cloned();
        let mut out: Vec<Focusable> = self
            .buttons
            .iter()
            .map(|b| {
                let mut f = Focusable::new(b);
                f.up = above_buttons.clone();
                f
            })
            .collect();
        for (i, r) in rows.iter().enumerate() {
            let mut f = Focusable::new(*r);
            f.up = i.checked_sub(1).map(|j| rows[j].clone());
            f.down = rows
                .get(i + 1)
                .map(|r| (*r).clone())
                .or_else(|| save.cloned())
                .or_else(|| link.cloned())
                .or_else(|| self.buttons.first().cloned());
            out.push(f);
        }
        if let Some(s) = save {
            let mut f = Focusable::new(s);
            f.up = rows.last().map(|r| (*r).clone());
            f.down = link.cloned().or_else(|| self.buttons.first().cloned());
            out.push(f);
        }
        if let Some(l) = link {
            let mut f = Focusable::new(l);
            f.up = save.cloned().or_else(|| rows.last().map(|r| (*r).clone()));
            f.down = self.buttons.first().cloned();
            out.push(f);
        }
        out
    }

    /// Frames while something loads (the shell's lines change), so they
    /// are read.
    fn animating(&self) -> bool {
        // Changed by the shell since built (a picture, a line, a step):
        // a frame builds it, whatever the step.
        let st = self.state.borrow();
        matches!(st.step, PromptStep::Loading(_))
            || (st.bindings.is_some() && !self.env.reduced() && st.step == PromptStep::Ready)
            || st.done
            || self.counting().is_some()
            || (self.countdown == Countdown::Off && st.countdown.is_some())
            || st.version != self.built
            || (st.picture.is_some() && self.reveal_time < ddi_actor::background::PROMPT_SECONDS)
    }

    fn update(&mut self, dt: f32, root: &mut Actor, cx: &mut Cx) {
        // The shell started the play: gone.
        if self.state.borrow().done {
            cx.request(crate::UiRequest::ReceptorPreview(None));
            cx.close(false);
            return;
        }
        // Fades in from the first frame, taking keys at once: built into
        // the tree, the fade would be the screen's entrance, which holds
        // input back (a quick Enter or Escape was lost). None under
        // reduced motion. Under the web's loader, unseen until the
        // loader fades (`Cx::holding`), then the fade.
        if !self.faded
            && let Some(card) = crate::hit::find_mut(root, "Prompt")
        {
            if cx.holding() {
                call(card, "diffusealpha", &[0.0]);
            } else {
                self.faded = true;
                if !self.env.reduced() {
                    let t = self.env.metric("Motion.Screen", 0.2);
                    look::run(card, &format!("diffusealpha,0;linear,{t};diffusealpha,1"));
                }
            }
        }
        let reduced = self.env.reduced();
        let source = self
            .state
            .borrow()
            .picture
            .as_ref()
            .map(|p| p.source.clone());
        let new_source = source != self.picture_source;
        if new_source {
            self.picture_source = source;
            self.reveal_time = 0.0;
        }
        let loading = matches!(self.state.borrow().step, PromptStep::Loading(_));
        let moved = if loading {
            // The loading screen: the entrance the play would run at its
            // first frame, run here (`Reveal::continuing`), from where
            // the prompt's was; a picture that comes meanwhile enters
            // from the start.
            let before = self.state.borrow().entrance();
            let l = match self.loading {
                Some(mut l) => {
                    l.shown += dt;
                    l.entrance += dt;
                    if new_source {
                        l.entrance = 0.0;
                        l.from = 0.0;
                    }
                    l
                }
                None => Loading {
                    shown: 0.0,
                    entrance: 0.0,
                    from: if new_source { 0.0 } else { before.position },
                },
            };
            self.loading = Some(l);
            let now = ddi_actor::background::Reveal::continuing(
                ddi_actor::background::Reveal {
                    position: l.from,
                    clarity: 0.0,
                },
                l.entrance,
                reduced,
            );
            let mut st = self.state.borrow_mut();
            st.reveal = now.position;
            st.clarity = now.clarity;
            new_source || now != before
        } else {
            self.loading = None;
            self.state.borrow_mut().clarity = 0.0;
            if self.picture_source.is_some() {
                let duration = ddi_actor::background::PROMPT_SECONDS;
                let before = self.reveal_time;
                self.reveal_time = if reduced {
                    duration
                } else {
                    (self.reveal_time + dt).min(duration)
                };
                self.state.borrow_mut().reveal =
                    ddi_actor::background::ease(self.reveal_time / duration);
                new_source || self.reveal_time != before
            } else {
                false
            }
        };
        let config = self.state.borrow().bindings.clone();
        if self.demo_config != config {
            self.demo_time = 0.0;
            self.preview_ready.set(false);
            self.receptors_fade_at = None;
            self.demo_config = config;
        }
        if self.preview_ready.get()
            && !reduced
            && !loading
            && let Some(c) = &self.demo_config
        {
            self.demo_time = (self.demo_time + dt) % (c.duration() + 1.0);
        }
        self.tick_countdown(dt, root, cx);
        // Built again only while it moves: any other change bumps the
        // version, which builds the whole tree below.
        if moved {
            let state = self.state.borrow();
            if let Some(backdrop) = crate::hit::find_mut(root, "Backdrop") {
                *backdrop = picture(&self.env, self.screen, &state, cx);
            }
        }
        let vp = cx.viewport().screen;
        let resized = [vp.width, vp.height] != self.vp;
        if resized || self.state.borrow().version != self.built {
            self.rebuild(root, cx);
            self.publish();
        } else {
            self.show_loading(root);
        }
        if self.env.state.borrow().screen != "prompt" {
            self.publish();
        }
        let focused = cx.focused().map(str::to_string);
        self.light(root, focused.as_deref());
        self.draw_controls(root, cx);
        self.publish_view();
    }

    fn message(&mut self, m: &crate::Message, root: &mut Actor, cx: &mut Cx) {
        if m.name == crate::controller_hints::CHANGED {
            self.rebuild(root, cx);
            self.draw_controls(root, cx);
        }
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        let step = self.state.borrow().step.clone();
        // The loading screen: Back leaves; Start or a tap reach the shell
        // (a pad's start waiting for a gesture takes it); nothing else.
        if matches!(step, PromptStep::Loading(_)) {
            match input {
                Input::Menu {
                    action: MenuAction::Back,
                    repeat: false,
                } => self.choose(PromptChoice::Back, cx),
                Input::Menu {
                    action: MenuAction::Start,
                    repeat: false,
                }
                | Input::Pointer {
                    gesture: Gesture::Tap,
                    ..
                } => self.choose(PromptChoice::Start, cx),
                _ => {}
            }
            return Handled::Yes;
        }
        // The countdown: skipped (the press goes on to start), or
        // cancelled (the press does what it does, a tap anywhere does not
        // start then).
        if self.counting().is_some() {
            match countdown_input(input) {
                CountdownInput::Skip => {
                    self.countdown = Countdown::Done;
                    self.publish_view();
                }
                CountdownInput::Cancel => {
                    self.countdown = Countdown::Cancelled;
                    self.rebuild(root, cx);
                    self.publish_view();
                    // On the background: its tap does not start.
                    if let Input::Pointer {
                        target,
                        gesture: Gesture::Press | Gesture::Tap | Gesture::LongPress,
                        ..
                    } = input
                        && !self.on_control(target.as_ref())
                    {
                        self.swallow_tap = true;
                    }
                }
                CountdownInput::Ignore => {}
            }
        }
        // The play options' rows: Left and Right change the focused one
        // (held, repeating), a tap on ◀ ▶ steps it, on the row cycles it.
        if step == PromptStep::Ready {
            let row = cx
                .focused()
                .and_then(|f| f.strip_prefix("Opt."))
                .map(str::to_string);
            if let (
                Input::Menu {
                    action: action @ (MenuAction::Left | MenuAction::Right),
                    ..
                },
                Some(id),
            ) = (input, &row)
            {
                let by = if *action == MenuAction::Left { -1 } else { 1 };
                self.change_option(id, by, false, cx);
                return Handled::Yes;
            }
            if let Input::Pointer {
                target: Some(t),
                gesture: Gesture::Tap,
                ..
            } = input
                && let Some(name) = self
                    .controls
                    .iter()
                    .find(|n| n.starts_with("Opt.") && t.within(n))
                    .cloned()
            {
                let id = &name["Opt.".len()..];
                let (by, wrap) = if t.within(&format!("{name}.Down")) {
                    (-1, false)
                } else if t.within(&format!("{name}.Up")) {
                    (1, false)
                } else {
                    (1, true)
                };
                cx.focus(name.clone());
                self.change_option(id, by, wrap, cx);
                return Handled::Yes;
            }
        }
        match input {
            Input::Menu { repeat: true, .. } => Handled::Yes,
            Input::Menu {
                action: MenuAction::Back,
                ..
            } => {
                self.choose(PromptChoice::Back, cx);
                Handled::Yes
            }
            // Start starts, unless Back has the focus (then the menu's
            // default activates it).
            Input::Menu {
                action: MenuAction::Start,
                ..
            } if step == PromptStep::Ready
                && !matches!(cx.focused(), Some("Back" | "SaveOptions" | "Calibrate")) =>
            {
                self.choose(PromptChoice::Start, cx);
                Handled::Yes
            }
            Input::Activate { name } => {
                let choice = match name.as_str() {
                    "Start" => Some(PromptChoice::Start),
                    "Back" => Some(PromptChoice::Back),
                    "Allow" => Some(PromptChoice::Allow),
                    "Retry" => Some(PromptChoice::Retry),
                    "SaveOptions" => Some(PromptChoice::SaveOptions),
                    "Calibrate" => Some(PromptChoice::Calibrate),
                    n => n
                        .strip_prefix("Action.")
                        .map(|id| PromptChoice::Action(id.to_string())),
                };
                match choice {
                    Some(c) => {
                        self.choose(c, cx);
                        Handled::Yes
                    }
                    None => Handled::No,
                }
            }
            // A tap anywhere but a button starts (the web's prompt was one
            // big button), but the one that stopped the countdown.
            Input::Pointer {
                target,
                gesture: Gesture::Tap,
                ..
            } if step == PromptStep::Ready && !self.on_control(target.as_ref()) => {
                if !std::mem::take(&mut self.swallow_tap) {
                    self.choose(PromptChoice::Start, cx);
                }
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{MenuEvent, UiEvent};
    use crate::testing::Harness;
    use crate::{PointerKind, TextMode};

    type Seen = Rc<RefCell<Vec<(PromptChoice, bool)>>>;

    thread_local! {
        static LAST_ENV: RefCell<Option<Env>> = const { RefCell::new(None) };
    }

    fn prompt(state: &SharedPrompt) -> (Harness, Seen) {
        prompt_with(state, false)
    }

    fn prompt_with(state: &SharedPrompt, reduced: bool) -> (Harness, Seen) {
        let env = Env::new(Rc::new(look::Plain), Rc::new(|s: &str| s.to_string()));
        env.reduced_motion.set(reduced);
        LAST_ENV.with(|e| *e.borrow_mut() = Some(env.clone()));
        let seen: Seen = Rc::default();
        let act: PromptFn = {
            let seen = seen.clone();
            Rc::new(move |c, cx: &mut Cx| seen.borrow_mut().push((c, cx.in_gesture())))
        };
        let info = PromptInfo {
            title: "Some Things Must".into(),
            artist: "Sevish".into(),
            chart: chart_line(ddi_chart::Difficulty::Hard, 13, "Single"),
            difficulty: "Hard".into(),
            mods: "×3 · reverse".into(),
            badge: String::new(),
            timing: None,
        };
        let mut h = Harness::new(super::super::DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
        state.borrow_mut().set_info(info);
        h.push(PromptScreen::new(env, state.clone(), act));
        h.run(0.3);
        (h, seen)
    }

    fn controls_state() -> SharedPrompt {
        let state = SharedPrompt::default();
        let settings = crate::state::settings::Settings::default();
        let layout = ddi_chart::Layout::builtin("danoni-7").unwrap();
        state.borrow_mut().set_bindings(Some(&layout), &settings);
        super::super::prompt_options::PlayOptions::new(
            settings,
            &layout.id,
            super::super::data::Platform::Web,
        )
        .show(&mut state.borrow_mut());
        state
    }

    fn latest_picture(h: &Harness) -> &super::super::prompt_controls::Picture {
        h.requests
            .iter()
            .rev()
            .find_map(|r| match r {
                crate::UiRequest::ReceptorPreview(Some(p)) => Some(p),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn controls_wait_for_the_skin_and_animate_keys_with_receptors() {
        let state = controls_state();
        let (mut h, seen) = prompt(&state);
        assert_eq!(view().bindings.len(), 7);
        assert!(h.ui.actor("BindingKey6").is_some());
        h.run(1.0);
        assert_eq!(latest_picture(&h).time, 0.0);
        latest_picture(&h).ready.set(true);
        h.run(0.25);
        assert_eq!(latest_picture(&h).active, Some(0));
        assert!(h.actor("KeyMotion0").current().pos[1] > 0.0);
        h.run(1.5);
        assert_eq!(latest_picture(&h).active, None);
        assert!(seen.borrow().is_empty());
        assert!(h.ui.actor("Opt.speed").is_some());
        assert!(h.ui.actor("BindingKey0").is_some());
        assert!(h.ui.actor("Tab.Controls").is_none());
        for name in ["BindingCaption", "Status0", "Note0.0", "Quit0"] {
            assert!(
                h.ui.actor(name).is_none(),
                "unexpected explanatory line {name}"
            );
        }
        let key = h.ui.bounds("BindingKey0").unwrap();
        let row = h.ui.bounds("Opt.speed").unwrap();
        assert!(row[1] > key[3], "modifiers belong below the bindings");
        // With no replay button, the sequence loops after its quiet pause.
        let until_next =
            latest_picture(&h).controls.duration() + 1.0 - latest_picture(&h).time + 0.25;
        h.run(until_next);
        assert_eq!(latest_picture(&h).active, Some(0));
        assert!(h.ui.actor("ReplayControls").is_none());
        h.tap(PointerKind::Mouse, "Start");
        assert_eq!(*seen.borrow(), [(PromptChoice::Start, true)]);
        assert!(matches!(
            h.requests.last(),
            Some(crate::UiRequest::ReceptorPreview(None))
        ));
    }

    #[test]
    fn auto_start_cuts_the_demo_even_if_the_skin_is_not_ready() {
        let state = controls_state();
        state.borrow_mut().set_countdown(Some(0.5));
        let (mut h, seen) = prompt(&state);
        h.run(0.3);
        assert_eq!(starts(&seen), [false]);
    }

    #[test]
    fn reduced_motion_shows_bindings_without_synthetic_presses() {
        let state = controls_state();
        let (mut h, seen) = prompt_with(&state, true);
        latest_picture(&h).ready.set(true);
        h.run(2.0);
        assert_eq!(latest_picture(&h).time, 0.0);
        assert_eq!(latest_picture(&h).active, None);
        assert!(seen.borrow().is_empty());
    }

    #[test]
    fn start_and_back_reach_the_shell_inside_the_gesture() {
        let state = SharedPrompt::default();
        let (mut h, seen) = prompt(&state);
        assert!(h.texts().iter().any(|t| t == "Hard 13 · Single"));
        h.key("Enter", "Enter");
        // A pad's Start the browser does not count as a gesture.
        h.send(UiEvent::Menu(MenuEvent {
            action: MenuAction::Start,
            down: true,
            source: 0,
            gesture: false,
            time: h.now,
        }));
        // A tap anywhere but a button starts; the Back button goes back.
        h.tap_at(PointerKind::Touch, [100.0, 100.0]);
        h.tap(PointerKind::Mouse, "Back");
        h.key("Escape", "Escape");
        let got = seen.borrow().clone();
        assert_eq!(
            got[..4],
            [
                (PromptChoice::Start, true),
                (PromptChoice::Start, false),
                (PromptChoice::Start, true),
                (PromptChoice::Back, true),
            ]
        );
        // Escape too (going back needs no gesture).
        assert_eq!(got[4].0, PromptChoice::Back);
    }

    #[test]
    fn the_shells_steps_show_as_they_change() {
        let state = SharedPrompt::default();
        state
            .borrow_mut()
            .set_step(PromptStep::Loading("preparing audio…".into()));
        let (mut h, seen) = prompt(&state);
        // The loading screen: no card, no buttons.
        assert!(h.ui.actor("Start").is_none());
        assert!(h.ui.actor("Card").is_none());
        // A press while it loads still reaches the shell (a pad's start
        // waiting for a gesture takes it).
        h.key("Enter", "Enter");
        assert_eq!(seen.borrow().last(), Some(&(PromptChoice::Start, true)));
        state.borrow_mut().set_step(PromptStep::Access {
            folder: "Pack".into(),
            refused: Some("Reading the folder was not allowed.".into()),
        });
        h.run(0.1);
        assert!(h.texts().iter().any(|t| t.contains("“Pack”")));
        h.key("Enter", "Enter");
        assert_eq!(seen.borrow().last(), Some(&(PromptChoice::Allow, true)));
        state.borrow_mut().set_step(PromptStep::Interrupted);
        h.run(0.1);
        h.key("Enter", "Enter");
        assert_eq!(seen.borrow().last(), Some(&(PromptChoice::Retry, true)));
        state
            .borrow_mut()
            .set_notes(vec!["loading the video decoder… 40 %".into()]);
        state.borrow_mut().set_step(PromptStep::Ready);
        h.run(0.1);
        assert!(!h.texts().iter().any(|t| t.contains("40 %")));
    }

    #[test]
    fn keys_work_from_the_first_frame() {
        let state = SharedPrompt::default();
        state.borrow_mut().set_step(PromptStep::Ready);
        let env = Env::new(Rc::new(look::Plain), Rc::new(|s: &str| s.to_string()));
        let seen: Seen = Rc::default();
        let act: PromptFn = {
            let seen = seen.clone();
            Rc::new(move |c, cx: &mut Cx| seen.borrow_mut().push((c, cx.in_gesture())))
        };
        let mut h = Harness::new(super::super::DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
        h.push(PromptScreen::new(env, state, act));
        // No time for an entrance: Escape at once goes back.
        assert_eq!(h.ui.layers()[0].phase, "active");
        h.key("Escape", "Escape");
        assert_eq!(
            seen.borrow().last().map(|s| &s.0),
            Some(&PromptChoice::Back)
        );
    }

    #[test]
    fn enter_starts_unless_back_has_the_focus() {
        let state = SharedPrompt::default();
        let (mut h, seen) = prompt(&state);
        assert_eq!(h.ui.focused(), Some("Start"));
        h.key("Enter", "Enter");
        assert_eq!(
            seen.borrow().last().map(|s| &s.0),
            Some(&PromptChoice::Start)
        );
        h.key("ArrowLeft", "ArrowLeft");
        assert_eq!(h.ui.focused(), Some("Back"));
        h.key("Enter", "Enter");
        assert_eq!(
            seen.borrow().last().map(|s| &s.0),
            Some(&PromptChoice::Back)
        );
    }

    fn picture() -> PromptPicture {
        PromptPicture {
            source: "play:picture".into(),
            aspect: 4.0 / 3.0,
            frame: Some(4.0 / 3.0),
            brightness: 0.75,
            filter: Some(([0.5, 0.3], 0.4)),
        }
    }

    #[test]
    fn the_loading_screen_runs_the_plays_entrance_and_says_loading_late() {
        let state = SharedPrompt::default();
        state.borrow_mut().set_picture(Some(picture()));
        let (mut h, seen) = prompt(&state);
        h.run(0.5);
        assert_eq!(state.borrow().reveal, 1.0);
        assert_eq!(state.borrow().clarity, 0.0);
        // Start pressed before all was ready: the shell's loading step.
        state
            .borrow_mut()
            .set_step(PromptStep::Loading("loading…".into()));
        h.run(0.1);
        assert!(view().loading && !view().label);
        let c = state.borrow().clarity;
        assert!(c > 0.0 && c < 1.0, "{c}");
        // The sharp copy under the blurred ones, as the play draws them.
        assert!(h.ui.actor("Sharp").is_some());
        // Its buttons go with the card fading out.
        LAST_ENV.with(|e| {
            assert!(
                e.borrow()
                    .as_ref()
                    .unwrap()
                    .state
                    .borrow()
                    .page_items
                    .is_empty()
            )
        });
        // Settled: the play's first frame continues from nothing left.
        h.run(0.35);
        assert_eq!(
            state.borrow().entrance(),
            ddi_actor::background::Reveal::default()
        );
        assert!(view().label);
        assert!(h.texts().iter().any(|t| t == "Loading"));
        // Only Back and Start (for the shell) do anything.
        h.key("KeyA", "a");
        h.key("ArrowDown", "ArrowDown");
        assert!(seen.borrow().is_empty() || seen.borrow().last().unwrap().0 == PromptChoice::Start);
        h.key("Escape", "Escape");
        assert_eq!(
            seen.borrow().last().map(|s| &s.0),
            Some(&PromptChoice::Back)
        );
        // The shell started the play: the screen goes.
        state.borrow_mut().finish();
        h.run(0.05);
        assert!(h.ui.layers().is_empty());
    }

    /// What the last prompt made published.
    fn view() -> PromptView {
        LAST_ENV.with(|e| {
            e.borrow()
                .as_ref()
                .and_then(|e: &Env| e.state.borrow().prompt.clone())
                .unwrap_or_default()
        })
    }

    #[test]
    fn a_song_without_a_picture_loses_the_lists_base_on_the_loading_screen() {
        let state = SharedPrompt::default();
        state.borrow_mut().set_base(Some("cached:menu".into()));
        let (mut h, _) = prompt(&state);
        assert!(h.ui.actor("Base").is_some());
        state
            .borrow_mut()
            .set_step(PromptStep::Loading("loading…".into()));
        h.run(0.5);
        // The play's own ground alone, as the play draws it.
        assert!(h.ui.actor("Base").is_none());
        assert!(h.ui.actor("Ground").is_some());
    }

    #[test]
    fn under_reduced_motion_the_loading_screen_settles_at_once() {
        let state = SharedPrompt::default();
        state.borrow_mut().set_picture(Some(picture()));
        let (mut h, _) = prompt_with(&state, true);
        state
            .borrow_mut()
            .set_step(PromptStep::Loading("loading…".into()));
        h.run(0.02);
        assert_eq!(
            state.borrow().entrance(),
            ddi_actor::background::Reveal::default()
        );
        // The label after its delay, its dots a second each.
        h.run(0.4);
        assert!(view().label);
    }

    #[test]
    fn the_timing_line_keeps_its_row_and_links_to_calibration() {
        use crate::state::history::timing::Verdict;
        // The wording, by verdict and whether the devices are calibrated.
        let late = Verdict::Shift {
            ms: 21.4,
            plays: 3,
            hits: 360,
        };
        assert_eq!(timing_hint(&late, true), "Hits run 21 ms late");
        let early = Verdict::Shift {
            ms: -16.0,
            plays: 3,
            hits: 360,
        };
        assert!(timing_hint(&early, false).contains("16 ms early"));
        let none = Verdict::Inconclusive { plays: 0, hits: 0 };
        assert_eq!(timing_hint(&none, false), "Timing not calibrated");
        assert_eq!(timing_hint(&none, true), "");
        let fine = Verdict::Fine {
            plays: 5,
            hits: 500,
        };
        assert_eq!(timing_hint(&fine, false), "");
        // On the card: a row with the line and its link, or without, the
        // card the same height either way; no row for a replay.
        let state = controls_state();
        let (mut h, seen) = prompt(&state);
        let height =
            |h: &Harness| h.ui.bounds("Card").unwrap()[3] - h.ui.bounds("Card").unwrap()[1];
        let without = height(&h);
        let mut info = state.borrow().info.clone();
        info.timing = Some(String::new());
        state.borrow_mut().set_info(info.clone());
        h.run(0.1);
        assert!(h.ui.actor("Calibrate").is_none());
        // Beside the buttons on a wide card: no height of its own.
        assert_eq!(height(&h), without);
        info.timing = Some("Timing not calibrated".into());
        state.borrow_mut().set_info(info);
        h.run(0.1);
        assert_eq!(height(&h), without);
        assert!(h.texts().iter().any(|t| t.contains("not calibrated")));
        let link = h.ui.bounds("Calibrate").unwrap();
        let start = h.ui.bounds("Start").unwrap();
        assert!(
            link[2] < start[0] && (link[1] - start[1]).abs() < 12.0,
            "{link:?} {start:?}"
        );
        // Reached from Start with Up, pressed, it asks for calibration.
        h.run(0.3);
        assert_eq!(h.ui.focused(), Some("Start"));
        h.key("ArrowUp", "ArrowUp");
        assert_eq!(h.ui.focused(), Some("Calibrate"));
        h.key("Enter", "Enter");
        assert_eq!(seen.borrow().last(), Some(&(PromptChoice::Calibrate, true)));
        h.key("ArrowDown", "ArrowDown");
        assert_eq!(h.ui.focused(), Some("Start"));
    }

    #[test]
    fn a_card_offers_its_actions_the_first_focused() {
        let state = SharedPrompt::default();
        state.borrow_mut().set_step(PromptStep::Card {
            heading: "+12.4 ms".into(),
            lines: vec![("Converged.".into(), "Text".into())],
            actions: vec![
                ("save".into(), "Save".into()),
                ("again".into(), "Again".into()),
            ],
        });
        let (mut h, seen) = prompt(&state);
        assert!(h.texts().iter().any(|t| t == "Converged."));
        h.key("Enter", "Enter");
        h.tap(PointerKind::Mouse, "Action.again");
        assert_eq!(
            *seen.borrow(),
            [
                (PromptChoice::Action("save".into()), true),
                (PromptChoice::Action("again".into()), true)
            ]
        );
    }

    /// A prompt counting down from `seconds`.
    fn counting(seconds: f32) -> (SharedPrompt, Harness, Seen) {
        let state = SharedPrompt::default();
        state.borrow_mut().set_countdown(Some(seconds));
        let (h, seen) = prompt(&state);
        (state, h, seen)
    }

    fn starts(seen: &Seen) -> Vec<bool> {
        seen.borrow()
            .iter()
            .filter(|(c, _)| *c == PromptChoice::Start)
            .map(|(_, g)| *g)
            .collect()
    }

    #[test]
    fn the_countdown_presses_start_at_zero_outside_a_gesture() {
        let (state, mut h, seen) = counting(3.0);
        // Begun when Start showed (0.3 s ago): 2.7 s left.
        let v = view();
        assert_eq!(v.countdown_state, "running");
        assert!((v.countdown.unwrap() - 2.7).abs() < 0.05, "{v:?}");
        assert!(!h.texts().iter().any(|t| t.starts_with("Starting in")));
        assert!(h.ui.actor("CountdownBar").is_none());
        let initial = h.ui.actor("CountdownFill").unwrap().clip.unwrap();
        h.run(1.0);
        let later = h.ui.actor("CountdownFill").unwrap().clip.unwrap();
        assert!(
            later[2] > initial[2],
            "button background fills left to right"
        );
        assert_eq!(later[0], initial[0]);
        assert!(!h.texts().iter().any(|t| t.starts_with("Starting in")));
        assert!(starts(&seen).is_empty());
        h.run(2.0);
        // Not a gesture: the shell decides what may start now.
        assert_eq!(starts(&seen), [false]);
        assert_eq!(view().countdown_state, "done");
        // Once.
        h.run(4.0);
        assert_eq!(starts(&seen).len(), 1);
        let _ = state;
    }

    #[test]
    fn enter_a_pads_start_or_start_tapped_skip_the_countdown() {
        for how in 0..3 {
            let (_, mut h, seen) = counting(3.0);
            match how {
                0 => drop(h.key("Enter", "Enter")),
                1 => drop(h.pad(MenuAction::Start, true)),
                _ => drop(h.tap(PointerKind::Mouse, "Start")),
            }
            assert_eq!(starts(&seen), [true], "{how}");
            assert_eq!(view().countdown_state, "done");
            h.run(4.0);
            assert_eq!(starts(&seen).len(), 1, "{how}: once");
        }
    }

    #[test]
    fn any_other_press_cancels_the_countdown_and_start_waits() {
        type Press = fn(&mut Harness);
        let presses: [(&str, Press); 6] = [
            ("a letter", |h| drop(h.key("KeyA", "a"))),
            ("a direction", |h| drop(h.key("ArrowDown", "ArrowDown"))),
            ("Tab", |h| drop(h.key("Tab", "Tab"))),
            ("a pad's direction", |h| {
                h.pad(MenuAction::Down, true);
                h.pad(MenuAction::Down, false);
            }),
            ("a click on the background", |h| {
                drop(h.tap_at(PointerKind::Mouse, [40.0, 40.0]))
            }),
            ("a tap on the background", |h| {
                drop(h.tap_at(PointerKind::Touch, [40.0, 40.0]))
            }),
        ];
        for (what, press) in presses {
            let (_, mut h, seen) = counting(3.0);
            press(&mut h);
            assert_eq!(view().countdown_state, "cancelled", "{what}");
            // The press that stopped it started nothing.
            h.run(4.0);
            assert!(starts(&seen).is_empty(), "{what}: {:?}", seen.borrow());
            assert!(
                !h.texts().iter().any(|t| t.starts_with("Press Enter")),
                "{what}"
            );
            assert!(h.ui.actor("CountdownBar").is_none(), "{what}");
            // Start then waits for Start, as without a countdown.
            h.key("Enter", "Enter");
            assert_eq!(starts(&seen), [true], "{what}");
        }
    }

    #[test]
    fn releases_repeats_hover_the_wheel_and_modifiers_leave_the_countdown_running() {
        use crate::event::KeyEvent;
        let (_, mut h, seen) = counting(3.0);
        // The press that chose the song: its release and the OS's repeats.
        h.key_up("Enter", "Enter");
        let mut repeat = KeyEvent::down("Enter", "Enter");
        repeat.repeat = true;
        h.send(UiEvent::Key(repeat));
        let mut repeat = KeyEvent::down("KeyA", "a");
        repeat.repeat = true;
        h.send(UiEvent::Key(repeat));
        h.pad(MenuAction::Start, false);
        // A modifier alone, the mouse moving, the wheel.
        h.key("ShiftLeft", "Shift");
        h.key("MetaLeft", "Meta");
        h.hover([600.0, 300.0]);
        h.hover([640.0, 360.0]);
        h.wheel_lines([640.0, 360.0], [0.0, 3.0]);
        h.run(0.1);
        assert_eq!(view().countdown_state, "running");
        assert!(starts(&seen).is_empty());
        h.run(3.0);
        assert_eq!(starts(&seen), [false]);
    }

    #[test]
    fn back_leaves_while_it_counts_and_no_countdown_was_asked_for() {
        let (_, mut h, seen) = counting(3.0);
        h.key("Escape", "Escape");
        assert_eq!(
            seen.borrow().last().map(|s| &s.0),
            Some(&PromptChoice::Back)
        );
        // None asked for: Start waits.
        let state = SharedPrompt::default();
        let (mut h, seen) = prompt(&state);
        h.run(5.0);
        assert!(starts(&seen).is_empty());
        assert_eq!(view().countdown_state, "off");
        assert!(h.ui.actor("CountdownBar").is_none());
    }

    #[test]
    fn under_reduced_motion_the_countdown_fill_advances_in_steps() {
        let state = SharedPrompt::default();
        state.borrow_mut().set_countdown(Some(3.0));
        let (mut h, seen) = prompt_with(&state, true);
        assert!(!h.texts().iter().any(|t| t.starts_with("Starting in")));
        let initial = h.ui.actor("CountdownFill").unwrap().clip;
        h.run(0.2);
        assert_eq!(h.ui.actor("CountdownFill").unwrap().clip, initial);
        h.run(0.8);
        assert_ne!(h.ui.actor("CountdownFill").unwrap().clip, initial);
        h.run(2.0);
        assert_eq!(starts(&seen), [false]);
    }

    #[test]
    fn the_play_options_are_navigable_and_say_what_changes() {
        ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
        use super::super::prompt_options::PlayOptions;
        let state = SharedPrompt::default();
        let options = Rc::new(RefCell::new(PlayOptions::new(
            crate::state::settings::Settings::default(),
            "dance-single",
            super::super::data::Platform::Web,
        )));
        options
            .borrow_mut()
            .set_noteskins(vec![("infinito".into(), "Infinito".into())], true);
        options.borrow().show(&mut state.borrow_mut());
        state.borrow_mut().set_countdown(Some(3.0));
        // The shell: options applied (this play's), saved when asked.
        let env = Env::new(Rc::new(look::Plain), Rc::new(|s: &str| s.to_string()));
        LAST_ENV.with(|e| *e.borrow_mut() = Some(env.clone()));
        let seen: Seen = Rc::default();
        let act: PromptFn = {
            let (seen, state, options) = (seen.clone(), state.clone(), options.clone());
            Rc::new(move |c, cx: &mut Cx| {
                match &c {
                    PromptChoice::Option { id, index } => {
                        options.borrow_mut().change(id, *index);
                        options.borrow().show(&mut state.borrow_mut());
                    }
                    PromptChoice::SaveOptions => {
                        options.borrow_mut().save();
                        options.borrow().show(&mut state.borrow_mut());
                    }
                    _ => {}
                }
                seen.borrow_mut().push((c, cx.in_gesture()))
            })
        };
        let mut h = Harness::new(super::super::DESIGN, [1280.0, 720.0], 1.0, TextMode::Mirror);
        h.push(PromptScreen::new(env, state.clone(), act));
        h.run(0.3);
        assert_eq!(
            view().options,
            [
                "speed=×2.5",
                "reverse=Off",
                "turn=Off",
                "appearance=Normal",
                "lane_cover=Off",
                "noteskin=Infinito"
            ]
        );
        assert_eq!(h.ui.focused(), Some("Start"));
        // Up from Start: the last row, then up the rows.
        h.key("ArrowUp", "ArrowUp");
        assert_eq!(h.ui.focused(), Some("Opt.noteskin"));
        // Navigating stopped the countdown.
        assert_eq!(view().countdown_state, "cancelled");
        for _ in 0..5 {
            h.key("ArrowUp", "ArrowUp");
        }
        assert_eq!(h.ui.focused(), Some("Opt.speed"));
        h.key("ArrowRight", "ArrowRight");
        h.run(0.05);
        assert_eq!(view().options[0], "speed=×2.75");
        assert!(view().options_differ);
        // ◀ ▶ step a row, a tap on it cycles it.
        h.tap(PointerKind::Mouse, "Opt.reverse.Up");
        h.run(0.05);
        assert_eq!(view().options[1], "reverse=On");
        h.tap(PointerKind::Mouse, "Opt.turn.Label");
        h.run(0.05);
        assert_eq!(view().options[2], "turn=Mirror");
        assert_eq!(h.ui.focused(), Some("Opt.turn"));
        // Not saved: the stored settings are as they were.
        assert_eq!(crate::state::settings::Settings::load().speed, 2.5);
        // Save as default: under the rows, Enter there saves, not starts.
        h.key("ArrowDown", "ArrowDown");
        h.key("ArrowDown", "ArrowDown");
        h.key("ArrowDown", "ArrowDown");
        h.key("ArrowDown", "ArrowDown");
        assert_eq!(h.ui.focused(), Some("SaveOptions"));
        h.key("Enter", "Enter");
        h.run(0.05);
        assert!(view().options_saved && !view().options_differ);
        let s = crate::state::settings::Settings::load();
        assert_eq!((s.speed, s.reverse), (2.75, true));
        assert!(!seen.borrow().iter().any(|(c, _)| *c == PromptChoice::Start));
        assert!(h.texts().iter().any(|t| t == "Saved as default"));
        // Then Start, from where the focus went.
        h.key("ArrowDown", "ArrowDown");
        h.key("Enter", "Enter");
        assert_eq!(
            seen.borrow().last().map(|s| &s.0),
            Some(&PromptChoice::Start)
        );
    }

    #[test]
    fn the_mods_line_says_the_speed_and_what_is_off_its_default() {
        use crate::state::settings::Settings;
        let mut s = Settings {
            speed: 2.5,
            reverse: true,
            ..Settings::default()
        };
        s.transform.turn = ddi_engine::Turn::Mirror;
        assert_eq!(mods_line(&s), "×2.5 · reverse · mirror");
    }
    #[test]
    fn base_survives_loading_and_rebuilds_do_not_restart_the_reveal() {
        let state = SharedPrompt::default();
        state.borrow_mut().set_base(Some("cached:menu".into()));
        let (mut h, _) = prompt(&state);
        assert!(h.ui.actor("Base").is_some());
        assert!(h.ui.actor("Picture").is_none());
        state.borrow_mut().set_picture(Some(PromptPicture {
            filter: None,
            ..picture()
        }));
        h.run(0.15);
        let progress = state.borrow().reveal;
        assert!(progress > 0.0 && progress < 1.0);
        assert_eq!(state.borrow().base.as_deref(), Some("cached:menu"));
        state.borrow_mut().set_notes(vec!["audio ready".into()]);
        h.run(0.1);
        assert!(state.borrow().reveal > progress);
        h.run(0.3);
        assert_eq!(state.borrow().reveal, 1.0);
    }
}
