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
use ddi_room::{Closure, Identity, Intent, RoomPolicy};

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
