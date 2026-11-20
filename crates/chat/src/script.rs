//! The scripted chat (C16; TESTING ONLY): a script of chat events on a
//! clock, played as the EventSub messages Twitch would send (so they go
//! through [`crate::eventsub::parse`] as real ones do), with emotes whose
//! images are the script's own plain shapes or addresses, never a
//! provider's. The web's `mpfakechat=<name or rate>` and desktop's
//! `--mp-dev-chat FILE` play one; the room's ceiling over the device's
//! choice is a line of the script until rooms carry their own.
//!
//! A script is lines (`#` starts a comment):
//!
//! ```text
//! ceiling messages              # off | emotes | messages
//! emote Kappa twitch 25 shape:circle:f08030
//! emote catJAM 7tv c1 shape:triangle:3080f0
//! emote wideHappy 7tv w1 shape:bar:a050e0:3
//! loop 30                       # starts again every 30 s
//! 0.2 say alice Hello chat Kappa
//! 1.0 say @gone gus this one is deleted later
//! 4.0 flood 50 2                # 50 messages a second for 2 s
//! 6.0 emotes 20 2               # 20 emote-only messages a second for 2 s
//! 7.0 animated 20 2             # the same, of the animated emotes alone
//! 8.0 delete @gone
//! 9.0 clear-user alice
//! 10.0 clear
//! ```
//!
//! A `say`'s words naming a `twitch` emote become Twitch emote fragments;
//! the other providers' words stay text, matched by the sets as real
//! third-party words are. An image is `shape:<kind>:<rrggbb>[:<aspect>]`
//! (`circle`, `square`, `triangle`, `diamond`, `bar`; drawn by
//! [`shape_pixels`]), an animated one
//! `anim:<kind>:<rrggbb>:<frames>:<ms>[:<aspect>]` (the shape pulsing
//! with a light sweeping across it, each frame `ms` long; drawn by
//! [`anim_pixels`], round TW4), or an address a shell fetches (a local
//! test server's file: an `anim:` emote's `.gif`, [`Script::images_at`]).
//! Every picture is made by code, never a provider's.

use crate::Level;
use crate::emotes::{Emote, EmoteSets, Image, Provider, Source};

/// One timed line.
#[derive(Clone, Debug, PartialEq)]
enum Step {
    Say {
        label: Option<String>,
        chatter: String,
        text: String,
    },
    Flood {
        rate: f64,
        seconds: f64,
        emotes: bool,
        /// Its emotes the animated ones alone.
        animated: bool,
    },
    Delete(String),
    ClearUser(String),
    Clear,
    Raw(String),
}

/// A script's emote: its word, provider, id and image, and whether it
/// moves (an `anim:` picture).
#[derive(Clone, Debug, PartialEq)]
struct ScriptEmote {
    word: String,
    provider: Provider,
    id: String,
    image: String,
    animated: bool,
}

impl ScriptEmote {
    fn emote(&self) -> Emote {
        let e = Emote::new(self.provider, self.id.clone());
        if self.animated { e.moving() } else { e }
    }
}

/// A parsed script.
#[derive(Clone, Debug, PartialEq)]
pub struct Script {
    ceiling: Level,
    emotes: Vec<ScriptEmote>,
    steps: Vec<(f64, Step)>,
    period: Option<f64>,
}

/// A line that could not be read: its number and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptError {
    pub line: usize,
    pub why: String,
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.why)
    }
}

impl std::error::Error for ScriptError {}

/// The chat's lines in several scripts, for floods and the rate scripts.
const LINES: &[&str] = &[
    "nice", "lets gooo", "that stream though", "こんにちは", "がんばれ！", "すごい",
    "مرحبا", "رائع جدا", "привет", "class", "🎉🎉", "GG", "clean", "wow 🔥",
    "안녕하세요", "olé", "trop bien", "加油", "w", "first time here",
];

/// The emotes every built-in script knows.
const DEMO_EMOTES: &str = "\
emote Kappa twitch 25 shape:circle:f08030
emote PogU twitch 305954156 shape:square:40c060
emote LUL twitch 425618 shape:diamond:f0d040
emote catJAM 7tv c1 shape:triangle:3080f0
emote wideHappy 7tv w1 shape:bar:a050e0:3
emote PogBTTV bttv b1 shape:circle:e04080
emote LilZ ffz f1 shape:diamond:40d0d0
emote PartyKappa twitch emotesv2_p1 anim:circle:f06040:8:100
emote catDance 7tv c2 anim:triangle:3090f0:12:70
emote BttvBounce bttv b2 anim:square:60d070:10:80
emote FfzSpin ffz f2 anim:bar:e0b030:16:50:2
";

const DEMO: &str = "\
ceiling messages
loop 30
0.2 say alice Hello chat! Kappa
0.6 say ken こんにちは！がんばって 🎶
1.0 say laila مرحبا بالجميع، حظا سعيدا
1.4 say bob Kappa Kappa Kappa Kappa
1.8 say dee PogU catJAM wideHappy LilZ
2.0 say sam party time PartyKappa catDance
2.2 say eve !song
2.6 say finn free stuff at example.com/free
3.0 say @gone gus this message is deleted three seconds later
3.4 say hana 🎉🎉🎉 nice combo 🎉
3.8 say ivan Привет всем, удачи
4.2 say jo PogU PogU PogBTTV
4.6 say tom hello hello Kappa
5.0 emotes 6 2
6.0 delete @gone
7.5 say tom still here LUL
8.0 flood 50 2
11.5 say tom one more from tom
12.0 clear-user tom
13.0 say kai 加油！ catJAM
14.0 emotes 4 3
16.0 say pia PartyKappa catDance BttvBounce FfzSpin
17.0 say lee that was clean wideHappy
18.0 clear
19.0 say mia after the clear: Kappa LUL
20.0 flood 12 6
27.0 say ned last one of the loop PogU
";

/// Emote-only bursts.
const EMOTES: &str = "\
ceiling emotes
loop 10
0.0 emotes 6 4
5.0 say ann Kappa Kappa Kappa
5.5 say ben catJAM PogU LilZ wideHappy LUL PogBTTV
5.7 say cid PartyKappa catDance BttvBounce FfzSpin
7.0 say dot catDance catDance FfzSpin
6.0 emotes 3 3
";

/// Animated emotes alone, as many as the layer takes (round TW4: a
/// screen full of them, for the frame's cost).
const ANIMATED: &str = "\
ceiling emotes
loop 10
0.0 animated 12 10
";

/// A flood, then its moderation.
const FLOOD: &str = "\
ceiling messages
loop 12
0.0 flood 50 6
7.0 say @x zed this is deleted
8.0 delete @x
9.0 clear
";

impl Script {
    /// A built-in script by name (`demo`, `emotes`, `animated`, `flood`),
    /// or a rate:
    /// a number of messages a second, mixed.
    pub fn builtin(name: &str) -> Option<Script> {
        let body = match name {
            "demo" | "1" | "" => DEMO.to_string(),
            "emotes" => EMOTES.to_string(),
            "animated" => ANIMATED.to_string(),
            "flood" => FLOOD.to_string(),
            rate => {
                let r: f64 = rate.parse().ok().filter(|r: &f64| r.is_finite() && *r > 0.0)?;
                format!("ceiling messages\nloop 10\n0.0 flood {} 10\n", r.min(200.0))
            }
        };
        Script::parse(&format!("{DEMO_EMOTES}{body}")).ok()
    }

    /// Reads a script.
    pub fn parse(text: &str) -> Result<Script, ScriptError> {
        let mut s = Script {
            ceiling: Level::Messages,
            emotes: Vec::new(),
            steps: Vec::new(),
            period: None,
        };
        for (n, raw) in text.lines().enumerate() {
            let err = |why: &str| ScriptError {
                line: n + 1,
                why: why.to_string(),
            };
            let line = match raw.find(" #") {
                Some(i) => &raw[..i],
                None => raw,
            }
            .trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut words = line.split_whitespace();
            let first = words.next().unwrap_or_default();
            match first {
                "ceiling" => {
                    s.ceiling = match words.next() {
                        Some("off") => Level::Off,
                        Some("emotes") => Level::Emotes,
                        Some("messages") => Level::Messages,
                        _ => return Err(err("ceiling is off, emotes or messages")),
                    }
                }
                "loop" => {
                    s.period = Some(
                        words
                            .next()
                            .and_then(|w| w.parse().ok())
                            .filter(|p: &f64| *p > 0.0)
                            .ok_or_else(|| err("loop takes seconds"))?,
                    )
                }
                "emote" => {
                    let w: Vec<&str> = words.collect();
                    let [word, provider, id, image] = w[..] else {
                        return Err(err("emote <word> <provider> <id> <image>"));
                    };
                    let provider = match provider {
                        "twitch" => Provider::Twitch,
                        "7tv" => Provider::SevenTv,
                        "bttv" => Provider::Bttv,
                        "ffz" => Provider::Ffz,
                        "test" => Provider::Test,
                        _ => return Err(err("a provider is twitch, 7tv, bttv, ffz or test")),
                    };
                    s.emotes.push(ScriptEmote {
                        word: word.into(),
                        provider,
                        id: id.into(),
                        image: image.into(),
                        animated: image.starts_with("anim:"),
                    });
                }
                time => {
                    let at: f64 = time
                        .parse()
                        .ok()
                        .filter(|t: &f64| t.is_finite() && *t >= 0.0)
                        .ok_or_else(|| err("a step starts with its second"))?;
                    let what = words.next().unwrap_or_default();
                    let rest: Vec<&str> = words.collect();
                    let step = match what {
                        "say" => {
                            let mut rest = rest.as_slice();
                            let label = match rest.first() {
                                Some(l) if l.starts_with('@') => {
                                    rest = &rest[1..];
                                    Some(l.to_string())
                                }
                                _ => None,
                            };
                            let Some((chatter, text)) = rest.split_first() else {
                                return Err(err("say <chatter> <text>"));
                            };
                            Step::Say {
                                label,
                                chatter: chatter.to_string(),
                                text: text.join(" "),
                            }
                        }
                        "flood" | "emotes" | "animated" => {
                            let nums: Vec<f64> =
                                rest.iter().filter_map(|w| w.parse().ok()).collect();
                            let [rate, seconds] = nums[..] else {
                                return Err(err("flood <a second> <seconds>"));
                            };
                            Step::Flood {
                                rate: rate.clamp(0.0, 1000.0),
                                seconds: seconds.max(0.0),
                                emotes: what != "flood",
                                animated: what == "animated",
                            }
                        }
                        "delete" => Step::Delete(
                            rest.first().ok_or_else(|| err("delete @label"))?.to_string(),
                        ),
                        "clear-user" => Step::ClearUser(
                            rest.first().ok_or_else(|| err("clear-user <chatter>"))?.to_string(),
                        ),
                        "clear" => Step::Clear,
                        "raw" => Step::Raw(rest.join(" ")),
                        _ => {
                            return Err(err(
                                "say, flood, emotes, animated, delete, clear-user, clear or raw",
                            ));
                        }
                    };
                    s.steps.push((at, step));
                }
            }
        }
        s.steps.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(s)
    }

    /// TESTING ONLY: its emotes' images from `base` instead
    /// (`<base><id>.png`, an animated one's `<base><id>.gif`: a local test
    /// server's files).
    pub fn images_at(&mut self, base: &str) {
        for e in &mut self.emotes {
            let kind = if e.animated { "gif" } else { "png" };
            e.image = format!("{base}{}.{kind}", e.id);
        }
    }

    /// Its emotes: id, whether animated and its picture's spec, for the
    /// tools that write their files (`chat_images`).
    pub fn emote_specs(&self) -> Vec<(String, bool, String)> {
        self.emotes
            .iter()
            .map(|e| (e.id.clone(), e.animated, e.image.clone()))
            .collect()
    }

    /// The room's ceiling this script stands for.
    pub fn ceiling(&self) -> Level {
        self.ceiling
    }

    /// The emote sets it chats with: offline (no provider is asked), each
    /// emote's image its own.
    pub fn sets(&self) -> EmoteSets {
        let mut sets = EmoteSets::offline();
        for (provider, list) in [Provider::SevenTv, Provider::Bttv, Provider::Ffz, Provider::Test]
            .map(|p| {
                (
                    p,
                    self.emotes
                        .iter()
                        .filter(|e| e.provider == p)
                        .map(|e| (e.word.clone(), e.emote(), image(&e.image)))
                        .collect::<Vec<_>>(),
                )
            })
        {
            sets.add(Source::Channel(provider), list);
        }
        for e in &self.emotes {
            sets.set_image(e.emote(), image(&e.image));
        }
        sets
    }
}

fn image(at: &str) -> Image {
    Image {
        url: at.to_string(),
        proxy: None,
    }
}

/// A script played from host second `start`.
pub struct Player {
    script: Script,
    /// The next step, its loop, and the floods running (`(from, until,
    /// rate, (emotes, animated), sent)`).
    next: usize,
    round: u64,
    floods: Vec<Flood>,
    welcomed: bool,
    serial: u64,
    /// Message ids by label, of this round.
    labels: Vec<(String, String)>,
    seed: u64,
}

/// A flood running: from, until, its rate, (emote-only, animated alone),
/// messages sent.
type Flood = (f64, f64, f64, (bool, bool), u64);

const BROADCASTER: &str = "1971641";

impl Player {
    pub fn new(script: Script) -> Player {
        Player {
            script,
            next: 0,
            round: 0,
            floods: Vec::new(),
            welcomed: false,
            serial: 0,
            labels: Vec::new(),
            seed: 0x9e37_79b9_7f4a_7c15,
        }
    }

    pub fn script(&self) -> &Script {
        &self.script
    }

    /// The socket messages due by `t` seconds since the script started.
    pub fn due(&mut self, t: f64) -> Vec<String> {
        let mut out = Vec::new();
        if !self.welcomed {
            self.welcomed = true;
            out.push(welcome());
        }
        loop {
            let base = self.script.period.map_or(0.0, |p| p * self.round as f64);
            let Some((at, step)) = self.script.steps.get(self.next).cloned() else {
                match self.script.period {
                    Some(p) if t >= p * (self.round + 1) as f64 => {
                        self.round += 1;
                        self.next = 0;
                        self.labels.clear();
                        continue;
                    }
                    _ => break,
                }
            };
            if base + at > t {
                break;
            }
            self.next += 1;
            self.run(base + at, step, &mut out);
        }
        // The floods' messages due by `t`, evenly spread.
        let mut floods = std::mem::take(&mut self.floods);
        for (from, until, rate, emotes, sent) in &mut floods {
            let upto = ((t.min(*until) - *from) * *rate).floor().max(0.0) as u64;
            while *sent < upto {
                *sent += 1;
                let m = self.flood_message(*emotes);
                out.push(m);
            }
        }
        floods.retain(|f| f.1 > t);
        self.floods.extend(floods);
        out
    }

    fn rand(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.seed;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.seed = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn flood_message(&mut self, (emotes, animated): (bool, bool)) -> String {
        let who = format!("viewer{}", self.rand() % 400);
        let words: Vec<String> = self
            .script
            .emotes
            .iter()
            .filter(|e| !animated || e.animated)
            .map(|e| e.word.clone())
            .collect();
        let text = if emotes && !words.is_empty() {
            let n = 1 + (self.rand() % 4) as usize;
            let w = words[(self.rand() % words.len() as u64) as usize].clone();
            let mut v = vec![w; n];
            if self.rand().is_multiple_of(3) {
                v.push(words[(self.rand() % words.len() as u64) as usize].clone());
            }
            v.join(" ")
        } else {
            let line = LINES[(self.rand() % LINES.len() as u64) as usize];
            if !words.is_empty() && self.rand().is_multiple_of(2) {
                let w = &words[(self.rand() % words.len() as u64) as usize];
                format!("{line} {w}")
            } else {
                line.to_string()
            }
        };
        self.say(None, &who, &text)
    }

    fn run(&mut self, at: f64, step: Step, out: &mut Vec<String>) {
        match step {
            Step::Say {
                label,
                chatter,
                text,
            } => {
                let m = self.say(label.as_deref(), &chatter, &text);
                out.push(m);
            }
            Step::Flood {
                rate,
                seconds,
                emotes,
                animated,
            } => self.floods.push((at, at + seconds, rate, (emotes, animated), 0)),
            Step::Delete(label) => {
                if let Some((_, id)) = self.labels.iter().find(|(l, _)| *l == label) {
                    out.push(notification(
                        "channel.chat.message_delete",
                        serde_json::json!({
                            "broadcaster_user_id": BROADCASTER,
                            "target_user_id": "",
                            "message_id": id,
                        }),
                    ));
                }
            }
            Step::ClearUser(who) => out.push(notification(
                "channel.chat.clear_user_messages",
                serde_json::json!({
                    "broadcaster_user_id": BROADCASTER,
                    "target_user_id": user_id(&who),
                    "target_user_login": who,
                    "target_user_name": who,
                }),
            )),
            Step::Clear => out.push(notification(
                "channel.chat.clear",
                serde_json::json!({"broadcaster_user_id": BROADCASTER}),
            )),
            Step::Raw(json) => out.push(json),
        }
    }

    fn say(&mut self, label: Option<&str>, chatter: &str, text: &str) -> String {
        self.serial += 1;
        let id = format!("script-{}-{}", self.round, self.serial);
        if let Some(l) = label {
            self.labels.push((l.to_string(), id.clone()));
        }
        let mut fragments: Vec<serde_json::Value> = Vec::new();
        let mut plain = String::new();
        let flush = |plain: &mut String, f: &mut Vec<serde_json::Value>| {
            if !plain.is_empty() {
                f.push(serde_json::json!({"type": "text", "text": std::mem::take(plain),
                    "cheermote": null, "emote": null, "mention": null}));
            }
        };
        for (i, word) in text.split(' ').enumerate() {
            if i > 0 {
                plain.push(' ');
            }
            match self
                .script
                .emotes
                .iter()
                .find(|e| e.provider == Provider::Twitch && e.word == word)
            {
                Some(e) => {
                    flush(&mut plain, &mut fragments);
                    let format: &[&str] = if e.animated {
                        &["static", "animated"]
                    } else {
                        &["static"]
                    };
                    fragments.push(serde_json::json!({"type": "emote", "text": word,
                        "cheermote": null, "mention": null,
                        "emote": {"id": e.id, "emote_set_id": "0", "owner_id": "0", "format": format}}));
                }
                None => plain.push_str(word),
            }
        }
        flush(&mut plain, &mut fragments);
        notification(
            "channel.chat.message",
            serde_json::json!({
                "broadcaster_user_id": BROADCASTER,
                "broadcaster_user_login": "streamer",
                "broadcaster_user_name": "Streamer",
                "chatter_user_id": user_id(chatter),
                "chatter_user_login": chatter.to_lowercase(),
                "chatter_user_name": chatter,
                "message_id": id,
                "message": {"text": text, "fragments": fragments},
                "color": "",
                "badges": [],
                "message_type": "text",
                "cheer": null,
                "reply": null,
                "channel_points_custom_reward_id": null,
                "source_broadcaster_user_id": null,
                "source_message_id": null,
            }),
        )
    }
}

/// A chatter's id from their name (the script's chatters have none).
fn user_id(name: &str) -> String {
    let h = name
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3));
    (h % 1_000_000_000).to_string()
}

fn welcome() -> String {
    serde_json::json!({
        "metadata": {"message_id": "script-welcome", "message_type": "session_welcome",
            "message_timestamp": "2026-01-01T00:00:00Z"},
        "payload": {"session": {"id": "scripted", "status": "connected",
            "keepalive_timeout_seconds": 10, "reconnect_url": null}},
    })
    .to_string()
}

fn notification(kind: &str, event: serde_json::Value) -> String {
    serde_json::json!({
        "metadata": {"message_id": "script", "message_type": "notification",
            "message_timestamp": "2026-01-01T00:00:00Z",
            "subscription_type": kind, "subscription_version": "1"},
        "payload": {"subscription": {"type": kind, "status": "enabled"}, "event": event},
    })
    .to_string()
}

/// A scripted emote's picture (`shape:<kind>:<rrggbb>[:<aspect>]`): its
/// size and its RGBA pixels (straight alpha), a plain shape on a clear
/// ground with a dark rim, [`crate::cache::CELL`] pixels tall. `None` for
/// anything else.
pub fn shape_pixels(spec: &str) -> Option<([u32; 2], Vec<u8>)> {
    let mut parts = spec.strip_prefix("shape:")?.split(':');
    let kind = parts.next()?;
    let rgb = color(parts.next()?)?;
    let aspect: f32 = parts.next().and_then(|a| a.parse().ok()).unwrap_or(1.0);
    let size = shape_size(aspect);
    Some((size, draw_shape(kind, rgb, size, 1.0, None)?))
}

/// A scripted animated emote's frames
/// (`anim:<kind>:<rrggbb>:<frames>:<ms>[:<aspect>]`, round TW4): their
/// size, every frame's RGBA pixels one after another, and each frame's
/// duration, milliseconds. The shape pulses and a light sweeps across it,
/// so every frame differs. At most 64 frames; `None` for anything else.
pub fn anim_pixels(spec: &str) -> Option<([u32; 2], Vec<u8>, Vec<u32>)> {
    let mut parts = spec.strip_prefix("anim:")?.split(':');
    let kind = parts.next()?;
    let rgb = color(parts.next()?)?;
    let frames: usize = parts.next()?.parse().ok().filter(|n| (1..=64).contains(n))?;
    let ms: u32 = parts.next()?.parse().ok().filter(|n| (1..=10_000).contains(n))?;
    let aspect: f32 = parts.next().and_then(|a| a.parse().ok()).unwrap_or(1.0);
    let size = shape_size(aspect);
    let mut rgba = Vec::with_capacity((size[0] * size[1] * 4) as usize * frames);
    for k in 0..frames {
        let phase = k as f32 / frames as f32;
        let pulse = 0.78 + 0.22 * (phase * std::f32::consts::TAU).cos();
        rgba.extend(draw_shape(kind, rgb, size, pulse, Some(phase))?);
    }
    Some((size, rgba, vec![ms; frames]))
}

fn color(hex: &str) -> Option<[u8; 3]> {
    let c = u32::from_str_radix(hex, 16).ok()?;
    Some([(c >> 16) as u8, (c >> 8) as u8, c as u8])
}

fn shape_size(aspect: f32) -> [u32; 2] {
    let h = crate::cache::CELL;
    [((h as f32 * aspect.clamp(0.25, 4.0)).round() as u32).max(1), h]
}

/// `kind` in `rgb` on a `size` picture, at `scale` of its full size, with
/// a light band across it at `sweep` (0 to 1, left to right).
fn draw_shape(kind: &str, rgb: [u8; 3], size: [u32; 2], scale: f32, sweep: Option<f32>) -> Option<Vec<u8>> {
    let [w, h] = size;
    let mut px = vec![0u8; (w * h * 4) as usize];
    let (fw, fh) = (w as f32, h as f32);
    for y in 0..h {
        for x in 0..w {
            // Coverage by sampling the pixel's four corners' middles.
            let mut inside = 0.0;
            let mut rim = 0.0;
            for (dx, dy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                let u = ((x as f32 + dx) / fw * 2.0 - 1.0) / scale;
                let v = ((y as f32 + dy) / fh * 2.0 - 1.0) / scale;
                let d = match kind {
                    "circle" => (u * u + v * v).sqrt() - 0.9,
                    "square" => u.abs().max(v.abs()) - 0.85,
                    "diamond" => u.abs() + v.abs() - 0.95,
                    "triangle" => (v - 0.85).max((1.842 * u.abs() - 0.9 - v) / 2.1),
                    "bar" => (u.abs() - 0.92).max(v.abs() - 0.6),
                    _ => return None,
                };
                if d <= 0.0 {
                    inside += 0.25;
                    if d > -0.12 {
                        rim += 0.25;
                    }
                }
            }
            let i = ((y * w + x) * 4) as usize;
            let k = if inside > 0.0 { rim / inside } else { 0.0 };
            let light = sweep.map_or(0.0, |s| {
                let at = (x as f32 + 0.5) / fw;
                (1.0 - ((at - s).abs() * 6.0)).clamp(0.0, 1.0) * 0.8
            });
            for ch in 0..3 {
                let c = f32::from(rgb[ch]) * (1.0 - 0.6 * k);
                px[i + ch] = (c + (255.0 - c) * light) as u8;
            }
            px[i + 3] = (inside * 255.0) as u8;
        }
    }
    Some(px)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventsub::{Event, Message, parse};

    fn events(p: &mut Player, t: f64) -> Vec<Message> {
        p.due(t).iter().map(|m| parse(m).expect("a socket message")).collect()
    }

    #[test]
    fn the_built_in_scripts_parse() {
        for name in ["demo", "emotes", "flood", "animated", "25"] {
            let s = Script::builtin(name).unwrap_or_else(|| panic!("{name}"));
            assert!(!s.steps.is_empty(), "{name}");
        }
        assert_eq!(Script::builtin("demo").unwrap().ceiling(), Level::Messages);
        assert_eq!(Script::builtin("emotes").unwrap().ceiling(), Level::Emotes);
        assert!(Script::builtin("nope").is_none());
        assert!(Script::parse("1.0 dance").is_err());
    }

    #[test]
    fn a_script_plays_as_twitch_would_send_it() {
        let s = Script::parse(
            "emote Kappa twitch 25 shape:circle:ff0000\n\
             0.5 say @a alice hi Kappa\n\
             1.0 delete @a\n\
             2.0 clear-user alice\n\
             3.0 clear\n",
        )
        .unwrap();
        let mut p = Player::new(s);
        let first = events(&mut p, 0.0);
        assert!(matches!(first[0], Message::Welcome { .. }));
        let m = events(&mut p, 0.6);
        let Message::Notification(Event::Message(msg)) = &m[0] else {
            panic!("{m:?}")
        };
        assert_eq!(msg.chatter_name, "alice");
        assert_eq!(msg.fragments.len(), 2);
        let d = events(&mut p, 1.0);
        assert_eq!(
            d,
            [Message::Notification(Event::Delete {
                message_id: msg.id.clone()
            })]
        );
        let c = events(&mut p, 3.0);
        assert!(matches!(c[0], Message::Notification(Event::ClearUser { .. })));
        assert_eq!(c[1], Message::Notification(Event::Clear));
    }

    #[test]
    fn a_flood_sends_its_rate_and_the_loop_comes_round() {
        let mut p = Player::new(Script::builtin("50").unwrap());
        let n = events(&mut p, 2.0).len();
        // The welcome and 50 a second for two seconds.
        assert_eq!(n, 1 + 100);
        // The loop's second round.
        let n = events(&mut p, 12.0).len();
        assert_eq!(n, 400 + 100);
    }

    #[test]
    fn its_emotes_are_its_own_pictures_never_a_providers() {
        let s = Script::builtin("demo").unwrap();
        let sets = s.sets();
        let kappa = Emote::twitch("25");
        assert_eq!(sets.image(&kappa).unwrap().url, "shape:circle:f08030");
        assert_eq!(sets.word("catJAM"), Some(&Emote::new(Provider::SevenTv, "c1")));
        // An emote it does not name: nothing asked.
        assert_eq!(sets.image(&Emote::twitch("1")), None);
    }

    #[test]
    fn shapes_are_drawn() {
        let (size, px) = shape_pixels("shape:circle:ff8000").unwrap();
        assert_eq!(size, [64, 64]);
        let mid = ((32 * 64 + 32) * 4) as usize;
        assert_eq!(&px[mid..mid + 4], [255, 128, 0, 255]);
        assert_eq!(px[3], 0);
        let (size, _) = shape_pixels("shape:bar:00ff00:3").unwrap();
        assert_eq!(size, [192, 64]);
        for k in ["square", "triangle", "diamond"] {
            assert!(shape_pixels(&format!("shape:{k}:123456")).is_some());
        }
        assert!(shape_pixels("https://x").is_none());
    }

    /// Round TW4: the scripted chat's animated emotes, drawn in code.
    #[test]
    fn animated_shapes_have_frames_that_differ() {
        let (size, px, d) = anim_pixels("anim:circle:ff8000:8:100").unwrap();
        assert_eq!(size, [64, 64]);
        assert_eq!(d, vec![100; 8]);
        let frame = (64 * 64 * 4) as usize;
        assert_eq!(px.len(), 8 * frame);
        assert_ne!(px[..frame], px[frame..2 * frame]);
        let (wide, _, _) = anim_pixels("anim:bar:00ff00:4:50:2").unwrap();
        assert_eq!(wide, [128, 64]);
        assert!(anim_pixels("anim:circle:ff8000:0:100").is_none());
        assert!(anim_pixels("anim:circle:ff8000:65:100").is_none());
        assert!(anim_pixels("shape:circle:ff8000").is_none());
        // Its Twitch emote offered animated, the others' words animated.
        let s = Script::builtin("demo").unwrap();
        let sets = s.sets();
        let party = Emote::twitch("emotesv2_p1").moving();
        assert_eq!(sets.image(&party).unwrap().url, "anim:circle:f06040:8:100");
        assert!(sets.word("catDance").is_some_and(|e| e.animated));
        let mut p = Player::new(s);
        let events = events(&mut p, 2.1);
        let animated = events.iter().any(|m| {
            matches!(m, Message::Notification(Event::Message(c))
                if c.fragments.iter().any(|f| matches!(f, crate::eventsub::Fragment::Emote { animated: true, .. })))
        });
        assert!(animated);
        // Their files: an animated one's a GIF.
        let mut s = Script::builtin("emotes").unwrap();
        s.images_at("/chat/");
        let specs = s.emote_specs();
        assert!(specs.iter().any(|(id, a, at)| id == "c2" && *a && at == "/chat/c2.gif"));
        assert!(specs.iter().any(|(id, a, at)| id == "25" && !*a && at == "/chat/25.png"));
    }
}
