//! Stable identifiers. A participant, match or stage is named by an id the
//! authority hands out once and never reuses, never by a display name or a
//! position in a list. Content is identified opaquely: the room never looks
//! inside a song or chart.

use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! numeric_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u64);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

numeric_id!(
    /// One person's or CPU's record in a room: kept through reconnects,
    /// departures and later matches, so results always point at the same row.
    ParticipantId
);
numeric_id!(
    /// One finite match in a room.
    MatchId
);
numeric_id!(
    /// One stage (one play of a song), unique within the room. A stage keeps
    /// its id from the lobby's playlist into its match and the match's record.
    StageId
);

/// The room's revision: one more for every accepted change, so a client can
/// tell a missed update from an old one and an edit made against an older
/// view from a current one.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Revision(pub u64);

impl Revision {
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl fmt::Display for Revision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "r{}", self.0)
    }
}

/// The authority's clock, in milliseconds. Deadlines and the committed start
/// are times on it; it never moves a play's note times.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Millis(pub u64);

impl Millis {
    pub fn after(self, ms: u64) -> Self {
        Self(self.0.saturating_add(ms))
    }
}

macro_rules! text_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(text: impl Into<String>) -> Self {
                Self(text.into())
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

text_id!(
    /// The room itself, as its authority names it.
    RoomId
);
text_id!(
    /// One client connection's name for idempotent retries (`seq` counts
    /// within it).
    ClientId
);
text_id!(
    /// Who a human is, as the authority established it: `local` for the
    /// practice player, `twitch:<user id>` online, `bot:<name>` for a
    /// simulated player the server signed in for its operator (M7d). Never
    /// a display name.
    Identity
);

impl Identity {
    /// The prefix of a bot's identity (`bot:<slug>`): a kind of player of
    /// its own, only ever set by the server (from a session it issued to an
    /// operator's admin token), never a Twitch user's.
    pub const BOT_PREFIX: &'static str = "bot:";

    /// Bot `slug`'s identity (`bot:<slug>`).
    pub fn bot(slug: &str) -> Self {
        Self(format!("{}{slug}", Self::BOT_PREFIX))
    }

    /// Whether this is a bot's identity: shown labeled Bot, and never a
    /// server administrator.
    pub fn is_bot(&self) -> bool {
        self.0.starts_with(Self::BOT_PREFIX)
    }
}
text_id!(
    /// A song, as the host's library or the shared-song service names it.
    SongKey
);
text_id!(
    /// A chart within a song, by its digest as text, so the same chart is
    /// recognized wherever it was loaded from.
    ChartKey
);

/// What a stage plays: a song and one of its charts, both opaque.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContentId {
    pub song: SongKey,
    pub chart: ChartKey,
}

/// Id-keyed maps travel as lists of `[key, value]` pairs: JSON object keys
/// are strings, which buffered (tagged) messages cannot turn back into
/// numeric ids.
pub(crate) mod pairs {
    use serde::de::Deserializer;
    use serde::ser::Serializer;
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;

    pub fn serialize<K, V, S>(map: &BTreeMap<K, V>, s: S) -> Result<S::Ok, S::Error>
    where
        K: Serialize,
        V: Serialize,
        S: Serializer,
    {
        s.collect_seq(map.iter())
    }

    pub fn deserialize<'de, K, V, D>(d: D) -> Result<BTreeMap<K, V>, D::Error>
    where
        K: Deserialize<'de> + Ord,
        V: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        Ok(Vec::<(K, V)>::deserialize(d)?.into_iter().collect())
    }
}
