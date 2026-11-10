//! A stand-in for the private server's room, for tests: the same reducer
//! in process ([`LocalAuthority`], which keeps each client's last sequence
//! number as the server does), reached through [`FakeSocket`]s that carry
//! every frame as JSON text both ways, as a WebSocket does. Faults can be
//! put in the way of what the room sends a person ([`Fault`]): delay,
//! duplicate, drop, reorder, an old revision sent again, and the socket
//! closed with a code (4401 to ask for signing in again).
//!
//! It is the room as `RoomDO` keeps it, cut to what a client sees: a
//! socket opens with a one-use ticket, its member Connected and the room
//! sent at once; each accepted revision's events to every socket; the
//! answer (and, after a stale refusal, the room) to the sender; a
//! [`Resync`](crate::Resync) answered with the room; an accepted `Leave`
//! closes the socket with 4000; a socket gone makes its member
//! Reconnecting. Not the server: no HTTP, no invites, no grace period.

use crate::authority::{Clock, LocalAuthority, ManualClock, RoomAuthority};
use crate::command::{Command, Event, Rejection};
use crate::ids::{ClientId, Identity, ParticipantId, RoomId};
use crate::member::{Connection, Intent};
use crate::online::{Transport, TransportEvent, close};
use crate::protocol::{ClientFrame, ServerEnvelope, ServerMessage};
use crate::rules::{ContentSource, MatchRules, RoomPolicy};
use crate::state::RoomState;
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

/// Something done to the frames the room sends one person.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// The next frame arrives this much later.
    Delay(u64),
    /// The next frame arrives twice.
    Duplicate,
    /// The next frame never arrives.
    Drop,
    /// The next two frames arrive in the other order.
    Reorder,
    /// After the next frame, the last events frame before it again.
    Stale,
    /// The socket is closed with this code now.
    Close(u16),
}

enum Frame {
    Text(String),
    Close(u16),
}

struct Conn {
    participant: ParticipantId,
    /// Frames on their way, each with when it arrives.
    queue: VecDeque<(u64, Frame)>,
    faults: VecDeque<Fault>,
    /// The last events frame sent, for [`Fault::Stale`].
    last_events: Option<String>,
    /// A frame held back to swap with the next ([`Fault::Reorder`]).
    held: Option<String>,
    open: bool,
}

/// The fake room. Share it ([`FakeServer::shared`]) between the sockets.
pub struct FakeServer {
    authority: LocalAuthority<ManualClock>,
    clock: ManualClock,
    conns: BTreeMap<u64, Conn>,
    next_conn: u64,
    next_ticket: u64,
    tickets: BTreeMap<String, ParticipantId>,
    /// Faults for the next socket a person opens, by participant.
    pending_faults: BTreeMap<ParticipantId, Vec<Fault>>,
}

pub type SharedFake = Rc<RefCell<FakeServer>>;

impl FakeServer {
    /// A hosted room (as the server makes one: `MatchRules::hosted`,
    /// content shared) owned by `owner`, not connected yet.
    pub fn hosted(
        room: &str,
        owner: Identity,
        name: &str,
        policy: RoomPolicy,
        clock: ManualClock,
    ) -> Self {
        let state = RoomState::new(
            RoomId::new(room),
            owner,
            name.into(),
            policy,
            MatchRules::hosted(),
            ContentSource::Shared,
        );
        let mut fake = Self::with_state(state, clock);
        let _ = fake.authority(Command::SetConnection {
            participant: ParticipantId(1),
            connection: Connection::Joining,
        });
        fake
    }

    pub fn with_state(state: RoomState, clock: ManualClock) -> Self {
        let mut authority = LocalAuthority::new(state, clock.clone());
        // The constructor's snapshot is no one's.
        let _ = authority.poll();
        Self {
            authority,
            clock,
            conns: BTreeMap::new(),
            next_conn: 1,
            next_ticket: 1,
            tickets: BTreeMap::new(),
            pending_faults: BTreeMap::new(),
        }
    }

    pub fn shared(self) -> SharedFake {
        Rc::new(RefCell::new(self))
    }

    pub fn state(&self) -> &RoomState {
        self.authority.state()
    }

    pub fn now(&self) -> u64 {
        self.clock.now().0
    }

    /// `POST /rooms/<id>/join`: admit (or restore) a person, Joining until
    /// their socket opens; a present member is answered as they are.
    pub fn join(
        &mut self,
        identity: Identity,
        name: &str,
        intent: Intent,
    ) -> Result<ParticipantId, Rejection> {
        if let Some(m) = self.state().member_by_identity(&identity)
            && !m.left
        {
            return Ok(m.id);
        }
        self.authority(Command::Join {
            identity: identity.clone(),
            name: name.into(),
            admin: false,
            intent,
        })?;
        let id = self
            .state()
            .member_by_identity(&identity)
            .map(|m| m.id)
            .ok_or(Rejection::NotAMember)?;
        let _ = self.authority(Command::SetConnection {
            participant: id,
            connection: Connection::Joining,
        });
        Ok(id)
    }

    /// `POST /rooms/<id>/ticket`: a one-use ticket for a present member.
    pub fn ticket(&mut self, identity: &Identity) -> Result<String, Rejection> {
        let id = self
            .state()
            .member_by_identity(identity)
            .filter(|m| !m.left)
            .map(|m| m.id)
            .ok_or(Rejection::NotAMember)?;
        let t = format!("ticket-{}", self.next_ticket);
        self.next_ticket += 1;
        self.tickets.insert(t.clone(), id);
        Ok(t)
    }

    /// The server's own command (content status, a confirmed result).
    pub fn authority(&mut self, command: Command) -> Result<Vec<Event>, Rejection> {
        let out = self.authority.authority(command);
        self.route(None);
        out
    }

    /// Faults for what `participant`'s socket gets, from now on (or, with
    /// none open, from the next one).
    pub fn fault(&mut self, participant: ParticipantId, fault: Fault) {
        let now = self.now();
        match self
            .conns
            .values_mut()
            .find(|c| c.open && c.participant == participant)
        {
            Some(conn) => match fault {
                Fault::Close(code) => {
                    conn.queue.push_back((now, Frame::Close(code)));
                }
                f => conn.faults.push_back(f),
            },
            None => self
                .pending_faults
                .entry(participant)
                .or_default()
                .push(fault),
        }
    }

    /// The network drops `participant`'s socket (no close frame: 1006).
    pub fn drop_socket(&mut self, participant: ParticipantId) {
        self.fault(participant, Fault::Close(1006));
    }

    /// Open sockets.
    pub fn sockets(&self) -> usize {
        self.conns.values().filter(|c| c.open).count()
    }

    fn connect(&mut self, ticket: &str) -> Option<u64> {
        let participant = self.tickets.remove(ticket)?;
        let conn = self.next_conn;
        self.next_conn += 1;
        let faults = self.pending_faults.remove(&participant).unwrap_or_default();
        let mut c = Conn {
            participant,
            queue: VecDeque::new(),
            faults: VecDeque::new(),
            last_events: None,
            held: None,
            open: true,
        };
        let now = self.now();
        for f in faults {
            match f {
                Fault::Close(code) => c.queue.push_back((now, Frame::Close(code))),
                f => c.faults.push_back(f),
            }
        }
        // The others hear of it; the new socket gets the room as it is.
        let _ = self.authority.authority(Command::SetConnection {
            participant,
            connection: Connection::Connected,
        });
        self.route(None);
        self.conns.insert(conn, c);
        let snapshot = ServerEnvelope::new(
            self.state().revision,
            ServerMessage::Snapshot {
                room: Box::new(self.state().clone()),
            },
        );
        self.push(conn, &snapshot);
        Some(conn)
    }

    fn receive(&mut self, conn: u64, text: &str) {
        let Some(participant) = self
            .conns
            .get(&conn)
            .filter(|c| c.open)
            .map(|c| c.participant)
        else {
            return;
        };
        let Ok(frame) = serde_json::from_str::<ClientFrame>(text) else {
            self.close_conn(conn, close::BAD_MESSAGE);
            return;
        };
        match frame {
            ClientFrame::Resync(_) => {
                let snapshot = ServerEnvelope::new(
                    self.state().revision,
                    ServerMessage::Snapshot {
                        room: Box::new(self.state().clone()),
                    },
                );
                self.push(conn, &snapshot);
            }
            ClientFrame::Command(envelope) => {
                let client = envelope.client.clone();
                let seq = envelope.seq;
                let leave = envelope.cmd == Command::Leave;
                if self.authority.bind(client.clone(), participant).is_err() {
                    let reply = ServerEnvelope::new(
                        self.state().revision,
                        ServerMessage::Reject {
                            client,
                            seq,
                            rejection: Rejection::UnknownClient,
                        },
                    );
                    self.push(conn, &reply);
                    return;
                }
                self.authority.submit(envelope);
                let accepted = self.route(Some((conn, &client, seq)));
                if leave && accepted {
                    self.close_conn(conn, close::LEFT);
                }
            }
        }
    }

    /// What the reducer sent out, to whom it goes: events to every socket,
    /// an answer and a snapshot to the sender. Whether the sender's
    /// command (if any) was accepted.
    fn route(&mut self, sender: Option<(u64, &ClientId, u64)>) -> bool {
        let mut accepted = false;
        for envelope in self.authority.poll() {
            match &envelope.msg {
                ServerMessage::Events { .. } => {
                    let open: Vec<u64> = self
                        .conns
                        .iter()
                        .filter(|(_, c)| c.open)
                        .map(|(k, _)| *k)
                        .collect();
                    for k in open {
                        self.push(k, &envelope);
                    }
                }
                ServerMessage::Ack { client, seq } => {
                    if let Some((conn, c, s)) = sender
                        && c == client
                        && s == *seq
                    {
                        accepted = true;
                        self.push(conn, &envelope);
                    }
                }
                ServerMessage::Reject { client, seq, .. } => {
                    if let Some((conn, c, s)) = sender
                        && c == client
                        && s == *seq
                    {
                        self.push(conn, &envelope);
                    }
                }
                ServerMessage::Snapshot { .. } => {
                    if let Some((conn, ..)) = sender {
                        self.push(conn, &envelope);
                    }
                }
            }
        }
        accepted
    }

    fn push(&mut self, conn: u64, envelope: &ServerEnvelope) {
        let Ok(text) = serde_json::to_string(envelope) else {
            return;
        };
        let now = self.now();
        let events = matches!(envelope.msg, ServerMessage::Events { .. });
        let Some(c) = self.conns.get_mut(&conn).filter(|c| c.open) else {
            return;
        };
        let mut at = now;
        let mut copies = vec![text.clone()];
        let mut stale = None;
        if let Some(held) = c.held.take() {
            // The frame held back goes after this one.
            copies.push(held);
        } else {
            match c.faults.pop_front() {
                Some(Fault::Delay(ms)) => at = now + ms,
                Some(Fault::Duplicate) => copies.push(text.clone()),
                Some(Fault::Drop) => copies.clear(),
                Some(Fault::Reorder) => {
                    c.held = Some(text.clone());
                    copies.clear();
                }
                Some(Fault::Stale) => stale = c.last_events.clone(),
                Some(Fault::Close(_)) | None => {}
            }
        }
        if events {
            c.last_events = Some(text);
        }
        for t in copies.into_iter().chain(stale) {
            c.queue.push_back((at, Frame::Text(t)));
        }
    }

    fn close_conn(&mut self, conn: u64, code: u16) {
        let now = self.now();
        if let Some(c) = self.conns.get_mut(&conn) {
            c.queue.push_back((now, Frame::Close(code)));
        }
    }

    /// The socket's frames due by now (time passes first: deadlines and
    /// countdowns, to every socket).
    fn deliver(&mut self, conn: u64) -> Vec<TransportEvent> {
        self.route(None);
        let now = self.now();
        let mut out = Vec::new();
        let mut closed = None;
        if let Some(c) = self.conns.get_mut(&conn) {
            if !c.open {
                return out;
            }
            // In order of arrival; a delayed frame holds back the rest.
            while let Some((at, _)) = c.queue.front() {
                if *at > now {
                    break;
                }
                let (_, frame) = c.queue.pop_front().expect("checked");
                match frame {
                    Frame::Text(t) => out.push(TransportEvent::Text(t)),
                    Frame::Close(code) => {
                        out.push(TransportEvent::Closed { code });
                        closed = Some(c.participant);
                        c.open = false;
                        c.queue.clear();
                        break;
                    }
                }
            }
        }
        if let Some(p) = closed {
            self.gone(p);
        }
        out
    }

    /// A socket closed: its member Reconnecting when it was their last.
    fn gone(&mut self, participant: ParticipantId) {
        let others = self
            .conns
            .values()
            .any(|c| c.open && c.participant == participant);
        let present = self
            .state()
            .member(participant)
            .is_some_and(|m| !m.left && m.connection == Connection::Connected);
        if !others && present {
            let _ = self.authority.authority(Command::SetConnection {
                participant,
                connection: Connection::Reconnecting,
            });
            self.route(None);
        }
    }

    fn client_closed(&mut self, conn: u64) {
        let p = match self.conns.get_mut(&conn) {
            Some(c) if c.open => {
                c.open = false;
                c.queue.clear();
                c.participant
            }
            _ => return,
        };
        self.gone(p);
    }
}

/// A client's socket to a [`FakeServer`].
pub struct FakeSocket {
    server: SharedFake,
    conn: Option<u64>,
    events: Vec<TransportEvent>,
    /// Ticket opens that failed (a used or unknown ticket), as the
    /// server's `401` on the upgrade.
    pub refused: u32,
}

impl FakeSocket {
    pub fn new(server: SharedFake) -> Self {
        Self {
            server,
            conn: None,
            events: Vec::new(),
            refused: 0,
        }
    }
}

impl Transport for FakeSocket {
    fn open(&mut self, _room: &RoomId, ticket: &str) {
        self.close();
        let conn = self.server.borrow_mut().connect(ticket);
        match conn {
            Some(c) => {
                self.conn = Some(c);
                self.events.push(TransportEvent::Opened);
            }
            None => {
                self.refused += 1;
                self.events.push(TransportEvent::Closed { code: 1006 });
            }
        }
    }

    fn send(&mut self, text: String) {
        if let Some(conn) = self.conn {
            self.server.borrow_mut().receive(conn, &text);
        }
    }

    fn close(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.server.borrow_mut().client_closed(conn);
        }
    }

    fn poll(&mut self) -> Vec<TransportEvent> {
        let mut out = std::mem::take(&mut self.events);
        if let Some(conn) = self.conn {
            let got = self.server.borrow_mut().deliver(conn);
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
