//! Invite-only online rooms on the web (`docs/plans/multiplayer-prototype-progress.md`,
//! M5c), behind the prototype's switch: the UI core's online state
//! (`ddi_ui::state::online`) kept for the session, its HTTP through
//! `fetch` to the private server (`crate::share::base`, `api` beside the
//! page; the session is the server's HttpOnly cookie, never read here) and
//! a room's socket through the browser's `WebSocket`.
//!
//! Signing in leaves the page for the server's sign-in (Twitch), which
//! comes back to `#/mp`, `#/mp/join/<invite>` or `#/mp/room/<id>`
//! (`?signin=canceled` when it was); what it was for (a room to make, an
//! invite to join) is kept in `sessionStorage` across the trip
//! ([`AFTER`]).
//!
//! A room's stage songs (M5d) go through `crate::room`: the host's pick
//! uploaded as the room's share under its grant (`crate::share`), the
//! others' read from it.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use ddi_platform::storage::{self, Area};
use ddi_room::{ClientId, RoomId, Transport, TransportEvent};
use ddi_ui::state::online::api::{ApiRequest, ApiResponse, Method};
use ddi_ui::state::online::{Dest, Http, Online, OnlineConfig, Platform, SharedOnline, SignIn};
use ddi_ui::state::room::{RoomClient, SharedRoom};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

/// `sessionStorage`: where signing in was going when the page left for it.
const AFTER: &str = "ddi.mp.after";

thread_local! {
    static ONLINE: RefCell<Option<SharedOnline>> = const { RefCell::new(None) };
    /// The online room shown last (its id and client).
    static ROOM: RefCell<Option<(String, SharedRoom)>> = const { RefCell::new(None) };
}

fn now_ms() -> u64 {
    js_sys::Date::now().max(0.0) as u64
}

/// The game's page, where invite links open.
fn site() -> String {
    web_sys::window()
        .and_then(|w| {
            let l = w.location();
            Some(format!("{}{}", l.origin().ok()?, l.pathname().ok()?))
        })
        .unwrap_or_else(|| "https://sugoijan.dev/ditoditoinfinito/".into())
}

/// The session's online state, made at first.
pub(crate) fn current() -> SharedOnline {
    ONLINE.with(|o| {
        o.borrow_mut()
            .get_or_insert_with(|| {
                Online::new(
                    Box::new(WebHttp::default()),
                    OnlineConfig {
                        platform: Platform::Web,
                        site: site(),
                    },
                )
                .shared()
            })
            .clone()
    })
}

fn dest_text(dest: &Dest) -> String {
    match dest {
        Dest::Home => "home".into(),
        Dest::Create => "create".into(),
        Dest::Join(invite) => format!("join:{invite}"),
        Dest::Room(id) => format!("room:{id}"),
    }
}

fn dest_of(text: &str) -> Option<Dest> {
    Some(match text.split_once(':') {
        None if text == "home" => Dest::Home,
        None if text == "create" => Dest::Create,
        Some(("join", invite)) => Dest::Join(invite.into()),
        Some(("room", id)) => Dest::Room(id.into()),
        _ => return None,
    })
}

/// The page left for the server's sign-in: what it was for, kept.
pub(crate) fn leave_for_sign_in(path: &str) {
    let dest = match current().borrow().sign_in_state() {
        SignIn::Completing { dest } => Some(dest.clone()),
        _ => None,
    };
    if let Some(dest) = dest {
        let _ = storage::set(Area::Session, AFTER, &dest_text(&dest));
    }
    let url = format!("{}{path}", crate::share::base());
    if let Some(w) = web_sys::window() {
        let _ = w.location().assign(&url);
    }
}

/// A multiplayer route opened: if the page came back from signing in,
/// on to what it was for (or the cancel said); `canceled` from the
/// route's `signin=canceled`.
pub(crate) fn arrived(canceled: bool) {
    let Some(after) = storage::get(Area::Session, AFTER) else {
        if canceled {
            current().borrow_mut().sign_in_canceled(Dest::Home);
        }
        return;
    };
    storage::remove(Area::Session, AFTER);
    let dest = dest_of(&after).unwrap_or(Dest::Home);
    let online = current();
    let mut o = online.borrow_mut();
    if canceled {
        o.sign_in_canceled(dest);
    } else {
        o.returned(dest);
    }
}

/// A fresh client id for this tab's session (the server keeps each one's
/// last sequence number; not a secret: the server ties it to the person).
fn client_id() -> ClientId {
    let a = (js_sys::Math::random() * 4_294_967_296.0) as u64;
    let b = (js_sys::Math::random() * 4_294_967_296.0) as u64;
    ClientId::new(format!("web-{:016x}", (a << 32) | b))
}

/// Online room `id`'s client, made (and joined) at first.
pub(crate) fn room(id: &str) -> SharedRoom {
    if let Some(r) =
        ROOM.with_borrow(|r| r.as_ref().filter(|(i, _)| i == id).map(|(_, r)| r.clone()))
    {
        return r;
    }
    let room = Rc::new(RefCell::new(RoomClient::online(
        current(),
        id,
        Box::new(WebSocketTransport::default()),
        client_id(),
        now_ms(),
    )));
    ROOM.with_borrow_mut(|r| *r = Some((id.into(), room.clone())));
    room
}

/// Leave room: forgotten (its socket closed once the room answered).
pub(crate) fn discard() {
    ROOM.with_borrow_mut(|r| *r = None);
}

/// The server over `fetch` (its cookie sent as the page's own origin's).
#[derive(Default)]
struct WebHttp {
    answers: Rc<RefCell<Vec<(u64, ApiResponse)>>>,
}

impl Http for WebHttp {
    fn start(&mut self, id: u64, request: ApiRequest) {
        let answers = self.answers.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let response = send(&request).await;
            answers.borrow_mut().push((id, response));
        });
    }

    fn poll(&mut self) -> Vec<(u64, ApiResponse)> {
        std::mem::take(&mut *self.answers.borrow_mut())
    }
}

async fn send(request: &ApiRequest) -> ApiResponse {
    use gloo::net::http::Request;
    let url = format!("{}{}", crate::share::base(), request.path);
    let builder = match request.method {
        Method::Get => Request::get(&url),
        Method::Post => Request::post(&url),
        Method::Delete => Request::delete(&url),
    };
    let built = match &request.body {
        Some(body) => builder
            .header("content-type", "application/json")
            .body(body.clone()),
        None => builder.build(),
    };
    let Ok(req) = built else {
        return ApiResponse::unreachable();
    };
    match req.send().await {
        Ok(r) => ApiResponse {
            status: r.status(),
            body: r.text().await.unwrap_or_default(),
            session: None,
        },
        Err(e) => {
            web_sys::console::warn_1(
                &format!("online: {} {}: {e}", request.method.as_str(), request.path).into(),
            );
            ApiResponse::unreachable()
        }
    }
}

/// A room's socket: the browser's `WebSocket` on
/// `<api>/sockets/<id>/ws?ticket=` (`ws:` or `wss:` as the API's scheme).
#[derive(Default)]
struct WebSocketTransport {
    socket: Option<Socket>,
    /// Bumped with each socket: an earlier one's events are not reported.
    generation: Rc<RefCell<u64>>,
    events: Rc<RefCell<VecDeque<TransportEvent>>>,
}

struct Socket {
    ws: web_sys::WebSocket,
    _open: Closure<dyn FnMut(web_sys::Event)>,
    _message: Closure<dyn FnMut(web_sys::MessageEvent)>,
    _close: Closure<dyn FnMut(web_sys::CloseEvent)>,
}

impl Drop for Socket {
    fn drop(&mut self) {
        self.ws.set_onopen(None);
        self.ws.set_onmessage(None);
        self.ws.set_onclose(None);
        let _ = self.ws.close();
    }
}

impl WebSocketTransport {
    fn url(room: &RoomId, ticket: &str) -> String {
        let base = crate::share::base();
        let base = if let Some(rest) = base.strip_prefix("https://") {
            format!("wss://{rest}")
        } else if let Some(rest) = base.strip_prefix("http://") {
            format!("ws://{rest}")
        } else {
            base
        };
        format!(
            "{base}{}",
            ddi_ui::state::online::api::socket_path(room.as_str(), ticket)
        )
    }
}

impl Transport for WebSocketTransport {
    fn open(&mut self, room: &RoomId, ticket: &str) {
        self.socket = None;
        *self.generation.borrow_mut() += 1;
        let generation = *self.generation.borrow();
        let ws = match web_sys::WebSocket::new(&Self::url(room, ticket)) {
            Ok(ws) => ws,
            Err(_) => {
                self.events
                    .borrow_mut()
                    .push_back(TransportEvent::Closed { code: 1006 });
                return;
            }
        };
        let current = {
            let g = self.generation.clone();
            move || *g.borrow() == generation
        };
        let (events, now) = (self.events.clone(), current.clone());
        let open = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            if now() {
                events.borrow_mut().push_back(TransportEvent::Opened);
            }
        });
        let (events, now) = (self.events.clone(), current.clone());
        let message =
            Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
                if now()
                    && let Some(text) = e.data().as_string()
                {
                    events.borrow_mut().push_back(TransportEvent::Text(text));
                }
            });
        let (events, now) = (self.events.clone(), current);
        let close =
            Closure::<dyn FnMut(web_sys::CloseEvent)>::new(move |e: web_sys::CloseEvent| {
                if now() {
                    events
                        .borrow_mut()
                        .push_back(TransportEvent::Closed { code: e.code() });
                }
            });
        ws.set_onopen(Some(open.as_ref().unchecked_ref()));
        ws.set_onmessage(Some(message.as_ref().unchecked_ref()));
        ws.set_onclose(Some(close.as_ref().unchecked_ref()));
        self.socket = Some(Socket {
            ws,
            _open: open,
            _message: message,
            _close: close,
        });
    }

    fn send(&mut self, text: String) {
        if let Some(s) = &self.socket
            && s.ws.ready_state() == web_sys::WebSocket::OPEN
        {
            let _ = s.ws.send_with_str(&text);
        }
    }

    fn close(&mut self) {
        *self.generation.borrow_mut() += 1;
        self.socket = None;
    }

    fn poll(&mut self) -> Vec<TransportEvent> {
        self.events.borrow_mut().drain(..).collect()
    }
}
