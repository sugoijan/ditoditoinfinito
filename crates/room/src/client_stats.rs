//! A member's own usage report on its room's socket (the stats proposal's
//! step 2, `stats-v1`, room model revision 13): what kind of client this is
//! and how its previous socket in the room fared, in fixed vocabularies and
//! ranges, so the server can add it to its daily totals beside what it sees
//! by itself (ddi-server's `stats.rs`).
//!
//! **Nothing identifying**: every value is one of a fixed list below (the
//! build a version's digits), no free text, no address, no id, no name, no
//! room, no song. The server refuses a report with anything else whole.
//!
//! The wire: its own envelope, `{"v":2,"stats":{…}}` ([`ClientStatsEnvelope`]),
//! one a socket, sent once its first snapshot came, only where the person
//! left Options' "Usage statistics" on and the shell said what it is. Not
//! inside `live`: a room's reader closes a socket on a frame it cannot read
//! (4400), and the server never closes one over a report: it tells one by
//! its prefix ([`is_stats`]), takes or drops it, and answers nothing. A
//! server that would read none (it predates revision 13) is never reached:
//! the client refuses an older authority before any socket opens; one that
//! dropped a report anyway changes nothing for the client, which waits for
//! no answer.
//!
//! What the client measures itself ([`Tracker`], inside
//! [`crate::OnlineAuthority`]): each socket's life, how it ended (a close
//! frame, a drop, the keep-alive's verdict, the client starting over), the
//! keep-alive's state on it, the room clock's round trip, the tickets it
//! took to get a socket again, and, from the shell, how long the page was
//! hidden while it lived.

use serde::{Deserialize, Serialize};

/// The report's schema (`s`).
pub const SCHEMA: u32 = 1;

/// A report's text at most (bytes); a longer one is refused whole.
pub const MAX_BYTES: usize = 1_024;

pub const APPS: [&str; 3] = ["web", "desktop", "bot"];
pub const SYSTEMS: [&str; 8] = [
    "windows", "macos", "linux", "ios", "android", "chromeos", "other", "unknown",
];
pub const FORMS: [&str; 4] = ["phone", "tablet", "computer", "unknown"];
pub const GPUS: [&str; 9] = [
    "webgpu", "webgl2", "metal", "vulkan", "dx12", "gl", "other", "none", "unknown",
];
pub const TOUCH: [&str; 3] = ["yes", "no", "unknown"];
/// This socket's number in the room since the client joined it.
pub const NTHS: [&str; 5] = ["1", "2", "3-4", "5-10", "gt10"];
/// Tickets asked for since the last socket ended (or the join) until this
/// one had the room: 1 where the first try worked.
pub const TRIES: [&str; 5] = ["1", "2", "3-4", "5-8", "gt8"];
/// How the previous socket ended, as the client saw it.
pub const FATES: [&str; 4] = ["closed", "dropped", "keepalive_dead", "restarted"];
/// Its close code as the client saw it (the server's list, then `none`).
pub const CODES: [&str; 14] = [
    "1000", "1001", "1005", "1006", "1011", "1012", "4000", "4400", "4401", "4403", "4410", "4429",
    "other", "none",
];
/// How long it lived (the server's own lifetimes' ranges).
pub const LIFETIMES: [&str; 6] = ["lt10s", "10-60s", "1-5m", "5-20m", "20-60m", "gt1h"];
/// How long the page was hidden while it lived.
pub const HIDDEN: [&str; 7] = ["0", "lt5s", "5-20s", "20-60s", "1-5m", "gt5m", "unknown"];
/// Whether the page was hidden when it ended, and for how long by then.
pub const AT_END: [&str; 6] = ["visible", "lt5s", "5-20s", "20-60s", "gt60s", "unknown"];
/// The keep-alive on it: none sent, every one answered, one awaiting its
/// answer when it ended, taken as dead, or the transport's own (desktop's).
pub const KEEPALIVES: [&str; 5] = ["none_sent", "answered", "awaiting", "dead", "own"];
/// The room clock's round trip (ms; the server's own ranges).
pub const RTTS: [&str; 7] = [
    "lt20", "20-50", "50-100", "100-200", "200-400", "gt400", "unknown",
];

/// A value in ranges: `edges` ascending, `labels` one more than them.
fn bucket(v: u64, edges: &[u64], labels: &'static [&'static str]) -> &'static str {
    let i = edges.iter().position(|e| v < *e).unwrap_or(edges.len());
    labels[i]
}

pub fn nth(n: u32) -> &'static str {
    bucket(u64::from(n.max(1)), &[2, 3, 5, 11], &NTHS)
}

pub fn tries(n: u32) -> &'static str {
    bucket(u64::from(n.max(1)), &[2, 3, 5, 9], &TRIES)
}

pub fn code(code: Option<u16>) -> &'static str {
    match code {
        None => "none",
        Some(c) => CODES[..13]
            .iter()
            .find(|x| x.parse::<u16>() == Ok(c))
            .copied()
            .unwrap_or("other"),
    }
}

pub fn lifetime(ms: u64) -> &'static str {
    bucket(
        ms,
        &[10_000, 60_000, 300_000, 1_200_000, 3_600_000],
        &LIFETIMES,
    )
}

pub fn hidden(ms: u64) -> &'static str {
    if ms == 0 {
        return "0";
    }
    bucket(ms, &[5_000, 20_000, 60_000, 300_000], &HIDDEN[1..6])
}

/// How long the page had been hidden when the socket ended (none: shown).
pub fn at_end(hidden_ms: Option<u64>) -> &'static str {
    match hidden_ms {
        None => "visible",
        Some(ms) => bucket(ms, &[5_000, 20_000, 60_000], &AT_END[1..5]),
    }
}

pub fn rtt(ms: Option<u64>) -> &'static str {
    match ms {
        Some(ms) => bucket(ms, &[20, 50, 100, 200, 400], &RTTS[..6]),
        None => "unknown",
    }
}

/// A build: one to four parts of one to four digits (`0.1.0`).
pub fn valid_build(b: &str) -> bool {
    let parts: Vec<&str> = b.split('.').collect();
    (1..=4).contains(&parts.len())
        && parts
            .iter()
            .all(|p| (1..=4).contains(&p.len()) && p.bytes().all(|c| c.is_ascii_digit()))
}

/// What a client is, as its shell says (every value from the lists above;
/// [`ClientKind::checked`] puts anything else to `unknown`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientKind {
    pub app: &'static str,
    pub system: &'static str,
    pub form: &'static str,
    pub gpu: &'static str,
    pub touch: &'static str,
    pub build: &'static str,
}

impl ClientKind {
    /// Each value in its list (else `unknown`, the app `web`'s or the
    /// build `0`).
    pub fn checked(self) -> ClientKind {
        let pick = |v: &'static str, list: &[&'static str], or: &'static str| {
            if list.contains(&v) { v } else { or }
        };
        ClientKind {
            app: pick(self.app, &APPS, "web"),
            system: pick(self.system, &SYSTEMS, "unknown"),
            form: pick(self.form, &FORMS, "unknown"),
            gpu: pick(self.gpu, &GPUS, "unknown"),
            touch: pick(self.touch, &TOUCH, "unknown"),
            build: if valid_build(self.build) {
                self.build
            } else {
                "0"
            },
        }
    }
}

/// A browser's system family and form from its User-Agent, its touch
/// points and the screen's shorter side (CSS px), read on the device and
/// never sent as such: an iPad that asks for desktop pages says it is a
/// Mac but has touch points.
pub fn web_device(
    user_agent: &str,
    touch_points: u32,
    short_side: f64,
) -> (&'static str, &'static str) {
    let has = |s: &str| user_agent.contains(s);
    let ipad_as_mac = has("Macintosh") && touch_points > 1;
    let system = if has("iPhone") || has("iPad") || has("iPod") || ipad_as_mac {
        "ios"
    } else if has("Android") {
        "android"
    } else if has("CrOS") {
        "chromeos"
    } else if has("Windows") {
        "windows"
    } else if has("Macintosh") || has("Mac OS X") {
        "macos"
    } else if has("Linux") || has("X11") {
        "linux"
    } else {
        "other"
    };
    let form = if has("iPad") || ipad_as_mac || has("Tablet") {
        "tablet"
    } else if has("iPhone") || has("iPod") {
        "phone"
    } else if has("Android") {
        if has("Mobile") { "phone" } else { "tablet" }
    } else if has("Mobile") || (touch_points > 0 && short_side > 0.0 && short_side < 500.0) {
        // A small touch screen with no word of its own (a phone's
        // browser asking for desktop pages).
        "phone"
    } else {
        "computer"
    };
    (system, form)
}

/// The previous socket in the room, as the client saw it end.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocketFate {
    pub fate: String,
    pub code: String,
    pub life: String,
    pub hidden: String,
    pub at_end: String,
    pub keepalive: String,
    pub rtt: String,
}

/// A member's report (`{"v":2,"stats":{…}}`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientStats {
    pub s: u32,
    pub app: String,
    pub system: String,
    pub form: String,
    pub gpu: String,
    pub touch: String,
    pub build: String,
    pub nth: String,
    pub tries: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev: Option<SocketFate>,
}

envelope!(ClientStatsEnvelope, stats, ClientStats, Eq);

impl SocketFate {
    fn check(&self) -> bool {
        FATES.contains(&self.fate.as_str())
            && CODES.contains(&self.code.as_str())
            && LIFETIMES.contains(&self.life.as_str())
            && HIDDEN.contains(&self.hidden.as_str())
            && AT_END.contains(&self.at_end.as_str())
            && KEEPALIVES.contains(&self.keepalive.as_str())
            && RTTS.contains(&self.rtt.as_str())
    }
}

impl ClientStats {
    /// Every value in its list and the schema known: a report taken whole
    /// or not at all.
    pub fn check(&self) -> bool {
        self.s == SCHEMA
            && APPS.contains(&self.app.as_str())
            && SYSTEMS.contains(&self.system.as_str())
            && FORMS.contains(&self.form.as_str())
            && GPUS.contains(&self.gpu.as_str())
            && TOUCH.contains(&self.touch.as_str())
            && valid_build(&self.build)
            && NTHS.contains(&self.nth.as_str())
            && TRIES.contains(&self.tries.as_str())
            && self.prev.as_ref().is_none_or(SocketFate::check)
    }
}

/// Whether a socket's text is a usage report, by its prefix alone
/// (`{"v":<digits>,"stats":`, as the client writes it): the server tells
/// one without reading a long command.
pub fn is_stats(text: &str) -> bool {
    let Some(rest) = text.strip_prefix("{\"v\":") else {
        return false;
    };
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    (1..=3).contains(&digits) && rest[digits..].starts_with(",\"stats\":")
}

/// A usage report's text read: `None` where it is not one ([`is_stats`]),
/// `Some(None)` where it is one refused whole (too long, unreadable, a
/// value outside its list, an unknown field or schema).
pub fn read(text: &str) -> Option<Option<ClientStats>> {
    if !is_stats(text) {
        return None;
    }
    if text.len() > MAX_BYTES {
        return Some(None);
    }
    Some(
        serde_json::from_str::<ClientStatsEnvelope>(text)
            .ok()
            .map(|e| e.stats)
            .filter(ClientStats::check),
    )
}

/// How a socket ended, as the client saw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    /// A close came (its code; 1005 and 1006 are a socket that dropped).
    Closed(u16),
    /// The keep-alive went unanswered (`crate::online::KEEPALIVE_V0`).
    KeepAliveDead,
    /// The client started over (signed in again, "Use this device").
    Restarted,
}

/// The keep-alive on a socket when it ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeepAliveSeen {
    /// The transport keeps itself alive (desktop's).
    pub own: bool,
    /// Keep-alives sent on it.
    pub sent: u64,
    /// One sent and not answered when it ended.
    pub awaiting: bool,
}

#[derive(Clone, Copy, Debug)]
struct Socket {
    opened_at: u64,
    /// Hidden time while it lived, the spell going on not counted yet.
    hidden_ms: u64,
    /// It had the room (its first snapshot).
    synced: bool,
    /// The keep-alives sent before it opened.
    keep_sent_before: u64,
}

/// The client's side ([`crate::OnlineAuthority`] keeps one): its sockets'
/// fates, measured sans-IO on the authority's clock, and the report due
/// when a socket has the room.
#[derive(Clone, Debug, Default)]
pub struct Tracker {
    /// Sockets that had the room so far.
    sockets: u32,
    /// Tickets used since the last socket that had the room ended.
    tries: u32,
    current: Option<Socket>,
    /// The last socket that had the room, as it ended.
    prev: Option<SocketFate>,
    /// A socket has the room and its report is not sent yet: its `nth`,
    /// `tries` and `prev`.
    due: Option<(u32, u32, Option<SocketFate>)>,
    /// The page hidden (the shell said so at least once), since when.
    hidden: Option<bool>,
    hidden_since: u64,
}

impl Tracker {
    /// A socket opens (a ticket given to it).
    pub fn opening(&mut self, now: u64, keep_sent: u64) {
        self.current = Some(Socket {
            opened_at: now,
            hidden_ms: 0,
            synced: false,
            keep_sent_before: keep_sent,
        });
        self.tries = self.tries.saturating_add(1);
    }

    /// The socket had the room: its report is due.
    pub fn synced(&mut self) {
        let Some(s) = self.current.as_mut().filter(|s| !s.synced) else {
            return;
        };
        s.synced = true;
        self.sockets = self.sockets.saturating_add(1);
        self.due = Some((self.sockets, self.tries, self.prev.take()));
        self.tries = 0;
    }

    /// The socket ended (only one that had the room is remembered for the
    /// next report; one that never had it counts in `tries`).
    pub fn ended(&mut self, now: u64, how: Ending, keep: KeepAliveSeen, rtt_ms: Option<u64>) {
        let Some(s) = self.current.take() else {
            return;
        };
        if !s.synced {
            return;
        }
        let spell = match self.hidden {
            Some(true) => Some(now.saturating_sub(self.hidden_since.max(s.opened_at))),
            _ => None,
        };
        let (fate, code_seen) = match how {
            Ending::Closed(c @ (1005 | 1006)) => ("dropped", Some(c)),
            Ending::Closed(c) => ("closed", Some(c)),
            Ending::KeepAliveDead => ("keepalive_dead", None),
            Ending::Restarted => ("restarted", None),
        };
        let keepalive = if keep.own {
            "own"
        } else if how == Ending::KeepAliveDead {
            "dead"
        } else if keep.awaiting {
            "awaiting"
        } else if keep.sent > s.keep_sent_before {
            "answered"
        } else {
            "none_sent"
        };
        let (hidden_label, end_label) = match self.hidden {
            None => ("unknown", "unknown"),
            Some(_) => (
                hidden(s.hidden_ms + spell.unwrap_or(0)),
                at_end(spell.map(|_| now.saturating_sub(self.hidden_since))),
            ),
        };
        self.prev = Some(SocketFate {
            fate: fate.into(),
            code: code(code_seen).into(),
            life: lifetime(now.saturating_sub(s.opened_at)).into(),
            hidden: hidden_label.into(),
            at_end: end_label.into(),
            keepalive: keepalive.into(),
            rtt: rtt(rtt_ms).into(),
        });
    }

    /// The page hidden or shown at `at` (the shell's; the same clock).
    pub fn set_hidden(&mut self, hidden: bool, at: u64) {
        if self.hidden == Some(hidden) {
            return;
        }
        if self.hidden == Some(true)
            && let Some(s) = self.current.as_mut()
        {
            s.hidden_ms += at.saturating_sub(self.hidden_since.max(s.opened_at));
        }
        self.hidden = Some(hidden);
        self.hidden_since = at;
    }

    /// A report is due (a socket has the room, nothing sent for it yet).
    pub fn due(&self) -> bool {
        self.due.is_some()
    }

    /// The due report for a client of `kind`, once; `None` (the person
    /// turned it off, or the shell said nothing of itself) drops it.
    pub fn take(&mut self, kind: Option<&ClientKind>) -> Option<ClientStats> {
        let (n, t, prev) = self.due.take()?;
        let k = kind?.checked();
        Some(ClientStats {
            s: SCHEMA,
            app: k.app.into(),
            system: k.system.into(),
            form: k.form.into(),
            gpu: k.gpu.into(),
            touch: k.touch.into(),
            build: k.build.into(),
            nth: nth(n).into(),
            tries: tries(t).into(),
            prev,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind() -> ClientKind {
        ClientKind {
            app: "web",
            system: "ios",
            form: "tablet",
            gpu: "webgpu",
            touch: "yes",
            build: "0.1.0",
        }
    }

    #[test]
    fn a_report_reads_back_and_is_told_by_its_prefix() {
        let mut t = Tracker::default();
        t.set_hidden(false, 0);
        t.opening(1_000, 0);
        t.synced();
        let first = t.take(Some(&kind())).unwrap();
        assert_eq!((first.nth.as_str(), first.tries.as_str()), ("1", "1"));
        assert!(first.prev.is_none());
        t.set_hidden(true, 61_000);
        t.ended(
            91_000,
            Ending::Closed(1006),
            KeepAliveSeen {
                own: false,
                sent: 1,
                awaiting: true,
            },
            Some(73),
        );
        t.opening(92_000, 1);
        t.opening(95_000, 1);
        t.set_hidden(false, 99_000);
        t.synced();
        let second = t.take(Some(&kind())).unwrap();
        assert_eq!((second.nth.as_str(), second.tries.as_str()), ("2", "2"));
        let prev = second.prev.clone().unwrap();
        assert_eq!(
            prev,
            SocketFate {
                fate: "dropped".into(),
                code: "1006".into(),
                life: "1-5m".into(),
                hidden: "20-60s".into(),
                at_end: "20-60s".into(),
                keepalive: "awaiting".into(),
                rtt: "50-100".into(),
            }
        );
        let text = ClientStatsEnvelope::text(second.clone());
        assert!(text.starts_with("{\"v\":2,\"stats\":{"), "{text}");
        assert!(text.len() < MAX_BYTES);
        assert_eq!(read(&text), Some(Some(second)));
        // Not one: a command, or anything else.
        assert_eq!(read(r#"{"v":2,"client":"c","seq":1,"cmd":"x"}"#), None);
        assert_eq!(read(r#"{"v":2,"live":{"kind":"hello","version":1}}"#), None);
        assert_eq!(read("ping"), None);
    }

    #[test]
    fn a_report_with_anything_outside_its_lists_is_refused_whole() {
        let mut t = Tracker::default();
        t.opening(0, 0);
        t.synced();
        let good = t.take(Some(&kind())).unwrap();
        let text = ClientStatsEnvelope::text(good);
        for (from, to) in [
            ("\"app\":\"web\"", "\"app\":\"Kirarin\""),
            ("\"build\":\"0.1.0\"", "\"build\":\"0.1.0-me@example.com\""),
            ("\"build\":\"0.1.0\"", "\"build\":\"203.0.113.7.1\""),
            (
                "\"form\":\"tablet\"",
                "\"form\":\"tablet\",\"name\":\"Kirarin\"",
            ),
            ("\"s\":1", "\"s\":9"),
        ] {
            let bad = text.replace(from, to);
            assert_ne!(bad, text);
            assert_eq!(read(&bad), Some(None), "{bad}");
        }
        let long = text.replace("\"0.1.0\"", &format!("\"{}\"", "1".repeat(2_000)));
        assert_eq!(read(&long), Some(None));
    }

    #[test]
    fn nothing_is_sent_without_a_kind_and_a_socket_that_never_had_the_room_is_a_try() {
        let mut t = Tracker::default();
        t.opening(0, 0);
        t.synced();
        assert!(t.due());
        assert_eq!(t.take(None), None);
        assert!(!t.due());
        // Desktop's own keep-alive, a socket that never had the room.
        t.ended(
            5_000,
            Ending::Closed(4401),
            KeepAliveSeen {
                own: true,
                sent: 0,
                awaiting: false,
            },
            None,
        );
        t.opening(6_000, 0);
        t.ended(
            6_500,
            Ending::Closed(1006),
            KeepAliveSeen {
                own: true,
                sent: 0,
                awaiting: false,
            },
            None,
        );
        t.opening(7_000, 0);
        t.synced();
        let r = t.take(Some(&kind())).unwrap();
        assert_eq!(r.tries, "2");
        let prev = r.prev.unwrap();
        assert_eq!(
            (
                prev.fate.as_str(),
                prev.code.as_str(),
                prev.keepalive.as_str()
            ),
            ("closed", "4401", "own")
        );
        // The page's visibility never said: unknown.
        assert_eq!(
            (prev.hidden.as_str(), prev.at_end.as_str()),
            ("unknown", "unknown")
        );
    }

    #[test]
    fn a_browser_says_its_system_and_form() {
        let ipad = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
                    (KHTML, like Gecko) Version/17.4 Safari/605.1.15";
        assert_eq!(web_device(ipad, 5, 820.0), ("ios", "tablet"));
        assert_eq!(web_device(ipad, 0, 900.0), ("macos", "computer"));
        let pixel = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 \
                     (KHTML, like Gecko) Chrome/130.0 Mobile Safari/537.36";
        assert_eq!(web_device(pixel, 5, 412.0), ("android", "phone"));
        let win =
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:132.0) Gecko/20100101 Firefox/132.0";
        assert_eq!(web_device(win, 0, 1080.0), ("windows", "computer"));
        assert!(valid_build(env!("CARGO_PKG_VERSION")));
        assert!(!valid_build("") && !valid_build("1..2") && !valid_build("v1"));
    }
}
