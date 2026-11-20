//! The chat layer's items over time (C6, C8, C9, C10, C11): what comes in
//! within `chat-overlay-v1`'s budgets, what the moderation events take
//! off, where each item goes and how it moves, all on the host's clock
//! (seconds), never the song's. Deterministic from the events, the
//! clock, the [`Options`] and the [`Area`]: the shells measure the text
//! and say which emote images are ready; nothing here draws.
//!
//! The layer is an ambient effect behind the play (round TW5, the
//! maintainer, 2026-11-20: "I want them to be behind the fields not in
//! between them, it's not necessarily going to be easy to read or to
//! notice everything, but that's fine"): items go over the whole screen,
//! behind the play's field, the opponents' fields and the HUD (the field
//! filter's band dims them in the lanes), and every emote is drawn by
//! itself, never summarized.
//!
//! - **Across**: items cross the whole screen right to left, each at a
//!   speed that keeps it on screen its own time whatever its length, at a
//!   height of its own; an item never starts where another is still
//!   coming in, nor where it would catch one before that one leaves
//!   (niconico's rule, over any heights rather than rows).
//! - **Falling**: the same, falling from the top over the whole width.
//! - Reduced motion: nothing travels; items take free places anywhere on
//!   the screen, fade in, stay and fade out.
//!
//! Full means dropped, never queued: late chat is worse than missing
//! chat.

use std::collections::{HashMap, VecDeque};

use crate::emotes::{Emote, EmoteSets};
use crate::eventsub::Event;
use crate::item::{Content, Cut, Left, Piece};
use crate::{Level, Style};

/// How the items look (round TW5's candidates, shown to the maintainer in
/// pictures; PROVISIONAL until they choose).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub name: &'static str,
    /// An emote's height in an emote-only item, in lines.
    pub emote: f32,
    /// An emote's height inline in a message's line, in lines (the text
    /// is a line tall).
    pub inline: f32,
    /// Each item's size varies by up to this share either way (0: every
    /// item the same).
    pub vary_size: f32,
    /// Each item's time on screen varies by up to this share either way,
    /// so a burst does not move as one block.
    pub vary_speed: f32,
    /// An emote-only message's emotes each an item of their own, coming
    /// in this many seconds apart, each at a place of its own (0: one item,
    /// its emotes in a row).
    pub scatter: f64,
}

/// As built in round TW4, every emote drawn: items a line tall, emotes as
/// tall as the text, all alike.
pub const LOOK_AS_BUILT: Look = Look {
    name: "as-built",
    emote: 1.0,
    inline: 1.0,
    vary_size: 0.0,
    vary_speed: 0.0,
    scatter: 0.0,
};

/// Emotes larger than the text, sizes and speeds varying a little.
pub const LOOK_LARGER: Look = Look {
    name: "larger",
    emote: 1.6,
    inline: 1.35,
    vary_size: 0.15,
    vary_speed: 0.2,
    scatter: 0.0,
};

/// As [`LOOK_LARGER`], an emote-only message's emotes each coming in by
/// itself, a little apart, so a burst is a cloud rather than rows.
pub const LOOK_SCATTERED: Look = Look {
    name: "scattered",
    emote: 1.6,
    inline: 1.35,
    vary_size: 0.2,
    vary_speed: 0.25,
    scatter: 0.12,
};

/// The candidates, by name (the tests' and the studies' pick).
pub const LOOKS: [Look; 3] = [LOOK_AS_BUILT, LOOK_LARGER, LOOK_SCATTERED];

/// `chat-overlay-v1`'s numbers (C9, round TW5). Every one PROVISIONAL.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rules {
    pub name: &'static str,
    /// Messages on screen at most (waiting for their images included; a
    /// scattered message counts once).
    pub on_screen: usize,
    /// Emotes on screen at most, every one counted.
    pub emotes_on_screen: usize,
    /// New messages a second at most.
    pub per_second: usize,
    /// Seconds between two messages of one chatter, at least.
    pub per_chatter: f64,
    pub cut: Cut,
    /// Seconds an item waits for its emotes' images before it goes
    /// without those not ready.
    pub wait: f64,
    /// Seconds an item is on screen: crossing, falling, standing still
    /// (reduced motion), and the still one's fades in and out.
    pub across: f64,
    pub falling: f64,
    pub still: f64,
    pub fade: f64,
    pub look: Look,
}

/// `chat-overlay-v1` (`crate::RULES_NAME`): no summarizing, budgets for
/// emotes as individual things. A message brings eight emotes at most;
/// the screen holds 80 messages and 160 emotes at most, 20 new messages a
/// second, one a chatter every 2 s; the rest is dropped (and the space on
/// screen drops more: an item that finds no free place is left out).
pub const CHAT_OVERLAY_V1: Rules = Rules {
    name: crate::RULES_NAME,
    on_screen: 80,
    emotes_on_screen: 160,
    per_second: 20,
    per_chatter: 2.0,
    cut: Cut {
        emotes: 8,
        graphemes: 60,
    },
    wait: 1.0,
    across: 7.0,
    falling: 8.0,
    still: 5.0,
    fade: 0.4,
    look: LOOK_LARGER,
};

/// A device's choices for the layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    /// What is shown (the device's under the room's ceiling).
    pub level: Level,
    pub style: Style,
    /// Chatter names lead messages.
    pub names: bool,
    /// Nothing travels (C10).
    pub reduced: bool,
}

/// Whether an emote's image can be drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Readiness {
    /// Drawn at this width over its height.
    Ready { aspect: f32 },
    /// Asked for, not in yet.
    Waiting,
    /// Never coming (failed, or nobody may ask for it).
    Failed,
}

/// Where items go: the whole target, in its pixels (round TW5: behind
/// everything, nothing routed around).
#[derive(Clone, Debug, PartialEq)]
pub struct Area {
    pub size: [f32; 2],
    /// A line of text's height; emotes are [`Look::emote`] lines tall.
    pub line: f32,
}

impl Area {
    pub fn new(size: [f32; 2], line: f32) -> Area {
        Area {
            size,
            line: line.max(1.0),
        }
    }
}

/// Room between items, along their way, in lines.
const GAP: f32 = 0.5;
/// Room between items side by side, in lines.
const BESIDE: f32 = 0.12;
/// The places across an item's way are this many lines apart.
const STEP: f32 = 0.25;
/// An emote at most this many times as wide as tall.
const WIDEST: f32 = 4.0;
/// The space between pieces and after a chatter's name, in lines.
const SPACE: f32 = 0.3;
/// Places a still item (reduced motion) tries before it is left out.
const STILL_TRIES: u64 = 96;

/// A piece of an item as placed: its offset from the item's left, its
/// width and its height (centered in the item's), pixels at the item's
/// own size.
#[derive(Clone, Debug, PartialEq)]
pub struct Laid {
    pub what: Glyphs,
    pub x: f32,
    pub width: f32,
    pub height: f32,
}

/// What a laid piece draws: a name, text or one emote. There is no count:
/// four emotes are four pieces (round TW5).
#[derive(Clone, Debug, PartialEq)]
pub enum Glyphs {
    /// The chatter's name leading the item.
    Name(String),
    Text(String),
    Emote(Emote),
}

/// An item as drawn this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawn<'a> {
    pub id: u64,
    /// Its box's top left, pixels; its size `width` by `height` times
    /// `scale`.
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub alpha: f32,
    pub width: f32,
    pub height: f32,
    pub laid: &'a [Laid],
}

/// What happened to what came in, for the debug state and the tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub messages: u64,
    /// Items placed (a scattered message's emotes each one).
    pub shown: u64,
    /// Left out by C11 (commands, links, cheers, shared chat) or empty.
    pub left_out: u64,
    /// Dropped by a budget: the messages or the emotes on screen, the
    /// messages a second, a chatter's 2 s.
    pub full: u64,
    pub rate: u64,
    pub chatter: u64,
    /// Dropped while frames ran late.
    pub shed: u64,
    /// No free place when its turn came.
    pub no_room: u64,
    /// Its emotes never came and nothing else was left.
    pub no_image: u64,
    /// Taken off by a moderation event.
    pub removed: u64,
    /// Emotes a message brought past its eight, left out.
    pub cut: u64,
}

struct Waiting {
    id: u64,
    since: f64,
    /// Not placed before this second (a scattered message's later
    /// emotes).
    due: f64,
    content: Content,
    /// Split from a scattered message: its emotes' images are in.
    part: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Motion {
    Across,
    Falling,
    Still,
}

struct Shown {
    id: u64,
    content: Content,
    laid: Vec<Laid>,
    /// Its size at scale 1, and its scale.
    size: [f32; 2],
    scale: f32,
    motion: Motion,
    /// Where it was at `born`, and its velocity, pixels a second.
    at: [f32; 2],
    velocity: [f32; 2],
    born: f64,
    ends: f64,
}

impl Shown {
    fn box_size(&self) -> [f32; 2] {
        self.size.map(|v| v * self.scale)
    }

    fn top_left(&self, now: f64) -> [f32; 2] {
        let t = (now - self.born).max(0.0) as f32;
        [self.at[0] + self.velocity[0] * t, self.at[1] + self.velocity[1] * t]
    }

    /// How far its leading edge is past the edge it came in at, and its
    /// length along its way (`area`'s `size`).
    fn lead(&self, now: f64, size: [f32; 2]) -> (f32, f32) {
        let [x, y] = self.top_left(now);
        let [w, h] = self.box_size();
        match self.motion {
            Motion::Across => (size[0] - x, w),
            _ => (y + h, h),
        }
    }

    /// Its extent across its way.
    fn beside(&self) -> [f32; 2] {
        let [w, h] = self.box_size();
        match self.motion {
            Motion::Across => [self.at[1], self.at[1] + h],
            _ => [self.at[0], self.at[0] + w],
        }
    }
}

/// A number from `id` and `salt`, the same every time (splitmix64).
fn hash(id: u64, salt: u64) -> u64 {
    let mut z = id
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .wrapping_add(salt.wrapping_mul(0xbf58_476d_1ce4_e5b9));
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A share in `-1..=1` from `id` and `salt`.
fn spread(id: u64, salt: u64) -> f32 {
    (hash(id, salt) % 20_001) as f32 / 10_000.0 - 1.0
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// The layer's state over a play.
pub struct Overlay {
    rules: Rules,
    options: Options,
    sets: EmoteSets,
    next: u64,
    waiting: Vec<Waiting>,
    shown: Vec<Shown>,
    admitted: VecDeque<f64>,
    chatters: HashMap<String, f64>,
    stats: Stats,
}

impl Overlay {
    pub fn new(rules: Rules, options: Options, sets: EmoteSets) -> Overlay {
        Overlay {
            rules,
            options,
            sets,
            next: 1,
            waiting: Vec::new(),
            shown: Vec::new(),
            admitted: VecDeque::new(),
            chatters: HashMap::new(),
            stats: Stats::default(),
        }
    }

    pub fn options(&self) -> Options {
        self.options
    }

    pub fn rules(&self) -> &Rules {
        &self.rules
    }

    /// The emote sets words are matched against (they come in when the
    /// providers answer; items made before keep theirs).
    pub fn set_sets(&mut self, sets: EmoteSets) {
        self.sets = sets;
    }

    pub fn sets(&self) -> &EmoteSets {
        &self.sets
    }

    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// Items on screen and waiting for their images.
    pub fn len(&self) -> usize {
        self.waiting.len() + self.shown.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Messages on screen or waiting (a scattered message's items once).
    pub fn messages(&self) -> usize {
        let mut ids: Vec<&str> = self
            .waiting
            .iter()
            .map(|w| &*w.content.message_id)
            .chain(self.shown.iter().map(|s| &*s.content.message_id))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids.len()
    }

    /// Emotes on screen or waiting, every one.
    pub fn emotes(&self) -> usize {
        self.waiting
            .iter()
            .map(|w| &w.content)
            .chain(self.shown.iter().map(|s| &s.content))
            .map(|c| c.emotes().count())
            .sum()
    }

    /// The emotes items wait for (each once, in order): what a shell's
    /// cache fetches.
    pub fn wanted(&self) -> Vec<&Emote> {
        let mut out: Vec<&Emote> = Vec::new();
        for w in &self.waiting {
            for e in w.content.emotes() {
                if !out.contains(&e) {
                    out.push(e);
                }
            }
        }
        out
    }

    /// The emotes drawn now (a cache keeps them).
    pub fn drawn_emotes(&self) -> impl Iterator<Item = &Emote> {
        self.shown.iter().flat_map(|s| {
            s.laid.iter().filter_map(|l| match &l.what {
                Glyphs::Emote(e) => Some(e),
                _ => None,
            })
        })
    }

    /// An event at host second `now`; `accepting` false while the play's
    /// frames run late (no new items then; moderation still applies).
    pub fn receive(&mut self, event: &Event, now: f64, accepting: bool) {
        match event {
            Event::Message(m) => {
                self.stats.messages += 1;
                if self.options.level == Level::Off {
                    return;
                }
                if !accepting {
                    self.stats.shed += 1;
                    return;
                }
                let content = match crate::item::content(
                    m,
                    &self.sets,
                    self.options.level,
                    self.options.names,
                    self.rules.cut,
                ) {
                    Ok(c) => c,
                    Err(Left::Command | Left::Link | Left::Cheer | Left::Shared | Left::Empty) => {
                        self.stats.left_out += 1;
                        return;
                    }
                };
                if self.messages() >= self.rules.on_screen
                    || self.emotes() + content.emotes().count() > self.rules.emotes_on_screen
                {
                    self.stats.full += 1;
                    return;
                }
                while self.admitted.front().is_some_and(|t| now - t >= 1.0) {
                    self.admitted.pop_front();
                }
                if self.admitted.len() >= self.rules.per_second {
                    self.stats.rate += 1;
                    return;
                }
                if self
                    .chatters
                    .get(&content.chatter_id)
                    .is_some_and(|t| now - t < self.rules.per_chatter)
                {
                    self.stats.chatter += 1;
                    return;
                }
                if self.chatters.len() > 512 {
                    let keep = self.rules.per_chatter;
                    self.chatters.retain(|_, t| now - *t < keep);
                }
                self.chatters.insert(content.chatter_id.clone(), now);
                self.admitted.push_back(now);
                self.stats.cut += content.left_out as u64;
                let id = self.take_id();
                self.waiting.push(Waiting {
                    id,
                    since: now,
                    due: now,
                    content,
                    part: false,
                });
            }
            Event::Delete { message_id } => self.remove(|c| c.message_id == *message_id),
            Event::ClearUser { user_id } => self.remove(|c| c.chatter_id == *user_id),
            Event::Clear => self.remove(|_| true),
            Event::Other(_) => {}
        }
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next;
        self.next += 1;
        id
    }

    fn remove(&mut self, which: impl Fn(&Content) -> bool) {
        let before = self.len();
        self.waiting.retain(|w| !which(&w.content));
        self.shown.retain(|s| !which(&s.content));
        self.stats.removed += (before - self.len()) as u64;
    }

    /// The layer at host second `now` in `area`: items past their time
    /// go, waiting ones whose images are in (or waited long enough) are
    /// laid with `measure` (a text's width at the line's size) and
    /// placed, or dropped where there is no room.
    pub fn advance(
        &mut self,
        now: f64,
        area: &Area,
        ready: &dyn Fn(&Emote) -> Readiness,
        measure: &mut dyn FnMut(&str) -> f32,
    ) {
        self.shown.retain(|s| s.ends > now);
        let look = self.rules.look;
        let waiting = std::mem::take(&mut self.waiting);
        let mut later = Vec::new();
        for w in waiting {
            if w.due > now {
                later.push(w);
                continue;
            }
            let states: Vec<Readiness> = w.content.emotes().map(ready).collect();
            let due = w.part || now - w.since >= self.rules.wait;
            if !due && states.contains(&Readiness::Waiting) {
                later.push(w);
                continue;
            }
            // A scattered message: each of its emotes ready an item of its
            // own, coming in a little apart.
            let emotes: Vec<Emote> = w
                .content
                .emotes()
                .filter(|e| matches!(ready(e), Readiness::Ready { .. }))
                .cloned()
                .collect();
            if !w.part && look.scatter > 0.0 && w.content.emote_only() && emotes.len() > 1 {
                for (k, e) in emotes.into_iter().enumerate() {
                    let id = if k == 0 { w.id } else { self.take_id() };
                    later.push(Waiting {
                        id,
                        since: w.since,
                        due: now + look.scatter * k as f64,
                        content: Content {
                            pieces: vec![Piece::Emote(e)],
                            left_out: 0,
                            ..w.content.clone()
                        },
                        part: true,
                    });
                }
                continue;
            }
            // A row of emotes as wide as the screen allows (its eight are
            // its cap); a line of text, Falling's narrower.
            let most = match self.options.style {
                Style::Falling if !w.content.emote_only() => (area.line * 14.0).min(area.size[0] * 0.9),
                _ => area.size[0] * 0.9,
            };
            let Some((laid, size)) = lay(&w.content, area.line, look, most, ready, measure) else {
                self.stats.no_image += 1;
                continue;
            };
            if self.place(w.id, w.content, laid, size, now, area) {
                self.stats.shown += 1;
            } else {
                self.stats.no_room += 1;
            }
        }
        // Those split now come in after the others waiting.
        later.sort_by(|a, b| a.due.total_cmp(&b.due).then(a.id.cmp(&b.id)));
        self.waiting = later;
    }

    /// Puts an item in a free place; whether there was one.
    fn place(
        &mut self,
        id: u64,
        content: Content,
        laid: Vec<Laid>,
        size: [f32; 2],
        now: f64,
        area: &Area,
    ) -> bool {
        let r = self.rules;
        let look = r.look;
        let [sw, sh] = area.size;
        let line = area.line;
        let margin = line * 0.25;
        let scale = 1.0 + look.vary_size * spread(id, 1);
        let [w, h] = size.map(|v| v * scale);
        let time = 1.0 + f64::from(look.vary_speed * spread(id, 2));
        let motion = if self.options.reduced {
            Motion::Still
        } else {
            match self.options.style {
                Style::Across => Motion::Across,
                Style::Falling => Motion::Falling,
            }
        };
        let mut shown = Shown {
            id,
            content,
            laid,
            size,
            scale,
            motion,
            at: [0.0; 2],
            velocity: [0.0; 2],
            born: now,
            ends: now,
        };
        let placed = match motion {
            Motion::Across | Motion::Falling => {
                let across = motion == Motion::Across;
                // Its way's length, its length along it, its width across.
                let (length, along, beside, room) = if across {
                    (sw, w, h, sh)
                } else {
                    (sh, h, w, sw)
                };
                let seconds = if across { r.across } else { r.falling } * time;
                let speed = (length + along) / seconds as f32;
                let ends = now + seconds;
                let gap = line * GAP;
                let side = line * BESIDE;
                let span = room - 2.0 * margin - beside;
                let places = if span < 0.0 {
                    1
                } else {
                    (span / (line * STEP)).floor() as usize + 1
                };
                // The places in an order of the item's own: a burst is
                // spread over the screen, never stacked from one edge.
                let start = (hash(id, 3) % places as u64) as usize;
                let stride = [97, 89, 61, 37, 23, 13, 7, 5, 3, 1]
                    .into_iter()
                    .find(|s| gcd(*s, places) == 1)
                    .unwrap_or(1);
                let others: Vec<(f32, f32, [f32; 2], f32, f64)> = self
                    .shown
                    .iter()
                    .filter(|s| s.motion == motion)
                    .map(|s| {
                        let (lead, along) = s.lead(now, area.size);
                        let v = s.velocity[0].abs().max(s.velocity[1].abs());
                        (lead, along, s.beside(), v, s.ends)
                    })
                    .collect();
                let free = (0..places).map(|k| (start + k * stride) % places).find(|&k| {
                    let p = if span < 0.0 {
                        (room - beside) / 2.0
                    } else {
                        margin + line * STEP * k as f32
                    };
                    others.iter().all(|&(lead, o_along, [a, b], v, o_ends)| {
                        if p + beside + side <= a || p >= b + side {
                            return true;
                        }
                        // The other's tail has come in by a gap…
                        if lead - o_along < gap {
                            return false;
                        }
                        // …and this one does not catch it before either
                        // has gone.
                        let t = (o_ends.min(ends) - now).max(0.0) as f32;
                        speed * t + gap <= lead - o_along + v * t
                    })
                });
                free.map(|k| {
                    let p = if span < 0.0 {
                        (room - beside) / 2.0
                    } else {
                        margin + line * STEP * k as f32
                    };
                    if across {
                        shown.at = [sw, p];
                        shown.velocity = [-speed, 0.0];
                    } else {
                        shown.at = [p, -h];
                        shown.velocity = [0.0, speed];
                    }
                    shown.ends = ends;
                })
            }
            Motion::Still => {
                // A free place anywhere, tried in an order of its own.
                let side = line * BESIDE * 2.0;
                let xs = ((sw - 2.0 * margin - w) / (line * 0.5)).floor().max(0.0) as u64 + 1;
                let ys = ((sh - 2.0 * margin - h) / (line * STEP)).floor().max(0.0) as u64 + 1;
                let taken: Vec<[f32; 4]> = self
                    .shown
                    .iter()
                    .filter(|s| s.motion == Motion::Still)
                    .map(|s| {
                        let [w, h] = s.box_size();
                        [s.at[0], s.at[1], s.at[0] + w, s.at[1] + h]
                    })
                    .collect();
                (0..STILL_TRIES)
                    .map(|k| {
                        let x = margin + (hash(id, 10 + 2 * k) % xs) as f32 * line * 0.5;
                        let y = margin + (hash(id, 11 + 2 * k) % ys) as f32 * line * STEP;
                        [x, y]
                    })
                    .find(|&[x, y]| {
                        taken.iter().all(|t| {
                            x + w + side <= t[0] || x >= t[2] + side || y + h + side <= t[1] || y >= t[3] + side
                        })
                    })
                    .map(|at| {
                        shown.at = at;
                        shown.ends = now + r.still;
                    })
            }
        };
        if placed.is_some() {
            self.shown.push(shown);
            true
        } else {
            false
        }
    }

    /// The items on screen at host second `now`, back to front.
    pub fn drawn(&self, now: f64) -> impl Iterator<Item = Drawn<'_>> {
        let fade = self.rules.fade;
        self.shown.iter().filter(move |s| s.ends > now).map(move |s| {
            let t = (now - s.born).max(0.0);
            // Standing still: faded in and out. Moving items come in and
            // leave at the screen's edges.
            let alpha = if s.motion == Motion::Still {
                let into = (t / fade).min(1.0);
                let out = ((s.ends - now) / fade).min(1.0);
                into.min(out).max(0.0) as f32
            } else {
                1.0
            };
            let [x, y] = s.top_left(now);
            Drawn {
                id: s.id,
                x,
                y,
                scale: s.scale,
                alpha,
                width: s.size[0],
                height: s.size[1],
                laid: &s.laid,
            }
        })
    }
}

/// `content`'s pieces laid on one line, `most` pixels wide at most: the
/// name, the text (a line tall, cut with an ellipsis where it would run
/// past), every emote whose image is ready (the others left out; each
/// [`Look::emote`] lines tall in an emote-only item, [`Look::inline`] in a
/// line of text). Its pieces and its size; `None` where nothing is left.
fn lay(
    content: &Content,
    line: f32,
    look: Look,
    most: f32,
    ready: &dyn Fn(&Emote) -> Readiness,
    measure: &mut dyn FnMut(&str) -> f32,
) -> Option<(Vec<Laid>, [f32; 2])> {
    let space = line * SPACE;
    let emote_only = content.emote_only();
    let eh = line * if emote_only { look.emote } else { look.inline };
    let mut laid = Vec::new();
    let mut x = 0.0f32;
    if let Some(name) = &content.name {
        let w = measure(name);
        laid.push(Laid {
            what: Glyphs::Name(name.clone()),
            x: 0.0,
            width: w,
            height: line,
        });
        x = w + space * 1.5;
    }
    let mut body = false;
    'pieces: for p in &content.pieces {
        match p {
            Piece::Text(t) => {
                if t.trim().is_empty() && !body {
                    continue;
                }
                let mut text = t.clone();
                let mut w = measure(&text);
                if x + w > most {
                    // As many graphemes as fit with an ellipsis.
                    use unicode_segmentation::UnicodeSegmentation;
                    let g: Vec<&str> = t.graphemes(true).collect();
                    let (mut lo, mut hi) = (0, g.len());
                    while lo < hi {
                        let mid = (lo + hi).div_ceil(2);
                        let cut = format!("{}…", g[..mid].concat().trim_end());
                        if x + measure(&cut) <= most {
                            lo = mid;
                        } else {
                            hi = mid - 1;
                        }
                    }
                    if lo == 0 {
                        break 'pieces;
                    }
                    text = format!("{}…", g[..lo].concat().trim_end());
                    w = measure(&text);
                    body = true;
                    laid.push(Laid {
                        what: Glyphs::Text(text),
                        x,
                        width: w,
                        height: line,
                    });
                    x += w;
                    break 'pieces;
                }
                body |= !t.trim().is_empty();
                laid.push(Laid {
                    what: Glyphs::Text(text),
                    x,
                    width: w,
                    height: line,
                });
                x += w;
            }
            Piece::Emote(emote) => {
                let Readiness::Ready { aspect } = ready(emote) else {
                    continue;
                };
                let w = eh * aspect.clamp(0.25, WIDEST);
                if body && x + w > most {
                    break 'pieces;
                }
                laid.push(Laid {
                    what: Glyphs::Emote(emote.clone()),
                    x,
                    width: w,
                    height: eh,
                });
                x += w;
                body = true;
                if emote_only {
                    x += eh * 0.15;
                }
            }
        }
    }
    if !body {
        return None;
    }
    // A trailing space left after the last emote.
    if emote_only {
        x -= eh * 0.15;
    }
    let height = laid.iter().map(|l| l.height).fold(0.0, f32::max);
    Some((laid, [x.max(1.0), height.max(1.0)]))
}

#[cfg(test)]
mod tests;
