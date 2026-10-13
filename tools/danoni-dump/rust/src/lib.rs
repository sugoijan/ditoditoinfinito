//! Offline DanOni chart/song preparation with an optional filesystem adapter.
//!
//! Exact source data is borrowed, never rewritten. Musical fitting and source
//! extraction remain separate producers of the inputs accepted here.

pub mod assets;
pub mod model;
#[cfg(feature = "native")]
pub mod native;
pub mod policy;
pub mod preview;
pub mod reviews;
pub mod ssc;
pub mod timing;
pub mod work;

pub use model::{Chart, CompileRequest, PreparationOptions, PreparedChart};
pub use ssc::compile_chart;

/// A rejected preparation with a stable, machine-readable reason.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Error {
    pub code: &'static str,
    pub message: String,
}

impl Error {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
