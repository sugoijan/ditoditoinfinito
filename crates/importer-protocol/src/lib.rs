//! Bounded wire messages for the desktop's importer child. Stdout is exclusively
//! framed JSON, never human-readable logs. Anonymous pipes are owned by the host.
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    path::PathBuf,
};

pub const VERSION: u32 = 1;
/// A single admitted many-chart metadata entry fits the SDK's JSON bound.
/// Snapshots are streamed one song per frame; PCM/evidence never cross this pipe.
pub const MAX_FRAME_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Paths {
    pub cache: PathBuf,
    pub staging: PathBuf,
    pub library: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capabilities {
    pub url_import: bool,
    pub offline: bool,
    pub origin_approval: bool,
}

/// What went wrong: `code` says what (an identifier the app words in the
/// player's language where it knows it), `message` the importer's or its
/// SDK's own technical words (English, shown as data).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Failure {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Installed,
    Unchanged,
    Cancelled,
    Failed,
    NeedsRecovery,
}

/// What an import is doing (`Event::Progress`): a value, never words; the
/// app says it in the player's language.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    /// Opening the work, before anything of it is known.
    OpeningWork,
    /// Downloading the work's page and files.
    Downloading,
    /// Reading the work's chart data from its files.
    ReadingCharts,
    /// Checking the charts read from the work.
    CheckingCharts,
    /// Converting the songs' music.
    ConvertingMusic,
    /// Mapping the charts' timing.
    MappingTiming,
    /// Making the songs' pictures.
    MakingArtwork,
    /// Preparing the songs.
    PreparingSongs,
    /// Finishing the preparation.
    FinishingPreparation,
    /// Saving the prepared songs.
    SavingPrepared,
    /// Checking the prepared import's records.
    CheckingImport,
    /// Checking the prepared songs' files.
    CheckingFiles,
    /// Checking the converted charts against the work's.
    VerifyingCharts,
    /// Adding the songs' details for the library.
    AddingDetails,
    /// Finishing the checks.
    FinishingChecks,
    /// Checking the prepared songs, before a step of it is known.
    CheckingPrepared,
    /// Comparing with what was imported before.
    CheckingExisting,
    /// Beginning the library's update.
    PreparingUpdate,
    /// Saving the import's records.
    SavingDetails,
    /// Adding the songs to the library.
    AddingSongs,
    /// Committing the import.
    FinishingImport,
}

/// An installed import's songs.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Counts {
    pub added: u64,
    pub replaced: u64,
    pub skipped: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Request {
    Hello {
        version: u32,
    },
    Initialize {
        paths: Paths,
        origins: BTreeMap<String, String>,
    },
    Import {
        id: u64,
        url: String,
        offline: bool,
    },
    Cancel {
        id: u64,
    },
    OriginDecision {
        id: u64,
        origin: String,
        decision: Option<String>,
    },
    Shutdown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Event {
    Hello {
        version: u32,
        capabilities: Capabilities,
    },
    Initialized {
        ready: bool,
        error: Option<Failure>,
    },
    Progress {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        progress: Option<(u64, u64)>,
        id: Option<u64>,
        stage: Stage,
    },
    OriginRequired {
        id: u64,
        origin: String,
    },
    SnapshotBegin {
        id: Option<u64>,
        revision: String,
    },
    SnapshotSong {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pack: Option<String>,
        id: Option<u64>,
        entry: Value,
        directory: PathBuf,
    },
    SnapshotEnd {
        id: Option<u64>,
        count: usize,
    },
    /// How an import ended, as values the app words.
    Terminal {
        id: u64,
        status: Outcome,
        /// The songs an installed import added, replaced and skipped.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        songs: Option<Counts>,
        restore_ids: Vec<String>,
        /// Why it failed, or why the library needs recovery.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        failure: Option<Failure>,
        /// After it ended, the library's songs could not be listed again.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        refresh: Option<Failure>,
    },
    Fatal {
        code: String,
        message: String,
    },
    Bye,
}

/// Clean EOF is recognized only between messages. A partial header or payload is
/// an error, not an empty message or a successfully completed snapshot.
pub fn read<T: DeserializeOwned>(reader: &mut impl Read) -> io::Result<Option<T>> {
    let mut header = [0; 4];
    loop {
        match reader.read(&mut header[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    reader.read_exact(&mut header[1..])?;
    let len = u32::from_be_bytes(header) as usize;
    if len == 0 || len > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Importer frame length exceeds protocol bounds",
        ));
    }
    let mut bytes = vec![0; len];
    reader.read_exact(&mut bytes)?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    // Serde's internally tagged unit variants otherwise ignore extra fields,
    // even with deny_unknown_fields. These envelopes have exactly one key.
    if value.as_object().is_some_and(|object| {
        matches!(
            object.get("type").and_then(Value::as_str),
            Some("shutdown" | "bye")
        ) && object.len() != 1
    }) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Unknown field in importer unit message",
        ));
    }
    serde_json::from_value(value)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn write<T: Serialize>(writer: &mut impl Write, message: &T) -> io::Result<()> {
    let bytes =
        serde_json::to_vec(message).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Importer frame length exceeds protocol bounds",
        ));
    }
    writer.write_all(&(bytes.len() as u32).to_be_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frames_round_trip_without_merging_messages() {
        let first = Request::Hello { version: VERSION };
        let second = Request::Import {
            id: 3,
            url: "https://example.invalid/song".into(),
            offline: false,
        };
        let mut bytes = Vec::new();
        write(&mut bytes, &first).unwrap();
        write(&mut bytes, &second).unwrap();
        let mut reader = bytes.as_slice();
        assert_eq!(read::<Request>(&mut reader).unwrap(), Some(first));
        assert_eq!(read::<Request>(&mut reader).unwrap(), Some(second));
        assert!(read::<Request>(&mut reader).unwrap().is_none());
    }
    #[test]
    fn eof_truncation_and_invalid_frames_are_distinct() {
        assert!(read::<Request>(&mut &b""[..]).unwrap().is_none());
        for bytes in [&b"\0"[..], &b"\0\0\0\x05null"[..]] {
            assert_eq!(
                read::<Request>(&mut &bytes[..]).unwrap_err().kind(),
                io::ErrorKind::UnexpectedEof
            );
        }
        for bytes in [
            0_u32.to_be_bytes(),
            ((MAX_FRAME_BYTES + 1) as u32).to_be_bytes(),
        ] {
            assert_eq!(
                read::<Request>(&mut &bytes[..]).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
        let mut invalid = Vec::new();
        write(
            &mut invalid,
            &serde_json::json!({"type":"shutdown","unexpected":1}),
        )
        .unwrap();
        assert_eq!(
            read::<Request>(&mut invalid.as_slice()).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
    #[test]
    fn outgoing_oversize_emits_no_partial_header() {
        let mut bytes = Vec::new();
        let message = serde_json::json!({"type":"fatal", "code":"test", "message":"x".repeat(MAX_FRAME_BYTES + 1)});
        assert_eq!(
            write(&mut bytes, &message).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert!(bytes.is_empty());
        write(
            &mut bytes,
            &serde_json::json!({"type":"bye", "unexpected":true}),
        )
        .unwrap();
        assert_eq!(
            read::<Event>(&mut bytes.as_slice()).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn stages_and_endings_cross_as_values_not_words() {
        let progress = Event::Progress {
            progress: Some((2, 5)),
            id: Some(1),
            stage: Stage::ConvertingMusic,
        };
        let json = serde_json::to_value(&progress).unwrap();
        assert_eq!(json["stage"], "convertingMusic");
        let ended = Event::Terminal {
            id: 1,
            status: Outcome::Installed,
            songs: Some(Counts {
                added: 1,
                replaced: 2,
                skipped: 3,
            }),
            restore_ids: vec!["a".into()],
            failure: None,
            refresh: Some(Failure {
                code: "filesystem".into(),
                message: "No such file or directory (os error 2)".into(),
            }),
        };
        let json = serde_json::to_value(&ended).unwrap();
        assert!(json.get("failure").is_none(), "{json}");
        let mut bytes = Vec::new();
        for event in [&progress, &ended] {
            write(&mut bytes, event).unwrap();
        }
        let mut reader = bytes.as_slice();
        assert_eq!(read::<Event>(&mut reader).unwrap(), Some(progress));
        assert_eq!(read::<Event>(&mut reader).unwrap(), Some(ended));
        // Words where a value belongs are refused.
        let mut worded = Vec::new();
        write(
            &mut worded,
            &serde_json::json!({"type":"progress","id":1,"stage":"Converting music…"}),
        )
        .unwrap();
        assert_eq!(
            read::<Event>(&mut worded.as_slice()).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn many_chart_metadata_above_one_mebibyte_fits() {
        let message = Event::SnapshotSong {
            pack: Some("Test pack".into()),
            id: Some(1),
            entry: serde_json::json!({"metadata": "x".repeat(1024 * 1024 + 10)}),
            directory: "/immutable/song".into(),
        };
        let mut bytes = Vec::new();
        write(&mut bytes, &message).unwrap();
        assert_eq!(read::<Event>(&mut bytes.as_slice()).unwrap(), Some(message));
    }
}
