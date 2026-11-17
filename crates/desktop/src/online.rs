//! Invite-only online rooms on desktop
//! (`docs/plans/multiplayer-prototype-progress.md`, M5c), behind no
//! switch since 2026-11-14: the UI core's online state (`ddi_ui::state::online`)
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
use ddi_ui::state::online::{Http, Kept, Online, OnlineConfig, Platform, SharedOnline};
use ddi_ui::state::audience::{AudienceClient, SharedAudience};
use ddi_ui::state::room::{RoomClient, SharedRoom};

thread_local! {
    /// The run's online state, the online room shown last and the room
    /// watched with its audience.
    static KEPT: Kept = const { Kept::new() };
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
    KEPT.with(Kept::signed_in_name)
}

/// The run's online state, made at first (the kept session with it).
pub fn current() -> SharedOnline {
    KEPT.with(|k| {
        k.online_or(|| {
            let online = session(
                crate::share::base(),
                site(),
                load_token(),
                Rc::new(save_token),
            );
            {
                let mut o = online.borrow_mut();
                o.set_client_kind(Rc::new(|| client_kind("desktop")));
                o.set_hidden(HIDDEN.get(), crate::clock::now_ms());
            }
            online
        })
    })
}

/// What the window draws with, for a room's usage report (`unknown`
/// before a window is made; a bot says `none`).
static GPU: std::sync::Mutex<&str> = std::sync::Mutex::new("unknown");

thread_local! {
    /// The window hidden (occluded or minimized).
    static HIDDEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The window's graphics backend, as the usage report names it.
pub fn set_gpu(backend: wgpu::Backend) {
    let gpu = match backend {
        wgpu::Backend::Metal => "metal",
        wgpu::Backend::Vulkan => "vulkan",
        wgpu::Backend::Dx12 => "dx12",
        wgpu::Backend::Gl => "gl",
        _ => "other",
    };
    if let Ok(mut g) = GPU.lock() {
        *g = gpu;
    }
}

/// The window hidden or shown: the online state told where there is one
/// (how long a room's socket lived hidden, for its usage report).
pub fn set_hidden(hidden: bool) {
    HIDDEN.set(hidden);
    if let Some(online) = KEPT.with(Kept::online) {
        online
            .borrow_mut()
            .set_hidden(hidden, crate::clock::now_ms());
    }
}

/// What kind of client this run is (`desktop`, or a bot's `bot`), for a
/// room's usage report (`ddi_room::client_stats`; sent only where
/// Options' Usage statistics is on).
pub fn client_kind(app: &'static str) -> ddi_room::client_stats::ClientKind {
    let system = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "other"
    };
    let gpu = if app == "bot" {
        "none"
    } else {
        GPU.lock().map_or("unknown", |g| *g)
    };
    ddi_room::client_stats::ClientKind {
        app,
        system,
        form: if app == "bot" { "unknown" } else { "computer" },
        gpu,
        touch: "unknown",
        build: env!("CARGO_PKG_VERSION"),
    }
}

/// An online state against the server at `base`, invite links on
/// `site`, with session `token`, a refreshed one handed to `save` (the
/// run's, and each bot's, `crate::bots`).
pub fn session(
    base: String,
    site: String,
    token: Option<String>,
    save: ddi_ui::state::online::SaveToken,
) -> SharedOnline {
    Online::new(
        Box::new(ThreadHttp::with_base(base)),
        OnlineConfig {
            platform: Platform::Desktop,
            site,
        },
    )
    .with_token(token, save)
    .shared()
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
    KEPT.with(|k| {
        k.room_or(id, || {
            let room = Rc::new(RefCell::new(RoomClient::online(
                current(),
                id,
                Box::new(SocketTransport::default()),
                client_id(),
                crate::clock::now_ms(),
            )));
            // Its uploads' and downloads' threads report here.
            room.borrow_mut().set_inbox(crate::room::inbox());
            room
        })
    })
}

/// Leave room: forgotten.
pub fn discard() {
    KEPT.with(Kept::discard_room);
}

/// The room watched with its audience by `link`
/// (`docs/plans/multiplayer-audience.md`): its seat's client, made at
/// first with the invite page's `ticket` (else asked for), its stages'
/// songs read from the room's share on threads (`crate::room::prepare_audience`).
pub fn audience(link: &str, ticket: Option<String>) -> SharedAudience {
    KEPT.with(|k| {
        k.audience_or(link, || {
            let a = Rc::new(RefCell::new(AudienceClient::new(
                current(),
                link,
                Box::new(SocketTransport::audience(None)),
                ticket,
                crate::clock::now_ms(),
            )));
            a.borrow_mut().set_inbox(crate::room::audience_inbox());
            a
        })
    })
}

/// Stop watching: the seat forgotten (its socket closed).
pub fn discard_audience() {
    KEPT.with(Kept::discard_audience);
}

/// HTTP on a thread a request; answers through a channel.
pub struct ThreadHttp {
    base: String,
    tx: Sender<(u64, ApiResponse)>,
    rx: Receiver<(u64, ApiResponse)>,
    /// How long an answer may take ([`crate::share::ANSWER_TIMEOUT`];
    /// a long poll's [`crate::share::LONG_POLL_TIMEOUT`]).
    answer: std::time::Duration,
}

impl ThreadHttp {
    pub fn new() -> Self {
        Self::with_base(crate::share::base())
    }

    /// Against the server at `base` (`…/ditoditoinfinito/api`).
    pub fn with_base(base: String) -> Self {
        let (tx, rx) = channel();
        Self {
            base,
            tx,
            rx,
            answer: crate::share::ANSWER_TIMEOUT,
        }
    }

    /// Answers waited for `answer` at most (a long poll's too; tests).
    pub fn with_answer_timeout(mut self, answer: std::time::Duration) -> Self {
        self.answer = answer;
        self
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
        let answer = if request.long() && self.answer == crate::share::ANSWER_TIMEOUT {
            crate::share::LONG_POLL_TIMEOUT
        } else {
            self.answer
        };
        std::thread::Builder::new()
            .name("ddi-online-http".into())
            .spawn(move || {
                let _ = tx.send((id, send(&url, &request, answer)));
            })
            .ok();
    }

    fn poll(&mut self) -> Vec<(u64, ApiResponse)> {
        self.rx.try_iter().collect()
    }
}

fn send(url: &str, request: &ApiRequest, answer: std::time::Duration) -> ApiResponse {
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
            r.config()
                .timeout_recv_response(Some(answer))
                .timeout_recv_body(Some(answer))
                .build()
                .call()
        }
        (Method::Delete, _) => {
            let mut r = agent.delete(url);
            if let Some(a) = &auth {
                r = r.header("authorization", a);
            }
            r.config()
                .timeout_recv_response(Some(answer))
                .timeout_recv_body(Some(answer))
                .build()
                .call()
        }
        (Method::Post, body) => {
            let mut r = agent.post(url).header("content-type", "application/json");
            if let Some(a) = &auth {
                r = r.header("authorization", a);
            }
            r.config()
                .timeout_recv_response(Some(answer))
                .timeout_recv_body(Some(answer))
                .build()
                .send(body.clone().unwrap_or_default())
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
    /// Its own keep-alive at the WebSocket layer ([`PING_AFTER`],
    /// [`DEAD_AFTER`]); off, the room's text keep-alive serves it, as on
    /// the web ([`SocketTransport::with_room_keepalive`]).
    own_keepalive: bool,
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
            own_keepalive: true,
            socket: None,
        }
    }

    /// TESTING ONLY: without its own keep-alive, so that the room's text
    /// keep-alive (`ddi_room::online::KEEPALIVE_V0`, the web's) keeps it
    /// and notices its death (`tests/online.rs` against a local server).
    pub fn with_room_keepalive(mut self) -> Self {
        self.own_keepalive = false;
        self
    }

    /// An audience seat's socket (against the server at `base`, else the
    /// configured one).
    pub fn audience(base: Option<String>) -> Self {
        Self {
            base,
            audience: true,
            own_keepalive: true,
            socket: None,
        }
    }
}

impl Transport for SocketTransport {
    fn open(&mut self, room: &RoomId, ticket: &str) {
        self.close();
        let base = self.base.clone().unwrap_or_else(crate::share::base);
        let url =
            ddi_ui::state::online::api::socket_url(&base, room.as_str(), ticket, self.audience);
        let (out_tx, out_rx) = channel();
        let (ev_tx, ev_rx) = channel();
        std::thread::Builder::new()
            .name("ddi-room-socket".into())
            .spawn({
                let own = self.own_keepalive;
                move || run_socket(&url, &out_rx, &ev_tx, own)
            })
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

    /// Its own keep-alive at the WebSocket layer ([`PING_AFTER`],
    /// [`DEAD_AFTER`]): the room's text keep-alive is not sent.
    fn keeps_alive(&self) -> bool {
        self.own_keepalive
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

/// How long the socket may take to connect, and each read and write of
/// its opening (TLS, the upgrade) to come (review3 desktop-3: a server
/// that accepts and never finishes the opening closes the socket into
/// the link's reconnecting, never leaves it waiting for good).
const OPEN_WAIT: Duration = Duration::from_secs(10);

/// Heard nothing this long (no message, no pong), the socket sends a
/// ping; the server's runtime answers it.
const PING_AFTER: Duration = Duration::from_secs(10);

/// Heard nothing this long, the connection is taken for gone (half-open
/// after a network change or a sleep) and the socket closes as dropped
/// (1006), into the link's reconnecting.
const DEAD_AFTER: Duration = Duration::from_secs(20);

/// The socket's stream: TCP to the address `url` names, in [`OPEN_WAIT`],
/// its reads and writes bounded by it for the opening.
fn connect_stream(url: &str) -> std::io::Result<TcpStream> {
    use std::net::ToSocketAddrs;
    let uri: tungstenite::http::Uri = url
        .parse()
        .map_err(|e| std::io::Error::new(ErrorKind::InvalidInput, format!("{e}")))?;
    let host = uri
        .host()
        .ok_or_else(|| std::io::Error::new(ErrorKind::InvalidInput, "no host"))?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let port = uri
        .port_u16()
        .unwrap_or(if uri.scheme_str() == Some("wss") { 443 } else { 80 });
    let mut last = std::io::Error::new(ErrorKind::NotFound, "no address");
    for addr in (host, port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&addr, OPEN_WAIT) {
            Ok(s) => {
                s.set_read_timeout(Some(OPEN_WAIT))?;
                s.set_write_timeout(Some(OPEN_WAIT))?;
                return Ok(s);
            }
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// The socket's thread: connect, then read (a short timeout at a time)
/// and write what comes from the game, until either side closes or the
/// connection is heard from no more ([`DEAD_AFTER`]).
/// [`Outgoing::Close`] from the game closes it.
fn run_socket(
    url: &str,
    out: &Receiver<Outgoing>,
    events: &Sender<TransportEvent>,
    own_keepalive: bool,
) {
    use tungstenite::Message;
    use tungstenite::stream::MaybeTlsStream;
    let closed = |code: u16| {
        let _ = events.send(TransportEvent::Closed { code });
    };
    let opened = connect_stream(url)
        .map_err(|e| e.to_string())
        .and_then(|stream| {
            tungstenite::client_tls_with_config(url, stream, None, None)
                .map_err(|e| e.to_string())
        });
    let (mut ws, _) = match opened {
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
    let mut heard = std::time::Instant::now();
    let mut pinged = false;
    loop {
        // Silence: a ping, then, still nothing, the connection gone.
        // (Without its own keep-alive, the room's judges it.)
        let quiet = heard.elapsed();
        if own_keepalive && quiet >= DEAD_AFTER {
            eprintln!("online: the room's socket: nothing heard for {quiet:?}");
            return closed(1006);
        }
        if own_keepalive && quiet >= PING_AFTER && !pinged {
            pinged = true;
            if ws.send(Message::Ping(Default::default())).is_err() {
                return closed(1006);
            }
        }
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
        let read = ws.read();
        if read.is_ok() {
            heard = std::time::Instant::now();
            pinged = false;
        }
        match read {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// An online room left is let go whole: the stage it handed to the
    /// play host last no longer holds its client (and with it its
    /// socket's thread), nor is it the stage's room any more.
    #[test]
    fn a_left_online_room_is_let_go() {
        // A practice room's client stands in for an online one: what is
        // kept of it is the same.
        let room = RoomClient::shared(1, 1);
        KEPT.with(|k| k.room_or("room", || room.clone()));
        let kept = Rc::downgrade(&room);
        drop(room);
        discard();
        assert!(kept.upgrade().is_none(), "the left room's client lives on");
    }

    /// A listener that accepts and never says a word, kept alive while
    /// the test runs; its address.
    fn silent_listener() -> (std::net::SocketAddr, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let at = listener.local_addr().unwrap();
        let held = std::thread::spawn(move || {
            let mut kept = Vec::new();
            for _ in 0..2 {
                if let Ok((s, _)) = listener.accept() {
                    kept.push(s);
                }
            }
            std::thread::sleep(Duration::from_secs(60));
        });
        (at, held)
    }

    /// review3 desktop-2: a request whose connection opens and then goes
    /// quiet is answered as unreachable once its answer is late, never
    /// left waiting for good (a room's ticket then asked again).
    #[test]
    fn a_silent_server_answers_unreachable_in_time() {
        let (at, _held) = silent_listener();
        let mut http = ThreadHttp::with_base(format!("http://{at}/api"))
            .with_answer_timeout(Duration::from_secs(2));
        http.start(7, ddi_ui::state::online::api::me());
        let began = std::time::Instant::now();
        loop {
            let got = http.poll();
            if let Some((id, r)) = got.into_iter().next() {
                assert_eq!((id, r.status), (7, 0));
                break;
            }
            assert!(began.elapsed() < Duration::from_secs(10), "never answered");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Polls `t` until it says the socket closed, `within` at most.
    fn closed_within(t: &mut SocketTransport, within: Duration) -> Option<u16> {
        let began = std::time::Instant::now();
        while began.elapsed() < within {
            for e in t.poll() {
                if let TransportEvent::Closed { code } = e {
                    return Some(code);
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        None
    }

    /// review3 desktop-3: a server that accepts the connection and never
    /// finishes the socket's opening handshake: the socket closes (into
    /// the link's reconnecting) within its bound, never hangs.
    #[test]
    fn a_socket_whose_handshake_never_ends_closes() {
        let (at, _held) = silent_listener();
        let mut t = SocketTransport::with_base(format!("http://{at}/api"));
        t.open(&RoomId::new("Room000001"), "ticket");
        assert_eq!(closed_within(&mut t, Duration::from_secs(40)), Some(1006));
    }

    /// review3 desktop-3: a connection gone half-open once the socket is
    /// up (the server says nothing, answers no ping): noticed and closed
    /// within its bound, not after TCP's minutes.
    #[test]
    fn a_socket_that_hears_nothing_closes() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let at = listener.local_addr().unwrap();
        let _server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let _ws = tungstenite::accept(stream).unwrap();
            // Opened, then silent: never reads, so never answers a ping.
            std::thread::sleep(Duration::from_secs(90));
        });
        let mut t = SocketTransport::with_base(format!("http://{at}/api"));
        t.open(&RoomId::new("Room000001"), "ticket");
        let began = std::time::Instant::now();
        let mut opened = false;
        while !opened && began.elapsed() < Duration::from_secs(5) {
            opened = t.poll().iter().any(|e| matches!(e, TransportEvent::Opened));
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(opened, "the socket opened");
        assert_eq!(closed_within(&mut t, Duration::from_secs(45)), Some(1006));
    }

    // Round M (the review's gap 5): the socket's and the HTTP's threads
    // against a listener of the test's own on 127.0.0.1, speaking as the
    // server does: what `just test` and CI never ran (only
    // `tests/online.rs` against a local server did).

    /// What a loopback server saw of one socket.
    #[derive(Debug, Default)]
    struct Saw {
        /// The upgrade's path and query.
        uri: String,
        texts: Vec<String>,
        binaries: Vec<Vec<u8>>,
        pings: u32,
        /// The client's end closed: with a close frame (`Some(true)`), or
        /// its connection ended without one.
        closed: Option<bool>,
    }

    /// A WebSocket server on 127.0.0.1 for one connection: after the
    /// upgrade it answers each text with `echo:<text>` and each binary
    /// reversed, and does `then` once it read `after` messages (its
    /// close with a code, or the TCP connection dropped); reading until
    /// the client is gone or `for_` passed. Its address and what it saw.
    fn loopback(
        after: usize,
        then: Option<Result<u16, ()>>,
        for_: Duration,
    ) -> (std::net::SocketAddr, std::thread::JoinHandle<Saw>) {
        use tungstenite::Message;
        use tungstenite::handshake::server::{Request, Response};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let at = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let mut saw = Saw::default();
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_millis(50)))
                .unwrap();
            let mut uri = String::new();
            // (The callback's error type is tungstenite's own.)
            #[allow(clippy::result_large_err)]
            let mut ws = tungstenite::accept_hdr(stream, |req: &Request, resp: Response| {
                uri = req.uri().to_string();
                Ok(resp)
            })
            .unwrap();
            saw.uri = uri;
            let began = std::time::Instant::now();
            let mut read = 0;
            let mut done = false;
            while began.elapsed() < for_ {
                match ws.read() {
                    Ok(Message::Text(t)) => {
                        read += 1;
                        saw.texts.push(t.to_string());
                        let _ = ws.send(Message::text(format!("echo:{t}")));
                    }
                    Ok(Message::Binary(b)) => {
                        read += 1;
                        saw.binaries.push(b.to_vec());
                        let back: Vec<u8> = b.iter().rev().copied().collect();
                        let _ = ws.send(Message::binary(back));
                    }
                    Ok(Message::Ping(_)) => {
                        saw.pings += 1;
                        // (tungstenite queued the pong: written here.)
                        let _ = ws.flush();
                    }
                    Ok(Message::Close(_)) => {
                        saw.closed = Some(true);
                        let _ = ws.flush();
                        break;
                    }
                    Ok(_) => {}
                    Err(tungstenite::Error::Io(e))
                        if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                    Err(_) => {
                        saw.closed.get_or_insert(false);
                        break;
                    }
                }
                if read >= after && !done {
                    done = true;
                    match then {
                        Some(Ok(code)) => {
                            use tungstenite::protocol::CloseFrame;
                            use tungstenite::protocol::frame::coding::CloseCode;
                            let _ = ws.close(Some(CloseFrame {
                                code: CloseCode::from(code),
                                reason: "".into(),
                            }));
                            let _ = ws.flush();
                        }
                        // Dropped: no close frame, the TCP connection gone.
                        Some(Err(())) => return saw,
                        None => {}
                    }
                }
            }
            saw
        });
        (at, server)
    }

    /// The socket to `at`, opened (asserted).
    fn opened(at: std::net::SocketAddr) -> SocketTransport {
        let mut t = SocketTransport::with_base(format!("http://{at}/ditoditoinfinito/api"));
        t.open(&RoomId::new("Room000001"), "ticket-7");
        let began = std::time::Instant::now();
        loop {
            if t.poll().iter().any(|e| matches!(e, TransportEvent::Opened)) {
                return t;
            }
            assert!(began.elapsed() < Duration::from_secs(5), "the socket never opened");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Polls `t` until `n` events came or its close, five seconds at
    /// most.
    fn events(t: &mut SocketTransport, n: usize) -> Vec<TransportEvent> {
        let began = std::time::Instant::now();
        let mut got = Vec::new();
        while got.len() < n && began.elapsed() < Duration::from_secs(5) {
            got.extend(t.poll());
            if got.iter().any(|e| matches!(e, TransportEvent::Closed { .. })) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        got
    }

    /// The socket's thread speaks the server's protocol: the ticket in
    /// the upgrade's query on the room's socket path, a text and a
    /// binary frame each way, and each close code the server ends a
    /// socket with told as it is (4401 sign in again, 4410 the room
    /// closed, 4000 left, 4429 too many sockets); after the close the
    /// thread is gone (the server sees its end of the connection close).
    #[test]
    fn the_socket_carries_text_and_binary_and_tells_each_close_code() {
        for code in [4401u16, 4410, 4000, 4429] {
            let (at, server) = loopback(2, Some(Ok(code)), Duration::from_secs(10));
            let mut t = opened(at);
            assert!(t.binary());
            t.send("hello".into());
            t.send_binary(vec![1, 2, 3]);
            let got = events(&mut t, 3);
            assert_eq!(
                got,
                vec![
                    TransportEvent::Text("echo:hello".into()),
                    TransportEvent::Binary(vec![3, 2, 1]),
                    TransportEvent::Closed { code },
                ],
                "{code}"
            );
            // Closed: nothing more is sent or read, and polling says
            // nothing.
            t.send("late".into());
            assert!(t.poll().is_empty());
            let saw = server.join().unwrap();
            assert_eq!(saw.uri, "/ditoditoinfinito/api/sockets/Room000001/ws?ticket=ticket-7");
            assert_eq!(saw.texts, ["hello"]);
            assert_eq!(saw.binaries, [vec![1, 2, 3]]);
            assert!(saw.closed.is_some(), "the client's end closed: {saw:?}");
        }
    }

    /// A TCP connection dropped with no close frame is a dropped socket
    /// (1006), into the link's reconnecting; and an audience seat's
    /// socket asks its own path.
    #[test]
    fn a_dropped_connection_closes_1006_and_an_audience_socket_asks_its_path() {
        let (at, server) = loopback(1, Some(Err(())), Duration::from_secs(10));
        let mut t = SocketTransport::audience(Some(format!("http://{at}/api")));
        t.open(&RoomId::new("Room000001"), "seat-ticket");
        let began = std::time::Instant::now();
        while !t.poll().iter().any(|e| matches!(e, TransportEvent::Opened)) {
            assert!(began.elapsed() < Duration::from_secs(5), "never opened");
            std::thread::sleep(Duration::from_millis(10));
        }
        t.send("hello".into());
        let got = events(&mut t, 2);
        assert_eq!(got.last(), Some(&TransportEvent::Closed { code: 1006 }), "{got:?}");
        let saw = server.join().unwrap();
        assert_eq!(saw.uri, "/api/sockets/Room000001/audience/ws?ticket=seat-ticket");
    }

    /// The game closing its socket (the room left, the transport
    /// dropped) sends the close and ends the thread: the server reads
    /// the close frame, and nothing is told the game after it.
    #[test]
    fn the_game_closing_its_socket_tells_the_server() {
        let (at, server) = loopback(usize::MAX, None, Duration::from_secs(10));
        let mut t = opened(at);
        t.send("one".into());
        assert_eq!(events(&mut t, 1), vec![TransportEvent::Text("echo:one".into())]);
        t.close();
        assert!(t.poll().is_empty());
        let saw = server.join().unwrap();
        assert_eq!(saw.closed, Some(true), "a close frame: {saw:?}");
    }

    /// The keepalive: a socket that hears nothing pings after
    /// [`PING_AFTER`]; a server that is there answers, so the socket is
    /// still open past [`DEAD_AFTER`] (a quiet room between stages is
    /// not a dead connection), and a message still crosses it.
    #[test]
    fn a_quiet_socket_pings_and_stays_open_while_the_server_answers() {
        let quiet = DEAD_AFTER + Duration::from_secs(4);
        let (at, server) = loopback(usize::MAX, None, quiet + Duration::from_secs(10));
        let mut t = opened(at);
        let began = std::time::Instant::now();
        while began.elapsed() < quiet {
            let got = t.poll();
            assert!(got.is_empty(), "a quiet socket says nothing: {got:?}");
            std::thread::sleep(Duration::from_millis(100));
        }
        t.send("still".into());
        assert_eq!(events(&mut t, 1), vec![TransportEvent::Text("echo:still".into())]);
        t.close();
        let saw = server.join().unwrap();
        assert!(saw.pings >= 1, "pinged while quiet: {saw:?}");
    }

    /// One HTTP request answered by a listener on 127.0.0.1: the request
    /// as read (its head and body) and the answer written, `head` after
    /// the status line.
    fn http_once(
        status: &'static str,
        head: &'static str,
        body: &'static str,
    ) -> (std::net::SocketAddr, std::thread::JoinHandle<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let at = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut read = Vec::new();
            let mut buf = [0u8; 2048];
            // The head, then as many bytes as its content-length says.
            loop {
                let text = String::from_utf8_lossy(&read).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text[..end]
                        .lines()
                        .find_map(|l| {
                            let (k, v) = l.split_once(':')?;
                            k.eq_ignore_ascii_case("content-length")
                                .then(|| v.trim().parse::<usize>().ok())?
                        })
                        .unwrap_or(0);
                    if read.len() >= end + 4 + length {
                        break;
                    }
                }
                match s.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => read.extend_from_slice(&buf[..n]),
                }
            }
            let answer = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n{head}\r\n{body}",
                body.len()
            );
            let _ = s.write_all(answer.as_bytes());
            String::from_utf8_lossy(&read).to_string()
        });
        (at, server)
    }

    /// The answer to `request` from the HTTP thread, ten seconds at most.
    fn answered(http: &mut ThreadHttp, request: ApiRequest) -> ApiResponse {
        http.start(3, request);
        let began = std::time::Instant::now();
        loop {
            if let Some((id, r)) = http.poll().into_iter().next() {
                assert_eq!(id, 3);
                return r;
            }
            assert!(began.elapsed() < Duration::from_secs(10), "never answered");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The HTTP thread sends what the server reads (the path under the
    /// base, the session as `Authorization: Bearer`, a JSON body) and
    /// reads what it answers: the status, the body, and the session
    /// signed again in `x-ddi-session` (kept by the caller as the new
    /// one); a refusal's status and code come through as they are (a
    /// ticket's `429 too_many_tickets`).
    #[test]
    fn the_http_thread_sends_the_session_and_reads_the_one_signed_again() {
        let (at, server) = http_once("200 OK", "x-ddi-session: signed-again\r\n", r#"{"ticket":"t1"}"#);
        let mut http = ThreadHttp::with_base(format!("http://{at}/ditoditoinfinito/api/"));
        let mut request = ddi_ui::state::online::api::ticket("Room000001");
        request.bearer = Some("my-session".into());
        let r = answered(&mut http, request);
        assert_eq!((r.status, r.body.as_str()), (200, r#"{"ticket":"t1"}"#));
        assert_eq!(r.session.as_deref(), Some("signed-again"));
        let read = server.join().unwrap();
        let lower = read.to_ascii_lowercase();
        assert!(
            read.starts_with("POST /ditoditoinfinito/api/rooms/Room000001/ticket HTTP/1.1"),
            "{read}"
        );
        assert!(lower.contains("authorization: bearer my-session"), "{read}");
        assert!(lower.contains("content-type: application/json"), "{read}");
        assert!(read.ends_with("}") && read.contains(r#""model":"#), "{read}");

        // A sign-in attempt's poll: its own secret, never the session.
        let (at, server) = http_once("429 Too Many Requests", "", r#"{"error":"too_many_tickets"}"#);
        let mut http = ThreadHttp::with_base(format!("http://{at}/api"));
        let mut request = ddi_ui::state::online::api::me();
        request.bearer = Some("my-session".into());
        request.poll = Some("poll-secret".into());
        let r = answered(&mut http, request);
        assert_eq!((r.status, r.session), (429, None));
        assert!(r.body.contains("too_many_tickets"), "{}", r.body);
        let read = server.join().unwrap();
        assert!(read.starts_with("GET /api/me HTTP/1.1"), "{read}");
        let lower = read.to_ascii_lowercase();
        assert!(lower.contains("authorization: poll poll-secret"), "{read}");
        assert!(!lower.contains("bearer"), "{read}");
    }
}
