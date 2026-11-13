//! The audience: people who watch a room beyond its members, counted and
//! not listed, read only (`docs/plans/multiplayer-audience.md`). The
//! contract the game and the private server both compile, `audience-v0`
//! (PROVISIONAL), beside the room's reducer and its live side, never
//! inside them: the reducer, the members' protocol and the live frames
//! are unchanged.
//!
//! - **Relays** ([`relay`]): the audience's sockets are not the room's.
//!   Each relay (a Durable Object of its own on the server) holds a few
//!   hundred seats, keeps one socket up to the room, takes each audience
//!   frame once and sends the same bytes to every seat; it reports its
//!   seats' count up once a second at most.
//! - **The room's side** ([`hub`]): the room coalesces its members' frames
//!   a few times a second into one audience frame ([`frames`]), encoded
//!   once and sent to each relay; it sends the public view
//!   ([`public::PublicView`]) when what the audience is shown changes,
//!   sums the relays' counts and tells its members how many watch.
//! - **The device's side** ([`client`]): an audience seat's link: its
//!   hello, the view, the frames read in order (the members'
//!   [`crate::live::FrameReader`]), the count, resumes after a gap, a
//!   keep-alive; the room's clock estimated from the frames' times.
//!
//! An audience seat says nothing but its hello, a resume after a gap and
//! its keep-alive ([`KEEPALIVE`], answered by the relay's host without
//! waking it); it has no name in the room and no row in the room's state.
//!
//! The JSON messages (`{"v":2,"audience":…}` between a seat and its
//! relay, `{"v":2,"relay":…}` between a relay and the room) and the
//! binary audience frame ([`frames::AUDIENCE_FRAME_V0`]) are new and
//! additive: nothing a member's device or the room's members' sockets
//! read changes, but for one live message each way (the count to members,
//! [`crate::live::ServerLive::Audience`]; the host's opening and closing,
//! [`crate::live::ClientLive::Audience`]) that an older peer passes over.

pub mod client;
pub mod frames;
pub mod hub;
pub mod public;
pub mod relay;

use crate::live::server::LiveSnapshot;
use crate::protocol::PROTOCOL_VERSION;
use crate::{Revision, StageId};
use serde::{Deserialize, Serialize};

pub use client::{Audience, AudienceLink, AudienceNews};
pub use frames::{AudienceFrame, Coalescer};
pub use hub::{AudienceHub, HubOut};
pub use public::PublicView;
pub use relay::{Relay, RelayOut, SeatId, Target};

/// The audience protocol's version, said in a seat's and a relay's hello.
pub const AUDIENCE_VERSION: u32 = 0;

/// What a seat sends to keep its socket open between stages: the host
/// answers it with [`KEEPALIVE_ANSWER`] itself (Durable Objects'
/// `setWebSocketAutoResponse`), never waking the relay.
pub const KEEPALIVE: &str = "ping";
pub const KEEPALIVE_ANSWER: &str = "pong";

/// How the audience is served: its numbers, named and versioned.
/// PROVISIONAL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudienceRules {
    pub id: &'static str,
    /// The room builds one audience frame this often during a stage (four
    /// a second at most, from its members' ten).
    pub frame_ms: u64,
    /// A room's audience bound: seats across its relays.
    pub max_audience: u32,
    /// Seats one relay holds.
    pub relay_seats: u32,
    /// Relays an audience spreads over first: every identity's home is
    /// one of them; past them relays are opened only as the first fill
    /// (so a small audience starts a few relays, never the bound's all).
    pub home_relays: u32,
    /// Seats one identity may hold in a room (two tabs, a phone).
    pub seats_per_identity: u32,
    /// A relay says its count up, and the room says the total to its
    /// members and relays, at most this often.
    pub count_ms: u64,
    /// Between stages (no frames to carry the count), a seat is told the
    /// count at most this often.
    pub idle_count_ms: u64,
    /// The view is sent again at most this often (changes in between are
    /// gathered).
    pub view_ms: u64,
    /// An audience device draws everything (music too) at most this far
    /// behind the room's song time (`crate::live::ViewDelay`'s ceiling).
    pub presentation_delay_ms: u64,
    /// Audience frames a relay (and the room) keeps for a resume (30 s).
    pub keep_frames: u32,
    /// Frames sent after a snapshot (2 s).
    pub snapshot_frames: u32,
    /// A seat's keep-alive.
    pub keepalive_ms: u64,
    /// A seat given and not taken (its socket never said hello) this long
    /// is given up (a ticket's life).
    pub ticket_ms: u64,
}

/// `audience-v0` (PROVISIONAL, 2026-11-13): four audience frames a second,
/// 20,000 seats a room in relays of 500 (40 relays), two seats an
/// identity, the count once a second, a view at most once a second, the
/// audience at most two seconds behind, 30 s of frames for a resume.
pub const AUDIENCE_V0: AudienceRules = AudienceRules {
    id: "audience-v0",
    frame_ms: 250,
    max_audience: 20_000,
    relay_seats: 500,
    home_relays: 4,
    seats_per_identity: 2,
    count_ms: 1_000,
    idle_count_ms: 5_000,
    view_ms: 1_000,
    presentation_delay_ms: 2_000,
    keep_frames: 120,
    snapshot_frames: 8,
    keepalive_ms: 30_000,
    ticket_ms: 60_000,
};

impl AudienceRules {
    pub fn named(id: &str) -> Option<AudienceRules> {
        [AUDIENCE_V0].into_iter().find(|r| r.id == id)
    }

    /// How many relays a full audience takes.
    pub fn relays(&self) -> u32 {
        self.max_audience.div_ceil(self.relay_seats.max(1))
    }

    /// The relay an identity tries first: its home, one of the first
    /// [`AudienceRules::home_relays`] (the same each time, so its seats are
    /// counted together).
    pub fn home_relay(&self, identity: &str) -> u32 {
        // FNV-1a 32: stable across builds and platforms.
        let mut h: u32 = 0x811c_9dc5;
        for b in identity.bytes() {
            h ^= u32::from(b);
            h = h.wrapping_mul(0x0100_0193);
        }
        h % self.home_relays.clamp(1, self.relays().max(1))
    }

    /// The relays an identity tries, in order, while each is full: its
    /// home, then every `home_relays`-th after it (the relays that open as
    /// its home's fill), then every other relay once.
    pub fn relay_order(&self, identity: &str) -> Vec<u32> {
        let n = self.relays().max(1);
        let stride = self.home_relays.clamp(1, n);
        let home = self.home_relay(identity);
        let mut order: Vec<u32> = (home..n).step_by(stride as usize).collect();
        order.extend((0..n).filter(|k| k % stride != home));
        order
    }
}

/// What a room that has an audience tells its clients (in its policy,
/// `RoomPolicy::audience`). ADDITIVE: a room without it has no audience
/// (an authority that predates it), so a client offers nothing of it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudiencePolicy {
    /// The audience contract's id (`audience-v0`).
    pub id: String,
}

impl AudiencePolicy {
    pub fn v0() -> Self {
        Self {
            id: AUDIENCE_V0.id.into(),
        }
    }

    pub fn rules(&self) -> Option<AudienceRules> {
        AudienceRules::named(&self.id)
    }
}

/// Why a seat was not given: the audience join's answers (the server's
/// `POST /rooms/<id>/audience`, `409`/`403`/`410` with the code).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudienceRefusal {
    /// Every seat is taken: "The room's audience is full."
    Full,
    /// The host closed the audience.
    Closed,
    /// The room ended.
    RoomClosed,
    /// This identity holds as many seats as it may.
    TooManySeats,
    /// The link is not this room's audience link (or was replaced).
    BadLink,
    /// The room has no audience (its authority predates it).
    NoAudience,
}

impl AudienceRefusal {
    /// The error code the server answers with.
    pub fn code(self) -> &'static str {
        match self {
            Self::Full => "audience_full",
            Self::Closed => "audience_closed",
            Self::RoomClosed => "room_closed",
            Self::TooManySeats => "too_many_seats",
            Self::BadLink => "audience_link_unknown",
            Self::NoAudience => "no_audience",
        }
    }

    /// The HTTP status the server answers with.
    pub fn status(self) -> u16 {
        match self {
            Self::Full | Self::TooManySeats => 409,
            Self::Closed => 403,
            Self::RoomClosed => 410,
            Self::BadLink | Self::NoAudience => 404,
        }
    }
}

/// Why a seat's socket was closed by its relay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudienceEnd {
    /// The room ended (4410).
    RoomClosed,
    /// The host closed the audience (4403).
    Closed,
}

/// Close codes on an audience seat's socket.
pub mod close {
    /// The host closed the audience.
    pub const AUDIENCE_CLOSED: u16 = 4403;
    /// The room ended (as a member's socket).
    pub const ROOM_CLOSED: u16 = crate::online::close::ROOM_CLOSED;

    pub fn of(end: super::AudienceEnd) -> u16 {
        match end {
            super::AudienceEnd::RoomClosed => ROOM_CLOSED,
            super::AudienceEnd::Closed => AUDIENCE_CLOSED,
        }
    }
}

/// A seat's message to its relay (JSON text).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AudienceUp {
    /// First on every socket: the view, the count and the stage as it
    /// stands come back.
    Hello { version: u32 },
    /// The frames of `stage` after `after` (none: from a snapshot).
    Resume {
        stage: StageId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<u32>,
    },
}

/// A relay's message to a seat (JSON text; frames are binary).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AudienceDown {
    /// The room as the audience is shown it.
    View(Box<PublicView>),
    /// How many watch (between stages; during one, the frames carry it).
    Count { count: u32 },
    /// The stage as it stands; the frames from its `first_frame` (the
    /// audience's numbering) follow.
    Snapshot(Box<LiveSnapshot>),
    /// The socket is about to close, and why.
    Ended { end: AudienceEnd },
}

/// A relay's message to the room (JSON text).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RelayUp {
    /// First on the relay's socket to the room: which relay, the view it
    /// has (its revision) and the last frame it has, so the room sends
    /// what it lacks.
    Hello {
        relay: u32,
        version: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        view: Option<Revision>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<(StageId, u32)>,
    },
    /// Its seats now.
    Count { seats: u32 },
}

/// The room's message to a relay (JSON text; frames are binary).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RelayDown {
    /// Who may take a seat: the audience link's key as the room's host
    /// keeps it (opaque here: the server's hash of the link's secret,
    /// compared as given), whether the audience is open, and how many
    /// seats this relay holds.
    Admission {
        key: String,
        open: bool,
        seats: u32,
    },
    View(Box<PublicView>),
    /// The stage as it stands (its `first_frame` in the audience's
    /// numbering); its frames follow.
    Snapshot(Box<LiveSnapshot>),
    /// How many watch in all (between stages).
    Count {
        count: u32,
    },
    /// The room ended, or the audience was closed: every seat goes.
    Ended {
        end: AudienceEnd,
    },
}

macro_rules! envelope {
    ($name:ident, $field:ident, $msg:ty) => {
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        pub struct $name {
            pub v: u32,
            pub $field: $msg,
        }

        impl $name {
            pub fn new($field: $msg) -> Self {
                Self {
                    v: PROTOCOL_VERSION,
                    $field,
                }
            }

            pub fn text(msg: $msg) -> String {
                serde_json::to_string(&Self::new(msg)).unwrap_or_default()
            }

            pub fn read(text: &str) -> Option<$msg> {
                serde_json::from_str::<Self>(text).ok().map(|e| e.$field)
            }
        }
    };
}

envelope!(AudienceUpEnvelope, audience, AudienceUp);
envelope!(AudienceDownEnvelope, audience, AudienceDown);
envelope!(RelayUpEnvelope, relay, RelayUp);
envelope!(RelayDownEnvelope, relay, RelayDown);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rules_spread_identities_over_the_first_relays_then_open_more() {
        let r = AUDIENCE_V0;
        assert_eq!(r.relays(), 40);
        let mut homes = [0u32; 4];
        for i in 0..20_000 {
            homes[r.home_relay(&format!("twitch:{i}")) as usize] += 1;
        }
        assert!(
            homes.iter().all(|n| (4_500..=5_500).contains(n)),
            "{homes:?}"
        );
        assert_eq!(r.home_relay("twitch:1"), r.home_relay("twitch:1"));
        // Every relay once, the home's own first.
        let order = r.relay_order("twitch:1");
        let home = r.home_relay("twitch:1");
        assert_eq!(order[0], home);
        assert!(order[..10].iter().all(|k| k % 4 == home));
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..40).collect::<Vec<_>>());
    }

    #[test]
    fn the_messages_read_back_and_are_none_of_the_members() {
        let hello = AudienceUpEnvelope::text(AudienceUp::Hello { version: 0 });
        assert_eq!(hello, r#"{"v":2,"audience":{"kind":"hello","version":0}}"#);
        assert_eq!(
            AudienceUpEnvelope::read(&hello),
            Some(AudienceUp::Hello { version: 0 })
        );
        // Never a member's frame: a room that predates the audience
        // closes a socket sending it (4400), as for any frame it cannot
        // read, and an audience socket is never a member's.
        assert!(serde_json::from_str::<crate::ClientFrame>(&hello).is_err());
        let up = RelayUpEnvelope::text(RelayUp::Hello {
            relay: 3,
            version: 0,
            view: Some(Revision(9)),
            after: Some((StageId(2), 40)),
        });
        assert!(serde_json::from_str::<crate::ClientFrame>(&up).is_err());
        assert!(matches!(
            RelayUpEnvelope::read(&up),
            Some(RelayUp::Hello { relay: 3, .. })
        ));
        let down = RelayDownEnvelope::text(RelayDown::Count { count: 132 });
        assert!(serde_json::from_str::<crate::ServerEnvelope>(&down).is_err());
        assert!(serde_json::from_str::<crate::live::ServerLiveEnvelope>(&down).is_err());
        assert_eq!(AudienceRefusal::Full.code(), "audience_full");
        assert_eq!(close::of(AudienceEnd::RoomClosed), 4410);
    }

    #[test]
    fn a_room_without_an_audience_says_nothing_of_it() {
        let policy = crate::RoomPolicy::default();
        assert!(!serde_json::to_string(&policy).unwrap().contains("audience"));
        let with = policy.with_audience();
        let text = serde_json::to_string(&with).unwrap();
        assert!(text.contains(r#""audience":{"id":"audience-v0"}"#));
        let back: crate::RoomPolicy = serde_json::from_str(&text).unwrap();
        assert_eq!(back.audience.unwrap().rules(), Some(AUDIENCE_V0));
        let newer: AudiencePolicy = serde_json::from_str(r#"{"id":"audience-v9"}"#).unwrap();
        assert_eq!(newer.rules(), None);
    }
}
