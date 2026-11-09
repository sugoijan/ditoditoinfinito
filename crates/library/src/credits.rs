//! The schema of `credits/credits.json`: what `xtask gen-credits` writes
//! and the Credits screen reads. One declaration, so that the two cannot
//! drift apart; the file's fields are in the order they are declared.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Code or data from another project compiled into the game
/// (`credits/THIRD_PARTY.toml`), with the notice its licence asks to ship.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CodeCredit {
    pub name: String,
    pub what: String,
    /// The files it is in (the generated `CREDITS.md`'s).
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub spdx: String,
    /// The licence's name as shown.
    pub license: String,
    pub holders: Vec<String>,
    pub source: String,
    pub commit: String,
    pub license_url: String,
    pub notice: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SongCredit {
    /// Its folder (the generated `CREDITS.md`'s).
    #[serde(default)]
    pub id: String,
    pub title: String,
    pub artist: String,
    pub source_url: String,
    pub source_path: String,
    pub source_commit: String,
    #[serde(default)]
    pub retrieved: String,
    pub parts: Vec<PartCredit>,
    /// Chart author → playable charts credited to them.
    #[serde(default)]
    pub chart_authors: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub excluded: Vec<String>,
    #[serde(default)]
    pub modifications: Vec<String>,
}

/// A bundled noteskin (`assets/noteskins/<game>/<name>/`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkinCredit {
    pub name: String,
    pub author: String,
    pub source_url: String,
    pub source_path: String,
    pub source_commit: String,
    #[serde(default)]
    pub retrieved: String,
    pub parts: Vec<PartCredit>,
    #[serde(default)]
    pub excluded: Vec<String>,
    #[serde(default)]
    pub modifications: Vec<String>,
}

/// One of our announcers (`assets/announcers/<name>/PROVENANCE.toml`):
/// what made it, its licence, and the recordings its voice learned from
/// where they ask for credit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnnouncerCredit {
    pub name: String,
    pub author: String,
    pub language: String,
    /// "Kokoro-82M (hexgrad, Apache-2.0), voice bm_george".
    pub made_with: String,
    pub model_url: String,
    pub revision: String,
    #[serde(default)]
    pub made: String,
    pub parts: Vec<PartCredit>,
    #[serde(default)]
    pub trained_on: Vec<TrainedOnCredit>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrainedOnCredit {
    pub voice: String,
    pub what: String,
    pub url: String,
    pub spdx: String,
    pub holders: Vec<String>,
    pub license_url: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PartCredit {
    pub part: String,
    pub spdx: String,
    pub holders: Vec<String>,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FontCredit {
    pub file: String,
    pub spdx: String,
    pub holders: String,
    pub source: String,
    #[serde(default)]
    pub modified: String,
}

/// The video decoder module: FFmpeg and what is linked with it
/// (`video/build.sh` serves the licence texts next to the module).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VideoCredit {
    pub name: String,
    pub version: String,
    pub spdx: String,
    pub homepage: String,
    pub source_url: String,
    /// The release tarball at FFmpeg, and the same file served next to the
    /// module (`xtask stage-video`), as a path relative to the site.
    pub tarball_url: String,
    pub tarball: String,
    pub tarball_sha256: String,
    /// The flags after the toolchain ones, the decoders last.
    pub configure: Vec<String>,
    /// The desktop helper's (`video/build-native.sh`), the decoders last.
    #[serde(default)]
    pub native_configure: Vec<String>,
    pub components: Vec<ComponentCredit>,
    /// Served next to the module, when the build has it.
    pub notices: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComponentCredit {
    pub name: String,
    pub spdx: String,
    pub url: String,
}
