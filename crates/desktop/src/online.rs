//! Invite-only online rooms on desktop
//! (`docs/plans/multiplayer-prototype-progress.md`, M5c), behind the
//! prototype's switch: the UI core's online state (`ddi_ui::state::online`)
//! kept for the run, its HTTP through ureq on a thread a request (as the
//! share client does, `crate::share::agent`), a room's socket through
//! tungstenite on a thread of its own, and the session kept beside the
//! settings in the config folder (`online-session`, never the share token).
//!
//! Signing in is the server's attempt: the system browser opens on it
//! (`crate::os::open_url`) and the attempt is long-polled until the
//! browser is done; a refreshed session (`x-ddi-session`) replaces the
//! kept one. `--join LINK|INVITE` opens an invite's page. A room's stage
//! songs (M5d) go through `crate::room`: the host's pick uploaded as the
//! room's share under its grant, the others' read from it, on threads
//! whose news the room client reads (`RoomClient::set_inbox`).

use std::cell::RefCell;
use std::io::ErrorKind;
use std::net::TcpStream;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::Duration;

use ddi_room::{ClientId, RoomId, Transport, TransportEvent};
use ddi_ui::state::online::api::{ApiRequest, ApiResponse, Method};
use ddi_ui::state::online::{Http, Online, OnlineConfig, Platform, SharedOnline};
use ddi_ui::state::audience::{AudienceClient, SharedAudience};
use ddi_ui::state::room::{RoomClient, SharedRoom};

thread_local! {
    static ONLINE: RefCell<Option<SharedOnline>> = const { RefCell::new(None) };
    static ROOM: RefCell<Option<(String, SharedRoom)>> = const { RefCell::new(None) };
    /// The room watched with its audience: its link and seat's client.
    static AUDIENCE: RefCell<Option<(String, SharedAudience)>> = const { RefCell::new(None) };
    /// `--join`'s invite, for the first screens.
    static JOIN: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Where the session is kept: beside the settings.
fn token_file() -> Option<PathBuf> {
    crate::paths::config_dir().map(|d| d.join("online-session"))
}

fn load_token() -> Option<String> {
    let t = std::fs::read_to_string(token_file()?).ok()?;
    let t = t.trim();
    (!t.is_empty()).then(|| t.to_string())
}

fn save_token(token: Option<&str>) {
    let Some(path) = token_file() else {
        return;
    };
    match token {
        Some(t) => {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Err(e) = write_private(&path, t) {
                eprintln!("online: keeping the session: {e}");
            }
        }
        None => {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// The session's file, readable by its owner alone where the system has
/// such modes.
fn write_private(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(text.as_bytes())
}

/// The game's web page, where invite links open: `DDI_SITE_URL`, else the
/// API's base without its `api`.
pub(crate) fn site() -> String {
    if let Ok(s) = std::env::var("DDI_SITE_URL")
        && !s.is_empty()
    {
        return s;
    }
    let base = crate::share::base();
    let site = base.trim_end_matches('/').trim_end_matches("/api");
    format!("{site}/")
}

/// `--join`'s invite (a link or the invite itself), for the first
/// screens.
pub fn set_join(text: &str) -> bool {
    match ddi_ui::state::online::api::invite_in(text) {
        Some(invite) => {
            JOIN.with_borrow_mut(|j| *j = Some(invite));
            true
        }
        None => false,
    }
}

pub fn take_join() -> Option<String> {
    JOIN.with_borrow_mut(Option::take)
}

/// The signed-in player's display name, where this session's online
/// state exists and is signed in (never made for the asking): CPU
/// practice's own row shows it with a "You" badge.
pub fn signed_in_name() -> Option<String> {
    ONLINE.with(|o| {
        o.borrow()
            .as_ref()
            .and_then(|o| o.borrow().me().map(|me| me.display_name.clone()))
    })
}

/// The run's online state, made at first (the kept session with it).
pub fn current() -> SharedOnline {
    ONLINE.with(|o| {
        o.borrow_mut()
            .get_or_insert_with(|| {
                Online::new(
                    Box::new(ThreadHttp::new()),
                    OnlineConfig {
                        platform: Platform::Desktop,
                        site: site(),
                    },
                )
                .with_token(load_token(), Rc::new(save_token))
                .shared()
            })
            .clone()
    })
}

/// A fresh client id for this run (the server keeps each one's last
/// sequence number; not a secret).
fn client_id() -> ClientId {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    ClientId::new(format!("desktop-{:x}-{:x}", n, std::process::id()))
}

/// Online room `id`'s client, made (and joined) at first.
pub fn room(id: &str) -> SharedRoom {
    if let Some(r) =
        ROOM.with_borrow(|r| r.as_ref().filter(|(i, _)| i == id).map(|(_, r)| r.clone()))
    {
        return r;
    }
    let room = Rc::new(RefCell::new(RoomClient::online(
        current(),
        id,
        Box::new(SocketTransport::default()),
        client_id(),
        crate::clock::epoch_ms().max(0.0) as u64,
    )));
    // Its uploads' and downloads' threads report here.
    room.borrow_mut().set_inbox(crate::room::inbox());
    ROOM.with_borrow_mut(|r| *r = Some((id.into(), room.clone())));
    room
}

/// Leave room: forgotten.
pub fn discard() {
    ROOM.with_borrow_mut(|r| *r = None);
}

/// The room watched with its audience by `link`
/// (`docs/plans/multiplayer-audience.md`): its seat's client, made at
/// first with the invite page's `ticket` (else asked for), its stages'
/// songs read from the room's share on threads (`crate::room::prepare_audience`).
pub fn audience(link: &str, ticket: Option<String>) -> SharedAudience {
    if let Some(a) =
        AUDIENCE.with_borrow(|a| a.as_ref().filter(|(l, _)| l == link).map(|(_, a)| a.clone()))
    {
        return a;
    }
    discard_audience();
    let a = Rc::new(RefCell::new(AudienceClient::new(
        current(),
        link,
        Box::new(SocketTransport::audience(None)),
        ticket,
        crate::clock::epoch_ms().max(0.0) as u64,
    )));
    a.borrow_mut().set_inbox(crate::room::audience_inbox());
    AUDIENCE.with_borrow_mut(|x| *x = Some((link.into(), a.clone())));
    a
}

/// Stop watching: the seat forgotten (its socket closed).
pub fn discard_audience() {
    if let Some((_, a)) = AUDIENCE.with_borrow_mut(Option::take) {
        a.borrow_mut().leave();
    }
}

/// HTTP on a thread a request; answers through a channel.
pub struct ThreadHttp {
    base: String,
    tx: Sender<(u64, ApiResponse)>,
    rx: Receiver<(u64, ApiResponse)>,
}

impl ThreadHttp {
    pub fn new() -> Self {
        Self::with_base(crate::share::base())
    }

    /// Against the server at `base` (`…/ditoditoinfinito/api`).
    pub fn with_base(base: String) -> Self {
        let (tx, rx) = channel();
        Self { base, tx, rx }
    }
}

impl Default for ThreadHttp {
    fn default() -> Self {
        Self::new()
    }
}

impl Http for ThreadHttp {
    fn start(&mut self, id: u64, request: ApiRequest) {
        let tx = self.tx.clone();
        let url = format!("{}{}", self.base.trim_end_matches('/'), request.path);
        std::thread::Builder::new()
            .name("ddi-online-http".into())
            .spawn(move || {
                let _ = tx.send((id, send(&url, &request)));
            })
            .ok();
    }

    fn poll(&mut self) -> Vec<(u64, ApiResponse)> {
        self.rx.try_iter().collect()
    }
}

fn send(url: &str, request: &ApiRequest) -> ApiResponse {
    let agent = crate::share::agent();
    let auth = request
        .poll
        .as_ref()
        .map(|p| format!("Poll {p}"))
        .or_else(|| request.bearer.as_ref().map(|b| format!("Bearer {b}")));
    let result = match (request.method, &request.body) {
        (Method::Get, _) => {
            let mut r = agent.get(url);
            if let Some(a) = &auth {
                r = r.header("authorization", a);
            }
            r.call()
        }
        (Method::Delete, _) => {
            let mut r = agent.delete(url);
            if let Some(a) = &auth {
                r = r.header("authorization", a);
            }
            r.call()
        }
        (Method::Post, body) => {
            let mut r = agent.post(url).header("content-type", "application/json");
            if let Some(a) = &auth {
                r = r.header("authorization", a);
            }
            r.send(body.clone().unwrap_or_default())
        }
    };
    match result {
        Ok(mut r) => {
            let session = r
                .headers()
                .get("x-ddi-session")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let status = r.status().as_u16();
            let body = r.body_mut().read_to_string().unwrap_or_default();
            ApiResponse {
                status,
                body,
                session,
            }
        }
        Err(e) => {
            eprintln!("online: {} {}: {e}", request.method.as_str(), request.path);
            ApiResponse::unreachable()
        }
    }
}

/// A room's socket on a thread of its own: tungstenite over the system's
/// TLS (native-tls, as the HTTP), `<api>/sockets/<id>/ws?ticket=`; text
/// both ways and binary (live play's batches and frames).
#[derive(Default)]
pub struct SocketTransport {
    base: Option<String>,
    /// An audience seat's socket (`/sockets/<id>/audience/ws`,
    /// `docs/plans/multiplayer-audience.md`), not a member's.
    audience: bool,
    socket: Option<Running>,
}

/// What the game puts on the socket.
enum Outgoing {
    Text(String),
    Binary(Vec<u8>),
    Close,
}

struct Running {
    out: Sender<Outgoing>,
    events: Receiver<TransportEvent>,
}

impl SocketTransport {
    /// Against the server at `base` rather than the configured one.
    pub fn with_base(base: String) -> Self {
        Self {
            base: Some(base),
            audience: false,
            socket: None,
        }
    }

    /// An audience seat's socket (against the server at `base`, else the
    /// configured one).
    pub fn audience(base: Option<String>) -> Self {
        Self {
            base,
            audience: true,
            socket: None,
        }
    }

    fn url(&self, room: &RoomId, ticket: &str) -> String {
        let base = self.base.clone().unwrap_or_else(crate::share::base);
        let base = base.trim_end_matches('/');
        let base = if let Some(rest) = base.strip_prefix("https://") {
            format!("wss://{rest}")
        } else if let Some(rest) = base.strip_prefix("http://") {
            format!("ws://{rest}")
        } else {
            base.to_string()
        };
        let path = if self.audience {
            ddi_ui::state::online::api::audience_socket_path(room.as_str(), ticket)
        } else {
            ddi_ui::state::online::api::socket_path(room.as_str(), ticket)
        };
        format!("{base}{path}")
    }
}

impl Transport for SocketTransport {
    fn open(&mut self, room: &RoomId, ticket: &str) {
        self.close();
        let url = self.url(room, ticket);
        let (out_tx, out_rx) = channel();
        let (ev_tx, ev_rx) = channel();
        std::thread::Builder::new()
            .name("ddi-room-socket".into())
            .spawn(move || run_socket(&url, &out_rx, &ev_tx))
            .ok();
        self.socket = Some(Running {
            out: out_tx,
            events: ev_rx,
        });
    }

    fn send(&mut self, text: String) {
        if let Some(s) = &self.socket {
            let _ = s.out.send(Outgoing::Text(text));
        }
    }

    fn send_binary(&mut self, bytes: Vec<u8>) {
        if let Some(s) = &self.socket {
            let _ = s.out.send(Outgoing::Binary(bytes));
        }
    }

    fn binary(&self) -> bool {
        true
    }

    fn close(&mut self) {
        if let Some(s) = self.socket.take() {
            let _ = s.out.send(Outgoing::Close);
        }
    }

    fn poll(&mut self) -> Vec<TransportEvent> {
        let Some(s) = &self.socket else {
            return Vec::new();
        };
        let events: Vec<TransportEvent> = s.events.try_iter().collect();
        if events
            .iter()
            .any(|e| matches!(e, TransportEvent::Closed { .. }))
        {
            self.socket = None;
        }
        events
    }
}

impl Drop for SocketTransport {
    fn drop(&mut self) {
        self.close();
    }
}

/// How long the socket's thread waits for a message before it looks at
/// what the game wants sent: short, as a live ping waits this long at most
/// on its way out (the clock's estimate takes the round trip).
const READ_WAIT: Duration = Duration::from_millis(10);

/// The socket's thread: connect, then read (a short timeout at a time)
/// and write what comes from the game, until either side closes.
/// [`Outgoing::Close`] from the game closes it.
fn run_socket(url: &str, out: &Receiver<Outgoing>, events: &Sender<TransportEvent>) {
    use tungstenite::Message;
    use tungstenite::stream::MaybeTlsStream;
    let closed = |code: u16| {
        let _ = events.send(TransportEvent::Closed { code });
    };
    let (mut ws, _) = match tungstenite::connect(url) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("online: the room's socket: {e}");
            return closed(1006);
        }
    };
    let tcp: Option<&TcpStream> = match ws.get_ref() {
        MaybeTlsStream::Plain(s) => Some(s),
        MaybeTlsStream::NativeTls(s) => Some(s.get_ref()),
        _ => None,
    };
    if let Some(tcp) = tcp {
        let _ = tcp.set_read_timeout(Some(READ_WAIT));
        let _ = tcp.set_nodelay(true);
    }
    let _ = events.send(TransportEvent::Opened);
    loop {
        loop {
            match out.try_recv() {
                Ok(Outgoing::Text(text)) => {
                    if ws.send(Message::text(text)).is_err() {
                        return closed(1006);
                    }
                }
                Ok(Outgoing::Binary(bytes)) => {
                    if ws.send(Message::binary(bytes)).is_err() {
                        return closed(1006);
                    }
                }
                Ok(Outgoing::Close) | Err(TryRecvError::Disconnected) => {
                    let _ = ws.close(None);
                    let _ = ws.flush();
                    return;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        match ws.read() {
            Ok(Message::Text(t)) => {
                let _ = events.send(TransportEvent::Text(t.to_string()));
            }
            Ok(Message::Binary(b)) => {
                let _ = events.send(TransportEvent::Binary(b.to_vec()));
            }
            Ok(Message::Close(frame)) => {
                let code = frame.map_or(1005, |f| u16::from(f.code));
                let _ = ws.flush();
                return closed(code);
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return closed(1005);
            }
            Err(e) => {
                eprintln!("online: the room's socket: {e}");
                return closed(1006);
            }
        }
    }
}
