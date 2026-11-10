//! The online room authority as a client holds it: the private server's
//! room (its `RoomDO`, running this crate's reducer) reached over a
//! WebSocket, behind the same [`RoomAuthority`] trait as the local
//! practice authority, so the room's screen and client code are one.
//!
//! What it adds to the envelope (`protocol`): the JSON codec on a
//! [`Transport`] each shell supplies (the web's `WebSocket`, desktop's
//! socket on a thread of its own), a [`Mirror`] whose gaps ask for the
//! room again ([`Resync`]), commands kept until answered and sent again
//! with their own `(client, seq)` after a reconnect (the server answers a
//! repeat without applying it twice), duplicate answers and old
//! revisions passed over, and the link's state ([`LinkState`]): joining,
//! connected, reconnecting with a fresh ticket each time, and the ends a
//! retry cannot mend (sign in again, the room closed, left).
//!
//! Sans-IO for tickets: a socket opens with a one-use ticket the shell
//! gets over HTTP (`POST /rooms/<id>/ticket`). The authority says when it
//! wants one ([`OnlineAuthority::wants_ticket`]) and is given the answer
//! ([`OnlineAuthority::ticket`]).

use crate::authority::{Clock, RoomAuthority};
use crate::command::Rejection;
use crate::ids::{Revision, RoomId};
use crate::member::Connection;
use crate::protocol::{ClientEnvelope, Mirror, MirrorError, Resync, ServerEnvelope, ServerMessage};
use crate::state::RoomState;
use serde::Serialize;

/// The socket's close codes the room uses (ddi-server `room.rs`).
pub mod close {
    /// The member left the room (their `Leave` accepted).
    pub const LEFT: u16 = 4000;
    /// A frame the server could not read.
    pub const BAD_MESSAGE: u16 = 4400;
    /// The session behind the socket expired: sign in again.
    pub const SIGN_IN_AGAIN: u16 = 4401;
    /// The room closed.
    pub const ROOM_CLOSED: u16 = 4410;
}

/// What a shell's socket reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransportEvent {
    Opened,
    Text(String),
    /// The socket closed or failed (`1006` where no close frame came).
    Closed {
        code: u16,
    },
}

/// A room's socket, as each shell opens it.
pub trait Transport {
    /// Open room `room`'s socket with `ticket` (the shell knows the
    /// server's address), closing any earlier one first.
    fn open(&mut self, room: &RoomId, ticket: &str);
    fn send(&mut self, text: String);
    fn close(&mut self);
    /// What happened since the last poll, oldest first: the socket open
    /// now only (an earlier one's late close is not reported).
    fn poll(&mut self) -> Vec<TransportEvent>;
}

impl<T: Transport + ?Sized> Transport for Box<T> {
    fn open(&mut self, room: &RoomId, ticket: &str) {
        (**self).open(room, ticket)
    }
    fn send(&mut self, text: String) {
        (**self).send(text)
    }
    fn close(&mut self) {
        (**self).close()
    }
    fn poll(&mut self) -> Vec<TransportEvent> {
        (**self).poll()
    }
}

/// Where the link to the room stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkState {
    /// Waiting for a ticket, the socket or its first snapshot.
    Joining,
    Connected,
    /// The socket dropped; trying again (the `attempt`th time).
    Reconnecting {
        attempt: u32,
    },
    /// The session ended (4401, or a ticket refused as signed out): only
    /// signing in again mends it, never a retry.
    SignInAgain,
    /// The room closed (4410, or the room gone when asking a ticket).
    Closed,
    /// Left the room (4000), or no member any more.
    Left,
}

impl LinkState {
    /// Whether the link is over until the person acts (signs in again,
    /// leaves for the home).
    pub fn ended(self) -> bool {
        matches!(self, Self::SignInAgain | Self::Closed | Self::Left)
    }
}

/// Why a ticket could not be had.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TicketProblem {
    /// `401`: the session expired or is gone.
    SignedOut,
    /// `403 not_a_member`.
    NotAMember,
    /// `404 room_unknown`, `410 room_closed`.
    RoomClosed,
    /// No answer, or another failure: retried.
    Unreachable,
}

/// How long a reconnect waits before each try.
///
/// PROVISIONAL (`RECONNECT_BACKOFF_V0`): no grace interval is agreed
/// (`docs/plans/multiplayer-ui.md`, "Connection-loss and return study");
/// the client keeps trying while the room shows the person Reconnecting,
/// then Disconnected after the server's own grace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReconnectPolicy {
    pub name: &'static str,
    /// The first retry's wait; each later one doubles it.
    pub first_ms: u64,
    pub max_ms: u64,
}

impl ReconnectPolicy {
    pub const RECONNECT_BACKOFF_V0: ReconnectPolicy = ReconnectPolicy {
        name: "reconnect-backoff-v0",
        first_ms: 500,
        max_ms: 8_000,
    };

    /// The wait before the `attempt`th try (from 1).
    pub fn wait_ms(&self, attempt: u32) -> u64 {
        let doubled = self
            .first_ms
            .saturating_mul(1u64 << attempt.saturating_sub(1).min(20));
        doubled.min(self.max_ms)
    }
}

/// How long a resync's answer may take before a later gap asks again.
const RESYNC_AGAIN_MS: u64 = 2_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ticket {
    /// Wanted from this time on (the reconnect's wait).
    Wanted { at: u64 },
    /// Asked for; the answer is the shell's.
    Asked,
    /// Given to the socket.
    Used,
}

/// The room's online authority, as a client holds it.
pub struct OnlineAuthority<T: Transport, C: Clock> {
    room: RoomId,
    transport: T,
    clock: C,
    policy: ReconnectPolicy,
    link: LinkState,
    mirror: Mirror,
    ticket: Ticket,
    /// The open socket gave its first snapshot.
    synced: bool,
    /// When a resync was asked for, not answered yet.
    resyncing: Option<u64>,
    /// Commands sent (or, while no socket is synced, waiting to be) and
    /// not answered, in order.
    unanswered: Vec<ClientEnvelope>,
    /// What the next poll hands on.
    inbox: Vec<ServerEnvelope>,
    /// The highest revision handed on, so an old one is not again.
    handed: Option<Revision>,
}

impl<T: Transport, C: Clock> OnlineAuthority<T, C> {
    /// A link to room `room`, joining: it wants a ticket at once.
    pub fn new(room: RoomId, transport: T, clock: C) -> Self {
        let now = clock.now().0;
        Self {
            room,
            transport,
            clock,
            policy: ReconnectPolicy::RECONNECT_BACKOFF_V0,
            link: LinkState::Joining,
            mirror: Mirror::new(),
            ticket: Ticket::Wanted { at: now },
            synced: false,
            resyncing: None,
            unanswered: Vec::new(),
            inbox: Vec::new(),
            handed: None,
        }
    }

    pub fn with_policy(mut self, policy: ReconnectPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn room_id(&self) -> &RoomId {
        &self.room
    }

    pub fn link(&self) -> LinkState {
        self.link
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    /// Commands not answered yet.
    pub fn unanswered(&self) -> usize {
        self.unanswered.len()
    }

    /// Whether a ticket should be asked for now.
    pub fn wants_ticket(&self) -> bool {
        !self.link.ended()
            && matches!(self.ticket, Ticket::Wanted { at } if self.clock.now().0 >= at)
    }

    /// When the next ticket is due, for a shell that sleeps.
    pub fn ticket_due(&self) -> Option<u64> {
        match self.ticket {
            Ticket::Wanted { at } if !self.link.ended() => Some(at),
            _ => None,
        }
    }

    /// The shell asked for a ticket.
    pub fn ticket_asked(&mut self) {
        self.ticket = Ticket::Asked;
    }

    /// The ticket asked for, or why there is none.
    pub fn ticket(&mut self, answer: Result<String, TicketProblem>) {
        if self.link.ended() {
            return;
        }
        match answer {
            Ok(ticket) => {
                self.ticket = Ticket::Used;
                self.synced = false;
                self.transport.open(&self.room, &ticket);
            }
            Err(TicketProblem::SignedOut) => self.end(LinkState::SignInAgain),
            Err(TicketProblem::NotAMember) => self.end(LinkState::Left),
            Err(TicketProblem::RoomClosed) => self.end(LinkState::Closed),
            Err(TicketProblem::Unreachable) => self.retry_later(),
        }
    }

    /// After signing in again: start over as a join (the same member is
    /// restored by the server; nothing here is assumed of the room).
    pub fn restart(&mut self) {
        self.transport.close();
        self.link = LinkState::Joining;
        self.synced = false;
        self.resyncing = None;
        self.ticket = Ticket::Wanted {
            at: self.clock.now().0,
        };
    }

    /// Close the socket for good (the room left behind).
    pub fn shut(&mut self) {
        self.transport.close();
        self.end(LinkState::Left);
    }

    fn end(&mut self, link: LinkState) {
        self.link = link;
        self.synced = false;
        self.ticket = Ticket::Used;
    }

    fn retry_later(&mut self) {
        let attempt = match self.link {
            LinkState::Reconnecting { attempt } => attempt + 1,
            _ => 1,
        };
        self.link = LinkState::Reconnecting { attempt };
        self.synced = false;
        self.resyncing = None;
        self.ticket = Ticket::Wanted {
            at: self.clock.now().0 + self.policy.wait_ms(attempt),
        };
    }

    fn send(&mut self, text: String) {
        self.transport.send(text);
    }

    fn send_envelope(&mut self, envelope: &ClientEnvelope) {
        if let Ok(text) = serde_json::to_string(envelope) {
            self.send(text);
        }
    }

    fn received(&mut self, text: &str) {
        let Ok(envelope) = serde_json::from_str::<ServerEnvelope>(text) else {
            // Not ours to read (a newer server's message): passed over.
            return;
        };
        match &envelope.msg {
            ServerMessage::Snapshot { .. } => {
                if self.mirror.receive(&envelope).is_err() {
                    return;
                }
                self.resyncing = None;
                if !self.synced {
                    // The socket's first: connected, and what was not
                    // answered goes again under its own sequence number.
                    self.synced = true;
                    self.link = LinkState::Connected;
                    for e in self.unanswered.clone() {
                        self.send_envelope(&e);
                    }
                }
                self.hand_on(envelope);
            }
            ServerMessage::Events { .. } => match self.mirror.receive(&envelope) {
                Ok(()) => {
                    if self.handed.is_none_or(|h| envelope.rev > h) {
                        self.hand_on(envelope);
                    }
                }
                Err(MirrorError::Gap { .. } | MirrorError::NoSnapshot) => {
                    // A revision missed (or the socket's first frame,
                    // the room, lost): the room again, once.
                    let now = self.clock.now().0;
                    if self.resyncing.is_none_or(|at| now >= at + RESYNC_AGAIN_MS) {
                        self.resyncing = Some(now);
                        if let Ok(text) = serde_json::to_string(&Resync::snapshot()) {
                            self.send(text);
                        }
                    }
                }
                Err(_) => {}
            },
            ServerMessage::Ack { client, seq } | ServerMessage::Reject { client, seq, .. } => {
                let Some(i) = self
                    .unanswered
                    .iter()
                    .position(|e| &e.client == client && e.seq == *seq)
                else {
                    // An answer already had (a duplicate frame).
                    return;
                };
                self.unanswered.remove(i);
                if let ServerMessage::Reject {
                    rejection: Rejection::OutOfOrder { last },
                    ..
                } = &envelope.msg
                    && *last >= *seq
                {
                    // Sent again after a reconnect, and a later one of
                    // ours already got through: it was taken (or refused)
                    // then; the room as it is tells which.
                    return;
                }
                self.inbox.push(envelope);
            }
        }
    }

    fn hand_on(&mut self, envelope: ServerEnvelope) {
        self.handed = Some(self.handed.map_or(envelope.rev, |h| h.max(envelope.rev)));
        self.inbox.push(envelope);
    }
}

impl<T: Transport, C: Clock> RoomAuthority for OnlineAuthority<T, C> {
    fn submit(&mut self, envelope: ClientEnvelope) {
        if self.synced {
            self.send_envelope(&envelope);
        }
        self.unanswered.push(envelope);
    }

    fn poll(&mut self) -> Vec<ServerEnvelope> {
        for event in self.transport.poll() {
            match event {
                TransportEvent::Opened => {}
                TransportEvent::Text(text) => self.received(&text),
                TransportEvent::Closed { code } => {
                    if self.link.ended() {
                        continue;
                    }
                    match code {
                        close::SIGN_IN_AGAIN => self.end(LinkState::SignInAgain),
                        close::ROOM_CLOSED => self.end(LinkState::Closed),
                        close::LEFT => self.end(LinkState::Left),
                        _ => self.retry_later(),
                    }
                }
            }
        }
        std::mem::take(&mut self.inbox)
    }

    fn room(&self) -> Option<&RoomState> {
        self.mirror.state()
    }

    fn connection(&self) -> Connection {
        match self.link {
            LinkState::Joining => Connection::Joining,
            LinkState::Connected => Connection::Connected,
            LinkState::Reconnecting { .. } => Connection::Reconnecting,
            LinkState::SignInAgain | LinkState::Closed | LinkState::Left => {
                Connection::Disconnected
            }
        }
    }
}
