//! The emotes a chat draws (C4, C5): Twitch's own, named by id in a
//! message's fragments, and the words of 7TV's, BTTV's and FFZ's sets for
//! the host's channel and their globals, read from the providers' JSON and
//! matched in a message's text. Each emote's image has an address on its
//! provider's host, asked directly first, and a path under our server's
//! proxy route (`GET <api>/emotes/<provider>/<path>`), asked once a direct
//! request failed (the route is the server's, a closed list of the four
//! providers' hosts).
//!
//! The providers' answers are read, the images' addresses built and their
//! routes through the proxy named by `barbed-emotes` (round BE, the
//! maintainer's own library: its parsers, its pick of an emote's image,
//! its `proxy::route`), its model made ours at one seam (`from_barbed`).
//!
//! An emote its provider animates is drawn from its animated image (round
//! TW4, C5): Twitch's `animated` GIF where a message's fragment offers
//! one, 7TV's `2x.webp` of an animated emote, BTTV's own image of one (a
//! GIF), FFZ's `animated` WebP; a still emote from its still image
//! (Twitch's `static`, 7TV's own `2x.webp`, BTTV's PNG, FFZ's `urls`).
//! The shells read each image's container before decoding it
//! ([`crate::anim`]).

use std::collections::HashMap;

use barbed_emotes as barbed;

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

    /// `barbed-emotes`' name for it; none for [`Provider::Test`].
    fn barbed(self) -> Option<barbed::EmoteProvider> {
        match self {
            Provider::Twitch => Some(barbed::EmoteProvider::Twitch),
            Provider::SevenTv => Some(barbed::EmoteProvider::SevenTv),
            Provider::Bttv => Some(barbed::EmoteProvider::BetterTtv),
            Provider::Ffz => Some(barbed::EmoteProvider::FrankerFaceZ),
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
    /// An image of `provider`'s at `url`, an address `barbed-emotes` built
    /// (its parsers', [`twitch_image`]'s), with its path under our proxy:
    /// `emotes/` and barbed-emotes' route for it as one of `provider`'s
    /// images ([`barbed::proxy::image_route`]: the closed list of shapes
    /// the parsers build and the proxy carries, narrowed to the provider's
    /// image route, every id plain, no path that leaves the provider's
    /// image files); none where it is not one of `provider`'s images (a
    /// set's route, another provider's). So an id or a path from a
    /// provider's answer (or a message's fragment) never becomes an
    /// address on another host or another route of our API, asked with
    /// the person's cookie (review 2026-11-22, chat-touch 5).
    fn carried(provider: Provider, url: &str) -> Option<Image> {
        let route = barbed::proxy::image_route(&provider.barbed()?, url)?;
        Some(Image {
            url: url.to_string(),
            proxy: Some(format!("emotes/{route}")),
        })
    }

    /// A set's address: `url` directly, and its route through our proxy
    /// ([`barbed::proxy::route`]).
    fn set_at(url: &str) -> Image {
        Image {
            url: url.to_string(),
            proxy: barbed::proxy::route(url).map(|r| format!("emotes/{r}")),
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

/// The image asked of each emote (`barbed-emotes`' pick, [`barbed::pick_image`]):
/// the 2× file, about 56 pixels tall, drawn into the atlas's 64-pixel
/// cells ([`crate::cache::CELL`]), else the nearest larger, else the
/// largest there is; an animated emote's animation; Twitch's dark theme;
/// any format the provider lists, the shells reading each image's
/// container before decoding it ([`crate::anim`]). Of equal candidates
/// the first listed is taken: BetterTTV's file as uploaded (`<n>x`, a PNG
/// or a GIF) before its WebP (`<n>x.webp`, which our proxy does not
/// carry); FrankerFaceZ's animated WebP before its still PNG.
const IMAGES: barbed::ImageRequest<'static> = barbed::ImageRequest {
    scale: 2,
    animated: true,
    theme: barbed::Theme::Dark,
    formats: &[],
};

/// 7TV's: as [`IMAGES`], its WebP files alone (it lists AVIF and GIF
/// files beside them, and our proxy carries its WebP ones).
const SEVEN_TV_IMAGES: barbed::ImageRequest<'static> = barbed::ImageRequest {
    formats: &[barbed::ImageFormat::WebP],
    ..IMAGES
};

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
    /// TESTING ONLY: Twitch's images (a message's fragments') served from
    /// here instead (`<at><host>/<path>`, as the live chat's sets' with
    /// `mpfakechatimg` and `--mp-dev-chat-at`).
    served_from: Option<String>,
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

    /// TESTING ONLY: Twitch's own images asked of `at` instead of
    /// Twitch's host ([`EmoteSets::served_from`]).
    pub fn serve_from(&mut self, at: Option<String>) {
        self.served_from = at;
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
        let image = match emote.provider {
            Provider::Twitch if emote.animated => twitch_animated_image(&emote.id)?,
            Provider::Twitch => twitch_image(&emote.id)?,
            _ => return None,
        };
        Some(match &self.served_from {
            Some(at) => Image {
                url: crate::global::local(&image.url, Some(at)),
                ..image
            },
            None => image,
        })
    }
}

/// Twitch's still image of emote `id`, dark theme, 2× (none for an id
/// of another shape: `barbed-emotes`' [`barbed::twitch::image_url`]).
pub fn twitch_image(id: &str) -> Option<Image> {
    twitch_template(id, false)
}

/// Twitch's animated image of emote `id` (a GIF), dark theme, 2×.
pub fn twitch_animated_image(id: &str) -> Option<Image> {
    twitch_template(id, true)
}

fn twitch_template(id: &str, animated: bool) -> Option<Image> {
    let url = barbed::twitch::image_url(id, animated, barbed::Theme::Dark, IMAGES.scale)?;
    Image::carried(Provider::Twitch, &url)
}

/// Where each provider's sets are read, directly and through the proxy,
/// for the channel of Twitch user `channel` (C4): the channel's, then the
/// global one, as `barbed-emotes` names them (none for a channel id of
/// another shape); Twitch's own need none. The proxy's routes for the
/// providers' APIs are `<provider>-api` (the server's `upstream`), apart
/// from their images'.
pub fn set_addresses(provider: Provider, channel: &str) -> Vec<Image> {
    use barbed::{bttv, ffz, seventv};
    let (own, global) = match provider {
        Provider::SevenTv => (seventv::user_url(channel), seventv::GLOBAL_SET_URL),
        Provider::Bttv => (bttv::channel_url(channel), bttv::GLOBAL_URL),
        Provider::Ffz => (ffz::room_url(channel), ffz::GLOBAL_URL),
        Provider::Twitch | Provider::Test => return Vec::new(),
    };
    own.map_or_else(Vec::new, |own| vec![Image::set_at(&own), Image::set_at(global)])
}

/// Where each provider's global set is read (C4's rule: directly first,
/// through the proxy after a failure; round TW5's global-emotes demo).
/// Twitch's own has no address a device may ask without a token: our
/// server asks Helix for it with the app's token, through the proxy alone
/// (`emotes/twitch-api/chat/emotes/global`, an empty direct address).
pub fn global_address(provider: Provider) -> Option<Image> {
    use barbed::{bttv, ffz, seventv, twitch};
    Some(match provider {
        Provider::Twitch => Image {
            url: String::new(),
            ..Image::set_at(twitch::GLOBAL_EMOTES_URL)
        },
        Provider::SevenTv => Image::set_at(seventv::GLOBAL_SET_URL),
        Provider::Bttv => Image::set_at(bttv::GLOBAL_URL),
        Provider::Ffz => Image::set_at(ffz::GLOBAL_URL),
        Provider::Test => return None,
    })
}

/// A list of emotes read from a set: each one's word, the emote and its
/// image.
pub type List = Vec<(String, Emote, Image)>;

/// The one seam between `barbed-emotes`' model and ours: the emotes a
/// provider's set gave, as the layer draws them. Each emote's word (in a
/// 7TV set the set's name for it, an alias where it has one), the image
/// `request` picks ([`barbed::Emote::image`]) with its proxy path
/// ([`Image::carried`]), animated where that image is its animation. Left
/// out: a modifier (BetterTTV's and FrankerFaceZ's effect words, 7TV's
/// zero-width overlays: drawn alone each would be a picture out of place,
/// round TW5), an emote FrankerFaceZ hides, and one with no image of the
/// shape our proxy carries. What barbed-emotes itself left out of a set
/// (an entry of another shape, an id that is not plain, an image off its
/// provider's host) never reaches here.
fn from_barbed(
    provider: Provider,
    emotes: Vec<barbed::Emote>,
    request: &barbed::ImageRequest<'_>,
) -> List {
    emotes
        .into_iter()
        .filter(|e| e.is_standalone() && !e.hidden)
        .filter_map(|e| {
            let picked = e.image(request)?;
            let (image, animated) = (Image::carried(provider, &picked.url)?, picked.animated);
            let emote = Emote {
                provider,
                id: e.id.value,
                animated,
            };
            Some((e.code, emote, image))
        })
        .collect()
}

/// 7TV's channel set from `/v3/users/twitch/<id>` (its active set, whole).
pub fn seven_tv_user(text: &str) -> List {
    let emotes = barbed::seventv::parse_user(text).map(|u| u.into_emotes());
    from_barbed(Provider::SevenTv, emotes.unwrap_or_default(), &SEVEN_TV_IMAGES)
}

/// 7TV's global set from `/v3/emote-sets/global`.
pub fn seven_tv_global(text: &str) -> List {
    let emotes = barbed::seventv::parse_emote_set(text).map(|s| s.emotes);
    from_barbed(Provider::SevenTv, emotes.unwrap_or_default(), &SEVEN_TV_IMAGES)
}

/// BTTV's channel and shared emotes from `/3/cached/users/twitch/<id>`.
pub fn bttv_user(text: &str) -> List {
    let emotes = barbed::bttv::parse_channel(text).map(|c| c.into_emotes());
    from_barbed(Provider::Bttv, emotes.unwrap_or_default(), &IMAGES)
}

/// BTTV's globals from `/3/cached/emotes/global`.
pub fn bttv_global(text: &str) -> List {
    let emotes = barbed::bttv::parse_global(text).map(|s| s.emotes);
    from_barbed(Provider::Bttv, emotes.unwrap_or_default(), &IMAGES)
}

/// FFZ's channel set from `/v1/room/id/<id>` (the room's `set`).
pub fn ffz_room(text: &str) -> List {
    let emotes = barbed::ffz::parse_room(text).map(|r| r.into_emotes());
    from_barbed(Provider::Ffz, emotes.unwrap_or_default(), &IMAGES)
}

/// FFZ's globals from `/v1/set/global`: its default sets alone (the
/// others in its answer are not global for everyone).
pub fn ffz_global(text: &str) -> List {
    let emotes = barbed::ffz::parse_global(text).map(|g| g.default_emotes());
    from_barbed(Provider::Ffz, emotes.unwrap_or_default(), &IMAGES)
}

/// Twitch's global emotes from Helix's Get Global Emotes (`{"data":
/// [{"id", "name", "format": ["static", "animated"], "scale",
/// "theme_mode", ...}], "template"}`, round TW5, as our server answers
/// it): each one's name, the emote (animated where its `format` lists
/// `animated`) and its image from the template by its id (dark theme, 2×,
/// among those its lists offer), as a message's fragment would give it.
pub fn twitch_global(text: &str) -> List {
    let emotes = barbed::twitch::parse_emotes(text).map(|s| s.emotes);
    from_barbed(Provider::Twitch, emotes.unwrap_or_default(), &IMAGES)
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

    /// Review 2026-11-22, chat-touch 5: an id or a path a provider's
    /// answer (or a message's fragment) gives that would step out of the
    /// provider's images (`..`, escapes, a query, backslashes) makes no
    /// image at all: its proxy address could otherwise reach another route
    /// of our API with the person's cookie. A channel id of another shape
    /// reads no set.
    #[test]
    fn ids_and_paths_that_leave_the_images_make_none() {
        let bttv = r#"{"channelEmotes":[{"id":"../../../me","code":"Bad"},{"id":"5f1b0186cf6d2144653d2970","code":"Good"}],"sharedEmotes":[]}"#;
        assert_eq!(words(&bttv_user(bttv)), ["Good"]);
        let seven = r#"{"emote_set":{"emotes":[
            {"id":"a1","name":"Up","data":{"host":{"url":"//cdn.7tv.app/../../auth/attempts/X/open","files":[{"name":"2x.webp"}]}}},
            {"id":"a2","name":"Esc","data":{"host":{"url":"//cdn.7tv.app/emote/%2e%2e","files":[{"name":"2x.webp"}]}}},
            {"id":"a3","name":"Fine","data":{"host":{"url":"//cdn.7tv.app/emote/a3","files":[{"name":"2x.webp"}]}}}
        ]}}"#;
        assert_eq!(words(&seven_tv_user(seven)), ["Fine"]);
        let ffz = r#"{"room":{"set":1},"sets":{"1":{"emoticons":[
            {"id":1,"name":"Up","urls":{"2":"https://cdn.frankerfacez.com/emote/1/../../me"}},
            {"id":2,"name":"Query","urls":{"2":"https://cdn.frankerfacez.com/emote/2/2?x=/../"}},
            {"id":3,"name":"Fine","urls":{"2":"https://cdn.frankerfacez.com/emote/3/2"}}
        ]}}}"#;
        assert_eq!(words(&ffz_room(ffz)), ["Fine"]);
        let twitch = r#"{"data":[{"id":"../x","name":"Bad","format":["static"]},{"id":"25","name":"Kappa","format":["static"]}]}"#;
        assert_eq!(words(&twitch_global(twitch)), ["Kappa"]);
        let sets = EmoteSets::default();
        assert!(sets.image(&Emote::twitch("../../me")).is_none(), "a fragment's id");
        assert!(sets.image(&Emote::twitch("emotesv2_1a2b")).is_some());
        assert!(set_addresses(Provider::Bttv, "../../me").is_empty());
        assert_eq!(set_addresses(Provider::Bttv, "12345").len(), 2);
    }

    /// Round BE: unusual or malformed answers read as `barbed-emotes`
    /// reads them, where its reading is the better one or as good (each
    /// differed from the game's own parsers before; none on the providers'
    /// real global sets, `tests/emote_lists.rs`).
    #[test]
    fn unusual_answers_are_read_as_barbed_emotes_reads_them() {
        let seven = |emotes: &str| seven_tv_global(&format!(r#"{{"emotes":[{emotes}]}}"#));
        let file = |l: &List| l.iter().map(|(_, _, i)| i.url.rsplit('/').next().unwrap().to_string()).collect::<Vec<_>>();
        // 7TV without a `2x.webp`: the nearest listed file (larger first),
        // never a file nobody listed (the game made up `2x.webp` or
        // `2x_static.webp`).
        let l = seven(r#"{"id":"a1","name":"Big","data":{"animated":false,"host":{"url":"//cdn.7tv.app/emote/a1","files":[{"name":"1x.webp"},{"name":"4x.webp"}]}}}"#);
        assert_eq!(file(&l), ["4x.webp"]);
        // 7TV with AVIF files alone: no image we can be sure to decode, so
        // no emote (the game made up `2x_static.webp`).
        let l = seven(r#"{"id":"a2","name":"Avif","data":{"host":{"url":"//cdn.7tv.app/emote/a2","files":[{"name":"2x.avif"}]}}}"#);
        assert!(l.is_empty());
        // 7TV without a `host`: the files every emote has, its `2x.webp`
        // (a still emote's own file; the game asked `2x_static.webp`).
        let l = seven(r#"{"id":"a3","name":"NoHost","data":{"animated":false}}"#);
        assert_eq!(file(&l), ["2x.webp"]);
        // 7TV, an emote zero-width by its own flag (`data.flags` 256) as
        // well as by the set's: left out as an overlay (the game read the
        // set's flag alone; 7TV's real zero-width globals have both).
        let l = seven(r#"{"id":"a4","name":"Cover","flags":0,"data":{"flags":256,"host":{"url":"//cdn.7tv.app/emote/a4","files":[{"name":"2x.webp"}]}}}"#);
        assert!(l.is_empty());
        // 7TV, `flags` not a number: the entry is of another shape, left
        // out (the game kept it as not zero-width).
        let l = seven(r#"{"id":"a5","name":"Odd","flags":"1","data":{"host":{"url":"//cdn.7tv.app/emote/a5","files":[{"name":"2x.webp"}]}}}"#);
        assert!(l.is_empty());
        // BetterTTV's file as uploaded whatever its `imageType` says, or
        // without one (the only one of its files our proxy carries).
        for kind in [r#","imageType":"webp""#, ""] {
            let l = bttv_global(&format!(r#"[{{"id":"5f1b0186cf6d2144653d2970","code":"Up"{kind}}}]"#));
            assert_eq!(l[0].2.proxy.as_deref(), Some("emotes/bttv/emote/5f1b0186cf6d2144653d2970/2x"));
        }
        // An empty word is no emote (it could never match a word).
        assert!(bttv_global(r#"[{"id":"5f1b0186cf6d2144653d2970","code":"","imageType":"png"}]"#).is_empty());
        // FrankerFaceZ without a 2× file: the nearest larger (4×, drawn
        // smaller into the 64-pixel cell) rather than the 1× (drawn larger
        // and blurred; the game took 1×).
        let room = |emotes: &str| ffz_room(&format!(r#"{{"room":{{"set":1}},"sets":{{"1":{{"id":1,"emoticons":[{emotes}]}}}}}}"#));
        let l = room(r#"{"id":5,"name":"NoTwo","urls":{"1":"https://cdn.frankerfacez.com/emote/5/1","4":"https://cdn.frankerfacez.com/emote/5/4"}}"#);
        assert_eq!(l[0].2.proxy.as_deref(), Some("emotes/ffz/emote/5/4"));
        // FrankerFaceZ, an address of another shape than its emotes'
        // files: no image (our proxy's route would refuse it anyway; the
        // game kept it).
        assert!(room(r#"{"id":6,"name":"Other","urls":{"2":"https://cdn.frankerfacez.com/emote/2/2.png"}}"#).is_empty());
        // FrankerFaceZ, `hidden` not a bool: the entry left out (the game
        // kept it).
        assert!(room(r#"{"id":7,"name":"Odd","hidden":"no","urls":{"2":"https://cdn.frankerfacez.com/emote/7/2"}}"#).is_empty());
        // FrankerFaceZ's default sets written as strings are read too (the
        // game read numbers alone, and so no global emote).
        let g = ffz_global(r#"{"default_sets":["3"],"sets":{"3":{"id":3,"emoticons":[{"id":9,"name":"Z","urls":{"2":"https://cdn.frankerfacez.com/emote/9/2"}}]}}}"#);
        assert_eq!(words(&g), ["Z"]);
        // Twitch, an entry whose `format` holds a non-string: of another
        // shape, left out (the game kept it).
        let t = twitch_global(r#"{"data":[{"id":"25","name":"Kappa","format":["static",1]},{"id":"26","name":"Fine","format":["static"]}]}"#);
        assert_eq!(words(&t), ["Fine"]);
        // Twitch, an emote whose lists offer 1× alone: its 1× image (the
        // game asked 2×, which its list did not offer).
        let t = twitch_global(r#"{"data":[{"id":"88","name":"Small","format":["static"],"scale":["1.0"],"theme_mode":["dark"]}]}"#);
        assert!(t[0].2.url.ends_with("/static/dark/1.0"), "{}", t[0].2.url);

        // Round B8, barbed-emotes 0.0.8 (asked for in round BE; 0.0.7
        // differed in both, the game's own parsers agreed): BetterTTV's
        // own `animated: false` on a GIF is believed (a GIF of one frame:
        // still, its file as uploaded; 0.0.7 read it as animated).
        let l = bttv_global(r#"[{"id":"5f1b0186cf6d2144653d2972","code":"StillGif","imageType":"gif","animated":false}]"#);
        assert_eq!(words(&l), ["StillGif"]);
        assert!(!l[0].1.animated);
        assert_eq!(l[0].2.proxy.as_deref(), Some("emotes/bttv/emote/5f1b0186cf6d2144653d2972/2x"));
        // A `null` in an optional field reads as the field left out, the
        // emote kept (0.0.7 left each of these emotes out). A BetterTTV
        // GIF whose `animated` is `null` moves, as one without the field
        // does (barbed-emotes' rule where the answer does not say; the
        // game's own parser called it still).
        let l = bttv_global(r#"[{"id":"5f1b0186cf6d2144653d2973","code":"NullGif","imageType":"gif","animated":null,"modifier":null},{"id":"5f1b0186cf6d2144653d2974","code":"NullPng","imageType":"png","animated":null}]"#);
        assert_eq!(words(&l), ["NullGif", "NullPng"]);
        assert!(l[0].1.animated && !l[1].1.animated);
        assert_eq!(l[1].2.proxy.as_deref(), Some("emotes/bttv/emote/5f1b0186cf6d2144653d2974/2x"));
        let l = room(r#"{"id":1,"name":"Nulls","hidden":null,"modifier":null,"modifier_flags":null,"animated":null,"urls":{"2":"https://cdn.frankerfacez.com/emote/1/2"}}"#);
        assert_eq!(words(&l), ["Nulls"]);
        assert!(!l[0].1.animated);
        assert_eq!(l[0].2.proxy.as_deref(), Some("emotes/ffz/emote/1/2"));
        let l = seven(r#"{"id":"b1","name":"Nulls","flags":null,"data":{"flags":null,"animated":null,"host":{"url":"//cdn.7tv.app/emote/b1","files":[{"name":"2x.webp","static_name":"2x.webp"}]}}}"#);
        assert_eq!(words(&l), ["Nulls"]);
        assert!(!l[0].1.animated);
        assert_eq!(l[0].2.proxy.as_deref(), Some("emotes/7tv/emote/b1/2x.webp"));
        let t = twitch_global(r#"{"data":[{"id":"25","name":"Kappa","format":null,"scale":null,"theme_mode":null}]}"#);
        assert_eq!(words(&t), ["Kappa"]);
        assert!(!t[0].1.animated);
        assert_eq!(t[0].2.url, "https://static-cdn.jtvnw.net/emoticons/v2/25/static/dark/2.0");
    }

    #[test]
    fn twitch_animated_images_are_gifs_on_the_same_host() {
        let i = twitch_animated_image("emotesv2_1").unwrap();
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
        let i = twitch_image("25").unwrap();
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
        // A still emote's image is its `2x.webp` itself, the file its
        // answer lists by `name` (round BE, barbed-emotes' reading; until
        // then its `static_name`, which this made-up fixture gives as
        // `2x_static.webp`: 7TV's real still emotes give `2x.webp` there
        // too, so the two readings asked the same file of every real one).
        assert!(user[1].2.url.ends_with("/2x.webp"), "{}", user[1].2.url);
        // The set's name for an emote (an alias), a zero-width one left
        // out, the 2x WebP among AVIF and GIF files.
        let global = seven_tv_global(&fixture("7tv-global.json"));
        assert_eq!(words(&global), ["Clap", "catJAM"]);
        assert!(global[0].2.url.ends_with("/2x.webp"), "{}", global[0].2.url);
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
        // A modifier left out.
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
        // The default sets alone, a hidden emote and modifiers left out.
        let global = ffz_global(&fixture("ffz-global.json"));
        assert_eq!(words(&global), ["ZreknarF", "FzSecond"]);
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

    /// Round TW5: Twitch's global emotes as Helix's documentation shows
    /// them, the animated ones from their animated image.
    #[test]
    fn twitchs_global_emotes_are_read() {
        let list = twitch_global(&fixture("twitch-global.json"));
        assert_eq!(
            words(&list),
            ["TwitchUnity", "DemoDance", "DemoWave", "DemoGrin", "DemoNod", "Demo Space"]
        );
        assert!(!list[0].1.animated && list[1].1.animated);
        assert_eq!(
            list[1].2.url,
            "https://static-cdn.jtvnw.net/emoticons/v2/emotesv2_a11/animated/dark/2.0"
        );
        assert_eq!(list[0].2.url, "https://static-cdn.jtvnw.net/emoticons/v2/196892/static/dark/2.0");
        assert!(twitch_global("{}").is_empty() && twitch_global("not json").is_empty());
        // The global sets' addresses; Twitch's through the proxy alone.
        let t = global_address(Provider::Twitch).unwrap();
        assert_eq!(t.url, "");
        assert_eq!(t.proxy.as_deref(), Some("emotes/twitch-api/chat/emotes/global"));
        assert_eq!(
            global_address(Provider::Bttv).unwrap().url,
            "https://api.betterttv.net/3/cached/emotes/global"
        );
        assert!(global_address(Provider::Test).is_none());
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
