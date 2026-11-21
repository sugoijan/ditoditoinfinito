//! The global-emotes demo (TESTING ONLY, round TW5): a scripted chat whose
//! emotes are real ones, read from the four providers' global sets by the
//! code the real chat will use, for the maintainer to run (the web's
//! `mpfakechat=global`, desktop's `--mp-dev-chat global`). Sans-IO: the
//! shells make the reads [`Demo::start`] asks for and hand the answers
//! back ([`Demo::answered`]); the script ([`Demo::script`]) is a
//! [`Script`] whose images the layer's cache asks for as a real chat's
//! (directly, then through our server's proxy).
//!
//! - Each set is asked directly first and through our server's proxy
//!   after a failure (C4); Twitch's own only through our server, which
//!   asks Helix with the app's token (`emotes/twitch-api/chat/emotes/global`).
//! - "Emotes from: Twitch" asks no other provider.
//! - From each set an assortment: [`ANIMATED`] animated emotes first, then
//!   [`STILL`] still ones (more of them where a provider has fewer animated
//!   ones: on 2026-11-20 7TV's globals had many, BetterTTV's one,
//!   FrankerFaceZ's none), chosen by a hash of their ids (so two runs, and
//!   a provider that reorders its set, look alike), a word another
//!   provider's emote already took left out. What the parsers leave out
//!   is never drawn: 7TV's zero-width emotes, BetterTTV's and
//!   FrankerFaceZ's modifiers, FrankerFaceZ's hidden ones and its sets
//!   outside `default_sets`.
//! - [`Demo::report`] says what each provider answered, for the debug
//!   state: so a failure names its provider and how.
//!
//! The sets' JSON is read by [`crate::emotes`]' parsers, written from the
//! providers' documentation: the demo is where they first meet the real
//! answers.

use serde_json::{Value, json};

use crate::EmotesFrom;
use crate::cache::Via;
use crate::emotes::{self, Emote, Image, List, Provider};
use crate::script::Script;

/// Animated emotes chosen from each provider's set, at most.
pub const ANIMATED: usize = 6;
/// Still emotes chosen beside them, at most.
pub const STILL: usize = 3;

/// The providers in the order their words are taken.
const ORDER: [Provider; 4] = [Provider::Twitch, Provider::SevenTv, Provider::Bttv, Provider::Ffz];

/// A read for a shell to make: a provider's global set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetRead {
    pub provider: Provider,
    pub url: String,
    pub via: Via,
}

#[derive(Clone, Debug, PartialEq)]
enum State {
    /// To ask (this way).
    Waiting(Via),
    Asked(Via),
    Read(Via),
    Failed,
    /// "Emotes from: Twitch": never asked.
    NotAsked,
}

struct Set {
    provider: Provider,
    at: Image,
    state: State,
    /// Why each way failed, in turn.
    why: Vec<String>,
    list: List,
}

/// The demo's reads and what they gave.
pub struct Demo {
    sets: Vec<Set>,
    api: Option<String>,
    /// TESTING ONLY: every provider's address served from here instead
    /// (`<at><host>/<path>`; the proxy at `<at>api`).
    at: Option<String>,
}

/// A provider's name in the report.
pub fn provider_name(p: Provider) -> &'static str {
    match p {
        Provider::Twitch => "twitch",
        Provider::SevenTv => "7tv",
        Provider::Bttv => "bttv",
        Provider::Ffz => "ffz",
        Provider::Test => "test",
    }
}

/// `url` (`https://host/path`) as served from `at` (`<at>host/path`):
/// TESTING ONLY, a local folder or server of fixtures standing for the
/// providers' hosts.
pub(crate) fn local(url: &str, at: Option<&str>) -> String {
    match at {
        Some(at) if !url.is_empty() => {
            let rest = url.split_once("://").map_or(url, |(_, r)| r);
            format!("{at}{rest}")
        }
        _ => url.to_string(),
    }
}

/// A number from an id, the same every run (FNV-1a).
fn stable(id: &str) -> u64 {
    id.bytes()
        .fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3))
}

impl Demo {
    /// The demo's reads under the device's "Emotes from", our server's
    /// API at `api` (none: no proxy); TESTING ONLY, every provider's
    /// address served from `at` instead (its proxy at `<at>api`).
    pub fn new(from: EmotesFrom, api: Option<String>, at: Option<String>) -> Demo {
        let api = match &at {
            Some(at) => Some(format!("{at}api")),
            None => api.map(|a| a.trim_end_matches('/').to_string()),
        };
        let sets = ORDER
            .into_iter()
            .filter_map(|p| {
                let at = emotes::global_address(p)?;
                let state = if from == EmotesFrom::TwitchOnly && p != Provider::Twitch {
                    State::NotAsked
                } else if at.url.is_empty() {
                    State::Waiting(Via::Proxy)
                } else {
                    State::Waiting(Via::Direct)
                };
                Some(Set {
                    provider: p,
                    at,
                    state,
                    why: Vec::new(),
                    list: Vec::new(),
                })
            })
            .collect();
        Demo { sets, api, at }
    }

    /// The proxy's base the demo's images are asked through.
    pub fn api(&self) -> Option<&str> {
        self.api.as_deref()
    }

    fn address(&self, set: &Set, via: Via) -> Option<String> {
        match via {
            Via::Direct => Some(local(&set.at.url, self.at.as_deref())),
            Via::Proxy => set.at.proxied(self.api.as_deref()?),
        }
    }

    /// The reads to make now (each set once, a proxy's after a direct
    /// failure).
    pub fn start(&mut self) -> Vec<SetRead> {
        let mut out = Vec::new();
        for k in 0..self.sets.len() {
            let State::Waiting(via) = self.sets[k].state else {
                continue;
            };
            match self.address(&self.sets[k], via) {
                Some(url) => {
                    self.sets[k].state = State::Asked(via);
                    out.push(SetRead {
                        provider: self.sets[k].provider,
                        url,
                        via,
                    });
                }
                None => {
                    self.sets[k].why.push("no proxy to ask through".into());
                    self.sets[k].state = State::Failed;
                }
            }
        }
        out
    }

    /// A read ended: the answer's text, or why it failed (a refused or
    /// failed request, a status). An answer the provider's parser reads no
    /// emote from fails too; a direct failure is asked again through the
    /// proxy.
    pub fn answered(&mut self, provider: Provider, via: Via, answer: Result<String, String>) {
        let has_proxy = self.api.is_some();
        let at = self.at.clone();
        let Some(set) = self.sets.iter_mut().find(|s| s.provider == provider) else {
            return;
        };
        if set.state != State::Asked(via) {
            return;
        }
        let list = answer.and_then(|text| {
            let list = match provider {
                Provider::Twitch => emotes::twitch_global(&text),
                Provider::SevenTv => emotes::seven_tv_global(&text),
                Provider::Bttv => emotes::bttv_global(&text),
                Provider::Ffz => emotes::ffz_global(&text),
                Provider::Test => Vec::new(),
            };
            if list.is_empty() {
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
            .all(|s| matches!(s.state, State::Read(_) | State::Failed | State::NotAsked))
    }

    /// The emotes chosen: from each set read, [`ANIMATED`] animated and
    /// [`STILL`] still ones by a hash of their ids, a word taken once.
    pub fn chosen(&self) -> List {
        let mut out: List = Vec::new();
        for set in &self.sets {
            let mut list: Vec<&(String, Emote, Image)> = set
                .list
                .iter()
                .filter(|(w, ..)| !w.is_empty() && !w.contains(char::is_whitespace))
                .collect();
            list.sort_by_key(|(_, e, _)| (stable(&e.id), e.id.clone()));
            let mut taken = 0;
            for animated in [true, false] {
                let most = if animated { ANIMATED } else { STILL };
                let mut n = 0;
                for (w, e, i) in list.iter().filter(|(_, e, _)| e.animated == animated) {
                    if n == most {
                        break;
                    }
                    if out.iter().any(|(had, ..)| had == w) {
                        continue;
                    }
                    out.push((w.clone(), e.clone(), i.clone()));
                    n += 1;
                }
                taken += n;
            }
            // Fewer animated ones than wanted: more still ones in their
            // place.
            let more = (ANIMATED + STILL).saturating_sub(taken);
            let extra: Vec<_> = list
                .iter()
                .filter(|(w, e, _)| !e.animated && !out.iter().any(|(had, ..)| had == w))
                .take(more)
                .map(|(w, e, i)| (w.clone(), e.clone(), i.clone()))
                .collect();
            out.extend(extra);
        }
        out
    }

    /// The chat to play now: the emotes chosen from the sets read so far
    /// ([`Script::global`]).
    pub fn script(&self) -> Script {
        Script::global(&self.chosen(), self.api.clone())
    }

    /// What each provider answered, for the debug state: whether its set
    /// was read and how (`read`, `failed`, `asked`, `waiting`, `not asked`
    /// under "Emotes from: Twitch"), why each way failed, the emotes it
    /// gave, how many of them animated, and those chosen.
    pub fn report(&self) -> Value {
        let chosen = self.chosen();
        Value::Array(
            self.sets
                .iter()
                .map(|s| {
                    let (state, via) = match &s.state {
                        State::Waiting(v) => ("waiting", Some(*v)),
                        State::Asked(v) => ("asked", Some(*v)),
                        State::Read(v) => ("read", Some(*v)),
                        State::Failed => ("failed", None),
                        State::NotAsked => ("not asked", None),
                    };
                    let mine: Vec<&(String, Emote, Image)> =
                        chosen.iter().filter(|(_, e, _)| e.provider == s.provider).collect();
                    json!({
                        "provider": provider_name(s.provider),
                        "set": state,
                        "via": via.map(|v| if v == Via::Direct { "direct" } else { "proxy" }),
                        "why": s.why,
                        "emotes": s.list.len(),
                        "animated": s.list.iter().filter(|(_, e, _)| e.animated).count(),
                        "chosen": mine.iter().map(|(w, ..)| w.as_str()).collect::<Vec<_>>(),
                        "chosen_animated": mine.iter().filter(|(_, e, _)| e.animated).count(),
                    })
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    fn answer(p: Provider) -> String {
        fixture(match p {
            Provider::Twitch => "twitch-global.json",
            Provider::SevenTv => "demo-7tv-global.json",
            Provider::Bttv => "demo-bttv-global.json",
            _ => "demo-ffz-global.json",
        })
    }

    /// Every set asked once, directly (Twitch's through our server
    /// alone), each read by its parser; an assortment of each, animated
    /// first, the same every run.
    #[test]
    fn the_four_global_sets_are_read_and_an_assortment_chosen() {
        let mut d = Demo::new(EmotesFrom::TwitchAndOthers, Some("https://api.test/x".into()), None);
        let reads = d.start();
        assert_eq!(
            reads.iter().map(|r| (r.provider, r.via, r.url.as_str())).collect::<Vec<_>>(),
            [
                (Provider::Twitch, Via::Proxy, "https://api.test/x/emotes/twitch-api/chat/emotes/global"),
                (Provider::SevenTv, Via::Direct, "https://7tv.io/v3/emote-sets/global"),
                (Provider::Bttv, Via::Direct, "https://api.betterttv.net/3/cached/emotes/global"),
                (Provider::Ffz, Via::Direct, "https://api.frankerfacez.com/v1/set/global"),
            ]
        );
        assert!(d.start().is_empty(), "each once");
        assert!(!d.done());
        for r in &reads {
            d.answered(r.provider, r.via, Ok(answer(r.provider)));
        }
        assert!(d.done());
        let chosen = d.chosen();
        for p in ORDER {
            let mine: Vec<_> = chosen.iter().filter(|(_, e, _)| e.provider == p).collect();
            let animated = mine.iter().filter(|(_, e, _)| e.animated).count();
            // Animated ones where the provider has them (as recorded on
            // 2026-11-20: 7TV many, BTTV one, FFZ none), still ones
            // filling the rest.
            let want = match p {
                // Twitch's fixture: two animated, three still with no
                // space in their names.
                Provider::Twitch => (5, 2),
                Provider::SevenTv => (ANIMATED + STILL, ANIMATED),
                Provider::Bttv => (ANIMATED + STILL, 1),
                _ => (ANIMATED + STILL, 0),
            };
            assert_eq!((mine.len(), animated), want, "{p:?}: {mine:?}");
            // Animated ones first.
            let first_still = mine.iter().position(|(_, e, _)| !e.animated).unwrap_or(mine.len());
            assert!(mine[first_still..].iter().all(|(_, e, _)| !e.animated));
        }
        assert!(chosen.iter().all(|(w, ..)| !w.contains(' ')));
        // The same choice from a set in another order.
        let mut again = Demo::new(EmotesFrom::TwitchAndOthers, Some("https://api.test/x".into()), None);
        for r in again.start() {
            let text = if r.provider == Provider::Bttv {
                let mut v: Vec<Value> = serde_json::from_str(&answer(r.provider)).unwrap();
                v.reverse();
                serde_json::to_string(&v).unwrap()
            } else {
                answer(r.provider)
            };
            again.answered(r.provider, r.via, Ok(text));
        }
        assert_eq!(again.chosen(), chosen);
        // The script chats with them, each image the provider's own.
        let script = d.script();
        assert!(script.online());
        assert_eq!(script.api().as_deref(), Some("https://api.test/x"));
        let sets = script.sets();
        let (word, emote, _) = chosen.iter().find(|(_, e, _)| e.provider == Provider::SevenTv).unwrap();
        assert_eq!(sets.word(word), Some(emote));
        assert!(sets.image(emote).unwrap().url.starts_with("https://cdn.7tv.app/emote/"));
        let report = d.report();
        assert_eq!(report[1]["set"], "read");
        assert_eq!(report[1]["via"], "direct");
        assert_eq!(report[1]["emotes"], 11);
        assert_eq!(report[1]["animated"], 7);
        assert_eq!(report[1]["chosen_animated"], 6);
    }

    /// A direct failure, or an answer no emote is read from, is asked
    /// again through the proxy; a second failure names both ways.
    #[test]
    fn a_failed_set_goes_through_the_proxy_and_the_report_says_why() {
        let mut d = Demo::new(EmotesFrom::TwitchAndOthers, Some("https://api.test".into()), None);
        for r in d.start() {
            match r.provider {
                Provider::Bttv => d.answered(r.provider, r.via, Err("TypeError: Failed to fetch".into())),
                Provider::Ffz => d.answered(r.provider, r.via, Ok("<html>nope</html>".into())),
                Provider::Twitch => d.answered(r.provider, r.via, Err("HTTP 503".into())),
                _ => d.answered(r.provider, r.via, Ok(answer(r.provider))),
            }
        }
        let again = d.start();
        assert_eq!(
            again.iter().map(|r| (r.provider, r.via, r.url.as_str())).collect::<Vec<_>>(),
            [
                (Provider::Bttv, Via::Proxy, "https://api.test/emotes/bttv-api/3/cached/emotes/global"),
                (Provider::Ffz, Via::Proxy, "https://api.test/emotes/ffz-api/v1/set/global"),
            ]
        );
        for r in again {
            if r.provider == Provider::Bttv {
                d.answered(r.provider, r.via, Ok(answer(r.provider)));
            } else {
                d.answered(r.provider, r.via, Err("HTTP 401".into()));
            }
        }
        assert!(d.done());
        let report = d.report();
        assert_eq!(report[0]["set"], "failed");
        assert_eq!(report[0]["why"][0], "through the proxy: HTTP 503");
        assert_eq!(report[2]["set"], "read");
        assert_eq!(report[2]["via"], "proxy");
        assert_eq!(report[3]["set"], "failed");
        assert_eq!(report[3]["why"][0], "directly: no emote read from 17 bytes");
        assert_eq!(report[3]["why"][1], "through the proxy: HTTP 401");
        // Without a proxy, a direct failure is the end.
        let mut d = Demo::new(EmotesFrom::TwitchAndOthers, None, None);
        let reads = d.start();
        assert_eq!(reads.len(), 3, "Twitch's needs our server");
        assert_eq!(d.report()[0]["why"][0], "no proxy to ask through");
    }

    /// "Emotes from: Twitch" asks no other provider.
    #[test]
    fn twitch_alone_asks_no_other_provider() {
        let mut d = Demo::new(EmotesFrom::TwitchOnly, Some("https://api.test".into()), None);
        let reads = d.start();
        assert_eq!(reads.len(), 1);
        assert_eq!(reads[0].provider, Provider::Twitch);
        d.answered(Provider::Twitch, Via::Proxy, Ok(answer(Provider::Twitch)));
        assert!(d.done());
        assert!(d.chosen().iter().all(|(_, e, _)| e.provider == Provider::Twitch));
        assert_eq!(d.report()[1]["set"], "not asked");
    }

    /// TESTING ONLY: every address served from a local base, the proxy's
    /// too.
    #[test]
    fn a_local_base_serves_every_address() {
        let mut d = Demo::new(
            EmotesFrom::TwitchAndOthers,
            Some("https://real.test/api".into()),
            Some("http://127.0.0.1:9/g/".into()),
        );
        let reads = d.start();
        assert_eq!(reads[0].url, "http://127.0.0.1:9/g/api/emotes/twitch-api/chat/emotes/global");
        assert_eq!(reads[1].url, "http://127.0.0.1:9/g/7tv.io/v3/emote-sets/global");
        for r in &reads {
            d.answered(r.provider, r.via, Ok(answer(r.provider)));
        }
        let chosen = d.chosen();
        let (_, _, image) = chosen.iter().find(|(_, e, _)| e.provider == Provider::Ffz).unwrap();
        assert!(image.url.starts_with("http://127.0.0.1:9/g/cdn.frankerfacez.com/emote/"), "{}", image.url);
        assert_eq!(
            image.proxied(d.api().unwrap()).unwrap(),
            format!("http://127.0.0.1:9/g/api/{}", image.proxy.as_deref().unwrap())
        );
    }

    /// The script made from nothing read still chats (its words alone);
    /// from the sets, its bursts and hypes name real emotes.
    #[test]
    fn the_demo_chats_with_what_it_read() {
        let d = Demo::new(EmotesFrom::TwitchAndOthers, None, None);
        let mut p = crate::script::Player::new(d.script());
        assert!(p.due(40.0).len() > 20);
        let mut d = Demo::new(EmotesFrom::TwitchAndOthers, Some("https://api.test".into()), None);
        for r in d.start() {
            d.answered(r.provider, r.via, Ok(answer(r.provider)));
        }
        let script = d.script();
        let words: Vec<String> = script.emotes().into_iter().map(|(w, ..)| w).collect();
        let mut p = crate::script::Player::new(script);
        let messages: Vec<String> = p
            .due(39.0)
            .iter()
            .filter_map(|m| match crate::eventsub::parse(m) {
                Ok(crate::eventsub::Message::Notification(crate::eventsub::Event::Message(c))) => Some(c.text),
                _ => None,
            })
            .collect();
        assert!(messages.len() > 100, "{}", messages.len());
        for w in &words {
            assert!(messages.iter().any(|m| m.split(' ').any(|x| x == w)), "{w} said");
        }
    }
}
