//! One whole MPEG Layer III frame. SWF counts physical frames, while the
//! decoder inventory separately recognizes duration metadata frames.
use crate::{Error, Result};
pub(crate) struct Frame {
    pub header: u32,
    pub version: u32,
    pub rate: u32,
    pub channels: u32,
    pub bytes: usize,
    pub samples: u32,
}
pub(crate) fn frame(bytes: &[u8]) -> Result<Frame> {
    let invalid = |s| Error::new("invalid-audio", s);
    if bytes.len() < 4 {
        return Err(invalid("Truncated MP3 header"));
    }
    let h = u32::from_be_bytes(bytes[..4].try_into().unwrap());
    let version = (h >> 19) & 3;
    let layer = (h >> 17) & 3;
    let bitrate = (h >> 12) & 15;
    let index = (h >> 10) & 3;
    let pad = (h >> 9) & 1;
    if h & 0xffe00000 != 0xffe00000
        || version == 1
        || layer != 1
        || bitrate == 0
        || bitrate == 15
        || index == 3
    {
        return Err(invalid("Unsupported/malformed MPEG Layer III frame"));
    }
    let rate = [44100u32, 48000, 32000][index as usize]
        / match version {
            3 => 1,
            2 => 2,
            _ => 4,
        };
    let channels = if h >> 6 & 3 == 3 { 1 } else { 2 };
    let rates = if version == 3 {
        [
            0u32, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
        ]
    } else {
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160]
    };
    let size = ((if version == 3 { 144000 } else { 72000 }) * rates[bitrate as usize] / rate + pad)
        as usize;
    if size < 4 || size > bytes.len() {
        return Err(invalid("MP3 frame crosses input boundary"));
    }
    Ok(Frame {
        header: h,
        version,
        rate,
        channels,
        bytes: size,
        samples: if version == 3 { 1152 } else { 576 },
    })
}
