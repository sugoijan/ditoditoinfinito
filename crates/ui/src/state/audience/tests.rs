//! An audience seat's client over the fake room's audience
//! (`ddi_room::fake`, its relays and seats): arriving by the audience link
//! and at the member bound, each refusal worded, the view and the count,
//! the count told to the members, the owner's closing, the room's end, a
//! relay's restart survived without a new ticket, a lost socket's new
//! ticket.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::state::online::api::Method;
use crate::state::online::tests::room::{client, person};
use crate::state::online::tests::{Handler, err, me_of, ok, signed_in};
use crate::state::room::RoomClient;
use ddi_room::audience::AudienceRules;
use ddi_room::fake::{FakeAudienceSocket, FakeServer, SharedFake};
use ddi_room::{Closure, Identity, Intent, ReconnectPolicy, RoomId, RoomPolicy, Transport};

/// The room's audience link (its key is the fake's own).
pub(crate) const AUDIENCE_LINK: &str = "Room000001.audienceAUDIENCEaudien01";

/// `who`'s online state, its HTTP the fake room's as the server answers
/// it: an invite's preview (`FakeServer::invite_preview`: the link's
/// kind, the audience offered) and the audience join
/// (`FakeServer::audience_join_answer`).
pub(crate) fn watcher(server: &SharedFake, who: &'static str) -> SharedOnline {
    let s = server.clone();
    let handler: Handler = Box::new(move |req| match (req.method, req.path.as_str()) {
        (Method::Get, "/me") => ok(&me_of(who)),
        (Method::Get, p) if p.starts_with("/invites/") => {
            match s.borrow().invite_preview(&p["/invites/".len()..]) {
                Ok(body) => ok(&body),
                Err((status, code)) => err(status, code),
            }
        }
        (Method::Post, "/rooms/Room000001/audience") => {
            let body: serde_json::Value =
                serde_json::from_str(req.body.as_deref().unwrap_or("{}")).unwrap();
            let link = body["invite"].as_str().unwrap_or_default();
            let (status, body) = s
                .borrow_mut()
                .audience_join_answer(Some(&format!("dev:{who}")), link, None);
            Some(crate::state::online::api::ApiResponse {
                status,
                body,
                session: None,
            })
        }
        _ => panic!("unexpected {req:?}"),
    });
    signed_in(handler)
}

/// Someone not signed in (a fresh browser, desktop with no session
/// file), over the fake room as the server answers them: `/me` signed
/// out, an invite's preview, the audience join admitted anonymously
/// (`FakeServer::audience_join_answer` with no identity). Any sign-in
/// request fails the test.
pub(crate) fn stranger(server: &SharedFake) -> SharedOnline {
    let s = server.clone();
    let handler: Handler = Box::new(move |req| match (req.method, req.path.as_str()) {
        (Method::Get, "/me") => err(401, "signed_out"),
        (Method::Get, p) if p.starts_with("/invites/") => {
            match s.borrow().invite_preview(&p["/invites/".len()..]) {
                Ok(body) => ok(&body),
                Err((status, code)) => err(status, code),
            }
        }
        (Method::Post, "/rooms/Room000001/audience") => {
            let body: serde_json::Value =
                serde_json::from_str(req.body.as_deref().unwrap_or("{}")).unwrap();
            let link = body["invite"].as_str().unwrap_or_default();
            let (status, body) = s.borrow_mut().audience_join_answer(None, link, None);
            Some(crate::state::online::api::ApiResponse {
                status,
                body,
                session: None,
            })
        }
        _ => panic!("no sign-in on the audience's path: {req:?}"),
    });
    signed_in(handler)
}

pub(crate) fn room(clock: &ManualClock, policy: RoomPolicy) -> SharedFake {
    let server = FakeServer::hosted(
        "Room000001",
        Identity::new("dev:alice"),
        "alice",
        policy,
        clock.clone(),
    )
    .shared();
    server.borrow_mut().set_room_name("After Hours");
    server
        .borrow_mut()
        .join(Identity::new("dev:bob"), "bob", Intent::Play)
        .unwrap();
    server
}

fn live_audience() -> RoomPolicy {
    RoomPolicy::default().with_live().with_audience()
}

/// `who` arriving by the audience link as its page does: the preview,
/// Watch, the seat given.
pub(crate) fn arrive(
    server: &SharedFake,
    who: &'static str,
    link: &str,
    now: u64,
) -> Result<AudienceClient, Problem> {
    arrive_then(server, who, link, now, || {})
}

/// [`arrive`], `between` done after the page's preview and before Watch
/// (the room's links replaced, say).
pub(crate) fn arrive_then(
    server: &SharedFake,
    who: &'static str,
    link: &str,
    now: u64,
    between: impl FnOnce(),
) -> Result<AudienceClient, Problem> {
    let online = watcher(server, who);
    {
        let mut o = online.borrow_mut();
        o.open_invite(link);
        o.poll(now);
        assert!(o.invite().unwrap().audience(), "the page offers the audience");
        between();
        o.watch_audience();
        o.poll(now);
    }
    let seated = online.borrow_mut().take_seated();
    match seated {
        Some(s) => {
            assert_eq!((s.room.as_str(), s.link.as_str()), ("Room000001", link));
            Ok(AudienceClient::new(
                online,
                &s.link,
                Box::new(FakeAudienceSocket::new(server.clone())),
                Some(s.ticket),
                now,
            ))
        }
        None => Err(online.borrow().invite().unwrap().refused.clone().unwrap()),
    }
}

/// Time passes for the room, its members and the audience.
pub(crate) fn run(
    clock: &ManualClock,
    server: &SharedFake,
    members: &mut [&mut RoomClient],
    seats: &mut [&mut AudienceClient],
    ms: u64,
) {
    for _ in 0..ms / 20 {
        clock.advance(20);
        let now = clock.now().0;
        server.borrow_mut().pump();
        for r in members.iter_mut() {
            r.tick(now);
        }
        for a in seats.iter_mut() {
            a.tick(now);
        }
    }
}

#[test]
fn the_audience_link_is_watched_with_the_room_s_view_and_count_told_to_the_members() {
    let clock = ManualClock::new(5_000);
    let server = room(&clock, live_audience());
    let mut alice = client(&server, person(&server, "alice"), "alice", 5_000);
    let mut carol = arrive(&server, "carol", AUDIENCE_LINK, 5_000).unwrap();
    run(&clock, &server, &mut [&mut alice], &mut [&mut carol], 7_000);
    assert_eq!(carol.state(), SeatState::Watching);
    let v = carol.view().expect("the view");
    assert_eq!(v.name, "After Hours");
    assert_eq!(v.host.as_deref(), Some("alice"));
    assert!(v.players.iter().any(|p| p.name.as_deref() == Some("bob")));
    assert_eq!(carol.count(), Some(1));
    assert_eq!(carol.sockets(), 1);
    // The room's members see the count; only its owner may close it.
    assert!(alice.has_audience());
    assert_eq!(alice.audience(), Some((1, true)));
    assert!(alice.may_close_audience());
    // Said after the room's status line between stages, never on a
    // stage's prompt (`screens::room::status_line`).
    let line = String::from(crate::screens::room::status(&alice));
    assert!(line.ends_with(" · One watching"), "{line}");
    let alone = String::from(crate::screens::room::status_line(&alice));
    assert!(!alone.contains("watching"), "{alone}");
    assert!(alice.audience_invite_link().is_none(), "the invite not read yet");
    // The audience is nobody in the room's state: no member, no event.
    assert_eq!(server.borrow().state().members.len(), 2);
    // A second seat by the same person: two are allowed, a third is not.
    let mut carol2 = arrive(&server, "carol", AUDIENCE_LINK, clock.now().0).unwrap();
    run(&clock, &server, &mut [&mut alice], &mut [&mut carol, &mut carol2], 7_000);
    assert_eq!(alice.audience(), Some((2, true)));
    assert_eq!(carol2.count(), Some(2));
    let third = arrive(&server, "carol", AUDIENCE_LINK, clock.now().0).unwrap_err();
    assert_eq!(third.code, "too_many_seats");
    assert_eq!(
        String::from(crate::screens::online::invite_problem(&third)),
        "You are already watching this room on two devices."
    );
    // Leaving frees the seat.
    carol2.leave();
    run(&clock, &server, &mut [&mut alice], &mut [&mut carol], 3_000);
    assert_eq!(alice.audience(), Some((1, true)));
}

#[test]
fn every_refusal_is_said_and_the_room_without_an_audience_offers_none() {
    let clock = ManualClock::new(5_000);
    // A link not the room's (any more: replaced after its page showed).
    let server = room(&clock, live_audience());
    // (The fake's links are the ones these tests name.)
    assert_eq!(server.borrow().links().audience, AUDIENCE_LINK);
    let bad = arrive_then(&server, "dan", AUDIENCE_LINK, 5_000, || {
        server.borrow_mut().set_links(ddi_room::fake::FakeLinks {
            audience: "Room000001.replacedAUDIENCElink01".into(),
            invite: "Room000001.replacedINVITElinkAB01".into(),
            watch: "Room000001.replacedWATCHlinkABC01".into(),
        });
    })
    .unwrap_err();
    assert_eq!(bad.code, "audience_link_unknown");
    assert_eq!(
        String::from(crate::screens::online::invite_problem(&bad)),
        "This audience link isn't this room's, or it was replaced."
    );
    // Full, at a small bound (the links as they were).
    server
        .borrow_mut()
        .set_links(ddi_room::fake::FakeLinks::of("Room000001"));
    server.borrow_mut().set_audience_rules(AudienceRules {
        max_audience: 1,
        relay_seats: 1,
        home_relays: 1,
        ..AUDIENCE_V0
    });
    let mut first = arrive(&server, "erin", AUDIENCE_LINK, 5_000).unwrap();
    run(&clock, &server, &mut [], &mut [&mut first], 1_000);
    let full = arrive(&server, "fay", AUDIENCE_LINK, clock.now().0).unwrap_err();
    assert_eq!(
        String::from(crate::screens::online::invite_problem(&full)),
        "The room's audience is full."
    );
    // A server older than this client (round R2): refused here, no seat.
    let clock = ManualClock::new(5_000);
    let behind = room(&clock, live_audience());
    behind
        .borrow_mut()
        .set_model(ddi_room::ROOM_MODEL_REVISION - 1);
    let old = arrive(&behind, "hal", AUDIENCE_LINK, 5_000).unwrap_err();
    assert_eq!(old.code, ddi_room::SERVER_BEHIND);
    assert_eq!(
        String::from(crate::screens::online::invite_problem(&old)),
        String::from(ddi_l10n::msg::room::server_updating())
    );
    assert_eq!(behind.borrow().sockets(), 0);
    // A room on a server without an audience: no count, link or control
    // for its members, and its links offer nothing.
    let clock = ManualClock::new(5_000);
    let plain = room(&clock, RoomPolicy::default().with_live());
    let mut alice = client(&plain, person(&plain, "alice"), "alice", 5_000);
    run(&clock, &plain, &mut [&mut alice], &mut [], 2_000);
    assert!(!alice.has_audience());
    assert_eq!(alice.audience(), None);
    assert!(!alice.may_close_audience());
    let online = watcher(&plain, "gus");
    online.borrow_mut().open_invite(AUDIENCE_LINK);
    online.borrow_mut().poll(0);
    assert!(online.borrow().invite().is_none_or(|v| !v.audience()));
    // Its own invite's preview says the server's kind and no audience.
    let own = plain.borrow().links().invite.clone();
    online.borrow_mut().open_invite(&own);
    online.borrow_mut().poll(0);
    let o = online.borrow();
    let v = o.invite().unwrap();
    assert!(!v.audience());
    assert_eq!(
        v.preview.as_ref().unwrap().as_ref().unwrap().kind,
        crate::state::online::api::InviteKind::Play
    );
}

#[test]
fn the_owner_closes_the_audience_and_the_room_s_end_sends_it_away() {
    let clock = ManualClock::new(5_000);
    let server = room(&clock, live_audience());
    let mut alice = client(&server, person(&server, "alice"), "alice", 5_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 5_000);
    let mut carol = arrive(&server, "carol", AUDIENCE_LINK, 5_000).unwrap();
    run(&clock, &server, &mut [&mut alice, &mut bob], &mut [&mut carol], 3_000);
    assert_eq!(carol.state(), SeatState::Watching);
    // Bob is no owner: nothing is sent.
    assert!(!bob.may_close_audience());
    bob.set_audience_open(false);
    run(&clock, &server, &mut [&mut alice, &mut bob], &mut [&mut carol], 1_000);
    assert_eq!(carol.state(), SeatState::Watching);
    // The owner closes it: the seat is told why and asks for no ticket.
    alice.set_audience_open(false);
    run(&clock, &server, &mut [&mut alice, &mut bob], &mut [&mut carol], 3_000);
    assert_eq!(carol.state(), SeatState::Ended(AudienceEnd::Closed));
    assert_eq!(alice.audience(), Some((0, false)));
    assert_eq!(
        alice.link_notice(),
        Some(crate::state::online::link::LinkNotice::AudienceClosed)
    );
    let refused = arrive(&server, "dan", AUDIENCE_LINK, clock.now().0).unwrap_err();
    assert_eq!(refused.code, "audience_closed");
    // Opened again: people come back in; the room's end sends them away.
    alice.set_audience_open(true);
    run(&clock, &server, &mut [&mut alice, &mut bob], &mut [], 1_000);
    let mut dan = arrive(&server, "dan", AUDIENCE_LINK, clock.now().0).unwrap();
    run(&clock, &server, &mut [&mut alice, &mut bob], &mut [&mut dan], 2_000);
    assert_eq!(dan.state(), SeatState::Watching);
    server
        .borrow_mut()
        .rewrite(|s| s.closed = Some(Closure::HostLeft));
    run(&clock, &server, &mut [], &mut [&mut dan], 2_000);
    assert_eq!(dan.state(), SeatState::Ended(AudienceEnd::RoomClosed));
    let sockets = dan.sockets();
    run(&clock, &server, &mut [], &mut [&mut dan], 10_000);
    assert_eq!(dan.sockets(), sockets, "an ended seat asks for nothing more");
}

#[test]
fn a_relay_s_restart_is_survived_silently_and_a_lost_socket_gets_a_new_ticket() {
    let clock = ManualClock::new(5_000);
    let server = room(&clock, live_audience());
    let mut carol = arrive(&server, "carol", AUDIENCE_LINK, 5_000).unwrap();
    run(&clock, &server, &mut [], &mut [&mut carol], 2_000);
    assert_eq!(carol.sockets(), 1);
    server.borrow_mut().restart_relay(0);
    server.borrow_mut().restart_relay(1);
    server.borrow_mut().restart_relay(2);
    server.borrow_mut().restart_relay(3);
    run(&clock, &server, &mut [], &mut [&mut carol], 7_000);
    assert_eq!(carol.state(), SeatState::Watching);
    assert_eq!(carol.sockets(), 1, "no new socket for a relay's restart");
    assert_eq!(carol.count(), Some(1));
    // The socket lost (a forced update closes it): a new ticket from the
    // audience join, after a short wait.
    let cut = Rc::new(std::cell::Cell::new(false));
    let online = watcher(&server, "dan");
    let mut dan = AudienceClient::new(
        online,
        AUDIENCE_LINK,
        Box::new(Cuttable {
            inner: FakeAudienceSocket::new(server.clone()),
            cut: cut.clone(),
        }),
        None,
        clock.now().0,
    );
    run(&clock, &server, &mut [], &mut [&mut carol, &mut dan], 2_000);
    assert_eq!((dan.state(), dan.sockets()), (SeatState::Watching, 1));
    cut.set(true);
    run(&clock, &server, &mut [], &mut [&mut carol, &mut dan], 100);
    assert_eq!(dan.state(), SeatState::Reconnecting);
    run(&clock, &server, &mut [], &mut [&mut carol, &mut dan], 3_000);
    assert_eq!((dan.state(), dan.sockets()), (SeatState::Watching, 2));
    assert!(dan.view().is_some());
}

/// A seat's socket the test cuts (as a forced update or the network
/// does): closed with 1006, once.
struct Cuttable {
    inner: FakeAudienceSocket,
    cut: Rc<std::cell::Cell<bool>>,
}

impl Transport for Cuttable {
    fn open(&mut self, room: &RoomId, ticket: &str) {
        self.inner.open(room, ticket);
    }

    fn send(&mut self, text: String) {
        self.inner.send(text);
    }

    fn send_binary(&mut self, bytes: Vec<u8>) {
        self.inner.send_binary(bytes);
    }

    fn binary(&self) -> bool {
        true
    }

    fn close(&mut self) {
        self.inner.close();
    }

    fn poll(&mut self) -> Vec<ddi_room::TransportEvent> {
        if self.cut.replace(false) {
            self.inner.close();
            return vec![ddi_room::TransportEvent::Closed { code: 1006 }];
        }
        self.inner.poll()
    }
}

/// The screens: an audience link's page offers Watch and hands the seat
/// to its shell; the audience's page says the room, the count and, once
/// the host closes the audience, why it ended.
#[test]
fn the_audience_link_s_page_and_the_audience_s_page_say_what_they_should() {
    use crate::look::Plain;
    use crate::screens::audience::AudienceScreen;
    use crate::screens::online::{OnlineCall, OnlinePage, OnlineScreen};
    use crate::screens::{DESIGN, Env};
    use crate::testing::Harness;
    let env = Env::new(
        Rc::new(Plain),
        Rc::new(|s| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s)),
    );
    let clock = ManualClock::new(5_000);
    let server = room(&clock, live_audience());
    let mut alice = client(&server, person(&server, "alice"), "alice", 5_000);
    let online = watcher(&server, "carol");
    let calls: Rc<RefCell<Vec<OnlineCall>>> = Rc::default();
    let c = calls.clone();
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, crate::TextMode::Own);
    h.ui.set_clock(5.0);
    h.push(OnlineScreen::new(
        env.clone(),
        online.clone(),
        OnlinePage::Invite(AUDIENCE_LINK.into()),
        Rc::new(move |call, _| c.borrow_mut().push(call)),
    ));
    h.run(0.5);
    let texts = h.texts().join("\n");
    assert!(texts.contains("You're invited to watch with the audience."), "{texts}");
    assert!(texts.contains("Watch"), "{texts}");
    assert!(!texts.contains("people in the room"), "an audience link is no member's join");
    online.borrow_mut().watch_audience();
    h.run(0.5);
    let (link, ticket) = match calls.borrow().last() {
        Some(OnlineCall::Watch { link, ticket }) => (link.clone(), ticket.clone()),
        other => panic!("{other:?}"),
    };
    assert_eq!(link, AUDIENCE_LINK);
    let carol = Rc::new(RefCell::new(AudienceClient::new(
        online,
        &link,
        Box::new(FakeAudienceSocket::new(server.clone())),
        Some(ticket),
        5_000,
    )));
    let mut ha = Harness::new(DESIGN, [1280.0, 720.0], 1.0, crate::TextMode::Own);
    let state = env.state.clone();
    ha.push(AudienceScreen::new(env, carol.clone(), Rc::new(|_, _| {})));
    let step = |ms: u64, ha: &mut Harness, alice: &mut RoomClient| {
        for _ in 0..ms / 50 {
            clock.advance(50);
            server.borrow_mut().pump();
            alice.tick(clock.now().0);
            ha.ui.set_clock(clock.now().0 as f64 / 1_000.0);
            ha.run(0.05);
        }
    };
    step(6_000, &mut ha, &mut alice);
    let texts = ha.texts().join("\n");
    assert!(texts.contains("After Hours"), "{texts}");
    assert!(texts.contains("Hosted by alice"), "{texts}");
    assert!(texts.contains("One person watching"), "{texts}");
    assert!(texts.contains("The host is choosing the first song."), "{texts}");
    let debug = state.borrow().audience.clone().unwrap();
    assert_eq!((debug.seat.as_str(), debug.count), ("watching", Some(1)));
    alice.set_audience_open(false);
    step(3_000, &mut ha, &mut alice);
    let texts = ha.texts().join("\n");
    assert!(texts.contains("The host closed the audience."), "{texts}");
}

/// A seat's state as a watched stage's end: ended by its relay, or a new
/// ticket refused (the link replaced, the audience closed, the room
/// ended) or the session gone; joining and reconnecting are no ends.
#[test]
fn a_seat_s_end_during_a_watched_stage_is_its_relay_s_or_a_refused_ticket() {
    use crate::state::audience::{Refused, SeatState, seat_end_of};
    use crate::state::room::SeatEnd;
    use ddi_room::audience::{AudienceEnd, AudienceRefusal};
    for s in [SeatState::Joining, SeatState::Watching, SeatState::Reconnecting] {
        assert_eq!(seat_end_of(&s), None);
    }
    let cases = [
        (SeatState::Ended(AudienceEnd::Closed), SeatEnd::AudienceClosed),
        (SeatState::Ended(AudienceEnd::RoomClosed), SeatEnd::RoomClosed),
        (
            SeatState::Refused(Refused::Audience(AudienceRefusal::BadLink)),
            SeatEnd::LinkReplaced,
        ),
        (
            SeatState::Refused(Refused::Audience(AudienceRefusal::Closed)),
            SeatEnd::AudienceClosed,
        ),
        (
            SeatState::Refused(Refused::Audience(AudienceRefusal::RoomClosed)),
            SeatEnd::RoomClosed,
        ),
        (
            SeatState::Refused(Refused::Audience(AudienceRefusal::Full)),
            SeatEnd::Refused("audience_full".into()),
        ),
    ];
    for (state, end) in cases {
        let e = seat_end_of(&state).unwrap();
        assert_eq!(e, end);
        // Each said in words of its own.
        let text = crate::screens::audience::seat_end_text(&e).to_string();
        assert!(!text.is_empty());
    }
    assert_eq!(
        crate::screens::audience::seat_end_text(&SeatEnd::LinkReplaced).to_string(),
        "This audience link isn't this room's, or it was replaced."
    );
    assert_eq!(
        crate::screens::audience::seat_end_text(&SeatEnd::ClosedByOperator).to_string(),
        "The room was closed."
    );
}

/// `audience-view-v1`: a stage whose fields had to fall behind the music
/// puts the next stage's music that much further behind (rounded up, at
/// most 1.6 s), and good stages bring it back 0.2 s at a time to 0.8 s;
/// a stage watched with no lag beyond the threshold changes nothing more,
/// and nothing watched keeps it.
#[test]
fn the_music_s_delay_follows_what_the_fields_needed_stage_by_stage() {
    let v = AUDIENCE_VIEW_V1;
    let steady = |ms: f32| vec![ms; 600];
    assert_eq!(v.next_music_delay(800, &[]), 800, "nothing watched");
    assert_eq!(v.next_music_delay(800, &steady(40.0)), 800, "good: as it was");
    // Lagged: the fields 330 ms behind the music for most of it.
    let mut lagged = steady(30.0);
    lagged.extend(steady(330.0));
    assert_eq!(v.next_music_delay(800, &lagged), 1_150);
    // A spike alone (a few frames) is not a lag that moves the music.
    let mut spike = steady(20.0);
    spike.extend(vec![900.0; 20]);
    assert_eq!(v.next_music_delay(800, &spike), 800);
    // At most 1.6 s, the fields keeping 0.4 s of the ceiling.
    assert_eq!(v.next_music_delay(1_400, &steady(700.0)), 1_600);
    // Good again: back 0.2 s a stage, to 0.8 s and no lower.
    let mut d = 1_600;
    let mut seen = Vec::new();
    for _ in 0..6 {
        d = v.next_music_delay(d, &steady(10.0));
        seen.push(d);
    }
    assert_eq!(seen, [1_400, 1_200, 1_000, 800, 800, 800]);
}

/// The seat keeps the fields' delay a frame as its play notes it, and the
/// next stage's music delay is moved by it when that stage is handed over
/// (`LiveRoom::note_view_delay`); within the ceiling throughout.
#[test]
fn the_seat_notes_the_fields_delay_for_the_next_stage() {
    use crate::state::room::LiveRoom;
    let clock = ManualClock::new(5_000);
    let server = room(&clock, live_audience());
    let mut seat = arrive(&server, "carol", AUDIENCE_LINK, 5_000).unwrap();
    assert_eq!(seat.next_music_delay_ms(), 800);
    // The fields of stage 1 lagged 400 ms beyond the music.
    for _ in 0..500 {
        seat.note_view_delay(ddi_room::StageId(1), 400.0);
    }
    // Moved once the next stage is handed over (here: fixed as
    // `take_stage_order` fixes it).
    seat.music.fix(ddi_room::StageId(2));
    assert_eq!(seat.next_music_delay_ms(), 1_200);
    // The fields' own room is what is left of the ceiling.
    let d = seat.view_delay(&ddi_room::live::LIVE_V1);
    assert!(d.delay_ms() <= 800.0, "{}", d.delay_ms());
}

/// The maintainer (2026-11-15): "let the audience spectate without having
/// to sign-in and be anonymous".
#[test]
fn someone_not_signed_in_watches_with_the_audience_without_a_sign_in() {
    use crate::look::Plain;
    use crate::screens::online::{OnlineCall, OnlinePage, OnlineScreen};
    use crate::screens::{DESIGN, Env};
    use crate::state::online::{Account, SignIn};
    use crate::testing::Harness;
    let env = Env::new(
        Rc::new(Plain),
        Rc::new(|s| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s)),
    );
    let clock = ManualClock::new(5_000);
    let server = room(&clock, live_audience());
    let online = stranger(&server);
    assert_eq!(*online.borrow().account(), Account::SignedOut);
    let calls: Rc<RefCell<Vec<OnlineCall>>> = Rc::default();
    let c = calls.clone();
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, crate::TextMode::Own);
    h.ui.set_clock(5.0);
    h.push(OnlineScreen::new(
        env,
        online.clone(),
        OnlinePage::Invite(AUDIENCE_LINK.into()),
        Rc::new(move |call, _| c.borrow_mut().push(call)),
    ));
    h.run(0.5);
    // The audience link's page: Watch, and nothing of signing in or of an
    // account.
    let texts = h.texts().join("\n");
    assert!(texts.contains("Watch"), "{texts}");
    assert!(!texts.contains("Sign in") && !texts.contains("Twitch"), "{texts}");
    assert!(!texts.contains("Not signed in"), "{texts}");
    online.borrow_mut().watch_audience();
    h.run(0.5);
    // Straight to the seat: no sign-in started, no page left for one.
    assert_eq!(*online.borrow().sign_in_state(), SignIn::Idle);
    assert!(
        calls
            .borrow()
            .iter()
            .all(|c| !matches!(c, OnlineCall::Redirect(_))),
        "{:?}",
        calls.borrow()
    );
    let ticket = match calls.borrow().last() {
        Some(OnlineCall::Watch { link, ticket }) if link == AUDIENCE_LINK => ticket.clone(),
        other => panic!("{other:?}"),
    };
    let mut seat = AudienceClient::new(
        online.clone(),
        AUDIENCE_LINK,
        Box::new(FakeAudienceSocket::new(server.clone())),
        Some(ticket),
        5_000,
    );
    run(&clock, &server, &mut [], &mut [&mut seat], 2_000);
    assert_eq!(seat.state(), SeatState::Watching);
    assert!(seat.view().is_some());
    // Nobody was signed out or in.
    assert_eq!(*online.borrow().account(), Account::SignedOut);
    // Seven more from the same network; the ninth is told its network's
    // seats are taken, in words for someone not signed in.
    for _ in 0..7 {
        assert!(arrive_anonymously(&server, AUDIENCE_LINK).is_ok());
    }
    let ninth = arrive_anonymously(&server, AUDIENCE_LINK).unwrap_err();
    assert_eq!(ninth.code, crate::state::online::api::TOO_MANY_ON_NETWORK);
    assert_eq!(
        String::from(crate::screens::online::invite_problem(&ninth)),
        "Too many people on your network are already watching this room."
    );
    // A seat asking its own ticket (a lost socket's, or desktop's
    // `--join`) is told the same.
    let mut own = AudienceClient::new(
        stranger(&server),
        AUDIENCE_LINK,
        Box::new(FakeAudienceSocket::new(server.clone())),
        None,
        5_000,
    );
    own.tick(5_020);
    assert_eq!(
        own.state(),
        SeatState::Refused(Refused::Other(
            crate::state::online::api::TOO_MANY_ON_NETWORK.into()
        ))
    );
    // A signed-in person keeps their own two (never their network's).
    assert!(arrive(&server, "erin", AUDIENCE_LINK, 5_000).is_ok());
    let too_many = arrive(&server, "erin", AUDIENCE_LINK, 5_000)
        .and_then(|_| arrive(&server, "erin", AUDIENCE_LINK, 5_000))
        .unwrap_err();
    assert_eq!(
        String::from(crate::screens::online::invite_problem(&too_many)),
        "You are already watching this room on two devices."
    );
    // Thirty joins a minute from one network, then "try again in a
    // moment": the press can be made again shortly.
    let busy = (0..40)
        .find_map(|_| arrive_anonymously(&server, AUDIENCE_LINK).err().filter(|p| p.code == "audience_busy"))
        .expect("the network's joins bounded");
    assert_eq!(
        String::from(crate::screens::online::invite_problem(&busy)),
        "Many people on your network joined just now. Try again in a moment."
    );
    // The same refusal read by someone signed in (round D1: an
    // account's joins are bounded too) names no network.
    let mine = crate::state::online::api::worded_for(busy.clone(), true);
    assert_eq!(mine.code, crate::state::online::api::BUSY_ACCOUNT);
    assert_eq!(
        String::from(crate::screens::online::invite_problem(&mine)),
        "You joined this room's audience many times just now. Try again in a moment."
    );
    assert_eq!(crate::state::online::api::worded_for(busy.clone(), false).code, "audience_busy");
    // A server from before (it asks the audience for a session): said as
    // the server being updated, and nobody signed out for it.
    let old: Handler = Box::new(|req| match (req.method, req.path.as_str()) {
        (Method::Get, "/me") => ok(&me_of("ivy")),
        (Method::Get, p) if p.starts_with("/invites/") => ok(
            r#"{"kind":"audience","room_name":"R","host_display_name":"h","phase":"lobby","admission":"open","members":1,"players_full":false,"audience":true}"#,
        ),
        (Method::Post, "/rooms/Room000001/audience") => err(401, "signed_out"),
        _ => panic!("unexpected {req:?}"),
    });
    let ivy = signed_in(old);
    ivy.borrow_mut().open_invite(AUDIENCE_LINK);
    ivy.borrow_mut().poll(0);
    ivy.borrow_mut().watch_audience();
    ivy.borrow_mut().poll(0);
    let o = ivy.borrow();
    assert_eq!(o.invite().unwrap().refused.as_ref().unwrap().code, ddi_room::SERVER_BEHIND);
    assert!(o.me().is_some(), "not signed out for it");
    assert_eq!(*o.sign_in_state(), SignIn::Idle);
}

/// Someone not signed in arriving by `link`: its page's Watch, the seat
/// given (its ticket) or the refusal.
fn arrive_anonymously(server: &SharedFake, link: &str) -> Result<String, Problem> {
    let online = stranger(server);
    let mut o = online.borrow_mut();
    o.open_invite(link);
    o.poll(0);
    o.watch_audience();
    o.poll(0);
    match o.take_seated() {
        Some(s) => Ok(s.ticket),
        None => Err(o.invite().unwrap().refused.clone().unwrap()),
    }
}

/// The audience's page between stages (`audience-page-v1`, the feedback
/// round I): a card as the Ready card is, over the start menu's rain
/// before the seat watched a stage, over the last stage's picture once
/// the shell told it (kept until another replaces it), the players the
/// public view lists in the card's players part, Stop watching its one
/// button while nothing plays; a seat that joins later, knowing no
/// stage's picture, on the rain.
#[test]
fn the_audience_page_is_a_card_over_the_rain_then_the_last_stages_picture() {
    use crate::look::Plain;
    use crate::screens::audience::AudienceScreen;
    use crate::screens::{DESIGN, Env};
    use crate::testing::Harness;
    let env = Env::new(
        Rc::new(Plain),
        Rc::new(|s| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s)),
    );
    let clock = ManualClock::new(5_000);
    let server = room(&clock, live_audience());
    let mut alice = client(&server, person(&server, "alice"), "alice", 5_000);
    let carol = Rc::new(RefCell::new(arrive(&server, "carol", AUDIENCE_LINK, 5_000).unwrap()));
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, crate::TextMode::Own);
    h.push(AudienceScreen::new(env.clone(), carol.clone(), Rc::new(|_, _| {})));
    let step = |ms: u64, h: &mut Harness, alice: &mut RoomClient| {
        for _ in 0..ms / 50 {
            clock.advance(50);
            server.borrow_mut().pump();
            alice.tick(clock.now().0);
            h.ui.set_clock(clock.now().0 as f64 / 1_000.0);
            h.run(0.05);
        }
    };
    step(4_000, &mut h, &mut alice);
    let texts = h.texts().join("\n");
    assert!(texts.contains("After Hours"), "{texts}");
    assert!(texts.contains("The host is choosing the first song."), "{texts}");
    // The card, translucent, over the rain; the players listed.
    assert!(h.ui.actor("Card").is_some());
    assert!(h.ui.actor("MenuBackground").is_some(), "the rain before a first stage");
    assert!(h.ui.actor("Audience.Backdrop").is_none());
    let names: Vec<String> = (0..4)
        .filter_map(|k| {
            h.ui.actor(&format!("Audience.Player{k}.Name"))
                .and_then(|a| match &a.kind {
                    ddi_actor::Kind::Text(t) => Some(t.text().to_string()),
                    _ => None,
                })
        })
        .collect();
    assert!(names.iter().any(|n| n == "alice") && names.iter().any(|n| n == "bob"), "{names:?}");
    assert_eq!(env.state.borrow().page_items, vec!["Item.back".to_string()]);
    let card = h.ui.bounds("Card").unwrap();
    assert!(card[2] - card[0] >= 1280.0 * 0.85, "most of the screen: {card:?}");
    // A stage watched: its picture told by the shell; the page over it.
    carol
        .borrow_mut()
        .set_backdrop(Some(("song:after-hours:bg".into(), 4.0 / 3.0)));
    step(200, &mut h, &mut alice);
    assert!(h.ui.actor("Audience.Backdrop").is_some(), "the last stage's picture");
    assert!(h.ui.actor("MenuBackground").is_none());
    // Nothing told by the next stage: the picture stays.
    carol.borrow_mut().set_backdrop(None);
    step(200, &mut h, &mut alice);
    assert!(h.ui.actor("Audience.Backdrop").is_some());
    // A seat that joins now knows no stage's picture: the rain.
    let dan = Rc::new(RefCell::new(
        arrive(&server, "dan", AUDIENCE_LINK, ddi_room::Clock::now(&clock).0).unwrap(),
    ));
    let mut hd = Harness::new(DESIGN, [1280.0, 720.0], 1.0, crate::TextMode::Own);
    hd.push(AudienceScreen::new(env, dan.clone(), Rc::new(|_, _| {})));
    run(&clock, &server, &mut [], &mut [&mut dan.borrow_mut()], 2_000);
    hd.run(0.1);
    assert!(hd.ui.actor("MenuBackground").is_some());
    // Escape stops watching.
    hd.key("Escape", "Escape");
    hd.run(0.1);
}

/// ui-screens-6: the audience's page counts a stage's countdown down on
/// the room's clock where the seat knows it (`countdown_seconds`, the
/// card built again as the seconds change), never "starts in 0" for the
/// whole of it.
#[test]
fn the_audience_page_counts_the_countdown_down() {
    use crate::look::Plain;
    use crate::screens::audience::AudienceScreen;
    use crate::screens::{DESIGN, Env};
    use crate::testing::Harness;
    let env = Env::new(
        Rc::new(Plain),
        Rc::new(|s| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s)),
    );
    let clock = ManualClock::new(5_000);
    let server = room(&clock, live_audience());
    let mut alice = client(&server, person(&server, "alice"), "alice", 5_000);
    let carol = Rc::new(RefCell::new(arrive(&server, "carol", AUDIENCE_LINK, 5_000).unwrap()));
    let mut h = Harness::new(DESIGN, [1280.0, 720.0], 1.0, crate::TextMode::Own);
    h.push(AudienceScreen::new(env, carol.clone(), Rc::new(|_, _| {})));
    let step = |ms: u64, h: &mut Harness, alice: &mut RoomClient| {
        for _ in 0..ms / 50 {
            clock.advance(50);
            server.borrow_mut().pump();
            alice.tick(clock.now().0);
            carol.borrow_mut().tick(clock.now().0);
            h.ui.set_clock(clock.now().0 as f64 / 1_000.0);
            h.run(0.05);
        }
    };
    step(3_000, &mut h, &mut alice);
    let start_at = ddi_room::Millis(clock.now().0 + 9_000);
    server.borrow_mut().rewrite(|s| {
        s.phase = ddi_room::Phase::Countdown {
            stage: ddi_room::StageId(1),
            start_at,
        };
    });
    step(1_000, &mut h, &mut alice);
    let texts = h.texts().join("\n");
    assert!(!texts.contains("starts in 0"), "{texts}");
    // The seat knows the room's clock from its first stage's frames: until
    // then the stage is about to start, without a count.
    assert!(carol.borrow().room_ms_at(clock.now().0).is_none());
    assert!(texts.contains("Stage 1 is about to start."), "{texts}");
}

/// ui-state-3 (fix round G1): a stage picked with one song, then backed
/// out of and picked again with another, is prepared again with the
/// second; an answer for the first that comes late is dropped.
#[test]
fn a_stage_picked_again_with_another_song_is_prepared_again() {
    use crate::state::online::tests::room::song;
    use ddi_room::{Command, ContentStatus};
    let clock = ManualClock::new(5_000);
    let server = room(&clock, live_audience());
    let mut alice = client(&server, person(&server, "alice"), "alice", 5_000);
    let mut carol = arrive(&server, "carol", AUDIENCE_LINK, 5_000).unwrap();
    run(&clock, &server, &mut [&mut alice], &mut [&mut carol], 1_000);
    let shared = |n: u8| {
        let mut s = song();
        s.id = format!("bundled:{n}");
        s.charts[0].digest = Some(ddi_chart::ChartDigest(u128::from(n)));
        s
    };
    let pick = |alice: &mut RoomClient, s: &crate::screens::data::SongEntry, share: &str| {
        alice.pick(s, &s.charts[0]).unwrap();
        let stage = alice.stages()[0].id;
        server
            .borrow_mut()
            .authority(Command::SetContentStatus {
                stage,
                status: ContentStatus::Available,
                share: Some(share.into()),
            })
            .unwrap();
        stage
    };
    let x = shared(7);
    let stage = pick(&mut alice, &x, "sharex01");
    run(&clock, &server, &mut [&mut alice], &mut [&mut carol], 1_000);
    let first = carol.take_load_jobs();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].share.as_deref(), Some("sharex01"));
    // Backed out of and picked again, the same stage, another song.
    alice.leave_prompt(stage);
    run(&clock, &server, &mut [&mut alice], &mut [&mut carol], 200);
    let y = shared(8);
    assert_eq!(pick(&mut alice, &y, "sharey01"), stage);
    run(&clock, &server, &mut [&mut alice], &mut [&mut carol], 1_000);
    let second = carol.take_load_jobs();
    assert_eq!(second.len(), 1, "the new song asked for");
    assert_eq!(second[0].share.as_deref(), Some("sharey01"));
    assert_eq!(second[0].digest, Some(ddi_chart::ChartDigest(8)));
    // The first song's answer, late: not the stage's any more.
    carol.finish_load(&first[0], Err(LoadProblem::Download));
    assert_eq!(carol.load_problem(stage), None);
}

/// A seat's socket whose relay refuses it: closed (1006) each time before
/// it ever opens.
struct Refusing;

impl Transport for Refusing {
    fn open(&mut self, _room: &RoomId, _ticket: &str) {}

    fn send(&mut self, _text: String) {}

    fn send_binary(&mut self, _bytes: Vec<u8>) {}

    fn binary(&self) -> bool {
        true
    }

    fn close(&mut self) {}

    fn poll(&mut self) -> Vec<ddi_room::TransportEvent> {
        vec![ddi_room::TransportEvent::Closed { code: 1006 }]
    }
}

/// ui-state-8 (fix round G1): a seat whose socket closes before it ever
/// opens asks for a new ticket after the same waits as a lost one, never
/// at every tick.
#[test]
fn a_socket_that_never_opens_is_asked_for_again_after_a_wait() {
    use crate::state::online::tests::FakeHttp;
    let clock = ManualClock::new(5_000);
    let server = room(&clock, live_audience());
    let log = Rc::new(RefCell::new(Vec::new()));
    let s = server.clone();
    let handler: Handler = Box::new(move |req| match (req.method, req.path.as_str()) {
        (Method::Get, "/me") => ok(&me_of("dan")),
        (Method::Post, "/rooms/Room000001/audience") => {
            let (status, body) = s
                .borrow_mut()
                .audience_join_answer(Some("dev:dan"), AUDIENCE_LINK, None);
            Some(crate::state::online::api::ApiResponse {
                status,
                body,
                session: None,
            })
        }
        _ => panic!("unexpected {req:?}"),
    });
    let online = crate::state::online::Online::new(
        Box::new(FakeHttp {
            handler: Rc::new(RefCell::new(handler)),
            log: log.clone(),
            out: Default::default(),
        }),
        crate::state::online::OnlineConfig {
            platform: crate::state::online::Platform::Web,
            site: "http://localhost:8080/".into(),
        },
    )
    .shared();
    let mut dan = AudienceClient::new(online, AUDIENCE_LINK, Box::new(Refusing), None, 5_000);
    let joins = || {
        log.borrow()
            .iter()
            .filter(|r| r.path == "/rooms/Room000001/audience")
            .count()
    };
    run(&clock, &server, &mut [], &mut [&mut dan], 400);
    let first = ReconnectPolicy::RECONNECT_BACKOFF_V0.wait_ms(1);
    assert!(first >= 400, "{first}");
    assert!(joins() <= 2, "asked {} times in 0.4 s", joins());
    run(&clock, &server, &mut [], &mut [&mut dan], 10_000);
    assert!(joins() <= 6, "asked {} times in 10 s", joins());
}

/// How two starters of a test stage performed: the first survived on the
/// stage's chart with attacks sent and taken, the second went out having
/// stepped down a chart.
pub(crate) fn test_details() -> [ddi_room::RunDetail; 2] {
    [
        ddi_room::RunDetail {
            score: Some(5_900_000),
            percent: Some(9_850),
            grade: "S".into(),
            life_ppm: 1_000_000,
            max_combo: 320,
            tiers: vec![200, 90, 20, 6, 2, 1, 1],
            attacks: Some(3),
            received: Some(1),
            shocks: Some(2),
            ..Default::default()
        },
        ddi_room::RunDetail {
            score: Some(2_100_000),
            percent: Some(4_100),
            grade: "C".into(),
            life_ppm: 0,
            out_at_ms: Some(41_000),
            max_combo: 45,
            tiers: vec![60, 50, 40, 20, 10, 5, 30],
            attacks: Some(1),
            received: Some(3),
            shocks: Some(9),
            chart: Some("Medium".into()),
            ..Default::default()
        },
    ]
}

/// The room's next stage played to its settlement by `alice` (who picks
/// song `n`, shared as `share`) and `bob`: picked, available, both loaded
/// and ready, started, each one's run reported with [`test_details`]
/// (Alice survives, Bob goes out), the song ended; the seats ticked
/// along. The stage.
pub(crate) fn play_stage(
    clock: &ManualClock,
    server: &SharedFake,
    [alice, bob]: [&mut RoomClient; 2],
    seats: &mut [&mut AudienceClient],
    n: u8,
    share: &str,
) -> StageId {
    use crate::state::online::tests::room::{prepare, song};
    use ddi_room::{Command, ContentStatus, Finish, Finisher, Run};
    let mut s = song();
    s.id = format!("bundled:{n}");
    s.title = format!("Song {n}");
    s.artist = "Someone".into();
    s.charts[0].digest = Some(ddi_chart::ChartDigest(u128::from(n)));
    alice.pick(&s, &s.charts[0]).unwrap();
    run(clock, server, &mut [alice, bob], seats, 400);
    let Phase::Preparing { stage, .. } = alice.state().phase else {
        panic!("the pick opens the stage: {:?}", alice.state().phase);
    };
    server
        .borrow_mut()
        .authority(Command::SetContentStatus {
            stage,
            status: ContentStatus::Available,
            share: Some(share.into()),
        })
        .unwrap();
    run(clock, server, &mut [alice, bob], seats, 400);
    prepare(alice);
    prepare(bob);
    alice.set_ready(true).unwrap();
    bob.set_ready(true).unwrap();
    run(clock, server, &mut [alice, bob], seats, 8_000);
    assert!(
        matches!(alice.state().phase, Phase::Stage { .. }),
        "the stage plays: {:?}",
        alice.state().phase
    );
    let [a, b] = test_details();
    let (ia, ib) = (alice.me(), bob.me());
    alice
        .report_run(stage, Run::Live { life: 1.0, score: 5_900_000, steps: 0 }, Some(a))
        .unwrap();
    bob.report_run(stage, Run::Out { at: 41.0 }, Some(b)).unwrap();
    run(clock, server, &mut [alice, bob], seats, 400);
    if !alice.stage_confirmed(stage) {
        // A live room's result is its ledger's: said by the authority, as
        // the room's live side says it.
        let ranking = vec![
            vec![Finisher { participant: ia, finish: Finish::Survived }],
            vec![Finisher { participant: ib, finish: Finish::Eliminated }],
        ];
        server
            .borrow_mut()
            .authority(Command::EndSong { stage, ranking: Some(ranking) })
            .unwrap();
    } else if matches!(alice.state().phase, Phase::SongCompletion { .. } | Phase::Stage { .. }) {
        server
            .borrow_mut()
            .authority(Command::EndSong { stage, ranking: None })
            .unwrap();
    }
    // (The audience's view comes once a second at most.)
    run(clock, server, &mut [alice, bob], seats, 2_400);
    assert!(alice.stage_confirmed(stage));
    stage
}

/// The next match's rules saved by the host with two stages.
pub(crate) fn two_stages(alice: &mut RoomClient) {
    let mut rules = alice.rules().clone();
    rules.stages = 2;
    alice.save_rules(rules).unwrap();
}

/// A member's table of `stage` as the public view can say it: without
/// the starters' raw runs (the public view carries none) and the
/// member's own attacks' summary.
fn public_part(mut t: StageTable) -> StageTable {
    for row in &mut t.rows {
        row.run = None;
    }
    t.summary = None;
    t
}

/// Round D3, item 1: the audience's standings are the members' table.
/// After a stage and after the match, an audience seat's table of the
/// stage (built from the public view by the members' own builder) has
/// the members' rows: the place, finish and award, the totals and the
/// places in the match and how they moved, each starter's details (the
/// score and percent, the grade, life or when out, the combo, the
/// judgments, the attacks' three counts, the chart finished on), the
/// stage's rules, and the match's result on top after the last stage.
/// As drawn, the lines say the same words, but for whose own line it is.
#[test]
fn the_audience_s_standings_are_the_members_table() {
    use crate::screens::audience::PublicWho;
    use crate::screens::results::{Tab, stage_view_of};
    let clock = ManualClock::new(5_000);
    let server = room(&clock, live_audience());
    // A match of two stages.
    let mut alice = client(&server, person(&server, "alice"), "alice", 5_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 5_000);
    let mut carol = arrive(&server, "carol", AUDIENCE_LINK, 5_000).unwrap();
    run(&clock, &server, &mut [&mut alice, &mut bob], &mut [&mut carol], 1_000);
    two_stages(&mut alice);
    run(&clock, &server, &mut [&mut alice, &mut bob], &mut [&mut carol], 400);
    // Nothing settled: no table.
    assert_eq!(carol.settled_stage(), None);
    let compare = |bob: &RoomClient, carol: &AudienceClient, stage: StageId| {
        let theirs = public_part(bob.stage_table(stage).expect("a member's table"));
        let ours = carol.stage_table(stage).expect("the audience's table");
        assert_eq!(ours, theirs);
        assert!(ours.confirmed);
        // As drawn (the debug state's words), wide enough for every
        // column: the same lines, nobody's own.
        let mut member = stage_view_of(bob, &theirs, Tab::Standings, 724.0);
        for row in &mut member.rows {
            (row.me, row.badge) = (false, false);
        }
        let seat = stage_view_of(&PublicWho(carol.view().unwrap()), &ours, Tab::Standings, 724.0);
        assert_eq!(seat, member);
        ours
    };
    let first = play_stage(&clock, &server, [&mut alice, &mut bob], &mut [&mut carol], 1, "share001");
    assert_eq!(carol.settled_stage(), Some(first));
    let t = compare(&bob, &carol, first);
    assert_eq!((t.number, t.of, t.result.is_none()), (1, 2, true));
    assert!(t.attacks, "a live room's match has attacks: the three columns");
    let [a, b] = test_details();
    assert_eq!(t.rows[0].detail.as_ref(), Some(&a));
    assert_eq!(t.rows[1].detail.as_ref(), Some(&b));
    assert_eq!(t.rows[0].placement.unwrap().place, 1);
    assert_eq!((t.rows[0].rank, t.rows[0].rank_before), (Some(1), None));
    // The last stage: the match's result on top.
    let second = play_stage(&clock, &server, [&mut alice, &mut bob], &mut [&mut carol], 2, "share002");
    run(&clock, &server, &mut [&mut alice, &mut bob], &mut [&mut carol], 2_000);
    assert_eq!(alice.state().phase, Phase::MatchResults);
    assert_eq!(carol.settled_stage(), Some(second));
    let t = compare(&bob, &carol, second);
    let result = t.result.expect("the match's result");
    assert_eq!(result.iter().map(|(_, rank, _)| *rank).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(t.rows[0].rank_before, Some(1));
    // The first stage's table is still the members'.
    compare(&bob, &carol, first);
}

/// The songs the audience's page asked its shell to prepare.
type Asked = Rc<RefCell<Vec<LoadJob>>>;

/// The audience's page over `seat` in a harness of `size`, in the plain
/// look.
fn page(
    seat: &SharedAudience,
    size: [f32; 2],
) -> (crate::screens::Env, crate::testing::Harness) {
    let (env, h, _) = page_asking(seat, size);
    (env, h)
}

/// [`page`], with the songs it asks its shell to prepare.
fn page_asking(
    seat: &SharedAudience,
    size: [f32; 2],
) -> (crate::screens::Env, crate::testing::Harness, Asked) {
    use crate::screens::audience::{AudienceCall, AudienceScreen};
    let asked = Asked::default();
    let sink = asked.clone();
    let env = crate::screens::Env::new(
        Rc::new(crate::look::Plain),
        Rc::new(|s| ddi_library::search::Fold::nfkd(&ddi_library::search::Nfkd, s)),
    );
    let design = if size[0] < 1000.0 { crate::screens::COMPACT } else { crate::screens::DESIGN };
    let mut h = crate::testing::Harness::new(design, size, 1.0, crate::TextMode::Own);
    h.push(AudienceScreen::new(
        env.clone(),
        seat.clone(),
        Rc::new(move |call, _| {
            if let AudienceCall::Prepare(job) = call {
                sink.borrow_mut().push(job);
            }
        }),
    ));
    (env, h, asked)
}

/// Round D3, item 1: the page between stages has the last stage's
/// standings as a second part beside the players': no tabs before a
/// stage settles; once one does, the tabs, the standings brought up
/// (once: the part chosen after stays), the members' table's lines and
/// columns; the shoulders, a tap and the focus walk (Up from the
/// buttons, Left and Right, Enter) change the part; after the match its
/// result on top; a phone's has fewer columns, as a member's.
#[test]
fn the_audience_page_brings_the_standings_up_once_a_stage_settles() {
    use crate::nav::MenuAction;
    use crate::event::PointerKind;
    let clock = ManualClock::new(5_000);
    let server = room(&clock, live_audience());
    let mut alice = client(&server, person(&server, "alice"), "alice", 5_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 5_000);
    let carol = Rc::new(RefCell::new(arrive(&server, "carol", AUDIENCE_LINK, 5_000).unwrap()));
    let (env, mut h) = page(&carol, [1280.0, 720.0]);
    let (_, mut phone) = page(&carol, [390.0, 844.0]);
    run(&clock, &server, &mut [&mut alice, &mut bob], &mut [&mut carol.borrow_mut()], 1_000);
    two_stages(&mut alice);
    run(&clock, &server, &mut [&mut alice, &mut bob], &mut [&mut carol.borrow_mut()], 1_000);
    h.run(0.3);
    let view = |env: &crate::screens::Env| env.state.borrow().audience.clone().unwrap();
    assert!(h.ui.actor("Tab.Standings").is_none(), "no tabs before a stage settles");
    assert!(h.ui.actor("Audience.Player0.Name").is_some());
    assert_eq!((view(&env).tabs, view(&env).tab.as_str(), view(&env).table), (false, "players", None));
    play_stage(
        &clock,
        &server,
        [&mut alice, &mut bob],
        &mut [&mut carol.borrow_mut()],
        1,
        "share001",
    );
    h.run(0.3);
    // The standings, brought up: the members' lines and every column.
    let v = view(&env);
    assert_eq!((v.tabs, v.tab.as_str()), (true, "standings"));
    let table = v.table.expect("the table");
    assert_eq!(
        table.columns,
        ["award", "total", "match", "score", "grade", "life", "judgments", "sent", "taken", "shocks"]
    );
    assert_eq!(
        table.rows.iter().map(|r| (r.name.as_str(), r.place)).collect::<Vec<_>>(),
        [("alice", Some(1)), ("bob", Some(2))]
    );
    assert_eq!(table.rows[0].attacks, Some((3, 1, 2)));
    assert_eq!(table.rows[1].stepped.as_deref(), Some("↓ Medium"));
    assert!(table.rows.iter().all(|r| !r.me), "nobody's own line");
    for name in ["Tab.Players", "Tab.Standings", "Stand0", "Stand1", "Head.judgments", "Audience.TableTitle"] {
        assert!(h.ui.actor(name).is_some(), "{name}");
    }
    assert!(h.ui.actor("Audience.Player0.Name").is_none(), "the players' part gives way");
    let texts = h.texts().join("\n");
    assert!(texts.contains("Out 0:41") && texts.contains("5900000"), "{texts}");
    // The table keeps inside the card.
    let (card, line) = (h.ui.bounds("Card").unwrap(), h.ui.bounds("Stand1").unwrap());
    assert!(line[0] >= card[0] && line[2] <= card[2] && line[3] <= card[3], "{line:?} in {card:?}");
    // A controller's shoulders change the part.
    h.pad(MenuAction::Previous, true);
    h.run(0.1);
    assert_eq!(view(&env).tab, "players");
    assert!(h.ui.actor("Audience.Player0.Name").is_some() && h.ui.actor("Stand0").is_none());
    h.pad(MenuAction::Next, true);
    h.run(0.1);
    assert_eq!(view(&env).tab, "standings");
    // The focus walk: Up from the buttons to the tab shown, Left to
    // Players, Enter; Down back to the buttons.
    h.key("ArrowUp", "ArrowUp");
    assert_eq!(h.ui.focused(), Some("Tab.Standings"));
    h.key("ArrowLeft", "ArrowLeft");
    assert_eq!(h.ui.focused(), Some("Tab.Players"));
    h.key("Enter", "Enter");
    h.run(0.1);
    assert_eq!(view(&env).tab, "players");
    h.key("ArrowDown", "ArrowDown");
    assert_eq!(h.ui.focused(), Some("Item.back"));
    // A tap.
    h.tap(PointerKind::Touch, "Tab.Standings");
    h.run(0.1);
    assert_eq!(view(&env).tab, "standings");
    // The part chosen stays while nothing new settles.
    h.tap(PointerKind::Mouse, "Tab.Players");
    h.run(0.1);
    assert_eq!(view(&env).tab, "players", "a click");
    run(&clock, &server, &mut [&mut alice, &mut bob], &mut [&mut carol.borrow_mut()], 2_000);
    h.run(0.3);
    assert_eq!(view(&env).tab, "players");
    // A phone held upright: the place, the name, the award, the total and
    // the score, inside its card.
    phone.run(0.3);
    assert!(phone.ui.actor("Stand1").is_some());
    assert!(phone.ui.actor("Head.score").is_some() && phone.ui.actor("Head.judgments").is_none());
    let (card, line) = (phone.ui.bounds("Card").unwrap(), phone.ui.bounds("Stand1").unwrap());
    assert!(line[0] >= card[0] && line[2] <= card[2] && line[3] <= card[3], "{line:?} in {card:?}");
    // The last stage: its standings brought up again, the match's result
    // on top.
    play_stage(
        &clock,
        &server,
        [&mut alice, &mut bob],
        &mut [&mut carol.borrow_mut()],
        2,
        "share002",
    );
    run(&clock, &server, &mut [&mut alice, &mut bob], &mut [&mut carol.borrow_mut()], 2_000);
    h.run(0.3);
    let v = view(&env);
    assert_eq!(v.tab, "standings");
    let table = v.table.unwrap();
    assert_eq!((table.number, table.of), (2, 2));
    assert_eq!(table.winners, ["alice"]);
    assert_eq!(table.result.len(), 2);
    assert!(h.ui.actor("Final.Winners").is_some());
    let texts = h.texts().join("\n");
    assert!(texts.contains("The match is over."), "{texts}");
}

/// Round D3, item 2: the next song's banner on the page. Nothing before
/// the pick (the picker's resting song is not public); from the pick the
/// song's title and artist beside a plain plate; the banner once the seat
/// read the room's share of the song (its listing names it, as a
/// member's client finds it); a song without a banner keeps the plate.
#[test]
fn the_audience_page_shows_the_picked_song_s_banner_once_the_share_is_read() {
    use crate::state::online::tests::room::song;
    use ddi_room::{Command, ContentStatus};
    let clock = ManualClock::new(5_000);
    let server = room(&clock, live_audience());
    let mut alice = client(&server, person(&server, "alice"), "alice", 5_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 5_000);
    let carol = Rc::new(RefCell::new(arrive(&server, "carol", AUDIENCE_LINK, 5_000).unwrap()));
    let (env, mut h, asked) = page_asking(&carol, [1280.0, 720.0]);
    let view = |env: &crate::screens::Env| env.state.borrow().audience.clone().unwrap();
    let step = |h: &mut crate::testing::Harness, alice: &mut RoomClient, bob: &mut RoomClient, ms| {
        run(&clock, &server, &mut [alice, bob], &mut [&mut carol.borrow_mut()], ms);
        h.run(0.3);
    };
    step(&mut h, &mut alice, &mut bob, 1_000);
    // The picker rests on a song: not the audience's to see.
    let mut s = song();
    s.title = "Night Drive".into();
    s.artist = "Someone".into();
    alice.hover(Some((&s, &s.charts[0])));
    step(&mut h, &mut alice, &mut bob, 2_000);
    assert!(h.ui.actor("Audience.BannerPlate").is_none() && h.ui.actor("Audience.Song").is_none());
    assert_eq!((view(&env).song, view(&env).banner), (None, None));
    // Picked: the title and artist beside the plain plate, at once.
    alice.pick(&s, &s.charts[0]).unwrap();
    step(&mut h, &mut alice, &mut bob, 2_000);
    let stage = alice.stages()[0].id;
    assert!(h.ui.actor("Audience.BannerPlate").is_some() && h.ui.actor("Audience.Banner").is_none());
    let texts = h.texts().join("\n");
    assert!(texts.contains("Night Drive") && texts.contains("Someone"), "{texts}");
    assert_eq!(
        (view(&env).song.as_deref(), view(&env).banner.as_deref()),
        (Some("Night Drive"), Some("plate"))
    );
    // The room's share of it: the seat reads it, and has its banner.
    server
        .borrow_mut()
        .authority(Command::SetContentStatus {
            stage,
            status: ContentStatus::Available,
            share: Some("sharex01".into()),
        })
        .unwrap();
    step(&mut h, &mut alice, &mut bob, 2_000);
    let jobs = asked.borrow().clone();
    assert_eq!(jobs.len(), 1);
    assert_eq!(carol.borrow().banner(stage), None, "not read yet");
    assert!(h.ui.actor("Audience.Banner").is_none());
    let read = |banner: Option<&str>| {
        let mut found = s.clone();
        found.id = jobs[0].song.clone();
        found.banner = banner.map(str::to_string);
        Ok((Picked { chart: found.charts[0].clone(), song: found }, None))
    };
    carol
        .borrow_mut()
        .finish_load(&jobs[0], read(Some("share:sharex01/banner.png")));
    step(&mut h, &mut alice, &mut bob, 200);
    assert_eq!(carol.borrow().banner(stage), Some("share:sharex01/banner.png"));
    assert!(h.ui.actor("Audience.Banner").is_some(), "the banner, once read");
    assert_eq!(view(&env).banner.as_deref(), Some("banner"));
    // Inside the card's lines' column, beside its title.
    let (banner, title) = (h.ui.bounds("Audience.Banner").unwrap(), h.ui.bounds("Audience.Song").unwrap());
    assert!(banner[2] <= title[0] && banner[1] <= title[1] && title[3] <= banner[3], "{banner:?} {title:?}");
    // A song without a banner: the plate stays.
    carol.borrow_mut().finish_load(&jobs[0], read(None));
    step(&mut h, &mut alice, &mut bob, 200);
    assert!(h.ui.actor("Audience.BannerPlate").is_some() && h.ui.actor("Audience.Banner").is_none());
    assert_eq!(view(&env).banner.as_deref(), Some("plate"));
}

/// Round D3, item 3: a seat that joins between stages shows the last
/// played stage's picture behind its card. No stage played yet: the
/// rain, nothing asked. A stage played before the seat came: its song
/// asked for from the share the public view names for it (as a stage's
/// song, without what live play needs), the rain meanwhile; its
/// background, once read, in the rain's place. A read that fails, a song
/// without a background and a stage whose share is gone (the view names
/// none) keep the rain. A seat that read the stage's song but watched
/// none of it has the picture from that read.
#[test]
fn a_late_joiner_s_page_shows_the_last_played_stage_s_picture() {
    use crate::state::online::tests::room::song;
    let clock = ManualClock::new(5_000);
    let server = room(&clock, live_audience());
    let mut alice = client(&server, person(&server, "alice"), "alice", 5_000);
    let mut bob = client(&server, person(&server, "bob"), "bob", 5_000);
    let read = |job: &LoadJob| -> LoadResult {
        let mut found = song();
        found.id = job.song.clone();
        Ok((Picked { chart: found.charts[0].clone(), song: found }, None))
    };
    let late = |who: &'static str| {
        let seat = Rc::new(RefCell::new(
            arrive(&server, who, AUDIENCE_LINK, clock.now().0).unwrap(),
        ));
        let (_, h, asked) = page_asking(&seat, [1280.0, 720.0]);
        (seat, h, asked)
    };
    let rain = |h: &crate::testing::Harness| {
        h.ui.actor("MenuBackground").is_some() && h.ui.actor("Audience.Backdrop").is_none()
    };
    let picture = |h: &crate::testing::Harness| {
        h.ui.actor("Audience.Backdrop").is_some() && h.ui.actor("MenuBackground").is_none()
    };
    // Before any stage: the rain, nothing asked.
    let (early, mut he, asked_early) = late("carol");
    run(&clock, &server, &mut [&mut alice, &mut bob], &mut [&mut early.borrow_mut()], 2_000);
    he.run(0.3);
    assert!(rain(&he) && asked_early.borrow().is_empty());
    // A stage played. The seat that was there read its song (with its
    // background) and watched none of it: the picture from that read.
    two_stages(&mut alice);
    let prepare_early = |ms: u64, alice: &mut RoomClient, bob: &mut RoomClient, he: &mut crate::testing::Harness| {
        run(&clock, &server, &mut [alice, bob], &mut [&mut early.borrow_mut()], ms);
        he.run(0.2);
        for job in asked_early.borrow_mut().drain(..) {
            early
                .borrow_mut()
                .finish_load_with(&job, read(&job), Some("share001/bg.png".into()));
        }
    };
    let stage = {
        use ddi_room::{Command, ContentStatus};
        let mut s = song();
        s.charts[0].digest = Some(ddi_chart::ChartDigest(1));
        alice.pick(&s, &s.charts[0]).unwrap();
        prepare_early(400, &mut alice, &mut bob, &mut he);
        let stage = alice.stages()[0].id;
        server
            .borrow_mut()
            .authority(Command::SetContentStatus {
                stage,
                status: ContentStatus::Available,
                share: Some("share001".into()),
            })
            .unwrap();
        prepare_early(1_400, &mut alice, &mut bob, &mut he);
        assert!(rain(&he), "nothing played yet");
        stage
    };
    // (The stage ends by its starters' reports, as `play_stage` has it.)
    {
        use crate::state::online::tests::room::prepare;
        use ddi_room::{Command, Finish, Finisher, Run};
        prepare(&mut alice);
        prepare(&mut bob);
        alice.set_ready(true).unwrap();
        bob.set_ready(true).unwrap();
        prepare_early(8_000, &mut alice, &mut bob, &mut he);
        assert!(matches!(alice.state().phase, Phase::Stage { .. }));
        assert!(rain(&he), "while the first stage plays: nothing played yet");
        alice
            .report_run(stage, Run::Live { life: 1.0, score: 1, steps: 0 }, None)
            .unwrap();
        bob.report_run(stage, Run::Out { at: 3.0 }, None).unwrap();
        prepare_early(400, &mut alice, &mut bob, &mut he);
        if !alice.stage_confirmed(stage) {
            let ranking = vec![
                vec![Finisher { participant: alice.me(), finish: Finish::Survived }],
                vec![Finisher { participant: bob.me(), finish: Finish::Eliminated }],
            ];
            server
                .borrow_mut()
                .authority(Command::EndSong { stage, ranking: Some(ranking) })
                .unwrap();
        }
        prepare_early(2_400, &mut alice, &mut bob, &mut he);
    }
    assert_eq!(early.borrow().settled_stage(), Some(stage));
    assert!(picture(&he), "the song as this seat read it");
    assert_eq!(early.borrow().backdrop().map(|b| b.0.as_str()), Some("share001/bg.png"));
    // A seat that joins now: the rain, the stage's song asked for from
    // the share the view names; its background in the rain's place.
    let (dan, mut hd, asked) = late("dan");
    run(&clock, &server, &mut [&mut alice, &mut bob], &mut [&mut dan.borrow_mut()], 2_000);
    hd.run(0.3);
    assert!(rain(&hd), "until the picture is read");
    let jobs = asked.borrow().clone();
    assert_eq!(jobs.len(), 1, "asked once");
    assert_eq!(
        (jobs[0].stage, jobs[0].share.as_deref(), jobs[0].digest, jobs[0].live.is_none()),
        (stage, Some("share001"), Some(ddi_chart::ChartDigest(1)), true)
    );
    dan.borrow_mut()
        .finish_load_with(&jobs[0], read(&jobs[0]), Some("share001/bg.png".into()));
    run(&clock, &server, &mut [&mut alice, &mut bob], &mut [&mut dan.borrow_mut()], 400);
    hd.run(0.3);
    assert!(picture(&hd), "the last played stage's picture");
    assert_eq!(asked.borrow().len(), 1, "not asked again");
    // A stage this seat watches replaces it, as before.
    dan.borrow_mut().set_backdrop(Some(("watched".into(), 1.5)));
    assert_eq!(dan.borrow().backdrop().map(|b| b.0.as_str()), Some("watched"));
    // The read fails; the song has no background: the rain stays.
    for (who, answer) in [("erin", None), ("fay", Some(None))] {
        let (seat, mut h, asked) = late(who);
        run(&clock, &server, &mut [&mut alice, &mut bob], &mut [&mut seat.borrow_mut()], 2_000);
        h.run(0.3);
        let jobs = asked.borrow().clone();
        assert_eq!(jobs.len(), 1);
        match answer {
            None => seat.borrow_mut().finish_load(&jobs[0], Err(LoadProblem::Download)),
            Some(background) => seat
                .borrow_mut()
                .finish_load_with(&jobs[0], read(&jobs[0]), background),
        }
        run(&clock, &server, &mut [&mut alice, &mut bob], &mut [&mut seat.borrow_mut()], 2_000);
        h.run(0.3);
        assert!(rain(&h), "{who}");
        assert_eq!(asked.borrow().len(), 1, "{who}: not asked again");
        assert_eq!(seat.borrow().load_problem(stage), None, "{who}: no stage's problem");
    }
    // The share gone (the view names none for the stage): nothing asked.
    server.borrow_mut().rewrite(|s| {
        let m = s.current.as_mut().unwrap();
        m.stages[0].content.as_mut().unwrap().share = None;
    });
    // (The audience's view comes once a second at most.)
    run(&clock, &server, &mut [&mut alice, &mut bob], &mut [], 2_000);
    let (gus, mut hg, asked) = late("gus");
    run(&clock, &server, &mut [&mut alice, &mut bob], &mut [&mut gus.borrow_mut()], 2_000);
    hg.run(0.3);
    assert_eq!(gus.borrow().settled_stage(), Some(stage));
    assert!(rain(&hg) && asked.borrow().is_empty());
}

/// Found in round D3's browser walk (a phone held upright, back from a
/// watched stage: the page's size came after its first frame and the
/// card stayed laid out for the window's design, clipped): the page is
/// laid out again when the screen changes shape.
#[test]
fn the_audience_page_is_laid_out_again_when_the_screen_changes() {
    let clock = ManualClock::new(5_000);
    let server = room(&clock, live_audience());
    let carol = Rc::new(RefCell::new(arrive(&server, "carol", AUDIENCE_LINK, 5_000).unwrap()));
    let (_, mut h) = page(&carol, [1280.0, 720.0]);
    run(&clock, &server, &mut [], &mut [&mut carol.borrow_mut()], 2_000);
    h.run(0.3);
    let wide = h.ui.bounds("Card").unwrap();
    assert!(wide[2] - wide[0] > 1000.0, "{wide:?}");
    // A phone held upright: the compact design, its own size.
    h.ui.set_design(crate::screens::COMPACT);
    h.send(crate::event::UiEvent::Resize {
        size: [390.0, 844.0],
        ratio: 1.0,
    });
    h.run(0.3);
    let (card, vp) = (h.ui.bounds("Card").unwrap(), h.ui.viewport().screen);
    assert!(card[0] >= 0.0 && card[2] <= vp.width, "inside the screen: {card:?} in {vp:?}");
    assert!(card[3] - card[1] > 0.8 * vp.height, "most of its height: {card:?} in {vp:?}");
}

/// Round M, item 4: the page's footer names each key's own action once.
/// Esc is Stop watching; Enter is named only while it does something
/// else (Watch, offered and focused): never "Stop watching" twice, as it
/// was while Stop watching was the one button.
#[test]
fn the_audience_page_s_footer_names_each_action_once() {
    let clock = ManualClock::new(5_000);
    let server = room(&clock, live_audience());
    let mut alice = client(&server, person(&server, "alice"), "alice", 5_000);
    let carol = Rc::new(RefCell::new(arrive(&server, "carol", AUDIENCE_LINK, 5_000).unwrap()));
    let (env, mut h) = page(&carol, [1280.0, 720.0]);
    run(&clock, &server, &mut [&mut alice], &mut [&mut carol.borrow_mut()], 1_000);
    h.run(0.3);
    let hints = env.state.borrow().hints.clone();
    assert_eq!(hints, ["ESC Stop watching"], "one button, one hint");
}

/// Round M, item 4: a controller scrolls the page's parts. Where the
/// players do not all fit, Down and Up (the D-pad, as the keys) scroll
/// them a line from the page's buttons while it has no tabs; once it has
/// them, from the tab focused, Down past the last line going on to the
/// buttons.
#[test]
fn a_controller_scrolls_the_audience_page_s_parts() {
    use crate::nav::MenuAction;
    let clock = ManualClock::new(5_000);
    let server = room(&clock, live_audience());
    let mut alice = client(&server, person(&server, "alice"), "alice", 5_000);
    for i in 0..60 {
        server
            .borrow_mut()
            .join(Identity::new(format!("dev:p{i:02}")), &format!("p{i:02}"), Intent::Play)
            .unwrap();
    }
    let carol = Rc::new(RefCell::new(arrive(&server, "carol", AUDIENCE_LINK, 5_000).unwrap()));
    let (_, mut h) = page(&carol, [1280.0, 720.0]);
    run(&clock, &server, &mut [&mut alice], &mut [&mut carol.borrow_mut()], 1_000);
    h.run(0.3);
    // The first line shown (the lines are named by their place in the list).
    let first = |h: &crate::testing::Harness| {
        (0..70)
            .find(|k| h.ui.actor(&format!("Audience.Player{k}.Name")).is_some())
            .expect("a player's line")
    };
    let top = first(&h);
    assert!(h.ui.actor("Audience.Down").is_some(), "more players than fit");
    h.pad(MenuAction::Down, true);
    h.pad(MenuAction::Down, false);
    h.run(0.1);
    let down = first(&h);
    assert_ne!(down, top, "the D-pad's Down scrolls the players");
    assert_eq!(h.ui.focused(), Some("Item.back"), "the focus stays on the button");
    h.pad(MenuAction::Up, true);
    h.pad(MenuAction::Up, false);
    h.run(0.1);
    assert_eq!(first(&h), top, "and Up scrolls back");
}
