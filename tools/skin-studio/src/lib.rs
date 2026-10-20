//! SkinCraft's shared native editor host and optional PhotoCraft noteskin panel.
#![forbid(unsafe_code)]

mod files;
mod panel;
mod project;
mod text;
mod workspace;
pub use panel::Panel;

#[cfg(feature = "skincraft")]
pub use panel::SkinCraft;

#[cfg(feature = "skincraft")]
pub mod mcp;
