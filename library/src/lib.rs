//! Song library: turning a pile of files (a folder, a zip, the bundled
//! assets) into songs the game can list and play. No I/O: callers hand in
//! paths and bytes, so the same code serves the web shell, the desktop shell
//! and `xtask`.
//!
//! - [`zip`]: a sans-IO ZIP reader driven by byte-range reads.
//! - [`pack`]: finding songs in a file listing and resolving their assets the
//!   way StepMania does.
//! - [`image`]: image dimensions from file headers, for the banner and
//!   background guess by size.
//! - [`manifest`]: the song list entry shared by bundled and imported songs.
//! - [`backgrounds`]: `#BGCHANGES` as a timed schedule of still images.
//! - [`danoni`]: Dancing☆Onigiri works (pages and dumps) in an import.

pub mod backgrounds;
pub mod danoni;
pub mod image;
pub mod manifest;
pub mod pack;
pub mod zip;
