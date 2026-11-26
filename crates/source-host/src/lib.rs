//! The desktop's host for song source plugins
//! (`docs/plans/source-plugins.md`, stage 1). A plugin is a native program
//! a player installs in the data folder's `plugins/<id>/`: trusted, with
//! the player's own access, doing its own network and file I/O, and never
//! confined by the game. What the host owes the game whatever the plugin:
//!
//! - a plugin that hangs, crashes, floods or sends garbage never takes
//!   the game down or stalls a frame: it runs as a separate process, read
//!   and written on threads of its own ([`supervisor`]), every message
//!   bounded ([`ddi_source_protocol::limits`]), stopped and reaped when it
//!   goes silent, overstays or breaks the protocol, and off for the run
//!   after three such ends;
//! - everything it returns is song data from outside, read by the
//!   library's own bounded scanner and zip reader ([`assemble`]);
//! - songs fetched without importing are kept in memory, within stated
//!   limits, else in the cache folder's `sources/` ([`store`]).
//!
//! Nothing runs unless a plugin is installed and switched on
//! ([`switches`]): a player with none pays a missing folder's check. The
//! sources built into the app ([`builtin`], stage 2: "This computer")
//! are asked the same way and answer the same events, in process. The
//! crate is empty on wasm.

#![cfg(not(target_arch = "wasm32"))]

pub mod assemble;
pub mod builtin;
pub mod host;
pub mod installed;
mod process;
pub mod store;
pub mod supervisor;
pub mod switches;

pub use ddi_source_protocol as protocol;
pub use builtin::Builtin;
pub use host::{Event, Failure, Host, HostConfig, HostFailure, Request, Stop, Timeouts};
pub use installed::{Installed, discover};
pub use store::{FetchedSong, Hold, ItemKey, SharedStore, Store, StoreLimits};
