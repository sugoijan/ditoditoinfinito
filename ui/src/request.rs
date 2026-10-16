//! What the UI asks of its shell. [`crate::Ui::handle`] returns requests
//! for the shell to carry out before its event handler returns: browsers
//! (Safari above all) refuse to start sound, open a picker, focus a field
//! for a phone's keyboard or open a window once the gesture's handler is
//! over (`docs/plans/ui-core.md`, P2; `AGENTS.md`). Where gestures are
//! required, a request that needs one made outside one (on a frame, from
//! a long press, a message) is refused and its screen told
//! ([`crate::Input::NeedsGesture`]); the others made outside an event are
//! queued for [`crate::Ui::take_requests`].

/// The kind of text a field takes (`inputmode`; a phone's keyboard).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InputMode {
    #[default]
    Text,
    Search,
    Numeric,
    Decimal,
    Url,
    Email,
}

/// What Enter does (`enterkeyhint`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EnterKey {
    #[default]
    Enter,
    Done,
    Go,
    Next,
    Search,
    Send,
}

/// How a field wants to be typed into.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputHints {
    pub mode: InputMode,
    pub enter: EnterKey,
    /// In characters (Unicode scalar values); `None`: no limit.
    pub max_chars: Option<usize>,
}

/// A rectangle in CSS pixels: `[x, y, width, height]`, as
/// `set_ime_cursor_area` and a textarea's style take it.
pub type CssRect = [f32; 4];

/// The pointer's look over the surface (CSS `cursor`, winit's
/// `CursorIcon`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cursor {
    #[default]
    Default,
    Pointer,
    Text,
    Grab,
    Grabbing,
}

/// A request to the shell.
#[derive(Clone, Debug, PartialEq)]
pub enum UiRequest {
    /// Typing begins: the web focuses its hidden textarea at `caret` with
    /// this value and selection (UTF-16 offsets, `[anchor, focus]`) and
    /// these hints; desktop calls `set_ime_allowed(true)` and
    /// `set_ime_cursor_area`.
    StartTextInput {
        caret: CssRect,
        text: String,
        selection: [usize; 2],
        hints: InputHints,
    },
    StopTextInput,
    /// The caret moved: the textarea or the IME's candidate window
    /// follows it.
    SetImeCursorArea(CssRect),
    /// The field's text was changed by the UI, not by typing (cleared,
    /// an autocomplete picked): the web sets the textarea's value and
    /// selection (UTF-16). Desktop has nothing to do.
    SetText {
        text: String,
        selection: [usize; 2],
    },
    /// Open the file picker, for a folder or for files.
    PickFiles {
        folder: bool,
    },
    /// Open a link in a new tab or the system's browser.
    OpenUrl(String),
    /// Go to a route (`#/play?song=…`).
    Navigate(String),
    /// Make (or resume) the audio context. Placeholder until the screens
    /// that need it move over.
    StartAudio,
    /// Start a song's preview. Placeholder.
    PlayPreview {
        song: String,
    },
    StopPreview,
    /// Draw a frame (the UI changed outside a frame).
    Redraw,
    SetCursor(Cursor),
    /// Store a value (settings, the list's place). Placeholder until the
    /// state moves out of the app (P12).
    Persist {
        key: String,
        value: String,
    },
    /// Put text on the clipboard.
    CopyText(String),
    /// Read the clipboard and answer with [`crate::UiEvent::Paste`]
    /// (desktop: the web's textarea pastes by itself).
    Paste,
    /// Where the options screen shows the field's preview, in the
    /// surface's physical pixels `[x, y, w, h]`, or `None` when it shows
    /// none (hidden, or the screen gone): the shell draws the demo chart
    /// into the texture named
    /// [`crate::screens::options::FIELD_PREVIEW`] at that size. Sent when
    /// it changes.
    FieldPreview(Option<[f32; 4]>),
}

impl UiRequest {
    /// Of the requests that need a gesture, those a controller's press may
    /// start where the platform counts it as one (`MenuEvent::gesture`):
    /// audio (Chrome and Firefox let a pad press start it; the play screen
    /// relies on that). Opening a window, a picker, the keyboard or the
    /// clipboard needs transient activation (a key, a click, a touch's
    /// release), which a pad press never gives.
    pub fn pad_may_start(&self) -> bool {
        matches!(
            self,
            UiRequest::StartAudio | UiRequest::PlayPreview { .. } | UiRequest::StopPreview
        )
    }

    /// Whether the shell must carry it out inside a user gesture's
    /// handler: creating or resuming an `AudioContext` and starting media,
    /// clicking a hidden file input, focusing the textarea (iOS raises the
    /// keyboard only then), opening a window, writing the clipboard.
    pub fn needs_gesture(&self) -> bool {
        matches!(
            self,
            UiRequest::StartTextInput { .. }
                | UiRequest::PickFiles { .. }
                | UiRequest::OpenUrl(_)
                | UiRequest::StartAudio
                | UiRequest::PlayPreview { .. }
                | UiRequest::CopyText(_)
                | UiRequest::Paste
        )
    }
}
