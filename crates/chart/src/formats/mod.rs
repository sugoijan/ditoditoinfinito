//! Importers. Each format module exposes a `parse_*` function taking the
//! file text and returning a [`crate::Song`]. See `docs/research/`.
//! `sm_write` writes a song back as `.ssc` or `.sm` (the chart editor's).

pub mod danoni;
pub mod dwi;
pub mod ffr;
pub mod lrc;
pub mod msd;
pub mod sm;
pub mod sm_write;

use thiserror::Error;

use crate::SourceFormat;

/// The synthetic tempo a frame-based format's charts are put on (their
/// ticks are fractions of a second, not of a beat), or none: a song's
/// tempo shown and its x-mods are its own reference's
/// (`ddi_engine::scroll::speed_for_song`), never this one.
pub fn synthetic_bpm(format: &SourceFormat) -> Option<f64> {
    match format {
        SourceFormat::Danoni => Some(danoni::SYNTHETIC_BPM),
        SourceFormat::Ffr => Some(ffr::SYNTHETIC_BPM),
        _ => None,
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error("{0}")]
    Malformed(String),
    #[error("unsupported steps type `{0}`")]
    UnsupportedStepsType(String),
}
