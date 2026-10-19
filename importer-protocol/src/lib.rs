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
        stage: String,
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
    Terminal {
        id: u64,
        status: Outcome,
        summary: String,
        restore_ids: Vec<String>,
        details: Vec<String>,
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
