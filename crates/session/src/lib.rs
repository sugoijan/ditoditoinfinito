//! One play session for both shells (`docs/plans/desktop.md`, D1 and step
//! 7.1): what a play does, platform-free, over `ddi_platform`'s traits.
//! The web (`crates/web/src/play.rs`) and the desktop run the same code; each
//! shell brings its audio (`AudioBackend`), its inputs ([`play::Inputs`])
//! and its drawing.
//!
//! - [`play`]: the play itself: the engine's `Player`, the bindings, the
//!   cancel gesture, autoplay (also through a test pad), the fail's fade
//!   and hold, a work's `startFrame`/`endFrame` fades, the facts a play is
//!   recorded with.
//! - [`calibration`]: the converging calibration's modes, generated chart
//!   and click track.
//! - [`listen`]: the chart editor's music from a second on, and the
//!   second heard.
//! - [`backgrounds`]: the frame backgrounds are fitted into, the sides
//!   their images are kept at, the refresh rate measured from frames, the
//!   automatic video setting's step.
//! - [`movies`]: a play's background movies: which segments have a
//!   decoder, their tracks, the frames due and what shows (the shell
//!   opens the decoders and makes the textures).
//! - [`stage`]: a Dancing☆Onigiri work's layers' timeline in a play.
//! - [`scripted`]: a song's scripted backgrounds and foregrounds in a
//!   play: their Lua, the commands at each change, their updates within a
//!   budget, what each draws.
//! - [`record`]: what the play history keeps of a play.
//! - [`preview`]: song select's previews' length and fades.
//! - [`sounds`]: the mine sound and the assist clap, made in code.

pub mod backgrounds;
pub mod calibration;
pub mod listen;
pub mod movies;
pub mod play;
pub mod preview;
pub mod record;
pub mod scripted;
pub mod sounds;
pub mod stage;

/// Whose decode of an MP3 `song`'s notes are timed against: StepMania's
/// for a simfile, Chrome's for a Dancing☆Onigiri work (danoniplus plays
/// the browser's own decode; PLAN.md decision 46).
pub fn music_target(song: &ddi_chart::Song) -> ddi_platform::mp3::Target {
    if song.source.format == ddi_chart::SourceFormat::Danoni {
        ddi_platform::mp3::Target::Chromium
    } else {
        ddi_platform::mp3::Target::StepMania
    }
}

/// [`music_target`] from what the library knows of a song: its chart
/// stored as a Dancing☆Onigiri work's.
pub fn music_target_of(entry: &ddi_library::manifest::ManifestEntry) -> ddi_platform::mp3::Target {
    if entry.is_danoni_work() {
        ddi_platform::mp3::Target::Chromium
    } else {
        ddi_platform::mp3::Target::StepMania
    }
}
