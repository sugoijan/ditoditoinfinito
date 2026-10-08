//! Importers. Each format module exposes a `parse_*` function taking the
//! file text and returning a [`crate::Song`]. See `docs/research/`.

pub mod danoni;
pub mod dwi;
pub mod msd;
pub mod sm;

use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error("{0}")]
    Malformed(String),
    #[error("unsupported steps type `{0}`")]
    UnsupportedStepsType(String),
}
