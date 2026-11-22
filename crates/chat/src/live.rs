//! The live connection to Twitch's chat (round TW6,
//! `docs/plans/multiplayer-twitch.md`, C2): a device's own socket to
//! Twitch's EventSub (`wss://eventsub.wss.twitch.tv/ws`) as a state
//! machine, sans-IO. The shells open and close the sockets, hand it each
//! text frame, each close and our server's answer to "subscribe this
//! socket" ([`Do::Subscribe`]), and the time; it says what to do next
//! ([`Live::take_do`]), hands the chat's events on ([`Live::take_events`],
//! read by [`crate::eventsub`]) and keeps what the maintainer's run
//! reports ([`Live::report`], [`Live::take_lines`]).
//!
//! What it does, as Twitch's guide says (`dev.twitch.tv/docs/eventsub/
//! handling-websocket-events`, read 2026-11-21; UNVERIFIED against
//! Twitch itself until the maintainer's run):
//!
//! - the welcome's session id is what our server subscribes, inside the
//!   ten seconds Twitch gives after the welcome (close 4003 otherwise); a
//!   failure our server calls passing is asked again once while enough of
//!   them is left;
//! - keep-alives and notifications reset a watchdog of the stated
//!   `keepalive_timeout_seconds` and a margin; silence past it is a lost
//!   connection, opened again and subscribed again;
//! - Twitch's reconnect message is followed to the address it gives, the
//!   old socket kept (its events still taken) until the new one welcomes,
//!   then closed; the subscriptions carry over, nothing is subscribed
//!   again;
//! - a revocation of the messages' subscription ends it quietly; each
//!   close code is read for what it means ([`close_word`]); a loss is
//!   opened again after a backoff, and after [`Policy::give_up_after`]
//!   losses in a row it gives up quietly, as it does at once for a
//!   refusal that would only repeat (no access, not a member, the room
//!   shows no chat, Twitch's limits, our socket said to have sent
//!   something);
//! - a notification Twitch sends again (the same `message_id`, as over a
//!   reconnect's hand-over) is taken once.
//!
//! Nothing here judges or logs a play. The report never holds a token,
//! our session or a person's chat: what the readers did not understand is
//! kept as its shape ([`redact`]: every text replaced by its length).

use std::collections::VecDeque;

use serde_json::{Value, json};

use crate::eventsub::{self, Event, Fragment, Message};

/// Twitch's EventSub socket.
pub const EVENTSUB_URL: &str = "wss://eventsub.wss.twitch.tv/ws";

/// The prefix of every line the chat's live part reports (desktop's
/// stderr, the web's console): the maintainer pastes the block whole.
pub const LOG_PREFIX: &str = "[ddi-chat]";

/// The live connection's numbers, PROVISIONAL: named and versioned so a
/// change is seen in the report.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Policy {
    pub name: &'static str,
    /// From asking for a socket to its welcome, at most.
    pub welcome_wait_ms: u64,
    /// Twitch's time from the welcome to the first subscription (its
    /// guide: 10 s, then close 4003).
    pub subscribe_window_ms: u64,
    /// A subscription that failed in passing is asked again once, a
    /// second later, while at least this much of the window is left.
    pub retry_left_ms: u64,
    /// Silence past the stated keep-alive plus this is a lost connection.
    pub keepalive_margin_ms: u64,
    /// The keep-alive taken where a welcome states none (Twitch's
    /// default).
    pub default_keepalive_s: u32,
    /// A reconnect's new socket must welcome within this (Twitch's grace:
    /// the old one is closed 30 s after its reconnect message).
    pub reconnect_wait_ms: u64,
    /// The waits before opening again, by the losses in a row (the last
    /// one for any more).
    pub backoff_ms: &'static [u64],
    /// Losses in a row before giving up quietly.
    pub give_up_after: u32,
}

/// `live-chat-v0` (PROVISIONAL, round TW6).
pub const LIVE_CHAT_V0: Policy = Policy {
    name: "live-chat-v0",
    welcome_wait_ms: 10_000,
    subscribe_window_ms: 10_000,
    retry_left_ms: 4_000,
    keepalive_margin_ms: 3_000,
    default_keepalive_s: 10,
    reconnect_wait_ms: 30_000,
    backoff_ms: &[0, 1_000, 2_000, 5_000, 10_000, 30_000, 60_000],
    give_up_after: 8,
};

/// What a WebSocket close code means, as Twitch's guide lists its own
/// (4000 to 4007) and the WebSocket standard the others.
pub fn close_word(code: u16) -> &'static str {
    match code {
        4000 => "Twitch's server error",
        4001 => "we sent something (never allowed)",
        4002 => "a ping not answered",
        4003 => "not subscribed in time",
        4004 => "a reconnect's grace ran out",
        4005 => "network timeout",
        4006 => "network error",
        4007 => "an invalid reconnect address",
        1000 => "closed normally",
        1001 => "going away",
        1005 => "closed without a code",
        1006 => "dropped",
        _ => "another code",
    }
}

/// What the shell is to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Do {
    /// Open socket `socket` (a number of this connection's own) at `url`.
    Open { socket: u32, url: String },
    /// Close it (normally; it is never heard from again).
    Close { socket: u32 },
    /// Ask our server to subscribe this socket's session to the chat
    /// (`POST <api>/rooms/<id>/chat/subscribe`, or `<api>/chat/subscribe`
    /// for one's own channel); its answer goes to [`Live::subscribed`].
    Subscribe { socket: u32, session_id: String },
}

/// One subscription as our server answered it (Twitch's answer, words of
/// ours for its refusals).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Subscription {
    /// Its type (`channel.chat.message`, …).
    pub kind: String,
    /// Our word for the answer: `enabled`, `exists`, `bad_session`,
    /// `bad_request`, `refused`, `forbidden`, `limits`, `unreachable`,
    /// `unknown`.
    pub outcome: String,
    /// Twitch's own: the status code, the subscription's status, its
    /// cost and the totals, the error's text where refused, and the time
    /// the call took.
    pub http: Option<u16>,
    pub status: Option<String>,
    pub cost: Option<i64>,
    pub total: Option<i64>,
    pub total_cost: Option<i64>,
    pub max_total_cost: Option<i64>,
    pub error: Option<String>,
    pub ms: Option<u64>,
}

impl Subscription {
    fn ok(&self) -> bool {
        matches!(&*self.outcome, "enabled" | "exists")
    }

    fn to_json(&self) -> Value {
        json!({
            "type": self.kind,
            "outcome": self.outcome,
            "http": self.http,
            "status": self.status,
            "cost": self.cost,
            "total": self.total,
            "total_cost": self.total_cost,
            "max_total_cost": self.max_total_cost,
            "error": self.error,
            "ms": self.ms,
        })
    }
}

/// The channel our server read the chat of, as it named it in its answer
/// (round TW7: a channel named by its login is found by our server).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Channel {
    pub id: String,
    pub login: String,
}

/// Our server's answer to a subscribe.
#[derive(Clone, Debug, PartialEq)]
pub enum SubscribeAnswer {
    /// Twitch answered each type; the channel read, where our server
    /// named it.
    Subscribed(Vec<Subscription>, Option<Channel>),
    /// Refused for good (asking again would be refused again): its word
    /// (`signed_out`, `not_a_member`, `chat_off`, `scope_missing`,
    /// `twitch_refused`, `no_twitch`, …).
    Refused(String),
    /// Failed in passing (our server or Twitch slow or down, the network,
    /// too many asks): its word.
    Failed(String),
}

const MESSAGES: &str = "channel.chat.message";

/// Our server's answer (`status`, `body`) to a subscribe, read: `200
/// {"subscriptions": [...]}` (its per-type words, Twitch's numbers), or a
/// refusal (`{"error": word}`); status 0 is no answer at all.
pub fn read_subscribe_answer(status: u16, body: &str) -> SubscribeAnswer {
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let word = v
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    match status {
        200 => {
            let Some(list) = v.get("subscriptions").and_then(Value::as_array) else {
                return SubscribeAnswer::Failed("an answer without subscriptions".into());
            };
            let text = |s: &Value, k: &str| s.get(k).and_then(Value::as_str).map(str::to_string);
            let int = |s: &Value, k: &str| s.get(k).and_then(Value::as_i64);
            let channel = match (text(&v, "channel_id"), text(&v, "channel")) {
                (Some(id), Some(login)) if !id.is_empty() => Some(Channel { id, login }),
                _ => None,
            };
            SubscribeAnswer::Subscribed(
                list.iter()
                    .map(|s| Subscription {
                        kind: text(s, "type").unwrap_or_default(),
                        outcome: text(s, "outcome").unwrap_or_else(|| "unknown".into()),
                        http: s.get("http").and_then(Value::as_u64).map(|h| h as u16),
                        status: text(s, "status"),
                        cost: int(s, "cost"),
                        total: int(s, "total"),
                        total_cost: int(s, "total_cost"),
                        max_total_cost: int(s, "max_total_cost"),
                        error: text(s, "error"),
                        ms: s.get("ms").and_then(Value::as_u64),
                    })
                    .collect(),
                channel,
            )
        }
        0 => SubscribeAnswer::Failed("no answer".into()),
        429 | 500..=599 => SubscribeAnswer::Failed(if word.is_empty() {
            format!("HTTP {status}")
        } else {
            word
        }),
        _ => SubscribeAnswer::Refused(if word.is_empty() {
            format!("HTTP {status}")
        } else {
            word
        }),
    }
}

/// Why a live chat could not show (round TW7): what the player is told
/// in one plain line after the stage, never during it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Trouble {
    /// This device's Twitch access does not allow reading chat (never
    /// given, disconnected, or refused by Twitch).
    Access,
    /// Twitch refused this reader the channel's chat (banned there, or
    /// its limits).
    Refused,
    /// The channel is gone from Twitch (TESTING ONLY too: the
    /// development flag named no such channel).
    ChannelGone,
    /// The connection to Twitch failed and kept failing.
    Unreachable,
    /// TESTING ONLY: the development flag named another channel, which
    /// only an administrator may.
    NotAllowed,
}

impl Trouble {
    /// Its word in the report.
    pub fn word(self) -> &'static str {
        match self {
            Trouble::Access => "access",
            Trouble::Refused => "refused",
            Trouble::ChannelGone => "channel gone",
            Trouble::Unreachable => "unreachable",
            Trouble::NotAllowed => "not allowed",
        }
    }

    /// Our server's refusal of a subscribe, read: none where the room
    /// itself changed (its chat turned off, the player no member), which
    /// is no trouble of this device's.
    pub fn of_refusal(word: &str) -> Option<Trouble> {
        Some(match word {
            "not_a_member" | "chat_off" => return None,
            "scope_missing" | "twitch_refused" | "no_twitch" | "signed_out" | "HTTP 401" => {
                Trouble::Access
            }
            "no_such_channel" | "bad_channel" => Trouble::ChannelGone,
            "channel_not_allowed" => Trouble::NotAllowed,
            _ => Trouble::Unreachable,
        })
    }

    /// The messages' subscription refused with our word for Twitch's
    /// answer.
    fn of_outcome(outcome: &str) -> Trouble {
        match outcome {
            "refused" => Trouble::Access,
            "forbidden" | "limits" => Trouble::Refused,
            _ => Trouble::Unreachable,
        }
    }

    /// The messages' subscription revoked with Twitch's status.
    fn of_revocation(status: &str) -> Trouble {
        match status {
            "authorization_revoked" => Trouble::Access,
            "user_removed" => Trouble::ChannelGone,
            _ => Trouble::Refused,
        }
    }
}

/// Where the connection is.
#[derive(Clone, Debug, PartialEq)]
pub enum Phase {
    /// Not started.
    Idle,
    /// A socket opening, welcomed, subscribing or live.
    Running,
    /// Lost: opened again at this time.
    Waiting { until: u64 },
    /// Given up quietly, and why.
    GaveUp(String),
    /// Ended by the play or the room.
    Stopped,
}

#[derive(Clone, Debug, PartialEq)]
enum Sub {
    /// Not asked yet (no welcome yet).
    No,
    /// Asked at this time, this many times.
    Asked { at: u64, tries: u32 },
    /// Asked again at this time.
    RetryAt { at: u64, tries: u32 },
    Done,
}

#[derive(Clone, Debug)]
struct Conn {
    id: u32,
    asked: u64,
    opened: Option<u64>,
    welcomed: Option<u64>,
    session: Option<String>,
    keepalive_s: u32,
    heard: u64,
    sub: Sub,
    /// Opened from a reconnect message: its subscriptions carried over.
    handover: bool,
}

/// What came in, counted for the report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Received {
    pub frames: u64,
    pub welcome: u64,
    pub keepalive: u64,
    pub notification: u64,
    pub reconnect: u64,
    pub revocation: u64,
    pub other: u64,
    pub unreadable: u64,
    /// Notifications already taken (the same `message_id`).
    pub duplicate: u64,
    /// Chat events by kind.
    pub messages: u64,
    pub deletes: u64,
    pub clear_users: u64,
    pub clears: u64,
    pub other_events: u64,
}

/// Samples of what the readers did not understand, kept at most.
pub const SAMPLES: usize = 5;
/// Lines of the timeline kept for the report.
const TIMELINE: usize = 60;
/// Notification ids remembered against duplicates.
const SEEN: usize = 512;

/// The live connection.
pub struct Live {
    policy: Policy,
    url: String,
    phase: Phase,
    conns: Vec<Conn>,
    next_socket: u32,
    losses: u32,
    started: Option<u64>,
    todo: Vec<Do>,
    events: Vec<Event>,
    lines: Vec<String>,
    timeline: VecDeque<(u64, String)>,
    seen: VecDeque<String>,
    received: Received,
    closes: Vec<(u64, u32, u16)>,
    subscriptions: Vec<Subscription>,
    revocations: Vec<(String, String)>,
    samples: Vec<Value>,
    not_understood: u64,
    welcome_ms: Option<u64>,
    subscribed_ms: Option<u64>,
    keepalive_s: Option<u32>,
    reconnects_asked: u64,
    reconnects_done: u64,
    reconnects_failed: u64,
    losses_total: u64,
    sockets: u64,
    trouble: Option<Trouble>,
    channel: Option<Channel>,
}

impl Live {
    /// A connection to `url` ([`EVENTSUB_URL`], or a test's own socket)
    /// under `policy`; nothing happens before [`Live::start`].
    pub fn new(policy: Policy, url: impl Into<String>) -> Live {
        Live {
            policy,
            url: url.into(),
            phase: Phase::Idle,
            conns: Vec::new(),
            next_socket: 1,
            losses: 0,
            started: None,
            todo: Vec::new(),
            events: Vec::new(),
            lines: Vec::new(),
            timeline: VecDeque::new(),
            seen: VecDeque::new(),
            received: Received::default(),
            closes: Vec::new(),
            subscriptions: Vec::new(),
            revocations: Vec::new(),
            samples: Vec::new(),
            not_understood: 0,
            welcome_ms: None,
            subscribed_ms: None,
            keepalive_s: None,
            reconnects_asked: 0,
            reconnects_done: 0,
            reconnects_failed: 0,
            losses_total: 0,
            sockets: 0,
            trouble: None,
            channel: None,
        }
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    /// The counts of what came in.
    pub fn received(&self) -> Received {
        self.received
    }

    /// One word for where it is: `idle`, `connecting`, `welcomed`,
    /// `subscribing`, `live`, `reconnecting` (a reconnect's hand-over),
    /// `waiting`, `gave up`, `stopped`.
    pub fn state(&self) -> &'static str {
        match &self.phase {
            Phase::Idle => "idle",
            Phase::Waiting { .. } => "waiting",
            Phase::GaveUp(_) => "gave up",
            Phase::Stopped => "stopped",
            Phase::Running => {
                if self.conns.len() > 1 {
                    return "reconnecting";
                }
                match self.conns.first() {
                    None => "connecting",
                    Some(c) if c.welcomed.is_none() => "connecting",
                    Some(c) => match c.sub {
                        Sub::Done => "live",
                        Sub::No => "welcomed",
                        _ => "subscribing",
                    },
                }
            }
        }
    }

    /// Why the chat could not show, where it could not (round TW7): its
    /// trouble where it gave up; [`Trouble::Unreachable`] where it never
    /// went live and lost a connection; none where it showed, or was never
    /// tried, or was stopped while still connecting.
    pub fn trouble(&self) -> Option<Trouble> {
        match &self.phase {
            Phase::GaveUp(_) => self.trouble,
            _ if self.subscribed_ms.is_none() && self.losses_total > 0 => Some(Trouble::Unreachable),
            _ => None,
        }
    }

    /// The channel our server last said it read (round TW7).
    pub fn channel(&self) -> Option<&Channel> {
        self.channel.as_ref()
    }

    /// Whether the chat is coming (subscribed, or carried over a
    /// reconnect).
    pub fn is_live(&self) -> bool {
        self.state() == "live" || self.state() == "reconnecting"
    }

    fn rel(&self, now: u64) -> u64 {
        now.saturating_sub(self.started.unwrap_or(now))
    }

    fn line(&mut self, now: u64, text: String) {
        let t = self.rel(now);
        self.lines.push(format!("+{t} ms {text}"));
        self.timeline.push_back((t, text));
        while self.timeline.len() > TIMELINE {
            self.timeline.pop_front();
        }
    }

    /// Starts: the first socket opened.
    pub fn start(&mut self, now: u64) {
        if self.phase != Phase::Idle {
            return;
        }
        self.started = Some(now);
        self.phase = Phase::Running;
        self.line(
            now,
            format!("{}: connecting to {}", self.policy.name, host_of(&self.url)),
        );
        self.open(now, self.url.clone(), false);
    }

    fn open(&mut self, now: u64, url: String, handover: bool) {
        let id = self.next_socket;
        self.next_socket += 1;
        self.sockets += 1;
        self.conns.push(Conn {
            id,
            asked: now,
            opened: None,
            welcomed: None,
            session: None,
            keepalive_s: self.policy.default_keepalive_s,
            heard: now,
            sub: Sub::No,
            handover,
        });
        self.todo.push(Do::Open { socket: id, url });
    }

    /// Ends it (the play or the room ended): every socket closed.
    pub fn stop(&mut self, now: u64) {
        if matches!(self.phase, Phase::Stopped) {
            return;
        }
        for c in std::mem::take(&mut self.conns) {
            self.todo.push(Do::Close { socket: c.id });
        }
        if self.phase != Phase::Idle {
            self.line(now, "stopped".into());
        }
        self.phase = Phase::Stopped;
    }

    fn close_all(&mut self) {
        for c in std::mem::take(&mut self.conns) {
            self.todo.push(Do::Close { socket: c.id });
        }
    }

    /// A loss: every socket closed, opened again after the backoff, or
    /// given up past [`Policy::give_up_after`] in a row.
    fn lose(&mut self, now: u64, why: String) {
        self.close_all();
        self.losses += 1;
        self.losses_total += 1;
        if self.losses >= self.policy.give_up_after {
            self.give_up(
                now,
                format!("{} losses in a row, the last: {why}", self.losses),
                Some(Trouble::Unreachable),
            );
            return;
        }
        let k = (self.losses as usize - 1).min(self.policy.backoff_ms.len().saturating_sub(1));
        let wait = self.policy.backoff_ms.get(k).copied().unwrap_or(0);
        self.phase = Phase::Waiting { until: now + wait };
        self.line(now, format!("lost ({why}): opened again in {wait} ms"));
    }

    /// Gives up quietly, with the player's line for it after the stage
    /// (`trouble`; none: nothing to tell).
    fn give_up(&mut self, now: u64, why: String, trouble: Option<Trouble>) {
        self.close_all();
        self.line(now, format!("gave up: {why}"));
        self.phase = Phase::GaveUp(why);
        self.trouble = trouble;
    }

    fn index(&self, socket: u32) -> Option<usize> {
        self.conns.iter().position(|c| c.id == socket)
    }

    /// Socket `socket` opened.
    pub fn opened(&mut self, socket: u32, now: u64) {
        let Some(k) = self.index(socket) else { return };
        let c = &mut self.conns[k];
        c.opened = Some(now);
        c.heard = now;
        let ms = now - c.asked;
        self.line(now, format!("socket {socket}: open in {ms} ms"));
    }

    /// A text frame came on `socket`.
    pub fn text(&mut self, socket: u32, text: &str, now: u64) {
        let Some(k) = self.index(socket) else { return };
        self.received.frames += 1;
        let message = match eventsub::parse(text) {
            Ok(m) => m,
            Err(e) => {
                self.received.unreadable += 1;
                self.sample(format!("unreadable: {e}"), text);
                return;
            }
        };
        match message {
            Message::Welcome {
                session_id,
                keepalive_seconds,
            } => {
                self.received.welcome += 1;
                self.conns[k].heard = now;
                self.welcome(k, session_id, keepalive_seconds, text, now);
            }
            Message::Keepalive => {
                self.received.keepalive += 1;
                self.conns[k].heard = now;
            }
            Message::Reconnect { url } => {
                self.received.reconnect += 1;
                self.conns[k].heard = now;
                self.reconnect(k, url, text, now);
            }
            Message::Revocation { kind, status } => {
                self.received.revocation += 1;
                self.conns[k].heard = now;
                self.line(now, format!("revoked: {kind} ({status})"));
                self.revocations.push((kind.clone(), status.clone()));
                if kind == MESSAGES {
                    let trouble = Trouble::of_revocation(&status);
                    self.give_up(
                        now,
                        format!("the chat's subscription was revoked ({status})"),
                        Some(trouble),
                    );
                }
            }
            Message::Notification(event) => {
                self.received.notification += 1;
                self.conns[k].heard = now;
                if let Some(id) = message_id(text) {
                    if self.seen.contains(&id) {
                        self.received.duplicate += 1;
                        return;
                    }
                    self.seen.push_back(id);
                    while self.seen.len() > SEEN {
                        self.seen.pop_front();
                    }
                }
                self.event(event, text);
            }
            Message::Other(kind) => {
                self.received.other += 1;
                self.conns[k].heard = now;
                self.sample(format!("message type {kind:?}"), text);
            }
        }
    }

    fn event(&mut self, event: Event, text: &str) {
        match &event {
            Event::Message(m) => {
                self.received.messages += 1;
                if m.fragments.iter().any(|f| matches!(f, Fragment::Other(_))) {
                    self.sample("a fragment of another kind".into(), text);
                } else if m.id.is_empty() || m.chatter_id.is_empty() {
                    self.sample("a message without its id or chatter".into(), text);
                } else if m.fragments.is_empty() && !m.text.is_empty() {
                    self.sample("a message without fragments".into(), text);
                }
            }
            Event::Delete { message_id } => {
                self.received.deletes += 1;
                if message_id.is_empty() {
                    self.sample("a delete without its message id".into(), text);
                }
            }
            Event::ClearUser { user_id } => {
                self.received.clear_users += 1;
                if user_id.is_empty() {
                    self.sample("a user's clear without its user".into(), text);
                }
            }
            Event::Clear => self.received.clears += 1,
            Event::Other(kind) => {
                self.received.other_events += 1;
                self.sample(format!("a notification of type {kind:?}"), text);
            }
        }
        self.events.push(event);
    }

    fn sample(&mut self, why: String, text: &str) {
        self.not_understood += 1;
        if self.samples.len() < SAMPLES {
            self.samples.push(json!({ "why": why, "shape": redact(text) }));
        }
    }

    fn welcome(
        &mut self,
        k: usize,
        session_id: String,
        keepalive: Option<u32>,
        text: &str,
        now: u64,
    ) {
        if self.conns[k].welcomed.is_some() {
            self.sample("a second welcome".into(), text);
            return;
        }
        let keepalive_s = keepalive.unwrap_or(self.policy.default_keepalive_s).max(1);
        let c = &mut self.conns[k];
        c.welcomed = Some(now);
        c.keepalive_s = keepalive_s;
        let socket = c.id;
        let since = c.opened.unwrap_or(c.asked);
        let in_ms = now - c.asked;
        self.welcome_ms = Some(now - since);
        self.keepalive_s = Some(keepalive_s);
        if keepalive.is_none() {
            self.sample("a welcome without keepalive_timeout_seconds".into(), text);
        }
        if self.conns[k].handover {
            // A reconnect's new socket: the subscriptions carried over;
            // the old one closed now.
            self.conns[k].sub = Sub::Done;
            self.conns[k].session = Some(session_id);
            let old: Vec<u32> = self
                .conns
                .iter()
                .filter(|c| c.id != socket)
                .map(|c| c.id)
                .collect();
            self.conns.retain(|c| c.id == socket);
            for o in &old {
                self.todo.push(Do::Close { socket: *o });
            }
            self.reconnects_done += 1;
            self.losses = 0;
            self.line(
                now,
                format!(
                    "socket {socket}: welcomed after a reconnect in {in_ms} ms (keepalive {keepalive_s} s); the old socket closed, nothing subscribed again"
                ),
            );
            return;
        }
        if session_id.is_empty() {
            self.sample("a welcome without a session id".into(), text);
            self.lose(now, "a welcome without a session id".into());
            return;
        }
        self.line(
            now,
            format!("socket {socket}: welcomed in {in_ms} ms (keepalive {keepalive_s} s as Twitch stated it)"),
        );
        self.conns[k].session = Some(session_id.clone());
        self.conns[k].sub = Sub::Asked { at: now, tries: 1 };
        self.todo.push(Do::Subscribe { socket, session_id });
    }

    fn reconnect(&mut self, k: usize, url: String, text: &str, now: u64) {
        self.reconnects_asked += 1;
        let socket = self.conns[k].id;
        if self.conns.len() > 1 {
            self.line(now, format!("socket {socket}: another reconnect message, passed over"));
            return;
        }
        if !(url.starts_with("wss://") || url.starts_with("ws://")) {
            self.sample("a reconnect without a usable address".into(), text);
            self.line(now, format!("socket {socket}: a reconnect message without an address; waiting for its close"));
            return;
        }
        self.line(
            now,
            format!("socket {socket}: Twitch asked for a reconnect to {}; the old socket kept until the new one welcomes", host_of(&url)),
        );
        self.open(now, url, true);
    }

    /// Socket `socket` closed with `code` (1006 for a drop without one).
    pub fn closed(&mut self, socket: u32, code: u16, now: u64) {
        let Some(k) = self.index(socket) else { return };
        let rel = self.rel(now);
        self.closes.push((rel, socket, code));
        let word = close_word(code);
        let c = self.conns.remove(k);
        if matches!(self.phase, Phase::Stopped | Phase::GaveUp(_)) {
            return;
        }
        if c.handover {
            // A reconnect's new socket closed before its welcome.
            self.reconnects_failed += 1;
            if self.conns.is_empty() {
                self.lose(now, format!("the reconnect's socket closed {code} ({word}) after the old one"));
            } else {
                self.line(now, format!("socket {socket}: the reconnect's socket closed {code} ({word}) before its welcome"));
            }
            return;
        }
        if let Some(next) = self.conns.iter().position(|n| n.handover) {
            // The old socket closed during a hand-over: the new one goes
            // on (its welcome still to come).
            let id = self.conns[next].id;
            self.line(
                now,
                format!("socket {socket}: closed {code} ({word}) during the hand-over; socket {id} goes on"),
            );
            return;
        }
        if code == 4001 {
            self.give_up(
                now,
                format!("socket {socket} closed {code} ({word})"),
                Some(Trouble::Unreachable),
            );
            return;
        }
        self.lose(now, format!("socket {socket} closed {code} ({word})"));
    }

    /// Our server answered the subscribe of `socket`'s session.
    pub fn subscribed(&mut self, socket: u32, answer: SubscribeAnswer, now: u64) {
        let Some(k) = self.index(socket) else { return };
        let welcomed = self.conns[k].welcomed.unwrap_or(now);
        let tries = match self.conns[k].sub {
            Sub::Asked { tries, .. } => tries,
            _ => return,
        };
        match answer {
            SubscribeAnswer::Subscribed(list, channel) => {
                if let Some(c) = channel
                    && self.channel.as_ref() != Some(&c)
                {
                    self.line(now, format!("channel {} is Twitch user {}", c.login, c.id));
                    self.channel = Some(c);
                }
                for s in &list {
                    // Twitch's totals are each call's own count, the calls
                    // made at once (round LO: "total 1" on each of four
                    // lines read as one in all).
                    let line = format!(
                        "subscribed {}: {} (HTTP {}, status {}, cost {}, total {} at this call, total cost {} of {}{})",
                        s.kind,
                        s.outcome,
                        opt(s.http),
                        s.status.as_deref().unwrap_or("-"),
                        opt(s.cost),
                        opt(s.total),
                        opt(s.total_cost),
                        opt(s.max_total_cost),
                        s.error.as_deref().map(|e| format!(", \"{e}\"")).unwrap_or_default(),
                    );
                    self.line(now, line);
                }
                self.subscriptions = list.clone();
                let messages = list.iter().find(|s| s.kind == MESSAGES);
                match messages {
                    Some(s) if s.ok() => {
                        self.conns[k].sub = Sub::Done;
                        self.losses = 0;
                        let ms = now - welcomed;
                        self.subscribed_ms = Some(ms);
                        self.line(now, format!("socket {socket}: live, subscribed {ms} ms after the welcome"));
                    }
                    Some(s) if s.outcome == "bad_session" => {
                        self.lose(now, "Twitch did not know the socket's session".into());
                    }
                    Some(s) if matches!(&*s.outcome, "unreachable" | "unknown") => {
                        self.retry(k, tries, format!("the chat's subscription: {}", s.outcome), now);
                    }
                    Some(s) => {
                        let why = format!("the chat's subscription: {}", s.outcome);
                        let trouble = Trouble::of_outcome(&s.outcome);
                        self.give_up(now, why, Some(trouble));
                    }
                    None => self.give_up(
                        now,
                        "our server subscribed no chat".into(),
                        Some(Trouble::Unreachable),
                    ),
                }
            }
            SubscribeAnswer::Refused(word) => {
                let trouble = Trouble::of_refusal(&word);
                self.give_up(now, format!("refused by our server: {word}"), trouble);
            }
            SubscribeAnswer::Failed(word) => self.retry(k, tries, word, now),
        }
    }

    /// A subscribe failed in passing: asked again a second later where
    /// enough of the window is left and it was asked once; else lost.
    fn retry(&mut self, k: usize, tries: u32, why: String, now: u64) {
        let welcomed = self.conns[k].welcomed.unwrap_or(now);
        let left = (welcomed + self.policy.subscribe_window_ms).saturating_sub(now + 1_000);
        if tries < 2 && left >= self.policy.retry_left_ms {
            self.conns[k].sub = Sub::RetryAt {
                at: now + 1_000,
                tries: tries + 1,
            };
            self.line(now, format!("subscribe failed ({why}): asked again in 1 s"));
        } else {
            self.lose(now, format!("subscribe failed ({why})"));
        }
    }

    /// The time is `now`: what waits for a time is done.
    pub fn tick(&mut self, now: u64) {
        match self.phase {
            Phase::Waiting { until } if now >= until => {
                self.phase = Phase::Running;
                let url = self.url.clone();
                self.open(now, url, false);
                return;
            }
            Phase::Running => {}
            _ => return,
        }
        let p = self.policy;
        let mut lost: Option<String> = None;
        let mut dropped_successor = None;
        // During a reconnect's hand-over the old socket's silence is no
        // loss: its close is expected.
        let handing_over = self.conns.iter().any(|c| c.handover && c.welcomed.is_none());
        for c in &mut self.conns {
            if handing_over && !c.handover {
                continue;
            }
            match c.welcomed {
                None => {
                    let wait = if c.handover {
                        p.reconnect_wait_ms
                    } else {
                        p.welcome_wait_ms
                    };
                    if now >= c.asked + wait {
                        if c.handover {
                            dropped_successor = Some(c.id);
                        } else {
                            lost = Some(format!("socket {}: no welcome in {} ms", c.id, wait));
                        }
                    }
                }
                Some(w) => {
                    let silent = now.saturating_sub(c.heard);
                    if silent > u64::from(c.keepalive_s) * 1_000 + p.keepalive_margin_ms {
                        lost = Some(format!(
                            "socket {}: silent {silent} ms, past the keepalive of {} s Twitch stated",
                            c.id, c.keepalive_s
                        ));
                    }
                    match c.sub {
                        Sub::RetryAt { at, tries } if now >= at => {
                            c.sub = Sub::Asked { at: now, tries };
                            self.todo.push(Do::Subscribe {
                                socket: c.id,
                                session_id: c.session.clone().unwrap_or_default(),
                            });
                        }
                        Sub::Asked { .. } | Sub::RetryAt { .. }
                            if now >= w + p.subscribe_window_ms + 5_000 =>
                        {
                            lost = Some(format!(
                                "socket {}: not subscribed within the {} ms after its welcome",
                                c.id, p.subscribe_window_ms
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
        if let Some(id) = dropped_successor {
            self.conns.retain(|c| c.id != id);
            self.todo.push(Do::Close { socket: id });
            self.reconnects_failed += 1;
            self.line(now, format!("socket {id}: the reconnect's socket never welcomed"));
            if self.conns.is_empty() {
                self.lose(now, "the reconnect's socket never welcomed".into());
                return;
            }
        }
        if let Some(why) = lost {
            self.lose(now, why);
        }
    }

    /// What to do, once.
    pub fn take_do(&mut self) -> Vec<Do> {
        std::mem::take(&mut self.todo)
    }

    /// The chat's events received since, once each.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// The report's lines since (without [`LOG_PREFIX`]: the shell adds
    /// it), once each.
    pub fn take_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.lines)
    }

    /// Everything the run reports (the web's `__DDI_DEBUG.chat.live`,
    /// desktop's lines at the end): the state with times, the
    /// subscriptions as Twitch answered them, what was received, the
    /// closes, the reconnects, the revocations and the shapes of what was
    /// not understood. Never a token, a session or a person's chat.
    pub fn report(&self) -> Value {
        let r = self.received;
        json!({
            "policy": self.policy.name,
            "state": self.state(),
            "why": match &self.phase { Phase::GaveUp(w) => Some(w.clone()), _ => None },
            "channel": self.channel.as_ref().map(|c| json!({"id": c.id, "login": c.login})),
            "trouble": self.trouble().map(Trouble::word),
            "url_host": host_of(&self.url),
            "sockets": self.sockets,
            "welcome_ms": self.welcome_ms,
            "subscribed_after_welcome_ms": self.subscribed_ms,
            "keepalive_s": self.keepalive_s,
            "losses": self.losses_total,
            "reconnects": {
                "asked": self.reconnects_asked,
                "done": self.reconnects_done,
                "failed": self.reconnects_failed,
            },
            "closes": self.closes.iter().map(|(t, s, c)| json!({
                "ms": t, "socket": s, "code": c, "word": close_word(*c),
            })).collect::<Vec<_>>(),
            "subscriptions": self.subscriptions.iter().map(Subscription::to_json).collect::<Vec<_>>(),
            "received": {
                "frames": r.frames,
                "welcome": r.welcome,
                "keepalive": r.keepalive,
                "notification": r.notification,
                "reconnect": r.reconnect,
                "revocation": r.revocation,
                "other": r.other,
                "unreadable": r.unreadable,
                "duplicate": r.duplicate,
            },
            "events": {
                "message": r.messages,
                "delete": r.deletes,
                "clear_user": r.clear_users,
                "clear": r.clears,
                "other": r.other_events,
            },
            "revocations": self.revocations.iter().map(|(k, s)| json!({"type": k, "status": s})).collect::<Vec<_>>(),
            "not_understood": {
                "count": self.not_understood,
                "samples": self.samples,
            },
            "timeline": self.timeline.iter().map(|(t, w)| json!({"ms": t, "what": w})).collect::<Vec<_>>(),
        })
    }
}

fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| "-".into(), |v| v.to_string())
}

/// An address's host alone (a reconnect address's query is Twitch's own,
/// never reported).
pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split(['/', '?']).next().unwrap_or_default().to_string()
}

/// A socket message's `metadata.message_id`.
fn message_id(text: &str) -> Option<String> {
    let v: Value = serde_json::from_str(text).ok()?;
    v.get("metadata")?
        .get("message_id")?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Keys whose text is kept as it is in a shape: the structure's own
/// words, never a person's.
const KEPT_TEXT: [&str; 9] = [
    "message_type",
    "subscription_type",
    "subscription_version",
    "type",
    "version",
    "status",
    "method",
    "format",
    "message_timestamp",
];

/// A message's shape for the report: its JSON with every text replaced
/// by `"<text N>"` (its length in characters), but for the structure's
/// own words ([`KEPT_TEXT`]); numbers, booleans and nulls kept, arrays cut
/// to their first four items (`"…+N"` for the rest). Not JSON: its first
/// 200 characters' length alone.
pub fn redact(text: &str) -> Value {
    fn walk(v: &Value, key: Option<&str>) -> Value {
        match v {
            Value::String(s) => {
                if key.is_some_and(|k| KEPT_TEXT.contains(&k)) && s.chars().count() <= 64 {
                    Value::String(s.clone())
                } else {
                    Value::String(format!("<text {}>", s.chars().count()))
                }
            }
            Value::Array(a) => {
                let mut out: Vec<Value> = a.iter().take(4).map(|x| walk(x, key)).collect();
                if a.len() > 4 {
                    out.push(Value::String(format!("…+{}", a.len() - 4)));
                }
                Value::Array(out)
            }
            Value::Object(o) => Value::Object(
                o.iter()
                    .map(|(k, x)| (k.clone(), walk(x, Some(k))))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
    match serde_json::from_str::<Value>(text) {
        Ok(v) => walk(&v, None),
        Err(_) => Value::String(format!("<not JSON, {} characters>", text.chars().count())),
    }
}

#[cfg(test)]
mod tests;
