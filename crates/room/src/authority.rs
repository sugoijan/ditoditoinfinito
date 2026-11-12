//! The authority a room's clients talk to. One trait for the shells: the
//! local practice authority below, and later an online adapter whose
//! authority is the private server running this same reducer.

use crate::command::{Actor, Command, Event, Rejection};
use crate::ids::{ClientId, Identity, Millis, ParticipantId, RoomId};
use crate::member::{Connection, Intent};
use crate::protocol::{
    ClientEnvelope, OLDEST_PROTOCOL_VERSION, PROTOCOL_VERSION, ServerEnvelope, ServerMessage,
    supported,
};
use crate::rules::{ContentSource, MatchRules, RoomPolicy};
use crate::state::RoomState;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;

/// What a shell holds to take part in a room.
pub trait RoomAuthority {
    /// Send a client's command. Its outcome (ack or reject) and any events
    /// arrive through `poll`, as they would over a network.
    fn submit(&mut self, envelope: ClientEnvelope);
    /// Messages since the last poll, oldest first. Also lets time pass:
    /// deadlines and countdowns due by now happen first.
    fn poll(&mut self) -> Vec<ServerEnvelope>;
    /// The room as this side knows it: authoritative locally, mirrored
    /// online.
    fn room(&self) -> Option<&RoomState>;
    fn connection(&self) -> Connection;
}

/// The authority's time source, injected so runs are deterministic.
pub trait Clock {
    fn now(&self) -> Millis;
}

/// A clock tests and the practice driver set by hand. Clones share it.
#[derive(Clone, Debug, Default)]
pub struct ManualClock(Rc<Cell<u64>>);

impl ManualClock {
    pub fn new(ms: u64) -> Self {
        Self(Rc::new(Cell::new(ms)))
    }

    pub fn set(&self, ms: u64) {
        self.0.set(ms);
    }

    pub fn advance(&self, ms: u64) {
        self.0.set(self.0.get().saturating_add(ms));
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Millis {
        Millis(self.0.get())
    }
}

/// The room's authority on this device: CPU practice, with no network, no
/// sign-in and no service. Deterministic for a given clock and inputs.
pub struct LocalAuthority<C: Clock> {
    state: RoomState,
    clock: C,
    clients: BTreeMap<ClientId, ParticipantId>,
    /// Each client's last sequence number and its outcome, for retries.
    last: BTreeMap<ClientId, (u64, Option<Rejection>)>,
    outbox: Vec<ServerEnvelope>,
}

impl<C: Clock> LocalAuthority<C> {
    /// The practice client's id and identity.
    pub const PRACTICE_CLIENT: &'static str = "local";

    /// Run `state`. The first poll carries its snapshot.
    pub fn new(state: RoomState, clock: C) -> Self {
        let outbox = vec![ServerEnvelope::new(
            state.revision,
            ServerMessage::Snapshot {
                room: Box::new(state.clone()),
            },
        )];
        Self {
            state,
            clock,
            clients: BTreeMap::new(),
            last: BTreeMap::new(),
            outbox,
        }
    }

    /// A practice room owned by the local player, bound to the practice
    /// client.
    pub fn practice(player_name: String, rules: MatchRules, clock: C) -> (Self, ClientId) {
        Self::practice_under(player_name, rules, clock, RoomPolicy::default())
    }

    /// [`LocalAuthority::practice`] under `policy` (DEVELOPMENT ONLY where
    /// it is [`RoomPolicy::development`]: far more CPUs than a match
    /// allows).
    pub fn practice_under(
        player_name: String,
        rules: MatchRules,
        clock: C,
        policy: RoomPolicy,
    ) -> (Self, ClientId) {
        let state = RoomState::new(
            RoomId::new("practice"),
            Identity::new(Self::PRACTICE_CLIENT),
            player_name,
            policy,
            rules,
            ContentSource::LocalLibrary,
        );
        let owner = ParticipantId(1);
        let mut authority = Self::new(state, clock);
        let client = ClientId::new(Self::PRACTICE_CLIENT);
        authority.clients.insert(client.clone(), owner);
        (authority, client)
    }

    pub fn state(&self) -> &RoomState {
        &self.state
    }

    /// The room itself, for the fake server's rewriting
    /// (`fake::FakeServer::rewrite`).
    pub(crate) fn state_mut(&mut self) -> &mut RoomState {
        &mut self.state
    }

    pub fn participant(&self, client: &ClientId) -> Option<ParticipantId> {
        self.clients.get(client).copied()
    }

    /// Admit (or restore) a person and bind `client` to them.
    pub fn connect(
        &mut self,
        client: ClientId,
        identity: Identity,
        name: String,
        admin: bool,
        intent: Intent,
    ) -> Result<ParticipantId, Rejection> {
        self.authority(Command::Join {
            identity: identity.clone(),
            name,
            admin,
            intent,
        })?;
        let id = self
            .state
            .member_by_identity(&identity)
            .map(|m| m.id)
            .ok_or(Rejection::NotAMember)?;
        self.clients.insert(client, id);
        Ok(id)
    }

    /// Bind `client` to member `participant`, admitted already (an online
    /// authority's socket: the person is known from the ticket, the
    /// client id from the envelope). Refused for a client id bound to
    /// someone else, as the server refuses another person's client id.
    pub fn bind(&mut self, client: ClientId, participant: ParticipantId) -> Result<(), Rejection> {
        match self.clients.get(&client) {
            Some(p) if *p != participant => Err(Rejection::UnknownClient),
            _ => {
                self.clients.insert(client, participant);
                Ok(())
            }
        }
    }

    /// The client's connection is gone. Its member stays, disconnected.
    pub fn disconnect(&mut self, client: &ClientId) {
        if let Some(id) = self.clients.remove(client) {
            self.last.remove(client);
            let _ = self.authority(Command::SetConnection {
                participant: id,
                connection: Connection::Disconnected,
            });
        }
    }

    /// The authority's own input: confirmed stage outcomes, the song's end,
    /// content status, interruptions.
    pub fn authority(&mut self, command: Command) -> Result<Vec<Event>, Rejection> {
        self.tick();
        let events = self
            .state
            .apply(Actor::Authority, None, &command, self.clock.now())?;
        self.publish(&events);
        Ok(events)
    }

    fn publish(&mut self, events: &[Event]) {
        if !events.is_empty() {
            self.outbox.push(ServerEnvelope::new(
                self.state.revision,
                ServerMessage::Events {
                    events: events.to_vec(),
                },
            ));
        }
    }

    fn tick(&mut self) {
        let events = self.state.advance(self.clock.now());
        self.publish(&events);
    }

    fn reply(&mut self, client: ClientId, seq: u64, outcome: Option<Rejection>) {
        let msg = match outcome {
            None => ServerMessage::Ack { client, seq },
            Some(rejection) => ServerMessage::Reject {
                client,
                seq,
                rejection,
            },
        };
        self.outbox
            .push(ServerEnvelope::new(self.state.revision, msg));
    }

    fn snapshot(&mut self) {
        self.outbox.push(ServerEnvelope::new(
            self.state.revision,
            ServerMessage::Snapshot {
                room: Box::new(self.state.clone()),
            },
        ));
    }
}

impl<C: Clock> RoomAuthority for LocalAuthority<C> {
    fn submit(&mut self, envelope: ClientEnvelope) {
        self.tick();
        let ClientEnvelope {
            v,
            client,
            seq,
            base,
            cmd,
        } = envelope;
        if !supported(v) {
            let rejection = Rejection::UnsupportedVersion {
                got: v,
                oldest: OLDEST_PROTOCOL_VERSION,
                newest: PROTOCOL_VERSION,
            };
            return self.reply(client, seq, Some(rejection));
        }
        let Some(&id) = self.clients.get(&client) else {
            return self.reply(client, seq, Some(Rejection::UnknownClient));
        };
        if let Some((last, outcome)) = self.last.get(&client).cloned() {
            if seq == last {
                // A retry: the same answer, nothing applied twice.
                return self.reply(client, seq, outcome);
            }
            if seq < last {
                return self.reply(client, seq, Some(Rejection::OutOfOrder { last }));
            }
        }
        let outcome = match self
            .state
            .apply(Actor::Member(id), base, &cmd, self.clock.now())
        {
            Ok(events) => {
                self.publish(&events);
                None
            }
            Err(rejection) => Some(rejection),
        };
        self.last.insert(client.clone(), (seq, outcome.clone()));
        let stale = matches!(
            outcome,
            Some(Rejection::Stale { .. } | Rejection::StaleReadiness { .. })
        );
        self.reply(client, seq, outcome);
        if stale {
            // Never a silent overwrite: the client gets the room as it is.
            self.snapshot();
        }
    }

    fn poll(&mut self) -> Vec<ServerEnvelope> {
        self.tick();
        std::mem::take(&mut self.outbox)
    }

    fn room(&self) -> Option<&RoomState> {
        Some(&self.state)
    }

    fn connection(&self) -> Connection {
        Connection::Connected
    }
}
