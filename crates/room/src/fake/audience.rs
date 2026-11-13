//! The audience in the fake room (`crate::audience`): the room's audience
//! side ([`AudienceHub`]) beside its live side, relays ([`Relay`]) as the
//! server's relay objects run them (each a socket up to the room, a hop of
//! a few milliseconds each way), and seats on them through
//! [`FakeAudienceSocket`]s with a latency each, in their thousands.
//!
//! - [`FakeServer::audience_join`]: the server's audience join (`POST
//!   /rooms/<id>/audience`): the identity's home relay first, then the
//!   next ones while full; a ticket for the seat's socket.
//! - A seat's keep-alive is answered by the relay's host itself, never
//!   reaching the relay (Durable Objects' auto-response).
//! - [`FakeServer::restart_relay`]: a relay's object loses its memory, its
//!   seats' sockets kept (hibernation): it opens a new socket up, says
//!   which frame it has, and the room sends what it lacks.
//! - The count the room says to its members arrives with whatever was
//!   last on its way to them (no latency of its own drawn), so the
//!   audience never moves a member's own messages.

use super::{FakeServer, Frame, Latency};
use crate::audience::hub::{AudienceHub, HubOut, HubSend};
use crate::audience::relay::{Relay, RelayOut, RelaySend, SeatId, Target, identity_mark};
use crate::audience::{AudienceRefusal, AudienceRules, KEEPALIVE, KEEPALIVE_ANSWER};
use crate::ids::{Millis, ParticipantId, RoomId};
use crate::live::{ServerLive, ServerLiveEnvelope};
use crate::online::{Transport, TransportEvent};
use std::collections::{BTreeMap, VecDeque};

/// What travels between the room and a relay.
enum Hop {
    Text(String),
    Binary(Vec<u8>),
}

struct FakeRelay {
    relay: Relay,
    /// Its socket up to the room is open.
    up: bool,
    /// From the room, with when each arrives.
    down: VecDeque<(u64, Hop)>,
    /// To the room, with when each arrives.
    upq: VecDeque<(u64, String)>,
}

struct SeatConn {
    relay: u32,
    seat: SeatId,
    mark: u64,
    open: bool,
    closing: bool,
    queue: VecDeque<(u64, Frame)>,
    latency: Option<Latency>,
    last_down: u64,
    last_up: u64,
}

pub(super) struct FakeAudience {
    pub(super) hub: AudienceHub,
    rules: AudienceRules,
    key: String,
    relays: BTreeMap<u32, FakeRelay>,
    seats: BTreeMap<u64, SeatConn>,
    /// Each seat's socket by its relay and seat.
    by_seat: BTreeMap<(u32, SeatId), u64>,
    /// What seats sent, on its way: by when it arrives (and in order).
    uplink: BTreeMap<(u64, u64), (u64, String)>,
    next_up: u64,
    next_conn: u64,
    tickets: BTreeMap<String, (u32, SeatId, u64, Option<Latency>)>,
    next_ticket: u64,
    hop_ms: u64,
}

impl FakeAudience {
    pub(super) fn new(rules: AudienceRules) -> Self {
        let key = "audience-key".to_string();
        Self {
            hub: AudienceHub::new(rules, key.clone()),
            rules,
            key,
            relays: BTreeMap::new(),
            seats: BTreeMap::new(),
            by_seat: BTreeMap::new(),
            uplink: BTreeMap::new(),
            next_up: 0,
            next_conn: 1,
            tickets: BTreeMap::new(),
            next_ticket: 1,
            hop_ms: 2,
        }
    }
}

/// A count of what the fake's audience did (for tests and studies).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AudienceStats {
    pub relays: u32,
    pub seats: u32,
    pub frames: u32,
}

impl FakeServer {
    fn audience_mut(&mut self) -> Option<&mut FakeAudience> {
        self.audience.as_mut()
    }

    /// The audience's rules from now on (a test's small bounds): every
    /// relay and seat is dropped.
    pub fn set_audience_rules(&mut self, rules: AudienceRules) {
        self.audience = Some(FakeAudience::new(rules));
    }

    /// The one-way time between the room and a relay (2 ms by default:
    /// both objects in one place).
    pub fn set_relay_hop(&mut self, ms: u64) {
        if let Some(a) = self.audience_mut() {
            a.hop_ms = ms;
        }
    }

    /// The room's audience link's key (what an audience join presents).
    pub fn audience_key(&self) -> Option<String> {
        self.audience.as_ref().map(|a| a.key.clone())
    }

    /// The room's audience side.
    pub fn audience_hub(&self) -> Option<&AudienceHub> {
        self.audience.as_ref().map(|a| &a.hub)
    }

    /// Relay `k`, if it was started.
    pub fn relay(&self, k: u32) -> Option<&Relay> {
        self.audience.as_ref()?.relays.get(&k).map(|r| &r.relay)
    }

    pub fn audience_stats(&self) -> AudienceStats {
        self.audience
            .as_ref()
            .map_or(AudienceStats::default(), |a| AudienceStats {
                relays: a.relays.len() as u32,
                seats: a.relays.values().map(|r| r.relay.seats()).sum(),
                frames: a.hub.frames(),
            })
    }

    /// The server's audience join: a seat for `identity` by a link whose
    /// key is `key`, on its home relay or the next with room (each relay
    /// started, and its socket up opened and answered, as the server's
    /// join waits for); a ticket for the seat's socket, which arrives with
    /// `latency`.
    pub fn audience_join(
        &mut self,
        identity: &str,
        key: &str,
        latency: Option<Latency>,
    ) -> Result<String, AudienceRefusal> {
        if self.state().closed.is_some() {
            return Err(AudienceRefusal::RoomClosed);
        }
        let Some(rules) = self.audience.as_ref().map(|a| a.rules) else {
            return Err(AudienceRefusal::NoAudience);
        };
        let now = self.now();
        let mut last = AudienceRefusal::Full;
        for k in rules.relay_order(identity) {
            self.start_relay(k);
            let a = self.audience.as_mut().expect("checked");
            let r = a.relays.get_mut(&k).expect("started");
            // An identity's seats are counted where they are: on its home
            // relay, and on the next ones only while that one is full.
            match r.relay.admit(identity, key, now) {
                Ok(seat) => {
                    let t = format!("aud-{}", a.next_ticket);
                    a.next_ticket += 1;
                    a.tickets
                        .insert(t.clone(), (k, seat, identity_mark(identity), latency));
                    return Ok(t);
                }
                Err(AudienceRefusal::Full) => last = AudienceRefusal::Full,
                Err(e) => return Err(e),
            }
        }
        Err(last)
    }

    /// The relay a ticket given and not taken yet is for.
    pub fn audience_ticket_relay(&self, ticket: &str) -> Option<u32> {
        self.audience.as_ref()?.tickets.get(ticket).map(|t| t.0)
    }

    /// Relay `k` running, its socket up open and its hello answered.
    fn start_relay(&mut self, k: u32) {
        let now = self.now();
        let Some(mut a) = self.audience.take() else {
            return;
        };
        let rules = a.rules;
        let r = a.relays.entry(k).or_insert_with(|| FakeRelay {
            relay: Relay::new(rules, k),
            up: false,
            down: VecDeque::new(),
            upq: VecDeque::new(),
        });
        if !r.up || !r.relay.ready() {
            r.up = true;
            let out = r.relay.upstream_opened();
            for text in out.upstream {
                let ans = a.hub.relay_text(
                    Some(k),
                    &text,
                    self.authority.state(),
                    &self.room_name,
                    self.live.as_ref(),
                    Millis(now),
                );
                // At once: the join waits for the relay to be ready.
                for (to, send) in ans.to_relay {
                    if to == k {
                        let r = a.relays.get_mut(&k).expect("started");
                        let o = match send {
                            HubSend::Text(t) => r.relay.from_room_text(&t, now),
                            HubSend::Binary(b) => r.relay.from_room_binary(&b, now),
                        };
                        self.relay_out(&mut a, k, o);
                    }
                }
            }
        }
        self.audience = Some(a);
    }

    /// Relay `k`'s object restarts: its memory is lost, its seats' sockets
    /// stay open (their seats in their attachments), its socket up to the
    /// room is gone; it opens another on the next pump.
    pub fn restart_relay(&mut self, k: u32) {
        let Some(a) = self.audience.as_mut() else {
            return;
        };
        let seats: Vec<(SeatId, u64)> = a
            .seats
            .values()
            .filter(|s| s.open && !s.closing && s.relay == k)
            .map(|s| (s.seat, s.mark))
            .collect();
        let rules = a.rules;
        if let Some(r) = a.relays.get_mut(&k) {
            r.relay = Relay::restore(rules, k, seats);
            r.up = false;
            r.down.clear();
            r.upq.clear();
        }
        a.hub.relay_gone(k);
    }

    /// Open audience sockets.
    pub fn audience_sockets(&self) -> usize {
        self.audience
            .as_ref()
            .map_or(0, |a| a.seats.values().filter(|s| s.open).count())
    }

    fn audience_connect(&mut self, ticket: &str) -> Option<u64> {
        let a = self.audience.as_mut()?;
        let (relay, seat, mark, latency) = a.tickets.remove(ticket)?;
        let conn = a.next_conn;
        a.next_conn += 1;
        a.seats.insert(
            conn,
            SeatConn {
                relay,
                seat,
                mark,
                open: true,
                closing: false,
                queue: VecDeque::new(),
                latency,
                last_down: 0,
                last_up: 0,
            },
        );
        a.by_seat.insert((relay, seat), conn);
        Some(conn)
    }

    fn audience_send(&mut self, conn: u64, text: String) {
        let now = self.now();
        let Some(a) = self.audience.as_mut() else {
            return;
        };
        let Some(s) = a.seats.get_mut(&conn).filter(|s| s.open) else {
            return;
        };
        let mut at = now;
        if let Some(l) = s.latency.as_mut() {
            at = now + l.draw(l.up_ms, l.up_jitter_ms);
        }
        at = at.max(s.last_up);
        s.last_up = at;
        a.next_up += 1;
        a.uplink.insert((at, a.next_up), (conn, text));
    }

    fn audience_closed_by_client(&mut self, conn: u64) {
        let Some(a) = self.audience.as_mut() else {
            return;
        };
        if let Some(s) = a.seats.get_mut(&conn)
            && s.open
        {
            s.open = false;
            s.queue.clear();
            let (relay, seat) = (s.relay, s.seat);
            if let Some(r) = a.relays.get_mut(&relay) {
                r.relay.seat_gone(seat);
            }
        }
    }

    /// The seat's messages due by now (the room's time is the test's to
    /// pump: thousands of seats do not each run the room).
    fn audience_deliver(&mut self, conn: u64) -> Vec<TransportEvent> {
        let now = self.now();
        let mut out = Vec::new();
        let Some(a) = self.audience.as_mut() else {
            return out;
        };
        let mut gone = None;
        if let Some(s) = a.seats.get_mut(&conn) {
            if !s.open {
                return out;
            }
            while let Some((at, _)) = s.queue.front() {
                if *at > now {
                    break;
                }
                let (_, f) = s.queue.pop_front().expect("checked");
                match f {
                    Frame::Text(t) => out.push(TransportEvent::Text(t)),
                    Frame::Binary(b) => out.push(TransportEvent::Binary(b)),
                    Frame::Close(code) => {
                        out.push(TransportEvent::Closed { code });
                        s.open = false;
                        s.queue.clear();
                        gone = Some((s.relay, s.seat));
                        break;
                    }
                }
            }
        }
        if let Some((relay, seat)) = gone
            && let Some(r) = a.relays.get_mut(&relay)
        {
            r.relay.seat_gone(seat);
        }
        out
    }

    /// The audience's time: seats' messages read, the room's audience side
    /// and every relay run, what is due sent on.
    pub(super) fn audience_pump(&mut self) {
        let Some(mut a) = self.audience.take() else {
            return;
        };
        let now = self.now();
        // Seats' messages to their relays.
        let mut arrived: Vec<(u64, String)> = Vec::new();
        while a
            .uplink
            .first_key_value()
            .is_some_and(|((at, _), _)| *at <= now)
        {
            arrived.push(a.uplink.pop_first().expect("checked").1);
        }
        for (conn, text) in arrived {
            let Some((relay, seat)) = a
                .seats
                .get(&conn)
                .filter(|s| s.open && !s.closing)
                .map(|s| (s.relay, s.seat))
            else {
                continue;
            };
            if text == KEEPALIVE {
                // The host's auto-response: the relay is not woken.
                Self::seat_push(&mut a, conn, Frame::Text(KEEPALIVE_ANSWER.into()), now);
                continue;
            }
            if let Some(r) = a.relays.get_mut(&relay) {
                let o = r.relay.seat_text(seat, &text);
                self.relay_out(&mut a, relay, o);
            }
        }
        // Relays whose socket up is gone open another.
        let down: Vec<u32> = a
            .relays
            .iter()
            .filter(|(_, r)| !r.up)
            .map(|(k, _)| *k)
            .collect();
        for k in down {
            let r = a.relays.get_mut(&k).expect("listed");
            r.up = true;
            let o = r.relay.upstream_opened();
            self.relay_out(&mut a, k, o);
        }
        // Relays' messages to the room.
        let mut up: Vec<(u32, String)> = Vec::new();
        for (k, r) in a.relays.iter_mut() {
            while r.upq.front().is_some_and(|(at, _)| *at <= now) {
                up.push((*k, r.upq.pop_front().expect("checked").1));
            }
        }
        for (k, text) in up {
            let out = a.hub.relay_text(
                Some(k),
                &text,
                self.authority.state(),
                &self.room_name,
                self.live.as_ref(),
                Millis(now),
            );
            self.hub_out(&mut a, out);
        }
        // The room's audience side.
        let out = a.hub.tick(
            self.authority.state(),
            &self.room_name,
            self.live.as_ref(),
            Millis(now),
        );
        self.hub_out(&mut a, out);
        // The room's messages at the relays.
        let keys: Vec<u32> = a.relays.keys().copied().collect();
        for k in keys {
            loop {
                let r = a.relays.get_mut(&k).expect("listed");
                if !r.down.front().is_some_and(|(at, _)| *at <= now) {
                    break;
                }
                let (_, hop) = r.down.pop_front().expect("checked");
                let o = match hop {
                    Hop::Text(t) => r.relay.from_room_text(&t, now),
                    Hop::Binary(b) => r.relay.from_room_binary(&b, now),
                };
                self.relay_out(&mut a, k, o);
            }
            let r = a.relays.get_mut(&k).expect("listed");
            let o = r.relay.tick(now);
            self.relay_out(&mut a, k, o);
        }
        self.audience = Some(a);
    }

    fn hub_out(&mut self, a: &mut FakeAudience, out: HubOut) {
        let now = self.now();
        let hop = a.hop_ms;
        for (k, send) in out.to_relay {
            if let Some(r) = a.relays.get_mut(&k).filter(|r| r.up) {
                r.down.push_back((now + hop, hop_of(send)));
            }
        }
        for send in out.to_relays {
            for r in a.relays.values_mut().filter(|r| r.up) {
                r.down.push_back((
                    now + hop,
                    match &send {
                        HubSend::Text(t) => Hop::Text(t.clone()),
                        HubSend::Binary(b) => Hop::Binary(b.clone()),
                    },
                ));
            }
        }
        if let Some(msg) = out.to_members {
            self.tell_members(msg);
        }
    }

    /// The count to every member's socket that said live hello, arriving
    /// with what was last on its way (no latency drawn).
    fn tell_members(&mut self, msg: ServerLive) {
        let Ok(text) = serde_json::to_string(&ServerLiveEnvelope::new(msg)) else {
            return;
        };
        let now = self.now();
        for c in self.conns.values_mut().filter(|c| c.open && c.live) {
            let at = now.max(c.last_down);
            c.queue.push_back((at, Frame::Text(text.clone())));
        }
    }

    fn relay_out(&mut self, a: &mut FakeAudience, k: u32, out: RelayOut) {
        let now = self.now();
        let hop = a.hop_ms;
        if let Some(r) = a.relays.get_mut(&k) {
            for text in out.upstream {
                r.upq.push_back((now + hop, text));
            }
        }
        let conns_of = |a: &FakeAudience, t: Target| -> Vec<u64> {
            let seats: Vec<SeatId> = match t {
                Target::All => a
                    .relays
                    .get(&k)
                    .map(|r| r.relay.hello_seats().collect())
                    .unwrap_or_default(),
                Target::Seat(seat) => vec![seat],
            };
            seats
                .into_iter()
                .filter_map(|seat| a.by_seat.get(&(k, seat)).copied())
                .filter(|c| a.seats.get(c).is_some_and(|s| s.open && !s.closing))
                .collect()
        };
        for (t, send) in out.to_seats {
            for c in conns_of(a, t) {
                let f = match &send {
                    RelaySend::Text(t) => Frame::Text(t.clone()),
                    RelaySend::Binary(b) => Frame::Binary(b.clone()),
                };
                Self::seat_push(a, c, f, now);
            }
        }
        for (t, code) in out.close {
            for c in conns_of(a, t) {
                Self::seat_push(a, c, Frame::Close(code), now);
                if let Some(s) = a.seats.get_mut(&c) {
                    s.closing = true;
                }
            }
        }
    }

    fn seat_push(a: &mut FakeAudience, conn: u64, f: Frame, now: u64) {
        let Some(s) = a.seats.get_mut(&conn).filter(|s| s.open) else {
            return;
        };
        let mut at = now;
        if let Some(l) = s.latency.as_mut() {
            at = now + l.draw(l.down_ms, l.down_jitter_ms);
        }
        at = at.max(s.last_down);
        s.last_down = at;
        s.queue.push_back((at, f));
    }

    /// A member's `ClientLive::Audience` (the owner opening or closing the
    /// audience).
    pub(super) fn audience_open(&mut self, from: ParticipantId, open: bool) {
        let Some(mut a) = self.audience.take() else {
            return;
        };
        let out = a.hub.set_open(self.authority.state(), from, open);
        self.hub_out(&mut a, out);
        self.audience = Some(a);
    }

    /// A frame the room's live side broadcast to its members.
    pub(super) fn audience_member_frame(&mut self, bytes: &[u8]) {
        if let Some(a) = self.audience.as_mut() {
            a.hub.member_frame(bytes);
        }
    }
}

fn hop_of(send: HubSend) -> Hop {
    match send {
        HubSend::Text(t) => Hop::Text(t),
        HubSend::Binary(b) => Hop::Binary(b),
    }
}

/// An audience seat's socket to a [`FakeServer`] (a `Transport`, as the
/// shells' sockets are): opened with a ticket from
/// [`FakeServer::audience_join`].
pub struct FakeAudienceSocket {
    server: super::SharedFake,
    conn: Option<u64>,
    events: Vec<TransportEvent>,
}

impl FakeAudienceSocket {
    pub fn new(server: super::SharedFake) -> Self {
        Self {
            server,
            conn: None,
            events: Vec::new(),
        }
    }
}

impl Transport for FakeAudienceSocket {
    fn open(&mut self, _room: &RoomId, ticket: &str) {
        self.close();
        match self.server.borrow_mut().audience_connect(ticket) {
            Some(c) => {
                self.conn = Some(c);
                self.events.push(TransportEvent::Opened);
            }
            None => self.events.push(TransportEvent::Closed { code: 1006 }),
        }
    }

    fn send(&mut self, text: String) {
        if let Some(c) = self.conn {
            self.server.borrow_mut().audience_send(c, text);
        }
    }

    fn send_binary(&mut self, _bytes: Vec<u8>) {
        // A seat sends nothing binary; a relay would close it (4400).
        if let Some(c) = self.conn.take() {
            self.server.borrow_mut().audience_closed_by_client(c);
            self.events.push(TransportEvent::Closed {
                code: crate::online::close::BAD_MESSAGE,
            });
        }
    }

    fn binary(&self) -> bool {
        true
    }

    fn close(&mut self) {
        if let Some(c) = self.conn.take() {
            self.server.borrow_mut().audience_closed_by_client(c);
        }
    }

    fn poll(&mut self) -> Vec<TransportEvent> {
        let mut out = std::mem::take(&mut self.events);
        if let Some(c) = self.conn {
            let got = self.server.borrow_mut().audience_deliver(c);
            if got
                .iter()
                .any(|e| matches!(e, TransportEvent::Closed { .. }))
            {
                self.conn = None;
            }
            out.extend(got);
        }
        out
    }
}
