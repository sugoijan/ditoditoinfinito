//! A room host's Twitch chat beside a play (`docs/plans/multiplayer-twitch.md`,
//! C8, C16): where its events come from, advanced on the host's clock in
//! the session's tick, never the song's, and nothing of it judged, logged
//! or replayed. The shells' play view lays and draws what it holds
//! (`ddi_shell::chat`).
//!
//! Today its one source is the scripted chat (TESTING ONLY: the web's
//! `mpfakechat`, desktop's `--mp-dev-chat`), connected to any play; the
//! device's socket to Twitch (C2) is another round's, and will connect it
//! only in a room's stage.

use std::cell::RefCell;
use std::collections::VecDeque;

use ddi_chat::emotes::EmoteSets;
use ddi_chat::eventsub::{Event, Message};
use ddi_chat::script::{Player, Script};
use ddi_chat::{EmotesFrom, Level, Style};
use ddi_ui::state::settings::Settings;

thread_local! {
    static SCRIPTED: RefCell<Option<Script>> = const { RefCell::new(None) };
}

/// TESTING ONLY: plays `script` as the chat of every play from now on
/// (the shells' `mpfakechat` and `--mp-dev-chat`); `None` stops it.
pub fn set_scripted(script: Option<Script>) {
    SCRIPTED.with(|s| *s.borrow_mut() = script);
}

/// Whether a scripted chat is set.
pub fn scripted() -> bool {
    SCRIPTED.with(|s| s.borrow().is_some())
}

/// THE ROOM'S CEILING (C1), the one place it is answered: how much of the
/// host's chat the room allows. Rooms do not carry it yet (the model's
/// `RoomState::twitch`, revision 18, is another round's): until then it
/// is the scripted chat's own line, and Off without one. Its caller
/// passes the room's state once there is one.
pub fn room_ceiling(script: Option<&Script>) -> Level {
    script.map_or(Level::Off, Script::ceiling)
}

/// What a play's chat shows, from the device's settings.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatConfig {
    /// The device's choice under the room's ceiling.
    pub level: Level,
    pub style: Style,
    pub names: bool,
    pub emotes_from: EmotesFrom,
    /// The layer's opacity, `0..=1`.
    pub opacity: f32,
    /// Where the events come from.
    pub source: ChatSource,
}

/// Where a chat's events come from.
#[derive(Clone, Debug, PartialEq)]
pub enum ChatSource {
    /// The scripted chat (TESTING ONLY).
    Script(Script),
}

impl ChatConfig {
    /// The chat a play shows under `settings`, if any: the scripted
    /// chat's, in any play, where the device's choice under the room's
    /// ceiling is not Off. (A room's own chat is the transports' round's.)
    pub fn for_play(settings: &Settings) -> Option<ChatConfig> {
        let script = SCRIPTED.with(|s| s.borrow().clone())?;
        let level = settings.twitch_chat.under(room_ceiling(Some(&script)));
        (level != Level::Off).then(|| ChatConfig {
            level,
            style: settings.chat_style,
            names: settings.chat_names,
            emotes_from: settings.chat_emotes_from,
            opacity: f32::from(settings.chat_opacity.min(100)) / 100.0,
            source: ChatSource::Script(script),
        })
    }
}

/// Events kept for the view at most (a flood's second and more).
const KEPT: usize = 512;

/// A play's chat: its source, the events it gave and when (host seconds),
/// numbered so a view takes each once.
pub struct ChatLayer {
    config: ChatConfig,
    player: Option<Player>,
    /// The host second the source started at.
    start: Option<f64>,
    events: VecDeque<(u64, f64, Event)>,
    next: u64,
    sets: EmoteSets,
    /// Socket messages that could not be read.
    unread: u64,
}

impl ChatLayer {
    pub fn new(config: ChatConfig) -> ChatLayer {
        let (player, sets) = match &config.source {
            ChatSource::Script(s) => (Some(Player::new(s.clone())), s.sets()),
        };
        ChatLayer {
            config,
            player,
            start: None,
            events: VecDeque::new(),
            next: 1,
            sets,
            unread: 0,
        }
    }

    pub fn config(&self) -> &ChatConfig {
        &self.config
    }

    /// The emote sets words are matched by.
    pub fn sets(&self) -> &EmoteSets {
        &self.sets
    }

    /// The source's events due by host second `now`, kept for the view.
    pub fn advance(&mut self, now: f64) {
        let start = *self.start.get_or_insert(now);
        let Some(player) = self.player.as_mut() else {
            return;
        };
        for text in player.due(now - start) {
            match ddi_chat::eventsub::parse(&text) {
                Ok(Message::Notification(e)) => {
                    self.events.push_back((self.next, now, e));
                    self.next += 1;
                }
                Ok(_) => {}
                Err(_) => self.unread += 1,
            }
        }
        while self.events.len() > KEPT {
            self.events.pop_front();
        }
    }

    /// The events numbered after `seen`, with their host seconds.
    pub fn since(&self, seen: u64) -> impl Iterator<Item = (u64, f64, &Event)> {
        self.events
            .iter()
            .filter(move |(n, ..)| *n > seen)
            .map(|(n, t, e)| (*n, *t, e))
    }

    /// Events given so far, and messages that could not be read.
    pub fn counts(&self) -> (u64, u64) {
        (self.next - 1, self.unread)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_room_ceiling_caps_the_devices_choice() {
        let mut s = Settings {
            twitch_chat: Level::Messages,
            ..Settings::default()
        };
        set_scripted(None);
        assert_eq!(ChatConfig::for_play(&s), None);
        assert_eq!(room_ceiling(None), Level::Off);
        set_scripted(Script::builtin("emotes"));
        assert_eq!(ChatConfig::for_play(&s).map(|c| c.level), Some(Level::Emotes));
        s.twitch_chat = Level::Off;
        assert_eq!(ChatConfig::for_play(&s), None);
        set_scripted(None);
    }

    #[test]
    fn events_come_on_the_host_clock_numbered() {
        let config = ChatConfig {
            level: Level::Messages,
            style: Style::Across,
            names: false,
            emotes_from: EmotesFrom::TwitchAndOthers,
            opacity: 0.6,
            source: ChatSource::Script(Script::builtin("demo").unwrap()),
        };
        let mut c = ChatLayer::new(config);
        c.advance(100.0);
        assert_eq!(c.since(0).count(), 0);
        c.advance(100.7);
        let got: Vec<u64> = c.since(0).map(|(n, ..)| n).collect();
        assert_eq!(got, [1, 2]);
        assert_eq!(c.since(1).count(), 1);
        assert!(c.since(0).all(|(_, t, _)| t == 100.7));
    }
}
