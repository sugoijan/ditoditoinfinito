//! `DDI_MOVIES=<dir> cargo test -p ddi-library --test local_h264 -- --nocapture`
//! checks every H.264 AVI under a local folder (third-party files stay
//! local; needs `ffprobe`): the SPS gives the size FFmpeg reports, every
//! keyframe of the index is an IDR, and `h264::OutputOrder` gives out the
//! pictures FFmpeg's decoder gives out, in its order (each frame's packet,
//! `pkt_pos`) and with its times (`pkt_dts`, a frame without one following
//! the previous by a period, as the module stamps them).

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Command;

use ddi_library::avi::{self, Chunk};
use ddi_library::h264;

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("avi"))
        {
            out.push(path);
        }
    }
}

fn read(file: &mut std::fs::File, at: u64, n: usize) -> Vec<u8> {
    let mut b = vec![0; n];
    let got = file
        .seek(SeekFrom::Start(at))
        .and_then(|_| file.read(&mut b));
    b.truncate(got.unwrap_or(0));
    b
}

/// `(time in frames, packet offset)` of each frame.
type Frames = Vec<(u64, u64)>;

/// FFmpeg's frames: times in frames, as the module stamps them, and the
/// file offset of each frame's packet.
fn ffmpeg_frames(path: &Path) -> Option<(Frames, u32, u32)> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0"])
        .args(["-show_entries", "frame=pkt_dts,pkt_pos:stream=width,height"])
        .args(["-of", "csv=p=0"])
        .arg(path)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    // The stream's line (`width,height`) comes after the frames'; a
    // frame's line may end in a comma (side data).
    let fields = |l: &str| -> Vec<String> {
        l.split(',')
            .map(|f| f.trim().to_string())
            .filter(|f| !f.is_empty())
            .collect()
    };
    // Frame lines are `pkt_dts,pkt_pos` (`N/A` for a missing time); the
    // stream's line is the last.
    let mut lines: Vec<Vec<String>> = text.lines().map(fields).filter(|f| !f.is_empty()).collect();
    let size = lines.pop()?;
    let (w, h) = (size.first()?.parse().ok()?, size.get(1)?.parse().ok()?);
    let mut stamps: Vec<(u64, u64)> = Vec::new();
    for f in lines {
        let t = match f.first().and_then(|t| t.parse::<u64>().ok()) {
            Some(t) => t,
            None => stamps.last().map_or(0, |p| p.0 + 1),
        };
        let pos = f.get(1).and_then(|p| p.parse::<u64>().ok()).unwrap_or(0);
        stamps.push((t, pos));
    }
    Some((stamps, w, h))
}

#[test]
fn local_h264() {
    let Some(dir) = std::env::var_os("DDI_MOVIES") else {
        return;
    };
    let mut files = Vec::new();
    walk(Path::new(&dir), &mut files);
    files.sort();
    let (mut checked, mut matched) = (0, 0);
    for path in &files {
        let Ok(mut file) = std::fs::File::open(path) else {
            continue;
        };
        let Ok(avi) = avi::parse_header(&read(&mut file, 0, avi::HEADER_LEN)) else {
            continue;
        };
        let header = read(&mut file, avi.idx1_at(), 8);
        if !header.starts_with(b"idx1") {
            println!("{}\tno idx1", path.display());
            continue;
        }
        let size = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        let chunks: Vec<Chunk> = avi.parse_idx1(&read(&mut file, avi.idx1_at(), size + 8));
        let Some(first) = chunks.first() else {
            continue;
        };
        let first_data = read(&mut file, first.offset, first.size as usize);
        let Some(stream) = h264::stream(&avi.extradata, &first_data) else {
            // Not H.264 (or no parameter sets).
            continue;
        };
        checked += 1;
        let keys: Vec<&Chunk> = chunks.iter().filter(|c| c.key).collect();
        let idr = keys
            .iter()
            .filter(|c| h264::is_idr(&read(&mut file, c.offset, c.size as usize), stream.framing))
            .count();
        let Some(delay) = stream.sps.reorder_delay() else {
            println!("{}\tno bitstream restriction", path.display());
            continue;
        };
        let Some((expected, w, h)) = ffmpeg_frames(path) else {
            println!("{}\tffprobe failed", path.display());
            continue;
        };
        let mut pictures = h264::PictureOrder::new(&stream);
        let mut order = h264::OutputOrder::new(delay);
        let mut got: Vec<(u64, u64)> = Vec::new();
        for (i, c) in chunks.iter().enumerate() {
            let data = read(&mut file, c.offset, c.size as usize);
            let Some(picture) = pictures.picture(&data, stream.framing, &stream.sps) else {
                continue;
            };
            let released = order.push(i, picture, c.frame);
            got.extend(released.out.map(|(id, t)| (t, chunks[id].offset)));
        }
        got.extend(
            order
                .drain()
                .into_iter()
                .map(|(id, t)| (t, chunks[id].offset)),
        );
        let first_diff = got.iter().zip(&expected).position(|(a, b)| a != b);
        let same = first_diff.is_none() && got.len() == expected.len();
        if same && (w, h) == (stream.sps.width, stream.sps.height) && idr == keys.len() {
            matched += 1;
        }
        println!(
            "{}\t{}\t{}x{} (ffprobe {w}x{h})\tdelay {delay}\tidr {idr}/{}\tframes {} (ffprobe {})\t{}",
            path.display(),
            stream.sps.codec(),
            stream.sps.width,
            stream.sps.height,
            keys.len(),
            got.len(),
            expected.len(),
            match first_diff {
                None if same => "same".to_string(),
                None => "lengths differ".to_string(),
                Some(i) => format!("differs at {i}: {:?} vs {:?}", got[i], expected[i]),
            },
        );
    }
    println!("{matched} of {checked} H.264 AVIs match FFmpeg");
}
