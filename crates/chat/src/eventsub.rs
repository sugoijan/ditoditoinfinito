//! Twitch's EventSub WebSocket messages (`wss://eventsub.wss.twitch.tv/ws`),
//! read from their JSON as Twitch's reference documents them
//! (`dev.twitch.tv/docs/eventsub/handling-websocket-events`, and the chat
//! types under `docs/eventsub/reference/chat`): what C2 and C11 need of
//! them, nothing else. Fields this reading does not use are ignored, and
//! a message of a kind it does not know is [`Message::Other`], never an
//! error.

use serde_json::Value;

/// One message of the socket.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// `session_welcome`: the session's id (what the subscribe route is
    /// given) and the keep-alive's period, seconds.
    Welcome {
        session_id: String,
        keepalive_seconds: Option<u32>,
    },
    /// `session_keepalive`.
    Keepalive,
    /// `session_reconnect`: open `url` and close this socket once its
    /// welcome came.
    Reconnect { url: String },
    /// `notification`.
    Notification(Event),
    /// `revocation`: a subscription Twitch ended (`authorization_revoked`,
    /// `user_removed`, `version_removed`, …).
    Revocation { kind: String, status: String },
    /// Any other `message_type`.
    Other(String),
}

/// A chat notification's event.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// `channel.chat.message`.
    Message(ChatMessage),
    /// `channel.chat.message_delete`: one message removed by a moderator.
    Delete { message_id: String },
    /// `channel.chat.clear_user_messages`: a chatter's messages removed
    /// (a ban or a timeout too).
    ClearUser { user_id: String },
    /// `channel.chat.clear`: the whole chat cleared.
    Clear,
    /// A subscription type this reading does not show.
    Other(String),
}

/// A `channel.chat.message` event, as much of it as the layer reads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChatMessage {
    pub id: String,
    pub chatter_id: String,
    pub chatter_login: String,
    /// The display name (may be in any script).
    pub chatter_name: String,
    pub text: String,
    pub fragments: Vec<Fragment>,
    /// `message_type`: `text`, `channel_points_highlighted`, `user_intro`,
    /// `power_ups_gigantified_emote`, ….
    pub kind: String,
    /// Bits were cheered with it (`cheer` set, or a cheermote in it).
    pub cheer: bool,
    /// It came from another channel of a shared chat session
    /// (`source_broadcaster_user_id` set and not the broadcaster's).
    pub shared: bool,
}

/// A piece of a message as Twitch cut it.
#[derive(Clone, Debug, PartialEq)]
pub enum Fragment {
    Text(String),
    /// One of Twitch's emotes: its word, its id, and whether a still
    /// image of it is offered (`format` has `static`).
    Emote {
        text: String,
        id: String,
        still: bool,
    },
    Cheermote(String),
    Mention(String),
    /// Anything else (`gif`, a type to come): its text.
    Other(String),
}

/// What could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// Not JSON.
    Json(String),
    /// JSON without `metadata.message_type`.
    NoType,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Json(e) => write!(f, "not JSON: {e}"),
            ParseError::NoType => write!(f, "no metadata.message_type"),
        }
    }
}

impl std::error::Error for ParseError {}

fn text(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Reads one socket message.
pub fn parse(json: &str) -> Result<Message, ParseError> {
    let v: Value = serde_json::from_str(json).map_err(|e| ParseError::Json(e.to_string()))?;
    let meta = v.get("metadata").ok_or(ParseError::NoType)?;
    let kind = meta
        .get("message_type")
        .and_then(Value::as_str)
        .ok_or(ParseError::NoType)?;
    let payload = v.get("payload").cloned().unwrap_or(Value::Null);
    Ok(match kind {
        "session_welcome" => {
            let s = payload.get("session").cloned().unwrap_or(Value::Null);
            Message::Welcome {
                session_id: text(&s, "id"),
                keepalive_seconds: s
                    .get("keepalive_timeout_seconds")
                    .and_then(Value::as_u64)
                    .map(|n| n.min(u64::from(u32::MAX)) as u32),
            }
        }
        "session_keepalive" => Message::Keepalive,
        "session_reconnect" => Message::Reconnect {
            url: payload
                .get("session")
                .map(|s| text(s, "reconnect_url"))
                .unwrap_or_default(),
        },
        "revocation" => {
            let s = payload.get("subscription").cloned().unwrap_or(Value::Null);
            Message::Revocation {
                kind: text(&s, "type"),
                status: text(&s, "status"),
            }
        }
        "notification" => {
            let kind = meta
                .get("subscription_type")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    payload
                        .get("subscription")
                        .map(|s| text(s, "type"))
                })
                .unwrap_or_default();
            let event = payload.get("event").cloned().unwrap_or(Value::Null);
            Message::Notification(read_event(&kind, &event))
        }
        other => Message::Other(other.to_string()),
    })
}

fn read_event(kind: &str, e: &Value) -> Event {
    match kind {
        "channel.chat.message" => Event::Message(read_message(e)),
        "channel.chat.message_delete" => Event::Delete {
            message_id: text(e, "message_id"),
        },
        "channel.chat.clear_user_messages" => Event::ClearUser {
            user_id: text(e, "target_user_id"),
        },
        "channel.chat.clear" => Event::Clear,
        other => Event::Other(other.to_string()),
    }
}

fn read_message(e: &Value) -> ChatMessage {
    let message = e.get("message").cloned().unwrap_or(Value::Null);
    let fragments: Vec<Fragment> = message
        .get("fragments")
        .and_then(Value::as_array)
        .map(|f| f.iter().map(read_fragment).collect())
        .unwrap_or_default();
    let cheered = e.get("cheer").is_some_and(|c| !c.is_null());
    let source = e
        .get("source_broadcaster_user_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let broadcaster = e.get("broadcaster_user_id").and_then(Value::as_str);
    ChatMessage {
        id: text(e, "message_id"),
        chatter_id: text(e, "chatter_user_id"),
        chatter_login: text(e, "chatter_user_login"),
        chatter_name: text(e, "chatter_user_name"),
        text: text(&message, "text"),
        cheer: cheered || fragments.iter().any(|f| matches!(f, Fragment::Cheermote(_))),
        shared: source.is_some_and(|s| Some(s) != broadcaster),
        kind: text(e, "message_type"),
        fragments,
    }
}

fn read_fragment(f: &Value) -> Fragment {
    let t = text(f, "text");
    match f.get("type").and_then(Value::as_str) {
        Some("text") => Fragment::Text(t),
        Some("emote") => {
            let emote = f.get("emote").cloned().unwrap_or(Value::Null);
            let still = emote
                .get("format")
                .and_then(Value::as_array)
                // No format given: Twitch's still image is always there.
                .is_none_or(|a| a.iter().any(|v| v.as_str() == Some("static")));
            Fragment::Emote {
                id: text(&emote, "id"),
                text: t,
                still,
            }
        }
        Some("cheermote") => Fragment::Cheermote(t),
        Some("mention") => Fragment::Mention(t),
        _ => Fragment::Other(t),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    #[test]
    fn the_session_messages_are_read() {
        assert_eq!(
            parse(&fixture("welcome.json")).unwrap(),
            Message::Welcome {
                session_id: "AQoQexAWVYKSTIu4ec_2VAxyuhAB".into(),
                keepalive_seconds: Some(10),
            }
        );
        assert_eq!(parse(&fixture("keepalive.json")).unwrap(), Message::Keepalive);
        assert_eq!(
            parse(&fixture("reconnect.json")).unwrap(),
            Message::Reconnect {
                url: "wss://eventsub.wss.twitch.tv?...".into()
            }
        );
        assert_eq!(
            parse(&fixture("revocation.json")).unwrap(),
            Message::Revocation {
                kind: "channel.chat.message".into(),
                status: "authorization_revoked".into(),
            }
        );
    }

    #[test]
    fn a_chat_message_keeps_its_fragments() {
        let Message::Notification(Event::Message(m)) =
            parse(&fixture("chat-message.json")).unwrap()
        else {
            panic!("a chat message");
        };
        assert_eq!(m.id, "cc106a89-1814-919d-454c-f4f2f970aae7");
        assert_eq!(m.chatter_id, "4145994");
        assert_eq!(m.chatter_name, "viewer32");
        assert_eq!(m.text, "Hi chat Kappa catJAM");
        assert_eq!(m.kind, "text");
        assert!(!m.cheer && !m.shared);
        assert_eq!(
            m.fragments,
            [
                Fragment::Text("Hi chat ".into()),
                Fragment::Emote {
                    text: "Kappa".into(),
                    id: "25".into(),
                    still: true
                },
                Fragment::Text(" catJAM".into()),
            ]
        );
    }

    #[test]
    fn cheers_and_shared_chat_are_told() {
        let Message::Notification(Event::Message(m)) =
            parse(&fixture("chat-message-cheer.json")).unwrap()
        else {
            panic!("a chat message");
        };
        assert!(m.cheer);
        let Message::Notification(Event::Message(m)) =
            parse(&fixture("chat-message-shared.json")).unwrap()
        else {
            panic!("a chat message");
        };
        assert!(m.shared);
        assert_eq!(m.fragments.len(), 1);
    }

    #[test]
    fn the_moderation_events_are_read() {
        assert_eq!(
            parse(&fixture("message-delete.json")).unwrap(),
            Message::Notification(Event::Delete {
                message_id: "ab24e0b0-2260-4bac-94e4-05eedd4ecd0e".into()
            })
        );
        assert_eq!(
            parse(&fixture("clear-user-messages.json")).unwrap(),
            Message::Notification(Event::ClearUser {
                user_id: "7734".into()
            })
        );
        assert_eq!(
            parse(&fixture("clear.json")).unwrap(),
            Message::Notification(Event::Clear)
        );
    }

    #[test]
    fn what_is_not_known_is_no_error() {
        assert_eq!(
            parse(r#"{"metadata":{"message_type":"something_new"},"payload":{}}"#).unwrap(),
            Message::Other("something_new".into())
        );
        assert_eq!(
            parse(
                r#"{"metadata":{"message_type":"notification","subscription_type":"channel.follow"},"payload":{"event":{}}}"#
            )
            .unwrap(),
            Message::Notification(Event::Other("channel.follow".into()))
        );
        assert_eq!(parse("{}"), Err(ParseError::NoType));
        assert!(matches!(parse("not json"), Err(ParseError::Json(_))));
    }
}
