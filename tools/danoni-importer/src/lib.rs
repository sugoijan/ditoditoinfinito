//! Native importer service. The application talks to an owned child over bounded
//! anonymous pipes; only the SDK owns durable evidence/catalog files.
mod job;
use ddi_danoni_export::{network::NetworkCancel, runtime::Installation};
use ddi_importer_protocol::{Capabilities, Event, Failure, Outcome, Paths, Request, VERSION};
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread::JoinHandle,
    time::Duration,
};

#[derive(Clone)]
struct Session {
    paths: Paths,
    installation: Arc<Installation>,
    origins: Arc<Mutex<BTreeMap<String, String>>>,
}
struct PendingOrigin {
    id: u64,
    origin: String,
    reply: mpsc::SyncSender<Option<String>>,
}
type Pending = Arc<Mutex<Option<PendingOrigin>>>;

#[derive(Clone)]
struct Emitter(Arc<Mutex<Box<dyn Write + Send>>>);
impl Emitter {
    fn send(&self, event: Event) -> io::Result<()> {
        let mut writer = self
            .0
            .lock()
            .map_err(|_| io::Error::other("Importer output lock poisoned"))?;
        ddi_importer_protocol::write(&mut *writer, &event)
    }
}
fn failure(code: &str, message: impl ToString) -> Failure {
    let mut message = message.to_string();
    if message.len() > 8192 {
        let mut end = 8192;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
        message.push('…');
    }
    Failure {
        code: code.into(),
        message,
    }
}
fn sdk_failure(error: ddi_danoni_export::Error) -> Failure {
    failure(error.code, error.message)
}

struct Report {
    id: u64,
    status: Outcome,
    summary: String,
    restore_ids: Vec<String>,
    details: Vec<String>,
}
impl Report {
    fn failed(id: u64, error: Failure) -> Self {
        Self {
            id,
            status: Outcome::Failed,
            summary: "DanOni import could not finish.".into(),
            restore_ids: Vec::new(),
            details: vec![format!("{}: {}", error.code, error.message)],
        }
    }
    fn event(self) -> Event {
        Event::Terminal {
            id: self.id,
            status: self.status,
            summary: self.summary,
            restore_ids: self.restore_ids,
            details: self.details,
        }
    }
}
enum Finished {
    Initialized(Result<Session, Failure>),
    Imported(Report),
}
struct Active {
    id: Option<u64>,
    cancel: NetworkCancel,
    pending: Pending,
    thread: JoinHandle<Finished>,
}

enum Input {
    Request(Request),
    Eof,
    Error(String),
}

/// The bundle layout is chosen by the packager, never by a song or IPC request.
pub fn runtime_path(executable: &Path) -> io::Result<PathBuf> {
    executable
        .parent()
        .and_then(Path::parent)
        .map(|root| root.join("runtime"))
        .ok_or_else(|| io::Error::other("Importer executable must live in bundle/bin"))
}
/// A relocation/package check: no network, user storage or captured work needed.
pub fn check_runtime(path: &Path) -> Result<(), Failure> {
    ddi_danoni_export::runtime::Installation::load(path)
        .map(|_| ())
        .map_err(sdk_failure)
}

/// Run until EOF/shutdown, cancelling and draining owned work first. Stdin's
/// decoder stays live while SDK work and origin approval wait on another thread.
pub fn serve(
    reader: impl Read + Send + 'static,
    writer: impl Write + Send + 'static,
    runtime: PathBuf,
) -> io::Result<()> {
    let output = Emitter(Arc::new(Mutex::new(Box::new(writer))));
    // A bounded command queue prevents an input flood from accumulating requests.
    let (tx, rx) = mpsc::sync_channel(8);
    let _reader = std::thread::Builder::new()
        .name("ddi-importer-input".into())
        .spawn(move || {
            let mut reader = reader;
            loop {
                let input = match ddi_importer_protocol::read::<Request>(&mut reader) {
                    Ok(Some(request)) => Input::Request(request),
                    Ok(None) => Input::Eof,
                    Err(e) => Input::Error(e.to_string()),
                };
                let stop = matches!(
                    input,
                    Input::Eof | Input::Error(_) | Input::Request(Request::Shutdown)
                );
                if tx.send(input).is_err() || stop {
                    break;
                }
            }
        })?;
    let mut hello = false;
    let mut attempted_init = false;
    let mut session: Option<Session> = None;
    let mut active: Option<Active> = None;
    let mut stopping = false;
    let mut highest_id = 0;
    let mut broken_output = false;
    loop {
        if active.as_ref().is_some_and(|job| job.thread.is_finished()) {
            let job = active.take().unwrap();
            let result = job.thread.join().unwrap_or_else(|_| match job.id {
                None => Finished::Initialized(Err(failure(
                    "importer-panic",
                    "Importer initialization stopped unexpectedly",
                ))),
                Some(id) => Finished::Imported(Report::failed(
                    id,
                    failure(
                        "importer-panic",
                        "Importer job stopped unexpectedly; reopen to verify/recover the catalog",
                    ),
                )),
            });
            let event = match result {
                Finished::Initialized(Ok(ready)) => {
                    session = Some(ready);
                    Event::Initialized {
                        ready: true,
                        error: None,
                    }
                }
                Finished::Initialized(Err(error)) => Event::Initialized {
                    ready: false,
                    error: Some(error),
                },
                Finished::Imported(report) => report.event(),
            };
            if output.send(event).is_err() {
                broken_output = true;
                stopping = true;
            }
        }
        if stopping && active.is_none() {
            break;
        }
        let input = if stopping {
            std::thread::sleep(Duration::from_millis(10));
            None
        } else {
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(input) => Some(input),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => Some(Input::Eof),
            }
        };
        let Some(input) = input else {
            continue;
        };
        let mut fatal = None;
        match input {
            Input::Eof | Input::Request(Request::Shutdown) => stopping = true,
            Input::Error(message) => {
                fatal = Some(failure("invalid-frame", message));
                stopping = true;
            }
            Input::Request(Request::Hello { version }) if !hello && !attempted_init => {
                if version != VERSION {
                    fatal = Some(failure(
                        "protocol-version",
                        format!("Importer protocol {VERSION} required"),
                    ));
                    stopping = true;
                } else {
                    hello = true;
                    let supported = cfg!(any(
                        target_os = "macos",
                        all(target_os = "linux", target_arch = "x86_64")
                    ));
                    if output
                        .send(Event::Hello {
                            version: VERSION,
                            capabilities: Capabilities {
                                url_import: supported,
                                offline: true,
                                origin_approval: true,
                            },
                        })
                        .is_err()
                    {
                        broken_output = true;
                        stopping = true;
                    }
                }
            }
            Input::Request(Request::Initialize { paths, origins }) if hello && !attempted_init => {
                attempted_init = true;
                let cancel = NetworkCancel::default();
                let token = cancel.clone();
                let emitter = output.clone();
                let runtime = runtime.clone();
                let pending = Arc::new(Mutex::new(None));
                let thread = std::thread::Builder::new()
                    .name("ddi-importer-init".into())
                    .spawn(move || {
                        Finished::Initialized(job::initialize(
                            paths, origins, &runtime, &token, &emitter,
                        ))
                    })?;
                active = Some(Active {
                    id: None,
                    cancel,
                    pending,
                    thread,
                });
            }
            Input::Request(Request::Import { id, url, offline }) if hello && attempted_init => {
                if id == 0 || id <= highest_id {
                    fatal = Some(failure(
                        "job-id",
                        "Import ids must be nonzero and strictly increasing",
                    ));
                    stopping = true;
                } else {
                    highest_id = id;
                    let report = if active.is_some() {
                        Some(Report::failed(
                            id,
                            failure("importer-busy", "Another importer job is running"),
                        ))
                    } else if session.is_none() {
                        Some(Report::failed(
                            id,
                            failure(
                                "importer-unavailable",
                                "Importer initialization failed; reinstall the complete matching application bundle",
                            ),
                        ))
                    } else {
                        None
                    };
                    if let Some(report) = report {
                        if output.send(report.event()).is_err() {
                            broken_output = true;
                            stopping = true;
                        }
                    } else {
                        let ready = session.as_ref().unwrap().clone();
                        let emitter = output.clone();
                        let cancel = NetworkCancel::default();
                        let token = cancel.clone();
                        let pending = Arc::new(Mutex::new(None));
                        let waiting = pending.clone();
                        let thread = std::thread::Builder::new()
                            .name("ddi-importer-job".into())
                            .spawn(move || {
                                Finished::Imported(job::import(
                                    ready, id, url, offline, &token, &waiting, &emitter,
                                ))
                            })?;
                        active = Some(Active {
                            id: Some(id),
                            cancel,
                            pending,
                            thread,
                        });
                    }
                }
            }
            Input::Request(Request::Cancel { id }) if hello => {
                if let Some(job) = &active
                    && job.id == Some(id)
                {
                    job.cancel.cancel();
                }
            }
            Input::Request(Request::OriginDecision {
                id,
                origin,
                decision,
            }) if hello => {
                if let Some(job) = &active
                    && job.id == Some(id)
                    && let Ok(mut waiting) = job.pending.lock()
                    && waiting
                        .as_ref()
                        .is_some_and(|p| p.id == id && p.origin == origin)
                {
                    if decision
                        .as_ref()
                        .is_some_and(|d| d.trim().is_empty() || d.len() > 4096)
                    {
                        fatal = Some(failure(
                            "origin-decision",
                            "Origin decisions must be nonempty and at most 4096 bytes",
                        ));
                        stopping = true;
                    } else if let Some(waiting) = waiting.take() {
                        let _ = waiting.reply.send(decision);
                    }
                }
            }
            Input::Request(_) => {
                fatal = Some(failure(
                    "protocol-order",
                    "Send Hello once, then Initialize once, before importing",
                ));
                stopping = true;
            }
        }
        if let Some(error) = fatal
            && output
                .send(Event::Fatal {
                    code: error.code,
                    message: error.message,
                })
                .is_err()
        {
            broken_output = true;
        }
        if stopping && let Some(job) = &active {
            job.cancel.cancel();
        }
    }
    if !broken_output {
        output.send(Event::Bye)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone)]
    pub(crate) struct Capture(pub Arc<Mutex<Vec<u8>>>);
    impl Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    pub(crate) fn output() -> (Emitter, Arc<Mutex<Vec<u8>>>) {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        (
            Emitter(Arc::new(Mutex::new(Box::new(Capture(bytes.clone()))))),
            bytes,
        )
    }
    fn transcript(requests: &[Request]) -> Vec<Event> {
        let mut input = Vec::new();
        for request in requests {
            ddi_importer_protocol::write(&mut input, request).unwrap();
        }
        let bytes = Arc::new(Mutex::new(Vec::new()));
        serve(
            io::Cursor::new(input),
            Capture(bytes.clone()),
            "/absent-runtime".into(),
        )
        .unwrap();
        let result = bytes.lock().unwrap().clone();
        let mut reader = result.as_slice();
        let mut events = Vec::new();
        while let Some(event) = ddi_importer_protocol::read(&mut reader).unwrap() {
            events.push(event);
        }
        events
    }
    #[test]
    fn hello_eof_and_shutdown_have_clean_terminal_frames() {
        for requests in [
            vec![Request::Hello { version: VERSION }],
            vec![Request::Hello { version: VERSION }, Request::Shutdown],
        ] {
            let events = transcript(&requests);
            assert!(matches!(
                events.first(),
                Some(Event::Hello {
                    version: VERSION,
                    ..
                })
            ));
            assert!(matches!(events.last(), Some(Event::Bye)));
            assert_eq!(events.len(), 2);
        }
    }
    #[test]
    fn incompatible_version_and_unnegotiated_commands_fail_closed() {
        for request in [
            Request::Hello {
                version: VERSION + 1,
            },
            Request::Import {
                id: 1,
                url: "https://example.invalid/work".into(),
                offline: false,
            },
        ] {
            let events = transcript(&[request]);
            assert!(matches!(events[0], Event::Fatal { .. }));
            assert_eq!(events.last(), Some(&Event::Bye));
        }
    }
    #[test]
    fn truncated_input_is_not_clean_eof() {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        serve(
            io::Cursor::new(vec![0, 0]),
            Capture(bytes.clone()),
            "/absent-runtime".into(),
        )
        .unwrap();
        let data = bytes.lock().unwrap().clone();
        let event = ddi_importer_protocol::read::<Event>(&mut data.as_slice())
            .unwrap()
            .unwrap();
        assert!(matches!(event, Event::Fatal {code, ..} if code == "invalid-frame"));
    }
    #[test]
    fn relocation_uses_bundle_relative_runtime() {
        assert_eq!(
            runtime_path(Path::new("/moved/bundle/bin/ddi-danoni-importer")).unwrap(),
            PathBuf::from("/moved/bundle/runtime")
        );
        assert!(check_runtime(Path::new("/nonexistent/ddi-test-runtime")).is_err());
    }
}
