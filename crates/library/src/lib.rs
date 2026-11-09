//! Song library: turning a pile of files (a folder, a zip, the bundled
//! assets) into songs the game can list and play. No I/O: callers hand in
//! paths and bytes, so the same code serves the web shell, the desktop shell
//! and `xtask`.
//!
//! - [`zip`]: a sans-IO ZIP reader driven by byte-range reads, and a
//!   writer of stored entries.
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
//! - [`credits`]: the schema of `credits/credits.json`, written by `xtask
//!   gen-credits` and read by the Credits screen.
//! - [`h264`]: H.264 parameters and the FFmpeg module's frame times, for other decoders.
//! - [`danoni`]: Dancing☆Onigiri works (pages and dumps) in an import.
//! - [`video`]: which background-change files are movies and what they are.
//! - [`swf`]: an FFR level's chart and music from its SWF; [`ffr`]: levels
//!   found among files, read into songs.
//! - [`edited`]: a stored song's entry after the chart editor saved it.
//! - [`report`]: what an import reports (why things were left out, what
//!   imported songs lack, what runs), as values the screens word.
//! - [`tags`]: a music file's title and artist, for a new song.
//! - [`announcer`]: the lines announcers speak, the announcers found in
//!   an import, and the announcer studio's drafts.
//! - [`wav`]: 16-bit PCM WAV files written.
//! - [`fft`]: a Fourier transform; [`music`]: the music's analysis a
//!   generative background follows; [`generative`]: those backgrounds'
//!   looks, as the design study chose them.
//! - [`date`]: calendar dates from day counts.

pub mod announcer;
pub mod avi;
pub mod backgrounds;
pub(crate) mod bytes;
pub mod credits;
pub mod danoni;
pub mod date;
pub mod edited;
pub mod ffr;
pub mod fft;
pub mod generative;
pub mod h264;
pub mod image;
pub mod legacy;
pub mod manifest;
pub mod music;
pub mod pack;
pub mod path;
pub mod report;
pub mod scan;
pub mod search;
pub mod share;
pub mod suggest;
pub mod swf;
pub mod tags;
pub mod video;
pub mod wav;
pub mod works;
pub mod zip;
