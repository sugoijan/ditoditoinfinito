//! The chat layer's items over time (C6, C8, C9, C10, C11): what comes in
//! within `chat-overlay-v0`'s budgets, what the moderation events take
//! off, where each item goes and how it moves, all on the host's clock
//! (seconds), never the song's. Deterministic from the events, the
//! clock, the [`Options`] and the [`Area`]: the shells measure the text
//! and say which emote images are ready; nothing here draws.
//!
//! - **Across**: rows, right to left, each item at a speed that keeps it
//!   on screen the same time whatever its length; a row takes a new item
//!   only once its last one has cleared the right edge and cannot be
//!   caught before it leaves (niconico's rule).
//! - **Falling**: columns, top to bottom, every item at one speed, a
//!   column taking a new item once its last one has come a line down.
//! - Reduced motion: nothing travels; items take free places in the rows
//!   (or the columns' cells), fade in, stay and fade out.
//!
//! Full means dropped, never queued: late chat is worse than missing
//! chat.

use std::collections::{HashMap, VecDeque};

use crate::emotes::{Emote, EmoteSets};
use crate::eventsub::Event;
use crate::item::{Content, Cut, Left, Piece};
use crate::{Level, Style};

/// `chat-overlay-v0`'s numbers (C9). Every one PROVISIONAL.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rules {
    pub name: &'static str,
    /// Items on screen at most (waiting for their images included).
    pub on_screen: usize,
    /// New items a second at most.
    pub per_second: usize,
    /// Seconds between two items of one chatter, at least.
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
}

/// `chat-overlay-v0` (`crate::RULES_NAME`).
pub const CHAT_OVERLAY_V0: Rules = Rules {
    name: crate::RULES_NAME,
    on_screen: 40,
    per_second: 8,
    per_chatter: 2.0,
    cut: Cut {
        emotes: 5,
        graphemes: 60,
    },
    wait: 1.0,
    across: 7.0,
    falling: 8.0,
    still: 5.0,
    fade: 0.4,
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

/// Where items go, in the target's pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Area {
    pub size: [f32; 2],
    /// An item's height.
    pub line: f32,
    /// The rows' tops (Across), in the order they are taken.
    pub rows: Vec<f32>,
    /// The columns (Falling).
    pub columns: Vec<Column>,
}

/// A column of Falling: its left and right edges, and where its items
/// come in and go out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Column {
    pub x: [f32; 2],
    pub y: [f32; 2],
}

/// Rows a line apart and a fifth of a line between.
const PITCH: f32 = 1.2;
/// Room between items in a row or a column, in lines.
const GAP: f32 = 0.8;
/// An emote at most this many times as wide as tall.
const WIDEST: f32 = 4.0;
/// The space between pieces and after a chatter's name, in lines.
const SPACE: f32 = 0.3;
/// An item narrowed to fit a column, to this share at most.
const NARROWEST: f32 = 0.5;

impl Area {
    /// The area for a `size` target with items `line` tall: rows down the
    /// whole height but half a line at the top and the bottom, taken from
    /// the top; Falling's columns in the side space beside the field's
    /// `band` (left and right edges) clear of `obstacles` (the miniatures'
    /// slots, `[left, top, right, bottom]`) where there is room for a
    /// column there, else over the side space behind them, else over the
    /// whole width (a phone held upright), each the whole height.
    pub fn new(
        size: [f32; 2],
        line: f32,
        band: [f32; 2],
        obstacles: &[[f32; 4]],
        level: Level,
    ) -> Area {
        let [w, h] = size;
        let line = line.max(1.0);
        let margin = line * 0.5;
        let widest = if level == Level::Messages { 14.0 } else { 5.0 } * line;
        let narrowest = if level == Level::Messages { 7.0 } else { 2.5 } * line;
        let sides = [[margin, band[0] - margin], [band[1] + margin, w - margin]];
        let wide_enough = |r: &[f32; 2]| r[1] - r[0] >= narrowest;
        // The side space less the obstacles' widths.
        let mut free: Vec<[f32; 2]> = Vec::new();
        for side in sides.iter().filter(|s| wide_enough(s)) {
            let mut parts = vec![*side];
            for o in obstacles {
                let (a, b) = (o[0] - margin, o[2] + margin);
                parts = parts
                    .into_iter()
                    .flat_map(|p| {
                        if b <= p[0] || a >= p[1] {
                            vec![p]
                        } else {
                            vec![[p[0], a.min(p[1])], [b.max(p[0]), p[1]]]
                        }
                    })
                    .filter(|p| p[1] > p[0])
                    .collect();
            }
            free.extend(parts.into_iter().filter(wide_enough));
        }
        let spaces: Vec<[f32; 2]> = if !free.is_empty() {
            free
        } else {
            let behind: Vec<[f32; 2]> = sides.into_iter().filter(wide_enough).collect();
            if behind.is_empty() {
                vec![[margin, w - margin]]
            } else {
                behind
            }
        };
        let mut columns = Vec::new();
        for s in spaces {
            let n = ((s[1] - s[0]) / widest).ceil().max(1.0);
            let each = (s[1] - s[0]) / n;
            for k in 0..n as usize {
                let x = s[0] + each * k as f32;
                columns.push(Column {
                    x: [x, x + each],
                    y: [0.0, h],
                });
            }
        }
        let pitch = line * PITCH;
        let rows = (0..)
            .map(|k| margin + pitch * k as f32)
            .take_while(|y| y + line <= h - margin)
            .collect();
        Area {
            size,
            line,
            rows,
            columns,
        }
    }

    /// The area kept clear of `keep_out` (`[left, top, right, bottom]`:
    /// the receptors' rows, the HUD): no row crosses one (a row crosses
    /// the whole width), and a column starts below one over it in the top
    /// half and ends above one under it in the bottom half; the rows taken
    /// from the bottom first where `from_bottom` (the receptors at the
    /// top: the chat as far from them as it can be).
    pub fn keeping_clear(mut self, keep_out: &[[f32; 4]], from_bottom: bool) -> Area {
        let line = self.line;
        let gap = line * 0.25;
        self.rows.retain(|&y| {
            keep_out
                .iter()
                .all(|k| y + line + gap <= k[1] || y >= k[3] + gap)
        });
        if from_bottom {
            self.rows.reverse();
        }
        let half = self.size[1] / 2.0;
        for c in &mut self.columns {
            for k in keep_out {
                if k[2] <= c.x[0] || k[0] >= c.x[1] {
                    continue;
                }
                if k[3] <= half {
                    c.y[0] = c.y[0].max(k[3] + gap);
                } else if k[1] >= half {
                    c.y[1] = c.y[1].min(k[1] - gap);
                }
            }
        }
        self.columns.retain(|c| c.y[1] - c.y[0] >= line * 3.0);
        self
    }
}

/// A piece of an item as placed: its offset from the item's left and
/// its width, pixels at the item's own size.
#[derive(Clone, Debug, PartialEq)]
pub struct Laid {
    pub what: Glyphs,
    pub x: f32,
    pub width: f32,
}

/// What a laid piece draws.
#[derive(Clone, Debug, PartialEq)]
pub enum Glyphs {
    /// The chatter's name leading the item.
    Name(String),
    Text(String),
    Emote(Emote),
    /// A run's count after its emote (`×3`).
    Count(String),
}

/// An item as drawn this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawn<'a> {
    pub id: u64,
    /// Its box's top left, pixels; its height the area's line times
    /// `scale`.
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub alpha: f32,
    pub width: f32,
    pub laid: &'a [Laid],
}

/// What happened to what came in, for the debug state and the tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub messages: u64,
    pub shown: u64,
    /// Left out by C11 (commands, links, cheers, shared chat) or empty.
    pub left_out: u64,
    /// Dropped by a budget: 40 on screen, 8 a second, a chatter's 2 s.
    pub full: u64,
    pub rate: u64,
    pub chatter: u64,
    /// Dropped while frames ran late.
    pub shed: u64,
    /// No room in any row or column when its turn came.
    pub no_room: u64,
    /// Its emotes never came and nothing else was left.
    pub no_image: u64,
    /// Taken off by a moderation event.
    pub removed: u64,
}

struct Waiting {
    id: u64,
    since: f64,
    content: Content,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Lane {
    Row(usize),
    Column(usize),
    /// Reduced motion: a row's or a column's cell (`k`).
    Fixed(usize, usize),
}

struct Shown {
    id: u64,
    content: Content,
    laid: Vec<Laid>,
    width: f32,
    scale: f32,
    lane: Lane,
    /// Where it was at `born`, and its velocity, pixels a second.
    at: [f32; 2],
    velocity: [f32; 2],
    born: f64,
    ends: f64,
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
                if self.len() >= self.rules.on_screen {
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
                let id = self.next;
                self.next += 1;
                self.waiting.push(Waiting {
                    id,
                    since: now,
                    content,
                });
            }
            Event::Delete { message_id } => self.remove(|c| c.message_id == *message_id),
            Event::ClearUser { user_id } => self.remove(|c| c.chatter_id == *user_id),
            Event::Clear => self.remove(|_| true),
            Event::Other(_) => {}
        }
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
        let waiting = std::mem::take(&mut self.waiting);
        for w in waiting {
            let states: Vec<Readiness> = w.content.emotes().map(ready).collect();
            let due = now - w.since >= self.rules.wait;
            if !due && states.contains(&Readiness::Waiting) {
                self.waiting.push(w);
                continue;
            }
            let Some((laid, width)) = lay(&w.content, area.line, ready, measure) else {
                self.stats.no_image += 1;
                continue;
            };
            if self.place(w.id, w.content, laid, width, now, area) {
                self.stats.shown += 1;
            } else {
                self.stats.no_room += 1;
            }
        }
    }

    /// Puts an item in a free row or column; whether there was one.
    fn place(
        &mut self,
        id: u64,
        content: Content,
        laid: Vec<Laid>,
        width: f32,
        now: f64,
        area: &Area,
    ) -> bool {
        let r = self.rules;
        let [w, _] = area.size;
        let line = area.line;
        let pitch = line * PITCH;
        let gap = line * GAP;
        let rows = area.rows.len();
        let mut shown = Shown {
            id,
            content,
            laid,
            width,
            scale: 1.0,
            lane: Lane::Row(0),
            at: [0.0; 2],
            velocity: [0.0; 2],
            born: now,
            ends: now,
        };
        let placed = match (self.options.style, self.options.reduced) {
            (Style::Across, false) => {
                let speed = (w + width) / r.across as f32;
                let free = (0..rows).find(|&row| {
                    let last = self
                        .shown
                        .iter()
                        .filter(|s| s.lane == Lane::Row(row))
                        .max_by(|a, b| a.born.total_cmp(&b.born));
                    let Some(p) = last else { return true };
                    let t = (now - p.born) as f32;
                    // Its right end has cleared the right edge…
                    let right = p.at[0] + p.width + p.velocity[0] * t;
                    if right > w - gap {
                        return false;
                    }
                    // …and the new one cannot catch it before it leaves.
                    let left_for = (p.ends - now) as f32;
                    left_for <= 0.0 || w - speed * left_for >= gap
                });
                free.map(|row| {
                    shown.lane = Lane::Row(row);
                    shown.at = [w, area.rows[row]];
                    shown.velocity = [-speed, 0.0];
                    shown.ends = now + r.across;
                })
            }
            (Style::Falling, false) => {
                // The free column whose last item is oldest.
                let free = (0..area.columns.len())
                    .filter_map(|c| {
                        let [top, bottom] = area.columns[c].y;
                        let speed = (bottom - top + line) / r.falling as f32;
                        let last = self
                            .shown
                            .iter()
                            .filter(|s| s.lane == Lane::Column(c))
                            .map(|s| s.born)
                            .fold(f64::NEG_INFINITY, f64::max);
                        let came = (now - last) as f32 * speed;
                        (came >= pitch + gap * 0.5).then_some((c, last))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
                free.map(|(c, _)| {
                    let Column { x: [x0, x1], y: [top, bottom] } = area.columns[c];
                    let speed = (bottom - top + line) / r.falling as f32;
                    let s = ((x1 - x0) / width.max(1.0)).clamp(NARROWEST, 1.0);
                    shown.scale = s;
                    shown.lane = Lane::Column(c);
                    shown.at = [x0 + ((x1 - x0) - width * s).max(0.0) / 2.0, top - line * s];
                    shown.velocity = [0.0, speed];
                    shown.ends = now + r.falling * f64::from((bottom - top + line * s) / (bottom - top + line));
                })
            }
            (Style::Across, true) => {
                // The row with fewest items, at the first place that fits.
                let margin = line * 0.5;
                let mut order: Vec<usize> = (0..rows).collect();
                let count = |row: usize, shown: &[Shown]| {
                    shown.iter().filter(|s| matches!(s.lane, Lane::Fixed(r, _) if r == row)).count()
                };
                order.sort_by_key(|&row| (count(row, &self.shown), row));
                order.into_iter().find_map(|row| {
                    let mut taken: Vec<[f32; 2]> = self
                        .shown
                        .iter()
                        .filter(|s| matches!(s.lane, Lane::Fixed(r, _) if r == row))
                        .map(|s| [s.at[0], s.at[0] + s.width])
                        .collect();
                    taken.sort_by(|a, b| a[0].total_cmp(&b[0]));
                    // From a place of its own in each row, so items do
                    // not all line up at the left.
                    let start = margin + (w - 2.0 * margin) * ((row as f32 * 0.618).fract() * 0.5);
                    let candidates = std::iter::once(start)
                        .chain(taken.iter().map(|t| t[1] + gap))
                        .chain(std::iter::once(margin));
                    for x in candidates {
                        if x + width > w - margin {
                            continue;
                        }
                        if taken.iter().all(|t| x + width + gap <= t[0] || x >= t[1] + gap) {
                            return Some((row, x));
                        }
                    }
                    None
                })
                .map(|(row, x)| {
                    shown.lane = Lane::Fixed(row, 0);
                    shown.at = [x, area.rows[row]];
                    shown.ends = now + r.still;
                })
            }
            (Style::Falling, true) => {
                let mut best: Option<(usize, usize, usize)> = None;
                for (c, column) in area.columns.iter().enumerate() {
                    let [top, bottom] = column.y;
                    let cells = (((bottom - top) / pitch).floor().max(1.0)) as usize;
                    let used: Vec<usize> = self
                        .shown
                        .iter()
                        .filter_map(|s| match s.lane {
                            Lane::Fixed(col, k) if col == c => Some(k),
                            _ => None,
                        })
                        .collect();
                    // Cells spread down the column: every other first.
                    let free = (0..cells)
                        .step_by(2)
                        .chain((1..cells).step_by(2))
                        .find(|k| !used.contains(k));
                    if let Some(k) = free
                        && best.is_none_or(|b| used.len() < b.2)
                    {
                        best = Some((c, k, used.len()));
                    }
                }
                best.map(|(c, k, _)| {
                    let Column { x: [x0, x1], y: [top, _] } = area.columns[c];
                    let s = ((x1 - x0) / width.max(1.0)).clamp(NARROWEST, 1.0);
                    shown.scale = s;
                    shown.lane = Lane::Fixed(c, k);
                    shown.at = [
                        x0 + ((x1 - x0) - width * s).max(0.0) / 2.0,
                        top + pitch * k as f32,
                    ];
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
            // Standing still, or falling in a column that starts or ends
            // within the screen: faded in and out.
            let alpha = if matches!(s.lane, Lane::Fixed(..) | Lane::Column(_)) {
                let into = (t / fade).min(1.0);
                let out = ((s.ends - now) / fade).min(1.0);
                into.min(out).max(0.0) as f32
            } else {
                1.0
            };
            Drawn {
                id: s.id,
                x: s.at[0] + s.velocity[0] * t as f32,
                y: s.at[1] + s.velocity[1] * t as f32,
                scale: s.scale,
                alpha,
                width: s.width,
                laid: &s.laid,
            }
        })
    }
}

/// `content`'s pieces laid on one line `line` tall: the name, the text,
/// the emotes whose images are ready (the others left out), a run's
/// count after its emote. `None` where nothing is left.
fn lay(
    content: &Content,
    line: f32,
    ready: &dyn Fn(&Emote) -> Readiness,
    measure: &mut dyn FnMut(&str) -> f32,
) -> Option<(Vec<Laid>, f32)> {
    let space = line * SPACE;
    let mut laid = Vec::new();
    let mut x = 0.0f32;
    let put = |what: Glyphs, width: f32, laid: &mut Vec<Laid>, x: &mut f32| {
        laid.push(Laid {
            what,
            x: *x,
            width,
        });
        *x += width;
    };
    let mut body = false;
    for p in &content.pieces {
        match p {
            Piece::Text(t) => {
                if t.trim().is_empty() && !body {
                    continue;
                }
                let w = measure(t);
                put(Glyphs::Text(t.clone()), w, &mut laid, &mut x);
                body |= !t.trim().is_empty();
            }
            Piece::Emote { emote, count } => {
                let Readiness::Ready { aspect } = ready(emote) else {
                    continue;
                };
                let w = line * aspect.clamp(0.25, WIDEST);
                put(Glyphs::Emote(emote.clone()), w, &mut laid, &mut x);
                body = true;
                if *count > 1 {
                    let text = format!("×{count}");
                    let w = measure(&text);
                    put(Glyphs::Count(text), w, &mut laid, &mut x);
                }
                if content.emote_only() {
                    x += space;
                }
            }
        }
    }
    if !body {
        return None;
    }
    // A trailing space left after the last emote.
    if content.emote_only() {
        x -= space;
    }
    if let Some(name) = &content.name {
        let w = measure(name);
        let shift = w + space * 1.5;
        for l in &mut laid {
            l.x += shift;
        }
        laid.insert(
            0,
            Laid {
                what: Glyphs::Name(name.clone()),
                x: 0.0,
                width: w,
            },
        );
        x += shift;
    }
    Some((laid, x.max(1.0)))
}

#[cfg(test)]
mod tests;
