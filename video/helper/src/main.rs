//! `ddi-video-helper`: the desktop's video decoder, FFmpeg in a process of
//! its own, confined by the OS (`sandbox/`), fed movie bytes by the game
//! and handing frames back through shared memory (`ddi_video_protocol`;
//! `docs/plans/native-video.md`, section 10).
//!
//! `--serve --arena <descriptor or handle> --arena-len <bytes>` maps the
//! arena, enters the sandbox, checks it holds, answers `Hello` on stdout
//! and then serves requests from stdin one at a time. `--sandbox-check`
//! enters the sandbox and runs the self-test alone (what the desktop's
//! tests and `--danoni-importer-check`-style diagnostics run).
//! `--version` says what is inside.

mod arena;
mod ffi;
mod sandbox;

use arena::Arena;
use ddi_video_protocol::{
    INDEX_RECORD, Layout, MAX_OPEN, MAX_READ, PACKET_RECORD, Reply, Request, VERSION, exit,
    read_frame, write_frame,
};
use std::collections::BTreeMap;
use std::ffi::{CStr, CString, c_int, c_void};
use std::io::{self, Write};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

/// What the read callback and the loop share.
struct State {
    arena: Arena,
    /// The request being served, echoed in every reply.
    seq: AtomicU32,
}

static STATE: OnceLock<State> = OnceLock::new();

fn state() -> &'static State {
    STATE.get().expect("serving")
}

fn send(reply: &Reply) {
    let payload = match reply.encode() {
        Ok(p) => p,
        Err(e) => die(exit::PROTOCOL, &format!("cannot encode a reply: {e}")),
    };
    let seq = state().seq.load(Ordering::Relaxed);
    if let Err(e) = write_frame(&mut io::stdout().lock(), seq, &payload) {
        // The parent is gone.
        std::process::exit(if e.kind() == io::ErrorKind::BrokenPipe {
            0
        } else {
            exit::PROTOCOL
        });
    }
}

/// The next request; `None` at the end of stdin.
fn recv() -> Option<(u32, Request)> {
    match read_frame(&mut io::stdin().lock()) {
        Ok(Some((seq, payload))) => match Request::decode(&payload) {
            Ok(r) => Some((seq, r)),
            Err(e) => die(exit::PROTOCOL, &format!("bad request: {e}")),
        },
        Ok(None) => None,
        Err(e) => die(exit::PROTOCOL, &format!("cannot read a request: {e}")),
    }
}

fn die(code: i32, why: &str) -> ! {
    let _ = writeln!(io::stderr(), "ddi-video-helper: {why}");
    std::process::exit(code)
}

/// FFmpeg's reads: asked of the parent, answered through the I/O region.
unsafe extern "C" fn read_movie(
    opaque: *mut c_void,
    offset: u64,
    buf: *mut u8,
    len: c_int,
) -> c_int {
    let id = opaque as usize as u32;
    let want = (len.max(0) as usize).min(MAX_READ);
    if want == 0 {
        return 0;
    }
    send(&Reply::Read {
        id,
        offset,
        len: want as u32,
    });
    let Some((seq, request)) = recv() else {
        std::process::exit(0);
    };
    if seq != state().seq.load(Ordering::Relaxed) {
        die(exit::PROTOCOL, "a data frame with another sequence number");
    }
    let got = match request {
        Request::Data { len } => len as usize,
        _ => die(exit::PROTOCOL, "a request where movie bytes were due"),
    };
    if got > want {
        die(exit::PROTOCOL, "more movie bytes than asked for");
    }
    let s = state();
    let io = &s.arena.layout.io;
    let src = s.arena.slice(&(io.start..io.start + got));
    // SAFETY: FFmpeg's buffer holds `len` bytes; `got` is at most that.
    unsafe { std::ptr::copy_nonoverlapping(src.as_ptr(), buf, got) };
    got as c_int
}

enum Handle {
    Video(*mut ffi::Video),
    Demux(*mut ffi::Demux),
}

fn c_str(p: *const std::ffi::c_char, cap: usize) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: the shim's strings are NUL-terminated.
    let s = unsafe { CStr::from_ptr(p) }.to_bytes();
    String::from_utf8_lossy(&s[..s.len().min(cap)]).into_owned()
}

/// A C string of a name, with its NULs (none expected) dropped.
fn c_name(name: &str) -> CString {
    CString::new(name.replace('\0', "")).expect("no NUL")
}

/// Copies `n` bytes at `p` into `region` (the shim bounds `n` to the
/// region's size; checked again here).
fn copy_into(arena: &Arena, region: &std::ops::Range<usize>, p: *const u8, n: usize) -> Option<()> {
    if n > region.len() || (n > 0 && p.is_null()) {
        return None;
    }
    if n > 0 {
        let dst = arena.slice_mut(&(region.start..region.start + n));
        // SAFETY: the shim's buffer holds `n` bytes.
        unsafe { std::ptr::copy_nonoverlapping(p, dst.as_mut_ptr(), n) };
    }
    Some(())
}

fn serve(threads_default: u32) -> ! {
    let s = state();
    // Hello first.
    let Some((seq, hello)) = recv() else {
        std::process::exit(0);
    };
    s.seq.store(seq, Ordering::Relaxed);
    let threads = match hello {
        Request::Hello {
            version,
            arena_len,
            threads,
        } => {
            if version != VERSION {
                send(&Reply::Error {
                    message: format!("protocol version {version}, this helper speaks {VERSION}"),
                });
                die(exit::PROTOCOL, "protocol version mismatch");
            }
            if arena_len != s.arena.len() as u64 {
                send(&Reply::Error {
                    message: "arena length mismatch".into(),
                });
                die(exit::PROTOCOL, "arena length mismatch");
            }
            if threads == 0 {
                threads_default
            } else {
                threads
            }
        }
        _ => {
            send(&Reply::Error {
                message: "the first request must be Hello".into(),
            });
            die(exit::PROTOCOL, "no Hello")
        }
    };
    let threads = threads.clamp(1, 16);
    // SAFETY: plain setters on the shim before any movie is open.
    unsafe {
        ffi::ddi_set_threads(threads as c_int);
        ffi::ddi_io_set_read(read_movie);
        // A single allocation past this fails, whatever a hostile file asks.
        ffi::av_max_alloc(512 << 20);
        // Errors only (AV_LOG_ERROR), for decoding and demuxing alike.
        ffi::av_log_set_level(16);
    }
    send(&Reply::Hello {
        version: VERSION,
        decoder: format!("FFmpeg {}", env!("DDI_FFMPEG_VERSION")),
        sandbox: SANDBOX.get().copied().unwrap_or("none").to_string(),
        threads,
    });

    let mut handles: BTreeMap<u32, Handle> = BTreeMap::new();
    let layout = &s.arena.layout;
    while let Some((seq, request)) = recv() {
        s.seq.store(seq, Ordering::Relaxed);
        let unknown = |id: u32| -> ! {
            send(&Reply::Error {
                message: format!("unknown handle {id}"),
            });
            die(exit::PROTOCOL, "unknown handle")
        };
        match request {
            Request::Hello { .. } => {
                send(&Reply::Error {
                    message: "Hello again".into(),
                });
                die(exit::PROTOCOL, "Hello twice");
            }
            Request::Data { .. } => {
                send(&Reply::Error {
                    message: "movie bytes nobody asked for".into(),
                });
                die(exit::PROTOCOL, "unexpected Data");
            }
            Request::Shutdown => {
                send(&Reply::Bye);
                std::process::exit(0);
            }
            Request::Open { id, size, name } => {
                if handles.len() >= MAX_OPEN || handles.contains_key(&id) {
                    send(&Reply::OpenFailed { id });
                    continue;
                }
                let hint = c_name(&name);
                // SAFETY: the shim copies the hint; the opaque is the id.
                let v =
                    unsafe { ffi::ddi_open_io(id as usize as *mut c_void, size, hint.as_ptr()) };
                if v.is_null() {
                    send(&Reply::OpenFailed { id });
                    continue;
                }
                handles.insert(id, Handle::Video(v));
                // SAFETY: a live handle.
                let (width, height, duration, codec) = unsafe {
                    (
                        ffi::ddi_width(v).max(0) as u32,
                        ffi::ddi_height(v).max(0) as u32,
                        ffi::ddi_duration(v),
                        c_str(ffi::ddi_codec(v), 64),
                    )
                };
                send(&Reply::Opened {
                    id,
                    width,
                    height,
                    duration,
                    codec,
                });
            }
            Request::Seek { id, t } | Request::SeekKey { id, t } => {
                let Some(Handle::Video(v)) = handles.get(&id) else {
                    unknown(id)
                };
                let key = matches!(request, Request::SeekKey { .. });
                // SAFETY: a live handle.
                let r = unsafe {
                    if key {
                        ffi::ddi_seek_key(*v, t)
                    } else {
                        ffi::ddi_seek(*v, t)
                    }
                };
                send(&Reply::Sought { id, ok: r == 0 });
            }
            Request::Next { id, slot } => {
                let Some(Handle::Video(v)) = handles.get(&id) else {
                    unknown(id)
                };
                let Some(range) = layout.frame(slot) else {
                    send(&Reply::Error {
                        message: format!("frame slot {slot} out of range"),
                    });
                    die(exit::PROTOCOL, "bad slot")
                };
                let (ptr, cap) = s.arena.raw(range);
                let start = Instant::now();
                // SAFETY: a live handle; the slot is ours to write.
                let pts = unsafe {
                    ffi::ddi_set_planes(*v, ptr, cap);
                    ffi::ddi_next(*v)
                };
                if pts == -1.0 {
                    // SAFETY: a live handle.
                    let last_time = unsafe { ffi::ddi_last_time(*v) };
                    send(&Reply::End { id, last_time });
                } else if pts < 0.0 {
                    send(&Reply::Bad { id });
                } else {
                    // SAFETY: a live handle.
                    let (width, height, colorspace, full_range) = unsafe {
                        (
                            ffi::ddi_width(*v).max(0) as u32,
                            ffi::ddi_height(*v).max(0) as u32,
                            ffi::ddi_colorspace(*v).max(0) as u32,
                            ffi::ddi_full_range(*v) != 0,
                        )
                    };
                    send(&Reply::Frame {
                        id,
                        slot,
                        pts,
                        width,
                        height,
                        colorspace,
                        full_range,
                        decode_us: start.elapsed().as_micros().min(u128::from(u32::MAX)) as u32,
                    });
                }
            }
            Request::Close { id } => {
                let Some(Handle::Video(v)) = handles.remove(&id) else {
                    unknown(id)
                };
                // SAFETY: a live handle, used no more.
                unsafe { ffi::ddi_close(v) };
                send(&Reply::Closed { id });
            }
            Request::DemuxOpen { id, size, name } => {
                if handles.len() >= MAX_OPEN || handles.contains_key(&id) {
                    send(&Reply::DemuxOpenFailed { id });
                    continue;
                }
                let hint = c_name(&name);
                // SAFETY: as for Open.
                let d = unsafe {
                    ffi::ddi_demux_open_io(id as usize as *mut c_void, size, hint.as_ptr())
                };
                if d.is_null() {
                    send(&Reply::DemuxOpenFailed { id });
                    continue;
                }
                handles.insert(id, Handle::Demux(d));
                // SAFETY: a live handle; the description is at most 4096 bytes.
                let info = unsafe { c_str(ffi::ddi_demux_info(d), 4096) };
                send(&Reply::DemuxOpened { id, info });
            }
            Request::DemuxConfig { id } => {
                let Some(Handle::Demux(d)) = handles.get(&id) else {
                    unknown(id)
                };
                // SAFETY: a live handle.
                let (p, n) = unsafe {
                    (
                        ffi::ddi_demux_config(*d),
                        ffi::ddi_demux_config_size(*d) as usize,
                    )
                };
                if copy_into(&s.arena, &layout.config, p, n).is_none() {
                    send(&Reply::DemuxError {
                        id,
                        message: "configuration exceeds its region".into(),
                    });
                    continue;
                }
                send(&Reply::DemuxConfig { id, len: n as u32 });
            }
            Request::DemuxNext { id } => {
                let Some(Handle::Demux(d)) = handles.get(&id) else {
                    unknown(id)
                };
                // SAFETY: a live handle.
                match unsafe { ffi::ddi_demux_next(*d) } {
                    0 => send(&Reply::DemuxEnd { id }),
                    1 => {
                        // SAFETY: a live handle with a packet; the record is 72 bytes.
                        let record = unsafe {
                            std::slice::from_raw_parts(
                                ffi::ddi_demux_packet_info(*d),
                                PACKET_RECORD,
                            )
                        }
                        .to_vec();
                        let data_size =
                            u32::from_le_bytes(record[8..12].try_into().unwrap()) as usize;
                        let side_size =
                            u32::from_le_bytes(record[12..16].try_into().unwrap()) as usize;
                        // SAFETY: a live handle with a packet.
                        let (data, side) =
                            unsafe { (ffi::ddi_demux_data(*d), ffi::ddi_demux_side_data(*d)) };
                        if copy_into(&s.arena, &layout.packet, data, data_size).is_none()
                            || copy_into(&s.arena, &layout.side, side, side_size).is_none()
                        {
                            send(&Reply::DemuxError {
                                id,
                                message: "packet exceeds its region".into(),
                            });
                            continue;
                        }
                        send(&Reply::Packet { id, record });
                    }
                    _ => {
                        // SAFETY: a live handle; the message is at most 256 bytes.
                        let message = unsafe { c_str(ffi::ddi_demux_error(*d), 256) };
                        send(&Reply::DemuxError { id, message });
                    }
                }
            }
            Request::DemuxSeek { id, timestamp } => {
                let Some(Handle::Demux(d)) = handles.get(&id) else {
                    unknown(id)
                };
                // SAFETY: a live handle.
                if unsafe { ffi::ddi_demux_seek(*d, timestamp) } == 0 {
                    send(&Reply::DemuxSought { id });
                } else {
                    // SAFETY: as above.
                    let message = unsafe { c_str(ffi::ddi_demux_error(*d), 256) };
                    send(&Reply::DemuxError { id, message });
                }
            }
            Request::DemuxIndexCount { id } => {
                let Some(Handle::Demux(d)) = handles.get(&id) else {
                    unknown(id)
                };
                // SAFETY: a live handle.
                let count = unsafe { ffi::ddi_demux_index_count(*d) };
                send(&Reply::DemuxIndexCount { id, count });
            }
            Request::DemuxIndex { id, first, count } => {
                let Some(Handle::Demux(d)) = handles.get(&id) else {
                    unknown(id)
                };
                // SAFETY: a live handle; the shim refuses counts over its page.
                let n = unsafe { ffi::ddi_demux_index(*d, first, count) };
                if n < 0 || n as u32 > count {
                    send(&Reply::DemuxError {
                        id,
                        message: "invalid seek index page".into(),
                    });
                    continue;
                }
                // SAFETY: the page holds `n` entries of 32 bytes.
                let entries = unsafe {
                    std::slice::from_raw_parts(
                        ffi::ddi_demux_index_data(*d),
                        n as usize * INDEX_RECORD,
                    )
                }
                .to_vec();
                send(&Reply::DemuxIndex { id, entries });
            }
            Request::DemuxClose { id } => {
                let Some(Handle::Demux(d)) = handles.remove(&id) else {
                    unknown(id)
                };
                // SAFETY: a live handle, used no more.
                unsafe { ffi::ddi_demux_close(d) };
                send(&Reply::Closed { id });
            }
        }
    }
    std::process::exit(0)
}

static SANDBOX: OnceLock<&'static str> = OnceLock::new();

fn confine() {
    match sandbox::enter() {
        Ok(name) => {
            let _ = SANDBOX.set(name);
        }
        Err(e) => die(exit::SANDBOX, &format!("cannot enter the sandbox: {e}")),
    }
    if let Err(e) = sandbox::self_test() {
        die(exit::SANDBOX, &format!("the sandbox does not hold: {e}"));
    }
}

fn usage() -> ! {
    die(
        exit::USAGE,
        "usage: ddi-video-helper --serve --arena <descriptor> --arena-len <bytes> [--threads N] | --sandbox-check | --version",
    )
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| -> Option<&str> {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
    };
    if args.iter().any(|a| a == "--version") {
        println!(
            "ddi-video-helper {} (FFmpeg {}, protocol {VERSION}, arena {} bytes)",
            env!("CARGO_PKG_VERSION"),
            env!("DDI_FFMPEG_VERSION"),
            Layout::LEN
        );
        return;
    }
    if args.iter().any(|a| a == "--sandbox-check") {
        confine();
        println!(
            "sandbox {} holds (files, directories, network and processes denied; threads work)",
            SANDBOX.get().copied().unwrap_or("none")
        );
        return;
    }
    if !args.iter().any(|a| a == "--serve") {
        usage();
    }
    let arena: u64 = flag("--arena")
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| usage());
    let len: usize = flag("--arena-len")
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| usage());
    let threads: u32 = flag("--threads").and_then(|v| v.parse().ok()).unwrap_or(1);
    #[cfg(unix)]
    let mapped = Arena::map(arena as i32, len);
    #[cfg(windows)]
    let mapped = Arena::map(arena, len);
    let arena = match mapped {
        Ok(a) => a,
        Err(e) => die(exit::ARENA, &e),
    };
    // Everything that needs the outside world is done: close the door.
    confine();
    STATE
        .set(State {
            arena,
            seq: AtomicU32::new(0),
        })
        .ok()
        .expect("one state");
    serve(threads)
}
