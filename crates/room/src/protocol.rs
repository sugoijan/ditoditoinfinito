//! The versioned envelope commands and updates travel in, and a client's
//! mirror of the room built from them. JSON on the wire (serde); the same
//! types serve the local practice authority, so practice and online rooms
//! exchange the same messages.
//!
//! Client to authority: `{"v":1,"client":"…","seq":17,"base":42,"cmd":{…}}`.
//! `seq` counts within a client id (retries are idempotent); `base` is the
//! room revision the client's view was at, which conflicting edits need.
//! Authority to client: `{"v":1,"rev":43,"msg":{"kind":"events",…}}`, or an
//! `ack`, a `reject` or a full `snapshot`. `rev` is the room's revision
//! after the message.
//!
//! One more frame from a client: [`Resync`], `{"v":1,"want":"snapshot"}`,
//! which asks for the whole room after its mirror found a gap
//! ([`MirrorError::Gap`]); the authority answers it alone with a snapshot.
//! A client's frames are either ([`ClientFrame`]).

use crate::command::{Command, Event, Rejection};
use crate::ids::{ClientId, Revision};
use crate::state::RoomState;
use serde::{Deserialize, Serialize};

/// The protocol this build speaks.
pub const PROTOCOL_VERSION: u32 = 1;
/// The oldest protocol an authority still accepts. The web app deploys on
/// push, the server separately and desktop builds lag, so an authority
/// keeps accepting the previous version once there is one.
pub const OLDEST_PROTOCOL_VERSION: u32 = 1;

pub fn supported(version: u32) -> bool {
    (OLDEST_PROTOCOL_VERSION..=PROTOCOL_VERSION).contains(&version)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientEnvelope {
    pub v: u32,
    pub client: ClientId,
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<Revision>,
    pub cmd: Command,
}

impl ClientEnvelope {
    pub fn new(client: ClientId, seq: u64, base: Option<Revision>, cmd: Command) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            client,
            seq,
            base,
            cmd,
        }
    }
}

/// A client's request for the room as it is, after a missed revision:
/// `{"v":1,"want":"snapshot"}`. Protocol 1 has had it from the start (the
/// server's first frame of its own), so it needs no new version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resync {
    pub v: u32,
    pub want: Want,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Want {
    Snapshot,
}

impl Resync {
    pub fn snapshot() -> Self {
        Self {
            v: PROTOCOL_VERSION,
            want: Want::Snapshot,
        }
    }
}

/// What a client sends over its socket: a command in its envelope, or a
/// resync request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ClientFrame {
    Command(ClientEnvelope),
    Resync(Resync),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerEnvelope {
    pub v: u32,
    pub rev: Revision,
    pub msg: ServerMessage,
}

impl ServerEnvelope {
    pub fn new(rev: Revision, msg: ServerMessage) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            rev,
            msg,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServerMessage {
    /// The command was accepted (its events came before this).
    Ack { client: ClientId, seq: u64 },
    /// The command was refused and changed nothing.
    Reject {
        client: ClientId,
        seq: u64,
        rejection: Rejection,
    },
    /// One revision's events, in order.
    Events { events: Vec<Event> },
    /// The whole room, for a new or out-of-step client.
    Snapshot { room: Box<RoomState> },
}

/// A client's copy of the room, kept by applying the authority's messages.
#[derive(Clone, Debug, Default)]
pub struct Mirror {
    state: Option<RoomState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MirrorError {
    /// Events arrived before any snapshot.
    NoSnapshot,
    /// A revision was missed: ask for a snapshot.
    Gap {
        have: Revision,
        got: Revision,
    },
    UnsupportedVersion {
        got: u32,
    },
}

impl Mirror {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> Option<&RoomState> {
        self.state.as_ref()
    }

    /// Apply one message. Events are applied only at the next revision; an
    /// old one is ignored, a later one is a gap.
    pub fn receive(&mut self, envelope: &ServerEnvelope) -> Result<(), MirrorError> {
        if !supported(envelope.v) {
            return Err(MirrorError::UnsupportedVersion { got: envelope.v });
        }
        match &envelope.msg {
            ServerMessage::Snapshot { room } => {
                if self
                    .state
                    .as_ref()
                    .is_none_or(|s| room.revision >= s.revision)
                {
                    self.state = Some((**room).clone());
                }
                Ok(())
            }
            ServerMessage::Events { events } => {
                let state = self.state.as_mut().ok_or(MirrorError::NoSnapshot)?;
                if envelope.rev <= state.revision {
                    Ok(())
                } else if envelope.rev == state.revision.next() {
                    state.commit(events);
                    Ok(())
                } else {
                    Err(MirrorError::Gap {
                        have: state.revision,
                        got: envelope.rev,
                    })
                }
            }
            ServerMessage::Ack { .. } | ServerMessage::Reject { .. } => Ok(()),
        }
    }
}
