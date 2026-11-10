//! The private server's sign-in and room routes as the game calls them
//! (`../ddi-server`'s README, "Sign-in with Twitch" and "Rooms"): each
//! request built here, each answer read here, the shells only carrying
//! them (the web's `fetch` with the session's cookie, desktop's HTTP on a
//! thread with the session as a bearer). Sans-IO, natively tested.

use ddi_room::{Intent, ParticipantId, Participation, PhaseKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Delete => "DELETE",
        }
    }
}

/// One request to the server, its path after the API's base.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiRequest {
    pub method: Method,
    /// `/me`, `/rooms`, … (the shell puts the base in front).
    pub path: String,
    /// A JSON body.
    pub body: Option<String>,
    /// `Authorization: Bearer <session>` (desktop; the web's session is
    /// its cookie).
    pub bearer: Option<String>,
    /// `Authorization: Poll <secret>` (desktop's sign-in attempt).
    pub poll: Option<String>,
}

impl ApiRequest {
    fn new(method: Method, path: impl Into<String>) -> Self {
        Self {
            method,
            path: path.into(),
            body: None,
            bearer: None,
            poll: None,
        }
    }

    fn json(mut self, body: serde_json::Value) -> Self {
        self.body = Some(body.to_string());
        self
    }

    /// Long-polled requests (desktop's attempt) wait this long at most on
    /// the server; a shell's timeout must be longer.
    pub fn long(&self) -> bool {
        self.poll.is_some() && self.method == Method::Get
    }
}

/// The server's answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiResponse {
    /// `0`: no answer came (the network, a refused connection).
    pub status: u16,
    pub body: String,
    /// A re-signed session (`x-ddi-session`), for desktop to keep.
    pub session: Option<String>,
}

impl ApiResponse {
    pub fn unreachable() -> Self {
        Self {
            status: 0,
            body: String::new(),
            session: None,
        }
    }
}

/// A refusal: the status and the server's code (`{"error": code}`), or
/// `unreachable` with status 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub status: u16,
    pub code: String,
}

impl Problem {
    pub fn unreachable() -> Self {
        Self {
            status: 0,
            code: "unreachable".into(),
        }
    }

    pub fn signed_out(&self) -> bool {
        self.status == 401
    }
}

#[derive(Deserialize)]
struct ErrorDoc {
    error: String,
}

/// The answer as `T`, or why not.
pub fn read<T: serde::de::DeserializeOwned>(r: &ApiResponse) -> Result<T, Problem> {
    if r.status == 0 {
        return Err(Problem::unreachable());
    }
    if !(200..300).contains(&r.status) {
        let code = serde_json::from_str::<ErrorDoc>(&r.body)
            .map(|e| e.error)
            .unwrap_or_else(|_| format!("http_{}", r.status));
        return Err(Problem {
            status: r.status,
            code,
        });
    }
    serde_json::from_str(&r.body).map_err(|_| Problem {
        status: r.status,
        code: "unreadable_answer".into(),
    })
}

/// A success with no body to read.
pub fn done(r: &ApiResponse) -> Result<(), Problem> {
    match read::<serde_json::Value>(r) {
        Ok(_) => Ok(()),
        Err(p) if p.code == "unreadable_answer" => Ok(()),
        Err(p) => Err(p),
    }
}

/// Who is signed in (`GET /me`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Me {
    pub player_id: String,
    #[serde(default)]
    pub login: String,
    pub display_name: String,
    #[serde(default)]
    pub admin: bool,
    #[serde(default)]
    pub expires_at_ms: i64,
}

/// A room made (`POST /rooms`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Created {
    pub room_id: String,
    pub invite: String,
}

/// Whether a newcomer could enter now, as an invite's preview says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Admission {
    Open,
    /// A match runs: entry waits for the next.
    NextMatch,
    Closed,
    Full,
}

/// What an invite shows before signing in (`GET /invites/<invite>`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Preview {
    pub room_id: String,
    pub room_name: String,
    pub host_display_name: String,
    pub phase: PhaseKind,
    pub admission: Admission,
    #[serde(default)]
    pub members: usize,
    #[serde(default)]
    pub expires_at_ms: i64,
}

/// A join's answer (`POST /rooms/<id>/join`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Joined {
    pub room_id: String,
    pub participant: ParticipantId,
    pub intent: Intent,
    pub participation: Participation,
    #[serde(default)]
    pub already: bool,
}

/// A room's summary for a member (`GET /rooms/<id>`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Summary {
    pub room_id: String,
    pub name: String,
    #[serde(default)]
    pub participant: Option<ParticipantId>,
}

/// A socket's ticket (`POST /rooms/<id>/ticket`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct TicketDoc {
    pub ticket: String,
}

/// The room's invite for its operators (`GET` or `POST
/// /rooms/<id>/invite`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct InviteDoc {
    pub invite: String,
    #[serde(default)]
    pub expires_at_ms: i64,
}

/// Desktop's sign-in attempt (`POST /auth/attempts`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct AttemptDoc {
    pub attempt_id: String,
    pub poll_secret: String,
    pub open_url: String,
    #[serde(default)]
    pub expires_at_ms: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Pending,
    Done,
    Claimed,
    Canceled,
    Expired,
}

/// An attempt polled (`GET /auth/attempts/<id>`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct AttemptPoll {
    pub status: AttemptStatus,
    #[serde(default)]
    pub session_token: Option<String>,
}

pub fn me() -> ApiRequest {
    ApiRequest::new(Method::Get, "/me")
}

pub fn logout() -> ApiRequest {
    ApiRequest::new(Method::Post, "/auth/logout")
}

/// The web's sign-in, a page to go to (not fetched): Twitch, then back to
/// the game at `route` (`/mp`, `/mp/join/<invite>`, `/mp/room/<id>`).
pub fn login_path(route: &str) -> String {
    format!("/auth/twitch/login?return={}", encode(route))
}

pub fn new_attempt(route: &str) -> ApiRequest {
    ApiRequest::new(Method::Post, "/auth/attempts").json(serde_json::json!({ "return": route }))
}

pub fn poll_attempt(id: &str, secret: &str) -> ApiRequest {
    ApiRequest {
        poll: Some(secret.into()),
        ..ApiRequest::new(Method::Get, format!("/auth/attempts/{id}"))
    }
}

pub fn cancel_attempt(id: &str, secret: &str) -> ApiRequest {
    ApiRequest {
        poll: Some(secret.into()),
        ..ApiRequest::new(Method::Delete, format!("/auth/attempts/{id}"))
    }
}

pub fn create_room(name: Option<&str>) -> ApiRequest {
    ApiRequest::new(Method::Post, "/rooms").json(serde_json::json!({ "name": name }))
}

pub fn preview(invite: &str) -> ApiRequest {
    ApiRequest::new(Method::Get, format!("/invites/{}", encode(invite)))
}

pub fn join(room: &str, invite: Option<&str>) -> ApiRequest {
    ApiRequest::new(Method::Post, format!("/rooms/{room}/join"))
        .json(serde_json::json!({ "invite": invite, "intent": "play" }))
}

pub fn summary(room: &str) -> ApiRequest {
    ApiRequest::new(Method::Get, format!("/rooms/{room}"))
}

pub fn ticket(room: &str) -> ApiRequest {
    ApiRequest::new(Method::Post, format!("/rooms/{room}/ticket"))
}

pub fn invite(room: &str) -> ApiRequest {
    ApiRequest::new(Method::Get, format!("/rooms/{room}/invite"))
}

pub fn rotate_invite(room: &str) -> ApiRequest {
    ApiRequest::new(Method::Post, format!("/rooms/{room}/invite"))
        .json(serde_json::json!({ "rotate": true }))
}

/// An upload grant for stage `stage`'s song (M5d): the room's operators
/// get one (`ddi_share_protocol::GrantAnswer`), or the share the room
/// made of the same content before.
pub fn grant(room: &str, stage: u64) -> ApiRequest {
    ApiRequest::new(Method::Post, format!("/rooms/{room}/grant"))
        .json(serde_json::json!({ "stage": stage }))
}

/// The socket's path for a ticket (`/sockets/<id>/ws?ticket=`).
pub fn socket_path(room: &str, ticket: &str) -> String {
    format!("/sockets/{room}/ws?ticket={}", encode(ticket))
}

/// Percent-encoding for a path segment or query value.
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A room id: 10 characters of `[0-9A-Za-z]`.
pub fn valid_room_id(id: &str) -> bool {
    id.len() == 10 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// An invite: `<room id>.<secret>`, the secret 22 characters of
/// `[0-9A-Za-z]`.
pub fn valid_invite(token: &str) -> bool {
    token.split_once('.').is_some_and(|(room, secret)| {
        valid_room_id(room)
            && secret.len() == 22
            && secret.bytes().all(|b| b.is_ascii_alphanumeric())
    })
}

/// The invite in what was pasted or given: a link
/// (`…#/mp/join/<invite>?…`) or the invite itself.
pub fn invite_in(text: &str) -> Option<String> {
    let text = text.trim();
    let token = match text.split_once("/mp/join/") {
        Some((_, rest)) => rest.split(['?', '&', '#', '/']).next().unwrap_or(""),
        None => text,
    };
    valid_invite(token).then(|| token.to_string())
}

/// The room id of an invite.
pub fn room_of(invite: &str) -> Option<&str> {
    invite.split_once('.').map(|(room, _)| room)
}

/// The link that opens an invite in the game on the web: `site` (the
/// game's page, ending in `/`) at `#/mp/join/<invite>`, with the
/// prototype's switch (`mp=1`) so it opens where the switch was never set.
pub fn invite_link(site: &str, invite: &str) -> String {
    let site = site.trim_end_matches('#');
    let site = if site.ends_with('/') {
        site.to_string()
    } else {
        format!("{site}/")
    };
    format!("{site}#/mp/join/{invite}?mp=1")
}

#[cfg(test)]
mod tests {
    use super::*;

    const INVITE: &str = "Room000001.abcdefghijABCDEFGHIJ01";

    #[test]
    fn invites_are_found_in_links_and_text() {
        assert!(valid_invite(INVITE));
        assert_eq!(invite_in(INVITE).as_deref(), Some(INVITE));
        let link = invite_link("https://sugoijan.dev/ditoditoinfinito/", INVITE);
        assert_eq!(
            link,
            "https://sugoijan.dev/ditoditoinfinito/#/mp/join/Room000001.abcdefghijABCDEFGHIJ01?mp=1"
        );
        assert_eq!(invite_in(&link).as_deref(), Some(INVITE));
        assert_eq!(invite_in(&format!("  {link}\n")).as_deref(), Some(INVITE));
        assert_eq!(invite_in("Room000001.short"), None);
        assert_eq!(invite_in("https://example.com/#/mp/join/x.y"), None);
        assert_eq!(room_of(INVITE), Some("Room000001"));
        assert_eq!(
            invite_link("http://localhost:8080", INVITE),
            "http://localhost:8080/#/mp/join/Room000001.abcdefghijABCDEFGHIJ01?mp=1"
        );
    }

    #[test]
    fn answers_and_refusals_are_read() {
        let ok = ApiResponse {
            status: 200,
            body: r#"{"player_id":"dev:alice","login":"alice","display_name":"Alice","admin":false,"expires_at_ms":5}"#.into(),
            session: None,
        };
        assert_eq!(read::<Me>(&ok).unwrap().display_name, "Alice");
        let refused = ApiResponse {
            status: 403,
            body: r#"{"error":"invite_rotated"}"#.into(),
            session: None,
        };
        assert_eq!(
            read::<Me>(&refused),
            Err(Problem {
                status: 403,
                code: "invite_rotated".into()
            })
        );
        let odd = ApiResponse {
            status: 502,
            body: "Bad gateway".into(),
            session: None,
        };
        assert_eq!(read::<Me>(&odd).unwrap_err().code, "http_502");
        assert_eq!(
            read::<Me>(&ApiResponse::unreachable()),
            Err(Problem::unreachable())
        );
        let preview = ApiResponse {
            status: 200,
            body: r#"{"room_id":"Room000001","room_name":"E2E","host_display_name":"Alice","phase":"stage","admission":"next_match","members":3,"expires_at_ms":9}"#.into(),
            session: None,
        };
        let p = read::<Preview>(&preview).unwrap();
        assert_eq!(
            (p.phase, p.admission),
            (PhaseKind::Stage, Admission::NextMatch)
        );
        let joined = ApiResponse {
            status: 200,
            body: r#"{"room_id":"Room000001","participant":4,"intent":"play","participation":"waiting_next_match","already":false}"#.into(),
            session: None,
        };
        assert_eq!(
            read::<Joined>(&joined).unwrap().participation,
            Participation::WaitingNextMatch
        );
        assert_eq!(
            login_path("/mp/join/a.b"),
            "/auth/twitch/login?return=%2Fmp%2Fjoin%2Fa.b"
        );
        assert_eq!(
            socket_path("Room000001", "a.b+c"),
            "/sockets/Room000001/ws?ticket=a.b%2Bc"
        );
    }
}
