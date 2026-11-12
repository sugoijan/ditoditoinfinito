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
                assert_eq!(req.body.as_deref(), Some(r#"{"return":"/mp"}"#));
                Some(ApiResponse {
                    status: 201,
                    body: r#"{"attempt_id":"a1","poll_secret":"s1","open_url":"http://api/auth/attempts/a1/open","expires_at_ms":9}"#.into(),
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
    assert!(matches!(o.sign_in_state(), SignIn::Waiting { .. }));
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
                ok(
                    r#"{"room_id":"Room000001","participant":3,"intent":"play","participation":{"not_ready":{"stage":1}},"already":false}"#,
                )
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

    fn me_of(name: &str) -> String {
        format!(
            r#"{{"player_id":"dev:{name}","login":"{name}","display_name":"{name}","admin":false,"expires_at_ms":1}}"#
        )
    }

    /// `who`'s online state, its HTTP the fake room's: tickets, the
    /// summary, the invite and stages' upload grants (for operators, as
    /// the server decides them; numbered `grant-<who>-<n>`).
    pub(crate) fn person(server: &SharedFake, who: &'static str) -> SharedOnline {
        let mut grants = 0;
        let s = server.clone();
        let handler: Handler = Box::new(move |req| {
            let identity = Identity::new(format!("dev:{who}"));
            match (req.method, req.path.as_str()) {
                (Method::Get, "/me") => ok(&me_of(who)),
                (Method::Get, "/rooms/Room000001") => ok(
                    r#"{"room_id":"Room000001","name":"After Hours","revision":1,"phase":"lobby","admission":"open","participant":1}"#,
                ),
                (Method::Post, "/rooms/Room000001/ticket") => {
                    match s.borrow_mut().ticket(&identity) {
                        Ok(t) => ok(&format!(r#"{{"ticket":"{t}","expires_at_ms":9}}"#)),
                        Err(_) => err(403, "not_a_member"),
                    }
                }
                (Method::Get, "/rooms/Room000001/invite") if who == "alice" => ok(&format!(
                    r#"{{"invite":"{INVITE}","expires_at_ms":9,"admission":"open"}}"#
                )),
                (Method::Get, "/rooms/Room000001/invite") => err(403, "not_an_operator"),
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

    fn room_of(clock: &ManualClock, others: &[&str]) -> SharedFake {
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

    fn shared_pick(s: &crate::screens::data::SongEntry) -> Picked {
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
            Some("http://localhost:8080/#/mp/join/Room000001.abcdefghijABCDEFGHIJ01?mp=1")
        );
        assert_eq!(bob.invite_link(), None);
        // Moving through the list sends nothing; resting does.
        let s = song();
        let mut other = song();
        other.id = "bundled:b".into();
        other.charts[0].digest = Some(ddi_chart::ChartDigest(8));
        alice.hover(Some((&other, &other.charts[0])));
        tick(&clock, &mut [&mut alice, &mut bob], 1_000);
        alice.hover(Some((&s, &s.charts[0])));
        tick(&clock, &mut [&mut alice, &mut bob], 2_000);
        assert!(bob.state().spotlight.is_none(), "not rested yet");
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 1_200);
        let spot = bob.state().spotlight.clone().unwrap();
        assert_eq!(spot.card.title, "A");
        assert_eq!(bob.spotlight().unwrap().1, None, "its card alone yet");
        // Its upload, once, under a grant for no stage.
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
        let jobs = alice.take_upload_jobs();
        assert_eq!(jobs.len(), 1);
        assert_eq!(
            (jobs[0].stage, jobs[0].grant.as_str()),
            (crate::state::room::SPOTLIGHT, "grant-alice-1")
        );
        server
            .borrow_mut()
            .authority(Command::SetSpotlightShare {
                chart: jobs[0].content.chart.clone(),
                share: "abc12345".into(),
            })
            .unwrap();
        alice.upload_finished(jobs[0].stage, Ok("abc12345".into()));
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
        assert!(alice.take_upload_jobs().is_empty());
        // Bob reads it from the share: the song shown with its banner.
        let b = bob.take_load_jobs();
        assert_eq!(b.len(), 1);
        assert_eq!(
            (b[0].stage, b[0].share.as_deref()),
            (crate::state::room::SPOTLIGHT, Some("abc12345"))
        );
        bob.finish_shared_load(&b[0], Ok(shared_pick(&s)));
        assert_eq!(
            bob.spotlight().unwrap().1.unwrap().song.id,
            "shared:abc12345:0"
        );
        let _ = carol.take_load_jobs();
        // Bob may not pick: refused by the room.
        assert!(bob.pick(&other, &other.charts[0]).is_err());
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
        // The pick: the match starts, the stage available with the
        // spotlight's share at once; nothing to upload again.
        alice.pick(&s, &s.charts[0]).unwrap();
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
        // Both ready; Carol (connected, loading) holds the early start,
        // so the deadline starts it, Carol sitting out.
        alice.set_ready(true).unwrap();
        bob.set_ready(true).unwrap();
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 400);
        assert_eq!(bob.state().phase.kind(), PhaseKind::Preparing);
        tick(&clock, &mut [&mut alice, &mut bob, &mut carol], 30_000);
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
            Some(&ddi_room::SitOutReason::NotReady)
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
                    accuracy: 0.8,
                    absolute: None,
                    score: None,
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
        assert_eq!(upload(&server, &mut alice, "abc12345").len(), 1);
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
        let jobs = upload(&server, &mut bob, "def67890");
        assert_eq!(
            (jobs.len(), jobs[0].grant.as_str()),
            (1, "grant-bob-1"),
            "the picker's own upload"
        );
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
        // Taken back: the song the owner rested on before is told again
        // and is shared already (the room kept its share): no upload.
        alice.take_pick().unwrap();
        step!(400);
        assert!(alice.chooses() && bob.may_take_pick());
        alice.hover(Some((&s, &s.charts[0])));
        step!(400);
        assert!(bob.state().spotlight.is_none(), "not rested yet");
        step!(3_000);
        let spot = bob.state().spotlight.clone().unwrap();
        assert_eq!(
            (spot.by, spot.share.as_deref()),
            (a, Some("abc12345")),
            "{spot:?}"
        );
        step!(400);
        assert!(alice.take_upload_jobs().is_empty());
        // The owner's pick starts the match, as before.
        alice.pick(&s, &s.charts[0]).unwrap();
        step!(400);
        assert!(matches!(alice.state().phase, Phase::Preparing { .. }));
    }

    /// A room whose authority predates the picker (its state has no
    /// `picking`): this client lets every operator choose, as that room
    /// does, and never offers or sends the take it could not read.
    #[test]
    fn a_room_from_before_the_picker_has_every_operator_choosing_and_no_take() {
        let clock = ManualClock::new(1_000);
        let mut state = ddi_room::RoomState::new(
            ddi_room::RoomId::new("Room000001"),
            Identity::new("dev:alice"),
            "alice".into(),
            RoomPolicy::default(),
            ddi_room::MatchRules::hosted(),
            ddi_room::ContentSource::Shared,
        );
        state.picking = None;
        let server = FakeServer::with_state(state, clock.clone()).shared();
        server
            .borrow_mut()
            .join(Identity::new("dev:bob"), "bob", Intent::Play)
            .unwrap();
        co_host(&server, 2);
        let mut alice = client(&server, person(&server, "alice"), "alice", 1_000);
        let mut bob = client(&server, person(&server, "bob"), "bob", 1_000);
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        assert!(alice.chooses() && bob.chooses());
        assert!(!alice.may_take_pick() && !bob.may_take_pick());
        let revision = server.borrow().state().revision;
        bob.take_pick().unwrap();
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        assert_eq!(server.borrow().state().revision, revision, "nothing sent");
        assert!(!server.borrow().state().one_picker());
        // The co-host's resting song is the room's, as it was.
        let s = song();
        bob.hover(Some((&s, &s.charts[0])));
        tick(&clock, &mut [&mut alice, &mut bob], 3_400);
        tick(&clock, &mut [&mut alice, &mut bob], 400);
        assert_eq!(alice.state().spotlight.as_ref().unwrap().by, bob.me());
    }
}
