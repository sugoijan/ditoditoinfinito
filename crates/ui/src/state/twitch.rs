//! What this device asks of Twitch (`docs/plans/multiplayer-twitch.md`,
//! part A) and what of a room's Twitch reaches a play (C1).
//!
//! - [`want_chat_scope`]: Options' "Twitch chat in rooms" turned on
//!   without the chat's scope (`user:read:chat`) asks for the trip through
//!   Twitch, which the options' shell starts ([`take_chat_trip`]; round
//!   TW3): the closed scope set `chat`, back where a room's Host options
//!   trip comes back, the setting made by itself once the access has the
//!   scope, "nothing changed" when the trip is canceled.
//! - [`RoomChat`]: the room's ceiling and whose channel it is, as a stage
//!   is ordered (`StageSetup::chat`), from `RoomState::twitch`.
//! - [`chat_testing`]: TESTING ONLY, the switch that shows the chat's rows
//!   (Options → Display's, Host options' "Show my Twitch chat") until the
//!   transports to Twitch exist (they wait for the maintainer's spike):
//!   on only where the scripted chat plays (`mpfakechat`, `--mp-dev-chat`).

use std::cell::Cell;

use ddi_chat::Level;

thread_local! {
    static CHAT_TRIP: Cell<Option<Level>> = const { Cell::new(None) };
    static CHAT_TESTING: Cell<Option<Level>> = const { Cell::new(None) };
}

/// "Twitch chat in rooms" was turned on at `level` on this device where
/// its Twitch access lacks the chat's scope: the options' shell takes it
/// ([`take_chat_trip`]) and starts the trip through Twitch; the setting is
/// made once the access comes back with the scope.
pub fn want_chat_scope(level: Level) {
    CHAT_TRIP.set((level != Level::Off).then_some(level));
}

/// The trip through Twitch [`want_chat_scope`] asked for, once.
pub fn take_chat_trip() -> Option<Level> {
    CHAT_TRIP.take()
}

/// TESTING ONLY (`TWITCH_CHAT_TESTING`): the scripted chat plays this run
/// at `ceiling` (its own line), or none. Set by
/// `ddi_session::chat::set_scripted`, the shells' `mpfakechat` and
/// `--mp-dev-chat`.
pub fn set_chat_testing(ceiling: Option<Level>) {
    CHAT_TESTING.set(ceiling);
}

/// TESTING ONLY: the scripted chat's ceiling where it plays this run.
/// The chat's rows (Options → Display's "Twitch chat in rooms" and its
/// own, Host options' "Show my Twitch chat") show only while it does:
/// until the transports to Twitch exist, a room's chat has no other
/// source, and rows that do nothing are not shown to players.
pub fn chat_testing() -> Option<Level> {
    CHAT_TESTING.get()
}

/// Whether the chat's rows show (see [`chat_testing`]).
pub fn chat_rows_shown() -> bool {
    chat_testing().is_some()
}

/// A room's chat as a stage of it is played (C1): how much of the host's
/// chat the room allows and whose channel it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomChat {
    /// The room's ceiling (never Off: a room that shows none has none).
    pub ceiling: Level,
    /// The channel's Twitch user id and login (the one who turned it on).
    pub channel_id: String,
    pub login: String,
}

impl RoomChat {
    /// The chat of a room whose Twitch setting is `setting`, if it shows
    /// any.
    pub fn of(setting: Option<&ddi_room::TwitchSetting>) -> Option<RoomChat> {
        let t = setting?;
        let ceiling = level_of(t.chat);
        (ceiling != Level::Off).then(|| RoomChat {
            ceiling,
            channel_id: t.user_id.clone(),
            login: t.login.clone(),
        })
    }

    /// TESTING ONLY: CPU practice has no host's Twitch; where the scripted
    /// chat plays (the testing switch), its own line stands for the
    /// room's ceiling.
    pub fn practice() -> Option<RoomChat> {
        let ceiling = chat_testing().filter(|c| *c != Level::Off)?;
        Some(RoomChat {
            ceiling,
            channel_id: "scripted".into(),
            login: "scripted".into(),
        })
    }
}

/// The room model's chat choice as the chat's level.
pub fn level_of(chat: ddi_room::TwitchChat) -> Level {
    match chat {
        ddi_room::TwitchChat::Off => Level::Off,
        ddi_room::TwitchChat::Emotes => Level::Emotes,
        ddi_room::TwitchChat::EmotesAndMessages => Level::Messages,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rooms_chat_is_its_setting_and_off_is_none() {
        let mut t = ddi_room::TwitchSetting {
            by: ddi_room::ParticipantId(1),
            mirror: ddi_room::TwitchMirror::Bans,
            chat: ddi_room::TwitchChat::Off,
            user_id: "77".into(),
            login: "host".into(),
        };
        assert_eq!(RoomChat::of(None), None);
        assert_eq!(RoomChat::of(Some(&t)), None);
        t.chat = ddi_room::TwitchChat::EmotesAndMessages;
        let c = RoomChat::of(Some(&t)).unwrap();
        assert_eq!((c.ceiling, &*c.channel_id, &*c.login), (Level::Messages, "77", "host"));
        set_chat_testing(None);
        assert_eq!(RoomChat::practice(), None);
        set_chat_testing(Some(Level::Emotes));
        assert_eq!(RoomChat::practice().map(|c| c.ceiling), Some(Level::Emotes));
        set_chat_testing(None);
    }

    #[test]
    fn a_trip_is_taken_once() {
        want_chat_scope(Level::Emotes);
        assert_eq!(take_chat_trip(), Some(Level::Emotes));
        assert_eq!(take_chat_trip(), None);
        want_chat_scope(Level::Off);
        assert_eq!(take_chat_trip(), None);
    }
}
