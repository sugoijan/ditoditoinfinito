//! The wire contract between the desktop app and a song source plugin
//! (`docs/plans/source-plugins.md`, section 2): a plugin is a separate
//! native program the player installs, started by the app and spoken to
//! over its standard input and output.
//!
//! - [`frame`]: version 1's framing, a length, the JSON's length, the
//!   JSON, and optionally a fetched file's bytes after it; clean EOF only
//!   between frames.
//! - [`message`]: what the app asks ([`AppMessage`]) and what a plugin
//!   answers ([`PluginMessage`]), as values (steps, failure codes,
//!   capabilities), never the app's words.
//! - [`limits`]: every size and count the app enforces, whatever the
//!   plugin.
//! - [`check`]: a plugin's messages and its Hello against those limits and
//!   its manifest; [`text`]: what a plugin's own text may be.
//! - [`manifest`]: `plugin.toml`'s fields (the host parses the TOML).
//!
//! `vectors/v1.json` holds frames as bytes with what they decode to (or
//! why they are refused), for implementations in other languages.
//!
//! Serde only, platform-free: it builds for wasm too, where nothing uses
//! it.

pub mod check;
pub mod frame;
pub mod limits;
pub mod manifest;
pub mod message;
pub mod text;

pub use frame::{Frame, WireError};
pub use manifest::Manifest;
pub use message::*;
pub use text::Text;

/// The protocol's version this crate speaks.
pub const VERSION: u32 = 1;

/// Every version this crate speaks, oldest first (the app's Hello lists
/// them; the plugin answers with the one it chose).
pub const VERSIONS: &[u32] = &[VERSION];
