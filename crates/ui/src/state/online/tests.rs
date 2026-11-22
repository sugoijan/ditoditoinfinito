//! The online state against a fake server's HTTP: desktop's sign-in
//! attempt from start to a room made, canceled and failed attempts, the
//! web's sign-in leaving and coming back, a refreshed session kept, an
//! invite's preview in each of its states, and an online room's client
//! over the fake room (`ddi_room::fake`): joining, its connection, 4401,
//! and the development-only content to the readiness window, countdown
//! and the development stop.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use super::api::{ApiRequest, ApiResponse, Method};
use super::*;

pub(crate) type Handler = Box<dyn FnMut(&ApiRequest) -> Option<ApiResponse>>;

/// HTTP answered by `handler` (none: the answer never comes), each
/// request kept.
pub(crate) struct FakeHttp {
    pub(crate) handler: Rc<RefCell<Handler>>,
    pub(crate) log: Rc<RefCell<Vec<ApiRequest>>>,
    pub(crate) out: VecDeque<(u64, ApiResponse)>,
}

impl Http for FakeHttp {
    fn start(&mut self, id: u64, request: ApiRequest) {
        self.log.borrow_mut().push(request.clone());
        if let Some(r) = (self.handler.borrow_mut())(&request) {
            self.out.push_back((id, r));
        }
    }

    fn poll(&mut self) -> Vec<(u64, ApiResponse)> {
        self.out.drain(..).collect()
    }
}

pub(crate) fn ok(body: &str) -> Option<ApiResponse> {
    Some(ApiResponse {
        status: 200,
        body: body.into(),
        session: None,
    })
}

pub(crate) fn err(status: u16, code: &str) -> Option<ApiResponse> {
    Some(ApiResponse {
        status,
        body: format!(r#"{{"error":"{code}"}}"#),
        session: None,
    })
}

pub(crate) const ME: &str = r#"{"player_id":"dev:carol","login":"carol","display_name":"Carol","admin":false,"expires_at_ms":1}"#;
pub(crate) const INVITE: &str = "Room000001.abcdefghijABCDEFGHIJ01";
/// The same room's watch-only link's invite.
pub(crate) const WATCH_INVITE: &str = "Room000001.watchwatchWATCHWATCH01";

/// `/me`'s answer for `name` (`dev:<name>`).
pub(crate) fn me_of(name: &str) -> String {
    format!(
        r#"{{"player_id":"dev:{name}","login":"{name}","display_name":"{name}","admin":false,"expires_at_ms":1}}"#
    )
}

/// An online state on the web over `handler`, its session checked (signed
/// in where the handler's `/me` says so).
pub(crate) fn signed_in(handler: Handler) -> SharedOnline {
    let http = FakeHttp {
        handler: Rc::new(RefCell::new(handler)),
        log: Rc::new(RefCell::new(Vec::new())),
        out: VecDeque::new(),
    };
    let online = Online::new(
        Box::new(http),
        OnlineConfig {
            platform: Platform::Web,
            site: "http://localhost:8080/".into(),
        },
    )
    .shared();
    online.borrow_mut().check();
    online.borrow_mut().poll(0);
    online
}

pub(crate) struct Rig {
    pub(crate) online: SharedOnline,
    pub(crate) log: Rc<RefCell<Vec<ApiRequest>>>,
    pub(crate) saved: Rc<RefCell<Vec<Option<String>>>>,
}

pub(crate) fn rig(platform: Platform, handler: Handler) -> Rig {
    let log = Rc::new(RefCell::new(Vec::new()));
    let http = FakeHttp {
        handler: Rc::new(RefCell::new(handler)),
        log: log.clone(),
        out: VecDeque::new(),
    };
    let saved = Rc::new(RefCell::new(Vec::new()));
    let s = saved.clone();
    let online = Online::new(
        Box::new(http),
        OnlineConfig {
            platform,
            site: "http://localhost:8080/".into(),
        },
    )
    .with_token(
        None,
        Rc::new(move |t| s.borrow_mut().push(t.map(str::to_string))),
    )
    .shared();
    Rig { online, log, saved }
}

fn paths(log: &Rc<RefCell<Vec<ApiRequest>>>) -> Vec<String> {
    log.borrow()
        .iter()
        .map(|r| format!("{} {}", r.method.as_str(), r.path))
        .collect()
}

#[test]
fn desktop_signs_in_through_an_attempt_then_makes_the_room() {
    let polls = Rc::new(RefCell::new(0));
    let p = polls.clone();
    let r = rig(
        Platform::Desktop,
        Box::new(move |req| match (req.method, req.path.as_str()) {
            (Method::Post, "/auth/attempts") => {
                // This build shows the attempt's code (a server refuses
                // one that does not).
                assert_eq!(
                    req.body.as_deref(),
                    Some(r#"{"return":"/mp","shows_code":true}"#)
                );
                Some(ApiResponse {
                    status: 201,
                    body: r#"{"attempt_id":"a1","poll_secret":"s1","open_url":"http://api/auth/attempts/a1/open","user_code":"ABC234","expires_at_ms":9}"#.into(),
                    session: None,
                })
            }
            (Method::Get, "/auth/attempts/a1") => {
                assert_eq!(req.poll.as_deref(), Some("s1"));
                assert_eq!(req.bearer, None, "a poll carries no session");
                *p.borrow_mut() += 1;
                if *p.borrow() < 3 {
                    ok(r#"{"status":"pending","return":"/mp","expires_at_ms":9}"#)
                } else {
                    ok(
                        r#"{"status":"done","session_token":"tok","return":"/mp","expires_at_ms":9}"#,
                    )
                }
            }
            (Method::Get, "/me") => {
                assert_eq!(req.bearer.as_deref(), Some("tok"));
                ok(ME)
            }
            (Method::Post, "/rooms") => {
                assert_eq!(req.bearer.as_deref(), Some("tok"));
                Some(ApiResponse {
                    status: 201,
                    body: format!(r#"{{"room_id":"Room000001","invite":"{INVITE}"}}"#),
                    session: Some("tok2".into()),
                })
            }
            (Method::Post, "/auth/logout") => {
                assert_eq!(req.bearer.as_deref(), Some("tok2"));
                Some(ApiResponse {
                    status: 204,
                    body: String::new(),
                    session: None,
                })
            }
            _ => panic!("unexpected {req:?}"),
        }),
    );
    let mut o = r.online.borrow_mut();
    o.private_room();
    assert!(matches!(
        o.sign_in_state(),
        SignIn::Starting { dest: Dest::Create }
    ));
    o.poll(0);
    // The browser opens on the attempt, once.
    assert_eq!(
        o.take_redirect(),
        Some(Redirect::Browser("http://api/auth/attempts/a1/open".into()))
    );
    assert_eq!(o.take_redirect(), None);
    // The code the page asks for, shown while waiting.
    assert!(matches!(
        o.sign_in_state(),
        SignIn::Waiting { code: Some(c), .. } if c == "ABC234"
    ));
    assert_eq!(crate::state::online::api::grouped_code("ABC234"), "ABC-234");
    // Open again: the same attempt's page.
    o.open_browser_again();
    assert!(matches!(o.take_redirect(), Some(Redirect::Browser(_))));
    for t in 1..=5 {
        o.poll(t * 100);
    }
    assert_eq!(*polls.borrow(), 3, "polled until done, one at a time");
    assert_eq!(o.me().unwrap().display_name, "Carol");
    assert_eq!(o.sign_in_state(), &SignIn::Idle);
    assert_eq!(o.epoch(), 1);
    // Resumed: the room made, the re-signed session kept.
    assert_eq!(o.take_entered().as_deref(), Some("Room000001"));
    assert_eq!(
        *r.saved.borrow(),
        vec![Some("tok".to_string()), Some("tok2".to_string())]
    );
    drop(o);
    assert_eq!(
        paths(&r.log),
        vec![
            "POST /auth/attempts",
            "GET /auth/attempts/a1",
            "GET /auth/attempts/a1",
            "GET /auth/attempts/a1",
            "GET /me",
            "POST /rooms"
        ]
    );
    // The next request carries the newer session; signing out forgets it.
    r.online.borrow_mut().sign_out();
    assert_eq!(r.saved.borrow().last(), Some(&None));
    assert_eq!(r.online.borrow().account(), &Account::SignedOut);
}

#[test]
fn a_canceled_attempt_signs_nobody_in_and_failures_keep_the_destination() {
    let r = rig(
        Platform::Desktop,
        Box::new(|req| match (req.method, req.path.as_str()) {
            (Method::Post, "/auth/attempts") => {
                ok(r#"{"attempt_id":"a1","poll_secret":"s1","open_url":"u","expires_at_ms":9}"#)
            }
            // The poll's answer never comes before the cancel.
            (Method::Get, "/auth/attempts/a1") => None,
            (Method::Delete, "/auth/attempts/a1") => {
                assert_eq!(req.poll.as_deref(), Some("s1"));
                Some(ApiResponse {
                    status: 204,
                    body: String::new(),
                    session: None,
                })
            }
            _ => panic!("unexpected {req:?}"),
        }),
    );
    let mut o = r.online.borrow_mut();
    o.sign_in(Dest::Join(INVITE.into()));
    o.poll(0);
    o.poll(1);
    o.cancel_sign_in();
    assert_eq!(
        o.sign_in_state(),
        &SignIn::Canceled {
            dest: Dest::Join(INVITE.into())
        }
    );
    o.poll(2);
    assert!(o.me().is_none());
    assert!(r.saved.borrow().is_empty());
    drop(o);
    assert!(paths(&r.log).contains(&"DELETE /auth/attempts/a1".to_string()));

    // The server's own ends of an attempt.
    for (status, expect) in [
        (r#"{"status":"canceled"}"#, "canceled"),
        // Ended by wrong codes in the browser's page: a failure of its
        // own, Try again a new attempt (and code).
        (
            r#"{"status":"canceled","reason":"too_many_codes"}"#,
            "too_many_codes",
        ),
        // An unknown reason is a plain cancel.
        (r#"{"status":"canceled","reason":"other"}"#, "canceled"),
        (r#"{"status":"expired"}"#, "attempt_expired"),
        (r#"{"status":"claimed"}"#, "attempt_claimed"),
    ] {
        let body = status.to_string();
        let r = rig(
            Platform::Desktop,
            Box::new(move |req| match (req.method, req.path.as_str()) {
                (Method::Post, "/auth/attempts") => {
                    ok(r#"{"attempt_id":"a1","poll_secret":"s1","open_url":"u","expires_at_ms":9}"#)
                }
                (Method::Get, "/auth/attempts/a1") => ok(&body),
                _ => panic!("unexpected {req:?}"),
            }),
        );
        let mut o = r.online.borrow_mut();
        o.sign_in(Dest::Create);
        o.poll(0);
        o.poll(1);
        match o.sign_in_state() {
            SignIn::Canceled { dest } => {
                assert_eq!((expect, dest), ("canceled", &Dest::Create))
            }
            SignIn::Failed { dest, code } => {
                assert_eq!((code.as_str(), dest), (expect, &Dest::Create))
            }
            other => panic!("{other:?}"),
        }
    }

    // A poll the network lost goes again after a while, not at once.
    let n = Rc::new(RefCell::new(0));
    let c = n.clone();
    let r = rig(
        Platform::Desktop,
        Box::new(move |req| match (req.method, req.path.as_str()) {
            (Method::Post, "/auth/attempts") => {
                ok(r#"{"attempt_id":"a1","poll_secret":"s1","open_url":"u","expires_at_ms":9}"#)
            }
            (Method::Get, "/auth/attempts/a1") => {
                *c.borrow_mut() += 1;
                Some(ApiResponse::unreachable())
            }
            _ => panic!("unexpected {req:?}"),
        }),
    );
    let mut o = r.online.borrow_mut();
    o.sign_in(Dest::Home);
    o.poll(1_000);
    o.poll(1_001);
    o.poll(1_500);
    assert_eq!(*n.borrow(), 1);
    o.poll(3_001);
    o.poll(3_002);
    assert_eq!(*n.borrow(), 2);
    assert!(matches!(o.sign_in_state(), SignIn::Waiting { .. }));
}

#[test]
fn the_web_leaves_for_the_servers_page_and_resumes_the_join_on_return() {
    let cookie = Rc::new(RefCell::new(false));
    let c = cookie.clone();
    let r = rig(
        Platform::Web,
        Box::new(move |req| match (req.method, req.path.as_str()) {
            (Method::Get, "/me") if *c.borrow() => ok(ME),
            (Method::Get, "/me") => err(401, "signed_out"),
            (Method::Get, p) if p.starts_with("/invites/") => ok(
                r#"{"room_id":"Room000001","room_name":"After Hours","host_display_name":"Alice","phase":"lobby","admission":"open","members":2,"expires_at_ms":9}"#,
            ),
            (Method::Post, "/rooms/Room000001/join") => {
                assert!(req.body.as_deref().unwrap().contains(INVITE));
                ok(&format!(
                    r#"{{"room_id":"Room000001","participant":3,"intent":"play","participation":{{"not_ready":{{"stage":1}}}},"already":false,"model":{}}}"#,
                    ddi_room::ROOM_MODEL_REVISION
                ))
            }
            _ => panic!("unexpected {req:?}"),
        }),
    );
    let mut o = r.online.borrow_mut();
    o.open_invite(INVITE);
    // Signed out (the cookie is not there yet): joining signs in first.
    o.poll(0);
    o.join_invite();
    assert_eq!(
        o.take_redirect(),
        Some(Redirect::Page(
            "/auth/twitch/login?return=%2Fmp%2Fjoin%2FRoom000001.abcdefghijABCDEFGHIJ01".into()
        ))
    );
    // The page came back signed in: who it is, then the join again.
    *cookie.borrow_mut() = true;
    o.returned(Dest::Join(INVITE.into()));
    o.poll(1);
    o.poll(2);
    assert_eq!(o.take_entered().as_deref(), Some("Room000001"));
    // The web keeps no token.
    assert!(r.saved.borrow().is_empty());
    o.sign_in_canceled(Dest::Home);
    assert_eq!(o.sign_in_state(), &SignIn::Canceled { dest: Dest::Home });
}

/// Round TW3: Options' "Twitch chat in rooms" turned on without the
/// chat's scope leaves for the trip through Twitch for `chat` (back to the
/// room kept); back with the scope, the setting is made by itself and
/// said once; a canceled trip changes nothing and says so.
#[test]
fn the_devices_chat_trip_makes_its_setting_on_return_or_changes_nothing() {
    use crate::state::settings::Settings;
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    let granted = Rc::new(RefCell::new(false));
    let g = granted.clone();
    let r = rig(
        Platform::Web,
        Box::new(move |req| match (req.method, req.path.as_str()) {
            (Method::Get, "/me") => {
                let mut me: serde_json::Value = serde_json::from_str(ME).unwrap();
                let features: Vec<&str> = if *g.borrow() { vec!["chat"] } else { vec![] };
                me["twitch"] = serde_json::json!({ "features": features, "login": "carol" });
                ok(&me.to_string())
            }
            _ => panic!("unexpected {req:?}"),
        }),
    );
    let mut o = r.online.borrow_mut();
    o.check();
    o.poll(0);
    assert_eq!(Settings::load().twitch_chat, ddi_chat::Level::Off);
    o.connect_twitch_chat(Dest::Room("Room000001".into()), ddi_chat::Level::Messages);
    assert_eq!(
        o.take_redirect(),
        Some(Redirect::Page(
            "/auth/twitch/login?return=%2Fmp%2Froom%2FRoom000001&scopes=chat".into()
        ))
    );
    // Canceled at Twitch: nothing changed, said once.
    o.sign_in_canceled(Dest::Room("Room000001".into()));
    assert_eq!(o.take_chat_outcome(), Some(false));
    assert_eq!(o.take_chat_outcome(), None);
    assert_eq!(Settings::load().twitch_chat, ddi_chat::Level::Off);
    // Again, and back with the scope: the setting made by itself.
    o.clear_sign_in();
    o.connect_twitch_chat(Dest::Room("Room000001".into()), ddi_chat::Level::Messages);
    let _ = o.take_redirect();
    *granted.borrow_mut() = true;
    o.returned(Dest::Room("Room000001".into()));
    assert_eq!(o.take_chat_outcome(), None, "not before /me says so");
    o.poll(1);
    assert_eq!(o.take_chat_outcome(), Some(true));
    assert_eq!(Settings::load().twitch_chat, ddi_chat::Level::Messages);
    assert_eq!(
        ddi_platform::storage::get(ddi_platform::storage::Area::Session, CHAT_WANTED),
        None,
        "forgotten"
    );
}

#[test]
fn an_invites_preview_says_each_state_and_a_refused_join_says_why() {
    use crate::screens::online::{admission_text, invite_problem};
    let cases: Vec<(Option<ApiResponse>, &str)> = vec![
        (
            ok(
                r#"{"room_id":"Room000001","room_name":"R","host_display_name":"Alice","phase":"lobby","admission":"open","members":1}"#,
            ),
            "open",
        ),
        (
            ok(
                r#"{"room_id":"Room000001","room_name":"R","host_display_name":"Alice","phase":"stage","admission":"next_match","members":3}"#,
            ),
            "next_match",
        ),
        (
            ok(
                r#"{"room_id":"Room000001","room_name":"R","host_display_name":"Alice","phase":"lobby","admission":"closed","members":3}"#,
            ),
            "closed",
        ),
        (
            ok(
                r#"{"room_id":"Room000001","room_name":"R","host_display_name":"Alice","phase":"lobby","admission":"full","members":32}"#,
            ),
            "full",
        ),
        (err(410, "invite_expired"), "invite_expired"),
        (err(403, "invite_rotated"), "invite_rotated"),
        (err(404, "invite_unknown"), "invite_unknown"),
        (err(410, "room_closed"), "room_closed"),
        (Some(ApiResponse::unreachable()), "unreachable"),
    ];
    let mut said = Vec::new();
    for (answer, what) in cases {
        let a = RefCell::new(answer);
        let r = rig(
            Platform::Web,
            Box::new(move |req| match req.path.as_str() {
                "/me" => err(401, "signed_out"),
                _ => a.borrow_mut().take(),
            }),
        );
        let mut o = r.online.borrow_mut();
        o.open_invite(INVITE);
        o.poll(0);
        let view = o.invite().unwrap();
        let text: String = match view.preview.as_ref().unwrap() {
            Ok(p) => {
                assert_eq!(
                    format!("{:?}", p.admission).to_lowercase().replace('_', ""),
                    what.replace('_', "")
                );
                admission_text(p.admission).into()
            }
            Err(p) => {
                assert_eq!(p.code, what);
                invite_problem(p).into()
            }
        };
        assert_eq!(o.account(), &Account::SignedOut);
        said.push(text);
    }
    // Each state is worded on its own.
    let distinct: std::collections::BTreeSet<_> = said.iter().collect();
    assert_eq!(distinct.len(), said.len(), "{said:?}");

    // A join refused (the invite rotated during sign-in): the reason, the
    // preview read again, no room entered.
    let joins = Rc::new(RefCell::new(0));
    let j = joins.clone();
    let r = rig(
        Platform::Web,
        Box::new(move |req| match (req.method, req.path.as_str()) {
            (Method::Get, "/me") => ok(ME),
            (Method::Get, _) => err(403, "invite_rotated"),
            (Method::Post, _) => {
                *j.borrow_mut() += 1;
                err(403, "invite_rotated")
            }
            _ => None,
        }),
    );
    let mut o = r.online.borrow_mut();
    o.open_invite(INVITE);
    o.poll(0);
    o.join_invite();
    o.poll(1);
    let v = o.invite().unwrap();
    assert_eq!(v.refused.as_ref().unwrap().code, "invite_rotated");
    assert!(!v.joining);
    assert_eq!(o.take_entered(), None);
    assert_eq!(*joins.borrow(), 1);
}

pub(crate) mod room {
    //! An online room's client over the fake room.

    use super::*;
    use crate::state::room::{
        LoadProblem, Picked, Refusal, RoomClient, Sharing, SharingProblem, UploadJob,
    };
    use ddi_room::fake::{FakeServer, FakeSocket, Fault, SharedFake};
    use ddi_room::{
        ClientId, Command, ContentStatus, Identity, Intent, LinkState, ManualClock, Phase,
        PhaseKind, Role, RoomPolicy, Run,
    };

    /// `who`'s online state, its HTTP the fake room's: tickets, the
    /// summary, the invite and stages' upload grants (for operators, as
    /// the server decides them; numbered `grant-<who>-<n>`).
    pub(crate) fn person(server: &SharedFake, who: &'static str) -> SharedOnline {
        signed_in(handler_of(server, who))
    }

    /// [`person`]'s HTTP.
    pub(crate) fn handler_of(server: &SharedFake, who: &'static str) -> Handler {
        let mut grants = 0;
        let s = server.clone();
        let handler: Handler = Box::new(move |req| {
            let identity = Identity::new(format!("dev:{who}"));
            // The room model the request says (round B1), as the server
            // reads it.
            let model = req
                .body
                .as_deref()
                .and_then(|b| serde_json::from_str::<serde_json::Value>(b).ok())
                .and_then(|v| v.get("model").and_then(|m| m.as_u64()))
                .map(|m| m as u32);
            match (req.method, req.path.as_str()) {
                (Method::Post, "/rooms/Room000001/ticket")
                    if s.borrow().admit_ticket(model).is_err() =>
                {
                    err(403, ddi_room::UPDATE_REQUIRED)
                }
                (Method::Post, "/rooms/Room000001/join") if s.borrow().admit_join(model).is_err() => {
                    err(409, ddi_room::UPDATE_REQUIRED)
                }
                (Method::Get, "/me") if s.borrow().signed_out(&identity) => {
                    err(401, "signed_out")
                }
                // With the Twitch access the fake grants (round TW1b).
                (Method::Get, "/me") => {
                    let features = s.borrow().twitch_features(&identity);
                    let mut me: serde_json::Value = serde_json::from_str(&me_of(who)).unwrap();
                    me["twitch"] = serde_json::json!({ "features": features, "login": who });
                    ok(&me.to_string())
                }
                // Revision 18: the room's Twitch setting, as the server
                // gives it (`FakeServer::grant_twitch` the access).
                (Method::Post, "/rooms/Room000001/twitch") => {
                    let (status, body) = s
                        .borrow_mut()
                        .answer_http(
                            Some(who),
                            ddi_room::fake::HttpRoom {
                                name: "After Hours",
                                grants_anyone: false,
                            },
                            ("POST", "/rooms/Room000001/twitch", req.body.as_deref()),
                            None,
                        )
                        .expect("answered");
                    Some(ApiResponse {
                        status,
                        body,
                        session: None,
                    })
                }
                (Method::Get, "/rooms/Room000001") => ok(
                    r#"{"room_id":"Room000001","name":"After Hours","revision":1,"phase":"lobby","admission":"open","participant":1}"#,
                ),
                // As the server answers it, its tickets' bound too where
                // the fake has it on (`FakeServer::set_bounds`).
                (Method::Post, "/rooms/Room000001/ticket") => {
                    match s.borrow_mut().ticket_bounded(&identity) {
                        Ok(body) => ok(&body),
                        Err((429, code)) => err(429, code),
                        Err((401, code)) => err(401, code),
                        Err(_) => err(403, "not_a_member"),
                    }
                }
                // The invite's preview and the join, as the server
                // answers them from the room (`players_full`, the
                // reducer's join).
                (Method::Get, p) if p.starts_with("/invites/") => {
                    let full = s.borrow().state().players_full();
                    let kind = if p.ends_with(WATCH_INVITE) { "watch" } else { "play" };
                    ok(&format!(
                        r#"{{"room_id":"Room000001","room_name":"After Hours","host_display_name":"alice","phase":"lobby","admission":"open","players_full":{full},"kind":"{kind}","members":2}}"#
                    ))
                }
                (Method::Post, "/rooms/Room000001/join") => {
                    let already = s
                        .borrow()
                        .state()
                        .member_by_identity(&identity)
                        .is_some_and(|m| !m.left);
                    let watch = req.body.as_deref().unwrap_or("").contains(WATCH_INVITE);
                    let joined = if watch {
                        s.borrow_mut().join_watch_only(identity.clone(), who)
                    } else {
                        s.borrow_mut().join(identity.clone(), who, Intent::Play)
                    };
                    // The host's Twitch lists keep them out (revision 18).
                    let id = match joined {
                        Err(ddi_room::Rejection::NotAdmitted) => return err(403, "not_admitted"),
                        j => j.unwrap(),
                    };
                    let intent = match s.borrow().state().member(id).unwrap().intent {
                        Intent::Play => "play",
                        Intent::Watch => "watch",
                    };
                    ok(&format!(
                        r#"{{"room_id":"Room000001","participant":{},"intent":"{intent}","participation":"watching","already":{already},"model":{}}}"#,
                        id.0,
                        s.borrow().model()
                    ))
                }
                (Method::Get, "/rooms/Room000001/invite") if who == "alice" => ok(&format!(
                    r#"{{"invite":"{INVITE}","watch_invite":"{WATCH_INVITE}","expires_at_ms":9,"admission":"open"}}"#
                )),
                (Method::Get, "/rooms/Room000001/invite") => err(403, "not_an_operator"),
                // Revision 17: an entry's upload grant, its adder's.
                (Method::Post, "/rooms/Room000001/songs/grant") => {
                    let srv = s.borrow();
                    let song = req
                        .body
                        .as_deref()
                        .and_then(|b| serde_json::from_str::<serde_json::Value>(b).ok())
                        .and_then(|v| v.get("song").and_then(|m| m.as_u64()))
                        .unwrap_or(0);
                    let adder = srv
                        .state()
                        .songs
                        .get(ddi_room::RoomSongId(song))
                        .and_then(|e| srv.state().member(e.by))
                        .is_some_and(|m| m.identity() == Some(&identity));
                    if !adder {
                        return err(403, "not_the_adder");
                    }
                    grants += 1;
                    ok(&format!(
                        r#"{{"grant":"grant-{who}-{grants}","expires_at_ms":9,"max_bytes":100.0}}"#
                    ))
                }
                (Method::Post, "/rooms/Room000001/grant") => {
                    let srv = s.borrow();
                    let operator = srv
                        .state()
                        .member_by_identity(&identity)
                        .is_some_and(|m| m.admin || matches!(m.role, Role::Owner | Role::CoHost));
                    if !operator {
                        return err(403, "not_an_operator");
                    }
                    grants += 1;
                    ok(&format!(
                        r#"{{"grant":"grant-{who}-{grants}","expires_at_ms":9,"max_bytes":100.0}}"#
                    ))
                }
                _ => panic!("unexpected {req:?}"),
            }
        });
        handler
    }

    pub(crate) fn client(
        server: &SharedFake,
        online: SharedOnline,
        who: &str,
        now: u64,
    ) -> RoomClient {
        RoomClient::online(
            online,
            "Room000001",
            Box::new(FakeSocket::new(server.clone())),
            ClientId::new(format!("{who}-web")),
            now,
        )
    }

    /// The shell's uploads of `room`, as the server ends them: the room
    /// told by the server (the Worker's word on `finish`) that the stage is
    /// available with share `share`, the client that its upload ended.
    pub(crate) fn upload(
        server: &SharedFake,
        room: &mut RoomClient,
        share: &str,
    ) -> Vec<UploadJob> {
        let jobs = room.take_upload_jobs();
        for j in &jobs {
            room.upload_progress(j.stage, 50.0, 100.0);
            let told = if j.stage == crate::state::room::SPOTLIGHT {
                Command::SetSpotlightShare {
                    chart: j.content.chart.clone(),
                    share: share.into(),
                }
            } else {
                Command::SetContentStatus {
                    stage: j.stage,
                    status: ContentStatus::Available,
                    share: Some(share.into()),
                }
            };
            server.borrow_mut().authority(told).unwrap();
            room.upload_finished(j.stage, Ok(share.into()));
        }
        jobs
    }

    /// Every song `room` asked for prepared, as a shell does: from the
    /// library, or from the room's share (read as [`song`], its id the
    /// shared one).
    pub(crate) fn prepare(room: &mut RoomClient) {
        for job in room.take_load_jobs() {
            if job.share.is_some() {
                let s = song();
                let picked = Picked {
                    song: crate::screens::data::SongEntry {
                        id: job.song.clone(),
                        ..s.clone()
                    },
                    chart: s.charts[0].clone(),
                };
                room.finish_shared_load(&job, Ok(picked));
            } else {
                room.finish_load(&job, Ok(()));
            }
        }
    }

    pub(crate) fn tick(clock: &ManualClock, rooms: &mut [&mut RoomClient], ms: u64) {
        for _ in 0..4 {
            clock.advance(ms / 4);
            let now = ddi_room::Clock::now(clock).0;
            for r in rooms.iter_mut() {
                r.tick(now);
            }
        }
    }

    pub(crate) fn song() -> crate::screens::data::SongEntry {
        crate::screens::data::SongEntry {
            id: "bundled:a".into(),
            title: "A".into(),
            charts: vec![crate::screens::data::ChartEntry {
                style: "dance-single".into(),
                digest: Some(ddi_chart::ChartDigest(7)),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    pub(crate) fn room_of(clock: &ManualClock, others: &[&str]) -> SharedFake {
        let server = FakeServer::hosted(
            "Room000001",
            Identity::new("dev:alice"),
            "alice",
            RoomPolicy::default(),
            clock.clone(),
        )
        .shared();
        for who in others {
            server
                .borrow_mut()
                .join(Identity::new(format!("dev:{who}")), who, Intent::Play)
                .unwrap();
        }
        server
    }

    /// A room whose player places are all taken (a cap of two): the
    /// invite's page says the join is to watch, the join is in to watch,
    /// the room's status line says so for a while and the player's line
    /// says Watching; Play is refused in plain words while it is full, and
    /// taken once a place frees up (nobody moved to it meanwhile).
    #[test]
    fn a_join_to_a_room_full_of_players_is_told_and_watches() {
        use crate::screens::online::{admission_text, preview_text};
        use crate::screens::room::{refusal_text, status};
        use ddi_l10n::msg::room as m;
        let clock = ManualClock::new(1_000);
        let server = FakeServer::hosted(
            "Room000001",
            Identity::new("dev:alice"),
            "alice",
            RoomPolicy {
                max_competitors: 2,
                ..RoomPolicy::default()
            },
            clock.clone(),
        )
        .shared();
        let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
        // Bob's join takes the last player's place.
        let bob_online = person(&server, "bob");
        bob_online.borrow_mut().open_invite(INVITE);
        bob_online.borrow_mut().poll(1);
        let bob_said: String = {
            let o = bob_online.borrow();
            preview_text(o.invite().unwrap().preview.as_ref().unwrap().as_ref().unwrap()).into()
        };
        let open: String = admission_text(super::api::Admission::Open).into();
        assert_eq!(bob_said, open);
        bob_online.borrow_mut().join_invite();
        bob_online.borrow_mut().poll(2);
        assert_eq!(bob_online.borrow_mut().take_entered().as_deref(), Some("Room000001"));
        assert!(!bob_online.borrow_mut().take_seated_to_watch("Room000001"));
        // Carol's invite page: full, she'll join to watch.
        let carol_online = person(&server, "carol");
        carol_online.borrow_mut().open_invite(INVITE);
        carol_online.borrow_mut().poll(3);
        let carol_said: String = {
            let o = carol_online.borrow();
            preview_text(o.invite().unwrap().preview.as_ref().unwrap().as_ref().unwrap()).into()
        };
        let full_watch: String = m::invite_full_watch().into();
        assert_eq!(carol_said, full_watch);
        carol_online.borrow_mut().join_invite();
        carol_online.borrow_mut().poll(4);
        assert_eq!(carol_online.borrow_mut().take_entered().as_deref(), Some("Room000001"));
        let mut bob = client(&server, bob_online, "bob", 1_000);
        let mut carol = client(&server, carol_online.clone(), "carol", 1_000);
        // Said once: a link made again for the room says nothing.
        assert!(!carol_online.borrow_mut().take_seated_to_watch("Room000001"));
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
        assert_eq!(carol.link_state(), LinkState::Connected);
        assert!(carol.watching());
        let told: String = m::joined_to_watch().into();
        assert_eq!(String::from(status(&carol)), told);
        assert_ne!(String::from(status(&bob)), told);
        let rows = crate::screens::room::player_rows(&carol);
        let row = rows.iter().find(|r| r.me).unwrap();
        assert_eq!(row.status, String::from(m::watching()));
        // Play while full: refused, in plain words.
        assert!(carol.set_intent(Intent::Play).is_err());
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
        let refused = carol.refusal().expect("refused while full");
        assert_eq!(
            String::from(refusal_text(refused)),
            String::from(m::refused_players_full())
        );
        assert!(carol.watching());
        // The line goes after a while, the room's own status back.
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 13_000);
        assert_eq!(carol.link_notice(), None);
        // Bob watches: a place frees up, Carol is not moved to it…
        bob.set_intent(Intent::Watch).unwrap();
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
        assert!(carol.watching());
        // …she takes it.
        carol.set_intent(Intent::Play).unwrap();
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
        assert!(!carol.watching());
        assert_eq!(
            server.borrow().state().member_by_identity(&Identity::new("dev:carol")).unwrap().intent,
            Intent::Play
        );
    }

    /// The watch-only link (part 2): the host copies it beside the
    /// room's invite; its page invites to watch with a Watch button; the
    /// join is in to watch and held to it (no "full" line, no Play
    /// offered), until the host's Let play, after which Play is theirs to
    /// choose.
    #[test]
    fn a_watch_only_link_joins_to_watch_until_the_host_lets_them_play() {
        use crate::screens::online::preview_text;
        use crate::screens::room::{more_items, refusal_text};
        use ddi_l10n::msg::room as m;
        let clock = ManualClock::new(1_000);
        let server = room_of(&clock, &["bob"]);
        let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
        let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        alice.fetch_invite();
        tick(&clock, &mut [&mut alice], 400);
        let link = alice.watch_invite_link().expect("the watch-only link");
        assert!(link.ends_with(&format!("#/mp/join/{WATCH_INVITE}")));
        // Kept but hidden (round B1): its Copy only under the hidden
        // option.
        let items: Vec<String> = more_items(&alice, false).into_iter().map(|(id, _)| id).collect();
        assert!(!items.iter().any(|i| i == "copywatch"), "{items:?}");
        let mut settings = crate::state::settings::Settings::load();
        assert!(!settings.multiplayer_watch_link, "off by default");
        settings.multiplayer_watch_link = true;
        settings.save();
        let items: Vec<String> = more_items(&alice, false).into_iter().map(|(id, _)| id).collect();
        let at = items.iter().position(|i| i == "copy").unwrap();
        assert_eq!(items[at + 1], "copywatch");
        // The fan's page: invited to watch, the button Watch.
        let fan_online = person(&server, "fan");
        fan_online.borrow_mut().open_invite(WATCH_INVITE);
        fan_online.borrow_mut().poll(1);
        let said: String = {
            let o = fan_online.borrow();
            preview_text(o.invite().unwrap().preview.as_ref().unwrap().as_ref().unwrap()).into()
        };
        assert_eq!(said, String::from(m::invite_watch()));
        fan_online.borrow_mut().join_invite();
        fan_online.borrow_mut().poll(2);
        assert_eq!(fan_online.borrow_mut().take_entered().as_deref(), Some("Room000001"));
        // Not said to be full: the page said why already.
        assert!(!fan_online.borrow_mut().take_seated_to_watch("Room000001"));
        let mut fan = client(&server, fan_online, "fan", 1_000);
        tick(&clock, &mut [&mut alice, &mut fan], 400);
        assert!(fan.watching());
        assert!(fan.watch_only());
        assert!(!fan.may_choose_intent());
        assert!(!more_items(&fan, false).iter().any(|(id, _)| id == "intent"));
        assert_eq!(fan.link_notice(), None);
        // An older screen's Play: refused in words of its own.
        assert!(fan.set_intent(Intent::Play).is_err());
        assert_eq!(
            String::from(refusal_text(fan.refusal().unwrap())),
            String::from(m::refused_watch_only())
        );
        // A co-host lets them play (round B1; Host options offered them
        // for it, never the owner's rows); they choose it.
        let id = fan.me();
        assert!(!bob.may_host_options());
        alice.set_co_host(bob.me(), true).unwrap();
        tick(&clock, &mut [&mut alice, &mut bob, &mut fan], 400);
        assert!(bob.may_host_options() && !bob.manages_hosts());
        bob.let_play(id).unwrap();
        tick(&clock, &mut [&mut alice, &mut bob, &mut fan], 400);
        assert!(!fan.watch_only());
        assert!(!bob.may_host_options(), "nothing left for a co-host there");
        assert!(fan.watching(), "let, not moved");
        assert!(fan.may_choose_intent());
        fan.set_intent(Intent::Play).unwrap();
        tick(&clock, &mut [&mut alice, &mut fan], 400);
        assert!(!fan.watching());
    }

    /// Round TW3: the room's Twitch setting ending without its owner's
    /// asking is said on the status line: to them where their access
    /// ended (the server turned it off), to everyone where they left;
    /// never where they turned it off themselves.
    #[test]
    fn a_twitch_setting_that_ends_by_itself_is_said() {
        use crate::state::online::link::LinkNotice;
        let clock = ManualClock::new(1_000);
        let server = room_of(&clock, &["bob"]);
        let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
        let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        server.borrow_mut().grant_twitch(&ddi_room::Identity::new("dev:alice"), &["bans"]);
        let me = alice.me();
        let on = || ddi_room::TwitchSetting {
            by: me,
            mirror: ddi_room::TwitchMirror::Bans,
            chat: ddi_room::TwitchChat::Off,
            user_id: "1".into(),
            login: "alice".into(),
        };
        let set = |s: Option<ddi_room::TwitchSetting>| server.borrow_mut().set_twitch(me, s).unwrap();
        set(Some(on()));
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        assert_eq!(alice.twitch().map(|t| t.mirror), Some(ddi_room::TwitchMirror::Bans));
        // Her access refused at Twitch: the server turned it off.
        set(None);
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        assert_eq!(alice.link_notice(), Some(LinkNotice::TwitchEnded));
        assert_eq!(bob.link_notice(), None);
        // Turned off by her own hand: nothing said.
        set(Some(on()));
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        alice.set_link_notice(None);
        alice.set_twitch(ddi_room::TwitchMirror::Off, ddi_room::TwitchChat::Off);
        set(None);
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        assert_eq!(alice.link_notice(), None);
        // She leaves with it on, the room passed on: everyone is told.
        set(Some(on()));
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        alice.leave();
        tick(&clock, &mut [&mut alice, &mut bob], 800);
        assert_eq!(server.borrow().state().twitch, None);
        assert_eq!(bob.link_notice(), Some(LinkNotice::TwitchSetterLeft { who: me }));
    }

    /// Round TW7: a player whose device has "Twitch chat in rooms" on, in
    /// a room that shows its host's chat, whose Twitch access does not
    /// allow reading chat, is told once what to do; with the access, or
    /// the option at its default, nothing is said.
    #[test]
    fn a_player_whose_access_cannot_show_the_chat_is_told_once() {
        use crate::state::online::link::LinkNotice;
        ddi_platform::storage::install(std::rc::Rc::new(ddi_platform::storage::MemoryStore::default()));
        crate::state::settings::Settings::default().save();
        let clock = ManualClock::new(1_000);
        let server = room_of(&clock, &["bob"]);
        server.borrow_mut().grant_twitch(&ddi_room::Identity::new("dev:alice"), &["chat"]);
        server.borrow_mut().grant_twitch(&ddi_room::Identity::new("dev:bob"), &["bans"]);
        let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
        let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        let me = alice.me();
        server
            .borrow_mut()
            .set_twitch(
                me,
                Some(ddi_room::TwitchSetting {
                    by: me,
                    mirror: ddi_room::TwitchMirror::Off,
                    chat: ddi_room::TwitchChat::Emotes,
                    user_id: "1".into(),
                    login: "alice".into(),
                }),
            )
            .unwrap();
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        assert_eq!(bob.link_notice(), None, "the option at its default: nothing");
        let mut s = crate::state::settings::Settings::load();
        s.twitch_chat = ddi_chat::Level::Messages;
        s.save();
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        assert_eq!(bob.link_notice(), Some(LinkNotice::TwitchChatNoAccess));
        assert_eq!(alice.link_notice(), None, "her access allows it");
        bob.set_link_notice(None);
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        assert_eq!(bob.link_notice(), None, "once");
        crate::state::settings::Settings::default().save();
    }

    /// Round FX: where the room's line says the access lacks chat's
    /// permission, the room offers the trip itself: a button beside the
    /// line and More's item, back to this room. Canceled at Twitch,
    /// nothing changes and the line stays; the room asks no more this
    /// session (another channel shown says nothing), More still offers it;
    /// asked again and allowed, the line says the chat is on and nothing
    /// is offered any more. A player whose access allows it is offered
    /// nothing.
    #[test]
    fn a_room_offers_the_trip_that_allows_chat_and_asks_no_more_after_a_no() {
        use crate::screens::room::{allow_chat_shown, more_items};
        use crate::state::online::link::LinkNotice;
        use crate::state::online::{Dest, Redirect};
        ddi_platform::storage::install(std::rc::Rc::new(ddi_platform::storage::MemoryStore::default()));
        crate::state::settings::Settings {
            twitch_chat: ddi_chat::Level::Messages,
            ..Default::default()
        }
        .save();
        let clock = ManualClock::new(1_000);
        let server = room_of(&clock, &["bob"]);
        server.borrow_mut().grant_twitch(&ddi_room::Identity::new("dev:alice"), &["chat"]);
        server.borrow_mut().grant_twitch(&ddi_room::Identity::new("dev:bob"), &["bans"]);
        let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
        let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        assert!(!bob.chat_allow_offered(), "the room shows no chat: nothing");
        let me = alice.me();
        let shown = |channel: &str| ddi_room::TwitchSetting {
            by: me,
            mirror: ddi_room::TwitchMirror::Off,
            chat: ddi_room::TwitchChat::Emotes,
            user_id: channel.into(),
            login: "alice".into(),
        };
        server.borrow_mut().set_twitch(me, Some(shown("1"))).unwrap();
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        assert_eq!(bob.link_notice(), Some(LinkNotice::TwitchChatNoAccess));
        assert!(bob.chat_allow_offered() && allow_chat_shown(&bob));
        assert!(more_items(&bob, false).iter().any(|(id, _)| id == "allowchat"));
        assert!(!alice.chat_allow_offered(), "her access allows it");
        assert!(!more_items(&alice, false).iter().any(|(id, _)| id == "allowchat"));
        let room = "Room000001".to_string();
        let trip = || Some(Redirect::Page(
            "/auth/twitch/login?return=%2Fmp%2Froom%2FRoom000001&scopes=chat".into(),
        ));
        // The step: through Twitch, back to this room.
        assert!(bob.allow_chat());
        let online = bob.online_link().unwrap().online().clone();
        assert_eq!(online.borrow_mut().take_redirect(), trip());
        // Canceled at Twitch: nothing changed, the line and its button
        // back.
        bob.set_link_notice(None);
        online.borrow_mut().sign_in_canceled(Dest::Room(room.clone()));
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        assert_eq!(bob.link_notice(), Some(LinkNotice::TwitchChatNoAccess));
        assert!(allow_chat_shown(&bob));
        assert_eq!(
            crate::state::settings::Settings::load().twitch_chat,
            ddi_chat::Level::Messages
        );
        // Asked no more this session: another channel shown says nothing;
        // More still offers it.
        bob.set_link_notice(None);
        server.borrow_mut().set_twitch(me, Some(shown("2"))).unwrap();
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        assert_eq!(bob.link_notice(), None);
        assert!(!allow_chat_shown(&bob));
        assert!(more_items(&bob, false).iter().any(|(id, _)| id == "allowchat"));
        // He asks (More), and allows it at Twitch: the chat is on, from the
        // next stage; nothing is offered any more.
        online.borrow_mut().clear_sign_in();
        assert!(bob.allow_chat());
        assert_eq!(online.borrow_mut().take_redirect(), trip());
        server.borrow_mut().grant_twitch(&ddi_room::Identity::new("dev:bob"), &["bans", "chat"]);
        online.borrow_mut().returned(Dest::Room(room));
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        assert_eq!(bob.link_notice(), Some(LinkNotice::TwitchChatOn));
        assert!(!bob.chat_allow_offered());
        assert!(!more_items(&bob, false).iter().any(|(id, _)| id == "allowchat"));
        crate::state::settings::Settings::default().save();
    }

    /// Round TW3: a newcomer the host's Twitch lists keep out sees the one
    /// neutral line, and never "You can join now" above it (the browser
    /// walk found both); Join stays, for a timeout that ends.
    #[test]
    fn a_refused_newcomers_invite_page_says_only_the_neutral_line() {
        use crate::screens::online::{invite_page, invite_texts};
        use ddi_l10n::msg::room as m;
        let clock = ManualClock::new(1_000);
        let server = room_of(&clock, &["bob"]);
        let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
        tick(&clock, &mut [&mut alice], 400);
        server.borrow_mut().grant_twitch(&ddi_room::Identity::new("dev:alice"), &["bans"]);
        let me = alice.me();
        server
            .borrow_mut()
            .set_twitch(
                me,
                Some(ddi_room::TwitchSetting {
                    by: me,
                    mirror: ddi_room::TwitchMirror::Bans,
                    chat: ddi_room::TwitchChat::Off,
                    user_id: "1".into(),
                    login: "alice".into(),
                }),
            )
            .unwrap();
        server.borrow_mut().bar(&ddi_room::Identity::new("dev:carol"), true);
        let carol = person(&server, "carol");
        carol.borrow_mut().open_invite(INVITE);
        carol.borrow_mut().poll(1);
        let open: String = crate::screens::online::admission_text(crate::state::online::api::Admission::Open).into();
        assert!(invite_texts(&carol.borrow()).contains(&open));
        carol.borrow_mut().join_invite();
        carol.borrow_mut().poll(2);
        carol.borrow_mut().poll(3);
        assert_eq!(carol.borrow_mut().take_entered(), None);
        let (notes, actions) = invite_page(&carol.borrow());
        assert!(notes.contains(&String::from(m::link_not_admitted())), "{notes:?}");
        assert!(!invite_texts(&carol.borrow()).contains(&open), "{:?}", invite_texts(&carol.borrow()));
        assert!(actions.iter().any(|a| a == "join"), "{actions:?}");
    }

    /// "Reload to update" (round B1): an authority whose room model is
    /// newer refuses this client's join with a page saying so (a Reload on
    /// the web), and a member's ticket between stages (the room's status
    /// line, More's Reload); never during a stage, so a deploy throws
    /// nobody out of one.
    #[test]
    fn an_authority_ahead_refuses_an_older_client_between_stages() {
        use crate::screens::online::RELOAD;
        use crate::screens::room::{MORE_RELOAD, more_items, status};
        use ddi_l10n::msg::room as m;
        let clock = ManualClock::new(1_000);
        let server = room_of(&clock, &["bob"]);
        let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
        tick(&clock, &mut [&mut alice], 400);
        assert_eq!(alice.link_state(), LinkState::Connected);
        // The server deployed ahead of this build.
        server.borrow_mut().set_model(ddi_room::ROOM_MODEL_REVISION + 1);
        // A newcomer's join: refused, said plainly, Reload offered.
        let carol = person(&server, "carol");
        carol.borrow_mut().open_invite(INVITE);
        carol.borrow_mut().poll(1);
        carol.borrow_mut().join_invite();
        carol.borrow_mut().poll(2);
        assert_eq!(carol.borrow_mut().take_entered(), None);
        let page = crate::screens::online::invite_page(&carol.borrow());
        assert!(page.0.contains(&String::from(m::update_reload())), "{page:?}");
        assert!(page.1.iter().any(|a| a == RELOAD), "{page:?}");
        // A member's reconnect during a stage keeps them in it.
        server.borrow_mut().rewrite(|s| {
            s.phase = ddi_room::Phase::Stage {
                stage: ddi_room::StageId(1),
            };
        });
        tick(&clock, &mut [&mut alice], 400);
        server.borrow_mut().drop_socket(alice.me());
        tick(&clock, &mut [&mut alice], 2_000);
        assert_eq!(alice.link_state(), LinkState::Connected);
        // Between stages: refused.
        server.borrow_mut().rewrite(|s| s.phase = ddi_room::Phase::Lobby);
        tick(&clock, &mut [&mut alice], 400);
        server.borrow_mut().drop_socket(alice.me());
        tick(&clock, &mut [&mut alice], 2_000);
        assert_eq!(alice.link_state(), LinkState::UpdateRequired);
        assert_eq!(String::from(status(&alice)), String::from(m::update_reload()));
        assert!(more_items(&alice, false).iter().any(|(id, _)| id == MORE_RELOAD));
        // A client as new as the server is admitted.
        server.borrow_mut().set_model(ddi_room::ROOM_MODEL_REVISION);
        assert!(server.borrow().admit_join(Some(ddi_room::ROOM_MODEL_REVISION)).is_ok());
        assert!(server.borrow().admit_join(None).is_err(), "one that says none is too old");
    }

    /// The check both ways (round R2): a server whose answers say an
    /// older room model than this client's, or none (a server from
    /// before revision 9), is refused by the client: the invite's page
    /// says the server is being updated with Join there to try again,
    /// and a member's link opens no socket, saying the same on the room's
    /// status line while it asks again after a wait; a server of this
    /// client's revision lets both in.
    #[test]
    fn a_server_behind_this_client_is_refused_and_opens_no_socket() {
        use crate::screens::room::status;
        use ddi_l10n::msg::room as m;
        let rev = ddi_room::ROOM_MODEL_REVISION;
        for said in [Some(rev - 1), None, Some(rev)] {
            let clock = ManualClock::new(1_000);
            let server = room_of(&clock, &["bob"]);
            if let Some(model) = said {
                server.borrow_mut().set_model(model);
            }
            // The server's answers as a server of `said` makes them (none:
            // without the field).
            let answered = |who| -> SharedOnline {
                let mut inner = handler_of(&server, who);
                signed_in(Box::new(move |req| {
                    let mut r = inner(req)?;
                    if said.is_none()
                        && let Ok(serde_json::Value::Object(mut doc)) =
                            serde_json::from_str::<serde_json::Value>(&r.body)
                    {
                        doc.remove("model");
                        r.body = serde_json::Value::Object(doc).to_string();
                    }
                    Some(r)
                }))
            };
            let current = said == Some(rev);
            // A newcomer's join.
            let carol = answered("carol");
            carol.borrow_mut().open_invite(INVITE);
            carol.borrow_mut().poll(1);
            carol.borrow_mut().join_invite();
            carol.borrow_mut().poll(2);
            let entered = carol.borrow_mut().take_entered();
            let page = crate::screens::online::invite_page(&carol.borrow());
            let updating = String::from(m::server_updating());
            if current {
                assert_eq!(entered.as_deref(), Some("Room000001"), "{said:?}");
                assert!(!page.0.contains(&updating), "{page:?}");
            } else {
                assert_eq!(entered, None, "{said:?}");
                assert!(page.0.contains(&updating), "{said:?}: {page:?}");
                assert!(page.1.iter().any(|a| a == "join"), "Join again: {page:?}");
            }
            // A member's link.
            let mut bob = client(&server, answered("bob"), "bob", 1_000);
            tick(&clock, &mut [&mut bob], 400);
            if current {
                assert_eq!(bob.link_state(), LinkState::Connected, "{said:?}");
            } else {
                assert!(
                    matches!(bob.link_state(), LinkState::ServerBehind { .. }),
                    "{said:?}: {:?}",
                    bob.link_state()
                );
                assert_eq!(String::from(status(&bob)), updating);
                assert_eq!(server.borrow().sockets(), 0, "no socket");
                // Asked again after a wait, in once the server is current.
                let asked = |bob: &RoomClient| {
                    let LinkState::ServerBehind { attempt } = bob.link_state() else {
                        return 0;
                    };
                    attempt
                };
                let before = asked(&bob);
                tick(&clock, &mut [&mut bob], 4_000);
                assert!(asked(&bob) > before, "asked again");
                server.borrow_mut().set_model(rev);
                if said.is_none() {
                    // The handler strips what the server says: a fresh
                    // one for the server brought up to date.
                    bob = client(&server, person(&server, "bob"), "bob", 1_000);
                }
                tick(&clock, &mut [&mut bob], 20_000);
                assert_eq!(bob.link_state(), LinkState::Connected, "{said:?}");
            }
        }
    }

    pub(crate) fn shared_pick(s: &crate::screens::data::SongEntry) -> Picked {
        Picked {
            song: crate::screens::data::SongEntry {
                id: "shared:abc12345:0".into(),
                ..s.clone()
            },
            chart: s.charts[0].clone(),
        }
    }

    /// M7b: the host's song select rests on a song: after the rest the
    /// room shows it to the others and it is uploaded under the room's
    /// grant ahead of any pick; a member reads it from the share (its
    /// banner and preview). The pick reuses that share (nothing uploaded
    /// again), opens every member's prompt once they have the song, the
    /// ready ones start at the deadline, the one still loading sits out;
    /// each plays on their own device and reports their run.
    #[test]
    fn the_spotlight_is_shared_ahead_the_pick_reuses_it_and_the_stage_is_played() {
        let clock = ManualClock::new(1_000);
        let server = room_of(&clock, &["bob", "carol"]);
        let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
        let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
        let mut carol = client(&server, person(&server, "carol"), "carol", 1_000);
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
        assert_eq!(alice.link_state(), LinkState::Connected);
        assert_eq!(alice.name(), Some("After Hours"));
        assert!(alice.may(ddi_room::Capability::ChooseContent));
        assert!(!bob.may(ddi_room::Capability::ChooseContent));
        alice.fetch_invite();
        bob.fetch_invite();
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        assert_eq!(
            alice.invite_link().as_deref(),
            Some("http://localhost:8080/#/mp/join/Room000001.abcdefghijABCDEFGHIJ01")
        );
        assert_eq!(bob.invite_link(), None);
        // Revision 17: the host adds a song of her library to the room's
        // list: listed for everyone at once (uploading), uploaded under
        // the room's grant for that entry (once), there for everyone once
        // the server says so; a member may not add.
        let mut s = song();
        s.pack = Some("Pack".into());
        let mut other = song();
        other.id = "bundled:b".into();
        other.charts[0].digest = Some(ddi_chart::ChartDigest(8));
        alice.set_library(&[s.clone(), other.clone()]);
        bob.set_library(std::slice::from_ref(&other));
        assert!(alice.may_add() && !bob.may_add());
        assert!(bob.add_song(&other).is_err());
        alice.add_song(&s).unwrap();
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
        let entry = bob.room_songs()[0].clone();
        assert_eq!((entry.card.title.as_str(), entry.by), ("A", alice.me()));
        assert!(matches!(bob.entry_state(&entry), crate::state::room::EntryState::Uploading { .. }));
        assert_eq!(alice.listed(&s).map(|e| e.id), Some(entry.id));
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
        let jobs = alice.take_upload_jobs();
        assert_eq!(jobs.len(), 1);
        assert_eq!(
            (jobs[0].entry, jobs[0].grant.as_str(), jobs[0].song.id.as_str()),
            (Some(entry.id), "grant-alice-1", s.id.as_str())
        );
        // Its progress is a change; the server's word makes it there.
        let before = alice.version();
        alice.song_upload_progress(entry.id, 1.0, 2.0);
        assert!(alice.version() > before);
        server
            .borrow_mut()
            .authority(Command::SetSongContent {
                song: entry.id,
                status: ContentStatus::Available,
                share: Some("abc12345".into()),
                charts: None,
            })
            .unwrap();
        alice.song_upload_finished(entry.id, Ok("abc12345".into()));
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
        assert!(alice.take_upload_jobs().is_empty(), "once");
        let entry = bob.room_songs()[0].clone();
        assert_eq!(bob.entry_state(&entry), crate::state::room::EntryState::NotLoaded);
        assert_eq!(alice.entry_state(&entry), crate::state::room::EntryState::Ready);
        // Nothing is read ahead: nobody follows the picker any more.
        assert!(bob.take_load_jobs().is_empty());
        let _ = carol.take_load_jobs();
        // Bob may not pick: refused by the room.
        let chart = entry.charts[0].chart.clone();
        assert!(bob.pick_listed(entry.id, &chart).is_err());
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        assert!(matches!(
            bob.refusal(),
            Some(Refusal::Room(ddi_room::Rejection::Unauthorized { .. }))
        ));
        // An online room does not run attacks yet. Its rules name no
        // battle with them (`MatchRules::hosted`, whoever made the room);
        // and should they (a client that sent the practice room's
        // battle), nothing of it is played here and nothing says it is.
        assert!(!alice.rules().attacks());
        let mut with = alice.rules().clone();
        with.set_attacks(true);
        alice.save_rules(with).unwrap();
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
        assert!(bob.rules().attacks(), "the rule as sent");
        for r in [&alice, &bob] {
            assert!(!r.attacks_offered() && !r.attacks_on());
            assert!(!r.attacks_played(r.rules()));
        }
        // The pick from the list: the match starts, the stage available
        // with the entry's share at once; nothing to upload again.
        alice.pick_listed(entry.id, &chart).unwrap();
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
        let stage = alice.stages()[0].id;
        let content = bob.stages()[0].content.clone().unwrap();
        assert_eq!(
            (content.status, content.share.as_deref()),
            (ContentStatus::Available, Some("abc12345"))
        );
        assert_eq!(alice.sharing(stage), None);
        assert!(
            alice.take_upload_jobs().is_empty(),
            "the pick sends nothing"
        );
        // Each prepares it: Alice from her library, Bob from the share.
        let a = alice.take_load_jobs();
        assert_eq!((a.len(), a[0].share.as_deref()), (1, None));
        let b = bob.take_load_jobs();
        assert_eq!(b[0].share.as_deref(), Some("abc12345"));
        assert_eq!(b[0].song, "shared:abc12345:0");
        assert_eq!(b[0].digest, Some(ddi_chart::ChartDigest(7)));
        assert!(!b[0].cpus);
        // The prompt opens for the one who picked at once, for Bob once he
        // has the song, never for Carol who never answers.
        let oa = alice.take_stage_order().expect("the picker's prompt");
        assert_eq!(oa.battle, ddi_room::BattlePreset::GaugeSurvivalV0);
        assert_eq!(oa.attacks_rule(), None, "nothing to say of attacks");
        assert!(bob.take_stage_order().is_none(), "not read yet");
        alice.finish_load(&a[0], Ok(()));
        bob.finish_shared_load(&b[0], Ok(shared_pick(&s)));
        let _ = carol.take_load_jobs();
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
        let ob = bob.take_stage_order().unwrap();
        assert_eq!(ob.battle, ddi_room::BattlePreset::GaugeSurvivalV0);
        assert_eq!(ob.song.id, "shared:abc12345:0");
        assert_eq!(ob.others, 2);
        assert!(carol.take_stage_order().is_none());
        assert_eq!(carol.view(), crate::state::room::RoomView::Select);
        // Both ready; Carol (connected, loading) holds the start through
        // the loading grace (`hosted-deadline-v2`), then it starts without
        // her, everyone who loaded being ready.
        alice.set_ready(true).unwrap();
        bob.set_ready(true).unwrap();
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
        assert_eq!(bob.state().phase.kind(), PhaseKind::Preparing);
        tick(
            &clock,
            &mut [&mut alice, &mut bob, &mut carol],
            ddi_room::ReadinessPolicy::LOADING_GRACE_V2_MS,
        );
        assert_eq!(bob.state().phase.kind(), PhaseKind::Countdown);
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 4_000);
        assert!(alice.stage_started(stage) && bob.stage_started(stage));
        assert!(!carol.stage_started(stage));
        let play = alice.state().current.as_ref().unwrap().stages[0]
            .play
            .clone()
            .unwrap();
        assert_eq!(
            play.sat_out.get(&carol.me()),
            Some(&ddi_room::SitOutReason::StillLoading),
            "room model revision 12: the loading is asked first"
        );
        // Before anyone reported: the stage's standings wait, as seated.
        let table = bob.stage_table(stage).unwrap();
        assert!(!table.confirmed && table.result.is_none());
        assert_eq!(
            table
                .rows
                .iter()
                .map(|r| (r.participant, r.run, r.placement))
                .collect::<Vec<_>>(),
            vec![(alice.me(), None, None), (bob.me(), None, None)],
            "the two who started, Carol sat out"
        );
        // Bob's report says nothing more than his run (an older build's);
        // Alice's says how she performed.
        bob.report_run(stage, Run::Out { at: 12.5 }, None).unwrap();
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
        assert_eq!(alice.state().phase.kind(), PhaseKind::Stage);
        let table = alice.stage_table(stage).unwrap();
        assert!(!table.confirmed);
        assert_eq!(table.rows[1].run, Some(Run::Out { at: 12.5 }));
        assert_eq!((table.rows[0].run, table.rows[1].placement), (None, None));
        let detail = ddi_room::RunDetail {
            percent: Some(8_000),
            grade: "A".into(),
            life_ppm: 400_000,
            max_combo: 90,
            tiers: vec![50, 30, 10, 0, 0, 0, 2],
            ..Default::default()
        };
        alice
            .report_run(
                stage,
                Run::Live {
                    life: 0.4,
                    score: 0,
                    steps: 0,
                },
                Some(detail.clone()),
            )
            .unwrap();
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
        // Confirmed: in the stage's order, with the awards, the match's
        // totals and places, and what each said of their play; the same
        // for everyone, Carol (who sat out) included.
        for who in [&alice, &bob, &carol] {
            let table = who.stage_table(stage).unwrap();
            assert!(
                table.confirmed && table.result.is_none(),
                "not the last stage"
            );
            assert_eq!((table.number, table.of), (1, 4));
            let rows = &table.rows;
            assert_eq!(rows[0].participant, alice.me());
            assert_eq!(rows[0].detail.as_ref(), Some(&detail));
            assert_eq!(rows[0].placement.unwrap().place, 1);
            assert_eq!((rows[0].rank, rows[0].rank_before), (Some(1), None));
            assert_eq!(rows[0].total, Some(rows[0].placement.unwrap().points));
            assert_eq!(rows[1].participant, bob.me());
            assert_eq!(rows[1].detail, None);
            assert_eq!(rows[1].run, Some(Run::Out { at: 12.5 }));
            assert_eq!(rows[1].rank, Some(2));
        }
        assert_eq!(bob.state().phase.kind(), PhaseKind::Intermission);
        assert_eq!(bob.mark_of(stage).unwrap().placement.place, 2);
        assert_eq!(alice.mark_of(stage).unwrap().placement.place, 1);
        assert_eq!(alice.state(), server.borrow().state());
        assert_eq!(bob.state(), server.borrow().state());
    }

    /// An expired grant is renewed and the upload made again; a failed
    /// upload is said and retried when asked; the room's refusal of what
    /// was shared (no chosen chart in it) offers a retry too; a member
    /// whose copy has another chart reports it failed.
    #[test]
    fn sharing_renews_retries_and_a_wrong_chart_is_refused() {
        let clock = ManualClock::new(1_000);
        let server = room_of(&clock, &["bob"]);
        let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
        let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        let s = song();
        let stage = alice.stages()[0].id;
        alice.pick(&s, &s.charts[0]).unwrap();
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        let jobs = alice.take_upload_jobs();
        assert_eq!(jobs[0].grant, "grant-alice-1");
        // The grant expired on the way: a new one, the upload again.
        alice.upload_finished(
            stage,
            Err(ddi_library::share::grant_refusal(
                401,
                Some("grant_expired"),
            )),
        );
        assert_eq!(alice.sharing(stage), Some(&Sharing::Asking));
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        let jobs = alice.take_upload_jobs();
        assert_eq!(jobs[0].grant, "grant-alice-2");
        // Another failure: said, and retried only when asked.
        alice.upload_finished(stage, Err(ddi_library::share::ShareError::TooLarge));
        assert!(matches!(
            alice.sharing(stage),
            Some(Sharing::Failed(SharingProblem::Upload(_)))
        ));
        assert!(alice.may_retry_sharing(stage));
        assert!(!bob.may_retry_sharing(stage));
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        assert!(alice.take_upload_jobs().is_empty());
        alice.retry_sharing(stage);
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        assert_eq!(alice.take_upload_jobs()[0].grant, "grant-alice-3");
        // The server found the share without the chosen chart: blocked, a
        // retry offered, nothing uploaded again by itself.
        server
            .borrow_mut()
            .authority(Command::SetContentStatus {
                stage,
                status: ContentStatus::Unavailable {
                    code: "digest_mismatch".into(),
                },
                share: None,
            })
            .unwrap();
        alice.upload_finished(stage, Ok("abc12345".into()));
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        assert!(alice.may_retry_sharing(stage));
        assert!(alice.take_upload_jobs().is_empty(), "not again by itself");
        // Shared at last; Bob's copy has another chart: failed, said so.
        server
            .borrow_mut()
            .authority(Command::SetContentStatus {
                stage,
                status: ContentStatus::Available,
                share: Some("def67890".into()),
            })
            .unwrap();
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        let b = bob.take_load_jobs();
        bob.finish_shared_load(&b[0], Err(LoadProblem::Changed));
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        assert_eq!(bob.load_problem(stage), Some(LoadProblem::Changed));
        // No song, no prompt: Bob stays on song select and sits it out.
        assert_eq!(bob.view(), crate::state::room::RoomView::Select);
    }

    /// Alone, a pick is refused whole (nothing locked); with a guest, a
    /// song still being shared holds the readiness window past its
    /// deadline, which counts from when the song is available.
    #[test]
    fn a_lone_pick_is_refused_and_a_pending_song_holds_the_window() {
        let clock = ManualClock::new(1_000);
        let server = room_of(&clock, &[]);
        let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
        tick(&clock, &mut [&mut alice], 400);
        let s = song();
        let _ = alice.pick(&s, &s.charts[0]);
        tick(&clock, &mut [&mut alice], 400);
        assert!(matches!(
            alice.refusal(),
            Some(Refusal::Room(ddi_room::Rejection::NotEnoughReady { .. }))
        ));
        assert_eq!(alice.state().phase, Phase::Lobby, "no match locked");
        let server = room_of(&clock, &["bob"]);
        let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
        tick(&clock, &mut [&mut alice], 400);
        alice.pick(&s, &s.charts[0]).unwrap();
        tick(&clock, &mut [&mut alice], 400);
        let stage = alice.stages()[0].id;
        assert!(alice.state().content_pending(stage));
        tick(&clock, &mut [&mut alice], 90_000);
        assert!(matches!(
            alice.state().phase,
            Phase::Preparing {
                window: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn a_session_ended_in_the_room_asks_to_sign_in_again_and_comes_back() {
        let clock = ManualClock::new(1_000);
        let server = FakeServer::hosted(
            "Room000001",
            Identity::new("dev:alice"),
            "alice",
            RoomPolicy::default(),
            clock.clone(),
        )
        .shared();
        let online = person(&server, "alice");
        let mut alice = client(&server, online.clone(), "alice", 1_000);
        tick(&clock, &mut [&mut alice], 400);
        // The session gone (feedback round AA: a 4401 alone asks for a
        // ticket first, and only a refused one ends the link).
        server
            .borrow_mut()
            .sign_out(&ddi_room::Identity::new("dev:alice"));
        server.borrow_mut().fault(
            ddi_room::ParticipantId(1),
            Fault::Close(ddi_room::online::close::SIGN_IN_AGAIN),
        );
        tick(&clock, &mut [&mut alice], 400);
        assert_eq!(alice.link_state(), LinkState::SignInAgain);
        // Nothing opens by itself; only Sign in again does.
        assert_eq!(online.borrow_mut().take_redirect(), None);
        tick(&clock, &mut [&mut alice], 60_000);
        assert_eq!(alice.link_state(), LinkState::SignInAgain);
        alice.sign_in_again();
        assert_eq!(
            online.borrow_mut().take_redirect(),
            Some(Redirect::Page(
                "/auth/twitch/login?return=%2Fmp%2Froom%2FRoom000001".into()
            ))
        );
        // Back, signed in: the same row, connected again.
        server
            .borrow_mut()
            .sign_in(&ddi_room::Identity::new("dev:alice"));
        online
            .borrow_mut()
            .returned(Dest::Room("Room000001".into()));
        tick(&clock, &mut [&mut alice], 400);
        tick(&clock, &mut [&mut alice], 400);
        assert_eq!(alice.link_state(), LinkState::Connected);
        assert_eq!(alice.me(), ddi_room::ParticipantId(1));
        assert_eq!(server.borrow().state().members.len(), 1);
        // A dropped socket: Reconnecting, then back on a fresh ticket.
        server.borrow_mut().drop_socket(ddi_room::ParticipantId(1));
        tick(&clock, &mut [&mut alice], 100);
        assert_eq!(alice.link_state(), LinkState::Reconnecting { attempt: 1 });
        tick(&clock, &mut [&mut alice], 800);
        assert_eq!(alice.link_state(), LinkState::Connected);
    }

    /// Feedback round AA: a first-time player opens a room's page address
    /// (shared in place of its invite link) signed out. They are asked to
    /// sign in to enter it, never told their sign-in "ended"; signed in,
    /// they are told they are not in the room, never that they "left";
    /// either way the room's More offers the multiplayer home.
    #[test]
    fn a_rooms_address_opened_by_someone_never_in_it_says_so() {
        use crate::screens::room::{more_items, status};
        let status_text = |r: &crate::state::room::RoomClient| String::from(status(r));
        let clock = ManualClock::new(1_000);
        let server = FakeServer::hosted(
            "Room000001",
            Identity::new("dev:alice"),
            "alice",
            RoomPolicy::default(),
            clock.clone(),
        )
        .shared();
        server
            .borrow_mut()
            .sign_out(&ddi_room::Identity::new("dev:eve"));
        let online = person(&server, "eve");
        let mut eve = client(&server, online.clone(), "eve", 1_000);
        tick(&clock, &mut [&mut eve], 400);
        assert_eq!(eve.link_state(), LinkState::SignInToEnter);
        assert_eq!(status_text(&eve), "Sign in to enter this room.");
        // Its notice, once, from the second ask (the screen says the
        // line first).
        assert!(!eve.take_opened_elsewhere());
        assert!(eve.take_opened_elsewhere());
        assert!(!eve.take_opened_elsewhere());
        let items = more_items(&eve, false);
        let labels: Vec<&str> = items.iter().map(|(_, l)| l.as_str()).collect();
        assert!(labels.contains(&"Sign in with Twitch"), "{labels:?}");
        assert!(labels.contains(&"Multiplayer home"), "{labels:?}");
        assert!(!labels.contains(&"Sign in again"), "{labels:?}");
        // Sign in: back to the room's address, then not a member.
        eve.sign_in_again();
        assert_eq!(
            online.borrow_mut().take_redirect(),
            Some(Redirect::Page(
                "/auth/twitch/login?return=%2Fmp%2Froom%2FRoom000001".into()
            ))
        );
        server
            .borrow_mut()
            .sign_in(&ddi_room::Identity::new("dev:eve"));
        online
            .borrow_mut()
            .returned(Dest::Room("Room000001".into()));
        tick(&clock, &mut [&mut eve], 400);
        tick(&clock, &mut [&mut eve], 400);
        assert_eq!(eve.link_state(), LinkState::NotAMember);
        assert_eq!(
            status_text(&eve),
            "You're not in this room. Ask its host for the invite link."
        );
        assert!(!eve.take_opened_elsewhere());
        assert!(eve.take_opened_elsewhere(), "its own notice");
        let items = more_items(&eve, false);
        assert!(items.iter().any(|(id, l)| id == "leave" && l == "Multiplayer home"));
        assert_eq!(server.borrow().state().members.len(), 1, "nobody let in");
        // Then the invite link, joined: the shell's kept client of that
        // room, which never reached it, is made anew, and the room is in.
        let kept = crate::state::online::Kept::new();
        let eve = Rc::new(RefCell::new(eve));
        kept.room_or("Room000001", || eve.clone());
        server
            .borrow_mut()
            .join(ddi_room::Identity::new("dev:eve"), "eve", Intent::Play)
            .unwrap();
        let again = kept.room_or("Room000001", || {
            Rc::new(RefCell::new(client(&server, online.clone(), "eve", 3_000)))
        });
        assert!(!Rc::ptr_eq(&again, &eve), "a fresh link");
        tick(&clock, &mut [&mut again.borrow_mut()], 400);
        assert_eq!(again.borrow().link_state(), LinkState::Connected);
    }

    /// Feedback round AA: the room closed the socket 4401 at the expiry
    /// its ticket carried while the person's session was good (re-signed
    /// since): a ticket asked again at once is granted and the room comes
    /// back, never "Your sign-in ended" nor a sign-in page.
    #[test]
    fn a_4401_with_the_session_still_good_reconnects() {
        let clock = ManualClock::new(1_000);
        let server = FakeServer::hosted(
            "Room000001",
            Identity::new("dev:alice"),
            "alice",
            RoomPolicy::default(),
            clock.clone(),
        )
        .shared();
        let online = person(&server, "alice");
        let mut alice = client(&server, online.clone(), "alice", 1_000);
        tick(&clock, &mut [&mut alice], 400);
        assert_eq!(alice.link_state(), LinkState::Connected);
        server.borrow_mut().fault(
            ddi_room::ParticipantId(1),
            Fault::Close(ddi_room::online::close::SIGN_IN_AGAIN),
        );
        let mut seen = Vec::new();
        for _ in 0..8 {
            tick(&clock, &mut [&mut alice], 100);
            seen.push(alice.link_state());
        }
        assert!(!seen.contains(&LinkState::SignInAgain), "{seen:?}");
        assert_eq!(alice.link_state(), LinkState::Connected);
        assert_eq!(online.borrow_mut().take_redirect(), None);
        assert_eq!(server.borrow().state().members.len(), 1);
    }

    /// The review's gap 4 (round M), changed by round P1b (the
    /// maintainer's of 2026-11-16): the game's client under the server's
    /// bounds on one member (`ddi_room::fake::Bounds::SERVER`: four
    /// sockets, the oldest closed 4429 for a newer one; thirty tickets a
    /// minute). One person in the room from five devices: the fifth socket
    /// closes the first, which now STOPS (`LinkState::OpenedElsewhere`, an
    /// end) where it used to connect again, so that five took turns for
    /// ever (refused 429 once the tickets' bound was reached). The member
    /// stays Connected throughout, the four stay connected and nobody asks
    /// for a ticket any more. "Use this device" on the stopped one joins
    /// again and stops another device the same way.
    #[test]
    fn a_fifth_device_of_one_member_stops_the_first_which_comes_back_only_when_asked() {
        use ddi_room::fake::{Bounds, CLOSE_TOO_MANY_SOCKETS};
        assert_eq!(CLOSE_TOO_MANY_SOCKETS, ddi_room::online::close::OPENED_ELSEWHERE);
        let clock = ManualClock::new(1_000);
        let server = room_of(&clock, &["bob"]);
        server.borrow_mut().set_bounds(Some(Bounds::SERVER));
        let asked = Rc::new(std::cell::Cell::new((0u32, 0u32)));
        let device = |k: usize| {
            let inner = handler_of(&server, "alice");
            let counted = asked.clone();
            let mut inner = inner;
            let handler: Handler = Box::new(move |req| {
                let ticket = req.path.ends_with("/ticket");
                let answer = inner(req);
                if ticket {
                    let (all, refused) = counted.get();
                    let no = answer.as_ref().is_some_and(|a| a.status == 429);
                    counted.set((all + 1, refused + u32::from(no)));
                }
                answer
            });
            client(&server, signed_in(handler), &format!("alice{k}"), 1_000)
        };
        let me = ddi_room::ParticipantId(1);
        let connected = |server: &SharedFake| {
            server.borrow().state().member(me).unwrap().connection == ddi_room::Connection::Connected
        };
        let step = |devices: &mut [RoomClient], ms: u64| {
            for _ in 0..ms / 50 {
                clock.advance(50);
                let now = ddi_room::Clock::now(&clock).0;
                for d in devices.iter_mut() {
                    d.tick(now);
                }
            }
        };
        // Four devices, then a fifth.
        let mut devices: Vec<RoomClient> = (0..4).map(device).collect();
        step(&mut devices, 1_000);
        assert!(devices.iter().all(|d| d.link_state() == LinkState::Connected));
        devices.push(device(4));
        step(&mut devices, 2_000);
        let links: Vec<LinkState> = devices.iter().map(|d| d.link_state()).collect();
        assert_eq!(
            links,
            [
                LinkState::OpenedElsewhere,
                LinkState::Connected,
                LinkState::Connected,
                LinkState::Connected,
                LinkState::Connected,
            ],
            "the first device stops"
        );
        assert!(devices[0].take_opened_elsewhere(), "told once");
        assert!(!devices[0].take_opened_elsewhere());
        // Two minutes: nobody takes turns, nothing more is asked.
        let before = asked.get();
        for _ in 0..(120_000 / 1_000) {
            step(&mut devices, 1_000);
            assert!(connected(&server), "the member stays Connected");
            assert!(server.borrow().sockets() <= Bounds::SERVER.sockets);
        }
        assert_eq!(asked.get(), before, "no ticket asked in two minutes");
        assert_eq!(before.1, 0, "never refused");
        assert_eq!(devices[0].link_state(), LinkState::OpenedElsewhere);
        // Use this device: the first joins again, the second (connected
        // longest now) stops the same way.
        devices[0].use_this_device();
        step(&mut devices, 2_000);
        let links: Vec<LinkState> = devices.iter().map(|d| d.link_state()).collect();
        assert_eq!(
            links,
            [
                LinkState::Connected,
                LinkState::OpenedElsewhere,
                LinkState::Connected,
                LinkState::Connected,
                LinkState::Connected,
            ]
        );
        assert!(connected(&server));
        // Round AC: back, it says so (no refusal of what the stopped
        // page had asked).
        assert!(devices[0].refusal().is_none());
        assert_eq!(
            String::from(crate::screens::room::status_line(&devices[0])),
            "You're back in the room."
        );
        assert!(devices[1].take_opened_elsewhere());
        assert!(devices[1].leave_cost().is_none(), "an ended link costs nothing to leave");
    }

    /// A co-host of the fake room.
    fn co_host(server: &SharedFake, who: u64) {
        server
            .borrow_mut()
            .authority(Command::SetCoHost {
                participant: ddi_room::ParticipantId(who),
                on: true,
            })
            .unwrap();
    }

    /// One picker at a time (2026-11-12): the owner chooses at first and
    /// the co-host follows, whatever their own song select rests on; the
    /// co-host takes the pick and their resting song is the room's, shared
    /// from their device; the owner takes it back and the song they
    /// rested on before is shared already; a member can do none of it.
    #[test]
    fn the_pick_is_taken_over_and_back_and_the_resting_song_is_the_pickers() {
        let clock = ManualClock::new(1_000);
        let server = room_of(&clock, &["bob", "carol"]);
        co_host(&server, 2);
        let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
        let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
        let mut carol = client(&server, person(&server, "carol"), "carol", 1_000);
        macro_rules! step {
            ($ms:expr) => {
                tick(&clock, &mut [&mut alice, &mut bob, &mut carol], $ms)
            };
        }
        step!(400);
        let (a, b) = (alice.me(), bob.me());
        assert!(alice.chooses() && !bob.chooses() && !carol.chooses());
        assert!(bob.may_take_pick() && !alice.may_take_pick() && !carol.may_take_pick());
        assert_eq!(carol.picker(), Some(a));
        // Both operators' song selects rest on a song: the picker's is the
        // room's, the co-host's is nobody's business.
        let s = song();
        let mut other = song();
        other.id = "bundled:b".into();
        other.title = "B".into();
        other.charts[0].digest = Some(ddi_chart::ChartDigest(8));
        alice.hover(Some((&s, &s.charts[0])));
        bob.hover(Some((&other, &other.charts[0])));
        step!(3_400);
        let spot = carol.state().spotlight.clone().unwrap();
        assert_eq!((spot.by, spot.card.title.as_str()), (a, "A"));
        step!(400);
        // Revision 17: nothing is shared ahead (the room's list is).
        assert!(alice.take_upload_jobs().is_empty());
        step!(400);
        assert!(bob.take_upload_jobs().is_empty());
        // A member's take is not even sent; the room would refuse it.
        carol.take_pick().unwrap();
        step!(400);
        assert_eq!(carol.picker(), Some(a));
        // Taken at once: the co-host chooses, the owner follows, the
        // owner's resting song gone with their pick.
        bob.take_pick().unwrap();
        step!(400);
        assert!(bob.chooses() && !alice.chooses());
        assert!(alice.may_take_pick() && !bob.may_take_pick());
        assert_eq!(carol.picker(), Some(b));
        assert!(
            carol.state().spotlight.as_ref().is_none_or(|s| s.by == b),
            "never the last picker's"
        );
        // The co-host's song select tells the room what it rests on (it
        // rested there all along; the screen says so again as the pick
        // comes), shared from their device.
        bob.hover(Some((&other, &other.charts[0])));
        step!(400);
        let spot = alice.state().spotlight.clone().unwrap();
        assert_eq!((spot.by, spot.card.title.as_str()), (b, "B"));
        step!(400);
        assert!(bob.take_upload_jobs().is_empty(), "nothing shared ahead");
        // The owner's pick is refused while they follow (the room says
        // who chooses); nothing of theirs is uploaded.
        assert_eq!(
            alice.pick(&s, &s.charts[0]),
            Err(Refusal::Room(ddi_room::Rejection::NotPicker {
                picker: Some(b)
            }))
        );
        step!(400);
        assert_eq!(alice.state().phase, Phase::Lobby);
        alice.clear_refusal();
        // Taken back: the song the owner rested on before is told again;
        // nothing uploaded.
        alice.take_pick().unwrap();
        step!(400);
        assert!(alice.chooses() && bob.may_take_pick());
        alice.hover(Some((&s, &s.charts[0])));
        step!(400);
        assert!(bob.state().spotlight.is_none(), "not rested yet");
        step!(3_000);
        let spot = bob.state().spotlight.clone().unwrap();
        assert_eq!((spot.by, spot.share.as_deref()), (a, None), "{spot:?}");
        step!(400);
        assert!(alice.take_upload_jobs().is_empty());
        // The owner's pick starts the match, as before.
        alice.pick(&s, &s.charts[0]).unwrap();
        step!(400);
        assert!(matches!(alice.state().phase, Phase::Preparing { .. }));
    }

    /// Round RL3 (revision 21): Who can add songs. Under Hosts Bob's My
    /// songs is browsing only and an addition is refused in the room's
    /// words; Alice lets everyone add: Bob adds his own songs (uploaded
    /// under each entry's grant, as a host's), the status line counts them
    /// against his half and says when it is reached (his next refused at
    /// the press, before anything is sent); he removes his own; Alice turns
    /// it back to Hosts: his songs stay, his next is refused in those
    /// words; a full list that refuses is said before the press.
    #[test]
    fn a_member_adds_their_own_songs_when_everyone_may() {
        use crate::screens::room::{refusal_text, status_on_my_songs};
        use ddi_room::{ListRules, Rejection, WhenFull, WhoAdds};
        let clock = ManualClock::new(1_000);
        let server = room_of(&clock, &["bob"]);
        let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
        let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
        macro_rules! step {
            ($ms:expr) => {
                tick(&clock, &mut [&mut alice, &mut bob], $ms)
            };
        }
        step!(400);
        let songs: Vec<crate::screens::data::SongEntry> = (0..4u128)
            .map(|k| {
                let mut s = song();
                s.id = format!("u-{k}");
                s.title = format!("Song {k}");
                s.charts[0].digest = Some(ddi_chart::ChartDigest(100 + k));
                s
            })
            .collect();
        bob.set_library(&songs);
        alice.set_library(&songs);
        let text = |t: ddi_l10n::UiText| String::from(t);
        // Hosts (the default): Bob may not add; My songs says nothing.
        assert_eq!(bob.who_adds(), WhoAdds::Hosts);
        assert!(!bob.may_add());
        assert!(status_on_my_songs(&bob).is_none());
        let refused = bob.add_song(&songs[0]).unwrap_err();
        assert_eq!(text(refusal_text(&refused)), "Only the hosts can add songs to this room now.");
        bob.clear_refusal();
        // Alice: a list of four, refusing when full, everyone adding.
        alice
            .set_song_list(ListRules {
                size: 4,
                when_full: WhenFull::Refuse,
            })
            .unwrap();
        alice.set_who_adds(WhoAdds::Everyone).unwrap();
        step!(400);
        assert!(bob.may_add());
        assert_eq!(bob.my_song_count(), Some((0, 2)));
        assert_eq!(alice.my_song_count(), None, "a host fills the whole list");
        assert_eq!(
            status_on_my_songs(&bob).map(text).as_deref(),
            Some("Add songs to the room here. You've added 0 of the 2 songs you can add.")
        );
        bob.add_song(&songs[0]).unwrap();
        bob.add_song(&songs[1]).unwrap();
        step!(400);
        let mine: Vec<_> = alice.room_songs().iter().filter(|e| e.by == bob.me()).map(|e| e.id).collect();
        assert_eq!(mine.len(), 2);
        // His uploads, one at a time, each under its entry's grant.
        step!(400);
        let jobs = bob.take_upload_jobs();
        assert_eq!(jobs.len(), 1);
        assert_eq!((jobs[0].entry, jobs[0].song.id.as_str()), (Some(mine[0]), "u-0"));
        // At his half: said before the press, refused at it, nothing sent.
        assert_eq!(bob.add_blocked(), Some(Rejection::TooManySongs { limit: 2 }));
        assert_eq!(
            status_on_my_songs(&bob).map(text).as_deref(),
            Some("You've added all 2 songs you can. Remove one to add another.")
        );
        let refused = bob.add_song(&songs[2]).unwrap_err();
        assert_eq!(
            text(refusal_text(&refused)),
            "You've added the 2 songs you can add to this room. Remove one of yours to add another."
        );
        bob.clear_refusal();
        // His own he removes; not Alice's.
        alice.add_song(&songs[3]).unwrap();
        step!(400);
        let hers = alice.listed(&songs[3]).unwrap().id;
        assert!(bob.may_remove(mine[1]) && !bob.may_remove(hers));
        bob.remove_song(mine[1]).unwrap();
        step!(400);
        assert_eq!(alice.room_songs().len(), 2);
        // Back to Hosts: his song stays, his next is refused.
        alice.set_who_adds(WhoAdds::Hosts).unwrap();
        step!(400);
        assert!(!bob.may_add());
        assert!(alice.room_songs().iter().any(|e| e.id == mine[0]));
        let refused = bob.add_song(&songs[2]).unwrap_err();
        assert!(matches!(refused, Refusal::Room(Rejection::Unauthorized { .. })));
        bob.clear_refusal();
        // Full and refusing (Alice fills it): said on My songs before the
        // press, to Bob once he may add again and to Alice.
        alice.add_song(&songs[1]).unwrap();
        alice.add_song(&songs[2]).unwrap();
        alice.set_who_adds(WhoAdds::Everyone).unwrap();
        step!(400);
        assert_eq!(alice.room_songs().len(), 4);
        let full = "The room's song list is full. A song must be removed first.";
        for r in [&alice, &bob] {
            assert_eq!(r.add_blocked(), Some(Rejection::ListFull));
            assert_eq!(status_on_my_songs(r).map(text).as_deref(), Some(full));
        }
    }

}

thread_local! {
    /// A test's epoch clock (milliseconds), for `ddi_platform::storage`.
    static EPOCH_NOW: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

fn epoch_now() -> f64 {
    EPOCH_NOW.with(std::cell::Cell::get)
}

/// Round TB (the maintainer, 2026-11-22: "we should avoid downgrading the
/// permissions, at least within a time-frame, so that turning off and on
/// again is not annoying, maybe if a permission isn't used for 30 or 60
/// days, it isn't requested on the next sign-in"): the device remembers
/// which Twitch permissions it used and when, and a sign-in on either
/// shell asks Twitch for those used within `TWITCH_KEEP_DAYS` in its own
/// step (the web's `keep=`, the desktop attempt's `keep`); one unused
/// longer is left out; Options → Data's Disconnect forgets them all.
#[test]
fn a_sign_in_asks_again_for_the_twitch_permissions_used_lately() {
    use crate::state::twitch::{TWITCH_KEEP_DAYS, kept_features, mark_used};
    ddi_platform::storage::install(Rc::new(ddi_platform::storage::MemoryStore::default()));
    ddi_platform::storage::install_clock(epoch_now);
    const DAY: f64 = 86_400_000.0;
    let t0 = 1_800_000_000_000.0;
    EPOCH_NOW.with(|n| n.set(t0));
    let web_path = || {
        let r = rig(Platform::Web, Box::new(|_| None));
        let mut o = r.online.borrow_mut();
        o.sign_in(Dest::Home);
        match o.take_redirect() {
            Some(Redirect::Page(p)) => p,
            other => panic!("{other:?}"),
        }
    };
    let desktop_body = || {
        let r = rig(Platform::Desktop, Box::new(|_| None));
        r.online.borrow_mut().sign_in(Dest::Home);
        let log = r.log.borrow();
        log.last().and_then(|q| q.body.clone()).unwrap_or_default()
    };
    // Nothing used: a plain sign-in, as before.
    assert_eq!(web_path(), "/auth/twitch/login?return=%2Fmp");
    assert_eq!(desktop_body(), r#"{"return":"/mp","shows_code":true}"#);
    // A host's bans and a room's chat used.
    mark_used(&["bans", "chat"]);
    assert_eq!(kept_features(), ["bans", "chat"]);
    assert_eq!(web_path(), "/auth/twitch/login?return=%2Fmp&keep=bans%2Cchat");
    let body: serde_json::Value = serde_json::from_str(&desktop_body()).unwrap();
    assert_eq!(body["keep"], "bans,chat");
    // The chat used again later; the bans not since: past the window
    // they are not asked for.
    EPOCH_NOW.with(|n| n.set(t0 + 20.0 * DAY));
    mark_used(&["chat"]);
    EPOCH_NOW.with(|n| n.set(t0 + TWITCH_KEEP_DAYS as f64 * DAY - 1.0));
    assert_eq!(kept_features(), ["bans", "chat"]);
    EPOCH_NOW.with(|n| n.set(t0 + TWITCH_KEEP_DAYS as f64 * DAY));
    assert_eq!(kept_features(), ["chat"]);
    assert_eq!(web_path(), "/auth/twitch/login?return=%2Fmp&keep=chat");
    // Signing out keeps them (the server deletes the link, the device
    // asks again at its next sign-in).
    {
        let r = rig(Platform::Web, Box::new(|_| None));
        r.online.borrow_mut().sign_out();
    }
    assert_eq!(kept_features(), ["chat"]);
    // Disconnect forgets them: nothing asked at the next sign-in.
    {
        let r = rig(Platform::Web, Box::new(|_| None));
        r.online.borrow_mut().disconnect_twitch();
    }
    assert!(kept_features().is_empty());
    assert_eq!(web_path(), "/auth/twitch/login?return=%2Fmp");
    // Written at most once an hour a feature (not once a frame).
    let written = || crate::state::settings::Settings::load().twitch_used.get("blocks").copied();
    mark_used(&["blocks"]);
    let first = written();
    assert!(first.is_some());
    EPOCH_NOW.with(|n| n.set(t0 + TWITCH_KEEP_DAYS as f64 * DAY + 60_000.0));
    mark_used(&["blocks"]);
    assert_eq!(written(), first, "a minute later: nothing written");
    // A stored entry that is not a feature is dropped on load.
    let mut s = crate::state::settings::Settings::load();
    s.twitch_used.insert("moderation:read".into(), 1);
    s.save();
    assert!(!crate::state::settings::Settings::load().twitch_used.contains_key("moderation:read"));
}
