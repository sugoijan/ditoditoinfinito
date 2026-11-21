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
//!   (Options → Display's, Host options' "Show my Twitch chat") and lets
//!   the live chat run (round TW6) until the maintainer has verified the
//!   live connection: on only where the scripted chat plays
//!   (`mpfakechat`, `--mp-dev-chat`) or the live one's development script
//!   ([`set_live_own`]: `mpfakechat=live`, `--mp-dev-chat live`). Showing
//!   the rows to players is [`chat_rows_shown`]'s one line.

use std::cell::Cell;

use ddi_chat::Level;

thread_local! {
    static CHAT_TRIP: Cell<Option<Level>> = const { Cell::new(None) };
    static CHAT_TESTING: Cell<Option<Level>> = const { Cell::new(None) };
    static LIVE_OWN: std::cell::RefCell<Option<RoomChat>> = const { std::cell::RefCell::new(None) };
}

/// TESTING ONLY (round TW6), the live chat's development script
/// (`mpfakechat=live`, `--mp-dev-chat live`): the signed-in person's own
/// channel (`channel_id`, `login`), whose live chat any play shows outside
/// a room (a solo play, autoplay) and CPU practice's stages, through
/// exactly the code a room's stage uses; and the testing switch on
/// ([`chat_testing`]: Emotes and messages). `None` ends it.
pub fn set_live_own(own: Option<(String, String)>) {
    LIVE_OWN.with(|l| {
        *l.borrow_mut() = own.map(|(channel_id, login)| RoomChat {
            ceiling: Level::Messages,
            channel_id,
            login,
            room: None,
        })
    });
}

/// The live chat's development script's own channel, where it runs.
pub fn live_own() -> Option<RoomChat> {
    LIVE_OWN.with(|l| l.borrow().clone())
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
    CHAT_TESTING
        .get()
        .or_else(|| live_own().map(|o| o.ceiling))
}

/// Whether the chat's rows show and a room's live chat runs (see
/// [`chat_testing`]). Once the maintainer's run has verified the live
/// connection, players are shown them by this function's one line
/// becoming `true`.
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
    /// The online room's id, which names the route a device's socket is
    /// subscribed by (round TW6); none outside an online room (one's own
    /// channel, the development script's).
    pub room: Option<String>,
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
            room: None,
        })
    }

    /// The same, in the online room `room` (its subscribe route's).
    pub fn in_room(self, room: Option<String>) -> RoomChat {
        RoomChat { room, ..self }
    }

    /// TESTING ONLY: CPU practice has no host's Twitch; where the live
    /// chat's development script runs, the person's own channel; where
    /// the scripted chat plays (the testing switch), its own line stands
    /// for the room's ceiling.
    pub fn practice() -> Option<RoomChat> {
        if CHAT_TESTING.get().is_none()
            && let Some(own) = live_own()
        {
            return Some(own);
        }
        let ceiling = chat_testing().filter(|c| *c != Level::Off)?;
        Some(RoomChat {
            ceiling,
            channel_id: "scripted".into(),
            login: "scripted".into(),
            room: None,
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
        assert_eq!(c.room, None);
        assert_eq!(c.clone().in_room(Some("Room000001".into())).room.as_deref(), Some("Room000001"));
        set_chat_testing(None);
        assert_eq!(RoomChat::practice(), None);
        set_chat_testing(Some(Level::Emotes));
        assert_eq!(RoomChat::practice().map(|c| c.ceiling), Some(Level::Emotes));
        set_chat_testing(None);
    }

    /// Round TW6: the live chat's development script is the testing
    /// switch too, and CPU practice's stages show the person's own channel.
    #[test]
    fn the_live_script_is_the_testing_switch_and_its_own_channel() {
        set_chat_testing(None);
        set_live_own(None);
        assert!(!chat_rows_shown());
        set_live_own(Some(("1001".into(), "me".into())));
        assert!(chat_rows_shown());
        assert_eq!(chat_testing(), Some(Level::Messages));
        let p = RoomChat::practice().unwrap();
        assert_eq!((&*p.channel_id, &*p.login, p.room.as_deref()), ("1001", "me", None));
        set_live_own(None);
        assert!(!chat_rows_shown());
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
