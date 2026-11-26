//! A plugin's process and its three pipes: frames written on a thread of
//! their own (from a short queue: a plugin that stops reading fills it),
//! frames read on another (into a queue of two: a plugin that floods is
//! held back by its own pipe), and the last 32 KiB of its diagnostics.
//! The threads are never joined: a pipe a plugin's own children keep open
//! cannot hold the game up. The process is killed and reaped when the
//! connection is dropped.

use std::collections::VecDeque;
use std::io::Read as _;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ddi_source_protocol::frame::{self, WireError};
use ddi_source_protocol::{AppMessage, PluginMessage};

/// What the reader thread hands on: a message and its attachment, the
/// clean end of the plugin's output, or why it was refused.
pub(crate) type Wire = Result<Option<(PluginMessage, Vec<u8>)>, WireError>;

/// The diagnostics kept: their last bytes.
pub(crate) const STDERR_TAIL: usize = 32 << 10;

/// Frames waiting to be written to the plugin, at most.
const OUTGOING: usize = 16;

pub(crate) struct Connection {
    child: Child,
    outgoing: Option<SyncSender<Vec<u8>>>,
    pub(crate) incoming: Receiver<Wire>,
    stderr: Arc<Mutex<VecDeque<u8>>>,
    stderr_done: Arc<AtomicBool>,
    reaped: Option<ExitStatus>,
}

/// Why a message could not be handed to the plugin.
#[derive(Debug)]
pub(crate) enum SendProblem {
    /// It is not reading what it is sent.
    NotReading,
    /// Its input is closed.
    Closed,
}

impl Connection {
    /// Starts `executable` in `dir` at a lower priority than the game.
    pub(crate) fn spawn(executable: &Path, dir: &Path) -> std::io::Result<Connection> {
        let mut command = Command::new(executable);
        command
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            // BELOW_NORMAL_PRIORITY_CLASS | CREATE_NO_WINDOW.
            command.creation_flags(0x0000_4000 | 0x0800_0000);
        }
        let mut child = spawn(&mut command)?;
        lower_priority(&child);
        let missing = || std::io::Error::other("a pipe to the plugin is missing");
        let (Some(mut input), Some(output), Some(mut errors)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(missing());
        };
        let mut connection = Connection {
            child,
            outgoing: None,
            incoming: mpsc::sync_channel(1).1,
            stderr: Arc::new(Mutex::new(VecDeque::new())),
            stderr_done: Arc::new(AtomicBool::new(false)),
            reaped: None,
        };
        let (outgoing, to_write) = mpsc::sync_channel::<Vec<u8>>(OUTGOING);
        std::thread::Builder::new()
            .name("ddi-plugin-write".into())
            .spawn(move || {
                use std::io::Write as _;
                while let Ok(bytes) = to_write.recv() {
                    if input.write_all(&bytes).and_then(|()| input.flush()).is_err() {
                        break;
                    }
                }
            })?;
        connection.outgoing = Some(outgoing);
        let (read_tx, incoming) = mpsc::sync_channel::<Wire>(2);
        std::thread::Builder::new()
            .name("ddi-plugin-read".into())
            .spawn(move || {
                let mut output = std::io::BufReader::with_capacity(64 << 10, output);
                loop {
                    let message = frame::read_plugin(&mut output);
                    let end = !matches!(message, Ok(Some(_)));
                    if read_tx.send(message).is_err() || end {
                        break;
                    }
                }
            })?;
        connection.incoming = incoming;
        let tail = connection.stderr.clone();
        let done = connection.stderr_done.clone();
        std::thread::Builder::new()
            .name("ddi-plugin-stderr".into())
            .spawn(move || {
                let mut buffer = [0; 4096];
                loop {
                    match errors.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Ok(mut tail) = tail.lock() {
                                tail.extend(&buffer[..n]);
                                let over = tail.len().saturating_sub(STDERR_TAIL);
                                tail.drain(..over);
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(_) => break,
                    }
                }
                done.store(true, Ordering::Release);
            })?;
        Ok(connection)
    }

    /// Queues `message` for the plugin, never waiting.
    pub(crate) fn send(&self, message: &AppMessage) -> Result<(), SendProblem> {
        let bytes = frame::encode(message, &[]).map_err(|_| SendProblem::Closed)?;
        match self.outgoing.as_ref().map(|o| o.try_send(bytes)) {
            Some(Ok(())) => Ok(()),
            Some(Err(TrySendError::Full(_))) => Err(SendProblem::NotReading),
            _ => Err(SendProblem::Closed),
        }
    }

    /// Its exit status, once it has exited.
    pub(crate) fn exited(&mut self) -> Option<ExitStatus> {
        if self.reaped.is_none() {
            self.reaped = self.child.try_wait().ok().flatten();
        }
        self.reaped
    }

    /// Waits up to `within` for it to exit by itself.
    pub(crate) fn wait_exit(&mut self, within: Duration) -> Option<ExitStatus> {
        let until = Instant::now() + within;
        loop {
            if let Some(status) = self.exited() {
                return Some(status);
            }
            if Instant::now() >= until {
                return None;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Kills it (where it still runs) and reaps it.
    pub(crate) fn kill(&mut self) {
        if self.exited().is_none() {
            let _ = self.child.kill();
            self.reaped = self.child.wait().ok();
        }
        self.outgoing = None;
    }

    /// The last of what it wrote to its diagnostics, after waiting up to
    /// `settle` for their end.
    pub(crate) fn diagnostics(&self, settle: Duration) -> Option<String> {
        let until = Instant::now() + settle;
        while !self.stderr_done.load(Ordering::Acquire) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        let tail = self.stderr.lock().ok()?;
        let (a, b) = tail.as_slices();
        let text = String::from_utf8_lossy(&[a, b].concat()).trim().to_string();
        (!text.is_empty()).then_some(text)
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.kill();
    }
}

/// `command` started, tried again while the file is still being written
/// (a plugin copied in a moment ago).
fn spawn(command: &mut Command) -> std::io::Result<Child> {
    let mut tries = 0;
    loop {
        match command.spawn() {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && tries < 10 => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(10 * tries));
            }
            spawned => return spawned,
        }
    }
}

/// A plugin runs below the game's priority (`nice` 10), so a busy one
/// leaves the frames alone.
fn lower_priority(child: &Child) {
    #[cfg(unix)]
    {
        // SAFETY: `setpriority` takes plain integers and touches only the
        // scheduling of the process named by its id, our own child.
        unsafe {
            libc::setpriority(libc::PRIO_PROCESS, child.id() as libc::id_t, 10);
        }
    }
    #[cfg(not(unix))]
    let _ = child;
}
