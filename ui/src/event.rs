//! What a shell hands the UI: one [`UiEvent`] per platform event, in the
//! platform's own handler (`docs/plans/ui-core.md`, P2). Positions are in
//! CSS (logical) pixels from the surface's top left, as `PointerEvent`
//! and winit's logical positions give them; the UI maps them onto its
//! virtual screen ([`ddi_actor::screen::Viewport::to_virtual`]). Times are
//! the host's, in milliseconds (`event.timeStamp`, or an `Instant` on
//! desktop), and are only compared with each other.
//!
//! Keys carry both the physical code (the DOM's `KeyboardEvent.code`,
//! which winit's `KeyCode` names alike: `"ArrowUp"`, `"KeyA"`, `"Enter"`)
//! and the logical key (`KeyboardEvent.key`, winit's `Key` as the DOM
//! names it: `"a"`, `"/"`, `"Enter"`, `"Process"`): navigation reads the
//! code, shortcuts the key, which moves with the layout (P3).

use crate::nav::MenuAction;

/// One event from the platform.
#[derive(Clone, Debug, PartialEq)]
pub enum UiEvent {
    Pointer(PointerEvent),
    Wheel(WheelEvent),
    Key(KeyEvent),
    Text(TextEvent),
    /// A menu action from the bindings (a controller or a dance pad). The
    /// keyboard sends [`UiEvent::Key`] instead: the UI maps its codes.
    Menu(MenuEvent),
    /// The clipboard's text, in answer to [`crate::UiRequest::Paste`]
    /// (desktop; on the web the hidden textarea pastes by itself).
    Paste(String),
    /// The window or the page gained (`true`) or lost focus (`blur`,
    /// `visibilitychange`, winit's `Focused(false)`): losing it releases
    /// held input and cancels pointer captures (P3).
    Focus(bool),
    /// The surface's size in CSS pixels and the device pixel ratio
    /// (winit's scale factor), at start and whenever either changes.
    Resize {
        size: [f32; 2],
        ratio: f32,
    },
}

/// What points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PointerKind {
    Mouse,
    Touch,
    Pen,
}

/// Where a pointer is in its life (`pointerdown`, `pointermove`,
/// `pointerup`, `pointercancel`, `pointerleave`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerPhase {
    Down,
    Move,
    Up,
    /// The platform took the pointer (a scroll, a system gesture).
    Cancel,
    /// A mouse left the surface (hover ends).
    Leave,
}

/// `buttons` as the DOM counts them.
pub mod buttons {
    pub const PRIMARY: u32 = 1;
    pub const SECONDARY: u32 = 2;
    pub const MIDDLE: u32 = 4;
}

/// A pointer event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerEvent {
    /// `pointerId` (winit: the device and finger, folded into one).
    pub id: u64,
    pub kind: PointerKind,
    pub phase: PointerPhase,
    /// CSS pixels from the surface's top left.
    pub pos: [f32; 2],
    /// The buttons held after the event ([`buttons`]); a touch or a pen
    /// in contact is [`buttons::PRIMARY`].
    pub buttons: u32,
    pub modifiers: Modifiers,
    /// Host time, milliseconds.
    pub time: f64,
}

/// What a wheel delta counts (`WheelEvent.deltaMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeltaMode {
    Pixel,
    Line,
    Page,
}

/// Pixels a line of wheel scrolling is worth. Chrome reports pixels
/// (about 100 a notch); Firefox may report lines, 3 a notch, which this
/// makes 120. Our own convention, as `normalize-wheel` and egui use 40.
pub const LINE_PX: f32 = 40.0;

/// Lines a wheel notch is worth where the platform counts notches
/// (winit's `LineDelta` gives 1.0 a notch): the desktop shell multiplies
/// by this before [`wheel_pixels`].
pub const LINES_PER_NOTCH: f32 = 3.0;

/// A wheel delta in CSS pixels: lines are [`LINE_PX`], pages the
/// surface's size (`page`, CSS pixels) less a line, as browsers page.
pub fn wheel_pixels(delta: [f32; 2], mode: DeltaMode, page: [f32; 2]) -> [f32; 2] {
    match mode {
        DeltaMode::Pixel => delta,
        DeltaMode::Line => [delta[0] * LINE_PX, delta[1] * LINE_PX],
        DeltaMode::Page => [
            delta[0] * (page[0] - LINE_PX).max(LINE_PX),
            delta[1] * (page[1] - LINE_PX).max(LINE_PX),
        ],
    }
}

/// A wheel turned (or a touchpad scrolled) over the surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelEvent {
    /// Where the pointer is, CSS pixels.
    pub pos: [f32; 2],
    /// CSS pixels, already normalised ([`wheel_pixels`]); positive y
    /// scrolls down (the content moves up), as the DOM's `deltaY`.
    pub delta: [f32; 2],
    pub modifiers: Modifiers,
    pub time: f64,
}

/// Modifier keys held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// ⌘ on macOS, the Windows key elsewhere.
    pub meta: bool,
}

impl Modifiers {
    /// The shortcut modifier: ⌘ on macOS, Ctrl elsewhere. Either counts,
    /// so the core needs no notion of the platform.
    pub fn command(&self) -> bool {
        self.ctrl || self.meta
    }

    /// Any modifier but Shift.
    pub fn any_but_shift(&self) -> bool {
        self.ctrl || self.alt || self.meta
    }
}

/// A key pressed or released.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyEvent {
    /// The physical key, as the DOM's `code` names it.
    pub code: String,
    /// The logical key, as the DOM's `key` names it (a character, or a
    /// name such as `"Enter"`; `"Process"` while an IME takes it).
    pub key: String,
    pub down: bool,
    /// The OS's auto-repeat: ignored for navigation, which repeats by
    /// itself ([`crate::nav::REPEAT_DELAY`]).
    pub repeat: bool,
    pub modifiers: Modifiers,
    /// The key is the IME's. The web shell sets it to `isComposing ||
    /// keyCode == 229`: Safari sends the Enter that commits a composition
    /// after `compositionend`, with `isComposing` false but key code 229;
    /// desktop sets it while a preedit is shown.
    pub composing: bool,
    pub time: f64,
}

impl KeyEvent {
    /// A key press with no modifiers (tests and shells).
    pub fn down(code: &str, key: &str) -> KeyEvent {
        KeyEvent {
            code: code.into(),
            key: key.into(),
            down: true,
            repeat: false,
            modifiers: Modifiers::default(),
            composing: false,
            time: 0.0,
        }
    }

    /// The release of [`KeyEvent::down`].
    pub fn up(code: &str, key: &str) -> KeyEvent {
        KeyEvent {
            down: false,
            ..KeyEvent::down(code, key)
        }
    }

    /// The letter a shortcut means: the logical key when it is one ASCII
    /// character, else the physical key's letter (`KeyC` on a Cyrillic
    /// layout is still copy), lowercased.
    pub fn shortcut_char(&self) -> Option<char> {
        let mut chars = self.key.chars();
        if let (Some(c), None) = (chars.next(), chars.next())
            && c.is_ascii_graphic()
        {
            return Some(c.to_ascii_lowercase());
        }
        let letter = self.code.strip_prefix("Key")?;
        let mut chars = letter.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => Some(c.to_ascii_lowercase()),
            _ => None,
        }
    }
}

/// Text from the platform's editor (P5).
#[derive(Clone, Debug, PartialEq)]
pub enum TextEvent {
    /// The web: the hidden textarea's state, re-read on every
    /// composition, input, key and selection event (never inferred from
    /// their order, which differs between browsers).
    Sync(TextSync),
    /// Desktop (winit's `Ime::Preedit`): the composition shown at the
    /// caret, and its cursor as byte offsets into it (`None`: hidden);
    /// empty text ends it.
    Preedit {
        text: String,
        cursor: Option<(usize, usize)>,
    },
    /// Desktop (winit's `Ime::Commit`, or a typed character from
    /// `KeyboardInput`'s `text`): text to insert at the caret. Shells pass
    /// printable text only; control characters (Tab, Enter, Backspace's
    /// `\u{8}`) are dropped here anyway, and so is text left empty.
    Commit(String),
    /// The platform's field lost focus (the textarea's `blur`, winit's
    /// `Ime::Disabled`): typing ends, the text kept
    /// ([`crate::TextChange::Blurred`]).
    Blurred,
}

/// The hidden textarea's state. Offsets are UTF-16 code units, as the DOM
/// counts them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextSync {
    pub text: String,
    /// `[anchor, focus]`: `selectionStart` and `selectionEnd`, swapped when
    /// `selectionDirection` is `"backward"`.
    pub selection: [usize; 2],
    /// The composition's range, while composing: the shell keeps the value
    /// at `compositionstart` and derives it with
    /// [`crate::text_field::composition_range`] (what changed since, or,
    /// when nothing did, as Android's reconversion of a word does, the
    /// last `compositionupdate`'s `data` before the caret).
    pub composition: Option<[usize; 2]>,
}

/// A menu action pressed or released on a controller.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuEvent {
    pub action: MenuAction,
    pub down: bool,
    /// Which controller (the pad's slot): each one's held directions
    /// repeat on their own.
    pub source: u32,
    /// Whether the platform lets it start audio as a user gesture. Pad
    /// presses come from the poll loop, which Chrome and Firefox count as
    /// one and Safari does not (P2); the shell says. Even then it starts
    /// only what [`crate::UiRequest::pad_may_start`] allows.
    pub gesture: bool,
    pub time: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_lines_and_pages_become_pixels() {
        assert_eq!(
            wheel_pixels([0.0, 3.0], DeltaMode::Line, [800.0, 600.0]),
            [0.0, 120.0]
        );
        assert_eq!(
            wheel_pixels([1.0, -1.0], DeltaMode::Page, [800.0, 600.0]),
            [760.0, -560.0]
        );
        assert_eq!(
            wheel_pixels([2.5, 100.0], DeltaMode::Pixel, [800.0, 600.0]),
            [2.5, 100.0]
        );
    }

    #[test]
    fn shortcuts_fall_back_to_the_physical_letter() {
        let mut k = KeyEvent::down("KeyC", "с");
        assert_eq!(k.shortcut_char(), Some('c'));
        k.key = "C".into();
        assert_eq!(k.shortcut_char(), Some('c'));
        assert_eq!(KeyEvent::down("Slash", "/").shortcut_char(), Some('/'));
        assert_eq!(KeyEvent::down("Enter", "Enter").shortcut_char(), None);
    }
}
