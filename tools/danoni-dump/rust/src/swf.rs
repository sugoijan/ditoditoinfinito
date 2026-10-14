//! Bounded, non-executing FWS/CWS inventory and whole-frame MP3 demux.
//! All byte offsets refer to the decompressed whole file unless noted.
use crate::{Error, Result, assets::sha256};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
pub const MOVIE_LIMIT: usize = 16 * 1024 * 1024;
pub const DECOMPRESSED_LIMIT: usize = 64 * 1024 * 1024;
fn invalid(s: impl Into<String>) -> Error {
    Error::new("invalid-swf-audio", s)
}
fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(b[at..at + 2].try_into().unwrap())
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    pub fps: f64,
    pub frames: u16,
    pub tag_start: usize,
    pub swf_version: u8,
    pub sha256: String,
}
#[derive(Debug)]
struct Tag {
    path: String,
    frame: u32,
    code: u16,
    offset: usize,
    begin: usize,
    end: usize,
}
/// Non-executing script/container fingerprints; no third-party bytecode is
/// exported or evaluated by the host.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActionFingerprint {
    pub path: String,
    pub frame: u32,
    pub code: u16,
    pub bytes: usize,
    pub sha256: String,
}
pub struct Movie {
    pub header: Header,
    data: Vec<u8>,
    tags: Vec<Tag>,
}
impl Movie {
    pub fn read(original: &[u8], checkpoint: &mut impl FnMut() -> Result<()>) -> Result<Self> {
        checkpoint()?;
        if !(12..=MOVIE_LIMIT).contains(&original.len())
            || !matches!(&original[..3], b"FWS" | b"CWS")
        {
            return Err(invalid("Only bounded FWS/CWS input is supported"));
        }
        let declared = u32::from_le_bytes(original[4..8].try_into().unwrap()) as usize;
        if !(12..=DECOMPRESSED_LIMIT).contains(&declared) {
            return Err(invalid("Invalid declared SWF length"));
        }
        let data = if &original[..3] == b"CWS" {
            let mut decoder = flate2::Decompress::new(true);
            let mut data = original[..8].to_vec();
            loop {
                checkpoint()?;
                let mut chunk = [0; 65536];
                let before = decoder.total_out();
                let consumed = decoder.total_in() as usize;
                let status = decoder
                    .decompress(
                        &original[8 + consumed..],
                        &mut chunk,
                        flate2::FlushDecompress::None,
                    )
                    .map_err(|e| invalid(e.to_string()))?;
                let produced = (decoder.total_out() - before) as usize;
                if data.len() + produced > declared {
                    return Err(invalid("Compressed SWF exceeds declared size"));
                }
                data.extend_from_slice(&chunk[..produced]);
                if status == flate2::Status::StreamEnd {
                    if decoder.total_in() as usize != original.len() - 8 {
                        return Err(invalid("Trailing compressed SWF bytes"));
                    }
                    break;
                }
                if produced == 0 && decoder.total_in() as usize == consumed {
                    return Err(invalid("Truncated compressed SWF"));
                }
            }
            data
        } else {
            original.to_vec()
        };
        if data.len() != declared {
            return Err(invalid("SWF length differs from header"));
        }
        let bits = data[8] >> 3;
        let offset = 8 + (5 + 4 * bits as usize).div_ceil(8);
        if bits == 0 || offset + 4 > data.len() {
            return Err(invalid("Invalid stage RECT"));
        }
        let fps = u16_at(&data, offset) as f64 / 256.0;
        let frames = u16_at(&data, offset + 2);
        if !(1.0..=240.0).contains(&fps) || frames == 0 {
            return Err(invalid("Unsupported frame rate/count"));
        }
        let header = Header {
            fps,
            frames,
            tag_start: offset + 4,
            swf_version: data[3],
            sha256: sha256(original),
        };
        let mut tags = vec![];
        Self::walk(
            &data,
            header.tag_start,
            data.len(),
            "root",
            0,
            &mut tags,
            checkpoint,
        )?;
        let mut routes = BTreeSet::new();
        for tag in tags.iter().filter(|t| t.code == 0) {
            if !routes.insert(&tag.path) {
                return Err(invalid("Duplicate sprite timeline route"));
            }
        }
        Ok(Self { header, data, tags })
    }
    fn walk(
        data: &[u8],
        start: usize,
        end: usize,
        path: &str,
        depth: usize,
        tags: &mut Vec<Tag>,
        checkpoint: &mut impl FnMut() -> Result<()>,
    ) -> Result<()> {
        if depth > 32 {
            return Err(invalid("Sprite nesting limit"));
        }
        let mut pos = start;
        let mut frame = 1;
        while pos + 2 <= end {
            if tags.len() >= 200000 {
                return Err(invalid("Tag budget exceeded"));
            }
            if tags.len().is_multiple_of(256) {
                checkpoint()?;
            }
            let offset = pos;
            let head = u16_at(data, pos);
            pos += 2;
            let code = head >> 6;
            let size = if head & 63 == 63 {
                if pos + 4 > end {
                    return Err(invalid("Truncated long tag header"));
                }
                let n = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
                pos += 4;
                n
            } else {
                (head & 63) as usize
            };
            if size > end - pos {
                return Err(invalid("Truncated tag payload"));
            }
            let next = pos + size;
            tags.push(Tag {
                path: path.into(),
                frame,
                code,
                offset,
                begin: pos,
                end: next,
            });
            if code == 39 {
                if size < 4 {
                    return Err(invalid("Truncated sprite"));
                }
                let child = format!("{path}/sprite{}", u16_at(data, pos));
                Self::walk(data, pos + 4, next, &child, depth + 1, tags, checkpoint)?;
            }
            pos = next;
            if code == 0 {
                if size != 0 || pos != end {
                    return Err(invalid("Trailing bytes after End tag"));
                }
                return Ok(());
            }
            if code == 1 {
                if size != 0 {
                    return Err(invalid("Nonempty ShowFrame tag"));
                }
                frame += 1;
            }
        }
        Err(invalid("Missing End tag"))
    }
    pub fn action_fingerprints(
        &self,
        checkpoint: &mut impl FnMut() -> Result<()>,
    ) -> Result<Vec<ActionFingerprint>> {
        let mut result = Vec::new();
        for (index, tag) in self.tags.iter().enumerate() {
            if index.is_multiple_of(256) {
                checkpoint()?;
            }
            if [12, 26, 34, 59, 70, 72, 82].contains(&tag.code) {
                if result.len() >= 16384 {
                    return Err(invalid("Action fingerprint count exceeds 16384"));
                }
                checkpoint()?;
                result.push(ActionFingerprint {
                    path: tag.path.clone(),
                    frame: tag.frame,
                    code: tag.code,
                    bytes: tag.end - tag.begin,
                    sha256: sha256(&self.data[tag.begin..tag.end]),
                });
            }
        }
        Ok(result)
    }
    pub fn inventory(&self, checkpoint: &mut impl FnMut() -> Result<()>) -> Result<Vec<Stream>> {
        let mut streams: Vec<Stream> = vec![];
        let mut active = BTreeMap::new();
        for (i, tag) in self.tags.iter().enumerate() {
            if i.is_multiple_of(256) {
                checkpoint()?;
            }
            let payload = &self.data[tag.begin..tag.end];
            if matches!(tag.code, 18 | 45) {
                if payload.len() < 4 {
                    return Err(invalid("Truncated stream head"));
                }
                let packed = payload[1];
                let compression = packed >> 4;
                if compression == 2 && payload.len() < 6 {
                    return Err(invalid("Missing MP3 latency header"));
                }
                let stream = Stream {
                    schema: "ddi-swf-audio/experimental-1".into(),
                    path: tag.path.clone(),
                    head_tag_offset: tag.offset,
                    head_movie_frame: tag.frame,
                    compression,
                    sample_rate: [5512, 11025, 22050, 44100][((packed >> 2) & 3) as usize],
                    channels: 1 + (packed & 1),
                    samples_per_movie_frame: u16_at(payload, 2),
                    latency_seek: if compression == 2 {
                        Some(u16_at(payload, 4) as i16)
                    } else {
                        None
                    },
                    blocks: vec![],
                    movie: self.header.clone(),
                    bytes: vec![],
                };
                if streams.len() >= 4096 {
                    return Err(invalid("Stream head budget exceeded"));
                }
                active.insert(tag.path.clone(), streams.len());
                streams.push(stream);
            } else if tag.code == 19 {
                let index = active
                    .get(&tag.path)
                    .ok_or_else(|| invalid("Stream block has no head"))?;
                let stream = &mut streams[*index];
                if stream.compression != 2 {
                    continue;
                }
                if payload.len() < 4 {
                    return Err(invalid("Truncated MP3 block header"));
                }
                if stream
                    .blocks
                    .last()
                    .is_some_and(|b| b.movie_frame == tag.frame)
                {
                    return Err(invalid("Multiple stream blocks in one frame"));
                }
                let chunk = &payload[4..];
                let mut samples = 0;
                let mut count = 0;
                let mut at = 0;
                while at < chunk.len() {
                    if count % 256 == 0 {
                        checkpoint()?;
                    }
                    let frame = crate::mpeg::frame(&chunk[at..]).map_err(|e| invalid(e.message))?;
                    if frame.rate != stream.sample_rate || frame.channels != stream.channels as u32
                    {
                        return Err(invalid("MPEG frame differs from stream head"));
                    }
                    samples += frame.samples;
                    count += 1;
                    at += frame.bytes;
                }
                if samples != u16_at(payload, 0) as u32 {
                    return Err(invalid("Block sample count differs from MPEG frames"));
                }
                stream.blocks.push(Block {
                    movie_frame: tag.frame,
                    tag_offset: tag.offset,
                    sample_count: samples,
                    seek_samples: u16_at(payload, 2) as i16,
                    mp3_frames: count,
                    mp3_offset: stream.bytes.len(),
                    mp3_bytes: chunk.len(),
                    sha256: sha256(chunk),
                });
                stream.bytes.extend_from_slice(chunk);
            }
        }
        Ok(streams)
    }
    /// Exact route with an optional reviewed whole-file byte interval.
    pub fn demux(
        &self,
        path: &str,
        slice: Option<(usize, usize)>,
        checkpoint: &mut impl FnMut() -> Result<()>,
    ) -> Result<Demuxed> {
        let matches = self
            .inventory(checkpoint)?
            .into_iter()
            .filter(|s| s.path == path)
            .collect::<Vec<_>>();
        if matches.len() != 1 || matches[0].compression != 2 || matches[0].blocks.is_empty() {
            return Err(invalid("Need exactly one MP3 stream on selected route"));
        }
        self.select(matches.into_iter().next().unwrap(), slice, checkpoint)
    }
    fn select(
        &self,
        stream: Stream,
        slice: Option<(usize, usize)>,
        checkpoint: &mut impl FnMut() -> Result<()>,
    ) -> Result<Demuxed> {
        let mut report = serde_json::to_value(&stream).map_err(|e| invalid(e.to_string()))?;
        let mut bytes = vec![];
        let mut blocks = vec![];
        if let Some((a, b)) = slice {
            if a >= b || b > self.data.len() {
                return Err(invalid("Invalid whole-file slice bounds"));
            }
            report["selection"] = json!({"sliceStart":a,"sliceEnd":b,"excludedBlocks":stream.blocks.iter().filter(|t|t.tag_offset<a || t.tag_offset>=b).count(),"scope":"Only blocks in the runtime-observed continuous song slice"});
        }
        for block in &stream.blocks {
            checkpoint()?;
            if slice.is_some_and(|(a, b)| block.tag_offset < a || block.tag_offset >= b) {
                continue;
            }
            let mut row = serde_json::to_value(block).map_err(|e| invalid(e.to_string()))?;
            if slice.is_some() {
                row["sourceMp3Offset"] = json!(block.mp3_offset);
            }
            row["mp3Offset"] = json!(bytes.len());
            bytes.extend_from_slice(
                &stream.bytes[block.mp3_offset..block.mp3_offset + block.mp3_bytes],
            );
            blocks.push(row);
        }
        if blocks.is_empty() || bytes.is_empty() {
            return Err(invalid("Selected stream slice is empty"));
        }
        let origin = blocks[0]["movieFrame"].as_u64().unwrap();
        let mut total = 0u64;
        let mut regular = true;
        let mut contiguous = true;
        for (i, b) in blocks.iter_mut().enumerate() {
            let frame = b["movieFrame"].as_u64().unwrap();
            let ideal = (frame - origin) as f64 * stream.sample_rate as f64 / self.header.fps;
            let residual = total as f64 + b["seekSamples"].as_i64().unwrap() as f64
                - ideal
                - stream.latency_seek.unwrap_or(0) as f64;
            b["sourceSampleOffset"] = json!(total);
            b["seekResidualSamples"] = json!(residual);
            if b["mp3Bytes"] != 0 && residual.abs() > 1e-9 {
                regular = false;
            }
            contiguous &= frame == origin + i as u64;
            total += b["sampleCount"].as_u64().unwrap();
        }
        let last = blocks
            .iter()
            .filter(|b| b["mp3Bytes"] != 0)
            .map(|b| b["movieFrame"].as_u64().unwrap())
            .max()
            .unwrap();
        let by_frame = blocks
            .iter()
            .map(|b| (b["movieFrame"].as_u64().unwrap(), b))
            .collect::<BTreeMap<_, _>>();
        let mut buffered = 0i64;
        let mut gap = None;
        for frame in origin..=last {
            if frame % 256 == 0 {
                checkpoint()?;
            }
            let block = by_frame.get(&frame);
            if let Some(b) = block {
                buffered += b["sampleCount"].as_i64().unwrap();
            }
            buffered -= stream.samples_per_movie_frame as i64;
            if block.is_none() && buffered <= -(stream.samples_per_movie_frame as i64) {
                gap = Some(frame);
                break;
            }
        }
        report["ruffleContinuousPayloadEligible"] = json!(gap.is_none());
        report["firstRufflePayloadGapMovieFrame"] = json!(gap);
        report["mp3Sha256"] = json!(sha256(&bytes));
        report["mp3Bytes"] = json!(bytes.len());
        report["declaredSampleFrames"] = json!(total);
        report["firstBlockMovieFrame"] = json!(origin);
        report["continuousZeroLatencyEligible"] = json!(
            regular
                && contiguous
                && stream.latency_seek == Some(0)
                && blocks[0]["seekSamples"] == 0
        );
        report["blocks"] = json!(blocks);
        report["transformation"] = json!(
            "Concatenate whole MP3 payloads after each four-byte samples/seek header; no decoding, trimming or transcoding"
        );
        Ok(Demuxed { bytes, report })
    }
    /// Ruffle's SwfSlice offsets exclude the RECT/FPS/frame-count header.
    /// Match both the container's exact End and the observed audio format.
    pub fn observed_stream(
        &self,
        start: &ObservedStart,
        checkpoint: &mut impl FnMut() -> Result<()>,
    ) -> Result<Demuxed> {
        let first = start
            .slice_start
            .checked_add(self.header.tag_start)
            .ok_or_else(|| invalid("Slice start overflow"))?;
        let last = start
            .slice_end
            .checked_add(self.header.tag_start)
            .ok_or_else(|| invalid("Slice end overflow"))?;
        if first >= last || last > self.data.len() {
            return Err(invalid("Invalid native tag-data slice"));
        }
        let ends = self
            .tags
            .iter()
            .filter(|t| t.code == 0)
            .map(|t| (&t.path, t.end))
            .collect::<BTreeMap<_, _>>();
        let candidates = self
            .tags
            .iter()
            .filter(|t| t.code == 19 && first <= t.offset && t.offset < last)
            .map(|t| t.path.clone())
            .collect::<BTreeSet<_>>();
        let mut matches = vec![];
        let inventory = self.inventory(checkpoint)?;
        for path in candidates {
            checkpoint()?;
            if ends.get(&path).copied() != Some(last) {
                continue;
            }
            let streams = inventory
                .iter()
                .filter(|s| s.path == path)
                .collect::<Vec<_>>();
            if streams.len() != 1 {
                continue;
            }
            let stream = streams[0];
            if stream.compression != 2
                || stream.blocks.is_empty()
                || stream.sample_rate != start.sample_rate
                || (stream.channels == 2) != start.stereo
                || stream.latency_seek != Some(start.latency_seek)
            {
                continue;
            }
            let result = match self.select(stream.clone(), Some((first, last)), checkpoint) {
                Ok(r) => r,
                Err(e) if e.code == "invalid-swf-audio" => continue,
                Err(e) => return Err(e),
            };
            if result.report["ruffleContinuousPayloadEligible"] == true {
                matches.push(result);
            }
        }
        if matches.len() != 1 {
            return Err(invalid(
                "Observed slice does not identify one continuous song stream",
            ));
        }
        let mut result = matches.remove(0);
        result.report["nativeSlice"] = json!({"tagDataOffsetBase":self.header.tag_start,"sliceStart":start.slice_start,"sliceEnd":start.slice_end});
        Ok(result)
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stream {
    pub schema: String,
    pub path: String,
    pub head_tag_offset: usize,
    pub head_movie_frame: u32,
    pub compression: u8,
    pub sample_rate: u32,
    pub channels: u8,
    pub samples_per_movie_frame: u16,
    pub latency_seek: Option<i16>,
    pub blocks: Vec<Block>,
    pub movie: Header,
    #[serde(skip)]
    bytes: Vec<u8>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub movie_frame: u32,
    pub tag_offset: usize,
    pub sample_count: u32,
    pub seek_samples: i16,
    pub mp3_frames: u32,
    pub mp3_offset: usize,
    pub mp3_bytes: usize,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObservedStart {
    pub slice_start: usize,
    pub slice_end: usize,
    pub sample_rate: u32,
    pub stereo: bool,
    pub latency_seek: i16,
}
pub struct Demuxed {
    pub bytes: Vec<u8>,
    pub report: Value,
}

/// Source-bound, successful saved-run selection. No execution or decoding.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DemuxRequest {
    pub movie: crate::assets::Binding,
    pub capture: crate::extraction::CaptureBindings,
    pub stream_start_index: usize,
    pub optional_color: Option<crate::extraction::OptionalColorEvidence>,
}
pub struct PreparedDemux {
    pub bytes: Vec<u8>,
    pub manifest: Value,
}
pub fn prepare_demux(
    request: &DemuxRequest,
    source: &impl crate::assets::AssetSource,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<PreparedDemux> {
    use crate::{assets, extraction};
    checkpoint()?;
    let mut inputs = assets::Inputs::new(source);
    // Apply the SWF bound before the more general saved-state verifier reads it.
    let original = inputs.checked(&request.movie, MOVIE_LIMIT)?;
    let (capture, retained) = extraction::checked_capture(
        &request.capture,
        &request.movie,
        request.optional_color.as_ref(),
        &mut inputs,
    )?;
    let movie = Movie::read(&original, checkpoint)?;
    let demux = select_capture(
        &movie,
        &capture,
        request.stream_start_index,
        &[],
        checkpoint,
    )?;
    let start = &capture["audio"]["streamStarts"][request.stream_start_index];
    let manifest = json!({"schema":"ddi-danoni-native-demux/1","producer":"native-swf-mp3-demux/1",
        "request":request,"sourceInputs":inputs.used,"reviewedOptionalColorDiagnostics":retained,
        "streamStart":start,"stream":demux.report,"audio":{"path":"stream.mp3","sha256":sha256(&demux.bytes)},
        "scope":"Unchanged whole MP3 frames selected by a successful source-bound saved run; decoded origin, chart contracts and fresh worker supervision remain separate proofs"});
    Ok(PreparedDemux {
        bytes: demux.bytes,
        manifest,
    })
}

pub(crate) fn select_capture(
    movie: &Movie,
    capture: &Value,
    stream_start_index: usize,
    reviewed: &[Value],
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Demuxed> {
    use crate::extraction;
    extraction::audit_ok(capture, reviewed)
        .map_err(|e| Error::new("native-audio-audit-unproved", e.message))?;
    let starts = capture["audio"]["streamStarts"]
        .as_array()
        .filter(|v| v.len() <= 4096)
        .ok_or_else(|| invalid("Missing/bounded native stream starts"))?;
    let start = starts
        .get(stream_start_index)
        .ok_or_else(|| invalid("Missing selected stream start"))?;
    if start["compression"] != "Mp3" {
        return Err(invalid("Selected start is not MP3"));
    }
    let selected: ObservedStart = serde_json::from_value(json!({"sliceStart":start["sliceStart"],
        "sliceEnd":start["sliceEnd"],"sampleRate":start["sampleRate"],"stereo":start["stereo"],
        "latencySeek":start["latencySeek"]}))
    .map_err(|e| invalid(e.to_string()))?;
    if capture["swfHeaderFps"].as_f64() != Some(movie.header.fps) {
        return Err(invalid("Capture and movie FPS differ"));
    }
    let demux = movie.observed_stream(&selected, checkpoint)?;
    if start["samplesPerMovieFrame"].as_u64() != demux.report["samplesPerMovieFrame"].as_u64() {
        return Err(invalid(
            "Observed stream sample budget differs from its SWF head",
        ));
    }
    Ok(demux)
}
