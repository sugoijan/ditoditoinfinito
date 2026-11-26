//! Version 1's framing (`docs/plans/source-plugins.md`, "Framing"):
//!
//! ```text
//! u32 big-endian  length of the rest (4 + json + attachment), at most MAX_FRAME_BYTES
//! u32 big-endian  json's length (at least 2)
//! json            one message, UTF-8
//! attachment      the rest: a `File` frame's bytes (at most MAX_ATTACHMENT_BYTES), else none
//! ```
//!
//! A clean end of the stream is recognized only between frames: a partial
//! length or body is [`WireError::Truncated`], never a message or a clean
//! end. Lengths are checked before anything is allocated.

use std::io::{self, Read, Write};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::limits::{MAX_ATTACHMENT_BYTES, MAX_FRAME_BYTES};
use crate::message::{AppMessage, PluginMessage};

/// One frame's two parts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Frame {
    pub json: Vec<u8>,
    pub attachment: Vec<u8>,
}

/// Why a frame was refused.
#[derive(Debug)]
pub enum WireError {
    /// The pipe failed.
    Io(io::Error),
    /// The stream ended inside a frame.
    Truncated,
    /// A length past the protocol's bounds (as announced).
    Oversized(u64),
    /// Not a frame or not a message of version 1 (the reason, for a
    /// log).
    Malformed(String),
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WireError::Io(e) => write!(f, "pipe: {e}"),
            WireError::Truncated => write!(f, "the stream ended inside a frame"),
            WireError::Oversized(n) => write!(f, "a frame length of {n} bytes is past the bounds"),
            WireError::Malformed(why) => write!(f, "malformed frame: {why}"),
        }
    }
}

impl std::error::Error for WireError {}

impl WireError {
    /// The value the test vectors name it by.
    pub fn kind(&self) -> &'static str {
        match self {
            WireError::Io(_) => "io",
            WireError::Truncated => "truncated",
            WireError::Oversized(_) => "oversized",
            WireError::Malformed(_) => "malformed",
        }
    }
}

fn malformed(why: impl Into<String>) -> WireError {
    WireError::Malformed(why.into())
}

/// Reads `buf` whole: a short read is [`WireError::Truncated`].
fn read_whole(reader: &mut impl Read, buf: &mut [u8]) -> Result<(), WireError> {
    reader.read_exact(buf).map_err(|e| {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            WireError::Truncated
        } else {
            WireError::Io(e)
        }
    })
}

/// The next frame, or `None` at a clean end of the stream.
pub fn read_frame(reader: &mut impl Read) -> Result<Option<Frame>, WireError> {
    let mut header = [0u8; 4];
    loop {
        match reader.read(&mut header[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(WireError::Io(e)),
        }
    }
    read_whole(reader, &mut header[1..])?;
    let total = u32::from_be_bytes(header) as usize;
    if total > MAX_FRAME_BYTES {
        return Err(WireError::Oversized(total as u64));
    }
    if total < 4 + 2 {
        return Err(malformed("frame shorter than a message"));
    }
    read_whole(reader, &mut header)?;
    let json_len = u32::from_be_bytes(header) as usize;
    if json_len < 2 || json_len > total - 4 {
        return Err(malformed("JSON length outside the frame"));
    }
    let attachment_len = total - 4 - json_len;
    if attachment_len > MAX_ATTACHMENT_BYTES {
        return Err(WireError::Oversized(attachment_len as u64));
    }
    let mut json = vec![0; json_len];
    read_whole(reader, &mut json)?;
    let mut attachment = vec![0; attachment_len];
    read_whole(reader, &mut attachment)?;
    Ok(Some(Frame { json, attachment }))
}

/// A frame's bytes, or why it cannot be sent (nothing is written then).
pub fn encode_frame(json: &[u8], attachment: &[u8]) -> Result<Vec<u8>, WireError> {
    let total = 4 + json.len() + attachment.len();
    if json.len() < 2 || total > MAX_FRAME_BYTES || attachment.len() > MAX_ATTACHMENT_BYTES {
        return Err(WireError::Oversized(total as u64));
    }
    let mut out = Vec::with_capacity(4 + total);
    out.extend_from_slice(&(total as u32).to_be_bytes());
    out.extend_from_slice(&(json.len() as u32).to_be_bytes());
    out.extend_from_slice(json);
    out.extend_from_slice(attachment);
    Ok(out)
}

/// `message` (with `attachment`) as a frame's bytes.
pub fn encode<T: Serialize>(message: &T, attachment: &[u8]) -> Result<Vec<u8>, WireError> {
    let json = serde_json::to_vec(message).map_err(|e| malformed(e.to_string()))?;
    encode_frame(&json, attachment)
}

/// Writes `message` (with `attachment`) whole, then flushes.
pub fn write<T: Serialize>(
    writer: &mut impl Write,
    message: &T,
    attachment: &[u8],
) -> Result<(), WireError> {
    let bytes = encode(message, attachment)?;
    writer.write_all(&bytes).map_err(WireError::Io)?;
    writer.flush().map_err(WireError::Io)
}

/// A message of either side from a frame's JSON: an object with a known
/// `type` and nothing it does not know.
fn decode_json<T: DeserializeOwned>(json: &[u8]) -> Result<T, WireError> {
    let value: Value = serde_json::from_slice(json).map_err(|e| malformed(e.to_string()))?;
    if !value.is_object() {
        return Err(malformed("not a JSON object"));
    }
    serde_json::from_value(value).map_err(|e| malformed(e.to_string()))
}

/// A plugin's message and its attachment: only a `File` carries one, and
/// a `File`'s is never empty unless it is its file's last chunk.
pub fn decode_plugin(frame: Frame) -> Result<(PluginMessage, Vec<u8>), WireError> {
    let message: PluginMessage = decode_json(&frame.json)?;
    match &message {
        PluginMessage::File { last, .. } => {
            if frame.attachment.is_empty() && !last {
                return Err(malformed("an empty chunk that is not a file's last"));
            }
        }
        _ if !frame.attachment.is_empty() => {
            return Err(malformed("an attachment on a message that takes none"));
        }
        _ => {}
    }
    Ok((message, frame.attachment))
}

/// The app's message (the app attaches nothing).
pub fn decode_app(frame: Frame) -> Result<AppMessage, WireError> {
    if !frame.attachment.is_empty() {
        return Err(malformed("an attachment on an app message"));
    }
    decode_json(&frame.json)
}

/// The next plugin message, or `None` at a clean end.
pub fn read_plugin(reader: &mut impl Read) -> Result<Option<(PluginMessage, Vec<u8>)>, WireError> {
    read_frame(reader)?.map(decode_plugin).transpose()
}

/// The next app message, or `None` at a clean end.
pub fn read_app(reader: &mut impl Read) -> Result<Option<AppMessage>, WireError> {
    read_frame(reader)?.map(decode_app).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::*;

    #[test]
    fn frames_round_trip_with_and_without_attachments() {
        let mut bytes = Vec::new();
        write(&mut bytes, &PluginMessage::Bye {}, &[]).unwrap();
        let file = PluginMessage::File {
            op: 3,
            path: "song/a.ogg".into(),
            size: 5,
            offset: 0,
            last: true,
        };
        write(&mut bytes, &file, b"hello").unwrap();
        let mut r = bytes.as_slice();
        assert_eq!(read_plugin(&mut r).unwrap().unwrap().0, PluginMessage::Bye {});
        assert_eq!(
            read_plugin(&mut r).unwrap().unwrap(),
            (file, b"hello".to_vec())
        );
        assert!(read_plugin(&mut r).unwrap().is_none());
    }

    #[test]
    fn truncation_oversize_and_garbage_are_told_apart() {
        assert!(read_frame(&mut &b""[..]).unwrap().is_none());
        let mut good = Vec::new();
        write(&mut good, &AppMessage::Shutdown {}, &[]).unwrap();
        for cut in 1..good.len() {
            assert!(matches!(
                read_frame(&mut &good[..cut]),
                Err(WireError::Truncated)
            ));
        }
        let huge = ((MAX_FRAME_BYTES + 1) as u32).to_be_bytes();
        assert!(matches!(
            read_frame(&mut &huge[..]),
            Err(WireError::Oversized(_))
        ));
        let mut wrong_json_len = Vec::new();
        wrong_json_len.extend_from_slice(&10u32.to_be_bytes());
        wrong_json_len.extend_from_slice(&20u32.to_be_bytes());
        wrong_json_len.extend_from_slice(&[b' '; 6]);
        assert!(matches!(
            read_frame(&mut &wrong_json_len[..]),
            Err(WireError::Malformed(_))
        ));
        // Extra fields on a field-less message, words for a value, an
        // attachment where none belongs.
        for json in [
            br#"{"type":"bye","extra":1}"#.as_slice(),
            br#"{"type":"progress","op":1,"step":"Downloading..."}"#,
            br#"{"type":"teleport"}"#,
            br#"[1,2]"#,
        ] {
            let frame = Frame {
                json: json.to_vec(),
                attachment: Vec::new(),
            };
            assert!(decode_plugin(frame).is_err(), "{}", String::from_utf8_lossy(json));
        }
        let frame = Frame {
            json: br#"{"type":"bye"}"#.to_vec(),
            attachment: vec![1],
        };
        assert!(decode_plugin(frame).is_err());
    }

    #[test]
    fn nothing_is_written_for_a_frame_past_the_bounds() {
        let mut out = Vec::new();
        let chunk = vec![0u8; MAX_ATTACHMENT_BYTES + 1];
        let file = PluginMessage::File {
            op: 1,
            path: "a".into(),
            size: chunk.len() as u64,
            offset: 0,
            last: true,
        };
        assert!(write(&mut out, &file, &chunk).is_err());
        assert!(out.is_empty());
    }
}
