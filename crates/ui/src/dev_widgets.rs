//! The widget developer screen (`docs/plans/ui-core.md`, step 4): every
//! widget on one screen, run by both shells (`#/ui-dev?screen=widgets`,
//! the desktop's `--widgets`, or the developer screen's Widgets button).
//!
//! - A list of ten thousand songs under two hundred packs' headers (the
//!   library's own songs first, with their banners; the rest made up,
//!   most with one of the library's banners, some without so the
//!   placeholder shows). A header's Start or tap closes or opens its pack.
//! - A search field with suggestions from the titles, artists and packs
//!   (`ddi_library::suggest`, folded by the shell's NFKD); submitting
//!   selects the first song that matches.
//! - Option rows (style, speed, which results panels), a toggle, a
//!   slider.
//! - Results-like panels laid out by a grid frame (`crate::layout`):
//!   judgement counts as bars, a timing histogram, a life graph, a
//!   summary; the panels row chooses which.
//!
//! What it did is in [`WidgetState`], shared with the shell for tests.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use ddi_actor::{Actor, Kind};
use ddi_library::suggest::{Kind as SuggestKind, Suggester};

use crate::graph;
use crate::hit;
use crate::look::{self, Look, Plain, Spec, call};
use crate::message::Message;
use crate::nav::Focusable;
use crate::request::{EnterKey, InputHints, InputMode};
use crate::screen::{Cx, Handled, Input, Screen};
use crate::widgets::controls::{OptionRow, Slider, Toggle};
use crate::widgets::field::{FieldEvent, SearchField};
use crate::widgets::list::{ItemKind, List, ListConfig, ListEvent, ListModel};
use crate::widgets::set_text;

/// A song of the shell's library.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LibrarySong {
    pub title: String,
    pub artist: String,
    pub pack: Option<String>,
    /// Its banner as a texture source.
    pub banner: Option<String>,
}

/// What the screen did, shared with the shell.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WidgetState {
    /// The shell's library; [`LIBRARY_MESSAGE`] after it changes.
    pub library: Vec<LibrarySong>,
    pub status: String,
    pub selected: usize,
    pub scroll: f32,
    pub items: usize,
    /// Item actors built (bounded by what shows).
    pub pool: usize,
    pub style: usize,
    pub speed: usize,
    /// The list's motion blur now, virtual pixels.
    pub blur: f32,
    pub panels: usize,
    pub reverse: bool,
    pub volume: f32,
    pub search: String,
    pub suggestions: Vec<String>,
    pub popup: bool,
    pub submitted: Vec<String>,
}

pub type SharedWidgets = Rc<RefCell<WidgetState>>;

/// The message a shell sends once [`WidgetState::library`] is filled.
pub const LIBRARY_MESSAGE: &str = "DevLibrary";

/// NFKD as the shell has it (the browser's, or Unicode's tables).
pub type FoldFn = Rc<dyn Fn(&str) -> String>;

const PACKS: usize = 200;
const PER_PACK: usize = 50;
const ADJECTIVES: [&str; 20] = [
    "Silver", "Crimson", "Midnight", "Electric", "Paper", "Velvet", "Neon", "Quiet", "Broken",
    "Golden", "Hidden", "Frozen", "Lucky", "Distant", "Rapid", "Gentle", "Wild", "Hollow",
    "Bright", "Last",
];
const NOUNS: [&str; 25] = [
    "Sakura",
    "Drops",
    "Dream",
    "Signal",
    "Garden",
    "Engine",
    "Harbor",
    "Comet",
    "Mirror",
    "Rain",
    "Circuit",
    "Lantern",
    "Echo",
    "Valley",
    "Pulse",
    "Summer",
    "Tide",
    "Orbit",
    "Thunder",
    "Parade",
    "さくら",
    "ゆめ",
    "ほしぞら",
    "東京",
    "花火",
];
const ARTISTS: [&str; 12] = [
    "Sevish",
    "Kommisar",
    "おにぎり",
    "Dito Band",
    "M. Ito",
    "Kana Lab",
    "The Arrows",
    "Rinne",
    "Café Sound",
    "ジャン",
    "Leafy",
    "Okami",
];

/// A song of the list.
#[derive(Clone, Debug)]
struct Song {
    title: String,
    artist: String,
    banner: Option<String>,
}

#[derive(Clone, Debug)]
struct Pack {
    name: String,
    songs: Vec<Song>,
}

#[derive(Clone, Copy, Debug)]
enum Item {
    Header(usize),
    Row(usize, usize),
}

/// The songs, in packs, some closed.
struct Songs {
    packs: Vec<Pack>,
    closed: BTreeSet<usize>,
    items: Vec<Item>,
}

impl Songs {
    fn new(library: &[LibrarySong]) -> Songs {
        let banners: Vec<String> = library.iter().filter_map(|s| s.banner.clone()).collect();
        let mut packs = Vec::with_capacity(PACKS);
        // The library's own songs first, as they are.
        let mut own: Vec<Song> = library
            .iter()
            .map(|s| Song {
                title: s.title.clone(),
                artist: s.artist.clone(),
                banner: s.banner.clone(),
            })
            .collect();
        let mut n = 0usize;
        for p in 0..PACKS {
            let name = if p == 0 && !library.is_empty() {
                "Library".to_string()
            } else {
                format!("Pack {:03}", p + 1)
            };
            let mut songs = Vec::with_capacity(PER_PACK);
            if p == 0 {
                songs.append(&mut own);
                songs.truncate(PER_PACK);
            }
            while songs.len() < PER_PACK {
                let a = ADJECTIVES[n % ADJECTIVES.len()];
                let b = NOUNS[(n / ADJECTIVES.len() + n) % NOUNS.len()];
                // Most made-up songs borrow a banner; every third has none.
                let banner = (!banners.is_empty() && !n.is_multiple_of(3))
                    .then(|| banners[n % banners.len()].clone());
                songs.push(Song {
                    title: format!("{a} {b}"),
                    artist: ARTISTS[(n * 7) % ARTISTS.len()].to_string(),
                    banner,
                });
                n += 1;
            }
            packs.push(Pack { name, songs });
        }
        let mut s = Songs {
            packs,
            closed: BTreeSet::new(),
            items: Vec::new(),
        };
        s.lay_out();
        s
    }

    fn lay_out(&mut self) {
        self.items.clear();
        for (p, pack) in self.packs.iter().enumerate() {
            self.items.push(Item::Header(p));
            if !self.closed.contains(&p) {
                self.items
                    .extend((0..pack.songs.len()).map(|s| Item::Row(p, s)));
            }
        }
    }

    fn rows(&self) -> usize {
        self.packs.iter().map(|p| p.songs.len()).sum()
    }

    fn suggester(&self, fold: &FoldFn) -> Suggester {
        let f = |s: &str| fold(s);
        let mut cands: Vec<(&str, &str, SuggestKind)> = Vec::new();
        for p in &self.packs {
            cands.push((&p.name, "", SuggestKind::Pack));
            for s in &p.songs {
                cands.push((&s.title, "", SuggestKind::Title));
                cands.push((&s.artist, "", SuggestKind::Artist));
            }
        }
        Suggester::new(cands, &f)
    }
}

impl ListModel for Songs {
    fn len(&self) -> usize {
        self.items.len()
    }

    fn kind(&self, i: usize) -> ItemKind {
        match self.items[i] {
            Item::Header(_) => ItemKind::Header,
            Item::Row(..) => ItemKind::Row,
        }
    }

    fn key(&self, i: usize) -> Option<u64> {
        Some(match self.items[i] {
            Item::Header(p) => (1 << 40) | p as u64,
            Item::Row(p, s) => ((p as u64) << 20) | s as u64,
        })
    }

    fn fill(&self, i: usize, item: &mut Actor, cx: &mut Cx) {
        match self.items[i] {
            Item::Header(p) => {
                let pack = &self.packs[p];
                let mark = if self.closed.contains(&p) {
                    "▸"
                } else {
                    "▾"
                };
                set_text(
                    item,
                    "Title",
                    &format!("{mark} {} ({})", pack.name, pack.songs.len()),
                );
            }
            Item::Row(p, s) => {
                let song = &self.packs[p].songs[s];
                set_text(item, "Title", &song.title);
                set_text(item, "Subtitle", &song.artist);
                let banner = song.banner.as_deref().map(|b| cx.texture(b, 128.0));
                crate::widgets::set_image(item, "Banner", banner);
            }
        }
    }
}

/// The widget screen.
pub struct WidgetScreen {
    pub state: SharedWidgets,
    fold: FoldFn,
    look: Plain,
    songs: Songs,
    /// Built at the first keystroke (folding ten thousand titles through
    /// the browser's NFKD is not free), again after the library changes.
    suggester: RefCell<Option<Suggester>>,
    list: List,
    search: SearchField,
    style: OptionRow,
    speed: OptionRow,
    panels: OptionRow,
    reverse: Toggle,
    volume: Slider,
}

const PANEL_SETS: [&[&str]; 3] = [
    &["Judgements", "Timing", "Life", "Summary"],
    &["Timing"],
    &["Life", "Summary"],
];

impl WidgetScreen {
    pub fn new(state: SharedWidgets, fold: FoldFn) -> WidgetScreen {
        let songs = Songs::new(&state.borrow().library);
        let list = List::new(ListConfig::new("Songs", [16.0, 16.0, 380.0, 508.0]), &songs);
        let x = 676.0;
        let w = [520.0, 34.0];
        let mut search = SearchField::new("Search", "Search songs, artists, packs", [x, 34.0], w);
        search.hints = InputHints {
            mode: InputMode::Search,
            enter: EnterKey::Search,
            max_chars: Some(80),
        };
        WidgetScreen {
            fold,
            look: Plain,
            suggester: RefCell::new(None),
            list,
            search,
            style: OptionRow::new(
                "Style",
                "Style",
                [x, 86.0],
                w,
                &["Single", "Double", "Solo"],
                0,
            ),
            speed: OptionRow::new(
                "Speed",
                "Speed",
                [x, 126.0],
                w,
                &["×1", "×1.5", "×2", "×2.5", "×3", "×4"],
                3,
            ),
            panels: OptionRow::new(
                "Panels",
                "Results",
                [x, 166.0],
                w,
                &["All", "Timing", "Life"],
                0,
            ),
            reverse: Toggle::new("Reverse", "Reverse", [x, 206.0], w, false),
            volume: Slider::new("Volume", "Volume", [x, 246.0], w, 0.8),
            songs,
            state,
        }
    }

    fn status(&self, root: &mut Actor, s: String) {
        set_text(root, "WStatus", &s);
        self.state.borrow_mut().status = s;
    }

    fn publish(&self) {
        let mut s = self.state.borrow_mut();
        s.selected = self.list.selected().unwrap_or(0);
        s.scroll = self.list.scroll() as f32;
        s.blur = self.list.blur();
        s.items = self.list.len();
        s.pool = self.list.pool();
        s.style = self.style.index;
        s.speed = self.speed.index;
        s.panels = self.panels.index;
        s.reverse = self.reverse.on;
        s.volume = self.volume.value;
        s.search = self.search.text.clone();
        s.popup = self.search.popup_open();
        s.suggestions = self
            .search
            .suggestions
            .iter()
            .map(|(t, _)| t.clone())
            .collect();
    }

    /// The panels for the chosen set, laid out by a grid frame.
    fn panels_actor(&self) -> Actor {
        let look: &dyn Look = &self.look;
        let rect = [416.0, 278.0, 520.0, 232.0];
        let names = PANEL_SETS[self.panels.index.min(PANEL_SETS.len() - 1)];
        let columns: u16 = if names.len() > 1 { 2 } else { 1 };
        let placed = panel_rects(names, columns, rect);
        let mut children = Vec::new();
        // Deterministic stand-ins for a play's records.
        let offsets = sample_offsets(400);
        for (name, [x, y, w, h]) in placed {
            let mut panel = look::component(look, "Panel", &Spec::new([w, h], &name));
            panel.name = format!("Panel.{name}");
            call(&mut panel, "xy", &[x, y]);
            let inner = [10.0, 26.0, w - 20.0, h - 36.0];
            let content = match name.as_str() {
                "Judgements" => graph::bars(
                    "Judgements.Bars",
                    &[312.0, 61.0, 17.0, 6.0, 1.0, 3.0],
                    312.0,
                    inner,
                    6.0,
                    look::color(look, "Graph.Bar"),
                ),
                "Timing" => graph::histogram_actor(
                    "Timing.Histogram",
                    look,
                    &offsets,
                    [-50.0, 50.0],
                    41,
                    inner,
                ),
                "Life" => {
                    // Rising, a run of misses at 70 s, recovering.
                    let series: Vec<(f32, f32)> = (0..=120)
                        .map(|i| {
                            let t = i as f32;
                            let dip = if (70.0..80.0).contains(&t) {
                                (t - 70.0) * 0.04
                            } else if t >= 80.0 {
                                (0.4 - (t - 80.0) * 0.01).max(0.0)
                            } else {
                                0.0
                            };
                            (t, (0.5 + 0.005 * t - dip).clamp(0.0, 1.0))
                        })
                        .collect();
                    graph::line_graph("Life.Graph", look, &series, [0.0, 120.0], [0.0, 1.0], inner)
                }
                _ => {
                    let mut t = look::text(
                        "Summary.Text",
                        "Score 98.71 %  ·  Max combo 389",
                        14.0,
                        look::color(look, "Text"),
                    );
                    call(&mut t, "xy", &[10.0, 44.0]);
                    call(&mut t, "halign", &[0.0]);
                    t
                }
            };
            if let Kind::Frame(c) = &mut panel.kind {
                c.push(content);
            }
            children.push(panel);
        }
        let mut f = Actor::new(Kind::Frame(children));
        f.name = "PanelGrid".into();
        f
    }

    fn rebuild_panels(&self, root: &mut Actor) {
        let fresh = self.panels_actor();
        if let Some(p) = hit::find_mut(root, "PanelGrid") {
            *p = fresh;
        }
    }

    /// The first row matching `text` (its title, artist or pack), selected.
    fn find(&mut self, text: &str, root: &mut Actor) {
        let f = |s: &str| (self.fold)(s);
        let q = ddi_library::search::Query::new(text, &f);
        let hit = self.songs.items.iter().position(|it| match *it {
            Item::Row(p, s) => {
                let pack = &self.songs.packs[p];
                let song = &pack.songs[s];
                q.matches(&ddi_library::search::haystack(
                    [
                        song.title.as_str(),
                        song.artist.as_str(),
                        pack.name.as_str(),
                    ],
                    [],
                    &f,
                ))
            }
            Item::Header(_) => false,
        });
        match hit {
            Some(i) => {
                self.list.select(i);
                self.status(root, format!("found {text:?} at {i}"));
            }
            None => self.status(root, format!("no song for {text:?}")),
        }
    }

    fn list_event(&mut self, e: ListEvent, root: &mut Actor) {
        match e {
            ListEvent::Selected(i) => {
                if let Some(Item::Row(p, s)) = self.songs.items.get(i) {
                    let t = self.songs.packs[*p].songs[*s].title.clone();
                    self.status(root, format!("selected {t}"));
                }
            }
            ListEvent::Activated(i) => match self.songs.items.get(i).copied() {
                Some(Item::Header(p)) => {
                    // A pack opens or closes; the selection stays on it.
                    if !self.songs.closed.remove(&p) {
                        self.songs.closed.insert(p);
                    }
                    self.songs.lay_out();
                    // The selection stays on the header (by its key).
                    self.list.reload(&self.songs);
                    if let Some(at) = self.list.selected() {
                        self.list.select(at);
                    }
                    self.status(root, format!("pack {} toggled", self.songs.packs[p].name));
                }
                Some(Item::Row(p, s)) => {
                    let t = self.songs.packs[p].songs[s].title.clone();
                    self.status(root, format!("play {t}"));
                }
                None => {}
            },
        }
    }
}

/// Suggestions with what each is.
fn suggest_with(suggester: &Suggester, fold: &FoldFn, typed: &str) -> Vec<(String, String)> {
    let f = |s: &str| fold(s);
    suggester
        .suggest(typed, &f, 6)
        .into_iter()
        .map(|s| {
            let note = match s.kind {
                SuggestKind::Title => "title",
                SuggestKind::Artist => "artist",
                SuggestKind::Pack => "pack",
            };
            (s.text, note.to_string())
        })
        .collect()
}

/// Where the named panels go: a grid frame when layout frames are built
/// in, else equal cells by hand.
fn panel_rects(names: &[&str], columns: u16, rect: [f32; 4]) -> Vec<(String, [f32; 4])> {
    let columns = columns.max(1);
    #[cfg(feature = "layout")]
    {
        crate::layout::compute(&crate::layout::grid(names, columns, 8.0), rect)
    }
    #[cfg(not(feature = "layout"))]
    {
        let rows = (names.len() as u16).div_ceil(columns).max(1);
        let (w, h) = (
            (rect[2] - 8.0 * f32::from(columns - 1)) / f32::from(columns),
            (rect[3] - 8.0 * f32::from(rows - 1)) / f32::from(rows),
        );
        names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let (c, r) = ((i as u16 % columns) as f32, (i as u16 / columns) as f32);
                (
                    n.to_string(),
                    [rect[0] + c * (w + 8.0), rect[1] + r * (h + 8.0), w, h],
                )
            })
            .collect()
    }
}

/// Timing offsets (ms) shaped like a play's: most near zero, a few late.
fn sample_offsets(n: usize) -> Vec<f32> {
    let mut x: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x >> 40) as f32 / (1u64 << 24) as f32
    };
    (0..n)
        .map(|_| {
            // The sum of four uniforms: a bell, about ±12 ms, shifted late.
            let s: f32 = (0..4).map(|_| next()).sum::<f32>() - 2.0;
            s * 14.0 + 3.0
        })
        .collect()
}

impl Screen for WidgetScreen {
    fn cursor(&self, target: &crate::hit::Target) -> Option<crate::request::Cursor> {
        self.search.cursor(target)
    }

    fn name(&self) -> &str {
        "widgets"
    }

    fn build(&mut self, _cx: &mut Cx) -> Actor {
        let look: &dyn Look = &self.look;
        let mut status = look::text("WStatus", "Widgets", 14.0, look::color(look, "Muted"));
        call(&mut status, "xy", &[416.0, 526.0]);
        call(&mut status, "halign", &[0.0]);
        // The screen's ground: the developer screen under it stays drawn.
        let mut ground = look::quad("", [4000.0, 4000.0], look::color(look, "Background"));
        call(&mut ground, "xy", &[480.0, 270.0]);
        let children = vec![
            ground,
            self.list.build(look),
            self.style.build(look),
            self.speed.build(look),
            self.panels.build(look),
            self.reverse.build(look),
            self.volume.build(look),
            self.panels_actor(),
            status,
            // Last: its popup lies over the rest.
            self.search.build(look),
        ];
        Actor::new(Kind::Frame(children))
    }

    fn focusables(&self) -> Vec<Focusable> {
        let mut songs = Focusable::new_list("Songs");
        songs.right = Some("Search".into());
        let mut search = Focusable::new("Search");
        search.left = Some("Songs".into());
        vec![
            songs,
            search,
            Focusable::new("Style"),
            Focusable::new("Speed"),
            Focusable::new("Panels"),
            Focusable::new("Reverse"),
            Focusable::new("Volume"),
        ]
    }

    fn animating(&self) -> bool {
        self.list.animating()
    }

    fn update(&mut self, dt: f32, root: &mut Actor, cx: &mut Cx) {
        let look = self.look;
        if let Some(e) = self.list.update(dt, root, &self.songs, &look, cx) {
            self.list_event(e, root);
        }
        self.publish();
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        let focused = cx.focused().map(str::to_string);
        let is = |n: &str| focused.as_deref() == Some(n);
        // The field first: while typing, keys are its.
        let (suggester, fold, songs) = (&self.suggester, &self.fold, &self.songs);
        let suggest = |t: &str| {
            let mut built = suggester.borrow_mut();
            let s = built.get_or_insert_with(|| songs.suggester(fold));
            suggest_with(s, fold, t)
        };
        let (h, e) = self.search.input(input, root, cx, &suggest);
        if let Some(FieldEvent::Submitted(text)) = e {
            self.state.borrow_mut().submitted.push(text.clone());
            cx.end_text();
            self.find(&text, root);
        }
        if h == Handled::Yes {
            self.publish();
            return h;
        }
        // The list: pointers and the wheel on it; menu actions while it
        // has focus.
        let for_list = !matches!(input, Input::Menu { .. }) || is("Songs");
        if for_list {
            let (h, e) = self.list.input(input, root, &self.songs);
            if let Some(e) = e {
                self.list_event(e, root);
            }
            if h == Handled::Yes {
                if matches!(input, Input::Pointer { .. }) && !is("Songs") {
                    cx.focus("Songs");
                }
                self.publish();
                return h;
            }
        }
        let mut handled = Handled::No;
        let (h, changed) = self.style.input(input, root, is("Style"));
        if let Some(i) = changed {
            self.status(root, format!("style {}", self.style.choices[i]));
        }
        handled = if h == Handled::Yes { h } else { handled };
        let (h, changed) = self.speed.input(input, root, is("Speed"));
        if let Some(i) = changed {
            self.status(root, format!("speed {}", self.speed.choices[i]));
        }
        handled = if h == Handled::Yes { h } else { handled };
        let (h, changed) = self.panels.input(input, root, is("Panels"));
        if changed.is_some() {
            self.rebuild_panels(root);
            self.status(
                root,
                format!("panels {}", self.panels.choices[self.panels.index]),
            );
        }
        handled = if h == Handled::Yes { h } else { handled };
        if let Some(on) = self.reverse.input(input, root) {
            self.status(root, format!("reverse {}", if on { "on" } else { "off" }));
            handled = Handled::Yes;
        }
        let (h, changed) = self.volume.input(input, root, is("Volume"));
        if let Some(v) = changed {
            self.status(root, format!("volume {:.0} %", v * 100.0));
        }
        handled = if h == Handled::Yes { h } else { handled };
        self.publish();
        handled
    }

    fn message(&mut self, m: &Message, root: &mut Actor, cx: &mut Cx) {
        // A suggestion tapped in the popup.
        if let Some(FieldEvent::Submitted(text)) = self.search.message(m, root, cx) {
            self.state.borrow_mut().submitted.push(text.clone());
            cx.end_text();
            self.find(&text, root);
            self.publish();
        }
        if m.name == LIBRARY_MESSAGE {
            self.songs = Songs::new(&self.state.borrow().library);
            self.suggester = RefCell::new(None);
            if let Some(e) = self.list.reload(&self.songs) {
                self.list_event(e, root);
            }
            self.status(root, format!("{} songs", self.songs.rows()));
            self.publish();
        }
    }
}
