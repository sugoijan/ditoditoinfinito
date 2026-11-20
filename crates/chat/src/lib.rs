//! A room host's Twitch chat behind the play (`docs/plans/multiplayer-twitch.md`,
//! part C): everything of it that needs no Twitch, pure and natively
//! tested.
//!
//! - [`eventsub`]: Twitch's EventSub WebSocket messages read from their
//!   JSON (the welcome, keep-alives, the reconnect, the four chat
//!   notifications, revocations).
//! - [`emotes`]: the four providers' emotes (Twitch's from a message's
//!   fragments; 7TV's, BTTV's and FFZ's channel and global sets read from
//!   their JSON and matched as words), each one's still image's address
//!   directly and through our server's proxy (C4, C5).
//! - [`item`]: a message made into an item, a row of emotes or a line of
//!   text with emotes inline, after C11's filters.
//! - [`overlay`]: C9's budgets (`chat-overlay-v0`), the moderation events
//!   and the two motions (C6: Across and Falling; C10: reduced motion's
//!   fixed places), on the host's clock. Deterministic from the events,
//!   the clock, the options and the area.
//! - [`cache`]: the emote images kept (256 at most, four fetched at once,
//!   the proxy once after a failure, C4) and their places in one atlas.
//! - [`script`]: the scripted chat the tests and the studies play
//!   (TESTING ONLY, C16): EventSub messages on a clock, never Twitch.
//!
//! No I/O: the shells bring the sockets, the fetches and the drawing.

pub mod cache;
pub mod emotes;
pub mod eventsub;
pub mod item;
pub mod overlay;
pub mod script;

use serde::{Deserialize, Serialize};

/// The flood rules' name and version (C9; every number PROVISIONAL).
pub const RULES_NAME: &str = "chat-overlay-v0";

/// How much of the chat a device shows (Options → Display, "Twitch chat
/// in rooms"), and the room's ceiling over it (C1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Level {
    /// Nothing: no socket, nothing drawn.
    #[default]
    Off,
    /// Messages' emotes alone, as rows of emotes.
    Emotes,
    /// Messages with their emotes inline.
    Messages,
}

impl Level {
    /// What a device shows under a room's `ceiling`: its own choice,
    /// never above the room's.
    pub fn under(self, ceiling: Level) -> Level {
        self.min(ceiling)
    }
}

/// How items move (C6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Style {
    /// Rows crossing the screen right to left (niconico's way).
    #[default]
    Across,
    /// Columns falling, in the side space where there is some.
    Falling,
}

/// Which providers a device asks for emotes (C4, "Emotes from").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EmotesFrom {
    /// Twitch's own alone: nothing is asked of the other providers.
    TwitchOnly,
    /// Twitch's, 7TV's, BTTV's and FFZ's.
    #[default]
    TwitchAndOthers,
}
