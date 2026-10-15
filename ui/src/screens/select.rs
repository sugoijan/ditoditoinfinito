//! Song select, layout A of `docs/plans/theme.md` (the wheel):
//!
//! - **Left**, the selected song: banner, title, artist, BPM, length and
//!   pack (`SongInfo`); the style tabs, lit where the song has charts,
//!   with how many (`StyleTab`); the chart ladder for that style
//!   (`ChartLadder.Row`), or a message where it has none
//!   (`ChartLadder.Empty`).
//! - **Right**, the wheel: every song in every style, under pack headers
//!   that open and close; a song with no chart in the style is greyed
//!   (`Unavailable`) and can still be selected.
//! - **Header**: the screen's name, the search field (suggestions drop
//!   under it; the wheel shows the matches), what is in effect.
//!   **Footer**: what the keys do now, and links.
//! - **Keys**: Up and Down move through songs, Left and Right through
//!   charts, Tab (the menu's Select) to the next lit style, Start plays
//!   (or opens and closes a pack), `/` searches, Back clears the search
//!   or closes the pack.
//!
//! Changing style keeps the chart's difficulty, or the closest
//! ([`super::data::closest`]); what the player picked is kept as their
//! preferred difficulty, as StepMania keeps it.
//!
//! A small window (below `Compact.Below` CSS pixels high) gets the
//! compact layout of the same screen: the wheel and the selected song
//! kept, the charts as buttons, a large Play button, icon buttons for
//! back, search and options, no key hints, 48 px kept clear for a
//! notch.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use ddi_actor::{Actor, Kind};
use ddi_chart::Difficulty;
use ddi_library::suggest::{Kind as SuggestKind, Suggester};

use super::data::{ChartEntry, SelectData, SongEntry, closest, difficulty_name};
use super::{Backdrop, Env, Pulse, extent, footer, frame, ground, header, left, light, right};
use crate::hit;
use crate::look::{self, Spec, call, quad};
use crate::message::Message;
use crate::nav::{Focusable, MenuAction};
use crate::request::{EnterKey, InputHints, InputMode};
use crate::screen::{Cx, Handled, Input, Screen, TextChange};
use crate::widgets::field::{FieldEvent, SearchField};
use crate::widgets::list::{ItemKind, List, ListConfig, ListEvent, ListModel, ListStyle};
use crate::widgets::{part, set_image, set_text};

/// What Start does on a song with a chart (step 6: start play; the
/// developer routes: show the sample results).
pub type PlayFn = Rc<dyn Fn(&SongEntry, &ChartEntry, &Env, &mut Cx)>;
/// What the Options link does.
pub type OpenFn = Rc<dyn Fn(&Env, &mut Cx)>;

/// Ladder rows at most.
const LADDER_ROWS: usize = 8;
/// The compact layout's left side, from its top (`Phone.dc.html`):
/// banner 94, the title and a line, then the style button (36), the
/// chart buttons (44) and Play (48), 8 apart.
const COMPACT_STYLE_Y: f32 = 165.0;
const COMPACT_CHARTS_Y: f32 = 213.0;
const COMPACT_PLAY_Y: f32 = 267.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Item {
    Header(usize),
    Song(usize),
}

/// What the footer says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hints {
    Song,
    Greyed,
    Header,
    Typing,
}

/// Where everything goes, from the look's metrics and the screen's size.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Layout {
    screen: [f32; 2],
    compact: bool,
    song: [f32; 3],
    banner: f32,
    tabs_y: f32,
    ladder: [f32; 2],
    wheel: [f32; 4],
    row: f32,
    field: [f32; 4],
}

impl Layout {
    fn new(env: &Env, screen: [f32; 2], compact: bool) -> Layout {
        let m = |n: &str, d: f32| env.metric(n, d);
        if compact {
            let margin = m("Compact.Margin", 48.0);
            let right = screen[0] - margin;
            return Layout {
                screen,
                compact,
                song: [margin, 60.0, 300.0],
                banner: 94.0,
                tabs_y: 0.0,
                ladder: [0.0, 0.0],
                wheel: [
                    364.0_f32.max(margin + 316.0),
                    56.0,
                    right - 364.0_f32.max(margin + 316.0),
                    screen[1] - 56.0,
                ],
                row: 48.0,
                field: [margin + 52.0, 6.0, right - margin - 52.0 - 104.0, 44.0],
            };
        }
        let design_w = m("Design.Width", 1280.0);
        let header = m("Header.Height", 64.0);
        let footer = m("Footer.Height", 40.0);
        let wheel_x = screen[0] - (design_w - m("Select.Wheel.X", 640.0));
        let ladder_y = m("Select.Ladder.Y", 421.0);
        Layout {
            screen,
            compact,
            song: [
                m("Select.Song.X", 32.0),
                m("Select.Song.Y", 84.0),
                m("Select.Song.Width", 560.0),
            ],
            banner: m("Select.Banner.Height", 175.0),
            tabs_y: m("Select.Tabs.Y", 377.0),
            ladder: [ladder_y, screen[1] - footer - 14.0 - ladder_y],
            wheel: [
                wheel_x,
                header,
                m("Select.Wheel.Width", 596.0) + 5.0,
                screen[1] - header - footer,
            ],
            row: m("Select.Wheel.Row", 66.0),
            field: [
                m("Select.Search.X", 280.0),
                (header - 42.0) / 2.0,
                m("Select.Search.Width", 560.0),
                42.0,
            ],
        }
    }
}

/// The wheel's items for the list.
struct Wheel<'a> {
    items: &'a [Item],
    data: &'a SelectData,
    packs: &'a [(String, Vec<usize>)],
    closed: &'a BTreeSet<usize>,
    style: &'a str,
    style_name: &'a str,
    banner_side: f32,
}

impl ListModel for Wheel<'_> {
    fn len(&self) -> usize {
        self.items.len()
    }

    fn kind(&self, i: usize) -> ItemKind {
        match self.items[i] {
            Item::Header(_) => ItemKind::Header,
            Item::Song(_) => ItemKind::Row,
        }
    }

    fn key(&self, i: usize) -> Option<u64> {
        Some(match self.items[i] {
            Item::Header(p) => (1 << 40) | p as u64,
            Item::Song(s) => s as u64,
        })
    }

    fn fill(&self, i: usize, item: &mut Actor, cx: &mut Cx) {
        match self.items[i] {
            Item::Header(p) => {
                let (name, songs) = &self.packs[p];
                let open = !self.closed.contains(&p);
                set_text(item, "Chevron", if open { "▾" } else { "▸" });
                set_text(item, "Title", name);
                let n = songs.len();
                set_text(
                    item,
                    "Count",
                    &format!("{n} song{}", if n == 1 { "" } else { "s" }),
                );
            }
            Item::Song(s) => {
                let song = &self.data.songs[s];
                let title = if song.subtitle.is_empty() {
                    song.title.clone()
                } else {
                    format!("{} {}", song.title, song.subtitle)
                };
                set_text(item, "Title", &title);
                set_text(item, "Subtitle", &song.artist);
                let banner = song
                    .banner
                    .as_deref()
                    .map(|b| cx.texture(b, self.banner_side));
                set_image(item, "Banner", banner);
                let charts = song.charts_in(self.style);
                for (k, d) in [
                    Difficulty::Beginner,
                    Difficulty::Easy,
                    Difficulty::Medium,
                    Difficulty::Hard,
                    Difficulty::Challenge,
                ]
                .into_iter()
                .enumerate()
                {
                    let meter = charts.iter().find(|c| c.difficulty == d).map(|c| c.meter);
                    set_text(
                        item,
                        &format!("Meter{k}"),
                        &meter.map(|m| m.to_string()).unwrap_or_default(),
                    );
                    if let Some(c) = part(item, &format!("Cell{k}")) {
                        crate::widgets::play(c, if meter.is_some() { "Full" } else { "Empty" });
                    }
                }
                set_text(item, "NoCharts", &format!("no {} charts", self.style_name));
                crate::widgets::play(
                    item,
                    if charts.is_empty() {
                        "Unavailable"
                    } else {
                        "Available"
                    },
                );
            }
        }
    }
}

/// Song select.
pub struct SelectScreen {
    env: Env,
    data: Rc<SelectData>,
    packs: Vec<(String, Vec<usize>)>,
    /// Each song's search text, folded.
    haystacks: Vec<String>,
    style: usize,
    preferred: Difficulty,
    /// The ladder's chart picked for the song shown (by Left and Right).
    pick: Option<usize>,
    closed: BTreeSet<usize>,
    matches: Option<Vec<bool>>,
    items: Vec<Item>,
    list: List,
    search: SearchField,
    suggester: RefCell<Option<Suggester>>,
    backdrop: Backdrop,
    pulse: Pulse,
    lit: Option<String>,
    layout: Layout,
    /// The footer's hints, and whether a search was in them.
    hints: Option<(Hints, bool)>,
    /// The item whose song the left side shows.
    shown: Option<Item>,
    /// The left side is to be filled again (a new song, a new tree).
    left_dirty: bool,
    /// The ladder's window as drawn: its first chart and how many.
    ladder: (usize, usize),
    /// What the wheel is filtered by.
    filtered: String,
    /// Something the debug view shows changed.
    publish_dirty: bool,
    typing: bool,
    dimmed: bool,
    reduced: bool,
    play: PlayFn,
    options: OpenFn,
}

impl SelectScreen {
    /// Song select over `data`, with the song `start` selected (else the
    /// first).
    pub fn new(
        env: Env,
        data: SelectData,
        start: Option<&str>,
        play: PlayFn,
        options: OpenFn,
    ) -> SelectScreen {
        let mut packs: Vec<(String, Vec<usize>)> = Vec::new();
        for (i, s) in data.songs.iter().enumerate() {
            match packs.iter_mut().find(|(p, _)| *p == s.pack) {
                Some((_, v)) => v.push(i),
                None => packs.push((s.pack.clone(), vec![i])),
            }
        }
        let f = |s: &str| (env.fold)(s);
        let haystacks = data
            .songs
            .iter()
            .map(|s| {
                let credits: Vec<&str> = s.charts.iter().map(|c| c.credit.as_str()).collect();
                ddi_library::search::haystack(
                    [
                        s.title.as_str(),
                        s.subtitle.as_str(),
                        s.artist.as_str(),
                        s.pack.as_str(),
                    ]
                    .into_iter()
                    .chain(credits),
                    [],
                    &f,
                )
            })
            .collect();
        let layout = Layout::new(&env, super::DESIGN, false);
        let mut search = SearchField::new(
            "Search",
            "Search  /",
            [
                layout.field[0] + layout.field[2] / 2.0,
                layout.field[1] + layout.field[3] / 2.0,
            ],
            [layout.field[2], layout.field[3]],
        );
        search.look = env.look.clone();
        search.hints = InputHints {
            mode: InputMode::Search,
            enter: EnterKey::Search,
            max_chars: Some(80),
        };
        let preferred = data.preferred.unwrap_or(Difficulty::Hard);
        let data = Rc::new(data);
        let mut s = SelectScreen {
            list: List::new(wheel_config(&layout), &EmptyModel),
            env,
            data,
            packs,
            haystacks,
            style: 0,
            preferred,
            pick: None,
            closed: BTreeSet::new(),
            matches: None,
            items: Vec::new(),
            search,
            suggester: RefCell::new(None),
            backdrop: Backdrop::new(),
            pulse: Pulse::default(),
            lit: None,
            layout,
            hints: None,
            shown: None,
            left_dirty: true,
            ladder: (0, 0),
            filtered: String::new(),
            publish_dirty: true,
            typing: false,
            dimmed: false,
            reduced: false,
            play,
            options,
        };
        s.lay_out_items();
        s.list = List::new(wheel_config(&s.layout), &s.model());
        let at = start
            .and_then(|id| s.data.songs.iter().position(|x| x.id == id))
            .and_then(|i| s.items.iter().position(|it| *it == Item::Song(i)))
            .or_else(|| s.items.iter().position(|it| matches!(it, Item::Song(_))));
        if let Some(at) = at {
            s.list.select(at);
            s.list.finish();
        }
        s
    }

    fn model(&self) -> Wheel<'_> {
        wheel(
            &self.items,
            &self.data,
            &self.packs,
            &self.closed,
            self.style,
            self.layout.compact,
        )
    }

    fn lay_out_items(&mut self) {
        self.items.clear();
        for (p, (_, songs)) in self.packs.iter().enumerate() {
            let shown: Vec<usize> = songs
                .iter()
                .copied()
                .filter(|&s| self.matches.as_ref().is_none_or(|m| m[s]))
                .collect();
            if shown.is_empty() {
                continue;
            }
            self.items.push(Item::Header(p));
            // While searching every pack shows its matches.
            if self.matches.is_some() || !self.closed.contains(&p) {
                self.items.extend(shown.into_iter().map(Item::Song));
            }
        }
    }

    fn reload(&mut self) {
        let model = wheel(
            &self.items,
            &self.data,
            &self.packs,
            &self.closed,
            self.style,
            self.layout.compact,
        );
        self.list.reload(&model);
    }

    fn selected_item(&self) -> Option<Item> {
        self.list
            .selected()
            .and_then(|i| self.items.get(i).copied())
    }

    fn song(&self) -> Option<&SongEntry> {
        match self.shown? {
            Item::Song(s) => self.data.songs.get(s),
            Item::Header(_) => None,
        }
    }

    fn style_id(&self) -> &str {
        self.data
            .styles
            .get(self.style)
            .map_or("", |s| s.id.as_str())
    }

    fn style_name(&self) -> &str {
        self.data
            .styles
            .get(self.style)
            .map_or("", |s| s.name.as_str())
    }

    /// The song's charts in the style, and the one picked.
    fn charts(&self) -> (Vec<&ChartEntry>, Option<usize>) {
        let Some(song) = self.song() else {
            return (Vec::new(), None);
        };
        let charts = song.charts_in(self.style_id());
        let pick = self
            .pick
            .filter(|&p| p < charts.len())
            .or_else(|| closest(&charts, self.preferred));
        (charts, pick)
    }

    // Building.

    fn build_tree(&mut self, cx: &mut Cx) -> Actor {
        let env = self.env.clone();
        let l = self.layout;
        let screen = l.screen;
        let mut children = vec![ground(&env, screen), self.backdrop.build(&env, screen)];
        // The wheel.
        self.list.config = wheel_config(&l);
        self.list.config.glide = !self.reduced;
        self.list.reset_actors();
        children.push(self.list.build(&*env.look));
        if !l.compact {
            let [x, y, _, h] = l.wheel;
            let bar_x = screen[0] - 10.0 - 1.5;
            let mut track = quad("Wheel.Track", [3.0, h - 24.0], env.color("Track"));
            call(&mut track, "xy", &[bar_x, y + h / 2.0]);
            let mut thumb = quad("Wheel.Thumb", [3.0, 40.0], env.color("Thumb"));
            call(&mut thumb, "xy", &[bar_x, y + 12.0]);
            let _ = x;
            children.extend([track, thumb]);
        }
        // The selected song.
        let [sx, sy, sw] = l.song;
        let info_h = if l.compact {
            140.0
        } else {
            l.tabs_y - sy - 12.0
        };
        let info = env.component(
            "SongInfo",
            "Panel",
            &Spec::new([sw, info_h], ""),
            "Info",
            [0.0, 0.0],
        );
        let mut song = vec![info];
        if l.compact {
            let w = sw;
            let mut style = env.component(
                "StyleButton",
                "Button",
                &Spec::new([w, 36.0], ""),
                "StyleButton",
                [w / 2.0, COMPACT_STYLE_Y],
            );
            if let Some(p) = part(&mut style, "Label") {
                call(p, "halign", &[0.0]);
                call(p, "x", &[-w / 2.0 + 12.0]);
            }
            song.push(style);
            song.push(frame("Charts", Vec::new()));
            let play = env.component(
                "PlayButton",
                "Button",
                &Spec::new([w, 48.0], "PLAY"),
                "Play",
                [w / 2.0, COMPACT_PLAY_Y],
            );
            song.push(play);
        } else {
            song.push(frame("Tabs", Vec::new()));
            let mut kbd = env.numbers("TabKey", "TAB", 11.0, "Muted");
            kbd = right(kbd, sw, l.tabs_y - sy + 16.0);
            song.push(kbd);
            song.push(frame("Ladder", Vec::new()));
        }
        let mut song = frame("Song", song);
        call(&mut song, "xy", &[sx, sy]);
        children.push(song);
        // Header, mods, search.
        if l.compact {
            let margin = env.metric("Compact.Margin", 48.0);
            for (name, label, x) in [
                ("Back", "‹", margin + 22.0),
                ("SearchIcon", "/", screen[0] - margin - 74.0),
                ("OptionsIcon", "•••", screen[0] - margin - 22.0),
            ] {
                children.push(env.component(
                    "IconButton",
                    "Button",
                    &Spec::new([44.0, 44.0], label),
                    name,
                    [x, 28.0],
                ));
            }
            children.push(left(
                env.text("PackTitle", "", 16.0, 700, "Text"),
                margin + 56.0,
                28.0,
            ));
        } else {
            children.push(header(&env, screen[0], "SELECT MUSIC"));
            children.push(self.mods(screen));
        }
        let [fx, fy, fw, fh] = l.field;
        self.search.centre = [fx + fw / 2.0, fy + fh / 2.0];
        self.search.size = [fw, fh];
        let mut field = self.search.build(&*env.look);
        if l.compact {
            field.visible = self.typing || !self.search.text.is_empty();
        }
        children.push(field);
        let mut root = Actor::new(Kind::Frame(children));
        super::entrance(&mut root, &env);
        self.hints = None;
        // The same song and chart, drawn again.
        self.left_dirty = true;
        self.publish_dirty = true;
        self.lit = None;
        self.dimmed = false;
        self.backdrop.restore(&mut root, cx, &env);
        root
    }

    /// "Playing with" and the pills, from the header's right.
    fn mods(&self, screen: [f32; 2]) -> Actor {
        let env = &self.env;
        let margin = env.metric("Margin", 32.0);
        let mut x = screen[0] - margin;
        let mut children = Vec::new();
        for (i, m) in self.data.mods.iter().enumerate().rev() {
            let w = look::mono_width(m, 14.0) + 20.0;
            let mut pill =
                look::component_or(&*env.look, "Pill", "Button", &Spec::new([w, 26.0], m));
            pill.name = format!("Mod{i}");
            let w = extent(&mut pill, [w, 26.0])[0];
            call(&mut pill, "xy", &[x - w / 2.0, 32.0]);
            x -= w + 8.0;
            children.push(pill);
        }
        if !self.data.mods.is_empty() {
            children.push(right(
                env.text("PlayingWith", "Playing with", 13.0, 400, "Muted"),
                x,
                32.0,
            ));
        }
        frame("Mods", children)
    }

    // Filling.

    /// The song the selection shows, at once (what Start, Left, Right
    /// and Tab act on); the left side is filled at the next frame.
    fn follow(&mut self) {
        let item = self.selected_item();
        // A header, or a search matching nothing, keeps the song last
        // shown.
        let item = match item {
            Some(Item::Header(_)) | None => self.shown.or(item),
            other => other,
        };
        if item != self.shown {
            self.shown = item;
            self.pick = None;
            self.pulse.restart();
            self.left_dirty = true;
        }
    }

    /// The left side for the song shown, if it changed.
    fn show(&mut self, root: &mut Actor, cx: &mut Cx) {
        self.follow();
        if !std::mem::take(&mut self.left_dirty) {
            return;
        }
        self.publish_dirty = true;
        self.fill_info(root, cx);
        self.fill_tabs(root);
        self.fill_charts(root);
        let source = self.song().and_then(|s| s.backdrop().map(str::to_string));
        self.backdrop
            .set(source.as_deref(), root, cx, &self.env.clone());
    }

    fn fill_info(&self, root: &mut Actor, cx: &mut Cx) {
        let Some(info) = hit::find_mut(root, "Info") else {
            return;
        };
        let Some(song) = self.song() else {
            for p in [
                "Title", "Subtitle", "Artist", "Bpm", "Length", "Pack", "Line",
            ] {
                set_text(info, p, "");
            }
            set_image(info, "Banner", None);
            return;
        };
        set_text(info, "Title", &song.title);
        set_text(info, "Subtitle", &song.subtitle);
        set_text(info, "Artist", &song.artist);
        set_text(info, "Bpm", &song.bpm_text);
        set_text(info, "Length", &song.length_text());
        set_text(info, "Pack", &song.pack);
        set_text(
            info,
            "Line",
            &format!(
                "{} · {} BPM · {}",
                song.artist,
                song.bpm_text,
                song.length_text()
            ),
        );
        let side = if self.layout.compact { 300.0 } else { 560.0 };
        let banner = song.banner.as_deref().map(|b| cx.texture(b, side));
        set_image(info, "Banner", banner);
        if self.layout.compact {
            let pack = song.pack.clone();
            if let Some(t) = hit::find_mut(root, "PackTitle")
                && let Kind::Text(t) = &mut t.kind
            {
                t.set_text(&pack);
            }
        }
    }

    /// Each style's chart count for the song.
    fn counts(&self) -> Vec<usize> {
        self.data
            .styles
            .iter()
            .map(|st| {
                self.song()
                    .map_or(0, |s| s.charts.iter().filter(|c| c.style == st.id).count())
            })
            .collect()
    }

    fn fill_tabs(&self, root: &mut Actor) {
        let env = &self.env;
        let counts = self.counts();
        if self.layout.compact {
            let lit = counts.iter().filter(|&&n| n > 0).count();
            let n = counts.get(self.style).copied().unwrap_or(0);
            let others = lit - usize::from(n > 0);
            let note = match (n, others) {
                (0, 0) => "no charts".to_string(),
                (0, o) => format!(
                    "no charts · in {o} other style{}",
                    if o == 1 { "" } else { "s" }
                ),
                (n, 0) => format!("{n} chart{}", if n == 1 { "" } else { "s" }),
                (n, o) => format!(
                    "{n} chart{} · also in {o} other style{}",
                    if n == 1 { "" } else { "s" },
                    if o == 1 { "" } else { "s" }
                ),
            };
            super::set(
                root,
                "StyleButton",
                "Label",
                &format!("{}  {note}  ▾", self.style_name()),
            );
            return;
        }
        let Some(tabs) = hit::find_mut(root, "Tabs") else {
            return;
        };
        let y = self.layout.tabs_y - self.layout.song[1] + 16.0;
        let mut x = 0.0;
        let mut children = Vec::new();
        for (i, st) in self.data.styles.iter().enumerate() {
            let n = counts[i];
            let label_w = look::mono_width(&st.name, 13.0);
            let count = if n > 0 { n.to_string() } else { String::new() };
            let w = label_w
                + 20.0
                + if n > 0 {
                    look::mono_width(&count, 11.0) + 6.0
                } else {
                    0.0
                };
            let mut tab = look::component_or(
                &*env.look,
                "StyleTab",
                "Button",
                &Spec::new([w, 32.0], &st.name).with_items([count.clone()]),
            );
            tab.name = format!("Tab{i}");
            let w = extent(&mut tab, [w, 32.0])[0];
            call(&mut tab, "xy", &[x + w / 2.0, y]);
            x += w + 6.0;
            let state = if i == self.style {
                "Selected"
            } else if n > 0 {
                "Available"
            } else {
                "Unavailable"
            };
            crate::widgets::play(&mut tab, state);
            tab.finish_tweening();
            children.push(tab);
        }
        tabs.kind = Kind::Frame(children);
    }

    fn fill_charts(&mut self, root: &mut Actor) {
        let env = self.env.clone();
        let (len, pick) = {
            let (c, p) = self.charts();
            (c.len(), p)
        };
        let (first, n, row) = self.ladder_window(len, pick);
        self.ladder = (first, n);
        let (charts, pick) = self.charts();
        let style_name = self.style_name().to_string();
        if self.layout.compact {
            let w = self.layout.song[2];
            let cell = (w - 4.0 * 5.0) / 5.0;
            let mut children = Vec::new();
            for k in 0..n {
                let c = charts[first + k];
                let mut b = look::component_or(
                    &*env.look,
                    "ChartButton",
                    "Button",
                    &Spec::new([cell, 44.0], c.meter.to_string()),
                );
                b.name = format!("Chart{k}");
                set_text(&mut b, "Meter", &c.meter.to_string());
                crate::widgets::play(&mut b, difficulty_name(c.difficulty));
                crate::widgets::play(
                    &mut b,
                    if Some(first + k) == pick {
                        "Select"
                    } else {
                        "Deselect"
                    },
                );
                b.finish_tweening();
                call(
                    &mut b,
                    "xy",
                    &[cell / 2.0 + k as f32 * (cell + 5.0), COMPACT_CHARTS_Y],
                );
                children.push(b);
            }
            if charts.is_empty() {
                let t = env.text(
                    "NoChartsText",
                    &format!("No {style_name} charts for this song."),
                    14.0,
                    600,
                    "Text",
                );
                children.push(left(t, 0.0, COMPACT_CHARTS_Y));
            }
            if let Some(f) = hit::find_mut(root, "Charts") {
                f.kind = Kind::Frame(children);
            }
            let label = match pick.map(|p| charts[p]) {
                Some(c) => format!(
                    "PLAY {} {}",
                    difficulty_name(c.difficulty).to_uppercase(),
                    c.meter
                ),
                None => "NO CHARTS".to_string(),
            };
            super::set(root, "Play", "Label", &label);
            return;
        }
        let w = self.layout.song[2];
        let top = self.layout.ladder[0] - self.layout.song[1];
        let gap = env.metric("Select.Ladder.Gap", 5.0);
        let mut children = Vec::new();
        for k in 0..n {
            let c = charts[first + k];
            let mut r = env.component(
                "ChartLadder.Row",
                "List.Row",
                &Spec::new([w, row], difficulty_name(c.difficulty)),
                &format!("Ladder.Row{k}"),
                [0.0, top + k as f32 * (row + gap)],
            );
            set_text(
                &mut r,
                "Label",
                &difficulty_name(c.difficulty).to_uppercase(),
            );
            set_text(&mut r, "Title", difficulty_name(c.difficulty));
            let notes = format!("{} notes · steps by {}", c.notes, c.credit);
            set_text(&mut r, "Notes", &notes);
            set_text(&mut r, "Subtitle", &notes);
            set_text(&mut r, "Meter", &c.meter.to_string());
            crate::widgets::play(&mut r, difficulty_name(c.difficulty));
            crate::widgets::play(
                &mut r,
                if Some(first + k) == pick {
                    "Select"
                } else {
                    "Deselect"
                },
            );
            r.finish_tweening();
            children.push(r);
        }
        if charts.is_empty() && self.song().is_some() {
            let mut e = env.component(
                "ChartLadder.Empty",
                "Panel",
                &Spec::new([w, 150.0], format!("No {style_name} charts for this song.")),
                "Ladder.Empty",
                [0.0, top],
            );
            set_text(
                &mut e,
                "Title",
                &format!("No {style_name} charts for this song."),
            );
            set_text(
                &mut e,
                "Hint",
                "The lit styles above have charts; Tab moves to the next one.",
            );
            children.push(e);
        }
        if let Some(f) = hit::find_mut(root, "Ladder") {
            f.kind = Kind::Frame(children);
        }
    }

    /// The ladder's window for `len` charts with `pick` chosen: its
    /// first, how many, and their height. Rows shrink (to 32 at least) so
    /// that all fit; more than fit scroll with the pick.
    fn ladder_window(&self, len: usize, pick: Option<usize>) -> (usize, usize, f32) {
        if self.layout.compact {
            let (first, n) = window(len, pick, 5);
            return (first, n, 44.0);
        }
        let height = self.layout.ladder[1];
        let gap = self.env.metric("Select.Ladder.Gap", 5.0);
        let base = self.env.metric("Select.Ladder.Row", 44.0);
        let fits =
            |row: f32| (((height + gap) / (row + gap)).floor() as usize).clamp(1, LADDER_ROWS);
        let row = if len <= fits(base) {
            base
        } else {
            ((height + gap) / len.min(LADDER_ROWS) as f32 - gap)
                .floor()
                .clamp(32.0, base)
        };
        let (first, n) = window(len, pick, fits(row));
        (first, n, row)
    }

    fn hints_now(&self) -> Hints {
        if self.typing {
            return Hints::Typing;
        }
        match self.selected_item() {
            Some(Item::Header(_)) => Hints::Header,
            Some(Item::Song(_)) if self.charts().0.is_empty() => Hints::Greyed,
            _ => Hints::Song,
        }
    }

    fn fill_footer(&mut self, root: &mut Actor) {
        if self.layout.compact {
            self.env.state.borrow_mut().hints.clear();
            return;
        }
        let now = self.hints_now();
        let searching = !self.search.text.is_empty();
        if self.hints == Some((now, searching)) {
            return;
        }
        self.hints = Some((now, searching));
        // Escape clears a search before it closes packs.
        let esc = if searching {
            "Clear search"
        } else {
            "Close pack"
        };
        let links: &[(&str, &str)] = &[
            ("Link.Add", "+ Add songs"),
            ("Link.Options", "Options"),
            ("Link.Calibrate", "Calibrate"),
            ("Link.Credits", "Credits"),
        ];
        let (hints, note): (&[(&[&str], &str)], &str) = match now {
            Hints::Song => (
                &[
                    (&["↑↓"], "Song"),
                    (&["←→"], "Chart"),
                    (&["TAB"], "Style"),
                    (&["ENTER"], "Play"),
                    (&["/"], "Search"),
                    (&["ESC"], esc),
                ],
                "",
            ),
            Hints::Greyed => (
                &[
                    (&["↑↓"], "Song"),
                    (&["TAB"], "Style"),
                    (&["/"], "Search"),
                    (&["ESC"], esc),
                ],
                "",
            ),
            Hints::Header if searching => (
                &[(&["↑↓"], "Song"), (&["/"], "Search"), (&["ESC"], esc)],
                "",
            ),
            Hints::Header => (
                &[
                    (&["↑↓"], "Song"),
                    (&["ENTER"], "Open or close"),
                    (&["/"], "Search"),
                ],
                "",
            ),
            Hints::Typing => (
                &[
                    (&["↑↓"], "Choose"),
                    (&["ENTER"], "Go to"),
                    (&["ESC"], "Close"),
                ],
                "Typing searches titles, artists, packs and step artists; kana and romaji match each other.",
            ),
        };
        let links = if now == Hints::Typing { &[][..] } else { links };
        let (f, texts) = footer(&self.env, self.layout.screen, hints, note, links);
        if let Some(old) = hit::find_mut(root, "Footer") {
            *old = f;
        } else if let Kind::Frame(c) = &mut root.kind {
            c.push(f);
        }
        self.env.state.borrow_mut().hints = texts;
    }

    // Actions.

    fn set_style(&mut self, i: usize, root: &mut Actor) {
        if i >= self.data.styles.len() || i == self.style {
            return;
        }
        self.style = i;
        self.pick = None;
        self.reload();
        self.fill_tabs(root);
        self.fill_charts(root);
        self.status(format!("style {}", self.style_name()));
    }

    /// Tab: the next style the song has charts in (any next one when it
    /// has none).
    fn next_style(&mut self, root: &mut Actor) {
        self.follow();
        let n = self.data.styles.len();
        if n == 0 {
            return;
        }
        let counts = self.counts();
        let next = (1..=n)
            .map(|k| (self.style + k) % n)
            .find(|&i| counts[i] > 0)
            .unwrap_or((self.style + 1) % n);
        self.set_style(next, root);
    }

    fn move_chart(&mut self, by: i32, root: &mut Actor) {
        self.follow();
        let (charts, pick) = self.charts();
        let Some(p) = pick else { return };
        let to = (p as i32 + by).clamp(0, charts.len() as i32 - 1) as usize;
        if to == p {
            return;
        }
        self.preferred = charts[to].difficulty;
        self.pick = Some(to);
        self.fill_charts(root);
    }

    /// The ladder's row (or chart button) `k` as drawn.
    fn pick_chart(&mut self, k: usize, root: &mut Actor, cx: &mut Cx) {
        self.follow();
        if self.left_dirty {
            // Another song since the ladder was drawn: not what was meant.
            return;
        }
        let (charts, pick) = self.charts();
        let (first, n) = self.ladder;
        let to = first + k;
        if k >= n || to >= charts.len() {
            return;
        }
        if Some(to) == pick {
            self.start(root, cx);
            return;
        }
        self.preferred = charts[to].difficulty;
        self.pick = Some(to);
        self.fill_charts(root);
    }

    fn toggle_pack(&mut self, p: usize) {
        if !self.closed.remove(&p) {
            self.closed.insert(p);
        }
        self.lay_out_items();
        self.reload();
        let at = self.items.iter().position(|it| *it == Item::Header(p));
        if let Some(at) = at {
            self.list.select(at);
        }
        let name = self.packs[p].0.clone();
        let open = !self.closed.contains(&p);
        self.status(format!(
            "pack {name} {}",
            if open { "opened" } else { "closed" }
        ));
    }

    /// Start: plays the chart, or opens or closes the pack (not while
    /// searching: every pack shows its matches then).
    fn start(&mut self, root: &mut Actor, cx: &mut Cx) {
        self.follow();
        match self.selected_item() {
            Some(Item::Header(_)) if self.matches.is_some() => {
                self.status("packs stay open while searching".into());
            }
            Some(Item::Header(p)) => self.toggle_pack(p),
            Some(Item::Song(s)) => {
                let (charts, pick) = self.charts();
                match pick.and_then(|p| charts.get(p).copied()) {
                    Some(c) => {
                        let (song, chart) = (self.data.songs[s].clone(), c.clone());
                        self.status(format!(
                            "play {} {} {}",
                            song.title,
                            difficulty_name(chart.difficulty),
                            chart.meter
                        ));
                        let env = self.env.clone();
                        (self.play)(&song, &chart, &env, cx);
                    }
                    None => self.status(format!("no {} charts", self.style_name())),
                }
            }
            None => {}
        }
        let _ = root;
    }

    /// Back: clears the search, else closes the selected song's pack.
    fn back(&mut self, root: &mut Actor, cx: &mut Cx) -> Handled {
        if !self.search.text.is_empty() {
            self.clear_search(root, cx);
            return Handled::Yes;
        }
        if let Some(Item::Song(s)) = self.selected_item() {
            let pack = &self.data.songs[s].pack;
            if let Some(p) = self.packs.iter().position(|(n, _)| n == pack) {
                self.toggle_pack(p);
                self.follow();
                return Handled::Yes;
            }
        }
        Handled::No
    }

    fn clear_search(&mut self, root: &mut Actor, cx: &mut Cx) {
        self.search.text.clear();
        if self.typing {
            cx.set_text("");
        }
        set_text(root, &self.search.text_actor(), "");
        if let Some(h) = hit::find_mut(root, "Search")
            && let Some(hint) = part(h, "Hint")
        {
            hint.visible = !self.typing;
        }
        self.filter("");
    }

    /// Shows the songs matching `text` (all for none).
    fn filter(&mut self, text: &str) {
        self.filtered = text.to_string();
        self.publish_dirty = true;
        let f = |s: &str| (self.env.fold)(s);
        self.matches = if text.trim().is_empty() {
            None
        } else {
            let q = ddi_library::search::Query::new(text, &f);
            Some(self.haystacks.iter().map(|h| q.matches(h)).collect())
        };
        // Cleared: the song shown comes back, its pack opened if it was
        // closed.
        if self.matches.is_none()
            && let Some(Item::Song(s)) = self.shown
            && let Some(p) = self.packs.iter().position(|(_, v)| v.contains(&s))
        {
            self.closed.remove(&p);
        }
        self.lay_out_items();
        self.reload();
        // Onto the first match; with the search cleared, back on the song
        // last shown.
        let at = if self.matches.is_some() {
            self.items.iter().position(|it| matches!(it, Item::Song(_)))
        } else {
            self.shown
                .and_then(|shown| self.items.iter().position(|it| *it == shown))
        };
        if let Some(at) = at {
            self.list.select(at);
        }
        self.follow();
        let n = self
            .matches
            .as_ref()
            .map_or(self.data.songs.len(), |m| m.iter().filter(|&&x| x).count());
        self.env.state.borrow_mut().matches = n;
    }

    fn begin_search(&mut self, root: &mut Actor, cx: &mut Cx) {
        let activate = Input::Activate {
            name: self.search.name.clone(),
        };
        let suggest = |_: &str| Vec::new();
        self.search.input(&activate, root, cx, &suggest);
        self.typing = true;
        cx.focus(self.search.name.clone());
        if let Some(f) = hit::find_mut(root, "Search") {
            f.visible = true;
        }
    }

    fn status(&self, s: String) {
        self.env.state.borrow_mut().status = s;
    }

    fn field_event(&mut self, e: FieldEvent, root: &mut Actor, cx: &mut Cx) {
        match e {
            FieldEvent::Changed(t) => self.filter(&t),
            FieldEvent::Submitted(t) => {
                // Enter goes to the selected match: the wheel is filtered
                // again only by a suggestion taken.
                if t != self.filtered {
                    self.filter(&t);
                }
                self.typing = false;
                cx.end_text();
                cx.focus("Wheel");
            }
        }
        let _ = root;
    }

    fn publish(&self) {
        let mut st = self.env.state.borrow_mut();
        st.screen = "select".into();
        st.compact = self.layout.compact;
        st.reduced_motion = self.reduced;
        st.style = self.style_name().to_string();
        st.preferred = difficulty_name(self.preferred).to_string();
        let (charts, pick) = self.charts();
        st.chart = pick.map(|p| {
            format!(
                "{} {}",
                difficulty_name(charts[p].difficulty),
                charts[p].meter
            )
        });
        st.ladder = charts
            .iter()
            .map(|c| format!("{} {}", difficulty_name(c.difficulty), c.meter))
            .collect();
        st.greyed = self.song().is_some() && charts.is_empty();
        st.style_counts = self
            .data
            .styles
            .iter()
            .zip(self.counts())
            .map(|(s, n)| (s.name.clone(), n))
            .collect();
        match self.selected_item() {
            Some(Item::Header(p)) => {
                st.header = Some(self.packs[p].0.clone());
            }
            _ => st.header = None,
        }
        let song = self.song();
        st.song = song.map(|s| s.title.clone()).unwrap_or_default();
        st.song_id = song.map(|s| s.id.clone()).unwrap_or_default();
        st.items = self.items.len();
        st.closed = self
            .closed
            .iter()
            .map(|&p| self.packs[p].0.clone())
            .collect();
        let style = self.style_id();
        st.greyed_rows = self
            .list
            .shown()
            .iter()
            .filter(|&&i| matches!(self.items.get(i), Some(Item::Song(s)) if self.data.songs[*s].charts_in(style).is_empty()))
            .count();
        st.search = self.search.text.clone();
        st.popup = self.search.popup_open();
        st.suggestions = self
            .search
            .suggestions
            .iter()
            .map(|(t, _)| t.clone())
            .collect();
        st.pulse = self.pulse.value;
        st.pulsing = self.pulse.on;
        st.backdrop = self.backdrop.source().map(str::to_string);
    }
}

/// Suggestions for what is typed: titles, artists and packs (the
/// suggester built at the first keystroke).
fn suggestions(
    built: &RefCell<Option<Suggester>>,
    fold: &crate::dev_widgets::FoldFn,
    packs: &[(String, Vec<usize>)],
    data: &SelectData,
    typed: &str,
) -> Vec<(String, String)> {
    let f = |s: &str| fold(s);
    let mut built = built.borrow_mut();
    let s = built.get_or_insert_with(|| {
        let mut cands: Vec<(&str, &str, SuggestKind)> = Vec::new();
        for (p, _) in packs {
            cands.push((p, "", SuggestKind::Pack));
        }
        for s in &data.songs {
            cands.push((&s.title, "", SuggestKind::Title));
            cands.push((&s.artist, "", SuggestKind::Artist));
        }
        Suggester::new(cands, &f)
    });
    s.suggest(typed, &f, 6)
        .into_iter()
        .map(|s| {
            let kind = match s.kind {
                SuggestKind::Title => "TITLE",
                SuggestKind::Artist => "ARTIST",
                SuggestKind::Pack => "PACK",
            };
            (s.text, kind.to_string())
        })
        .collect()
}

/// The wheel's model over the screen's fields (borrowed apart from its
/// list).
fn wheel<'a>(
    items: &'a [Item],
    data: &'a SelectData,
    packs: &'a [(String, Vec<usize>)],
    closed: &'a BTreeSet<usize>,
    style: usize,
    compact: bool,
) -> Wheel<'a> {
    let st = data.styles.get(style);
    Wheel {
        items,
        data,
        packs,
        closed,
        style: st.map_or("", |s| s.id.as_str()),
        style_name: st.map_or("", |s| s.name.as_str()),
        banner_side: if compact { 96.0 } else { 224.0 },
    }
}

/// `n` items from the one that keeps `pick` in view: (first, count).
fn window(len: usize, pick: Option<usize>, fits: usize) -> (usize, usize) {
    if len <= fits {
        return (0, len);
    }
    let p = pick.unwrap_or(0);
    let first = p.saturating_sub(fits / 2).min(len - fits);
    (first, fits)
}

fn wheel_config(l: &Layout) -> ListConfig {
    let mut c = ListConfig::new("Wheel", l.wheel);
    c.component = "SongList".into();
    c.row = l.row;
    c.header = l.row;
    c.bar = 3.0;
    c.margin = l.row * 2.0;
    c.style = if l.compact {
        ListStyle::Plain
    } else {
        ListStyle::Wheel { focus: 0.5 }
    };
    if l.compact {
        c.name = "Wheel".into();
    }
    c
}

/// No items (before the first layout).
struct EmptyModel;

impl ListModel for EmptyModel {
    fn len(&self) -> usize {
        0
    }

    fn kind(&self, _i: usize) -> ItemKind {
        ItemKind::Row
    }

    fn fill(&self, _i: usize, _item: &mut Actor, _cx: &mut Cx) {}
}

impl Screen for SelectScreen {
    fn name(&self) -> &str {
        "select"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        let compact = super::is_compact(&*self.env.look, vp);
        self.reduced = self.env.reduced();
        self.layout = Layout::new(&self.env, [vp.width, vp.height], compact);
        self.build_tree(cx)
    }

    fn focusables(&self) -> Vec<Focusable> {
        let mut v = vec![
            Focusable::new("Wheel"),
            Focusable::new(self.search.name.clone()),
        ];
        if self.layout.compact {
            for n in ["Back", "SearchIcon", "OptionsIcon", "StyleButton", "Play"] {
                v.push(Focusable::new(n));
            }
            v.extend((0..5).map(|k| Focusable::new(format!("Chart{k}"))));
        } else {
            for n in ["Link.Add", "Link.Options", "Link.Calibrate", "Link.Credits"] {
                v.push(Focusable::new(n));
            }
            v.extend((0..self.data.styles.len()).map(|i| Focusable::new(format!("Tab{i}"))));
            v.extend((0..LADDER_ROWS).map(|k| Focusable::new(format!("Ladder.Row{k}"))));
        }
        v
    }

    fn animating(&self) -> bool {
        self.list.animating() || self.pulse.on
    }

    fn update(&mut self, dt: f32, root: &mut Actor, cx: &mut Cx) {
        // A new size (a window resized or turned, a new design), or
        // reduced motion changed: laid out again.
        let vp = cx.viewport().screen;
        let compact = super::is_compact(&*self.env.look, vp);
        if [vp.width, vp.height] != self.layout.screen
            || compact != self.layout.compact
            || self.env.reduced() != self.reduced
        {
            self.reduced = self.env.reduced();
            self.layout = Layout::new(&self.env, [vp.width, vp.height], compact);
            let selected = self.list.selected();
            *root = self.build_tree(cx);
            self.reload();
            if let Some(s) = selected {
                self.list.select(s);
                self.list.finish();
            }
            // The first frame of the new tree shows it as it is.
            root.finish_tweening();
        }
        let event = {
            let look = self.env.look.clone();
            let model = wheel(
                &self.items,
                &self.data,
                &self.packs,
                &self.closed,
                self.style,
                self.layout.compact,
            );
            self.list.update(dt, root, &model, &*look, cx)
        };
        if let Some(ListEvent::Activated(_)) = event {
            self.start(root, cx);
        }
        self.show(root, cx);
        self.fill_footer(root);
        // The scrollbar: where the selection is in the list.
        if let Some(sel) = self.list.selected() {
            let n = self.items.len().max(1);
            let [_, y, _, h] = self.layout.wheel;
            let track = h - 24.0;
            let thumb = (track * 6.0 / n as f32).clamp(24.0, track);
            if let Some(t) = hit::find_mut(root, "Wheel.Thumb") {
                if let Kind::Sprite(s) = &mut t.kind {
                    s.size = [3.0, thumb];
                }
                let at = if n > 1 {
                    sel as f32 / (n - 1) as f32
                } else {
                    0.0
                };
                call(t, "y", &[y + 12.0 + thumb / 2.0 + at * (track - thumb)]);
            }
        }
        // The beat pulse on the selected row and chart.
        let (id, bpm) = self
            .song()
            .map_or((String::new(), 0.0), |s| (s.id.clone(), s.bpm));
        let v = self.pulse.update(dt, &self.env, &id, bpm);
        let row = self.list.selected().and_then(|i| self.list.item_actor(i));
        light(root, row.as_deref(), v, &mut self.lit);
        let (_, pick) = self.charts();
        let (first, n) = self.ladder;
        if let Some(p) = pick.filter(|&p| p >= first && p < first + n) {
            let name = if self.layout.compact {
                format!("Chart{}", p - first)
            } else {
                format!("Ladder.Row{}", p - first)
            };
            if let Some(a) = hit::find_mut(root, &name)
                && let Some(pp) = part(a, "Pulse")
            {
                call(pp, "diffusealpha", &[v]);
            }
        }
        // Dimmed while typing.
        if self.dimmed != self.typing
            && let Some(s) = hit::find_mut(root, "Song")
        {
            self.dimmed = self.typing;
            let to = if self.typing { 0.45 } else { 1.0 };
            look::run(s, &format!("stoptweening;linear,0.12;diffusealpha,{to}"));
        }
        // Back on top of a screen that said what it did: say it again.
        let st = self.env.state.borrow();
        let back_on_top = st.over == 0 && st.screen != "select";
        drop(st);
        if back_on_top {
            self.publish_dirty = true;
        }
        if std::mem::take(&mut self.publish_dirty) || event.is_some() || self.list.animating() {
            self.publish();
        } else {
            let mut st = self.env.state.borrow_mut();
            st.pulse = self.pulse.value;
            st.pulsing = self.pulse.on;
        }
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        let h = self.input_inner(input, root, cx);
        // The song the selection shows, now: a Start in the same poll as a
        // move acts on the song moved to.
        self.follow();
        self.sync_field(root);
        self.publish_dirty = true;
        self.publish();
        h
    }

    fn message(&mut self, m: &Message, root: &mut Actor, cx: &mut Cx) {
        if let Some(e) = self.search.message(m, root, cx) {
            self.field_event(e, root, cx);
        }
        self.follow();
        self.sync_field(root);
        self.publish();
    }
}

impl SelectScreen {
    /// On a phone the field shows only while typed into or holding text.
    fn sync_field(&self, root: &mut Actor) {
        if self.layout.compact
            && let Some(f) = hit::find_mut(root, &self.search.name)
        {
            f.visible = self.typing || !self.search.text.is_empty();
        }
    }

    fn input_inner(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        // The field first: while typing, keys are its.
        let popup_before = self.search.popup_open();
        let (h, e) = {
            let (built, fold, packs, data) =
                (&self.suggester, &self.env.fold, &self.packs, &self.data);
            let suggest = |t: &str| suggestions(built, fold, packs, data, t);
            self.search.input(input, root, cx, &suggest)
        };
        match input {
            Input::Activate { name } if *name == self.search.name => self.typing = true,
            Input::Text { change, .. } => match change {
                TextChange::Blurred => self.typing = false,
                // The first Escape closes the popup, the next stops
                // typing (the field's).
                TextChange::Cancel if !popup_before => self.typing = false,
                _ => {}
            },
            _ => {}
        }
        if let Some(e) = e {
            self.field_event(e, root, cx);
        }
        if h == Handled::Yes {
            self.publish();
            return h;
        }
        match input {
            Input::Key(k) if k.down && !k.composing && k.key == "/" && !self.typing => {
                self.begin_search(root, cx);
                return Handled::Yes;
            }
            Input::Key(k)
                if k.down && self.typing && (k.key == "ArrowUp" || k.key == "ArrowDown") =>
            {
                let action = if k.key == "ArrowUp" {
                    MenuAction::Up
                } else {
                    MenuAction::Down
                };
                let model = wheel(
                    &self.items,
                    &self.data,
                    &self.packs,
                    &self.closed,
                    self.style,
                    self.layout.compact,
                );
                self.list.input(
                    &Input::Menu {
                        action,
                        repeat: k.repeat,
                    },
                    root,
                    &model,
                );
                return Handled::Yes;
            }
            Input::Activate { name } => {
                let name = name.clone();
                return self.activate(&name, root, cx);
            }
            _ => {}
        }
        // The wheel: pointers and the wheel on it, and the menu's Up and
        // Down whatever has focus.
        let for_list = match input {
            Input::Menu { action, .. } => matches!(action, MenuAction::Up | MenuAction::Down),
            _ => true,
        };
        if for_list {
            let model = wheel(
                &self.items,
                &self.data,
                &self.packs,
                &self.closed,
                self.style,
                self.layout.compact,
            );
            let (h, e) = self.list.input(input, root, &model);
            if let Some(ListEvent::Activated(_)) = e {
                self.start(root, cx);
            }
            if h == Handled::Yes {
                self.publish();
                return h;
            }
        }
        match input {
            Input::Menu { action, .. } => match action {
                MenuAction::Left => {
                    self.move_chart(-1, root);
                    Handled::Yes
                }
                MenuAction::Right => {
                    self.move_chart(1, root);
                    Handled::Yes
                }
                MenuAction::Select => {
                    self.next_style(root);
                    Handled::Yes
                }
                MenuAction::Start => {
                    let focused = cx.focused().unwrap_or("Wheel");
                    if focused == "Wheel" || focused == self.search.name {
                        self.start(root, cx);
                        Handled::Yes
                    } else {
                        // The focused button's own.
                        Handled::No
                    }
                }
                MenuAction::Back => self.back(root, cx),
                _ => Handled::No,
            },
            _ => Handled::No,
        }
    }

    fn activate(&mut self, name: &str, root: &mut Actor, cx: &mut Cx) -> Handled {
        if let Some(i) = name
            .strip_prefix("Tab")
            .and_then(|n| n.parse::<usize>().ok())
        {
            self.set_style(i, root);
            cx.focus("Wheel");
            return Handled::Yes;
        }
        if let Some(k) = name
            .strip_prefix("Ladder.Row")
            .or_else(|| name.strip_prefix("Chart"))
            .and_then(|n| n.parse::<usize>().ok())
        {
            self.pick_chart(k, root, cx);
            return Handled::Yes;
        }
        match name {
            "Link.Options" | "OptionsIcon" => {
                let env = self.env.clone();
                (self.options)(&env, cx);
            }
            "SearchIcon" => self.begin_search(root, cx),
            "StyleButton" => self.next_style(root),
            "Play" => self.start(root, cx),
            "Back" => {
                if self.back(root, cx) == Handled::No {
                    self.status("back".into());
                }
            }
            "Wheel" => self.start(root, cx),
            other => self.status(format!("{other} (not here yet)")),
        }
        if name != "SearchIcon" {
            cx.focus("Wheel");
        }
        self.publish();
        Handled::Yes
    }
}
