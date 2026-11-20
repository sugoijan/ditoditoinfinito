//! A room host's Twitch chat beside a play (`docs/plans/multiplayer-twitch.md`,
//! C8, C16): where its events come from, advanced on the host's clock in
//! the session's tick, never the song's, and nothing of it judged, logged
//! or replayed. The shells' play view lays and draws what it holds
//! (`ddi_shell::chat`).
//!
//! Today its one source is the scripted chat (TESTING ONLY: the web's
//! `mpfakechat`, desktop's `--mp-dev-chat`); the device's socket to
//! Twitch (C2) is another round's. In a room's stage the room's own
//! ceiling caps it ([`room_ceiling`], round TW3), and a room that shows
//! no chat has no layer at all; outside a room the scripted chat's own
//! line stands for a ceiling, for tests.

use std::cell::RefCell;
use std::collections::VecDeque;

use ddi_chat::emotes::EmoteSets;
use ddi_chat::eventsub::{Event, Message};
use ddi_chat::script::{Player, Script};
use ddi_chat::{EmotesFrom, Level, Style};
use ddi_ui::state::room::StageSetup;
use ddi_ui::state::settings::Settings;

thread_local! {
    static SCRIPTED: RefCell<Option<Script>> = const { RefCell::new(None) };
}

/// TESTING ONLY: plays `script` as the chat of every play from now on
/// (the shells' `mpfakechat` and `--mp-dev-chat`); `None` stops it. It
/// is also the testing switch that shows the chat's rows
/// (`ddi_ui::state::twitch::chat_testing`).
pub fn set_scripted(script: Option<Script>) {
    ddi_ui::state::twitch::set_chat_testing(script.as_ref().map(Script::ceiling));
    SCRIPTED.with(|s| *s.borrow_mut() = script);
}

/// Whether a scripted chat is set.
pub fn scripted() -> bool {
    SCRIPTED.with(|s| s.borrow().is_some())
}

/// THE ROOM'S CEILING (C1), the one place it is answered: how much of the
/// host's chat a play may show. In a room's stage (`stage`), the room's
/// own (`RoomState::twitch`'s chat choice, carried by the stage's setup,
/// `StageSetup::chat`; Off where the room shows none); outside a room,
/// TESTING ONLY, the scripted chat's own line, and Off without one.
pub fn room_ceiling(stage: Option<&StageSetup>, script: Option<&Script>) -> Level {
    match stage {
        Some(stage) => stage.chat.as_ref().map_or(Level::Off, |c| c.ceiling),
        None => script.map_or(Level::Off, Script::ceiling),
    }
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
    /// The device's chat under `settings`, if any: its choice ("Twitch
    /// chat in rooms") over the scripted chat, the only source until the
    /// transports exist. The room's ceiling is applied as the play
    /// starts ([`ChatConfig::under_room`]).
    pub fn for_play(settings: &Settings) -> Option<ChatConfig> {
        let script = SCRIPTED.with(|s| s.borrow().clone())?;
        (settings.twitch_chat != Level::Off).then(|| ChatConfig {
            level: settings.twitch_chat,
            style: settings.chat_style,
            names: settings.chat_names,
            emotes_from: settings.chat_emotes_from,
            opacity: f32::from(settings.chat_opacity.min(100)) / 100.0,
            source: ChatSource::Script(script),
        })
    }

    /// This chat as a play shows it, under the room's ceiling
    /// ([`room_ceiling`]: `stage`'s in a room's stage): never more than
    /// the room allows, and none where that is Off.
    pub fn under_room(mut self, stage: Option<&StageSetup>) -> Option<ChatConfig> {
        let script = match &self.source {
            ChatSource::Script(s) => Some(s),
        };
        self.level = self.level.under(room_ceiling(stage, script));
        (self.level != Level::Off).then_some(self)
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

    /// Our server's API base its emotes' images are asked through after a
    /// direct failure: the global-emotes demo's (round TW5); none for a
    /// scripted chat's own pictures.
    pub fn api(&self) -> Option<String> {
        match &self.config.source {
            ChatSource::Script(s) => s.api(),
        }
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
        assert_eq!(room_ceiling(None, None), Level::Off);
        assert_eq!(ddi_ui::state::twitch::chat_testing(), None);
        set_scripted(Script::builtin("emotes"));
        assert_eq!(ddi_ui::state::twitch::chat_testing(), Some(Level::Emotes));
        let shown = |s: &Settings, stage: Option<&StageSetup>| {
            ChatConfig::for_play(s).and_then(|c| c.under_room(stage)).map(|c| c.level)
        };
        // Outside a room: the scripted chat's own line (tests only).
        assert_eq!(shown(&s, None), Some(Level::Emotes));
        // A room's stage: the room's ceiling, whatever the script says.
        let mut stage = setup();
        assert_eq!(shown(&s, Some(&stage)), None, "a room that shows none has no layer");
        stage.chat = Some(ddi_ui::state::twitch::RoomChat {
            ceiling: Level::Messages,
            channel_id: "1".into(),
            login: "host".into(),
        });
        assert_eq!(shown(&s, Some(&stage)), Some(Level::Messages));
        s.twitch_chat = Level::Emotes;
        assert_eq!(shown(&s, Some(&stage)), Some(Level::Emotes), "never above the device's");
        s.twitch_chat = Level::Off;
        assert_eq!(shown(&s, Some(&stage)), None);
        set_scripted(None);
        assert_eq!(ddi_ui::state::twitch::chat_testing(), None);
    }

    fn setup() -> StageSetup {
        StageSetup {
            stage: ddi_room::StageId(1),
            number: 1,
            of: 1,
            battle: ddi_room::BattlePreset::GaugeSurvivalV0,
            rules: Default::default(),
            me: ddi_room::ParticipantId(1),
            targeting: Default::default(),
            cpus: Vec::new(),
            dev_mines_at_beat: None,
            others: 0,
            watching: false,
            live: None,
            chat: None,
        }
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
