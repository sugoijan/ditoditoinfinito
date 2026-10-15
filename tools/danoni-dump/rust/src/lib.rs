//! DanOni song preparation with optional native acquisition and filesystem adapters.
//!
//! Exact source data is borrowed, never rewritten. Musical fitting and source
//! extraction remain separate producers of the inputs accepted here.

#[cfg(feature = "native")]
pub mod acquisition;
pub mod artwork;
pub mod assets;
pub mod audio;
#[cfg(feature = "native")]
pub mod collection;
#[cfg(feature = "native")]
pub mod dependencies;
pub mod extraction;
pub mod import_host;
#[cfg(feature = "native")]
pub mod import_job;
#[cfg(feature = "native")]
pub mod managed_store;
pub mod model;
mod mpeg;
#[cfg(feature = "native")]
pub mod native;
#[cfg(feature = "native")]
pub mod network;
#[cfg(feature = "native")]
pub mod orchestration;
pub mod package;
pub mod policy;
pub mod preparation;
pub mod prepared_import;
pub mod preview;
#[cfg(feature = "native")]
pub mod profiles;
pub mod reviews;
pub mod ssc;
#[cfg(feature = "native")]
pub mod supervisor;
pub mod swf;
pub mod tempo;
pub mod timing;
#[cfg(feature = "native")]
pub mod transaction;
#[cfg(feature = "native")]
pub mod url_native;
#[cfg(feature = "native")]
pub mod url_prepare;
pub mod work;
#[cfg(feature = "native")]
pub mod worker_build;

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

#[cfg(all(test, feature = "native"))]
extern crate self as ddi_danoni_export;
#[cfg(all(test, feature = "native"))]
#[path = "../tests/support/import_fixture.rs"]
mod import_fixture;
