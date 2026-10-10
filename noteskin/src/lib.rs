//! StepMania noteskins for the dance layouts: where a skin's files are
//! (its fallback chain, `GetPath`, redirects), what its `metrics.ini` and
//! file names say, and what each element is. Pure: a skin is given as file
//! names plus the text of its `metrics.ini` and `.redir` files; images are
//! the shell's.
//!
//! Follows StepMania 5.1 (`5_1-new` @ 825467b); `docs/research/noteskins.md`
//! has the sources, `docs/plans/noteskins.md` the steps. A skin's own Lua
//! does not run yet: what needs it is reported and drawn built in.

pub mod actor;
pub mod build;
pub mod builtin;
pub mod commands;
pub mod hints;
pub mod import;
pub mod ini;
pub mod load;
pub mod metrics;
pub mod notes;
pub mod skin;

pub use load::{Button, Resolved, Source};
pub use metrics::{NoteColorType, NoteDisplay, Part, PartMetrics};
pub use skin::{Dir, FileRef, Folder, LoadError, PathError, Skin};
