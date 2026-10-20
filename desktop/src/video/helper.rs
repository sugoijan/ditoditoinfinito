//! The game's side of `ddi-video-helper` (`video/helper`,
//! `ddi_video_protocol`): starting the child with the arena and nothing
//! else, one request at a time with a deadline, the movie bytes it asks
//! for served from here through the arena's I/O region (a 64 KiB
//! read-ahead window per movie, as the web worker reads blobs), its
//! stderr shown as the decoder's messages, and the child killed when the
//! client is dropped or any of this goes wrong. Everything the child says
//! is checked: sequence numbers, lengths, handles, slot numbers.

use super::{Crash, MAX_LOG_LINE, MAX_LOG_LINES, MovieFile, READ_AHEAD, arena::SharedArena};
use ddi_video_protocol::{Layout, MAX_READ, Reply, Request, VERSION, read_frame, write_frame};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// What the helper said in its `Hello`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelperInfo {
    pub decoder: String,
    pub sandbox: String,
    pub threads: u32,
}

/// A movie the helper may read: its file, a read-ahead window into it.
pub(super) struct OpenFile {
    file: std::fs::File,
    /// Where its bytes start in `file` (a zip's stored entry).
    base: u64,
    size: u64,
    cache: Vec<u8>,
    cache_at: u64,
}

impl OpenFile {
    pub fn open(movie: &MovieFile) -> std::io::Result<OpenFile> {
        let file = std::fs::File::open(&movie.path)?;
        let len = file.metadata()?.len();
        let (base, size) = match movie.range {
            Some((at, n)) => (at.min(len), n.min(len.saturating_sub(at.min(len)))),
            None => (0, len),
        };
        Ok(OpenFile {
            file,
            base,
            size,
            cache: Vec::new(),
            cache_at: 0,
        })
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    /// Bytes `[pos, pos + n)`, through a read-ahead window.
    pub fn read_at(&mut self, pos: u64, n: usize) -> std::io::Result<&[u8]> {
        let end = self.size.min(pos.saturating_add(n as u64));
        if pos >= end {
            return Ok(&[]);
        }
        let inside = pos >= self.cache_at && end <= self.cache_at + self.cache.len() as u64;
        if !inside {
            let to = self.size.min(pos.saturating_add(n.max(READ_AHEAD) as u64));
            self.cache.resize((to - pos) as usize, 0);
            self.file.seek(SeekFrom::Start(self.base + pos))?;
            self.file.read_exact(&mut self.cache)?;
            self.cache_at = pos;
        }
        let from = (pos - self.cache_at) as usize;
        Ok(&self.cache[from..from + (end - pos) as usize])
    }
}

/// A frame read from the helper: `(sequence number, payload)`, none at
/// the end of its stdout, or the read's error.
type Framed = std::io::Result<Option<(u32, Vec<u8>)>>;

/// One running helper.
pub struct Helper {
    child: Child,
    stdin: Box<dyn Write + Send>,
    replies: Receiver<Framed>,
    pub arena: SharedArena,
    files: HashMap<u32, OpenFile>,
    seq: u32,
    deadline: Duration,
    pub info: HelperInfo,
    /// A frame slot not handed to the child in the last `Next`.
    pub slot: u32,
}

/// The process, as each OS runs it.
struct Child {
    #[cfg(unix)]
    inner: std::process::Child,
    #[cfg(windows)]
    inner: windows_spawn::Process,
}

impl Child {
    fn kill(&mut self) {
        #[cfg(unix)]
        {
            let _ = self.inner.kill();
            let _ = self.inner.wait();
        }
        #[cfg(windows)]
        {
            self.inner.kill();
        }
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        // Closing stdin would let it exit on its own; killing is certain,
        // and a stuck decoder is the case this is for.
        self.child.kill();
    }
}

/// How many decoder threads to grant: a share of the machine, never all
/// of it (the game draws and mixes too); `DDI_VIDEO_THREADS` overrides
/// (1 decodes exactly as StepMania and the web do).
pub fn threads() -> u32 {
    if let Some(n) = std::env::var("DDI_VIDEO_THREADS")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
    {
        return n.clamp(1, 16);
    }
    let cores = std::thread::available_parallelism().map_or(2, |n| n.get() as u32);
    (cores / 2).clamp(1, 4)
}

impl Helper {
    /// Starts the helper at `path` and completes its handshake; `deadline`
    /// bounds every request (the handshake too).
    pub fn spawn(path: &Path, deadline: Duration, threads: u32) -> Result<Helper, String> {
        let arena = SharedArena::create()
            .map_err(|e| format!("cannot create the video decoder's shared memory: {e}"))?;
        let (child, stdin, stdout, stderr) = spawn_child(path, &arena)?;
        // Bounded: a flooding helper is not read into memory.
        let (tx, replies) = mpsc::sync_channel::<Framed>(4);
        let mut stdout = stdout;
        std::thread::Builder::new()
            .name("ddi-video-helper-read".into())
            .spawn(move || {
                loop {
                    let r = read_frame(&mut stdout);
                    let end = !matches!(r, Ok(Some(_)));
                    if tx.send(r).is_err() || end {
                        break;
                    }
                }
            })
            .map_err(|e| format!("cannot start the decoder's reader: {e}"))?;
        let mut stderr = stderr;
        std::thread::Builder::new()
            .name("ddi-video-helper-log".into())
            .spawn(move || {
                let mut pending = String::new();
                let mut buffer = [0u8; 4096];
                loop {
                    match stderr.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            pending.push_str(&String::from_utf8_lossy(&buffer[..n]));
                            log_lines(&mut pending);
                        }
                    }
                }
                if !pending.trim().is_empty() {
                    pending.push('\n');
                    log_lines(&mut pending);
                }
            })
            .map_err(|e| format!("cannot start the decoder's log reader: {e}"))?;
        let mut helper = Helper {
            child,
            stdin,
            replies,
            arena,
            files: HashMap::new(),
            seq: 0,
            deadline,
            info: HelperInfo {
                decoder: String::new(),
                sandbox: String::new(),
                threads: 0,
            },
            slot: 0,
        };
        let hello = helper
            .request(&Request::Hello {
                version: VERSION,
                arena_len: Layout::LEN as u64,
                threads,
            })
            .map_err(|Crash(e)| format!("the video decoder does not start: {e}"))?;
        match hello {
            Reply::Hello {
                version,
                decoder,
                sandbox,
                threads,
            } if version == VERSION => {
                helper.info = HelperInfo {
                    decoder,
                    sandbox,
                    threads,
                };
                Ok(helper)
            }
            Reply::Hello { version, .. } => Err(format!(
                "the video decoder speaks protocol {version}, this build {VERSION}"
            )),
            other => Err(format!("the video decoder answered {other:?} to Hello")),
        }
    }

    /// The helper may read `movie` as handle `id` from now on.
    pub fn register(&mut self, id: u32, movie: &MovieFile) -> std::io::Result<u64> {
        let file = OpenFile::open(movie)?;
        let size = file.size();
        self.files.insert(id, file);
        Ok(size)
    }

    pub fn unregister(&mut self, id: u32) {
        self.files.remove(&id);
    }

    /// Sends `request` and waits for its reply, serving the helper's reads
    /// meanwhile; a deadline, a dead helper, a malformed or out-of-order
    /// reply or a `Reply::Error` is a [`Crash`] (the caller drops the
    /// helper, which kills it).
    pub fn request(&mut self, request: &Request) -> Result<Reply, Crash> {
        self.seq = self.seq.wrapping_add(1);
        let payload = request.encode().map_err(|e| Crash(e.to_string()))?;
        write_frame(&mut self.stdin, self.seq, &payload)
            .map_err(|e| Crash(format!("the video decoder stopped taking requests: {e}")))?;
        let until = Instant::now() + self.deadline;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            let (seq, payload) = match self.replies.recv_timeout(left) {
                Ok(Ok(Some(frame))) => frame,
                Ok(Ok(None)) => return Err(Crash("the video decoder exited".into())),
                Ok(Err(e)) => return Err(Crash(format!("the video decoder's reply: {e}"))),
                Err(RecvTimeoutError::Timeout) => {
                    return Err(Crash(format!(
                        "a call took longer than {:.1} s",
                        self.deadline.as_secs_f64()
                    )));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(Crash("the video decoder exited".into()));
                }
            };
            if seq != self.seq {
                return Err(Crash("a reply out of sequence".into()));
            }
            match Reply::decode(&payload).map_err(|e| Crash(e.to_string()))? {
                Reply::Read { id, offset, len } => self.serve_read(id, offset, len)?,
                Reply::Error { message } => {
                    return Err(Crash(format!("the video decoder refused: {message}")));
                }
                reply => return Ok(reply),
            }
        }
    }

    fn serve_read(&mut self, id: u32, offset: u64, len: u32) -> Result<(), Crash> {
        let len = len as usize;
        if len > MAX_READ || len > self.arena.layout.io.len() {
            return Err(Crash("a read past the I/O region".into()));
        }
        let Some(file) = self.files.get_mut(&id) else {
            return Err(Crash(format!("a read of unknown movie {id}")));
        };
        // A read error is the end of the file, as a truncated movie's is.
        let bytes = file.read_at(offset, len).unwrap_or(&[]);
        let n = bytes.len();
        let io = self.arena.layout.io.start;
        self.arena.slice_mut(&(io..io + n)).copy_from_slice(bytes);
        let payload = Request::Data { len: n as u32 }
            .encode()
            .map_err(|e| Crash(e.to_string()))?;
        write_frame(&mut self.stdin, self.seq, &payload)
            .map_err(|e| Crash(format!("the video decoder stopped taking data: {e}")))
    }

    /// The frame slot for the next `Next` (the other one than last time, so
    /// the planes just received are not written over while they are copied).
    pub fn next_slot(&mut self) -> u32 {
        self.slot = (self.slot + 1) % Layout::FRAME_SLOTS as u32;
        self.slot
    }
}

/// Prints whole lines of the helper's stderr as the decoder's messages,
/// a line cut at [`MAX_LOG_LINE`] bytes and no more than [`MAX_LOG_LINES`]
/// of them a process.
fn log_lines(pending: &mut String) {
    static LINES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    if pending.len() > MAX_LOG_LINE && !pending.contains('\n') {
        let mut cut = MAX_LOG_LINE;
        while !pending.is_char_boundary(cut) {
            cut -= 1;
        }
        pending.truncate(cut);
        pending.push('\n');
    }
    while let Some(nl) = pending.find('\n') {
        let line: String = pending.drain(..=nl).collect();
        let n = LINES.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if n < MAX_LOG_LINES {
            eprintln!("[video] {}", line.trim_end());
        } else if n == MAX_LOG_LINES {
            eprintln!("[video] further decoder messages are not shown");
        }
    }
}

type Spawned = (
    Child,
    Box<dyn Write + Send>,
    Box<dyn Read + Send>,
    Box<dyn Read + Send>,
);

/// Starts the helper with the arena on descriptor 3, stdio piped, no
/// environment; the helper confines itself before it reads anything.
#[cfg(unix)]
fn spawn_child(path: &Path, arena: &SharedArena) -> Result<Spawned, String> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let fd = arena.raw_fd();
    let mut command = Command::new(path);
    command
        .args(["--serve", "--arena", "3", "--arena-len"])
        .arg(Layout::LEN.to_string())
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // SAFETY: only async-signal-safe calls between fork and exec.
    unsafe {
        command.pre_exec(move || {
            if fd == 3 {
                let flags = libc::fcntl(3, libc::F_GETFD);
                if flags < 0 || libc::fcntl(3, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
            } else if libc::dup2(fd, 3) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("cannot start the video decoder ({}): {e}", path.display()))?;
    let stdin = child.stdin.take().ok_or("no pipe to the video decoder")?;
    let stdout = child
        .stdout
        .take()
        .ok_or("no pipe from the video decoder")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("no log pipe from the video decoder")?;
    Ok((
        Child { inner: child },
        Box::new(stdin),
        Box::new(stdout),
        Box::new(stderr),
    ))
}

#[cfg(windows)]
fn spawn_child(path: &Path, arena: &SharedArena) -> Result<Spawned, String> {
    let (process, stdin, stdout, stderr) = windows_spawn::spawn(path, arena.raw_handle())?;
    Ok((
        Child { inner: process },
        Box::new(stdin),
        Box::new(stdout),
        Box::new(stderr),
    ))
}

#[cfg(windows)]
pub(super) mod windows_spawn;
