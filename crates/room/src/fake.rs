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
//! Reconnecting, and with a grace set ([`FakeServer::set_grace`]: the
//! server's reconnect grace; none by default) Disconnected once it has
//! passed; a room that ended ([`RoomState::closed`]) closes every socket
//! with 4410 and opens none. Not the server: no HTTP; its links'
//! previews and the audience join are answered as the server's bodies
//! (`invites.rs`).
//!
//! Live play (`crate::live`) where the room's policy has it: the room's
//! live side ([`LiveServer`], the same the server runs) beside the
//! reducer, its commands applied as the authority's, its frames to every
//! socket that said hello, binary messages both ways. A live message to a
//! room without it closes the socket with 4400, as an authority that
//! predates it would. Each socket may be given a latency
//! ([`FakeServer::set_latency`]: a floor and a jitter each way, in order
//! as a stream keeps it) and faults on what it sends
//! ([`FakeServer::fault_uplink`]: delay, drop).

use crate::authority::{Clock, LocalAuthority, ManualClock, RoomAuthority};
use crate::command::{Command, Event, Rejection};
use crate::ids::{ClientId, Identity, Millis, ParticipantId, RoomId};
use crate::live::server::{LiveOut, LiveReply, LiveServer};
use crate::live::{ClientLive, ServerLiveEnvelope};
use crate::member::{Connection, Intent};
use crate::online::{Transport, TransportEvent, close};
use crate::protocol::{ClientFrame, ServerEnvelope, ServerMessage};
use crate::rules::{ContentSource, MatchRules, RoomPolicy};
use crate::state::RoomState;
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

mod audience;
mod invites;
pub use audience::{AudienceStats, FakeAudienceSocket};
pub use invites::{FakeLinks, LinkKind};

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
    Binary(Vec<u8>),
    Close(u16),
}

impl Frame {
    fn copy(&self) -> Frame {
        match self {
            Frame::Text(t) => Frame::Text(t.clone()),
            Frame::Binary(b) => Frame::Binary(b.clone()),
            Frame::Close(c) => Frame::Close(*c),
        }
    }
}

/// What a client sent, on its way to the room.
enum Up {
    Text(String),
    Binary(Vec<u8>),
}

/// A socket's latency: each message's one-way delay is the floor plus up
/// to the jitter, drawn from the seed; messages keep their order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Latency {
    pub down_ms: u64,
    pub down_jitter_ms: u64,
    pub up_ms: u64,
    pub up_jitter_ms: u64,
    pub seed: u64,
}

impl Latency {
    fn draw(&mut self, floor: u64, jitter: u64) -> u64 {
        self.seed = self.seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        floor + if jitter == 0 { 0 } else { z % (jitter + 1) }
    }
}

/// A clock for a client of the fake room: the room's own, `offset_ms`
/// ahead (negative: behind), as a device's clock would be.
#[derive(Clone, Debug)]
pub struct OffsetClock {
    pub base: ManualClock,
    pub offset_ms: i64,
}

impl Clock for OffsetClock {
    fn now(&self) -> Millis {
        Millis(self.base.now().0.saturating_add_signed(self.offset_ms))
    }
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
    /// The server closed it: its close frame is on its way.
    closing: bool,
    /// It said hello (`ClientLive::Hello`): it gets the frames.
    live: bool,
    latency: Option<Latency>,
    /// When its last frame arrives (a later one never overtakes it).
    last_down: u64,
    /// What it sent, on its way, with when each arrives.
    uplink: VecDeque<(u64, Up)>,
    up_faults: VecDeque<Fault>,
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
    /// How long a member whose socket is gone stays Reconnecting before
    /// they are Disconnected; none: for ever, as before.
    grace_ms: Option<u64>,
    /// When each Reconnecting member becomes Disconnected.
    grace: BTreeMap<ParticipantId, u64>,
    /// The room's live side, where its policy plays live.
    live: Option<LiveServer>,
    /// Latencies and uplink faults for the next sockets people open.
    pending_latency: BTreeMap<ParticipantId, Latency>,
    pending_up_faults: BTreeMap<ParticipantId, Vec<Fault>>,
    /// The stage in play whose live part a restart lost
    /// ([`FakeServer::restart_live`]), while the room is still on it.
    lost: Option<crate::StageId>,
    /// The room's audience (`crate::audience`), where its policy has one:
    /// its audience side, relays and seats (`audience.rs`).
    audience: Option<audience::FakeAudience>,
    /// The room's name as the server keeps it beside the state (the
    /// audience's view says it).
    room_name: String,
    /// Its three links (`invites.rs`).
    links: FakeLinks,
}

pub type SharedFake = Rc<RefCell<FakeServer>>;

impl FakeServer {
    /// A hosted room (as the server makes one: `MatchRules::hosted_for`
    /// its policy, content shared) owned by `owner`, not connected yet.
    pub fn hosted(
        room: &str,
        owner: Identity,
        name: &str,
        policy: RoomPolicy,
        clock: ManualClock,
    ) -> Self {
        let rules = MatchRules::hosted_for(&policy);
        let state = RoomState::new(
            RoomId::new(room),
            owner,
            name.into(),
            policy,
            rules,
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
        let live = state
            .policy
            .live
            .as_ref()
            .and_then(|l| l.rules())
            .map(LiveServer::new);
        let audience = state
            .policy
            .audience
            .as_ref()
            .and_then(|a| a.rules())
            .map(audience::FakeAudience::new);
        let room_name = state.room.0.clone();
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
            grace_ms: None,
            grace: BTreeMap::new(),
            live,
            pending_latency: BTreeMap::new(),
            pending_up_faults: BTreeMap::new(),
            lost: None,
            links: FakeLinks::of(&room_name),
            audience,
            room_name,
        }
    }

    /// The room's name (as its host gave it; the room's id until set).
    pub fn set_room_name(&mut self, name: &str) {
        self.room_name = name.into();
    }

    /// The room's object restarts, as the server's may mid-stage: what
    /// its live side keeps for storage (`LiveServer::durable`) survives,
    /// the stage in play is lost (its streams were in memory). As the
    /// server's `LiveHost` does then, every batch and resume for that
    /// stage is answered `unknown_stage` and the stage gets no frames
    /// while the room is still on it; it settles on its reports.
    pub fn restart_live(&mut self) {
        let Some(live) = self.live.as_ref() else {
            return;
        };
        let rules = live.rules();
        let durable = live.durable();
        self.live = Some(LiveServer::restore(rules, durable));
        self.lost = crate::live::server::live_stage(self.authority.state())
            .filter(|_| {
                matches!(
                    self.authority.state().phase,
                    crate::Phase::Stage { .. } | crate::Phase::SongCompletion { .. }
                )
            })
            .map(|(id, _)| id);
    }

    /// The stage this room's restart lost, while the room is still on it.
    fn lost_now(&mut self) -> Option<crate::StageId> {
        let current = crate::live::server::live_stage(self.authority.state()).map(|(id, _)| id);
        if self.lost.is_some() && self.lost != current {
            self.lost = None;
        }
        self.lost
    }

    /// The room's live side, where it plays live.
    pub fn live(&self) -> Option<&LiveServer> {
        self.live.as_ref()
    }

    /// `participant`'s sockets' latency, from now on (and for the next
    /// one they open).
    pub fn set_latency(&mut self, participant: ParticipantId, latency: Latency) {
        for c in self
            .conns
            .values_mut()
            .filter(|c| c.participant == participant)
        {
            c.latency = Some(latency);
        }
        self.pending_latency.insert(participant, latency);
    }

    /// A fault on what `participant`'s socket sends next (a delay or a
    /// drop).
    pub fn fault_uplink(&mut self, participant: ParticipantId, fault: Fault) {
        match self
            .conns
            .values_mut()
            .find(|c| c.open && c.participant == participant)
        {
            Some(c) => c.up_faults.push_back(fault),
            None => self
                .pending_up_faults
                .entry(participant)
                .or_default()
                .push(fault),
        }
    }

    /// Time passes: what arrived is read, what is due runs.
    pub fn pump(&mut self) {
        self.uplinks();
        self.route(None);
        self.audience_pump();
    }

    /// The server's reconnect grace (`ROOM_RECONNECT_GRACE_MS`, 30 s
    /// there): a member whose last socket is gone is Disconnected once it
    /// has passed without another.
    pub fn set_grace(&mut self, ms: Option<u64>) {
        self.grace_ms = ms;
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

    /// `POST /rooms/<id>/join` with the room's watch-only invite: a
    /// newcomer joins to watch and is held to it
    /// (`Command::SetWatchOnly`); a present member is answered as they
    /// are.
    pub fn join_watch_only(
        &mut self,
        identity: Identity,
        name: &str,
    ) -> Result<ParticipantId, Rejection> {
        let known = self
            .state()
            .member_by_identity(&identity)
            .is_some_and(|m| !m.left);
        let id = self.join(identity, name, Intent::Watch)?;
        if !known {
            self.authority(Command::SetWatchOnly {
                participant: id,
                on: true,
            })?;
        }
        Ok(id)
    }

    /// `POST /rooms/<id>/ticket`: a one-use ticket for a present member
    /// (`Rejection::RoomClosed` for a room that ended: the server's `410
    /// room_closed`).
    pub fn ticket(&mut self, identity: &Identity) -> Result<String, Rejection> {
        if self.state().closed.is_some() {
            return Err(Rejection::RoomClosed);
        }
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

    /// The room put, outside the reducer, into a state another authority
    /// would have it in (one that predates something this reducer does:
    /// a room nobody chooses in while its host is away, say), as one more
    /// revision; every socket is sent the room. For tests of what a
    /// client shows of such a room: this reducer decides the next command
    /// as it does.
    pub fn rewrite(&mut self, f: impl FnOnce(&mut RoomState)) {
        self.route(None);
        let state = self.authority.state_mut();
        f(state);
        state.revision = state.revision.next();
        let snapshot = ServerEnvelope::new(
            self.state().revision,
            ServerMessage::Snapshot {
                room: Box::new(self.state().clone()),
            },
        );
        let open: Vec<u64> = self
            .conns
            .iter()
            .filter(|(_, c)| c.open)
            .map(|(k, _)| *k)
            .collect();
        for k in open {
            self.push(k, &snapshot);
        }
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
        if self.state().closed.is_some() {
            return None;
        }
        let participant = self.tickets.remove(ticket)?;
        self.grace.remove(&participant);
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
            closing: false,
            live: false,
            latency: self.pending_latency.get(&participant).copied(),
            last_down: 0,
            uplink: VecDeque::new(),
            up_faults: self
                .pending_up_faults
                .remove(&participant)
                .unwrap_or_default()
                .into(),
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

    /// What a client's socket sent: on its way (its latency, its faults),
    /// or read at once.
    fn client_send(&mut self, conn: u64, up: Up) {
        let now = self.now();
        let Some(c) = self.conns.get_mut(&conn).filter(|c| c.open) else {
            return;
        };
        let mut at = now;
        match c.up_faults.pop_front() {
            Some(Fault::Drop) => return,
            Some(Fault::Delay(ms)) => at = now + ms,
            _ => {}
        }
        if let Some(l) = c.latency.as_mut() {
            at = at.max(now + l.draw(l.up_ms, l.up_jitter_ms));
        }
        if let Some((last, _)) = c.uplink.back() {
            at = at.max(*last);
        }
        if at > now || !c.uplink.is_empty() {
            c.uplink.push_back((at, up));
            return;
        }
        match up {
            Up::Text(t) => self.receive(conn, &t),
            Up::Binary(b) => self.receive_binary(conn, &b),
        }
    }

    /// What arrived by now from every socket, in order.
    fn uplinks(&mut self) {
        let now = self.now();
        loop {
            let next = self
                .conns
                .iter_mut()
                .filter(|(_, c)| c.open)
                .find_map(|(k, c)| {
                    (c.uplink.front().is_some_and(|(at, _)| *at <= now))
                        .then(|| (*k, c.uplink.pop_front().expect("checked").1))
                });
            let Some((conn, up)) = next else { break };
            match up {
                Up::Text(t) => self.receive(conn, &t),
                Up::Binary(b) => self.receive_binary(conn, &b),
            }
        }
    }

    fn receive_binary(&mut self, conn: u64, bytes: &[u8]) {
        let Some(participant) = self
            .conns
            .get(&conn)
            .filter(|c| c.open)
            .map(|c| c.participant)
        else {
            return;
        };
        let now = Millis(self.now());
        if self.live.is_none() {
            // An authority that predates live play reads no binary.
            self.close_conn(conn, close::BAD_MESSAGE);
            return;
        }
        if let Some(stage) = self.lost_now() {
            let mut out = LiveOut::default();
            out.reply.push(LiveReply::Message(crate::live::ServerLive::Refused {
                refusal: crate::live::LiveRefusal::UnknownStage { stage },
            }));
            self.live_out(Some(conn), out);
            return;
        }
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let out = live.receive_binary(self.authority.state(), participant, bytes, now);
        self.live_out(Some(conn), out);
    }

    fn live_text(&mut self, conn: u64, participant: ParticipantId, msg: ClientLive) {
        let now = Millis(self.now());
        if matches!(msg, ClientLive::Hello { .. })
            && let Some(c) = self.conns.get_mut(&conn)
        {
            c.live = true;
        }
        if let ClientLive::Audience { open } = msg {
            self.audience_open(participant, open);
        }
        let Some(live) = self.live.as_mut() else {
            self.close_conn(conn, close::BAD_MESSAGE);
            return;
        };
        let out = live.receive(self.authority.state(), participant, msg, now);
        self.live_out(Some(conn), out);
        self.live_tick();
    }

    /// What the live side said, to whom it goes; its commands applied as
    /// the authority's, their events to every socket.
    fn live_out(&mut self, sender: Option<u64>, out: LiveOut) {
        let LiveOut {
            reply,
            broadcast,
            commands,
        } = out;
        if let Some(conn) = sender {
            for r in reply {
                match r {
                    LiveReply::Message(m) => {
                        if let Ok(t) = serde_json::to_string(&ServerLiveEnvelope::new(m)) {
                            self.push_frame(conn, Frame::Text(t), false);
                        }
                    }
                    LiveReply::Frame(b) => self.push_frame(conn, Frame::Binary(b), false),
                }
            }
        }
        for b in broadcast {
            self.audience_member_frame(&b);
            let live: Vec<u64> = self
                .conns
                .iter()
                .filter(|(_, c)| c.open && c.live)
                .map(|(k, _)| *k)
                .collect();
            for k in live {
                self.push_frame(k, Frame::Binary(b.clone()), false);
            }
        }
        if !commands.is_empty() {
            for cmd in commands {
                let _ = self.authority.authority(cmd);
            }
            self.send_out(None);
        }
    }

    /// The live side's time: its stage follows the room's, what is due
    /// runs (again after its own commands changed the room).
    fn live_tick(&mut self) {
        if self.lost_now().is_some() {
            return;
        }
        for _ in 0..4 {
            let now = Millis(self.now());
            let Some(live) = self.live.as_mut() else {
                return;
            };
            let out = live.tick(self.authority.state(), now);
            let again = !out.commands.is_empty();
            self.live_out(None, out);
            if !again {
                return;
            }
        }
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
            ClientFrame::Live(env) => self.live_text(conn, participant, env.live),
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
                self.graces();
                self.authority.submit(*envelope);
                let accepted = self.send_out(Some((conn, &client, seq)));
                self.live_tick();
                // The one who left is told so, also where their leaving
                // ended the room for the others.
                if leave && accepted {
                    self.close_conn(conn, close::LEFT);
                }
                self.close_if_ended();
            }
        }
    }

    /// What the reducer sent out, to whom it goes: events to every socket,
    /// an answer and a snapshot to the sender. Whether the sender's
    /// command (if any) was accepted.
    fn route(&mut self, sender: Option<(u64, &ClientId, u64)>) -> bool {
        self.graces();
        let accepted = self.send_out(sender);
        self.live_tick();
        self.close_if_ended();
        accepted
    }

    /// Reconnect graces that ended by now: their members Disconnected.
    fn graces(&mut self) {
        let now = self.now();
        let due: Vec<ParticipantId> = self
            .grace
            .iter()
            .filter(|(_, at)| **at <= now)
            .map(|(id, _)| *id)
            .collect();
        for participant in due {
            self.grace.remove(&participant);
            if self
                .state()
                .member(participant)
                .is_some_and(|m| m.connection == Connection::Reconnecting)
            {
                let _ = self.authority.authority(Command::SetConnection {
                    participant,
                    connection: Connection::Disconnected,
                });
            }
        }
    }

    /// The room ended: every socket is closed with 4410, after what was
    /// on its way.
    fn close_if_ended(&mut self) {
        if self.state().closed.is_some() {
            let open: Vec<u64> = self
                .conns
                .iter()
                .filter(|(_, c)| c.open && !c.closing)
                .map(|(k, _)| *k)
                .collect();
            for k in open {
                self.close_conn(k, close::ROOM_CLOSED);
            }
        }
    }

    fn send_out(&mut self, sender: Option<(u64, &ClientId, u64)>) -> bool {
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
        let events = matches!(envelope.msg, ServerMessage::Events { .. });
        self.push_frame(conn, Frame::Text(text), events);
    }

    fn push_frame(&mut self, conn: u64, frame: Frame, events: bool) {
        let now = self.now();
        let Some(c) = self.conns.get_mut(&conn).filter(|c| c.open) else {
            return;
        };
        let mut at = now;
        let mut copies = vec![frame.copy()];
        let mut stale = None;
        if let Some(held) = c.held.take() {
            // The frame held back goes after this one.
            copies.push(Frame::Text(held));
        } else {
            match c.faults.pop_front() {
                Some(Fault::Delay(ms)) => at = now + ms,
                Some(Fault::Duplicate) => copies.push(frame.copy()),
                Some(Fault::Drop) => copies.clear(),
                Some(Fault::Reorder) => match &frame {
                    Frame::Text(t) => {
                        c.held = Some(t.clone());
                        copies.clear();
                    }
                    // Held back for the next text.
                    _ => c.faults.push_front(Fault::Reorder),
                },
                Some(Fault::Stale) => stale = c.last_events.clone().map(Frame::Text),
                Some(Fault::Close(_)) | None => {}
            }
        }
        if let Some(l) = c.latency.as_mut() {
            at = at.max(now + l.draw(l.down_ms, l.down_jitter_ms));
        }
        // A stream never lets a later message overtake an earlier one.
        at = at.max(c.last_down);
        c.last_down = at;
        if events && let Frame::Text(t) = frame {
            c.last_events = Some(t);
        }
        for f in copies.into_iter().chain(stale) {
            c.queue.push_back((at, f));
        }
    }

    fn close_conn(&mut self, conn: u64, code: u16) {
        let now = self.now();
        if let Some(c) = self.conns.get_mut(&conn)
            && !c.closing
        {
            c.closing = true;
            c.queue.push_back((now, Frame::Close(code)));
        }
    }

    /// The socket's frames due by now (time passes first: deadlines and
    /// countdowns, to every socket).
    fn deliver(&mut self, conn: u64) -> Vec<TransportEvent> {
        self.pump();
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
                    Frame::Binary(b) => out.push(TransportEvent::Binary(b)),
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
            if let Some(ms) = self.grace_ms {
                self.grace.insert(participant, self.now() + ms);
            }
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
            self.server.borrow_mut().client_send(conn, Up::Text(text));
        }
    }

    fn send_binary(&mut self, bytes: Vec<u8>) {
        if let Some(conn) = self.conn {
            self.server
                .borrow_mut()
                .client_send(conn, Up::Binary(bytes));
        }
    }

    fn binary(&self) -> bool {
        true
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
