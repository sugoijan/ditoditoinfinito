//! The host's Twitch in a room (room model revision 18,
//! `docs/plans/multiplayer-twitch.md`, B and C1): which of a person's
//! Twitch lists keep people out of the room (the mirror: their bans, or
//! their bans and blocks) and how much of their chat the room may show
//! (the ceiling: emotes, or emotes and messages), both the setting of the
//! one who turned it on ([`TwitchSetting::by`]).
//!
//! The model knows only the setting and who may change it
//! (`RoomState::may_set_twitch`); the checks themselves are the server's
//! (it holds the person's Twitch access, never the game), which gives the
//! room the change as its authority (`crate::Command::SetTwitch`) and
//! removes a barred member as the authority (`crate::Command::Remove`).

use crate::ids::ParticipantId;
use serde::{Deserialize, Serialize};

/// Whose Twitch lists keep people out (B1): none, the bans (timeouts
/// included, while they last), or the bans and the blocks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TwitchMirror {
    #[default]
    Off,
    Bans,
    BansAndBlocks,
}

impl TwitchMirror {
    pub const ALL: [TwitchMirror; 3] = [Self::Off, Self::Bans, Self::BansAndBlocks];

    pub fn on(self) -> bool {
        self != Self::Off
    }

    /// Whether the blocks are read too.
    pub fn blocks(self) -> bool {
        self == Self::BansAndBlocks
    }
}

/// How much of the host's chat the room may show (C1, the room's
/// ceiling; each device chooses for itself, never above it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TwitchChat {
    #[default]
    Off,
    Emotes,
    EmotesAndMessages,
}

impl TwitchChat {
    pub const ALL: [TwitchChat; 3] = [Self::Off, Self::Emotes, Self::EmotesAndMessages];

    pub fn on(self) -> bool {
        self != Self::Off
    }
}

/// The room's Twitch setting ([`crate::RoomState::twitch`]): one a room,
/// the lists and the channel of the person who turned it on (B3, C1). It
/// keeps applying while they are a member, also after the room changed
/// hands, and ends when they leave the room (or are removed from it).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwitchSetting {
    /// Who turned it on: whose lists and whose channel.
    pub by: ParticipantId,
    #[serde(default, skip_serializing_if = "mirror_off")]
    pub mirror: TwitchMirror,
    #[serde(default, skip_serializing_if = "chat_off")]
    pub chat: TwitchChat,
    /// Their Twitch user id (the channel's, for a device's chat).
    pub user_id: String,
    /// Their Twitch login, for the screens to say whose lists apply.
    pub login: String,
}

fn mirror_off(m: &TwitchMirror) -> bool {
    !m.on()
}

fn chat_off(c: &TwitchChat) -> bool {
    !c.on()
}

impl TwitchSetting {
    /// The longest id and login a setting carries (bytes; Twitch's are
    /// far shorter).
    pub const MAX_TEXT: usize = 64;

    /// Whether anything of it is on (a setting with both off is none).
    pub fn on(&self) -> bool {
        self.mirror.on() || self.chat.on()
    }

    /// Within [`TwitchSetting::MAX_TEXT`], the id and login not empty.
    pub fn fits(&self) -> bool {
        [&self.user_id, &self.login]
            .iter()
            .all(|t| !t.is_empty() && t.len() <= Self::MAX_TEXT)
    }

    /// Whether `to` only turns parts of this setting down, the same
    /// person's (what the one who set it may do once the room is no
    /// longer theirs, B3: turn it off, or a part of it).
    pub fn lowered_to(&self, to: &TwitchSetting) -> bool {
        to.by == self.by
            && to.user_id == self.user_id
            && to.mirror <= self.mirror
            && to.chat <= self.chat
    }
}
