//! Our own UI core (`docs/plans/ui-core.md`, step 3): screens drawn with
//! the game's actors, on the web and on desktop alike. Platform-free: a
//! shell turns its events into [`UiEvent`]s, calls [`Ui::handle`] inside
//! its own handler and carries out the [`UiRequest`]s it gets back (those
//! that need a user gesture at once, P2); each frame it calls
//! [`Ui::update`] and [`Ui::draw`], and stops its frames while
//! [`Ui::needs_redraw`] is false (P13).
//!
//! - [`event`], [`request`]: the vocabulary between the core and a shell.
//! - [`hit`]: what is under a point, through actors' transforms and clips.
//! - [`gesture`]: taps, long presses, drags, flicks.
//! - [`nav`]: menu actions, held directions repeated as StepMania does,
//!   focus by neighbours and by place (P3).
//! - [`message`]: broadcast messages played as `<Name>MessageCommand` (P7).
//! - [`text_field`]: a single-line field, mirrored from the web's textarea
//!   or edited by grapheme on desktop (P5).
//! - [`screen`]: the [`Screen`] trait and its [`Cx`].
//! - [`state`]: settings, the song list's place and the libraries' views,
//!   shared by the shells (P12).
//! - [`testing`]: a headless harness that scripts events and frames.
//! - [`dev`]: the developer screen both shells run.

pub mod dev;
pub mod dev_widgets;
pub mod event;
pub mod gesture;
pub mod graph;
pub mod hit;
#[cfg(feature = "layout")]
pub mod layout;
pub mod look;
pub mod message;
pub mod nav;
pub mod request;
pub mod screen;
pub mod screens;
pub mod state;
pub mod testing;
pub mod text_field;
pub mod textures;
mod ui;
pub mod widgets;

pub use event::{KeyEvent, Modifiers, PointerEvent, PointerKind, PointerPhase, UiEvent};
pub use gesture::Gesture;
pub use hit::Target;
pub use message::{Message, Param};
pub use nav::{Focusable, MenuAction};
pub use request::{InputHints, UiRequest};
pub use screen::{Cx, Handled, Input, Screen, TextChange};
pub use text_field::{TextField, TextMode};
pub use ui::{ACTIVATION_WINDOW, DESIGN_MESSAGE, RING_COLOR};
pub use ui::{LayerInfo, Outcome, Ui};
