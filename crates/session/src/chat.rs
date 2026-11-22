//! A room host's Twitch chat beside a play (`docs/plans/multiplayer-twitch.md`,
//! C8, C16): where its events come from, advanced on the host's clock in
//! the session's tick, never the song's, and nothing of it judged, logged
//! or replayed. The shells' play view lays and draws what it holds
//! (`ddi_shell::chat`).
//!
//! Its sources: the scripted chat (TESTING ONLY: the web's `mpfakechat`,
//! desktop's `--mp-dev-chat`), and since round TW6 the live chat: the
//! device's own socket to Twitch's EventSub (`ddi_chat::live`, the state
//! machine, driven here in the session's tick over a shell's [`LiveIo`],
//! the web's `WebSocket` and `fetch`, desktop's threads) and the
//! channel's emote sets (`ddi_chat::channel`). In an online room (round
//! DE) the connection is the room's, not a stage's: kept open from the
//! moment the member is in a room that shows chat until they leave it or
//! the chat is turned off ([`keep_for_room`], told by the room's client as
//! it ticks), lent to each stage's play and given back at its end, so the
//! chat is there from a stage's first frame; between stages nothing is
//! drawn and nothing kept but the last [`BUFFER_SECONDS`] of events (at
//! most [`BUFFER_MOST`]), which the next stage's first frame shows. Outside
//! a room a live chat (the development script's alone) starts with a play
//! and ends with it (the session dropped closes its sockets). For a player
//! (round TW7) it is a multiplayer feature alone:
//! an online room's stage, the room's channel under its ceiling (C1),
//! subscribed through `POST <api>/rooms/<id>/chat/subscribe`; a
//! singleplayer play shows none and connects to nothing, and CPU practice
//! has no host's Twitch (the maintainer, 2026-11-21: "singleplayer should
//! never require online connectivity"). Under the development script
//! alone (`mpfakechat=live`, `--mp-dev-chat live`: the signed-in person's
//! own channel, or with `live:<login>` the channel named, which also
//! stands in for an online room's channel on that device) solo plays and
//! CPU practice's stages show a live chat, through `POST
//! <api>/me/chat/subscribe`. In a room's stage the room's own ceiling caps
//! it ([`room_ceiling`], round TW3), and a room that shows no chat has no
//! layer at all; outside a room the scripted chat's own line stands for a
//! ceiling, for tests alone (TESTING ONLY).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use ddi_chat::cache::Via;
use ddi_chat::channel::ChannelSets;
use ddi_chat::emotes::EmoteSets;
use ddi_chat::eventsub::{Event, Message};
use ddi_chat::live::{LIVE_CHAT_V0, Live};
use ddi_chat::script::{Player, Script};
use ddi_chat::{EmotesFrom, Level, Style};
use ddi_ui::state::room::StageSetup;
use ddi_ui::state::settings::Settings;

pub use ddi_chat::live::SubscribeAnswer;

thread_local! {
    static SCRIPTED: RefCell<Option<Script>> = const { RefCell::new(None) };
    static LIVE: RefCell<Option<Rc<LiveSetup>>> = const { RefCell::new(None) };
    static ROOM_CHAT: RefCell<Keeper> = RefCell::new(Keeper::default());
}

/// Round DE: the seconds of events a room's kept connection holds for
/// the next stage's first frame (older ones are dropped as they age: late
/// chat is worse than missing chat).
pub const BUFFER_SECONDS: f64 = 2.0;
/// And how many at most (a second of new messages at the layer's rate).
pub const BUFFER_MOST: usize = 20;

/// A room's kept connection that gave up (a refusal of our server's, or
/// Twitch unreachable time after time: `ddi_chat::live`'s give-up) is
/// opened again this long after, milliseconds on the room's clock (review
/// 2026-11-22, chat-touch 1; the live machine's own longest wait between
/// two tries). Where the room itself ended it (its chat off for this
/// player, no member) it waits for the room's chat to change instead.
pub const ROOM_RETRY_MS: u64 = 60_000;

/// What a room's kept connection is for: whose channel by which route,
/// whether messages show (the moderation events subscribed too), and the
/// providers asked for the channel's emote sets.
#[derive(Clone, Debug, PartialEq)]
struct Key {
    want: LiveWant,
    messages: bool,
    from: EmotesFrom,
}

/// Round DE: a room's live chat between its stages.
#[derive(Default)]
struct Keeper {
    /// What the room wants kept now (none: outside a room, or its chat
    /// off).
    want: Option<Key>,
    /// The connection held between stages.
    held: Option<(Key, LiveChat)>,
    /// A play holds a connection of this key (lent, or its own).
    lent: Option<Key>,
    /// The machine's millisecond the room took the connection at.
    since: u64,
    /// Stages that took it.
    stages: u32,
    /// The last events, with the machine's millisecond they came at.
    buffer: VecDeque<(u64, Event)>,
    /// A held connection of this key gave up at this room clock's
    /// millisecond (none again until [`ROOM_RETRY_MS`] after), or for
    /// good (`None`: the room itself ended it).
    gave_up: Option<(Key, Option<u64>)>,
    /// The room clock's millisecond the room last told.
    room_ms: u64,
}

impl Keeper {
    /// The held connection, where it gave up, closed: the time kept, so
    /// that it is opened again only after [`ROOM_RETRY_MS`] (never where
    /// the room ended it). Whether one was.
    fn close_given_up(&mut self, now_ms: u64) -> bool {
        let Some((key, l)) = self.held.as_ref() else {
            return false;
        };
        let ddi_chat::live::Phase::GaveUp(why) = l.machine.phase() else {
            return false;
        };
        let (key, why) = (key.clone(), why.clone());
        let again = l.machine.trouble().map(|_| now_ms);
        self.close(&format!("it gave up: {why}"));
        self.gave_up = Some((key, again));
        true
    }

    /// Whether a connection of `key` may be opened at `now_ms`.
    fn may_open(&self, key: &Key, now_ms: u64) -> bool {
        match &self.gave_up {
            Some((k, at)) if k == key => at.is_some_and(|at| now_ms >= at.saturating_add(ROOM_RETRY_MS)),
            _ => true,
        }
    }

    /// The held connection closed (left, or its chat changed or off),
    /// its lifetime and totals told.
    fn close(&mut self, why: &str) {
        let Some((key, mut l)) = self.held.take() else {
            return;
        };
        let held_s = l.last_ms.saturating_sub(self.since) as f64 / 1000.0;
        l.io.report(&format!(
            "closed for the room ({why}): kept {held_s:.1} s, {} stages took it",
            self.stages
        ));
        l.end(key.messages, None);
        self.buffer.clear();
        self.stages = 0;
    }
}

fn key_of(want: &ddi_ui::state::twitch::KeptChat) -> Key {
    Key {
        want: LiveWant {
            channel_id: want.chat.channel_id.clone(),
            login: want.chat.login.clone(),
            room: want.chat.room.clone(),
            named: want.chat.named,
        },
        messages: want.level == Level::Messages,
        from: want.emotes_from,
    }
}

/// Round DE: what an online room's chat wants kept now (`want`, from its
/// client's tick, `RoomClient::chat_kept`; none once left or turned off)
/// at the room clock's `now_ms`: a connection opened for it where none is
/// held and no stage holds one, closed where the room no longer wants it
/// (or wants another), and driven between the stages: its events dropped
/// but the last [`BUFFER_SECONDS`], at most [`BUFFER_MOST`]. Nothing
/// without the shell's live chat ([`set_live`]).
pub fn keep_for_room(want: Option<&ddi_ui::state::twitch::KeptChat>, now_ms: u64) {
    let key = want.map(key_of);
    ROOM_CHAT.with_borrow_mut(|kp| {
        kp.want = key.clone();
        kp.room_ms = now_ms;
        // A give-up counts for its own chat alone.
        if kp.gave_up.as_ref().is_some_and(|(k, _)| Some(k) != key.as_ref()) {
            kp.gave_up = None;
        }
        if kp.held.as_ref().is_some_and(|(k, _)| Some(k) != key.as_ref()) {
            let why = if key.is_none() {
                "left the room, or its chat, this device's or the access's turned off"
            } else {
                "the room's chat changed: its channel or what shows"
            };
            kp.close(why);
        }
        kp.close_given_up(now_ms);
        if let Some(key) = key.as_ref()
            && kp.held.is_none()
            && kp.lent.as_ref() != Some(key)
            && kp.may_open(key, now_ms)
            && let Some(mut l) = LiveChat::new(key.want.clone(), key.from)
        {
            l.io.report("kept for the room: open until the room is left or its chat turned off");
            kp.since = l.ms(Driver::Room, now_ms);
            kp.stages = 0;
            kp.held = Some((key.clone(), l));
        }
        if let Some((key, l)) = kp.held.as_mut() {
            let ms = l.ms(Driver::Room, now_ms);
            let (events, _) = l.drive(ms, key.messages);
            kp.buffer.extend(events.into_iter().map(|e| (ms, e)));
            prune(&mut kp.buffer, ms);
        }
        kp.close_given_up(now_ms);
    });
}

/// The buffer within [`BUFFER_SECONDS`] of `ms` and [`BUFFER_MOST`].
fn prune(buffer: &mut VecDeque<(u64, Event)>, ms: u64) {
    let oldest = ms.saturating_sub((BUFFER_SECONDS * 1000.0) as u64);
    while buffer.front().is_some_and(|(t, _)| *t < oldest) || buffer.len() > BUFFER_MOST {
        buffer.pop_front();
    }
}

/// The room's kept connection for a play of `key`, where it is held for
/// it, with the events buffered; either way a play now holds one of it.
/// One that gave up is never lent (the play opens its own, as before the
/// room kept one): closed, opened again after [`ROOM_RETRY_MS`].
fn take_kept(key: &Key) -> Option<(LiveChat, Vec<Event>)> {
    ROOM_CHAT.with_borrow_mut(|kp| {
        kp.lent = Some(key.clone());
        let now = kp.room_ms;
        kp.close_given_up(now);
        match kp.held.take() {
            Some((k, l)) if k == *key => {
                kp.stages += 1;
                prune(&mut kp.buffer, l.last_ms);
                let events = kp.buffer.drain(..).map(|(_, e)| e).collect();
                Some((l, events))
            }
            other => {
                kp.held = other;
                None
            }
        }
    })
}

/// A play's connection of `key` at its end: kept by the room where the
/// room wants it now and holds none (its own, or a stage's that opened
/// one before the room could); else handed back to be closed.
fn give_back(key: Key, l: LiveChat) -> Option<LiveChat> {
    ROOM_CHAT.with_borrow_mut(|kp| {
        if kp.lent.as_ref() == Some(&key) {
            kp.lent = None;
        }
        if kp.want.as_ref() == Some(&key) && kp.held.is_none() {
            if kp.stages == 0 {
                kp.since = l.last_ms;
                kp.stages = 1;
            }
            kp.buffer.clear();
            kp.held = Some((key, l));
            None
        } else {
            Some(l)
        }
    })
}

/// Round DE: the room's kept connection between stages, for the debug
/// state (the web's `__DDI_DEBUG.chatKept`): its lifetime, whether a
/// stage holds it, the stages that took it, the events buffered, the
/// connection's report; none where no room wants one.
pub fn kept_report() -> Option<serde_json::Value> {
    ROOM_CHAT.with_borrow(|kp| {
        let want = kp.want.as_ref()?;
        Some(serde_json::json!({
            "lifetime": "room",
            "channel": want.want.login,
            "messages": want.messages,
            "held": kp.held.is_some(),
            "lent": kp.lent.is_some(),
            "stages": kp.stages,
            "kept_ms": kp.held.as_ref().map(|(_, l)| l.last_ms.saturating_sub(kp.since)),
            "buffered": kp.buffer.len(),
            "connection": kp.held.as_ref().map(|(_, l)| l.machine.report()),
        }))
    })
}

/// What a live chat reads: whose channel, and through which route.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveWant {
    /// The channel's Twitch user id (empty for a [`LiveWant::named`]
    /// channel until our server names it) and login.
    pub channel_id: String,
    pub login: String,
    /// The online room's id (its subscribe route, `POST
    /// <api>/rooms/<id>/chat/subscribe`); none, one's own route (`POST
    /// <api>/me/chat/subscribe`).
    pub room: Option<String>,
    /// TESTING ONLY (round TW7): the development script's named channel,
    /// asked for by its login on one's own route (administrators only).
    pub named: bool,
}

impl LiveWant {
    /// The subscribe route's path under our server's API base.
    pub fn route(&self) -> String {
        match &self.room {
            Some(room) => format!("/rooms/{room}/chat/subscribe"),
            None => "/me/chat/subscribe".into(),
        }
    }

    /// The subscribe route's body: the socket's session, whether messages
    /// show (the moderation events subscribed too) and, for a named
    /// channel, its login.
    pub fn body(&self, session_id: &str, messages: bool) -> String {
        let mut b = serde_json::json!({ "session_id": session_id, "messages": messages });
        if self.named {
            b["channel"] = self.login.clone().into();
        }
        b.to_string()
    }

    /// Which channel and route, for the report's first line (never a
    /// token or a session).
    pub fn describe(&self) -> String {
        let whose = match (&self.room, self.named) {
            (_, true) => "named by the development flag, administrators only",
            (Some(_), false) => "the room's",
            (None, false) => "your own",
        };
        format!("channel {} ({whose}), route {}", self.login, self.route())
    }
}

/// What a shell hands back to the live chat.
#[derive(Clone, Debug, PartialEq)]
pub enum LiveInput {
    /// Socket `n` opened.
    Opened(u32),
    /// A text frame on socket `n`.
    Text(u32, String),
    /// Socket `n` closed with a code (1006 where none came).
    Closed(u32, u16),
    /// Our server answered socket `n`'s subscribe
    /// (`ddi_chat::live::read_subscribe_answer`).
    Subscribed(u32, SubscribeAnswer),
    /// A provider's set read `key` ended (`Err`: `HTTP 404` or why it
    /// failed).
    SetRead(usize, Via, Result<String, String>),
}

/// A shell's side of the live chat: the sockets, our server's subscribe
/// route, the providers' sets, and the report's lines. Dropped with the
/// play, it closes whatever it still holds.
pub trait LiveIo {
    /// Opens socket `socket` at `url` (no credentials: Twitch's socket
    /// takes none).
    fn open(&mut self, socket: u32, url: &str);
    /// Closes it.
    fn close(&mut self, socket: u32);
    /// `POST <api><want.route()>` with its [`LiveWant::body`], signed in as
    /// the person (the web's cookie, desktop's session).
    fn subscribe(&mut self, socket: u32, session_id: &str, want: &LiveWant, messages: bool);
    /// Reads a provider's set at `url` (`via` the proxy: signed in).
    fn read_set(&mut self, key: usize, url: &str, via: Via);
    /// What came back since.
    fn poll(&mut self) -> Vec<LiveInput>;
    /// A line of the run's report (`ddi_chat::live::LOG_PREFIX` added by
    /// the shell: desktop's stderr, the web's console).
    fn report(&mut self, line: &str);
}

/// A shell's live chat: where the socket is, our server, and how its
/// [`LiveIo`] is made for a play.
pub struct LiveSetup {
    /// Twitch's EventSub socket (`ddi_chat::live::EVENTSUB_URL`), or,
    /// TESTING ONLY, a local fake's.
    pub url: String,
    /// Our server's API base (the subscribe route's, the emote proxy's).
    pub api: String,
    /// TESTING ONLY: the providers' addresses served from here
    /// (`<at><host>/<path>`, its proxy `<at>api`), never a provider.
    pub at: Option<String>,
    pub make: Box<dyn Fn() -> Box<dyn LiveIo>>,
}

/// The shell's live chat for every play from now on (`None`: no live chat
/// on this shell). Whether a play shows it is the settings' and the
/// room's ([`ChatConfig::for_play`], [`ChatConfig::under_room`]).
pub fn set_live(setup: Option<LiveSetup>) {
    // The room's kept connection is the old setup's: closed.
    ROOM_CHAT.with_borrow_mut(|kp| {
        kp.close("the shell's live chat changed");
        *kp = Keeper::default();
    });
    let keeper = setup.is_some();
    LIVE.with(|l| *l.borrow_mut() = setup.map(Rc::new));
    ddi_ui::state::twitch::set_chat_keeper(
        keeper.then(|| Box::new(keep_for_room) as ddi_ui::state::twitch::ChatKeeper),
    );
}

fn live_setup() -> Option<Rc<LiveSetup>> {
    LIVE.with(|l| l.borrow().clone())
}

/// Whether a play may show a live chat: the shell has one.
pub fn live_possible() -> bool {
    live_setup().is_some()
}

/// TESTING ONLY: plays `script` as the chat of every play from now on
/// (the shells' `mpfakechat` and `--mp-dev-chat`); `None` stops it. Its
/// own line stands for CPU practice's ceiling
/// (`ddi_ui::state::twitch::scripted_ceiling`).
pub fn set_scripted(script: Option<Script>) {
    ddi_ui::state::twitch::set_scripted_ceiling(script.as_ref().map(Script::ceiling));
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
    /// The live chat, whose channel the play's start decides
    /// ([`ChatConfig::under_room`]).
    Room,
    /// The live chat of this channel (round TW6).
    Live(LiveWant),
}

impl ChatConfig {
    /// The device's chat under `settings`, if any: its choice ("Twitch
    /// chat in rooms") over the scripted chat where one plays, else the
    /// live chat where it may run ([`live_possible`]). The room's ceiling
    /// and channel are applied as the play starts
    /// ([`ChatConfig::under_room`]).
    pub fn for_play(settings: &Settings) -> Option<ChatConfig> {
        if settings.twitch_chat == Level::Off {
            return None;
        }
        let source = match SCRIPTED.with(|s| s.borrow().clone()) {
            Some(script) => ChatSource::Script(script),
            None if live_possible() => ChatSource::Room,
            None => return None,
        };
        Some(ChatConfig {
            level: settings.twitch_chat,
            style: settings.chat_style,
            names: settings.chat_names,
            emotes_from: settings.chat_emotes_from,
            opacity: f32::from(settings.chat_opacity.min(100)) / 100.0,
            source,
        })
    }

    /// This chat as a play shows it, under the room's ceiling
    /// ([`room_ceiling`]: `stage`'s in a room's stage): never more than
    /// the room allows, and none where that is Off. The live chat reads
    /// the stage's room's channel (as this device shows it,
    /// `RoomChat::on_this_device`); for a player, nothing else: CPU
    /// practice has no host's Twitch and a singleplayer play (`stage`
    /// none) shows none and connects to nothing (the maintainer,
    /// 2026-11-21: "singleplayer should never require online
    /// connectivity"). Only the development script gives CPU practice's
    /// stages and solo plays a channel (`RoomChat::practice`: one's own,
    /// or the one it names). None for the audience (C13).
    pub fn under_room(mut self, stage: Option<&StageSetup>) -> Option<ChatConfig> {
        match &self.source {
            ChatSource::Script(s) => {
                self.level = self.level.under(room_ceiling(stage, Some(s)));
            }
            ChatSource::Room | ChatSource::Live(_) => {
                let chat = match stage {
                    Some(stage) => stage.chat.clone(),
                    // A solo play: the development script's alone.
                    None => ddi_ui::state::twitch::RoomChat::practice(),
                }
                    .filter(|c| c.channel_id != "scripted")
                    .filter(|c| c.named || !c.channel_id.is_empty())?;
                self.level = self.level.under(chat.ceiling);
                self.source = ChatSource::Live(LiveWant {
                    channel_id: chat.channel_id,
                    login: chat.login,
                    room: chat.room,
                    named: chat.named,
                });
            }
        }
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
    /// The live chat's connection (round TW6).
    live: Option<LiveChat>,
    /// What the connection is for, which a room keeps between stages
    /// (round DE), and whether it was the room's when the play took it.
    key: Option<Key>,
    kept: bool,
    /// The room's buffered events, given at the first frame (none once
    /// given).
    handed: Option<Vec<Event>>,
    /// The chat messages received when the play took the connection.
    messages_before: u64,
    /// The host second the source started at.
    start: Option<f64>,
    events: VecDeque<(u64, f64, Event)>,
    next: u64,
    sets: EmoteSets,
    /// Bumped whenever [`ChatLayer::sets`] changed (a provider answered).
    sets_version: u64,
    /// Socket messages that could not be read.
    unread: u64,
    /// What the view last showed of it, for the live chat's last lines.
    view_note: RefCell<Option<ViewNote>>,
}

/// What a play's view showed of its chat (`ddi_shell::chat`), told back
/// for the live chat's report at the play's end.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewNote {
    /// The overlay's counts: shown, left out by each filter, dropped by
    /// each budget, removed by moderation.
    pub stats: ddi_chat::overlay::Stats,
    /// Images ready, in flight, failed; hosts through the proxy.
    pub images: (usize, usize, usize, usize),
    /// The hosts that needed our server's proxy.
    pub proxied: Vec<String>,
    /// Animated emotes ready.
    pub animated: usize,
    /// The rules' new messages a second and emotes a message
    /// (`chat-overlay-v2`'s), for the lines.
    pub per_second: usize,
    pub cut_at: usize,
}

/// A play's live chat: the connection's state machine, the channel's
/// emote sets and the shell's side.
struct LiveChat {
    want: LiveWant,
    machine: Live,
    sets: ChannelSets,
    /// The sets are read once the channel's id is known (a named
    /// channel's, round TW7: from our server's answer).
    sets_ready: bool,
    emotes_from: EmotesFrom,
    api: String,
    at: Option<String>,
    sets_seen: u64,
    sets_told: bool,
    io: Box<dyn LiveIo>,
    started: bool,
    /// The machine's millisecond last driven to.
    last_ms: u64,
    /// Who drives its clock now and the machine's milliseconds less the
    /// driver's (round DE: a room's connection is driven by the room's
    /// clock between stages and by a play's host clock in them).
    clock: Option<(Driver, i128)>,
}

/// Who drives a live chat's clock (round DE).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Driver {
    /// A play's host seconds.
    Play,
    /// The room clock's milliseconds.
    Room,
}

impl LiveChat {
    fn new(want: LiveWant, from: EmotesFrom) -> Option<LiveChat> {
        let setup = live_setup()?;
        let mut io = (setup.make)();
        io.report(&want.describe());
        Some(LiveChat {
            machine: Live::new(LIVE_CHAT_V0, setup.url.clone()),
            sets: ChannelSets::new(&want.channel_id, from, Some(setup.api.clone()), setup.at.clone()),
            sets_ready: !want.channel_id.is_empty(),
            emotes_from: from,
            api: setup.api.clone(),
            at: setup.at.clone(),
            want,
            sets_seen: 0,
            sets_told: false,
            io,
            started: false,
            last_ms: 0,
            clock: None,
        })
    }

    /// The machine's millisecond for `driver`'s `raw` (never backwards):
    /// a new driver goes on from where the last one left it.
    fn ms(&mut self, driver: Driver, raw: u64) -> u64 {
        let off = match self.clock {
            Some((d, off)) if d == driver => off,
            _ => {
                let off = if self.started {
                    i128::from(self.last_ms) - i128::from(raw)
                } else {
                    0
                };
                self.clock = Some((driver, off));
                off
            }
        };
        let ms = (i128::from(raw) + off).max(i128::from(self.last_ms)).max(0);
        self.last_ms = u64::try_from(ms).unwrap_or(u64::MAX);
        self.last_ms
    }

    /// Drives the connection to the machine's `ms`: what the shell's side
    /// brought since, its timers, what it is to do next. The chat's events
    /// received, and whether the emote sets changed.
    fn drive(&mut self, ms: u64, messages: bool) -> (Vec<Event>, bool) {
        if !self.started {
            self.started = true;
            self.machine.start(ms);
            if self.sets_ready {
                for r in self.sets.start() {
                    self.io.read_set(r.key, &r.url, r.via);
                }
            }
        }
        for input in self.io.poll() {
            match input {
                LiveInput::Opened(s) => self.machine.opened(s, ms),
                LiveInput::Text(s, text) => self.machine.text(s, &text, ms),
                LiveInput::Closed(s, code) => self.machine.closed(s, code, ms),
                LiveInput::Subscribed(s, answer) => self.machine.subscribed(s, answer, ms),
                LiveInput::SetRead(key, via, answer) => {
                    self.sets.answered(key, via, answer);
                    for r in self.sets.start() {
                        self.io.read_set(r.key, &r.url, r.via);
                    }
                }
            }
        }
        self.machine.tick(ms);
        // A named channel's id, once our server found it: its sets read
        // from now on (round TW7).
        if !self.sets_ready
            && let Some(c) = self.machine.channel()
        {
            self.want.channel_id = c.id.clone();
            self.sets = ChannelSets::new(&c.id, self.emotes_from, Some(self.api.clone()), self.at.clone());
            self.sets_ready = true;
            for r in self.sets.start() {
                self.io.read_set(r.key, &r.url, r.via);
            }
        }
        self.flush(messages);
        let events = self.machine.take_events();
        let changed = self.sets.version() != self.sets_seen;
        self.sets_seen = self.sets.version();
        if self.sets_ready && !self.sets_told && self.sets.done() {
            self.sets_told = true;
            for line in self.sets.lines() {
                self.io.report(&line);
            }
        }
        (events, changed)
    }

    /// Stops it: every socket closed, the last lines told (what was
    /// received, the shapes not understood, what `view` showed, the emote
    /// sets).
    fn end(&mut self, messages: bool, view: Option<&ViewNote>) {
        let now = self.last_ms;
        self.machine.stop(now);
        self.flush(messages);
        let r = self.machine.received();
        self.io.report(&format!(
            "received {} frames: {} chat messages, {} deletes, {} user clears, {} clears, {} other; {} keepalives, {} duplicates, {} not understood",
            r.frames,
            r.messages,
            r.deletes,
            r.clear_users,
            r.clears,
            r.other_events,
            r.keepalive,
            r.duplicate,
            self.machine.report()["not_understood"]["count"],
        ));
        for sample in self.machine.report()["not_understood"]["samples"]
            .as_array()
            .into_iter()
            .flatten()
        {
            self.io.report(&format!("not understood: {sample}"));
        }
        if let Some(n) = view {
            self.report_view(n);
        }
        for line in self.sets.lines() {
            self.io.report(&line);
        }
    }

    /// What a play's view showed of the chat, in two lines.
    fn report_view(&mut self, n: &ViewNote) {
        let s = n.stats;
        self.io.report(&format!(
            "shown: {} items from {} messages; left out {} (commands {}, links {}, cheers {}, shared chat {}, nothing to show {}); dropped: screen full {}, {} a second {}, a chatter's 2 s {}, frames late {}, no room {}, no image {}; taken off by moderation {}; emotes past {} {}",
            s.shown,
            s.messages,
            s.left_out,
            s.left.command,
            s.left.link,
            s.left.cheer,
            s.left.shared,
            s.left.empty,
            s.full,
            n.per_second,
            s.rate,
            s.chatter,
            s.shed,
            s.no_room,
            s.no_image,
            s.removed,
            n.cut_at,
            s.cut,
        ));
        let (ready, flying, failed, proxied) = n.images;
        self.io.report(&format!(
            "images: {ready} ready ({} animated), {flying} in flight, {failed} failed, {proxied} hosts through our proxy{}",
            n.animated,
            if n.proxied.is_empty() {
                String::new()
            } else {
                format!(" ({})", n.proxied.join(", "))
            }
        ));
    }

    /// Hands the machine's wishes and lines to the shell.
    fn flush(&mut self, messages: bool) {
        for d in self.machine.take_do() {
            match d {
                ddi_chat::live::Do::Open { socket, url } => self.io.open(socket, &url),
                ddi_chat::live::Do::Close { socket } => self.io.close(socket),
                ddi_chat::live::Do::Subscribe { socket, session_id } => {
                    self.io.subscribe(socket, &session_id, &self.want, messages)
                }
            }
        }
        for line in self.machine.take_lines() {
            self.io.report(&line);
        }
    }
}

impl Drop for ChatLayer {
    /// The play ended: in a room, the connection given back to the room
    /// (round DE: kept between stages where it still wants it), the
    /// stage's lines told; else its sockets closed, its last lines told.
    fn drop(&mut self) {
        let messages = self.config.level == Level::Messages;
        let Some(l) = self.live.take() else {
            return;
        };
        let view = self.view_note.borrow().clone();
        let back = match self.key.take() {
            Some(key) => {
                let given = l.machine.received().messages.saturating_sub(self.messages_before);
                let l2 = give_back(key, l);
                match l2 {
                    Some(l2) => Some(l2),
                    None => {
                        // Kept: the stage's own lines through the room's.
                        ROOM_CHAT.with_borrow_mut(|kp| {
                            if let Some((_, l)) = kp.held.as_mut() {
                                l.io.report(&format!(
                                    "the stage ended: {given} chat messages during it; kept for the room"
                                ));
                                if let Some(n) = view.as_ref() {
                                    l.report_view(n);
                                }
                            }
                        });
                        None
                    }
                }
            }
            None => Some(l),
        };
        if let Some(mut l) = back {
            l.end(messages, view.as_ref());
        }
    }
}

impl ChatLayer {
    pub fn new(config: ChatConfig) -> ChatLayer {
        let (player, sets, live) = match &config.source {
            ChatSource::Script(s) => (Some(Player::new(s.clone())), s.sets(), None),
            ChatSource::Live(want) => {
                // Round DE: the room's kept connection where it has one
                // for this chat; else one of the play's own.
                let k = Key {
                    want: want.clone(),
                    messages: config.level == Level::Messages,
                    from: config.emotes_from,
                };
                let (live, handed, kept) = match take_kept(&k) {
                    Some((l, events)) => (Some(l), events, true),
                    None => (LiveChat::new(want.clone(), config.emotes_from), Vec::new(), false),
                };
                let sets = live.as_ref().map(|l| l.sets.sets()).unwrap_or_default();
                let before = live.as_ref().map_or(0, |l| l.machine.received().messages);
                (None, sets, live.map(|l| (l, k, kept, handed, before)))
            }
            ChatSource::Room => (None, EmoteSets::default(), None),
        };
        let (live, key, kept, handed, messages_before) = match live {
            Some((l, k, kept, handed, before)) => (Some(l), Some(k), kept, Some(handed), before),
            None => (None, None, false, None, 0),
        };
        ChatLayer {
            config,
            player,
            live,
            key,
            kept,
            handed,
            messages_before,
            start: None,
            events: VecDeque::new(),
            next: 1,
            sets,
            sets_version: 0,
            unread: 0,
            view_note: RefCell::new(None),
        }
    }

    /// The view tells what it showed (each frame of a live chat).
    pub fn note_view(&self, note: ViewNote) {
        *self.view_note.borrow_mut() = Some(note);
    }

    /// Bumped whenever the emote sets changed (a provider's set read):
    /// the view takes [`ChatLayer::sets`] again.
    pub fn sets_version(&self) -> u64 {
        self.sets_version
    }

    /// Whether this is the live chat (round TW6).
    pub fn is_live(&self) -> bool {
        self.live.is_some()
    }

    /// Why the live chat could not show, where it could not (round TW7:
    /// `ddi_chat::live::Live::trouble`), for one quiet line after the
    /// stage; none for any other source.
    pub fn trouble(&self) -> Option<ddi_chat::live::Trouble> {
        self.live.as_ref().and_then(|l| l.machine.trouble())
    }

    /// The live chat's report for the debug state (`__DDI_DEBUG.chat.live`):
    /// the connection's (`ddi_chat::live::Live::report`), the emote sets'
    /// by provider, whose channel and which route; none for another
    /// source. Never a token, a session or a person's chat.
    pub fn live_report(&self) -> Option<serde_json::Value> {
        let l = self.live.as_ref()?;
        Some(serde_json::json!({
            "channel": l.want.login,
            "route": if l.want.room.is_some() { "room" } else { "own" },
            "named": l.want.named,
            "level": format!("{:?}", self.config.level),
            // Round DE: the room's connection (kept between its stages)
            // or this play's own.
            "lifetime": if self.key.is_some() && (self.kept || ROOM_CHAT.with_borrow(|kp| kp.want.is_some())) { "room" } else { "play" },
            "taken_from_room": self.kept,
            "connection": l.machine.report(),
            "emote_sets": l.sets.report(),
            "events_given": self.next - 1,
        }))
    }

    pub fn config(&self) -> &ChatConfig {
        &self.config
    }

    /// The emote sets words are matched by.
    pub fn sets(&self) -> &EmoteSets {
        &self.sets
    }

    /// Our server's API base its emotes' images are asked through after a
    /// direct failure: the global-emotes demo's (round TW5) and the live
    /// chat's; none for a scripted chat's own pictures.
    pub fn api(&self) -> Option<String> {
        match &self.config.source {
            ChatSource::Script(s) => s.api(),
            ChatSource::Live(_) | ChatSource::Room => {
                self.live.as_ref().and_then(|l| l.sets.api().map(str::to_string))
            }
        }
    }

    /// The source's events due by host second `now`, kept for the view:
    /// the scripted chat's, or the live chat's connection driven (what the
    /// shell's side brought since, its timers, what it is to do next).
    pub fn advance(&mut self, now: f64) {
        let start = *self.start.get_or_insert(now);
        if let Some(player) = self.player.as_mut() {
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
        }
        let messages = self.config.level == Level::Messages;
        if let Some(l) = self.live.as_mut() {
            let ms = l.ms(Driver::Play, (now.max(0.0) * 1000.0) as u64);
            // The room's last events, at the play's first frame.
            let handed = self.handed.take();
            if self.kept && let Some(h) = handed.as_ref() {
                l.io.report(&format!(
                    "a stage took the room's connection ({}, {} events from the last {BUFFER_SECONDS} s)",
                    l.machine.state(),
                    h.len()
                ));
            }
            for e in handed.into_iter().flatten() {
                self.events.push_back((self.next, now, e));
                self.next += 1;
            }
            let (events, changed) = l.drive(ms, messages);
            for e in events {
                self.events.push_back((self.next, now, e));
                self.next += 1;
            }
            if changed {
                self.sets = l.sets.sets();
                self.sets_version += 1;
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
pub(crate) mod tests {
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
        assert_eq!(ddi_ui::state::twitch::scripted_ceiling(), None);
        set_scripted(Script::builtin("emotes"));
        assert_eq!(ddi_ui::state::twitch::scripted_ceiling(), Some(Level::Emotes));
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
            room: None,
            named: false,
        });
        assert_eq!(shown(&s, Some(&stage)), Some(Level::Messages));
        s.twitch_chat = Level::Emotes;
        assert_eq!(shown(&s, Some(&stage)), Some(Level::Emotes), "never above the device's");
        s.twitch_chat = Level::Off;
        assert_eq!(shown(&s, Some(&stage)), None);
        set_scripted(None);
        assert_eq!(ddi_ui::state::twitch::scripted_ceiling(), None);
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
            start: None,
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

    /// A shell's side of the live chat for tests (round TW6): every socket
    /// welcomes at once, every subscribe is answered enabled, and once
    /// subscribed each poll brings a chat message; what was asked is kept.
    #[derive(Default)]
    pub(crate) struct FakeIoState {
        pub opened: Vec<(u32, String)>,
        pub closed: Vec<u32>,
        pub subscribed: Vec<(u32, String, LiveWant, bool)>,
        pub reads: Vec<String>,
        pub lines: Vec<String>,
        /// Our server's refusals to answer the next subscribes with, in
        /// order (none left: each subscribed).
        pub refuse: VecDeque<String>,
        queue: Vec<LiveInput>,
        live: Option<u32>,
        n: u64,
    }

    pub(crate) struct FakeIo(pub Rc<RefCell<FakeIoState>>);

    impl LiveIo for FakeIo {
        fn open(&mut self, socket: u32, url: &str) {
            let mut s = self.0.borrow_mut();
            s.opened.push((socket, url.to_string()));
            s.queue.push(LiveInput::Opened(socket));
            let welcome = format!(
                r#"{{"metadata":{{"message_id":"w{socket}","message_type":"session_welcome"}},"payload":{{"session":{{"id":"session-{socket}","keepalive_timeout_seconds":10}}}}}}"#
            );
            s.queue.push(LiveInput::Text(socket, welcome));
        }
        fn close(&mut self, socket: u32) {
            self.0.borrow_mut().closed.push(socket);
        }
        fn subscribe(&mut self, socket: u32, session_id: &str, want: &LiveWant, messages: bool) {
            let mut s = self.0.borrow_mut();
            s.subscribed.push((socket, session_id.to_string(), want.clone(), messages));
            if let Some(word) = s.refuse.pop_front() {
                s.queue.push(LiveInput::Subscribed(socket, SubscribeAnswer::Refused(word)));
                return;
            }
            let subs = r#"[{"type":"channel.chat.message","outcome":"enabled","http":202,"cost":0,"total":1,"total_cost":0,"max_total_cost":10}]"#;
            // A named channel's found as our server finds it (round TW7).
            let body = if want.named {
                format!(r#"{{"channel":"{}","channel_id":"id-{}","subscriptions":{subs}}}"#, want.login, want.login)
            } else {
                format!(r#"{{"subscriptions":{subs}}}"#)
            };
            s.queue.push(LiveInput::Subscribed(socket, ddi_chat::live::read_subscribe_answer(200, &body)));
            s.live = Some(socket);
        }
        fn read_set(&mut self, key: usize, url: &str, via: Via) {
            let mut s = self.0.borrow_mut();
            s.reads.push(url.to_string());
            s.queue.push(LiveInput::SetRead(key, via, Err("HTTP 404".into())));
        }
        fn poll(&mut self) -> Vec<LiveInput> {
            let mut s = self.0.borrow_mut();
            if let Some(socket) = s.live {
                s.n += 1;
                let n = s.n;
                let chat = format!(
                    r#"{{"metadata":{{"message_id":"n{n}","message_type":"notification","subscription_type":"channel.chat.message"}},"payload":{{"event":{{"broadcaster_user_id":"1","chatter_user_id":"{c}","chatter_user_login":"v","chatter_user_name":"V","message_id":"m{n}","message_type":"text","message":{{"text":"hi Kappa","fragments":[{{"type":"text","text":"hi "}},{{"type":"emote","text":"Kappa","emote":{{"id":"25"}}}}]}}}}}}}}"#,
                    c = n % 7
                );
                s.queue.push(LiveInput::Text(socket, chat));
            }
            std::mem::take(&mut s.queue)
        }
        fn report(&mut self, line: &str) {
            self.0.borrow_mut().lines.push(line.to_string());
        }
    }

    /// Installs the fake as the shell's live chat; its state.
    pub(crate) fn fake_live() -> Rc<RefCell<FakeIoState>> {
        let state = Rc::new(RefCell::new(FakeIoState::default()));
        let shared = state.clone();
        set_live(Some(LiveSetup {
            url: "ws://127.0.0.1:9/eventsub".into(),
            api: "https://api.test".into(),
            at: None,
            make: Box::new(move || Box::new(FakeIo(shared.clone()))),
        }));
        state
    }

    fn own() -> Option<ddi_ui::state::twitch::DevLive> {
        Some(ddi_ui::state::twitch::DevLive::Own {
            channel_id: "1001".into(),
            login: "me".into(),
        })
    }

    /// A CPU practice stage as the room orders it under the live chat's
    /// development script.
    fn practice_stage() -> StageSetup {
        StageSetup {
            chat: ddi_ui::state::twitch::RoomChat::practice(),
            ..setup()
        }
    }

    /// Where the live chat runs and whose channel it reads (rounds TW6 and
    /// TW7): for a player, an online room's stage alone, the room's
    /// channel through its route where the device's option and the room
    /// allow it (a stage of a room that shows none: nothing); a solo play
    /// and CPU practice nothing; under the development script both show
    /// one's own channel; nothing with the option at its default.
    #[test]
    fn the_live_chat_reads_the_rooms_channel_or_ones_own() {
        set_scripted(None);
        ddi_ui::state::twitch::set_dev_live(None);
        let _state = fake_live();
        let s = Settings {
            twitch_chat: Level::Messages,
            ..Settings::default()
        };
        let c = ChatConfig::for_play(&s).unwrap();
        assert_eq!(c.source, ChatSource::Room);
        assert_eq!(c.clone().under_room(None), None, "a player's solo play: none");
        assert_eq!(c.clone().under_room(Some(&practice_stage())), None, "practice: none for a player");
        ddi_ui::state::twitch::set_dev_live(own());
        for stage in [None, Some(practice_stage())] {
            let shown = c.clone().under_room(stage.as_ref()).unwrap();
            let ChatSource::Live(want) = &shown.source else { panic!() };
            assert_eq!((&*want.channel_id, want.route()), ("1001", "/me/chat/subscribe".to_string()));
            assert_eq!(want.body("s1", false), r#"{"messages":false,"session_id":"s1"}"#);
        }
        ddi_ui::state::twitch::set_dev_live(None);
        let mut stage = setup();
        assert_eq!(c.clone().under_room(Some(&stage)), None, "a room that shows none");
        stage.chat = Some(ddi_ui::state::twitch::RoomChat {
            ceiling: Level::Emotes,
            channel_id: "77".into(),
            login: "host".into(),
            room: Some("Room000001".into()),
            named: false,
        });
        let in_room = c.clone().under_room(Some(&stage)).unwrap();
        assert_eq!(in_room.level, Level::Emotes);
        let ChatSource::Live(want) = &in_room.source else { panic!() };
        assert_eq!(want.route(), "/rooms/Room000001/chat/subscribe");
        assert_eq!(
            ChatConfig::for_play(&Settings::default()),
            None,
            "the option at its default: nothing"
        );
        set_live(None);
    }

    /// The development script's named channel (round TW7): asked for by its
    /// login on one's own route, in a solo play, in practice and in an
    /// online room's stage in the room's channel's place, at the room's
    /// ceiling.
    #[test]
    fn a_named_channel_is_asked_for_by_its_login() {
        set_scripted(None);
        let _state = fake_live();
        ddi_ui::state::twitch::set_dev_live(Some(ddi_ui::state::twitch::DevLive::Channel {
            login: "busy".into(),
        }));
        let c = ChatConfig::for_play(&Settings {
            twitch_chat: Level::Messages,
            ..Settings::default()
        })
        .unwrap();
        let solo = c.clone().under_room(None).unwrap();
        let ChatSource::Live(want) = &solo.source else { panic!() };
        assert_eq!((want.named, &*want.login), (true, "busy"));
        let practice = c.clone().under_room(Some(&practice_stage())).unwrap();
        let ChatSource::Live(want) = &practice.source else { panic!() };
        assert_eq!((want.named, &*want.login, want.route()), (true, "busy", "/me/chat/subscribe".to_string()));
        assert_eq!(
            want.body("s1", true),
            r#"{"channel":"busy","messages":true,"session_id":"s1"}"#
        );
        let room = ddi_ui::state::twitch::RoomChat {
            ceiling: Level::Emotes,
            channel_id: "77".into(),
            login: "host".into(),
            room: Some("Room000001".into()),
            named: false,
        };
        let stage = StageSetup {
            chat: Some(room.on_this_device()),
            ..setup()
        };
        let online = c.under_room(Some(&stage)).unwrap();
        assert_eq!(online.level, Level::Emotes);
        let ChatSource::Live(want) = &online.source else { panic!() };
        assert_eq!((want.named, &*want.login, want.room.as_deref()), (true, "busy", None));
        // Its emote sets wait for the channel's id, which our server's
        // answer gives; the report names the channel.
        let state = fake_live();
        let mut layer = ChatLayer::new(practice);
        layer.advance(1.0);
        assert!(state.borrow().reads.is_empty(), "no set read before the id is known");
        layer.advance(1.1);
        layer.advance(1.2);
        {
            let st = state.borrow();
            assert_eq!(st.subscribed[0].2.login, "busy");
            assert!(st.reads.iter().any(|u| u.contains("id-busy")), "{:?}", st.reads);
            assert_eq!(
                st.lines[0],
                "channel busy (named by the development flag, administrators only), route /me/chat/subscribe"
            );
            assert!(st.lines.iter().any(|l| l.ends_with("channel busy is Twitch user id-busy")));
        }
        assert_eq!(layer.live_report().unwrap()["named"], true);
        drop(layer);
        ddi_ui::state::twitch::set_dev_live(None);
        set_live(None);
    }

    /// Round DE: an online room's chat is the room's connection, not a
    /// stage's: opened once the room wants it (its client's tick, through
    /// the hook `set_live` installs), driven between the stages with only
    /// its last events buffered (at most [`BUFFER_MOST`], none older than
    /// [`BUFFER_SECONDS`]), lent to each stage's play, whose first frame
    /// already has the chat, given back at its end; one socket for two
    /// stages, closed when the room is left. Outside a room a play's live
    /// chat (the development script's) is still its own, closed with it.
    #[test]
    fn a_rooms_chat_stays_connected_across_its_stages() {
        use ddi_ui::state::twitch::{KeptChat, RoomChat, keep_room_chat};
        set_scripted(None);
        ddi_ui::state::twitch::set_dev_live(None);
        let state = fake_live();
        let room = RoomChat {
            ceiling: Level::Messages,
            channel_id: "77".into(),
            login: "host".into(),
            room: Some("Room000001".into()),
            named: false,
        };
        let want = KeptChat {
            chat: room.clone(),
            level: Level::Messages,
            emotes_from: EmotesFrom::TwitchAndOthers,
        };
        let mut t = 1_700_000_000_000u64;
        let room_ticks = |n: usize, t: &mut u64, want: Option<&KeptChat>| {
            for _ in 0..n {
                keep_room_chat(want, *t);
                *t += 100;
            }
        };
        room_ticks(3, &mut t, Some(&want));
        {
            let s = state.borrow();
            assert_eq!(s.opened, [(1, "ws://127.0.0.1:9/eventsub".to_string())]);
            assert_eq!(s.subscribed.len(), 1);
            assert!(s.subscribed[0].3, "messages: the moderation events too");
            assert!(s.lines.iter().any(|l| l.starts_with("kept for the room")), "{:?}", s.lines);
        }
        // Five seconds between stages: a message a tick, the last two
        // seconds' kept at most, never more than the cap.
        room_ticks(50, &mut t, Some(&want));
        let r = kept_report().unwrap();
        assert_eq!((r["held"].as_bool(), r["lent"].as_bool(), r["stages"].as_u64()), (Some(true), Some(false), Some(0)));
        assert_eq!(r["buffered"].as_u64(), Some(BUFFER_MOST as u64));
        assert_eq!(r["connection"]["state"], "live");
        let settings = Settings {
            twitch_chat: Level::Messages,
            ..Settings::default()
        };
        let stage = StageSetup {
            chat: Some(room.clone()),
            ..setup()
        };
        for n in 1..=2u64 {
            let config = ChatConfig::for_play(&settings).unwrap().under_room(Some(&stage)).unwrap();
            let mut layer = ChatLayer::new(config);
            // The room's ticks during the play open nothing more.
            room_ticks(3, &mut t, Some(&want));
            let first = 500.0 * n as f64;
            layer.advance(first);
            assert!(
                layer.since(0).filter(|(_, at, _)| *at == first).count() > BUFFER_MOST,
                "the chat at the stage's first frame: the room's last events and what came since"
            );
            let r = layer.live_report().unwrap();
            assert_eq!((r["lifetime"].as_str(), r["taken_from_room"].as_bool()), (Some("room"), Some(true)));
            assert_eq!(r["connection"]["state"], "live");
            for k in 1..20 {
                layer.advance(first + f64::from(k) * 0.1);
            }
            drop(layer);
            let s = state.borrow();
            assert_eq!(s.opened.len(), 1, "one socket for the room, stage {n}");
            assert!(s.closed.is_empty());
            assert!(s.lines.iter().any(|l| l.starts_with("a stage took the room's connection (live, 20 events")));
            assert!(s.lines.iter().any(|l| l.starts_with("the stage ended:") && l.ends_with("kept for the room")));
            drop(s);
            assert_eq!(kept_report().unwrap()["stages"].as_u64(), Some(n));
            room_ticks(30, &mut t, Some(&want));
        }
        // Left: closed, its lifetime told.
        room_ticks(1, &mut t, None);
        {
            let s = state.borrow();
            assert_eq!(s.closed, [1]);
            let closed = s.lines.iter().find(|l| l.starts_with("closed for the room")).unwrap();
            assert!(closed.contains("2 stages took it"), "{closed}");
            assert!(s.lines.iter().any(|l| l.starts_with("received")));
        }
        assert_eq!(kept_report(), None);
        // Another channel (or what shows) changes it: closed, opened again.
        room_ticks(2, &mut t, Some(&want));
        let other = KeptChat {
            level: Level::Emotes,
            ..want.clone()
        };
        room_ticks(2, &mut t, Some(&other));
        // (Each connection numbers its own sockets from 1.)
        assert_eq!((state.borrow().opened.len(), state.borrow().closed.len()), (3, 2));
        room_ticks(1, &mut t, None);
        // A play outside a room (the development script's own channel) is
        // still its own: opened with it, closed with it.
        ddi_ui::state::twitch::set_dev_live(own());
        let config = ChatConfig::for_play(&settings).unwrap().under_room(None).unwrap();
        let mut layer = ChatLayer::new(config);
        layer.advance(10.0);
        assert_eq!(layer.live_report().unwrap()["lifetime"], "play");
        drop(layer);
        assert_eq!((state.borrow().opened.len(), state.borrow().closed.len()), (4, 4));
        ddi_ui::state::twitch::set_dev_live(None);
        set_live(None);
    }

    /// Round DE: a stage that opened its own connection before the room
    /// could keep one (the access not known yet) gives it to the room at
    /// its end when the room wants it by then; the room opens none of its
    /// own while the stage holds it.
    #[test]
    fn a_stages_own_connection_is_given_to_the_room() {
        use ddi_ui::state::twitch::{KeptChat, RoomChat, keep_room_chat};
        set_scripted(None);
        ddi_ui::state::twitch::set_dev_live(None);
        let state = fake_live();
        let room = RoomChat {
            ceiling: Level::Emotes,
            channel_id: "77".into(),
            login: "host".into(),
            room: Some("Room000001".into()),
            named: false,
        };
        let stage = StageSetup {
            chat: Some(room.clone()),
            ..setup()
        };
        let settings = Settings {
            twitch_chat: Level::Messages,
            ..Settings::default()
        };
        let config = ChatConfig::for_play(&settings).unwrap().under_room(Some(&stage)).unwrap();
        let mut layer = ChatLayer::new(config);
        layer.advance(1.0);
        layer.advance(1.1);
        assert_eq!(layer.live_report().unwrap()["taken_from_room"], false);
        let want = KeptChat {
            chat: room,
            level: Level::Emotes,
            emotes_from: EmotesFrom::TwitchAndOthers,
        };
        keep_room_chat(Some(&want), 5_000);
        keep_room_chat(Some(&want), 5_100);
        assert_eq!(state.borrow().opened.len(), 1, "none of the room's own while the stage holds one");
        drop(layer);
        assert!(state.borrow().closed.is_empty(), "given to the room");
        assert_eq!(kept_report().unwrap()["held"], true);
        keep_room_chat(Some(&want), 5_200);
        keep_room_chat(None, 5_300);
        assert_eq!(state.borrow().closed, [1]);
        set_live(None);
    }

    /// Review 2026-11-22, chat-touch 1: a room's kept connection that gave
    /// up (our server refused its subscribe: the person's session expired
    /// meanwhile, say) is closed and opened again after
    /// [`ROOM_RETRY_MS`] (a stage meanwhile opens its own, as before the
    /// room kept one), never lent dead to every later stage; one the room
    /// itself ended (its chat turned off, the player no member) waits for
    /// the room's chat to change.
    #[test]
    fn a_rooms_kept_chat_that_gave_up_is_opened_again() {
        use ddi_ui::state::twitch::{KeptChat, RoomChat, keep_room_chat};
        set_scripted(None);
        ddi_ui::state::twitch::set_dev_live(None);
        let state = fake_live();
        state.borrow_mut().refuse.push_back("HTTP 401".into());
        let room = RoomChat {
            ceiling: Level::Messages,
            channel_id: "77".into(),
            login: "host".into(),
            room: Some("Room000001".into()),
            named: false,
        };
        let want = KeptChat {
            chat: room.clone(),
            level: Level::Messages,
            emotes_from: EmotesFrom::TwitchAndOthers,
        };
        let mut t = 1_700_000_000_000u64;
        let room_ticks = |n: usize, t: &mut u64, want: Option<&KeptChat>| {
            for _ in 0..n {
                keep_room_chat(want, *t);
                *t += 100;
            }
        };
        room_ticks(5, &mut t, Some(&want));
        assert_eq!(state.borrow().opened.len(), 1);
        // A stage meanwhile is not lent the dead connection: its own.
        let settings = Settings {
            twitch_chat: Level::Messages,
            ..Settings::default()
        };
        let stage = StageSetup {
            chat: Some(room.clone()),
            ..setup()
        };
        let config = ChatConfig::for_play(&settings).unwrap().under_room(Some(&stage)).unwrap();
        let mut layer = ChatLayer::new(config);
        for k in 0..4 {
            layer.advance(1.0 + f64::from(k) * 0.1);
        }
        assert_eq!(layer.live_report().unwrap()["taken_from_room"], false);
        assert_eq!(layer.live_report().unwrap()["connection"]["state"], "live");
        drop(layer);
        room_ticks(1, &mut t, None);
        // Kept again for the room, and refused once more: given up, then
        // opened again after the wait.
        state.borrow_mut().refuse.push_back("HTTP 401".into());
        room_ticks(5, &mut t, Some(&want));
        let opened = state.borrow().opened.len();
        room_ticks((ROOM_RETRY_MS / 100) as usize + 5, &mut t, Some(&want));
        assert_eq!(state.borrow().opened.len(), opened + 1, "opened again after the wait");
        room_ticks(3, &mut t, Some(&want));
        assert_eq!(kept_report().unwrap()["connection"]["state"], "live");
        // The room's own end (its chat off for this player): kept closed.
        state.borrow_mut().refuse.push_back("chat_off".into());
        room_ticks(1, &mut t, None);
        room_ticks(5, &mut t, Some(&want));
        let opened = state.borrow().opened.len();
        room_ticks((ROOM_RETRY_MS / 100) as usize * 2, &mut t, Some(&want));
        assert_eq!(state.borrow().opened.len(), opened, "not opened again");
        room_ticks(1, &mut t, None);
        set_live(None);
    }

    /// Round TW6: the live chat over a shell's side: the socket opened at
    /// the setup's address, the welcome's session subscribed through the
    /// route, the chat's events numbered on the host clock, the providers'
    /// sets read (a 404 no failure), the report kept; the play's end
    /// closes the socket.
    #[test]
    fn the_live_chat_drives_its_connection() {
        set_scripted(None);
        let state = fake_live();
        ddi_ui::state::twitch::set_dev_live(own());
        let config = ChatConfig::for_play(&Settings {
            twitch_chat: Level::Messages,
            ..Settings::default()
        })
        .unwrap()
        .under_room(Some(&practice_stage()))
        .unwrap();
        let mut c = ChatLayer::new(config);
        assert!(c.is_live());
        assert_eq!(c.api().as_deref(), Some("https://api.test"));
        c.advance(10.0);
        c.advance(10.1);
        c.advance(10.2);
        {
            let s = state.borrow();
            assert_eq!(s.opened, [(1, "ws://127.0.0.1:9/eventsub".to_string())]);
            assert_eq!(s.subscribed.len(), 1);
            assert_eq!(s.subscribed[0].1, "session-1");
            assert!(s.subscribed[0].3, "messages");
            // Six sets, the three global ones again through the proxy.
            assert_eq!(s.reads.len(), 9);
            assert!(s.lines.iter().any(|l| l.contains("live, subscribed")), "{:?}", s.lines);
        }
        assert!(c.since(0).count() >= 1);
        let r = c.live_report().unwrap();
        assert_eq!(r["connection"]["state"], "live");
        assert_eq!(r["route"], "own");
        assert_eq!(r["emote_sets"][0]["state"], "no set");
        drop(c);
        let s = state.borrow();
        assert_eq!(s.closed, [1]);
        assert!(s.lines.iter().any(|l| l.starts_with("received")));
        assert_eq!(s.lines[0], "channel me (your own), route /me/chat/subscribe");
        drop(s);
        ddi_ui::state::twitch::set_dev_live(None);
        set_live(None);
    }
}
