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
//! - Since round TW7 (2026-11-21) the chat's rows and a room's live chat
//!   are every player's; what stays is for tests and the maintainer:
//!   [`scripted_ceiling`], TESTING ONLY, the scripted chat's own line
//!   (`mpfakechat`, `--mp-dev-chat`), which CPU practice's stages take for
//!   the room's ceiling; and [`set_dev_live`], the live chat's development
//!   script (`mpfakechat=live`, `--mp-dev-chat live`, and with a channel
//!   `live:<login>`), which gives solo plays and CPU practice's stages a
//!   live channel and, with a channel named, shows that channel in this
//!   device's online stages in the room's channel's place. For a player
//!   the chat is an online room's stages' alone.
//! - [`KeptChat`] (round DE): the room's chat as this device keeps it
//!   connected between the stages, told by an online room's client as it
//!   ticks ([`keep_room_chat`]) to whoever keeps the connection
//!   (`ddi_session::chat`, [`set_chat_keeper`]); none outside an online
//!   room.

use std::cell::Cell;

use ddi_chat::Level;

thread_local! {
    static KEEPER: std::cell::RefCell<Option<ChatKeeper>> = const { std::cell::RefCell::new(None) };
    static CHAT_TRIP: Cell<Option<Level>> = const { Cell::new(None) };
    static SCRIPTED_CEILING: Cell<Option<Level>> = const { Cell::new(None) };
    static DEV_LIVE: std::cell::RefCell<Option<DevLive>> = const { std::cell::RefCell::new(None) };
}

/// TESTING ONLY, the live chat's development script (round TW6; round
/// TW7's channel): whose live chat this device shows in solo plays and
/// CPU practice's stages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DevLive {
    /// `live`: the signed-in person's own channel (its Twitch user id and
    /// login, from `/me`).
    Own { channel_id: String, login: String },
    /// `live:<login>`: that channel, named to our server by its login
    /// (`POST <api>/me/chat/subscribe {channel}`, administrators only),
    /// there and in an online room's stage in the room's channel's place,
    /// on this device alone.
    Channel { login: String },
}

/// TESTING ONLY: the channel a development flag's `live:<login>` names, as
/// our server takes it (1 to 25 letters, digits or underscores, compared
/// lowercase); none for anything else.
pub fn named_login(login: &str) -> Option<String> {
    ((1..=25).contains(&login.len()) && login.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
        .then(|| login.to_ascii_lowercase())
}

/// TESTING ONLY: the live chat's development script from now on (`None`
/// ends it).
pub fn set_dev_live(dev: Option<DevLive>) {
    DEV_LIVE.with(|l| *l.borrow_mut() = dev);
}

/// The live chat's development script, where it runs.
pub fn dev_live() -> Option<DevLive> {
    DEV_LIVE.with(|l| l.borrow().clone())
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

/// Round TB (the maintainer, 2026-11-22: "we should avoid downgrading the
/// permissions, at least within a time-frame, so that turning off and on
/// again is not annoying, maybe if a permission isn't used for 30 or 60
/// days, it isn't requested on the next sign-in"): a Twitch permission
/// this device used within so many days is asked for again at its next
/// sign-in, in the same step through Twitch; one unused longer is not. The
/// same 30 days after which our server deletes a link nobody used.
pub const TWITCH_KEEP_DAYS: u64 = 30;

const DAY_MS: u64 = 24 * 60 * 60 * 1_000;

/// How often a permission's use is written down at most (one write an
/// hour, not one a frame).
const MARK_EVERY_MS: u64 = 60 * 60 * 1_000;

/// The Twitch features this device used now (`bans`, `blocks`, `chat`,
/// as the server names them; `blocks` stands for both of its scopes):
/// what marks a use is a room this person hosts with the feature on, or
/// a room whose host's chat this device shows
/// (`RoomClient`'s tick). Written to the settings at most once an hour a
/// feature; nothing without the epoch's clock.
pub fn mark_used(features: &[&str]) {
    mark_used_in(features, &crate::state::settings::TickSettings::new());
}

/// [`mark_used`] with the settings as a caller read them (the room
/// client's tick).
pub(crate) fn mark_used_in(features: &[&str], settings: &crate::state::settings::TickSettings) {
    let now = ddi_platform::storage::now_ms() as u64;
    if now == 0 || features.is_empty() {
        return;
    }
    let cached = settings.get();
    let due = features.iter().any(|f| {
        cached
            .twitch_used
            .get(*f)
            .is_none_or(|t| now.saturating_sub(*t) >= MARK_EVERY_MS)
    });
    if !due {
        return;
    }
    let mut s = crate::state::settings::Settings::load();
    for f in features {
        s.twitch_used.insert((*f).to_string(), now);
    }
    s.save();
}

/// The features to ask Twitch for again at a sign-in: those this device
/// used within [`TWITCH_KEEP_DAYS`], as the server's closed list names
/// them (`bans,chat`), in its order.
pub fn kept_features() -> Vec<&'static str> {
    let now = ddi_platform::storage::now_ms() as u64;
    let used = &crate::state::settings::Settings::cached().twitch_used;
    ["bans", "blocks", "chat"]
        .into_iter()
        .filter(|f| {
            used.get(*f)
                .is_some_and(|t| now.saturating_sub(*t) < TWITCH_KEEP_DAYS * DAY_MS)
        })
        .collect()
}

/// Options → Data's Disconnect: nothing is asked for again at the next
/// sign-in.
pub fn forget_used() {
    let mut s = crate::state::settings::Settings::load();
    if !s.twitch_used.is_empty() {
        s.twitch_used.clear();
        s.save();
    }
}

/// TESTING ONLY: the scripted chat plays this run at `ceiling` (its own
/// line), or none. Set by `ddi_session::chat::set_scripted` (the shells'
/// `mpfakechat` and `--mp-dev-chat`).
pub fn set_scripted_ceiling(ceiling: Option<Level>) {
    SCRIPTED_CEILING.set(ceiling);
}

/// TESTING ONLY: the scripted chat's ceiling where it plays this run.
pub fn scripted_ceiling() -> Option<Level> {
    SCRIPTED_CEILING.get()
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
    /// TESTING ONLY (round TW7): the development script's named channel
    /// ([`DevLive::Channel`]), asked for by its login (`channel_id` empty
    /// until our server names it).
    pub named: bool,
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
            named: false,
        })
    }

    /// The same, in the online room `room` (its subscribe route's).
    pub fn in_room(self, room: Option<String>) -> RoomChat {
        RoomChat { room, ..self }
    }

    /// The room's chat as this device shows it (round TW7): the
    /// development script's named channel ([`DevLive::Channel`]) in the
    /// room's channel's place where the room shows chat, on this device
    /// alone (never what the room or its other members see); else the
    /// room's own.
    pub fn on_this_device(self) -> RoomChat {
        match dev_live() {
            Some(DevLive::Channel { login }) => RoomChat {
                ceiling: self.ceiling,
                channel_id: String::new(),
                login,
                room: None,
                named: true,
            },
            _ => self,
        }
    }

    /// CPU practice's chat (and a solo play's live one), TESTING ONLY:
    /// practice has no host's Twitch, so a player's shows none; where the
    /// scripted chat plays, its own line stands for the room's ceiling;
    /// else where the live chat's development script runs, its channel
    /// (one's own, or the one it names).
    pub fn practice() -> Option<RoomChat> {
        if let Some(ceiling) = scripted_ceiling() {
            return (ceiling != Level::Off).then(|| RoomChat {
                ceiling,
                channel_id: "scripted".into(),
                login: "scripted".into(),
                room: None,
                named: false,
            });
        }
        Some(match dev_live()? {
            DevLive::Own { channel_id, login } => RoomChat {
                ceiling: Level::Messages,
                channel_id,
                login,
                room: None,
                named: false,
            },
            DevLive::Channel { login } => RoomChat {
                ceiling: Level::Messages,
                channel_id: String::new(),
                login,
                room: None,
                named: true,
            },
        })
    }
}

/// Round DE: an online room's chat as this device keeps it connected for
/// the room, from the moment its member is in a room that shows chat
/// (the room's ceiling on, this device's "Twitch chat in rooms" on, its
/// Twitch access allowing chat) until they leave it or any of those
/// turns off ([`crate::state::room::RoomClient::chat_kept`]): the
/// channel, the level shown and where the emotes come from, which is
/// what the connection subscribes and reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeptChat {
    pub chat: RoomChat,
    /// The device's choice under the room's ceiling.
    pub level: Level,
    pub emotes_from: ddi_chat::EmotesFrom,
}

/// Who keeps a room's chat connected: told the room's wish and the room
/// clock's milliseconds at each tick (`None`: nothing to keep).
pub type ChatKeeper = Box<dyn Fn(Option<&KeptChat>, u64)>;

/// The shell's keeper of a room's chat (`ddi_session::chat` installs its
/// own with the live chat; `None`: none on this thread).
pub fn set_chat_keeper(keeper: Option<ChatKeeper>) {
    KEEPER.with(|k| *k.borrow_mut() = keeper);
}

/// An online room's client tells the keeper what its room's chat wants
/// now (each tick, and `None` once it is left).
pub fn keep_room_chat(want: Option<&KeptChat>, now_ms: u64) {
    KEEPER.with(|k| {
        if let Some(keeper) = k.borrow().as_ref() {
            keeper(want, now_ms);
        }
    });
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
        set_dev_live(None);
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
        assert_eq!(c.clone().on_this_device(), c, "the room's own without the script");
        set_scripted_ceiling(None);
        assert_eq!(RoomChat::practice(), None);
        set_scripted_ceiling(Some(Level::Emotes));
        assert_eq!(RoomChat::practice().map(|c| c.ceiling), Some(Level::Emotes));
        set_scripted_ceiling(None);
    }

    /// The live chat's development script: CPU practice's stages show one's
    /// own channel (`live`) or the one named (`live:<login>`), and an
    /// online room's stage shows the named one in the room's channel's
    /// place on this device, at the room's ceiling, through one's own
    /// route; plain `live` leaves a room's channel as it is.
    #[test]
    fn the_live_script_gives_practice_a_channel_and_names_one_for_rooms() {
        set_scripted_ceiling(None);
        set_dev_live(Some(DevLive::Own {
            channel_id: "1001".into(),
            login: "me".into(),
        }));
        let p = RoomChat::practice().unwrap();
        assert_eq!((&*p.channel_id, &*p.login, p.room.as_deref(), p.named), ("1001", "me", None, false));
        let room = RoomChat {
            ceiling: Level::Emotes,
            channel_id: "77".into(),
            login: "host".into(),
            room: Some("Room000001".into()),
            named: false,
        };
        assert_eq!(room.clone().on_this_device(), room);
        set_dev_live(Some(DevLive::Channel { login: "busy".into() }));
        let p = RoomChat::practice().unwrap();
        assert_eq!((&*p.channel_id, &*p.login, p.room.as_deref(), p.named), ("", "busy", None, true));
        let shown = room.on_this_device();
        assert_eq!(
            (shown.ceiling, &*shown.login, shown.room.as_deref(), shown.named),
            (Level::Emotes, "busy", None, true)
        );
        set_dev_live(None);
        assert_eq!(RoomChat::practice(), None);
        assert_eq!(named_login("Busy_1").as_deref(), Some("busy_1"));
        assert_eq!(named_login(""), None);
        assert_eq!(named_login("a/b"), None);
        assert_eq!(named_login(&"a".repeat(26)), None);
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
