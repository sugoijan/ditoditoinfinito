//! The emotes a chat draws (C4, C5): Twitch's own, named by id in a
//! message's fragments, and the words of 7TV's, BTTV's and FFZ's sets for
//! the host's channel and their globals, read from the providers' JSON and
//! matched in a message's text. Each emote's still image has an address
//! on its provider's host, asked directly first, and a path under our
//! server's proxy route (`GET <api>/emotes/<provider>/<path>`), asked once
//! a direct request failed (the route is the server's, a closed list of
//! the four providers' hosts).
//!
//! An emote its provider animates is drawn from its animated image (round
//! TW4, C5): Twitch's `animated` GIF where a message's fragment offers
//! one, 7TV's `2x.webp` of an animated emote, BTTV's own image of one (a
//! GIF), FFZ's `animated` WebP; a still emote from its still image
//! (Twitch's `static`, 7TV's `_static`, FFZ's `urls`). The shells read
//! each image's container before decoding it ([`crate::anim`]).

use std::collections::HashMap;

use serde_json::Value;

/// Where an emote comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Provider {
    Twitch,
    SevenTv,
    Bttv,
    Ffz,
    /// The scripted chat's (TESTING ONLY): its image named by the script,
    /// never fetched from a provider.
    Test,
}

impl Provider {
    /// The proxy route's name for it (`<api>/emotes/<name>/…`); none for
    /// [`Provider::Test`].
    pub fn route(self) -> Option<&'static str> {
        match self {
            Provider::Twitch => Some("twitch"),
            Provider::SevenTv => Some("7tv"),
            Provider::Bttv => Some("bttv"),
            Provider::Ffz => Some("ffz"),
            Provider::Test => None,
        }
    }

    /// The host its images are on.
    pub fn image_host(self) -> &'static str {
        match self {
            Provider::Twitch => "static-cdn.jtvnw.net",
            Provider::SevenTv => "cdn.7tv.app",
            Provider::Bttv => "cdn.betterttv.net",
            Provider::Ffz => "cdn.frankerfacez.com",
            Provider::Test => "",
        }
    }
}

/// One emote: its provider, the provider's id for it, and whether its
/// provider animates it (its animated image is asked for).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Emote {
    pub provider: Provider,
    pub id: String,
    pub animated: bool,
}

impl Emote {
    pub fn new(provider: Provider, id: impl Into<String>) -> Emote {
        Emote {
            provider,
            id: id.into(),
            animated: false,
        }
    }

    /// Twitch's emote `id`.
    pub fn twitch(id: impl Into<String>) -> Emote {
        Emote::new(Provider::Twitch, id)
    }

    /// The same emote, animated by its provider.
    pub fn moving(mut self) -> Emote {
        self.animated = true;
        self
    }
}

/// An emote's still image: the address asked directly and the path under
/// the proxy route (none where there is no proxy for it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub url: String,
    pub proxy: Option<String>,
}

impl Image {
    /// An image on `provider`'s host at `path` (from its first `/`).
    fn on(provider: Provider, path: &str) -> Image {
        Image {
            url: format!("https://{}{path}", provider.image_host()),
            proxy: provider
                .route()
                .map(|r| format!("emotes/{r}{path}")),
        }
    }

    /// The host of [`Image::url`] (what C4's "a host that needed the
    /// proxy" is counted by).
    pub fn host(&self) -> &str {
        let rest = self.url.split_once("://").map_or(&*self.url, |(_, r)| r);
        rest.split(['/', '?']).next().unwrap_or_default()
    }

    /// The proxy route's address under `api` (the API's base, no trailing
    /// slash needed).
    pub fn proxied(&self, api: &str) -> Option<String> {
        let path = self.proxy.as_ref()?;
        Some(format!("{}/{path}", api.trim_end_matches('/')))
    }
}

/// The size an image is asked for: the 2× files, about 56 pixels tall,
/// drawn into the atlas's 64-pixel cells ([`crate::cache::CELL`]).
const TWITCH_SCALE: &str = "2.0";

/// The emotes known by word (third-party, matched in text), and the
/// scripted chat's images in place of any provider's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EmoteSets {
    /// Word to emote and its image.
    words: HashMap<String, (Emote, Image)>,
    /// The rank of the source each word's emote came from.
    ranks: HashMap<String, (u8, u8)>,
    /// Images that stand for an emote's own (the scripted chat's): what
    /// is asked instead of the provider's.
    overrides: HashMap<Emote, Image>,
    /// Only overridden images are asked for (the scripted chat: nothing
    /// is ever asked of a provider).
    offline: bool,
}

/// Which of a channel's or the global sets a list came from, and whose:
/// a channel's word wins over a global one, and between providers 7TV's
/// over BTTV's over FFZ's (as chat clients that show all three do).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    Global(Provider),
    Channel(Provider),
}

fn rank(source: Source) -> (u8, u8) {
    let p = |p: Provider| match p {
        Provider::SevenTv => 3,
        Provider::Bttv => 2,
        Provider::Ffz => 1,
        _ => 0,
    };
    match source {
        Source::Channel(pr) => (1, p(pr)),
        Source::Global(pr) => (0, p(pr)),
    }
}

impl EmoteSets {
    /// Sets for the scripted chat: nothing asked of a provider.
    pub fn offline() -> EmoteSets {
        EmoteSets {
            offline: true,
            ..EmoteSets::default()
        }
    }

    /// Adds `list` (word, emote, image) from `source`. Lists are added in
    /// any order: a word keeps the emote of the highest-ranked source
    /// ([`Source`]).
    pub fn add(&mut self, source: Source, list: Vec<(String, Emote, Image)>) {
        for (word, emote, image) in list {
            let r = rank(source);
            if self.ranks.get(&word).is_some_and(|had| *had >= r) {
                continue;
            }
            self.ranks.insert(word.clone(), r);
            self.words.insert(word, (emote, image));
        }
    }

    /// The emote `word` names, if any.
    pub fn word(&self, word: &str) -> Option<&Emote> {
        self.words.get(word).map(|(e, _)| e)
    }

    /// Words known.
    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// `emote`'s image is `image` (the scripted chat's own pictures).
    pub fn set_image(&mut self, emote: Emote, image: Image) {
        self.overrides.insert(emote, image);
    }

    /// The image asked for `emote`: an override, else its provider's
    /// (none offline, none for a test emote without one).
    pub fn image(&self, emote: &Emote) -> Option<Image> {
        if let Some(i) = self.overrides.get(emote) {
            return Some(i.clone());
        }
        if self.offline {
            return None;
        }
        if let Some((_, i)) = self.words.values().find(|(e, _)| e == emote) {
            return Some(i.clone());
        }
        match emote.provider {
            Provider::Twitch if emote.animated => Some(twitch_animated_image(&emote.id)),
            Provider::Twitch => Some(twitch_image(&emote.id)),
            _ => None,
        }
    }
}

/// Twitch's still image of emote `id`, dark theme, 2×.
pub fn twitch_image(id: &str) -> Image {
    Image::on(
        Provider::Twitch,
        &format!("/emoticons/v2/{id}/static/dark/{TWITCH_SCALE}"),
    )
}

/// Twitch's animated image of emote `id` (a GIF), dark theme, 2×.
pub fn twitch_animated_image(id: &str) -> Image {
    Image::on(
        Provider::Twitch,
        &format!("/emoticons/v2/{id}/animated/dark/{TWITCH_SCALE}"),
    )
}

/// A path on `provider`'s host from an address (`//host/path`,
/// `https://host/path`); `None` for another host.
fn path_on(provider: Provider, url: &str) -> Option<String> {
    let rest = url.trim_start_matches("https:").trim_start_matches("http:");
    let rest = rest.strip_prefix("//")?;
    let (host, path) = rest.split_at(rest.find('/')?);
    (host == provider.image_host()).then(|| path.to_string())
}

/// Where each provider's sets are read, directly and through the proxy,
/// for the channel of Twitch user `channel` (C4); Twitch's own need none.
/// The proxy's routes for the providers' APIs are `<provider>-api` (the
/// server's `upstream`), apart from their images'.
pub fn set_addresses(provider: Provider, channel: &str) -> Vec<Image> {
    let api = |host: &str, route: &str, path: String| Image {
        url: format!("https://{host}{path}"),
        proxy: Some(format!("emotes/{route}{path}")),
    };
    match provider {
        Provider::SevenTv => vec![
            api("7tv.io", "7tv-api", format!("/v3/users/twitch/{channel}")),
            api("7tv.io", "7tv-api", "/v3/emote-sets/global".into()),
        ],
        Provider::Bttv => vec![
            api(
                "api.betterttv.net",
                "bttv-api",
                format!("/3/cached/users/twitch/{channel}"),
            ),
            api("api.betterttv.net", "bttv-api", "/3/cached/emotes/global".into()),
        ],
        Provider::Ffz => vec![
            api("api.frankerfacez.com", "ffz-api", format!("/v1/room/id/{channel}")),
            api("api.frankerfacez.com", "ffz-api", "/v1/set/global".into()),
        ],
        Provider::Twitch | Provider::Test => Vec::new(),
    }
}

type List = Vec<(String, Emote, Image)>;

fn json(text: &str) -> Option<Value> {
    serde_json::from_str(text).ok()
}

/// 7TV's emotes of a set (`{"emotes": [...]}`): each one's name in the set
/// (an alias where it has one) and its 2× file: the animated WebP of an
/// animated emote, else the still one.
fn seven_tv_emotes(set: &Value) -> List {
    let Some(list) = set.get("emotes").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|e| {
            let name = e.get("name")?.as_str()?;
            let id = e.get("id")?.as_str()?;
            let data = e.get("data");
            let host = data.and_then(|d| d.get("host"));
            let animated = data
                .and_then(|d| d.get("animated"))
                .and_then(Value::as_bool)
                == Some(true);
            let base = host
                .and_then(|h| h.get("url"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("//cdn.7tv.app/emote/{id}"));
            let file = host
                .and_then(|h| h.get("files"))
                .and_then(Value::as_array)
                .and_then(|files| {
                    files.iter().find(|f| {
                        f.get("name")
                            .and_then(Value::as_str)
                            .is_some_and(|n| n.starts_with("2x"))
                    })
                })
                .and_then(|f| {
                    if animated {
                        f.get("name")
                    } else {
                        f.get("static_name").or_else(|| f.get("name"))
                    }
                    .and_then(Value::as_str)
                })
                .unwrap_or(if animated { "2x.webp" } else { "2x_static.webp" });
            let path = path_on(Provider::SevenTv, &format!("{base}/{file}"))?;
            let emote = Emote::new(Provider::SevenTv, id);
            Some((
                name.to_string(),
                if animated { emote.moving() } else { emote },
                Image::on(Provider::SevenTv, &path),
            ))
        })
        .collect()
}

/// 7TV's channel set from `/v3/users/twitch/<id>`.
pub fn seven_tv_user(text: &str) -> List {
    json(text)
        .and_then(|v| v.get("emote_set").map(seven_tv_emotes))
        .unwrap_or_default()
}

/// 7TV's global set from `/v3/emote-sets/global`.
pub fn seven_tv_global(text: &str) -> List {
    json(text).map(|v| seven_tv_emotes(&v)).unwrap_or_default()
}

fn bttv_emotes(list: Option<&Value>) -> List {
    let Some(list) = list.and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|e| {
            let code = e.get("code")?.as_str()?;
            let id = e.get("id")?.as_str()?;
            // An animated one's image is its animation (a GIF) at the
            // same address.
            let emote = Emote::new(Provider::Bttv, id);
            let animated = e.get("animated").and_then(Value::as_bool) == Some(true);
            Some((
                code.to_string(),
                if animated { emote.moving() } else { emote },
                Image::on(Provider::Bttv, &format!("/emote/{id}/2x")),
            ))
        })
        .collect()
}

/// BTTV's channel and shared emotes from `/3/cached/users/twitch/<id>`.
pub fn bttv_user(text: &str) -> List {
    let Some(v) = json(text) else {
        return Vec::new();
    };
    let mut out = bttv_emotes(v.get("channelEmotes"));
    out.extend(bttv_emotes(v.get("sharedEmotes")));
    out
}

/// BTTV's globals from `/3/cached/emotes/global`.
pub fn bttv_global(text: &str) -> List {
    json(text).map(|v| bttv_emotes(Some(&v))).unwrap_or_default()
}

fn ffz_set(set: &Value) -> List {
    let Some(list) = set.get("emoticons").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|e| {
            let name = e.get("name")?.as_str()?;
            let id = e.get("id").and_then(|i| {
                i.as_u64()
                    .map(|n| n.to_string())
                    .or_else(|| i.as_str().map(str::to_string))
            })?;
            // Its animated WebP where it has one (`animated`, as `urls`).
            let pick = |urls: &Value| {
                ["2", "1", "4"]
                    .iter()
                    .find_map(|k| urls.get(*k).and_then(Value::as_str))
                    .and_then(|u| path_on(Provider::Ffz, u))
            };
            let emote = Emote::new(Provider::Ffz, id);
            let (emote, path) = match e.get("animated").and_then(pick) {
                Some(path) => (emote.moving(), path),
                None => (emote, pick(e.get("urls")?)?),
            };
            Some((name.to_string(), emote, Image::on(Provider::Ffz, &path)))
        })
        .collect()
}

/// FFZ's channel set from `/v1/room/id/<id>` (the room's `set`).
pub fn ffz_room(text: &str) -> List {
    let Some(v) = json(text) else {
        return Vec::new();
    };
    let Some(set) = v.get("room").and_then(|r| r.get("set")) else {
        return Vec::new();
    };
    let key = set
        .as_u64()
        .map(|n| n.to_string())
        .or_else(|| set.as_str().map(str::to_string))
        .unwrap_or_default();
    v.get("sets")
        .and_then(|s| s.get(&key))
        .map(ffz_set)
        .unwrap_or_default()
}

/// FFZ's globals from `/v1/set/global`: its default sets alone.
pub fn ffz_global(text: &str) -> List {
    let Some(v) = json(text) else {
        return Vec::new();
    };
    let defaults: Vec<String> = v
        .get("default_sets")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|n| n.as_u64().map(|n| n.to_string()))
                .collect()
        })
        .unwrap_or_default();
    defaults
        .iter()
        .filter_map(|k| v.get("sets").and_then(|s| s.get(k)))
        .flat_map(ffz_set)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    fn words(list: &List) -> Vec<&str> {
        list.iter().map(|(w, ..)| &**w).collect()
    }

    #[test]
    fn twitch_animated_images_are_gifs_on_the_same_host() {
        let i = twitch_animated_image("emotesv2_1");
        assert_eq!(
            i.url,
            "https://static-cdn.jtvnw.net/emoticons/v2/emotesv2_1/animated/dark/2.0"
        );
        assert_eq!(
            i.proxy.as_deref(),
            Some("emotes/twitch/emoticons/v2/emotesv2_1/animated/dark/2.0")
        );
        let sets = EmoteSets::default();
        assert!(sets.image(&Emote::twitch("1").moving()).unwrap().url.contains("/animated/"));
        assert!(sets.image(&Emote::twitch("1")).unwrap().url.contains("/static/"));
    }

    #[test]
    fn twitch_images_are_still_and_have_a_proxy_path() {
        let i = twitch_image("25");
        assert_eq!(
            i.url,
            "https://static-cdn.jtvnw.net/emoticons/v2/25/static/dark/2.0"
        );
        assert_eq!(i.host(), "static-cdn.jtvnw.net");
        assert_eq!(
            i.proxied("https://example.test/api/").as_deref(),
            Some("https://example.test/api/emotes/twitch/emoticons/v2/25/static/dark/2.0")
        );
    }

    #[test]
    fn seven_tv_sets_are_read_with_their_animated_and_still_files() {
        let user = seven_tv_user(&fixture("7tv-user.json"));
        assert_eq!(words(&user), ["catJAM", "wideHappy"]);
        // catJAM is animated: its animated WebP; wideHappy still.
        assert_eq!(
            user[0].2.url,
            "https://cdn.7tv.app/emote/60ae7316f7c927fad14e6ca2/2x.webp"
        );
        assert_eq!(
            user[0].2.proxy.as_deref(),
            Some("emotes/7tv/emote/60ae7316f7c927fad14e6ca2/2x.webp")
        );
        assert!(user[0].1.animated);
        assert!(!user[1].1.animated);
        assert!(user[1].2.url.ends_with("/2x_static.webp"));
        let global = seven_tv_global(&fixture("7tv-global.json"));
        assert_eq!(words(&global), ["Clap", "catJAM"]);
    }

    #[test]
    fn bttv_reads_animated_emotes_too() {
        let user = bttv_user(&fixture("bttv-user.json"));
        assert_eq!(words(&user), ["PogBTTV", "catJAM", "OhMyGoodness"]);
        assert!(!user[0].1.animated);
        assert!(user[1].1.animated);
        assert_eq!(
            user[1].2.url,
            "https://cdn.betterttv.net/emote/5f1b0186cf6d2144653d2971/2x"
        );
        assert_eq!(
            user[0].2.url,
            "https://cdn.betterttv.net/emote/5f1b0186cf6d2144653d2970/2x"
        );
        let global = bttv_global(&fixture("bttv-global.json"));
        assert_eq!(words(&global), [":tf:", "DatSauce"]);
    }

    #[test]
    fn ffz_reads_the_rooms_set_and_the_default_globals() {
        let room = ffz_room(&fixture("ffz-room.json"));
        assert_eq!(words(&room), ["LilZ", "LilDance"]);
        // An animated one: its animated WebP.
        assert!(room[1].1.animated);
        assert_eq!(
            room[1].2.proxy.as_deref(),
            Some("emotes/ffz/emote/720507/animated/2")
        );
        assert_eq!(room[0].2.url, "https://cdn.frankerfacez.com/emote/28136/2");
        assert_eq!(room[0].2.proxy.as_deref(), Some("emotes/ffz/emote/28136/2"));
        let global = ffz_global(&fixture("ffz-global.json"));
        assert_eq!(words(&global), ["ZreknarF"]);
    }

    #[test]
    fn a_channels_word_wins_and_7tv_over_bttv() {
        let mut sets = EmoteSets::default();
        sets.add(
            Source::Global(Provider::SevenTv),
            seven_tv_global(&fixture("7tv-global.json")),
        );
        sets.add(
            Source::Channel(Provider::Bttv),
            bttv_user(&fixture("bttv-user.json")),
        );
        sets.add(
            Source::Channel(Provider::SevenTv),
            seven_tv_user(&fixture("7tv-user.json")),
        );
        // The channel's 7TV catJAM over the global one.
        assert_eq!(
            sets.word("catJAM"),
            Some(&Emote::new(Provider::SevenTv, "60ae7316f7c927fad14e6ca2").moving())
        );
        assert_eq!(sets.word("Clap").map(|e| e.provider), Some(Provider::SevenTv));
        assert_eq!(sets.word("PogBTTV").map(|e| e.provider), Some(Provider::Bttv));
        assert_eq!(sets.word("Kappa"), None);
        // A Twitch emote's image without a set; none offline.
        assert!(sets.image(&Emote::twitch("25")).is_some());
        assert_eq!(EmoteSets::offline().image(&Emote::twitch("25")), None);
    }

    #[test]
    fn set_addresses_name_each_provider() {
        let a = set_addresses(Provider::Bttv, "1971641");
        assert_eq!(
            a[0].url,
            "https://api.betterttv.net/3/cached/users/twitch/1971641"
        );
        assert_eq!(
            a[0].proxy.as_deref(),
            Some("emotes/bttv-api/3/cached/users/twitch/1971641")
        );
        assert_eq!(
            set_addresses(Provider::SevenTv, "1")[1].proxy.as_deref(),
            Some("emotes/7tv-api/v3/emote-sets/global")
        );
        assert_eq!(
            set_addresses(Provider::Ffz, "9")[0].proxy.as_deref(),
            Some("emotes/ffz-api/v1/room/id/9")
        );
        assert!(set_addresses(Provider::Twitch, "1").is_empty());
    }
}
