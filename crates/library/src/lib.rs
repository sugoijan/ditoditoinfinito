//! Song library: turning a pile of files (a folder, a zip, the bundled
//! assets) into songs the game can list and play. No I/O: callers hand in
//! paths and bytes, so the same code serves the web shell, the desktop shell
//! and `xtask`.
//!
//! - [`zip`]: a sans-IO ZIP reader driven by byte-range reads.
//! - [`avi`]: an AVI's video stream and its index, for frame-by-frame decoders.
//! - [`pack`]: finding songs in a file listing and resolving their assets the
//!   way StepMania does.
//! - [`image`]: image dimensions from file headers, for the banner and
//!   background guess by size.
//! - [`legacy`]: simfile text in the code pages of Japanese, Korean and
//!   Chinese packs.
//! - [`manifest`]: the song list entry shared by bundled and imported songs.
//! - [`path`]: logical paths (`/`-separated, relative to a root) and the
//!   one way they become OS paths.
//! - [`scan`]: a song's list entry from its folder, read in place.
//! - [`backgrounds`]: `#BGCHANGES` as a timed schedule of still images.
//! - [`h264`]: H.264 parameters and the FFmpeg module's frame times, for other decoders.
//! - [`danoni`]: Dancing☆Onigiri works (pages and dumps) in an import.
//! - [`video`]: which background-change files are movies and what they are.

pub mod avi;
pub mod backgrounds;
pub(crate) mod bytes;
pub mod danoni;
pub mod h264;
pub mod image;
pub mod legacy;
pub mod manifest;
pub mod pack;
pub mod path;
pub mod scan;
pub mod search;
pub mod suggest;
pub mod video;
pub mod works;
pub mod zip;
