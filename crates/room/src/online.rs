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
//! Live play (`crate::live`): on a socket to a room whose policy plays
//! live, and a transport that carries binary messages, a [`LiveLink`]
//! keeps the room's clock, sends the local play's stream and reads the
//! room's frames ([`OnlineAuthority::live`], [`OnlineAuthority::live_mut`]).
//!
//! Sans-IO for tickets: a socket opens with a one-use ticket the shell
//! gets over HTTP (`POST /rooms/<id>/ticket`). The authority says when it
//! wants one ([`OnlineAuthority::wants_ticket`]) and is given the answer
//! ([`OnlineAuthority::ticket`]).

use crate::authority::{Clock, RoomAuthority};
use crate::client_stats::{ClientKind, ClientStatsEnvelope, Ending, KeepAliveSeen, Tracker};
use crate::command::Rejection;
use crate::ids::{Millis, Revision, RoomId};
use crate::live::client::LiveLink;
use crate::live::{LIVE_V1, ServerLiveEnvelope};
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
    /// The person's oldest socket closed for a newer one: the server keeps
    /// four a person in a room (ddi-server `room.rs`'s
    /// `SOCKETS_A_PARTICIPANT`). Since round P1b (the maintainer's of
    /// 2026-11-16) the device it closed stops (`LinkState::OpenedElsewhere`)
    /// rather than connecting again, so five devices no longer take turns
    /// for ever; it connects again only when its person asks ("Use this
    /// device", `OnlineAuthority::restart`).
    pub const OPENED_ELSEWHERE: u16 = 4429;
    /// A host or a co-host removed the member (room model revision 18,
    /// `Command::Remove`, a Kick): every socket of theirs closed so
    /// (`LinkState::Kicked`). They may come back by the invite.
    pub const KICKED: u16 = 4403;
    /// The host's Twitch lists keep the member out (revision 18, the
    /// mirror's removal): every socket of theirs closed so
    /// (`LinkState::NotAdmitted`), never told apart from a ban, a timeout
    /// or a block.
    pub const NOT_ADMITTED: u16 = 4406;
}

/// A message on a socket: JSON text or binary (live play's frames,
/// `crate::live::wire`; an audience frame). What every side of the room
/// puts on a socket: a member's link, the hub, a relay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    Text(String),
    Binary(Vec<u8>),
}

/// What a shell's socket reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransportEvent {
    Opened,
    Text(String),
    /// A binary message (live play's frames, `crate::live::wire`).
    Binary(Vec<u8>),
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
    /// Send a binary message, where [`Transport::binary`] says it can.
    fn send_binary(&mut self, bytes: Vec<u8>) {
        let _ = bytes;
    }
    /// Whether it carries binary messages both ways (live play needs it;
    /// a transport that does not leaves the room's live side alone).
    fn binary(&self) -> bool {
        false
    }
    fn close(&mut self);
    /// What happened since the last poll, oldest first: the socket open
    /// now only (an earlier one's late close is not reported).
    fn poll(&mut self) -> Vec<TransportEvent>;
    /// Whether the socket keeps itself alive and notices its own death
    /// (desktop's: pings at the WebSocket layer after 10 s of silence,
    /// closed after 20 s without an answer): the authority's own
    /// keep-alive ([`KEEPALIVE_V0`]) is then not sent.
    fn keeps_alive(&self) -> bool {
        false
    }
    /// Put `message` on the socket: text or binary.
    fn put(&mut self, message: Message) {
        match message {
            Message::Text(t) => self.send(t),
            Message::Binary(b) => self.send_binary(b),
        }
    }
}

impl<T: Transport + ?Sized> Transport for Box<T> {
    fn open(&mut self, room: &RoomId, ticket: &str) {
        (**self).open(room, ticket)
    }
    fn send(&mut self, text: String) {
        (**self).send(text)
    }
    fn send_binary(&mut self, bytes: Vec<u8>) {
        (**self).send_binary(bytes)
    }
    fn binary(&self) -> bool {
        (**self).binary()
    }
    fn close(&mut self) {
        (**self).close()
    }
    fn poll(&mut self) -> Vec<TransportEvent> {
        (**self).poll()
    }
    fn keeps_alive(&self) -> bool {
        (**self).keeps_alive()
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
    /// The session ended: a ticket refused as signed out (401), or a
    /// second 4401 within [`SIGN_IN_RECHECK_MS`] of the reconnect the
    /// first one led to (a 4401 alone only asks for a ticket again:
    /// feedback round AA). Only signing in again mends it, never a retry.
    SignInAgain,
    /// The room closed (4410, or the room gone when asking a ticket).
    Closed,
    /// Left the room (4000), or no member any more.
    Left,
    /// Refused a ticket as signed out (401) before this link ever reached
    /// the room: the person was never signed in here (a room's page
    /// address opened by someone without a session; feedback round AA:
    /// a first-time player shown the room's address instead of its
    /// invite link was told their sign-in "ended"). Signing in mends it
    /// as [`LinkState::SignInAgain`] does ([`OnlineAuthority::restart`]).
    SignInToEnter,
    /// Refused a ticket as no member (403) before this link ever reached
    /// the room: signed in, but never joined it (only its invite link
    /// admits). Where the link had reached the room, it is
    /// [`LinkState::Left`].
    NotAMember,
    /// The room was opened on another device of the same person (4429:
    /// the server keeps four sockets a person and closed this one, the
    /// oldest, for a newer one): this device stops until its person asks
    /// for it back ([`OnlineAuthority::restart`], "Use this device"), which
    /// closes another device's socket in turn (round P1b).
    OpenedElsewhere,
    /// The room's authority is newer than this client's room model
    /// ([`crate::UPDATE_REQUIRED`], a ticket refused): only a newer game
    /// mends it (a reload on the web).
    UpdateRequired,
    /// The room's authority is older than this client's room model, or
    /// says none ([`crate::SERVER_BEHIND`], a ticket's answer): no socket
    /// is opened, and a ticket asked again after a wait (the `attempt`th
    /// time) while the server is being updated.
    ServerBehind {
        attempt: u32,
    },
    /// A host or a co-host removed this person from the room (4403,
    /// revision 18): "The host removed you from the room." The invite
    /// admits them again.
    Kicked,
    /// The host's Twitch lists keep this person out (4406, a join's or a
    /// ticket's `403 not_admitted`, revision 18): "You can't join this
    /// room.", the same words for a ban, a timeout and a block.
    NotAdmitted,
}

impl LinkState {
    /// Whether the link is over until the person acts (signs in again,
    /// leaves for the home).
    pub fn ended(self) -> bool {
        matches!(
            self,
            Self::SignInAgain
                | Self::SignInToEnter
                | Self::NotAMember
                | Self::Closed
                | Self::Left
                | Self::UpdateRequired
                | Self::OpenedElsewhere
                | Self::Kicked
                | Self::NotAdmitted
        )
    }
}

/// Why a ticket could not be had.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TicketProblem {
    /// `401`: the session expired or is gone.
    SignedOut,
    /// `403 not_a_member`.
    NotAMember,
    /// `403 not_admitted` (revision 18): the host's Twitch lists keep
    /// this person out.
    NotAdmitted,
    /// `404 room_unknown`, `410 room_closed`.
    RoomClosed,
    /// `403 update_required`: this client's room model is older than the
    /// authority's ([`crate::check_model`]).
    UpdateRequired,
    /// A ticket whose answer says an older room model than this client's,
    /// or none ([`crate::check_authority`]): asked again after a wait.
    ServerBehind,
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

/// A member's socket's keep-alive (room model revision 12, feedback
/// round N2; PROVISIONAL, `keepalive-v0`): a socket that has heard
/// nothing for `silent_ms` sends [`crate::audience::KEEPALIVE`], which
/// the server's runtime answers itself ([`crate::audience::KEEPALIVE_ANSWER`],
/// a hibernatable WebSocket's auto-response: the room's object is not
/// woken and the answer is not billed as a request to it), so that an
/// idle room's socket carries something often enough for a home router,
/// a phone network or a proxy not to drop it; and one that hears nothing
/// in `dead_after_ms` more of the client's own running time is taken as
/// dead (closed and replaced by a reconnect, as a socket that closed).
///
/// Running time: the time between two of the client's polls counts up to
/// `run_step_ms` at most, so a hidden tab whose timers fire once a minute
/// sends its keep-alive when it runs (silence is the wall clock's) but
/// never judges an answer overdue over the minutes it did not run (with
/// any message waking it, the web's `wake_hidden`, an answer that comes
/// is read at once).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeepAlivePolicy {
    pub name: &'static str,
    pub silent_ms: u64,
    pub dead_after_ms: u64,
    pub run_step_ms: u64,
}

/// `keepalive-v0` (PROVISIONAL).
pub const KEEPALIVE_V0: KeepAlivePolicy = KeepAlivePolicy {
    name: "keepalive-v0",
    silent_ms: 20_000,
    dead_after_ms: 20_000,
    run_step_ms: 1_500,
};

/// What [`KeepAlivePolicy`] keeps of a socket (a member's, and an
/// audience seat's, `crate::audience::client::Audience`).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct KeepAlive {
    /// When the socket last said anything (the device's ms).
    heard_at: u64,
    /// The client's running time (ms, [`KeepAlivePolicy::run_step_ms`]).
    run_ms: u64,
    last_poll: Option<u64>,
    /// The running time a keep-alive went out at, nothing heard since.
    sent_at_run: Option<u64>,
    /// Keep-alives sent, and sockets taken as dead.
    sent: u64,
    dead: u64,
}

/// What the keep-alive asks for at a poll.
enum KeepAliveDue {
    Send,
    Dead,
}

impl KeepAlive {
    pub(crate) fn ran(&mut self, now: u64, policy: &KeepAlivePolicy) {
        if let Some(last) = self.last_poll {
            self.run_ms += now.saturating_sub(last).min(policy.run_step_ms);
        }
        self.last_poll = Some(now);
    }

    pub(crate) fn heard(&mut self, now: u64) {
        self.heard_at = now;
        self.sent_at_run = None;
    }

    /// A keep-alive went out (its answer awaited unless one already is).
    pub(crate) fn sent(&mut self) {
        self.sent += 1;
        self.sent_at_run.get_or_insert(self.run_ms);
    }

    /// Nothing heard in the policy's running time since a keep-alive.
    pub(crate) fn dead(&mut self, policy: &KeepAlivePolicy) -> bool {
        let dead = self
            .sent_at_run
            .is_some_and(|at| self.run_ms.saturating_sub(at) >= policy.dead_after_ms);
        if dead {
            self.sent_at_run = None;
            self.dead += 1;
        }
        dead
    }

    pub(crate) fn counts(&self) -> (u64, u64) {
        (self.sent, self.dead)
    }

    /// One went out and nothing was heard since.
    pub(crate) fn awaiting(&self) -> bool {
        self.sent_at_run.is_some()
    }

    fn due(&mut self, now: u64, policy: &KeepAlivePolicy) -> Option<KeepAliveDue> {
        if self.dead(policy) {
            return Some(KeepAliveDue::Dead);
        }
        if self.sent_at_run.is_none() && now.saturating_sub(self.heard_at) >= policy.silent_ms {
            self.sent();
            return Some(KeepAliveDue::Send);
        }
        None
    }
}

/// How long a resync's answer may take before a later gap asks again.
const RESYNC_AGAIN_MS: u64 = 2_000;

/// A socket closed 4401 asks for a ticket once before the link ends:
/// the room closes a socket at the expiry its ticket carried, which may
/// be older than the session the person holds now (a cookie re-signed
/// since, a sign-in in another tab), and a ticket granted proves the
/// session good, so the link reconnects as after any lost socket. A
/// second 4401 within this long of that check ends it (no loop).
pub const SIGN_IN_RECHECK_MS: u64 = 60_000;

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
    live: LiveLink,
    keepalive: KeepAlivePolicy,
    keep: KeepAlive,
    /// The sockets' fates for the usage report (`crate::client_stats`).
    stats: Tracker,
    /// When a 4401 last led to a ticket asked again
    /// ([`SIGN_IN_RECHECK_MS`]).
    sign_in_checked: Option<u64>,
    /// A socket of this link has had the room (its first snapshot): a
    /// ticket refused later ends it as a session or a membership that
    /// ended ([`LinkState::SignInAgain`], [`LinkState::Left`]), before
    /// as one that never was ([`LinkState::SignInToEnter`],
    /// [`LinkState::NotAMember`]).
    reached: bool,
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
            live: LiveLink::new(LIVE_V1),
            keepalive: KEEPALIVE_V0,
            keep: KeepAlive::default(),
            stats: Tracker::default(),
            sign_in_checked: None,
            reached: false,
        }
    }

    /// The page hidden or shown at `at_ms` (the shell's, on this
    /// authority's clock; a shell that never says leaves it unknown in the
    /// usage report).
    pub fn set_hidden(&mut self, hidden: bool, at_ms: u64) {
        let at = at_ms.min(self.clock.now().0);
        self.stats.set_hidden(hidden, at);
    }

    /// A usage report is due: a socket has the room and none was sent for
    /// it (`crate::client_stats`).
    pub fn stats_due(&self) -> bool {
        self.stats.due()
    }

    /// The due usage report sent for a client of `kind`; `None` (Options'
    /// Usage statistics off, or no kind said) sends nothing and drops it.
    /// Whether one was sent.
    pub fn send_stats(&mut self, kind: Option<&ClientKind>) -> bool {
        let report = self.stats.take(kind);
        match report {
            Some(r) if self.synced => {
                self.send(ClientStatsEnvelope::text(r));
                true
            }
            _ => false,
        }
    }

    /// The socket ended (as the client saw it): its fate kept for the next
    /// socket's usage report.
    fn socket_ended(&mut self, how: Ending) {
        let now = self.clock.now().0;
        let keep = KeepAliveSeen {
            own: self.transport.keeps_alive(),
            sent: self.keep.counts().0,
            awaiting: self.keep.awaiting(),
        };
        let rtt = self.live.round_trip_ms();
        self.stats.ended(now, how, keep, rtt);
    }

    /// Under another keep-alive policy (tests).
    pub fn with_keepalive(mut self, policy: KeepAlivePolicy) -> Self {
        self.keepalive = policy;
        self
    }

    /// Keep-alives sent so far, and sockets taken as dead for want of an
    /// answer ([`KEEPALIVE_V0`]).
    pub fn keepalives(&self) -> (u64, u64) {
        self.keep.counts()
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

    /// Live play's link (`crate::live`): the room's clock, the frames read.
    pub fn live(&self) -> &LiveLink {
        &self.live
    }

    /// Live play's link, to queue live messages and the local play's
    /// stream; sent on the next poll.
    pub fn live_mut(&mut self) -> &mut LiveLink {
        &mut self.live
    }

    /// The room's time now, once the clock is estimated.
    pub fn room_now(&self) -> Option<Millis> {
        self.live.room_ms(self.clock.now().0)
    }

    /// The live rules of the room, where this socket plays live (its
    /// policy names rules this build knows and the transport carries
    /// binary messages).
    fn live_rules(&self) -> Option<crate::live::LiveRules> {
        if !self.transport.binary() {
            return None;
        }
        self.mirror
            .state()
            .and_then(|r| r.policy.live.as_ref().and_then(|l| l.rules()))
    }

    fn pump_live(&mut self) {
        // The room quiet (no stage gathering, counting down or playing):
        // its clock pinged as its policy says then (round P1b).
        if let Some(r) = self.mirror.state() {
            let quiet = !matches!(
                r.phase,
                crate::Phase::Preparing { window: Some(_), .. }
                    | crate::Phase::Countdown { .. }
                    | crate::Phase::Stage { .. }
                    | crate::Phase::SongCompletion { .. }
            );
            let every = r.policy.live.as_ref().and_then(|l| l.quiet_ping_ms);
            self.live.set_quiet(quiet, every, self.clock.now().0);
        }
        let (following, started) = self
            .mirror
            .state()
            .and_then(|r| {
                let (id, _) = crate::live::server::live_stage(r)?;
                Some((Some(id), !matches!(r.phase, crate::Phase::Countdown { .. })))
            })
            .unwrap_or((None, false));
        self.live.follow(following, started);
        for send in self.live.due(self.clock.now().0) {
            self.transport.put(send);
        }
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
                if matches!(self.link, LinkState::ServerBehind { .. }) {
                    self.link = LinkState::Joining;
                }
                self.ticket = Ticket::Used;
                self.synced = false;
                self.keep.heard(self.clock.now().0);
                self.stats.opening(self.clock.now().0, self.keep.counts().0);
                self.transport.open(&self.room, &ticket);
            }
            Err(TicketProblem::SignedOut) if self.reached => self.end(LinkState::SignInAgain),
            Err(TicketProblem::SignedOut) => self.end(LinkState::SignInToEnter),
            Err(TicketProblem::NotAMember) if self.reached => self.end(LinkState::Left),
            Err(TicketProblem::NotAMember) => self.end(LinkState::NotAMember),
            Err(TicketProblem::NotAdmitted) => self.end(LinkState::NotAdmitted),
            Err(TicketProblem::RoomClosed) => self.end(LinkState::Closed),
            Err(TicketProblem::UpdateRequired) => self.end(LinkState::UpdateRequired),
            Err(TicketProblem::ServerBehind) => {
                let attempt = match self.link {
                    LinkState::ServerBehind { attempt } => attempt + 1,
                    _ => 1,
                };
                self.link = LinkState::ServerBehind { attempt };
                self.ticket = Ticket::Wanted {
                    at: self.clock.now().0 + self.policy.wait_ms(attempt),
                };
            }
            Err(TicketProblem::Unreachable) => self.retry_later(),
        }
    }

    /// A reconnect that waits out its backoff tried at once (a page shown
    /// again, a hidden tab's socket that closed): its ticket wanted now,
    /// the link still Reconnecting and its attempts counted on (never
    /// Joining for a moment, as [`OnlineAuthority::restart`] would say).
    /// Whether one waited.
    pub fn retry_now(&mut self) -> bool {
        let now = self.clock.now().0;
        match (self.link, self.ticket) {
            (LinkState::Reconnecting { .. }, Ticket::Wanted { at }) if at > now => {
                self.ticket = Ticket::Wanted { at: now };
                true
            }
            _ => false,
        }
    }

    /// After signing in again, or "Use this device" after the room was
    /// opened on another ([`LinkState::OpenedElsewhere`]): start over as
    /// a join (the same member is restored by the server; nothing here is
    /// assumed of the room).
    pub fn restart(&mut self) {
        self.socket_ended(Ending::Restarted);
        self.transport.close();
        self.link = LinkState::Joining;
        self.synced = false;
        self.live.closed();
        self.resyncing = None;
        // A sign-in since: a 4401 asks for a ticket again.
        self.sign_in_checked = None;
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
        self.live.closed();
        self.ticket = Ticket::Used;
    }

    /// The socket closed 4401: the session it opened with expired. A
    /// ticket asked again at once tells whether the person's session is
    /// still good (a reconnect, the link Reconnecting meanwhile) or not
    /// (refused 401: [`LinkState::SignInAgain`]); a second 4401 within
    /// [`SIGN_IN_RECHECK_MS`] of that check ends the link as before.
    fn sign_in_closed(&mut self, now: u64) {
        if self
            .sign_in_checked
            .is_some_and(|at| now.saturating_sub(at) < SIGN_IN_RECHECK_MS)
        {
            self.end(LinkState::SignInAgain);
            return;
        }
        self.sign_in_checked = Some(now);
        self.retry_later();
        self.ticket = Ticket::Wanted { at: now };
    }

    fn retry_later(&mut self) {
        let attempt = match self.link {
            LinkState::Reconnecting { attempt } => attempt + 1,
            _ => 1,
        };
        self.link = LinkState::Reconnecting { attempt };
        self.synced = false;
        self.live.closed();
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
        if text == crate::audience::KEEPALIVE_ANSWER {
            // The keep-alive's answer: heard, nothing to read.
            return;
        }
        let Ok(envelope) = serde_json::from_str::<ServerEnvelope>(text) else {
            if let Ok(live) = serde_json::from_str::<ServerLiveEnvelope>(text) {
                let now = self.clock.now().0;
                self.live.text(live.live, now);
            }
            // Otherwise not ours to read (a newer server's message):
            // passed over.
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
                    self.reached = true;
                    self.stats.synced();
                    self.link = LinkState::Connected;
                    for e in self.unanswered.clone() {
                        self.send_envelope(&e);
                    }
                    if let Some(rules) = self.live_rules() {
                        let now = self.clock.now().0;
                        self.live.opened(now, rules);
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
        // What the live side has to say goes first: a stream's end before
        // the run it ends with (`ReportOutcome`).
        self.pump_live();
        if self.synced {
            self.send_envelope(&envelope);
        }
        self.unanswered.push(envelope);
    }

    fn poll(&mut self) -> Vec<ServerEnvelope> {
        let now = self.clock.now().0;
        self.keep.ran(now, &self.keepalive);
        for event in self.transport.poll() {
            if !matches!(event, TransportEvent::Closed { .. }) {
                self.keep.heard(now);
            }
            match event {
                TransportEvent::Opened => {}
                TransportEvent::Text(text) => self.received(&text),
                TransportEvent::Binary(bytes) => {
                    self.live.binary(&bytes, now);
                }
                TransportEvent::Closed { code } => {
                    if self.link.ended() {
                        continue;
                    }
                    self.socket_ended(Ending::Closed(code));
                    match code {
                        close::SIGN_IN_AGAIN => self.sign_in_closed(now),
                        close::ROOM_CLOSED => self.end(LinkState::Closed),
                        close::LEFT => self.end(LinkState::Left),
                        close::OPENED_ELSEWHERE => self.end(LinkState::OpenedElsewhere),
                        close::KICKED => self.end(LinkState::Kicked),
                        close::NOT_ADMITTED => self.end(LinkState::NotAdmitted),
                        _ => self.retry_later(),
                    }
                }
            }
        }
        // The keep-alive, on a socket that is the room's (its first
        // snapshot read) and keeps none of its own.
        if self.synced && !self.transport.keeps_alive() {
            match self.keep.due(now, &self.keepalive) {
                Some(KeepAliveDue::Send) => self.send(crate::audience::KEEPALIVE.to_string()),
                Some(KeepAliveDue::Dead) => {
                    // Silently gone (no close came): replaced, as a socket
                    // that closed.
                    self.socket_ended(Ending::KeepAliveDead);
                    self.transport.close();
                    self.retry_later();
                }
                None => {}
            }
        }
        self.pump_live();
        std::mem::take(&mut self.inbox)
    }

    fn room(&self) -> Option<&RoomState> {
        self.mirror.state()
    }

    fn connection(&self) -> Connection {
        match self.link {
            LinkState::Joining => Connection::Joining,
            LinkState::Connected => Connection::Connected,
            LinkState::Reconnecting { .. } | LinkState::ServerBehind { .. } => {
                Connection::Reconnecting
            }
            LinkState::SignInAgain
            | LinkState::SignInToEnter
            | LinkState::NotAMember
            | LinkState::Closed
            | LinkState::Left
            | LinkState::UpdateRequired
            | LinkState::OpenedElsewhere
            | LinkState::Kicked
            | LinkState::NotAdmitted => Connection::Disconnected,
        }
    }
}
