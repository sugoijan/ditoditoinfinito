//! Screens: what the player sees and drives, one at a time on a stack,
//! with popups and dialogs over them.
//!
//! A screen builds its actor tree once ([`Screen::build`]); the [`crate::Ui`]
//! owns the tree, updates and draws it, and hands it back with every
//! input, frame and message, so the screen's code and its actors' commands
//! work on the same retained scene (as a StepMania screen is an
//! `ActorFrame` its code holds). Screens own their navigation (P3): input
//! goes to the screen first; what it leaves ([`Handled::No`]) gets the
//! default behaviour (focus moves, Start activates, Back leaves).
//!
//! Everything a screen wants done goes through its [`Cx`]: requests to
//! the shell, screens pushed or left, messages, focus, typing. It is
//! applied when the call returns, still inside the platform's event
//! handler when the call came from one.
//!
//! Widgets (step 4) are named actors plus code that reads [`Input`]s for
//! their names ([`crate::hit::Target::within`]); a theme (step 5) builds
//! the actors.

use ddi_actor::Actor;
use ddi_actor::screen::Viewport;

use crate::event::{KeyEvent, PointerKind};
use crate::gesture::{Gesture, WheelKind};
use crate::hit::Target;
use crate::message::Message;
use crate::nav::{Focusable, MenuAction};
use crate::request::{InputHints, UiRequest};
use crate::text_field::TextField;

/// Whether a screen took an input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handled {
    Yes,
    No,
}

/// What changed in the field being typed into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextChange {
    /// The text (or the composition) changed.
    Edited,
    /// Only the caret or the selection.
    Moved,
    /// Enter, while not composing.
    Submit,
    /// Escape, while not composing.
    Cancel,
    /// The platform's field lost focus, or a press landed outside the
    /// field: typing has ended, the text is kept.
    Blurred,
}

/// An input for a screen. Positions, deltas and velocities are virtual
/// pixels (the screen's).
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    /// A menu button, from a key, a controller or a pad; `repeat` when it
    /// is our repeat of a held direction.
    Menu { action: MenuAction, repeat: bool },
    /// A key no menu action claimed (shortcuts: read
    /// [`KeyEvent::key`]); while typing, the keys the field leaves (Up,
    /// Down, Tab, Page Up and Down). Tab while typing is always consumed
    /// (it would take focus out of the platform's field) and comes here
    /// for the screen to use or ignore.
    Key(KeyEvent),
    /// A pointer's gesture on `target` (captured at the press: the rest of
    /// the gesture goes to it wherever the pointer goes); `None` where
    /// nothing named is.
    Pointer {
        target: Option<Target>,
        gesture: Gesture,
        pos: [f32; 2],
        kind: PointerKind,
    },
    /// A mouse moved onto another target (or off every one).
    Hover { target: Option<Target> },
    /// A wheel over `target`; positive y scrolls down. `kind` says whether
    /// it is a wheel's notch (a step) or part of a trackpad's stream (one
    /// for one), as [`crate::gesture::WheelStream`] tells them apart.
    Wheel {
        target: Option<Target>,
        delta: [f32; 2],
        pos: [f32; 2],
        kind: WheelKind,
    },
    /// Start on the focused actor, or a tap on a focusable one.
    Activate { name: String },
    /// Focus moved (it plays `LoseFocus` and `GainFocus` on the actors).
    Focus {
        from: Option<String>,
        to: Option<String>,
    },
    /// The field being typed into ([`Cx::begin_text`]).
    Text {
        name: String,
        change: TextChange,
        field: TextField,
    },
    /// A press outside a modal overlay (it may close itself).
    Outside,
    /// A key or a controller's control pressed or released, as it is, to
    /// a screen that asked for them ([`Screen::raw_input`]): keys by their
    /// code (no repeats, none with Ctrl, Alt or Meta, no function keys),
    /// controllers' as the shell reads them.
    Raw(ddi_platform::RawInput),
    /// A request that needs a user gesture was made outside one (from a
    /// frame, a message, a long press, a pad press the browser does not
    /// count) and was refused: show a prompt, and ask again from the next
    /// activation (a tap, a key). Never sent where gestures are not
    /// required (desktop).
    NeedsGesture(UiRequest),
}

/// A screen.
pub trait Screen {
    /// For debugging and tests.
    fn name(&self) -> &str {
        ""
    }

    /// The actor tree, built once when the screen is pushed: its root and
    /// what is inside. Children named `In`, `Out` and `Cancel` are its
    /// transition layers: `StartTransitioning` is played on them as the
    /// screen comes in, leaves, or is backed out of (StepMania's
    /// `ScreenWithMenuElements`); the root plays `On`, `Off` or `Cancel`.
    fn build(&mut self, cx: &mut Cx) -> Actor;

    fn input(&mut self, _input: &Input, _root: &mut Actor, _cx: &mut Cx) -> Handled {
        Handled::No
    }

    /// A frame of `dt` seconds, after the actors' own update.
    fn update(&mut self, _dt: f32, _root: &mut Actor, _cx: &mut Cx) {}

    /// A message broadcast, after the actors played its command.
    fn message(&mut self, _message: &Message, _root: &mut Actor, _cx: &mut Cx) {}

    /// Whether the screen moves things itself (a list's inertia, not
    /// tweens): frames keep coming while it does (P13).
    fn animating(&self) -> bool {
        false
    }

    /// The named actors focus moves between, in order (the first is
    /// focused when the screen comes in), with any explicit neighbours.
    fn focusables(&self) -> Vec<Focusable> {
        Vec::new()
    }

    /// Whether, on top, it takes keys and controllers' controls as they
    /// are ([`Input::Raw`]) instead of as menu actions: binding them.
    /// Pointers still come as gestures.
    fn raw_input(&self) -> bool {
        false
    }

    /// The chrome's ([`crate::Ui::push_chrome`]): how much of the top of
    /// the screen it takes (virtual pixels), which every other layer is
    /// laid out below.
    fn reserve(&self) -> f32 {
        0.0
    }

    /// The chrome's: whether a pointer on `target` goes on to what is
    /// under it (a line that only informs, such as the devices' toast,
    /// must not swallow taps on the screen's footer).
    fn lets_through(&self, target: &crate::hit::Target) -> bool {
        let _ = target;
        false
    }
}

/// What a screen asks for while handling a call.
pub(crate) enum Op {
    Request(UiRequest),
    Push(Box<dyn Screen>),
    Replace(Box<dyn Screen>),
    Overlay {
        screen: Box<dyn Screen>,
        modal: bool,
    },
    Close {
        cancel: bool,
    },
    Broadcast(Message),
    Focus(String),
    BeginText {
        name: String,
        text: String,
        hints: InputHints,
    },
    SetText(String),
    EndText,
    EnterChrome,
}

/// A screen's way to the [`crate::Ui`] during a call.
pub struct Cx {
    pub(crate) ops: Vec<Op>,
    textures: std::rc::Rc<std::cell::RefCell<crate::textures::TextureTable>>,
    gesture: bool,
    viewport: Viewport,
    time: f64,
    focused: Option<String>,
    /// The menu action being handled came from a keyboard key.
    pub(crate) menu_key: bool,
    /// ... or from a controller.
    pub(crate) menu_pad: bool,
    /// The last input was a key or a pad's (not a pointer).
    pub(crate) by_keys: bool,
}

impl Cx {
    pub(crate) fn new(
        gesture: bool,
        viewport: Viewport,
        time: f64,
        focused: Option<String>,
        textures: std::rc::Rc<std::cell::RefCell<crate::textures::TextureTable>>,
    ) -> Cx {
        Cx {
            ops: Vec::new(),
            textures,
            gesture,
            viewport,
            time,
            focused,
            menu_key: false,
            menu_pad: false,
            by_keys: false,
        }
    }

    /// Whether the player last used keys or a pad (the focus ring shows),
    /// not a pointer: key hints are worth showing then.
    pub fn by_keys(&self) -> bool {
        self.by_keys
    }

    /// Whether the menu action being handled came from the keyboard (Tab
    /// is the menu's Select; a pad's Y is too).
    pub fn menu_from_keyboard(&self) -> bool {
        self.menu_key
    }

    /// Whether the menu action being handled (and the activation it
    /// made) came from a controller: on the web its press may count as a
    /// gesture yet start no audio (WebKit; a context a poll made may stay
    /// suspended).
    pub fn menu_from_pad(&self) -> bool {
        self.menu_pad
    }

    /// Whether this call comes from a user gesture: requests that need one
    /// ([`UiRequest::needs_gesture`]) are carried out now; outside one they
    /// are refused ([`Input::NeedsGesture`]).
    pub fn in_gesture(&self) -> bool {
        self.gesture
    }

    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    /// Seconds of updates since the UI started.
    pub fn time(&self) -> f64 {
        self.time
    }

    /// The name of this screen's focused actor.
    pub fn focused(&self) -> Option<&str> {
        self.focused.as_deref()
    }

    /// The actor texture id of the image at `source` drawn at most `side`
    /// virtual pixels long ([`crate::textures`]).
    pub fn texture(&mut self, source: &str, side: f32) -> u32 {
        self.textures.borrow_mut().id(source, side)
    }

    /// A cached Gaussian-blurred copy, decoded at most `side` physical
    /// pixels long, independent of the display scale. Sharp uses stay sharp.
    pub fn blurred_texture(&mut self, source: &str, side: f32) -> u32 {
        self.textures.borrow_mut().blurred(source, side)
    }

    pub fn request(&mut self, r: UiRequest) {
        self.ops.push(Op::Request(r));
    }

    /// Pushes a screen over this one (this one stays, under it).
    pub fn push(&mut self, screen: impl Screen + 'static) {
        self.ops.push(Op::Push(Box::new(screen)));
    }

    /// This screen leaves (its `Off` and `Out`), then `screen` comes in.
    pub fn replace(&mut self, screen: impl Screen + 'static) {
        self.ops.push(Op::Replace(Box::new(screen)));
    }

    /// Opens a popup (not modal: input it leaves goes on to the screen
    /// under it) or a dialog (modal: input stays inside, focus too).
    pub fn overlay(&mut self, screen: impl Screen + 'static, modal: bool) {
        self.ops.push(Op::Overlay {
            screen: Box::new(screen),
            modal,
        });
    }

    /// This screen or overlay leaves, with its `Off` and `Out` (going on)
    /// or, with `cancel`, its `Cancel` (backing out).
    pub fn close(&mut self, cancel: bool) {
        self.ops.push(Op::Close { cancel });
    }

    /// Broadcasts a message to every screen shown, once this call returns.
    pub fn broadcast(&mut self, message: Message) {
        self.ops.push(Op::Broadcast(message));
    }

    /// Focuses this screen's actor `name`.
    pub fn focus(&mut self, name: impl Into<String>) {
        self.ops.push(Op::Focus(name.into()));
    }

    /// The menu's focus goes up past this screen's top: to the app's
    /// chrome (its notices), when it has something to focus. The
    /// screen's own focus stays where it is and is given back on Down,
    /// Back or F2. A screen calls it on a fresh Up at its top, instead of
    /// letting the menu's default move the focus to whatever lies above.
    pub fn enter_chrome(&mut self) {
        self.ops.push(Op::EnterChrome);
    }

    /// Starts typing into the text actor `name` of this screen, holding
    /// `text`: the shell is asked to start its text input (inside the
    /// gesture: a phone's keyboard opens only then), and the UI shows the
    /// field's text, caret, selection and composition on that actor.
    pub fn begin_text(&mut self, name: impl Into<String>, text: &str, hints: InputHints) {
        self.ops.push(Op::BeginText {
            name: name.into(),
            text: text.into(),
            hints,
        });
    }

    /// Replaces the text being typed (an autocomplete picked, cleared).
    pub fn set_text(&mut self, text: &str) {
        self.ops.push(Op::SetText(text.into()));
    }

    /// Stops typing.
    pub fn end_text(&mut self) {
        self.ops.push(Op::EndText);
    }
}
