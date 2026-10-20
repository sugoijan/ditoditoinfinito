//! StepMania noteskins for the dance layouts (and Dancing☆Onigiri's,
//! `load::LaneButton`): where a skin's files are
//! (its fallback chain, `GetPath`, redirects), what its `metrics.ini` and
//! file names say, and what each element is. Pure: a skin is given as file
//! names plus the text of its `metrics.ini` and `.redir` files; images are
//! the shell's.
//!
//! Follows StepMania 5.1 (`5_1-new` @ 825467b); `docs/research/noteskins.md`
//! has the sources, `docs/plans/noteskins.md` the steps. A skin's own Lua
//! runs at load in a sandboxed VM ([`lua`], `docs/plans/lua.md`); without
//! it, what needs Lua is reported and drawn built in. Skins may also use
//! SVG images, our extension ([`svg`]).

pub mod build;
pub mod builtin;
pub mod bundled;
pub mod hints;
pub mod import;
pub mod ini;
pub mod legacy;
pub mod load;
pub mod lua;
pub mod metrics;
pub mod model;
pub mod notes;
pub mod skin;
pub mod svg;

pub use load::{Button, LaneButton, Resolved, Source};
pub use metrics::{NoteColorType, NoteDisplay, Part, PartMetrics};
pub use skin::{Dir, FileRef, Folder, LoadError, PathError, Skin};
