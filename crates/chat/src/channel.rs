//! The live chat's emote sets (round TW6, C4): the channel's own sets and
//! the global ones of 7TV, BetterTTV and FrankerFaceZ, read when the chat
//! starts as round TW5's demo reads the globals (directly first, through
//! our server's proxy after a failure), sans-IO: the shells make the reads
//! [`ChannelSets::start`] asks for and hand the answers back
//! ([`ChannelSets::answered`]). Twitch's own emotes need no set: a
//! message's fragments name them by id.
//!
//! - "Emotes from: Twitch" asks no other provider.
//! - A channel the provider does not know (its `404`: the streamer has no
//!   7TV, BetterTTV or FrankerFaceZ set) is no failure and is not asked
//!   again through the proxy.
//! - A channel's word wins over a global one, 7TV's over BetterTTV's over
//!   FrankerFaceZ's ([`crate::emotes::Source`]).
//! - [`ChannelSets::report`] says, by provider and set, whether it was
//!   read and how, why each way failed, its emotes and how many of them
//!   animated.
//!
//! The parsers ([`crate::emotes`]) follow the global sets' shapes as they
//! answered on 2026-11-20; the channels' (`/v3/users/twitch/<id>`,
//! `/3/cached/users/twitch/<id>`, `/v1/room/id/<id>`) are read as their
//! documentation and the chat clients have them: UNVERIFIED until the
//! maintainer's run.

use serde_json::{Value, json};

use crate::EmotesFrom;
use crate::cache::Via;
use crate::emotes::{self, EmoteSets, Image, List, Provider, Source};
use crate::global::{local, provider_name};

/// A set to read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetRead {
    /// Its number, given back with the answer.
    pub key: usize,
    pub provider: Provider,
    /// The channel's (else the global one).
    pub channel: bool,
    pub url: String,
    pub via: Via,
}

#[derive(Clone, Debug, PartialEq)]
enum State {
    Waiting(Via),
    Asked(Via),
    Read(Via),
    /// The channel has no set there (the provider's 404).
    NoSet,
    Failed,
    /// "Emotes from: Twitch".
    NotAsked,
}

struct Set {
    provider: Provider,
    channel: bool,
    at: Image,
    state: State,
    why: Vec<String>,
    list: List,
}

/// A live chat's third-party emote sets.
pub struct ChannelSets {
    sets: Vec<Set>,
    api: Option<String>,
    at: Option<String>,
    version: u64,
}

impl ChannelSets {
    /// The sets of the channel of Twitch user `channel_id` and the globals,
    /// under the device's "Emotes from", our server's API at `api` (none:
    /// no proxy); TESTING ONLY, every provider's address served from `at`
    /// instead (its proxy at `<at>api`).
    pub fn new(
        channel_id: &str,
        from: EmotesFrom,
        api: Option<String>,
        at: Option<String>,
    ) -> ChannelSets {
        let api = match &at {
            Some(at) => Some(format!("{at}api")),
            None => api.map(|a| a.trim_end_matches('/').to_string()),
        };
        let mut sets = Vec::new();
        for p in [Provider::SevenTv, Provider::Bttv, Provider::Ffz] {
            // `set_addresses`: the channel's, then the global one.
            for (k, at) in emotes::set_addresses(p, channel_id).into_iter().enumerate() {
                sets.push(Set {
                    provider: p,
                    channel: k == 0,
                    at,
                    state: if from == EmotesFrom::TwitchOnly {
                        State::NotAsked
                    } else {
                        State::Waiting(Via::Direct)
                    },
                    why: Vec::new(),
                    list: Vec::new(),
                });
            }
        }
        ChannelSets {
            sets,
            api,
            at,
            version: 0,
        }
    }

    /// The proxy's base images are asked through after a failure.
    pub fn api(&self) -> Option<&str> {
        self.api.as_deref()
    }

    /// Bumped whenever a set was read: the overlay takes [`ChannelSets::sets`]
    /// again.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The reads to make now.
    pub fn start(&mut self) -> Vec<SetRead> {
        let mut out = Vec::new();
        for (key, set) in self.sets.iter_mut().enumerate() {
            let State::Waiting(via) = set.state else {
                continue;
            };
            let url = match via {
                Via::Direct => Some(local(&set.at.url, self.at.as_deref())),
                Via::Proxy => self.api.as_deref().and_then(|a| set.at.proxied(a)),
            };
            match url {
                Some(url) => {
                    set.state = State::Asked(via);
                    out.push(SetRead {
                        key,
                        provider: set.provider,
                        channel: set.channel,
                        url,
                        via,
                    });
                }
                None => {
                    set.why.push("no proxy to ask through".into());
                    set.state = State::Failed;
                }
            }
        }
        out
    }

    /// Read `key` ended: the answer's text, or why it failed (`HTTP 404`,
    /// a refused or failed request).
    pub fn answered(&mut self, key: usize, via: Via, answer: Result<String, String>) {
        let has_proxy = self.api.is_some();
        let at = self.at.clone();
        let Some(set) = self.sets.get_mut(key) else {
            return;
        };
        if set.state != State::Asked(via) {
            return;
        }
        let list = answer.and_then(|text| {
            let list = match (set.provider, set.channel) {
                (Provider::SevenTv, true) => emotes::seven_tv_user(&text),
                (Provider::SevenTv, false) => emotes::seven_tv_global(&text),
                (Provider::Bttv, true) => emotes::bttv_user(&text),
                (Provider::Bttv, false) => emotes::bttv_global(&text),
                (Provider::Ffz, true) => emotes::ffz_room(&text),
                (Provider::Ffz, false) => emotes::ffz_global(&text),
                _ => Vec::new(),
            };
            // A channel's set may be empty; a global one never is.
            if list.is_empty() && (!set.channel || serde_json::from_str::<Value>(&text).is_err()) {
                Err(format!("no emote read from {} bytes", text.len()))
            } else {
                Ok(list)
            }
        });
        match list {
            Ok(list) => {
                set.list = list
                    .into_iter()
                    .map(|(w, e, image)| {
                        let url = local(&image.url, at.as_deref());
                        (w, e, Image { url, ..image })
                    })
                    .collect();
                set.state = State::Read(via);
                self.version += 1;
            }
            Err(why) if set.channel && why.starts_with("HTTP 404") => {
                set.why.push(format!("the channel has no set ({why})"));
                set.state = State::NoSet;
            }
            Err(why) => {
                let way = if via == Via::Direct { "directly" } else { "through the proxy" };
                set.why.push(format!("{way}: {why}"));
                set.state = if via == Via::Direct && has_proxy && set.at.proxy.is_some() {
                    State::Waiting(Via::Proxy)
                } else {
                    State::Failed
                };
            }
        }
    }

    /// Whether every read has ended.
    pub fn done(&self) -> bool {
        self.sets
            .iter()
            .all(|s| !matches!(s.state, State::Waiting(_) | State::Asked(_)))
    }

    /// The words to match: every set read so far (online: Twitch's own
    /// images from the fragments too, served from `at` where a test says).
    pub fn sets(&self) -> EmoteSets {
        let mut out = EmoteSets::default();
        out.serve_from(self.at.clone());
        for s in &self.sets {
            if matches!(s.state, State::Read(_)) {
                let source = if s.channel {
                    Source::Channel(s.provider)
                } else {
                    Source::Global(s.provider)
                };
                out.add(source, s.list.clone());
            }
        }
        out
    }

    /// What each provider answered, by set: `read` (and how), `no set`,
    /// `failed`, `asked`, `waiting` or `not asked`, why each way failed,
    /// its emotes, how many animated.
    pub fn report(&self) -> Value {
        Value::Array(
            self.sets
                .iter()
                .map(|s| {
                    let (state, via) = match &s.state {
                        State::Waiting(v) => ("waiting", Some(*v)),
                        State::Asked(v) => ("asked", Some(*v)),
                        State::Read(v) => ("read", Some(*v)),
                        State::NoSet => ("no set", None),
                        State::Failed => ("failed", None),
                        State::NotAsked => ("not asked", None),
                    };
                    json!({
                        "provider": provider_name(s.provider),
                        "set": if s.channel { "channel" } else { "global" },
                        "state": state,
                        "via": via.map(|v| if v == Via::Direct { "direct" } else { "proxy" }),
                        "why": s.why,
                        "emotes": s.list.len(),
                        "animated": s.list.iter().filter(|(_, e, _)| e.animated).count(),
                    })
                })
                .collect(),
        )
    }

    /// The report in one line a set, for desktop's stderr.
    pub fn lines(&self) -> Vec<String> {
        self.report()
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| {
                format!(
                    "emotes {} {}: {}{} ({} emotes, {} animated){}",
                    r["provider"].as_str().unwrap_or(""),
                    r["set"].as_str().unwrap_or(""),
                    r["state"].as_str().unwrap_or(""),
                    r["via"].as_str().map(|v| format!(" {v}")).unwrap_or_default(),
                    r["emotes"],
                    r["animated"],
                    r["why"]
                        .as_array()
                        .filter(|w| !w.is_empty())
                        .map(|w| format!("; {}", w.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("; ")))
                        .unwrap_or_default(),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    fn answer(r: &SetRead) -> String {
        fixture(match (r.provider, r.channel) {
            (Provider::SevenTv, true) => "7tv-user.json",
            (Provider::SevenTv, false) => "7tv-global.json",
            (Provider::Bttv, true) => "bttv-user.json",
            (Provider::Bttv, false) => "bttv-global.json",
            (Provider::Ffz, true) => "ffz-room.json",
            _ => "ffz-global.json",
        })
    }

    /// The channel's and the global sets of the three providers, each
    /// asked directly once, read by their parsers; a channel's word over a
    /// global one.
    #[test]
    fn the_channels_and_the_global_sets_are_read() {
        let mut c = ChannelSets::new("141981764", EmotesFrom::TwitchAndOthers, Some("https://api.test/x".into()), None);
        let reads = c.start();
        assert_eq!(
            reads.iter().map(|r| r.url.as_str()).collect::<Vec<_>>(),
            [
                "https://7tv.io/v3/users/twitch/141981764",
                "https://7tv.io/v3/emote-sets/global",
                "https://api.betterttv.net/3/cached/users/twitch/141981764",
                "https://api.betterttv.net/3/cached/emotes/global",
                "https://api.frankerfacez.com/v1/room/id/141981764",
                "https://api.frankerfacez.com/v1/set/global",
            ]
        );
        assert!(c.start().is_empty());
        let v0 = c.version();
        for r in &reads {
            c.answered(r.key, r.via, Ok(answer(r)));
        }
        assert!(c.done());
        assert!(c.version() > v0);
        let sets = c.sets();
        assert!(!sets.is_empty());
        let report = c.report();
        for r in report.as_array().unwrap() {
            assert_eq!(r["state"], "read", "{r}");
            assert!(r["emotes"].as_u64().unwrap() > 0, "{r}");
        }
        assert_eq!(c.lines().len(), 6);
    }

    /// A channel the provider does not know is no failure; a failed
    /// direct read goes through the proxy; "Emotes from: Twitch" asks
    /// nothing.
    #[test]
    fn a_missing_channel_set_and_the_proxy() {
        let mut c = ChannelSets::new("9", EmotesFrom::TwitchAndOthers, Some("https://api.test".into()), None);
        for r in c.start() {
            match (r.provider, r.channel) {
                (Provider::SevenTv, true) => c.answered(r.key, r.via, Err("HTTP 404".into())),
                (Provider::Bttv, false) => c.answered(r.key, r.via, Err("TypeError: Failed to fetch".into())),
                _ => c.answered(r.key, r.via, Ok(answer(&r))),
            }
        }
        let again = c.start();
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].url, "https://api.test/emotes/bttv-api/3/cached/emotes/global");
        c.answered(again[0].key, again[0].via, Ok(answer(&again[0])));
        let report = c.report();
        assert_eq!(report[0]["state"], "no set");
        assert_eq!(report[3]["state"], "read");
        assert_eq!(report[3]["via"], "proxy");
        assert!(c.done());
        let mut c = ChannelSets::new("9", EmotesFrom::TwitchOnly, Some("https://api.test".into()), None);
        assert!(c.start().is_empty());
        assert!(c.done());
        assert_eq!(c.report()[0]["state"], "not asked");
    }

    /// TESTING ONLY: every address served from a local base, Twitch's own
    /// fragment images too.
    #[test]
    fn a_local_base_serves_every_address() {
        let mut c = ChannelSets::new("9", EmotesFrom::TwitchAndOthers, None, Some("http://127.0.0.1:9/g/".into()));
        let reads = c.start();
        assert_eq!(reads[0].url, "http://127.0.0.1:9/g/7tv.io/v3/users/twitch/9");
        assert_eq!(c.api(), Some("http://127.0.0.1:9/g/api"));
        let image = c.sets().image(&emotes::Emote::twitch("25")).unwrap();
        assert_eq!(image.url, "http://127.0.0.1:9/g/static-cdn.jtvnw.net/emoticons/v2/25/static/dark/2.0");
    }
}
