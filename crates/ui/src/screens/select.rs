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
//! - **Keys**: all four arrows move through songs; double-tap the same
//!   arrow to change chart (Up/Left easier, Down/Right harder). Tab
//!   (the menu's Select) goes to the next lit style, Start plays
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
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use ddi_actor::{Actor, Kind};
use ddi_chart::Difficulty;
use ddi_library::suggest::{Kind as SuggestKind, Suggester};

use super::data::{
    ChartEntry, ImportView, Notice, SelectData, SongEntry, closest, difficulty_name,
};
use super::{Backdrop, Env, Pulse, extent, footer, frame, ground, header, left, light, right};
use crate::hit;
use crate::look::{self, Spec, call, quad};
use crate::message::Message;
use crate::nav::{Focusable, MenuAction};
use crate::request::{EnterKey, InputHints, InputMode, UiRequest};
use crate::screen::{Cx, Handled, Input, Screen, TextChange};
use crate::widgets::field::{FieldEvent, SearchField};
use crate::widgets::list::{ItemKind, List, ListConfig, ListEvent, ListModel, ListStyle};
use crate::widgets::{part, set_image, set_text};

use super::data::PackEntry;
use super::select_dialogs::{ImportDialog, Manage, ManageDialog, NoticesDialog};
use super::select_navigation::{Event as NavEvent, Motion, Navigation};

/// What Start does on a song with a chart (step 6: start play; the
/// developer routes: show the sample results).
pub type PlayFn = Rc<dyn Fn(&SongEntry, &ChartEntry, &Env, &mut Cx)>;
/// What the Options link does.
pub type OpenFn = Rc<dyn Fn(&Env, &mut Cx)>;

/// What song select asks the shell. Selection can settle on a frame;
/// gesture-bound requests are made inside the input that asked.
#[derive(Clone, Debug, PartialEq)]
pub enum SelectCall {
    /// Leave song browsing for the start menu.
    Leave,
    /// The song shown changed (its preview starts): `None`, no song.
    /// `from_pad`: the move came from a controller.
    Selected {
        song: Option<String>,
        from_pad: bool,
    },
    /// Where the list is, kept for the session.
    Place(SelectPlace),
    /// A link: `calibrate`, `credits`, `source` (Options is [`OpenFn`]).
    Open(String),
    /// Import a folder (the browser's picker, kept to read again where it
    /// can), or zips and files.
    PickFolder,
    PickFiles,
    /// Import a DanOni work from its URL (desktop capability only).
    ImportDanoni {
        url: String,
        offline: bool,
    },
    /// Answer one pending request for files at an additional address.
    DanoniOriginDecision {
        request_id: u64,
        allow: bool,
        remember: bool,
    },
    /// Request cancellation without waiting for the import worker.
    CancelDanoni,
    /// An import option changed: `copy`, `videos` or `details`.
    ImportOption(String, bool),
    /// Read a pack's folder again.
    Reread(String),
    /// Remove a pack (by name) or a song (by id), confirmed.
    RemovePack(String),
    RemoveSong(String),
    /// A notice's link was taken (its id).
    Notice(String),
    /// Autoplay switched on or off (for the session).
    Autoplay(bool),
    /// A kept play (by id): watch its replay, show its results, or
    /// forget it (`docs/plans/play-history.md`, Replays).
    WatchPlay(String),
    ShowPlay(String),
    RemovePlay(String),
    /// Open the chart editor on a song (by id) at a chart (its index):
    /// E, where the shell offers it (`docs/plans/editor.md`).
    Edit {
        song: String,
        chart: usize,
    },
}

/// The plays kept of a song's chart, as the plays list shows them (the
/// shell's history; none on the developer routes).
pub type PlaysFn = Rc<dyn Fn(&SongEntry, &ChartEntry) -> Vec<super::data::PlayRow>>;

/// The shell's part of song select (none on the developer routes).
pub type ShellFn = Rc<dyn Fn(SelectCall, &mut Cx)>;

/// Where the list is: the song shown, its chart (the play route's
/// index), the style (id), the search, the packs closed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectPlace {
    pub song: Option<String>,
    pub chart: Option<usize>,
    pub style: String,
    pub search: String,
    pub closed: Vec<String>,
    /// The one expanded browsing pack (search ignores this state).
    pub open: Option<String>,
}

/// What the shell sends song select after it started (the library read
/// again, bests worked out, an import running), taken when the message
/// [`FEED`] comes.
#[derive(Default)]
pub struct Feed {
    pub data: Option<SelectData>,
    pub import: Option<ImportView>,
    pub notices: Option<Vec<Notice>>,
    /// Open the import dialog (files were dropped on the page).
    pub open_import: bool,
    /// Open a dialog of the shell's over song select (desktop's first
    /// start offers other games' song folders).
    pub overlay: Option<OverlayFn>,
    /// The song whose preview movie the shell now draws into
    /// [`PREVIEW_MOVIE`] (`Some(None)`: none; the banner again), from
    /// `ddi_session::movies::preview`.
    pub preview_movie: Option<Option<String>>,
}

/// The texture the shell draws the selected song's preview movie
/// (`#PREVIEWVID`) into; shown in the banner's place while
/// [`Feed::preview_movie`] names the song shown
/// (`docs/plans/lane-covers.md`, "Preview video").
pub const PREVIEW_MOVIE: &str = "preview-movie";

/// Opens a shell's dialog over song select ([`Feed::overlay`]).
pub type OverlayFn = Box<dyn FnOnce(&Env, &mut Cx)>;

pub type SharedFeed = Rc<RefCell<Feed>>;

/// The message that says the feed has something.
pub const FEED: &str = "SelectFeed";
/// The message song select sends its dialogs when the import changed.
pub const IMPORT_CHANGED: &str = "ImportChanged";
/// The message the notices dialog sends for a notice's link (`Index`).
pub const NOTICE_LINK: &str = "NoticeLink";

/// Ladder rows at most.
const LADDER_ROWS: usize = 8;
/// From this many songs the wheel groups them by pack, under headers;
/// fewer fit on it as one list.
const PACKS_FROM: usize = 10;
/// Rows a second past which the wheel's room for its selected row closes.
const FLICK_ROWS: f32 = 6.0;
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
    /// The viewport's size it was made for, and the layout's own size
    /// and zoom onto it (a phone upright: 480 wide, zoomed to the width).
    vp: [f32; 2],
    zoom: f32,
    screen: [f32; 2],
    compact: bool,
    /// A phone upright: the song on top, the wheel under it, Play at
    /// the bottom.
    upright: bool,
    /// The compact layouts' side margin.
    margin: f32,
    /// The compact song column's parts, from its top: the info's height,
    /// the style button's, the chart buttons' and Play's centres.
    info_h: f32,
    style_y: f32,
    charts_y: f32,
    play_y: f32,
    /// The chart buttons' and Play's heights.
    chart_h: f32,
    play_h: f32,
    song: [f32; 3],
    banner: f32,
    tabs_y: f32,
    ladder: [f32; 2],
    wheel: [f32; 4],
    row: f32,
    field: [f32; 4],
}

impl Layout {
    /// The layout for viewport `vp`: a phone upright gets its own, 480
    /// wide (`Options.Narrow.Width`) zoomed to the width.
    fn fit(env: &Env, vp: [f32; 2], compact: bool) -> Layout {
        let narrow = env.metric("Options.Narrow.Width", 480.0);
        if compact && vp[1] > vp[0] {
            let z = (vp[0] / narrow).max(1.0);
            let mut l = Layout::upright(env, [vp[0] / z, vp[1] / z]);
            l.vp = vp;
            l.zoom = z;
            return l;
        }
        Layout::new(env, vp, compact)
    }

    /// A phone upright (`screen` about 480 × 1000): the bar on top, the
    /// song's banner, title, style and charts under it, the wheel filling
    /// the middle, Play at the bottom within the thumb's reach.
    fn upright(env: &Env, screen: [f32; 2]) -> Layout {
        let inset = env.metric("Select.Wheel.Inset", 0.0);
        let margin = env.metric("Compact.Upright.Margin", 12.0);
        let w = screen[0] - 2.0 * margin;
        let top = 64.0;
        // The banner as the compact one, wider (the look's proportion).
        let info_h = (w * 94.0 / 300.0).round() + 50.0;
        // Taller buttons than sideways: thumbs, and a narrower phone
        // (360 wide) zooms them down.
        let chart_h = 52.0;
        let style_y = info_h + 22.0;
        let charts_y = style_y + 22.0 + 8.0 + chart_h / 2.0;
        let play_h = 60.0;
        let play = screen[1] - margin - play_h / 2.0;
        // The notices' line above Play, the wheel above it.
        let wheel_top = top + charts_y + chart_h / 2.0 + 10.0;
        let wheel_bottom = play - play_h / 2.0 - 32.0;
        Layout {
            vp: screen,
            zoom: 1.0,
            screen,
            compact: true,
            upright: true,
            margin,
            info_h,
            style_y,
            charts_y,
            play_y: play - top,
            chart_h,
            play_h,
            song: [margin, top, w],
            banner: 94.0,
            tabs_y: 0.0,
            ladder: [0.0, 0.0],
            wheel: [
                margin - inset,
                wheel_top,
                w + inset,
                (wheel_bottom - wheel_top).max(96.0),
            ],
            row: 52.0,
            field: [margin + 52.0, 6.0, w - 52.0 - 52.0, 44.0],
        }
    }

    fn new(env: &Env, screen: [f32; 2], compact: bool) -> Layout {
        let m = |n: &str, d: f32| env.metric(n, d);
        // The look draws the wheel's rows this far in (its selected row's
        // ring stays inside the list's clip): the list starts as far out.
        let inset = m("Select.Wheel.Inset", 0.0);
        if compact {
            let margin = m("Compact.Margin", 48.0);
            let right = screen[0] - margin;
            return Layout {
                vp: screen,
                zoom: 1.0,
                screen,
                compact,
                upright: false,
                margin,
                info_h: 140.0,
                style_y: COMPACT_STYLE_Y,
                charts_y: COMPACT_CHARTS_Y,
                play_y: COMPACT_PLAY_Y,
                chart_h: 44.0,
                play_h: 48.0,
                song: [margin, 60.0, 300.0],
                banner: 94.0,
                tabs_y: 0.0,
                ladder: [0.0, 0.0],
                wheel: [
                    364.0_f32.max(margin + 316.0) - inset,
                    56.0,
                    right - 364.0_f32.max(margin + 316.0) + inset,
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
            vp: screen,
            zoom: 1.0,
            screen,
            compact,
            upright: false,
            margin: m("Margin", 32.0),
            info_h: 0.0,
            style_y: 0.0,
            charts_y: 0.0,
            play_y: 0.0,
            chart_h: 0.0,
            play_h: 0.0,
            song: [
                m("Select.Song.X", 32.0),
                m("Select.Song.Y", 84.0),
                m("Select.Song.Width", 560.0),
            ],
            banner: m("Select.Banner.Height", 175.0),
            tabs_y: m("Select.Tabs.Y", 377.0),
            ladder: [ladder_y, screen[1] - footer - 14.0 - ladder_y],
            wheel: [
                wheel_x - inset,
                header,
                m("Select.Wheel.Width", 596.0) + 5.0 + inset,
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
    /// The songs a search matches (none: no search).
    matches: Option<&'a [bool]>,
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
                let open = self.matches.is_some() || !self.closed.contains(&p);
                set_text(item, "Chevron", if open { "▾" } else { "▸" });
                set_text(item, "Title", name);
                let cover = self
                    .data
                    .packs
                    .iter()
                    .find(|pack| pack.name == *name)
                    .and_then(|pack| pack.banner.as_deref())
                    .map(|source| cx.texture(source, 96.0));
                set_image(item, "Cover", cover);
                crate::widgets::play(
                    item,
                    if cover.is_some() {
                        "WithCover"
                    } else {
                        "WithoutCover"
                    },
                );
                let n = songs.len();
                let plural = if n == 1 { "" } else { "s" };
                // While searching: how many of its songs match.
                let count = match self.matches {
                    Some(m) => {
                        let found = songs.iter().filter(|&&s| m[s]).count();
                        format!("{found} of {n} song{plural}")
                    }
                    None => format!("{n} song{plural}"),
                };
                set_text(item, "Count", &count);
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
                for (k, &d) in Difficulty::ALL[..5].iter().enumerate() {
                    let chart = charts.iter().find(|c| c.difficulty == d);
                    let meter = chart.map(|c| c.meter);
                    set_text(
                        item,
                        &format!("Meter{k}"),
                        &meter.map(|m| m.to_string()).unwrap_or_default(),
                    );
                    // The best's lamp under the meter.
                    if let Some(l) = part(item, &format!("Lamp{k}")) {
                        let lamp = chart.and_then(|c| c.best.as_ref()).map(|b| b.lamp.as_str());
                        crate::widgets::play(l, &format!("Lamp.{}", lamp.unwrap_or("None")));
                    }
                    if let Some(c) = part(item, &format!("Cell{k}")) {
                        crate::widgets::play(c, if meter.is_some() { "Full" } else { "Empty" });
                    }
                }
                set_text(item, "NoCharts", &format!("no {} charts", self.style_name));
                // Edits (and named charts) beside the five meters.
                let edits = charts
                    .iter()
                    .filter(|c| c.difficulty == Difficulty::Edit)
                    .count();
                set_text(
                    item,
                    "Edits",
                    &if edits > 0 {
                        format!("+{edits}")
                    } else {
                        String::new()
                    },
                );
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
    /// Plays start autoplayed (the switch; the data's at first).
    autoplay: bool,
    packs: Vec<(String, Vec<usize>)>,
    /// Each song's search text, folded.
    haystacks: Vec<String>,
    style: usize,
    preferred: Difficulty,
    /// The ladder's chart picked for the song shown.
    pick: Option<usize>,
    closed: BTreeSet<usize>,
    /// Initial saved song waiting for the library scan.
    resume: Option<(String, Option<usize>)>,
    resume_pack: Option<String>,
    matches: Option<Vec<bool>>,
    /// How many songs `matches` matches.
    match_count: usize,
    items: Vec<Item>,
    list: List,
    search: SearchField,
    suggester: RefCell<Option<Suggester>>,
    backdrop: Backdrop,
    pulse: Pulse,
    lit: Option<String>,
    layout: Layout,
    /// The footer's hints, whether a search was in them, its note and
    /// whether Manage was offered.
    hints: Option<(Hints, bool, String, bool)>,
    /// A link a controller pressed could not open (a browser opens one
    /// only from a key or a tap): how long the footer still says so.
    gesture_note: f32,
    /// The item whose song the left side shows.
    shown: Option<Item>,
    /// The song whose preview movie shows in the banner's place
    /// ([`Feed::preview_movie`]).
    movie: Option<String>,
    /// The left side is to be filled again (a new song, a new tree).
    left_dirty: bool,
    /// The ladder's window as drawn: its first chart and how many.
    ladder: (usize, usize),
    /// What the wheel is filtered by.
    filtered: String,
    /// Something the debug view shows changed.
    publish_dirty: bool,
    typing: bool,
    /// What had the focus before the search field: given back when typing
    /// stops by Escape or elsewhere (Enter goes to the wheel).
    search_from: Option<String>,
    dimmed: bool,
    reduced: bool,
    play: PlayFn,
    options: OpenFn,
    shell: Option<ShellFn>,
    feed: Option<SharedFeed>,
    /// The kept plays of a chart (the plays list).
    plays: Option<PlaysFn>,
    /// E opens the chart editor on the song shown.
    editing: bool,
    /// The import's state, shared with its dialog.
    import: Rc<RefCell<ImportView>>,
    notices: Vec<Notice>,
    notices_dirty: bool,
    /// What the shell was last told.
    told_place: Option<SelectPlace>,
    told_song: Option<Option<String>>,
    /// The input being handled came from a controller.
    from_pad: bool,
    navigation: Navigation,
    wheel_motion: Option<Motion>,
    /// How open the wheel's room for the selected row is (closed while
    /// it flicks), and whether the selected row was shrunk for it.
    open: f32,
    shut: bool,
}

/// The wheel's model over a screen's fields, each borrowed on its own
/// (a method borrowing the whole screen would keep `list` from being
/// borrowed mutably beside it).
macro_rules! wheel_model {
    ($s:expr) => {
        wheel(
            &$s.items,
            &$s.data,
            &$s.packs,
            &$s.closed,
            $s.matches.as_deref(),
            $s.style,
            $s.layout.compact,
        )
    };
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
        let notices = data.notices.clone();
        let import = Rc::new(RefCell::new(data.import.clone()));
        let mut s = SelectScreen {
            list: List::new(wheel_config(&layout), &EmptyModel),
            env,
            data: Rc::new(SelectData::default()),
            autoplay: false,
            packs: Vec::new(),
            haystacks: Vec::new(),
            style: 0,
            preferred,
            pick: None,
            closed: BTreeSet::new(),
            resume: None,
            resume_pack: None,
            matches: None,
            match_count: 0,
            items: Vec::new(),
            search,
            suggester: RefCell::new(None),
            backdrop: Backdrop::new(),
            pulse: Pulse::default(),
            lit: None,
            layout,
            hints: None,
            gesture_note: 0.0,
            shown: None,
            movie: None,
            left_dirty: true,
            ladder: (0, 0),
            filtered: String::new(),
            publish_dirty: true,
            typing: false,
            search_from: None,
            dimmed: false,
            reduced: false,
            play,
            options,
            shell: None,
            feed: None,
            plays: None,
            editing: false,
            import,
            notices,
            notices_dirty: true,
            told_place: None,
            told_song: None,
            from_pad: false,
            navigation: Navigation::default(),
            wheel_motion: None,
            open: 1.0,
            shut: false,
        };
        let resume = data.resume.clone();
        let resume_pack = data.resume_pack.clone();
        let loading = data.loading;
        let closed = data.closed.clone();
        let style = data.style.clone();
        let search = data.search.clone();
        s.set_data(data, true);
        s.closed = s
            .packs
            .iter()
            .enumerate()
            .filter(|(_, (name, _))| closed.contains(name))
            .map(|(i, _)| i)
            .collect();
        if let Some(i) = style.and_then(|id| s.data.styles.iter().position(|st| st.id == id)) {
            s.style = i;
        }
        if !search.is_empty() {
            s.search.text = search.clone();
            s.filter_matches(&search);
        }
        s.lay_out_items();
        s.list = List::new(wheel_config(&s.layout), &wheel_model!(s));
        // An explicit start opens its pack. A saved position respects collapsed packs.
        let start_id = start.or_else(|| resume.as_ref().map(|(id, _)| id.as_str()));
        let start_song = start_id.and_then(|id| s.data.songs.iter().position(|x| x.id == id));
        let open = start_song
            .filter(|_| start.is_some())
            .and_then(|i| s.packs.iter().position(|(_, v)| v.contains(&i)))
            .or_else(|| (0..s.packs.len()).find(|p| !s.closed.contains(p)));
        s.open_pack(open);
        s.lay_out_items();
        s.reload();
        if start_song.is_none() && loading {
            s.resume = resume.clone();
        }
        if loading
            && let Some(pack) = resume_pack
            && !s
                .packs
                .iter()
                .any(|(name, _)| name.to_lowercase() == pack.to_lowercase())
        {
            s.resume_pack = Some(pack);
        }
        let at = start_song
            .and_then(|i| s.items.iter().position(|it| *it == Item::Song(i)))
            .or_else(|| s.items.iter().position(|it| matches!(it, Item::Song(_))))
            .or_else(|| (!s.items.is_empty()).then_some(0));
        if let Some(at) = at {
            s.list.select(at);
            s.list.finish();
        }
        s.follow();
        if let Some((id, Some(chart))) = resume
            && s.song().is_some_and(|song| song.id == id)
        {
            s = s.with_chart(chart);
        }
        s
    }

    /// Expand at most one pack. Search displays matches independently.
    fn open_pack(&mut self, open: Option<usize>) {
        self.closed = (0..self.packs.len()).filter(|p| Some(*p) != open).collect();
    }

    /// What the shell does for song select: previews, the list's place,
    /// links, the import; and where it sends data later.
    pub fn with_shell(mut self, shell: ShellFn, feed: SharedFeed) -> SelectScreen {
        self.shell = Some(shell);
        self.feed = Some(feed);
        self
    }

    /// Where the plays list finds a chart's kept plays.
    pub fn with_plays(mut self, plays: PlaysFn) -> SelectScreen {
        self.plays = Some(plays);
        self
    }

    /// E opens the chart editor on the song shown ([`SelectCall::Edit`]).
    pub fn with_editing(mut self) -> SelectScreen {
        self.editing = true;
        self
    }

    /// The chart editor on the song and chart shown.
    fn edit(&mut self, cx: &mut Cx) {
        let (charts, pick) = self.charts();
        let Some(song) = self.song() else {
            return;
        };
        let chart = pick.map_or(0, |p| charts[p].index);
        let song = song.id.clone();
        self.call(SelectCall::Edit { song, chart }, cx);
    }

    /// The plays list of the chart shown, over song select.
    fn open_plays(&mut self, cx: &mut Cx) {
        let Some(f) = self.plays.clone() else {
            return;
        };
        let (charts, pick) = self.charts();
        let (Some(song), Some(chart)) = (self.song(), pick.map(|p| charts[p])) else {
            return;
        };
        let title = format!("{} {}", chart.label(), chart.meter_text())
            .trim_end()
            .to_string();
        let rows = f(song, chart);
        cx.focus("Wheel");
        cx.overlay(
            super::select_dialogs::PlaysDialog::new(
                self.env.clone(),
                &title,
                rows,
                self.shell.clone(),
            ),
            true,
        );
        self.status("plays".into());
    }

    /// The chart shown's best play's replay.
    fn watch_best(&mut self, cx: &mut Cx) {
        let (charts, pick) = self.charts();
        let Some(id) = pick
            .and_then(|p| charts[p].best.as_ref())
            .map(|b| b.play.clone())
            .filter(|id| !id.is_empty())
        else {
            return;
        };
        self.call(SelectCall::WatchPlay(id), cx);
    }

    /// The chart to show first for the start song (the last one started).
    pub fn with_chart(mut self, index: usize) -> SelectScreen {
        self.follow();
        let (charts, _) = self.charts();
        if let Some(k) = charts.iter().position(|c| c.index == index) {
            self.preferred = charts[k].difficulty;
            self.pick = Some(k);
        }
        self
    }

    /// New data (the library read again, bests changed): what was shown
    /// stays, by name, where it still is.
    fn set_data(&mut self, data: SelectData, first: bool) {
        if first {
            self.autoplay = data.autoplay;
        }
        let restoring = self.resume.clone();
        let restoring_pack = self.resume_pack.clone();
        let restore_closed = data.closed.clone();
        let loading = data.loading;
        let shown_id = restoring
            .as_ref()
            .map(|(id, _)| id.clone())
            .or_else(|| self.song().map(|s| s.id.clone()));
        // Where the song shown was: its pack and its place there, and the
        // wheel's place (a removed song gives way to its neighbour).
        let was = match self.shown {
            Some(Item::Song(i)) => self.packs.iter().find_map(|(name, songs)| {
                let k = songs.iter().position(|&s| s == i)?;
                Some((name.clone(), k))
            }),
            _ => None,
        };
        let selected_header = if restoring.is_none() && restoring_pack.is_none() {
            match self.selected_item() {
                Some(Item::Header(p)) => self.packs.get(p).map(|(name, _)| name.clone()),
                _ => None,
            }
        } else {
            None
        };
        let was_at = self.list.selected();
        let style_id = self.style_id().to_string();
        let was_open = self
            .packs
            .iter()
            .enumerate()
            .find(|(p, _)| !self.closed.contains(p))
            .map(|(p, (name, _))| (p, name.clone()));
        let closed: Vec<String> = self
            .closed
            .iter()
            .filter_map(|&p| self.packs.get(p).map(|(n, _)| n.clone()))
            .collect();
        // The packs in order of first song.
        let mut packs: Vec<(String, Vec<usize>)> = Vec::new();
        let mut pack_at: HashMap<&str, usize> = HashMap::new();
        for (i, s) in data.songs.iter().enumerate() {
            let p = *pack_at.entry(s.pack.as_str()).or_insert_with(|| {
                packs.push((s.pack.clone(), Vec::new()));
                packs.len() - 1
            });
            packs[p].1.push(i);
        }
        // The songs' search text, folded again only when what it is made
        // of changed.
        let unchanged = self.haystacks.len() == data.songs.len()
            && self.data.songs.len() == data.songs.len()
            && self
                .data
                .songs
                .iter()
                .zip(&data.songs)
                .all(|(a, b)| same_search(a, b));
        if !unchanged {
            let f = |s: &str| (self.env.fold)(s);
            self.haystacks = data
                .songs
                .iter()
                .map(|s| {
                    let (fields, credits, translits) = search_fields(s);
                    ddi_library::search::haystack(fields.into_iter().chain(credits), translits, &f)
                })
                .collect();
        }
        self.packs = packs;
        *self.suggester.borrow_mut() = None;
        self.data = Rc::new(data);
        if first {
            return;
        }
        self.closed = self
            .packs
            .iter()
            .enumerate()
            .filter(|(_, (n, _))| closed.contains(n))
            .map(|(i, _)| i)
            .collect();
        if restoring.is_some() || restoring_pack.is_some() {
            self.closed = self
                .packs
                .iter()
                .enumerate()
                .filter(|(_, (name, _))| restore_closed.contains(name))
                .map(|(p, _)| p)
                .collect();
        }
        let open = (0..self.packs.len())
            .find(|p| !self.closed.contains(p))
            .or_else(|| {
                let (p, name) = was_open.as_ref()?;
                (!self.packs.is_empty() && !self.packs.iter().any(|(n, _)| n == name))
                    .then(|| (*p).min(self.packs.len() - 1))
            });
        self.open_pack(open);
        self.style = self
            .data
            .styles
            .iter()
            .position(|s| s.id == style_id)
            .unwrap_or(0);
        let text = self.search.text.clone();
        self.filter_matches(&text);
        self.lay_out_items();
        let shown = shown_id
            .and_then(|id| self.data.songs.iter().position(|s| s.id == id))
            .map(Item::Song);
        self.reload();
        let at = selected_header
            .and_then(|name| self.packs.iter().position(|(n, _)| *n == name))
            .and_then(|p| self.items.iter().position(|it| *it == Item::Header(p)))
            .or_else(|| shown.and_then(|it| self.items.iter().position(|x| *x == it)))
            .or_else(|| self.neighbour(was, was_at))
            .or_else(|| self.items.iter().position(|it| matches!(it, Item::Song(_))));
        if let Some(at) = at {
            self.list.select(at);
            self.list.finish();
        }
        self.shown = None;
        self.follow();
        if let Some(pack) = restoring_pack
            && (!loading
                || self
                    .packs
                    .iter()
                    .any(|(name, _)| name.to_lowercase() == pack.to_lowercase()))
        {
            self.resume_pack = None;
        }
        if let Some((id, chart)) = restoring {
            if self.song().is_some_and(|s| s.id == id) {
                self.resume = None;
                if let Some(chart) = chart {
                    let (charts, _) = self.charts();
                    if let Some(k) = charts.iter().position(|c| c.index == chart) {
                        self.preferred = charts[k].difficulty;
                        self.pick = Some(k);
                    }
                }
            } else if !loading {
                self.resume = None;
            }
        }
        self.left_dirty = true;
        self.hints = None;
        self.publish_dirty = true;
    }

    /// The item to show when the song shown is gone (removed): the song
    /// now at its place in its pack (its pack's header when closed), else,
    /// the pack gone, the first song from where the wheel was.
    fn neighbour(&self, was: Option<(String, usize)>, at: Option<usize>) -> Option<usize> {
        if let Some((pack, k)) = was
            && let Some(p) = self.packs.iter().position(|(n, _)| *n == pack)
        {
            let songs = &self.packs[p].1;
            let song = songs[k.min(songs.len().saturating_sub(1))];
            return self
                .items
                .iter()
                .position(|x| *x == Item::Song(song))
                .or_else(|| self.items.iter().position(|x| *x == Item::Header(p)));
        }
        let from = at?.min(self.items.len().checked_sub(1)?);
        self.items[from..]
            .iter()
            .position(|it| matches!(it, Item::Song(_)))
            .map(|k| from + k)
            .or_else(|| {
                self.items[..from]
                    .iter()
                    .rposition(|it| matches!(it, Item::Song(_)))
            })
    }

    /// Fewer songs than [`PACKS_FROM`] are one list, without their packs.
    fn flat(&self) -> bool {
        self.data.songs.len() < PACKS_FROM
    }

    fn lay_out_items(&mut self) {
        self.items.clear();
        let flat = self.flat();
        for (p, (_, songs)) in self.packs.iter().enumerate() {
            let shown: Vec<usize> = songs
                .iter()
                .copied()
                .filter(|&s| self.matches.as_ref().is_none_or(|m| m[s]))
                .collect();
            if shown.is_empty() {
                continue;
            }
            if !flat {
                self.items.push(Item::Header(p));
            }
            // While searching every pack shows its matches.
            if flat || self.matches.is_some() || !self.closed.contains(&p) {
                self.items.extend(shown.into_iter().map(Item::Song));
            }
        }
    }

    fn reload(&mut self) {
        self.cancel_navigation();
        let model = wheel_model!(self);
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
            l.info_h
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
                [w / 2.0, l.style_y],
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
                &Spec::new([w, l.play_h], "PLAY"),
                "Play",
                [w / 2.0, l.play_y],
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
            let margin = l.margin;
            for (name, label, x) in [
                ("Back", "‹", margin + 22.0),
                ("AddIcon", "+", screen[0] - margin - 126.0),
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
            let auto = self.autoplay_switch(screen);
            // The pack's name between Back and the icons.
            let room = auto.current().pos[0] - auto.size()[0] / 2.0 - 8.0 - (margin + 56.0);
            children.push(auto);
            let mut title = env.text("PackTitle", "", 16.0, 700, "Text");
            if let Kind::Text(t) = &mut title.kind {
                t.max_width = room.max(40.0);
                t.ellipsis = true;
            }
            children.push(left(title, margin + 56.0, 28.0));
        } else {
            let mut heading = header(&env, screen[0], "SELECT MUSIC");
            if let Some(title) = part(&mut heading, "Title") {
                call(title, "x", &[94.0]);
            }
            if let Some(subtitle) = part(&mut heading, "Subtitle") {
                subtitle.visible = false;
            }
            children.push(heading);
            children.push(env.component(
                "IconButton",
                "Button",
                &Spec::new([44.0, 44.0], "‹"),
                "Back",
                [54.0, 32.0],
            ));
            children.push(left(
                env.text(
                    "BackHint",
                    "Esc / Backspace: start menu",
                    12.0,
                    400,
                    "Muted",
                ),
                94.0,
                42.0,
            ));
            children.push(self.mods(screen));
        }
        let [fx, fy, fw, fh] = l.field;
        self.search.centre = [fx + fw / 2.0, fy + fh / 2.0];
        self.search.size = [(fw - 76.0).max(40.0), fh];
        self.search.centre[0] -= 38.0;
        let mut field = self.search.build(&*env.look);
        if l.compact {
            field.visible = self.typing || !self.search.text.is_empty();
        }
        children.push(field);
        children.push(env.component(
            "Button",
            "Button",
            &Spec::new([68.0, fh.max(44.0)], "Clear"),
            "ClearSearch",
            [fx + fw - 34.0, fy + fh / 2.0],
        ));
        children.push(frame("Notices", Vec::new()));
        let mut root = Actor::new(Kind::Frame(children));
        super::entrance(&mut root, &env);
        if l.zoom != 1.0 {
            call(&mut root, "zoom", &[l.zoom]);
        }
        self.hints = None;
        self.notices_dirty = true;
        // The same song and chart, drawn again.
        self.left_dirty = true;
        self.publish_dirty = true;
        self.lit = None;
        self.dimmed = false;
        self.backdrop.restore(&mut root, cx, &env);
        root
    }

    /// "Playing with" and the pills, from the header's right.
    /// The autoplay switch: lit when on (every play started then is
    /// autoplayed, not counted). In the header's right, or among the
    /// compact icons.
    fn autoplay_switch(&self, screen: [f32; 2]) -> Actor {
        let env = &self.env;
        let on = self.autoplay;
        let (name, label, x, y, h) = if self.layout.compact {
            let margin = self.layout.margin;
            (
                "AutoIcon",
                if on { "AUTO ON" } else { "AUTO" },
                screen[0] - margin - 154.0,
                28.0,
                44.0,
            )
        } else {
            let margin = env.metric("Margin", 32.0);
            (
                "Autoplay",
                if on { "AUTOPLAY ON" } else { "Autoplay off" },
                screen[0] - margin,
                32.0,
                26.0,
            )
        };
        let w = look::mono_width(label, 14.0) + 24.0;
        let mut pill = look::component_or(&*env.look, "Pill", "Button", &Spec::new([w, h], label));
        pill.name = name.into();
        let w = extent(&mut pill, [w, h])[0];
        // Its right edge at `x` (the compact one left of "+", the full one
        // at the margin).
        call(&mut pill, "xy", &[x - w / 2.0, y]);
        crate::widgets::play(&mut pill, if on { "Selected" } else { "Available" });
        pill.finish_tweening();
        pill
    }

    /// Autoplay on or off: the switch drawn again, the play button's
    /// word, the shell told.
    fn toggle_autoplay(&mut self, root: &mut Actor, cx: &mut Cx) {
        self.autoplay = !self.autoplay;
        let screen = self.layout.screen;
        if self.layout.compact {
            let fresh = self.autoplay_switch(screen);
            if let Some(a) = hit::find_mut(root, "AutoIcon") {
                *a = fresh;
            }
            self.left_dirty = true;
        } else {
            let fresh = self.mods(screen);
            if let Some(m) = hit::find_mut(root, "Mods") {
                *m = fresh;
            }
        }
        if let Some(s) = &self.shell {
            s(SelectCall::Autoplay(self.autoplay), cx);
        }
        self.publish();
    }

    fn mods(&self, screen: [f32; 2]) -> Actor {
        let env = &self.env;
        let margin = env.metric("Margin", 32.0);
        let mut x = screen[0] - margin;
        let mut children = Vec::new();
        // The autoplay switch, rightmost.
        let auto = self.autoplay_switch(screen);
        x -= extent(&mut auto.clone(), [0.0, 26.0])[0].max(
            look::mono_width(
                if self.autoplay {
                    "AUTOPLAY ON"
                } else {
                    "Autoplay off"
                },
                14.0,
            ) + 24.0,
        ) + 12.0;
        children.push(auto);
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

    /// The song the selection shows, at once (what Start, chart changes
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
                "Title", "Subtitle", "Artist", "Bpm", "Length", "Pack", "Line", "Offset", "Credit",
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
        let offset = if song.offset_ms != 0 {
            format!("offset {:+} ms", song.offset_ms)
        } else {
            String::new()
        };
        set_text(info, "Offset", &offset);
        // The credit line, unless it only repeats the artist.
        let credit = if song.credit.trim() == song.artist.trim() {
            ""
        } else {
            song.credit.as_str()
        };
        set_text(info, "Credit", credit);
        let mut line = format!(
            "{} · {} BPM · {}",
            song.artist,
            song.bpm_text,
            song.length_text()
        );
        if !offset.is_empty() {
            line.push_str(&format!(" · {offset}"));
        }
        set_text(info, "Line", &line);
        let side = if self.layout.compact {
            self.layout.song[2]
        } else {
            560.0
        };
        // The preview movie in the banner's place once its frames come
        // (cover-fitted, as the jacket's banner always is).
        let movie = self.movie.as_deref() == Some(song.id.as_str());
        let banner = if movie {
            Some(cx.texture(PREVIEW_MOVIE, side))
        } else {
            song.banner.as_deref().map(|b| cx.texture(b, side))
        };
        if let Some(Kind::Sprite(s)) = part(info, "Banner").map(|a| &mut a.kind) {
            s.fit = if movie {
                ddi_actor::Fit::Cover
            } else {
                ddi_actor::Fit::Stretch
            };
        }
        // A jacket: its square beside a narrower banner.
        let jacket = song.jacket.as_deref().map(|j| cx.texture(j, side));
        let (wide, short) = if jacket.is_some() {
            (None, banner)
        } else {
            (banner, None)
        };
        set_image(info, "Banner", wide);
        set_image(info, "BannerShort", short);
        set_image(info, "Jacket", jacket);
        if let Some(back) = part(info, "JacketBack") {
            back.visible = jacket.is_some();
        }
        let cd_title = song.cd_title.as_deref().map(|c| cx.texture(c, 128.0));
        set_image(info, "CdTitle", cd_title);
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
        let widths: Vec<f32> = self
            .data
            .styles
            .iter()
            .zip(&counts)
            .map(|(st, &n)| {
                look::mono_width(&st.name, 13.0)
                    + 20.0
                    + if n > 0 {
                        look::mono_width(&n.to_string(), 11.0) + 6.0
                    } else {
                        0.0
                    }
            })
            .collect();
        // As many tabs as fit before the TAB hint, the selected one among
        // them; "‹" and "›" say there are more.
        let room = self.layout.song[2] - 48.0;
        let span = |a: usize, b: usize| -> f32 {
            widths[a..b].iter().sum::<f32>()
                + (b - a).saturating_sub(1) as f32 * 6.0
                + if a > 0 { 22.0 } else { 0.0 }
                + if b < widths.len() { 22.0 } else { 0.0 }
        };
        let n = widths.len();
        let (mut a, mut b) = (self.style.min(n), (self.style + 1).min(n));
        loop {
            let grew_right = b < n && span(a, b + 1) <= room;
            if grew_right {
                b += 1;
            }
            let grew_left = a > 0 && span(a - 1, b) <= room;
            if grew_left {
                a -= 1;
            }
            if !grew_right && !grew_left {
                break;
            }
        }
        let mut x = 0.0;
        let mut children = Vec::new();
        if a > 0 {
            children.push(left(
                env.text("Tabs.Before", "‹", 16.0, 700, "Muted"),
                x + 4.0,
                y,
            ));
            x += 22.0;
        }
        for (i, st) in self.data.styles.iter().enumerate().take(b).skip(a) {
            let n = counts[i];
            let count = if n > 0 { n.to_string() } else { String::new() };
            let w = widths[i];
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
        if b < n {
            children.push(left(
                env.text("Tabs.After", "›", 16.0, 700, "Muted"),
                x + 2.0,
                y,
            ));
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
                // Its meter; a named chart without one by its name (cut
                // to the button; the play button says it whole).
                let by_name = c.named() && c.meter == 0;
                let shown = if by_name { c.label() } else { c.meter_text() };
                let mut b = look::component_or(
                    &*env.look,
                    "ChartButton",
                    "Button",
                    &Spec::new([cell, self.layout.chart_h], shown.clone()),
                );
                b.name = format!("Chart{k}");
                set_text(&mut b, "Meter", &shown);
                if by_name
                    && let Some(m) = part(&mut b, "Meter")
                    && let Kind::Text(t) = &mut m.kind
                {
                    t.max_width = cell - 8.0;
                    t.ellipsis = true;
                }
                crate::widgets::play(&mut b, difficulty_name(c.difficulty));
                let lamp = c.best.as_ref().map_or("None", |b| b.lamp.as_str());
                crate::widgets::play(&mut b, &format!("Lamp.{lamp}"));
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
                    &[cell / 2.0 + k as f32 * (cell + 5.0), self.layout.charts_y],
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
                children.push(left(t, 0.0, self.layout.charts_y));
            }
            if let Some(f) = hit::find_mut(root, "Charts") {
                f.kind = Kind::Frame(children);
            }
            let label = match pick.map(|p| charts[p]) {
                Some(c) => format!(
                    "{} {} {}",
                    if self.autoplay { "AUTOPLAY" } else { "PLAY" },
                    c.label().to_uppercase(),
                    c.meter_text()
                )
                .trim_end()
                .to_string(),
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
            set_text(&mut r, "Label", &c.label().to_uppercase());
            set_text(&mut r, "Title", &c.label());
            let notes = if c.credit.is_empty() {
                format!("{} notes", c.notes)
            } else {
                format!("{} notes · steps by {}", c.notes, c.credit)
            };
            set_text(&mut r, "Notes", &notes);
            set_text(&mut r, "Subtitle", &notes);
            set_text(&mut r, "Meter", &c.meter_text());
            // The best: grade, score, lamp; marked while it waits to be
            // worked out again.
            match &c.best {
                Some(b) => {
                    set_text(&mut r, "Grade", &b.grade);
                    let score = if b.pending {
                        format!("{} …", b.score)
                    } else {
                        b.score.clone()
                    };
                    set_text(&mut r, "Score", &score);
                    set_text(&mut r, "LampText", &b.lamp_text);
                    crate::widgets::play(&mut r, &format!("Lamp.{}", b.lamp));
                }
                None => {
                    for p in ["Grade", "Score", "LampText"] {
                        set_text(&mut r, p, "");
                    }
                    crate::widgets::play(&mut r, "Lamp.None");
                }
            }
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
        let manage = self.can_manage();
        // While searching (not typing), what it found.
        let found = match (&self.matches, now) {
            _ if self.gesture_note > 0.0 && now != Hints::Typing => {
                "Press Enter or tap to open the link.".to_string()
            }
            (_, Hints::Typing) | (None, _) => String::new(),
            (Some(_), _) => match self.match_count {
                0 => "No song matches.".to_string(),
                1 => "1 song matches.".to_string(),
                n => format!("{n} songs match."),
            },
        };
        let key = (now, searching, found.clone(), manage);
        if self.hints.as_ref() == Some(&key) {
            return;
        }
        self.hints = Some(key);
        let esc = "Back";
        // The plays are P's (its hint); the credits and the source are
        // the start menu's and the credits'.
        let mut links: Vec<(&str, &str)> = Vec::new();
        if manage {
            links.push(("Link.Manage", "Manage…"));
        }
        links.extend([
            ("Link.Add", "+ Add songs"),
            ("Link.Options", "Options"),
            ("Link.Calibrate", "Calibrate"),
        ]);
        let (hints, note): (&[(&[&str], &str)], &str) = match now {
            Hints::Song => (
                &[
                    (&["↑↓←→"], "Song"),
                    (&["2×↑↓←→"], "Chart"),
                    (&["TAB"], "Style"),
                    (&["ENTER"], "Play"),
                    (&["/"], "Search"),
                    (&["ESC"], esc),
                ],
                "",
            ),
            Hints::Greyed => (
                &[
                    (&["↑↓←→"], "Song"),
                    (&["TAB"], "Style"),
                    (&["/"], "Search"),
                    (&["ESC"], esc),
                ],
                "",
            ),
            Hints::Header if searching => (
                &[(&["↑↓←→"], "Song"), (&["/"], "Search"), (&["ESC"], esc)],
                "",
            ),
            Hints::Header => (
                &[
                    (&["↑↓←→"], "Song"),
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
        let links = if now == Hints::Typing {
            &[][..]
        } else {
            &links[..]
        };
        // Delete (or ⌘/Ctrl Backspace) manages what was imported.
        let mut hints: Vec<(&[&str], &str)> = hints.to_vec();
        if manage && now != Hints::Typing {
            hints.push((&["DEL"], "Manage"));
        }
        if self.plays.is_some() && now == Hints::Song {
            hints.push((&["P"], "Plays"));
        }
        if self.editing && now == Hints::Song && !self.layout.compact {
            hints.push((&["E"], "Edit"));
        }
        let note = if note.is_empty() {
            found.as_str()
        } else {
            note
        };
        let (f, texts) = footer(&self.env, self.layout.screen, &hints, note, links);
        if let Some(old) = hit::find_mut(root, "Footer") {
            *old = f;
        } else if let Kind::Frame(c) = &mut root.kind {
            c.push(f);
        }
        self.env.state.borrow_mut().hints = texts;
    }

    /// A flick or a drag passes rows over the wheel's line while the
    /// selected row stays where it was (the list selects what is on its
    /// line once at rest): the room on the line for the selected row, and
    /// the row's own growth, are there only while the selected row is on
    /// the line and the list moves slower than [`FLICK_ROWS`] a second;
    /// they close and open together, so a flick leaves no gap.
    fn flick_room(&mut self, dt: f32, root: &mut Actor) {
        if self.layout.compact {
            return;
        }
        let row = f64::from(self.layout.row);
        let fast = self.list.speed() > row * f64::from(FLICK_ROWS);
        // Every wheel item is a row high (headers too).
        let on_line = self.list.selected().is_some_and(|i| {
            let mid = (i as f64 + 0.5) * row - self.list.scroll();
            let line = f64::from(self.layout.wheel[3]) * 0.5;
            ((mid - line) / row).abs() < 0.5
        });
        let shut = fast || !on_line;
        if shut != self.shut {
            self.shut = shut;
            if let Some(name) = self.list.selected().and_then(|i| self.list.item_actor(i))
                && let Some(a) = hit::find_mut(root, &name)
            {
                crate::widgets::play(a, if shut { "Deselect" } else { "Select" });
            }
        }
        let target = if shut { 0.0 } else { 1.0 };
        let t = self.env.metric("Motion.Select", 0.12).max(0.01);
        self.open += (target - self.open) * (1.0 - (-dt / (t / 3.0)).exp());
        if (target - self.open).abs() < 0.01 {
            self.open = target;
        }
        self.env.look.set_wheel_open("Wheel", self.open);
    }

    /// Whether Manage can act on what is selected ([`Self::manageable`]
    /// is some).
    fn can_manage(&self) -> bool {
        match self.selected_item() {
            Some(Item::Header(p)) => self.packs.get(p).is_some_and(|(name, _)| {
                self.data
                    .packs
                    .iter()
                    .find(|q| q.name == *name)
                    .is_some_and(|q| q.removable || q.rereadable)
            }),
            Some(Item::Song(i)) => self.data.songs.get(i).is_some_and(|s| s.removable),
            None => false,
        }
    }

    /// Whether a key may be a letter's shortcut: a fresh press, not
    /// typed into search or composed, with no modifier but Shift.
    fn letter_shortcut(&self, k: &crate::event::KeyEvent) -> bool {
        k.down && !k.repeat && !k.composing && !self.typing && !k.modifiers.any_but_shift()
    }

    /// What Manage acts on: the selected header's pack, or the selected
    /// song (imported), when something can be done.
    fn manageable(&self) -> Option<Manage> {
        let pack = |name: &str| {
            self.data
                .packs
                .iter()
                .find(|p| p.name == name)
                .cloned()
                .unwrap_or_else(|| PackEntry {
                    name: name.to_string(),
                    ..Default::default()
                })
        };
        let count = |name: &str| {
            self.packs
                .iter()
                .find(|(n, _)| n == name)
                .map_or(0, |(_, v)| v.len())
        };
        match self.selected_item()? {
            Item::Header(p) => {
                let name = &self.packs.get(p)?.0;
                let pack = pack(name);
                (pack.removable || pack.rereadable).then(|| Manage::Pack {
                    songs: count(name),
                    pack,
                })
            }
            Item::Song(i) => {
                let song = self.data.songs.get(i)?;
                song.removable.then(|| Manage::Song {
                    id: song.id.clone(),
                    title: song.title.clone(),
                    pack: pack(&song.pack),
                    songs: count(&song.pack),
                })
            }
        }
    }

    /// An import runs: removing and reading folders again wait.
    fn busy(&self) -> bool {
        self.import.borrow().running.is_some()
    }

    /// A notice's link: the options, removing the songs that could not be
    /// read (asked first), else the shell's.
    fn notice_link(&mut self, i: usize, cx: &mut Cx) {
        let Some((_, id)) = self.notices.get(i).and_then(|n| n.link.clone()) else {
            return;
        };
        match id.as_str() {
            "options" => {
                let env = self.env.clone();
                (self.options)(&env, cx);
            }
            "remove-unreadable" => {
                let count = self.notices[i]
                    .text
                    .split(' ')
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
                self.open_manage(Manage::Unreadable { count }, cx);
            }
            _ => self.call(SelectCall::Notice(id), cx),
        }
    }

    /// Opens the manage dialog on `m`.
    fn open_manage(&self, m: Manage, cx: &mut Cx) {
        cx.overlay(
            ManageDialog::new(self.env.clone(), m, self.shell.clone(), self.busy())
                .on(self.import.borrow().platform),
            true,
        );
    }

    fn call(&self, c: SelectCall, cx: &mut Cx) {
        if let Some(s) = &self.shell {
            s(c, cx);
        } else if c == SelectCall::Leave {
            cx.close(true);
        }
    }

    /// Tells the shell what changed: the song shown (its preview), the
    /// list's place.
    fn tell(&mut self, cx: &mut Cx) {
        if self.shell.is_none() || !cx.is_current_screen() {
            return;
        }
        let song = match self.shown {
            Some(Item::Song(i)) => self.data.songs.get(i).map(|s| &s.id),
            _ => None,
        };
        // Compared by borrow, cloned only when it changed.
        if self.told_song.as_ref().map(Option::as_ref) != Some(song) {
            let song = song.cloned();
            self.told_song = Some(song.clone());
            let from_pad = self.from_pad;
            self.call(SelectCall::Selected { song, from_pad }, cx);
        }
        let (charts, pick) = self.charts();
        if self.resume.is_some() || self.resume_pack.is_some() {
            return;
        }
        let chart = pick.and_then(|p| charts.get(p)).map(|c| c.index);
        if self
            .told_place
            .as_ref()
            .is_some_and(|told| self.place_matches(told, chart))
        {
            return;
        }
        let place = SelectPlace {
            open: self.place_open().cloned(),
            song: self.place_song().cloned(),
            chart,
            style: self.style_id().to_string(),
            search: self.search.text.clone(),
            closed: self.place_closed().cloned().collect(),
        };
        self.told_place = Some(place.clone());
        self.call(SelectCall::Place(place), cx);
    }

    /// The list's place's open pack: the first one not closed.
    fn place_open(&self) -> Option<&String> {
        self.packs
            .iter()
            .enumerate()
            .find(|(p, _)| !self.closed.contains(p))
            .map(|(_, (name, _))| name)
    }

    /// The list's place's song: the selected row's.
    fn place_song(&self) -> Option<&String> {
        match self.selected_item() {
            Some(Item::Song(i)) => Some(&self.data.songs[i].id),
            _ => None,
        }
    }

    /// The list's place's closed packs, by name.
    fn place_closed(&self) -> impl Iterator<Item = &String> {
        self.closed
            .iter()
            .filter_map(|&p| self.packs.get(p).map(|(n, _)| n))
    }

    /// Whether `told` is the list's place now (with `chart` picked),
    /// without building it.
    fn place_matches(&self, told: &SelectPlace, chart: Option<usize>) -> bool {
        told.open.as_ref() == self.place_open()
            && told.song.as_ref() == self.place_song()
            && told.chart == chart
            && told.style == self.style_id()
            && told.search == self.search.text
            && told.closed.iter().eq(self.place_closed())
    }

    /// What the shell sent: new data, the import's progress, notices.
    fn take_feed(&mut self, root: &mut Actor, cx: &mut Cx) {
        let Some(feed) = self.feed.clone() else {
            return;
        };
        let (data, import, notices, open, overlay, movie) = {
            let mut f = feed.borrow_mut();
            (
                f.data.take(),
                f.import.take(),
                f.notices.take(),
                std::mem::take(&mut f.open_import),
                f.overlay.take(),
                f.preview_movie.take(),
            )
        };
        if let Some(movie) = movie
            && movie != self.movie
        {
            self.movie = movie;
            self.fill_info(root, cx);
            self.publish_dirty = true;
        }
        if let Some(d) = data {
            self.cancel_navigation();
            self.set_data(d, false);
            self.fill_tabs(root);
            self.fill_charts(root);
        }
        if let Some(i) = import {
            *self.import.borrow_mut() = i;
            cx.broadcast(Message::new(IMPORT_CHANGED));
        }
        if let Some(n) = notices {
            self.notices = n;
            self.notices_dirty = true;
        }
        if let Some(open) = overlay {
            self.cancel_navigation();
            open(&self.env, cx);
        }
        // Files dropped on the page: the dialog shows how the import goes.
        if open && !self.import_open(cx) {
            self.cancel_navigation();
            cx.focus("Wheel");
            cx.overlay(
                ImportDialog::new(self.env.clone(), self.import.clone(), self.shell.clone()),
                true,
            );
        }
    }

    /// Whether the import dialog is open over the screen (the theme's
    /// state counts what is over song select).
    fn import_open(&self, _cx: &Cx) -> bool {
        self.env.state.borrow().import_open
    }

    /// The notices under the header: their text and links.
    fn fill_notices(&mut self, root: &mut Actor) {
        if !std::mem::take(&mut self.notices_dirty) {
            return;
        }
        let env = self.env.clone();
        let l = self.layout;
        // One line, as wide as the song's column: the notices' texts (cut
        // to fit), then their links.
        let [x0, _, width] = l.song;
        let y = if l.upright {
            l.song[1] + l.play_y - l.play_h / 2.0 - 16.0
        } else if l.compact {
            l.screen[1] - 16.0
        } else {
            74.0
        };
        let links: Vec<(usize, &str)> = self
            .notices
            .iter()
            .enumerate()
            .filter_map(|(i, n)| n.link.as_ref().map(|(label, _)| (i, label.as_str())))
            .collect();
        let links_w: f32 = links
            .iter()
            .map(|(_, l)| look::mono_width(l, 13.0) + 16.0 + 8.0)
            .sum();
        let text = self
            .notices
            .iter()
            .map(|n| n.text.as_str())
            .collect::<Vec<_>>()
            .join(" · ");
        let error = self.notices.iter().any(|n| n.error);
        let mut children = Vec::new();
        let room = (width - links_w - 8.0).max(80.0);
        let mut t = env.text(
            "Notice",
            &text,
            13.0,
            400,
            if error { "Danger" } else { "Muted" },
        );
        if let Kind::Text(tx) = &mut t.kind {
            tx.max_width = room;
            tx.ellipsis = true;
        }
        children.push(left(t, x0, y));
        let mut x = x0 + look::mono_width(&text, 13.0).min(room) + 8.0;
        for (i, label) in links {
            let lw = look::mono_width(label, 13.0) + 16.0;
            let mut a =
                look::component_or(&*env.look, "Link", "Button", &Spec::new([lw, 24.0], label));
            a.name = format!("Notice.Link{i}");
            call(&mut a, "xy", &[x + lw / 2.0, y]);
            children.push(a);
            x += lw + 8.0;
        }
        if let Some(f) = hit::find_mut(root, "Notices") {
            f.kind = Kind::Frame(children);
        }
    }

    // Actions.

    fn cancel_navigation(&mut self) {
        self.navigation.cancel();
        if self.wheel_motion.take().is_some()
            && let Some(i) = self.list.selected()
        {
            let target = self.list.selection_scroll(i);
            self.list.jump(target);
        }
    }

    /// The row one step from row `i`: back for Up and Left, on for Down
    /// and Right, within the list.
    fn step_target(&self, i: usize, action: MenuAction) -> usize {
        if matches!(action, MenuAction::Up | MenuAction::Left) {
            i.saturating_sub(1)
        } else {
            (i + 1).min(self.items.len().saturating_sub(1))
        }
    }

    fn apply_navigation(&mut self, events: Vec<NavEvent>, root: &mut Actor, cx: &mut Cx) {
        let now = cx.time();
        let reduced = self.env.reduced();
        let mut moved_from = None;
        for event in events {
            match event {
                NavEvent::Preview(action) => {
                    let Some(i) = self.list.selected() else {
                        continue;
                    };
                    let to = self.step_target(i, action);
                    let origin = self.list.selection_scroll(i);
                    let target = self.list.selection_scroll(to);
                    let motion = self
                        .wheel_motion
                        .get_or_insert_with(|| Motion::new(self.list.scroll()));
                    motion.preview(origin, target, now, reduced);
                }
                NavEvent::Freeze => {
                    if let Some(m) = &mut self.wheel_motion {
                        m.stop(now, None);
                    }
                }
                NavEvent::Move(actions, pad) => {
                    self.wheel_motion
                        .get_or_insert_with(|| Motion::new(self.list.scroll()));
                    for action in actions {
                        let Some(i) = self.list.selected() else {
                            continue;
                        };
                        let to = self.step_target(i, action);
                        {
                            let model = wheel_model!(self);
                            self.list.select_without_motion(to, &model);
                        }
                        // A pack header keeps the last song passed, even when
                        // several buffered taps are committed together.
                        self.follow();
                    }
                    if let Some(i) = self.list.selected() {
                        self.wheel_motion.as_mut().unwrap().commit(
                            self.list.selection_scroll(i),
                            now,
                            reduced,
                        );
                    }
                    moved_from = Some(pad);
                }
                NavEvent::Charts(changes) => {
                    for by in changes {
                        self.move_chart(by, root);
                    }
                    if let Some(i) = self.list.selected() {
                        let target = self.list.selection_scroll(i);
                        if let Some(m) = &mut self.wheel_motion {
                            m.to(target, now, reduced);
                        }
                    }
                }
                NavEvent::Stop => {
                    if let Some(i) = self.list.selected() {
                        let target = self.list.selection_scroll(i);
                        if let Some(m) = &mut self.wheel_motion {
                            m.stop(now, Some(target));
                        }
                    }
                }
            }
            self.publish_dirty = true;
        }
        if let Some(pad) = moved_from {
            self.follow();
            let old = std::mem::replace(&mut self.from_pad, pad);
            self.tell(cx);
            self.from_pad = old;
        }
    }

    fn flush_move(&mut self, root: &mut Actor, cx: &mut Cx) {
        let events = self.navigation.finish();
        self.apply_navigation(events, root, cx);
    }

    fn arrow(&mut self, action: MenuAction, repeat: bool, root: &mut Actor, cx: &mut Cx) {
        if repeat {
            return;
        }
        let source = cx.menu_source().cloned().unwrap_or_else(|| {
            if self.from_pad {
                crate::nav::Source::Pad(0, action)
            } else {
                crate::nav::Source::Key(format!("{action:?}"))
            }
        });
        let charts = matches!(self.selected_item(), Some(Item::Song(_)));
        let events = self.navigation.down(action, source, cx.time(), charts);
        self.apply_navigation(events, root, cx);
    }

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
        let open = self.closed.contains(&p);
        self.open_pack(open.then_some(p));
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

    /// Ordinary browsing returns to the title menu, preserving the list.
    fn back(&mut self, _root: &mut Actor, cx: &mut Cx) -> Handled {
        self.cancel_navigation();
        self.tell(cx);
        self.call(SelectCall::Leave, cx);
        Handled::Yes
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
        self.filter_matches(text);
        // Cleared: the song shown comes back, its pack opened if it was
        // closed.
        if self.matches.is_none()
            && let Some(Item::Song(s)) = self.shown
            && let Some(p) = self.packs.iter().position(|(_, v)| v.contains(&s))
        {
            self.open_pack(Some(p));
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
        let n = if self.matches.is_some() {
            self.match_count
        } else {
            self.data.songs.len()
        };
        self.env.state.borrow_mut().matches = n;
    }

    /// The songs matching `text` (all for none), the wheel not laid out.
    fn filter_matches(&mut self, text: &str) {
        self.filtered = text.to_string();
        self.publish_dirty = true;
        let f = |s: &str| (self.env.fold)(s);
        self.matches = if text.trim().is_empty() {
            None
        } else {
            let q = ddi_library::search::Query::new(text, &f);
            Some(self.haystacks.iter().map(|h| q.matches(h)).collect())
        };
        self.match_count = self
            .matches
            .as_ref()
            .map_or(0, |m| m.iter().filter(|&&x| x).count());
        let n = if self.matches.is_some() {
            self.match_count
        } else {
            self.data.songs.len()
        };
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

    /// Typing stopped other than by Enter: the focus goes back to where
    /// it was before the field (the wheel when that is gone), unless it
    /// already moved on (a press elsewhere).
    fn stop_typing(&mut self, cx: &mut Cx) {
        self.typing = false;
        let from = self.search_from.take();
        if cx.focused() != Some(self.search.name.as_str()) {
            return;
        }
        let names = self.focusables();
        let to = from
            .filter(|f| *f != self.search.name && names.iter().any(|n| n.name == *f))
            .unwrap_or_else(|| "Wheel".into());
        cx.focus(to);
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
        st.autoplay = self.autoplay;
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
        st.selected_row = self.list.selected().and_then(|i| self.list.item_actor(i));
        st.wheel = self.layout.wheel;
        st.best = pick
            .and_then(|p| charts.get(p))
            .and_then(|c| c.best.as_ref())
            .map(|b| {
                format!(
                    "{} {} {}{}",
                    b.grade,
                    b.score,
                    b.lamp,
                    if b.pending { " pending" } else { "" }
                )
            });
        st.notices = self.notices.iter().map(|n| n.text.clone()).collect();
    }
}

/// What a song is searched by: its fields, its charts' credits and
/// names, and its transliterations.
fn search_fields(s: &SongEntry) -> ([&str; 5], impl Iterator<Item = &str> + '_, [&str; 2]) {
    (
        [
            s.title.as_str(),
            s.subtitle.as_str(),
            s.artist.as_str(),
            s.pack.as_str(),
            s.credit.as_str(),
        ],
        s.charts
            .iter()
            .flat_map(|c| [c.credit.as_str(), c.name.as_str()]),
        [s.title_translit.as_str(), s.artist_translit.as_str()],
    )
}

/// Whether two songs are searched by the same text.
fn same_search(a: &SongEntry, b: &SongEntry) -> bool {
    let (a_fields, a_credits, a_translits) = search_fields(a);
    let (b_fields, b_credits, b_translits) = search_fields(b);
    a_fields == b_fields && a_translits == b_translits && a_credits.eq(b_credits)
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
            cands.push((&s.title, &s.title_translit, SuggestKind::Title));
            cands.push((&s.artist, &s.artist_translit, SuggestKind::Artist));
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
    matches: Option<&'a [bool]>,
    style: usize,
    compact: bool,
) -> Wheel<'a> {
    let st = data.styles.get(style);
    Wheel {
        items,
        data,
        packs,
        closed,
        matches,
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
    c.continuous_selection = !l.compact;
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
        self.layout = Layout::fit(&self.env, [vp.width, vp.height], compact);
        self.build_tree(cx)
    }

    fn focusables(&self) -> Vec<Focusable> {
        // The header's row in order, explicitly (the wheel below lies
        // nearer than the next thing to the right): the search, the mods,
        // the autoplay switch.
        let n = self.data.mods.len();
        let row: Vec<String> = if self.layout.compact {
            Vec::new()
        } else {
            std::iter::once(self.search.name.clone())
                .chain((0..n).map(|i| format!("Mod{i}")))
                .chain(std::iter::once("Autoplay".to_string()))
                .collect()
        };
        let along = |name: &str| -> Focusable {
            let mut f = Focusable::new(name);
            if let Some(i) = row.iter().position(|r| r == name) {
                f.left = i.checked_sub(1).map(|j| row[j].clone());
                f.right = row.get(i + 1).cloned();
                if name != row[0] {
                    f.down = Some("Wheel".into());
                }
            }
            f
        };
        let mut v = vec![
            Focusable::new("Wheel"),
            along(&self.search.name),
            Focusable::new("ClearSearch"),
        ];
        if self.layout.compact {
            for n in [
                "Back",
                "AutoIcon",
                "AddIcon",
                "SearchIcon",
                "OptionsIcon",
                "StyleButton",
                "Play",
            ] {
                v.push(Focusable::new(n));
            }
            v.extend((0..5).map(|k| Focusable::new(format!("Chart{k}"))));
        } else {
            v.push(along("Autoplay"));
            for n in [
                "Back",
                "Link.Manage",
                "Link.Add",
                "Link.Options",
                "Link.Calibrate",
            ] {
                v.push(Focusable::new(n));
            }
            v.extend((0..self.data.styles.len()).map(|i| Focusable::new(format!("Tab{i}"))));
            v.extend((0..LADDER_ROWS).map(|k| Focusable::new(format!("Ladder.Row{k}"))));
        }
        if !self.notices.is_empty() {
            v.push(Focusable::new("Notice"));
        }
        v.extend((0..self.notices.len()).map(|i| Focusable::new(format!("Notice.Link{i}"))));
        v.extend((0..n).map(|i| along(&format!("Mod{i}"))));
        v
    }

    fn direction_edges(&self) -> bool {
        true
    }

    fn owns_direction_repeat(&self) -> bool {
        self.navigation.holding()
    }

    fn animating(&self) -> bool {
        self.navigation.animating()
            || self.wheel_motion.is_some()
            || self.list.animating()
            || self.pulse.on
            || (!self.layout.compact && (self.open > 0.0 && self.open < 1.0))
    }

    fn update(&mut self, dt: f32, root: &mut Actor, cx: &mut Cx) {
        if !cx.is_current_screen() {
            self.cancel_navigation();
            return;
        }
        let events = self.navigation.advance(cx.time());
        self.apply_navigation(events, root, cx);
        // A new size (a window resized or turned, a new design), or
        // reduced motion changed: laid out again.
        let vp = cx.viewport().screen;
        let compact = super::is_compact(&*self.env.look, vp);
        if self.gesture_note > 0.0 {
            self.gesture_note -= dt;
            if self.gesture_note <= 0.0 {
                self.hints = None;
                self.fill_footer(root);
            }
        }
        if [vp.width, vp.height] != self.layout.vp
            || compact != self.layout.compact
            || self.env.reduced() != self.reduced
        {
            self.cancel_navigation();
            self.reduced = self.env.reduced();
            self.layout = Layout::fit(&self.env, [vp.width, vp.height], compact);
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
        if let Some(m) = &self.wheel_motion {
            self.list.visual_scroll(m.sample(cx.time()).0);
            self.open = 1.0;
            self.shut = false;
            self.env.look.set_wheel_open("Wheel", 1.0);
        } else {
            self.flick_room(dt, root);
        }
        let event = {
            let look = self.env.look.clone();
            let model = wheel_model!(self);
            self.list.update(dt, root, &model, &*look, cx)
        };
        if self
            .wheel_motion
            .as_ref()
            .is_some_and(|m| !m.animating(cx.time()))
            && !self.navigation.pending()
        {
            self.wheel_motion = None;
        }
        if let Some(ListEvent::Activated(_)) = event {
            self.start(root, cx);
        }
        self.show(root, cx);
        self.fill_footer(root);
        self.fill_notices(root);
        self.tell(cx);
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
        let song = match self.shown {
            Some(Item::Song(s)) => self.data.songs.get(s),
            _ => None,
        };
        let (id, bpm) = song.map_or(("", 0.0), |s| (s.id.as_str(), s.bpm));
        let v = self.pulse.update(dt, &self.env, id, bpm);
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
        if let Some(clear) = hit::find_mut(root, "ClearSearch") {
            clear.visible = self.typing || !self.search.text.is_empty();
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
        let back_on_top = st.screen != "select";
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
        // Zoomed (a phone upright): pointers' moves and the wheel in the
        // layout's pixels, so the list follows the finger one for one
        // (places stay the screen's: the list finds its own through the
        // actors).
        let scaled;
        let input = match input {
            Input::Pointer {
                target,
                gesture,
                pos,
                kind,
            } if self.layout.zoom != 1.0 => {
                let k = 1.0 / self.layout.zoom;
                scaled = Input::Pointer {
                    target: target.clone(),
                    gesture: gesture.scaled(k),
                    pos: *pos,
                    kind: *kind,
                };
                &scaled
            }
            Input::Wheel {
                target,
                delta,
                pos,
                kind,
            } if self.layout.zoom != 1.0 => {
                let k = 1.0 / self.layout.zoom;
                scaled = Input::Wheel {
                    target: target.clone(),
                    delta: [delta[0] * k, delta[1] * k],
                    pos: *pos,
                    kind: *kind,
                };
                &scaled
            }
            _ => input,
        };
        if matches!(
            input,
            Input::Menu { .. }
                | Input::Activate { .. }
                | Input::Wheel { .. }
                | Input::Pointer {
                    gesture: crate::Gesture::Press,
                    ..
                }
        ) || matches!(input, Input::Key(k) if k.down)
        {
            self.resume = None;
            self.resume_pack = None;
        }
        if let Input::MenuRelease { source } = input {
            let events = self.navigation.up(source, cx.time());
            self.apply_navigation(events, root, cx);
            self.publish();
            return Handled::Yes;
        }
        if matches!(input, Input::CancelDirections) {
            self.cancel_navigation();
            return Handled::Yes;
        }
        self.from_pad = matches!(input, Input::Menu { .. }) && !cx.menu_from_keyboard();
        match input {
            // Starting or changing style immediately after a single tap acts
            // on the requested song, even before its tap window has elapsed.
            Input::Menu {
                action: MenuAction::Start | MenuAction::Select,
                ..
            } => {
                self.flush_move(root, cx);
            }
            Input::Menu {
                action: MenuAction::Back,
                ..
            }
            | Input::Activate { .. }
            | Input::Focus { .. }
            | Input::Wheel { .. }
            | Input::Pointer {
                gesture: crate::Gesture::Press,
                ..
            } => self.cancel_navigation(),
            Input::Key(k) if k.down => self.cancel_navigation(),
            _ => {}
        }
        // A link a controller pressed: the browser opens it only from a
        // key or a tap; the footer says so for a while.
        if let Input::NeedsGesture(UiRequest::OpenUrl(_)) = input {
            self.gesture_note = 4.0;
            self.hints = None;
            self.fill_footer(root);
            self.status("press Enter or tap to open".into());
            self.publish_dirty = true;
            self.publish();
            return Handled::Yes;
        }
        let h = self.input_inner(input, root, cx);
        // The song the selection shows, now: a Start in the same poll as a
        // move acts on the song moved to.
        self.follow();
        self.sync_field(root);
        self.publish_dirty = true;
        self.publish();
        // Tell the shell synchronously for changes committed by this input.
        self.tell(cx);
        self.from_pad = false;
        h
    }

    fn message(&mut self, m: &Message, root: &mut Actor, cx: &mut Cx) {
        if m.name == FEED {
            self.take_feed(root, cx);
        }
        if m.name == NOTICE_LINK
            && let Some(crate::message::Param::Int(i)) = m.get("Index")
        {
            self.notice_link(*i as usize, cx);
        }
        if let Some(e) = self.search.message(m, root, cx) {
            self.field_event(e, root, cx);
        }
        self.follow();
        self.sync_field(root);
        self.publish();
        self.tell(cx);
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
            Input::Focus { from, to: Some(to) } if *to == self.search.name => {
                self.search_from = from.clone()
            }
            Input::Activate { name } if *name == self.search.name => self.typing = true,
            Input::Text { change, .. } => match change {
                TextChange::Blurred => self.stop_typing(cx),
                // The first Escape closes the popup, the next stops
                // typing (the field's).
                TextChange::Cancel if !popup_before => self.stop_typing(cx),
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
            // Manage: Delete, or ⌘ Backspace (Ctrl elsewhere: a Mac's
            // forward delete is fn Backspace).
            Input::Key(k)
                if k.down
                    && !k.repeat
                    && !self.typing
                    && (k.key == "Delete" || k.key == "Backspace" && k.modifiers.command()) =>
            {
                if let Some(m) = self.manageable() {
                    self.open_manage(m, cx);
                }
                return Handled::Yes;
            }
            // A long press on a chart (a phone has no keys, no footer):
            // its plays.
            Input::Pointer {
                target: Some(t),
                gesture: crate::Gesture::LongPress,
                ..
            } if self.plays.is_some()
                && let Some(k) = t.chain.iter().find_map(|n| {
                    n.strip_prefix("Chart")
                        .or_else(|| n.strip_prefix("Ladder.Row"))
                        .and_then(|n| n.parse::<usize>().ok())
                }) =>
            {
                self.pick_chart(k, root, cx);
                self.open_plays(cx);
                self.publish();
                return Handled::Yes;
            }
            // The chart's plays (P), its best's replay (W).
            Input::Key(k)
                if self.letter_shortcut(k)
                    && self.plays.is_some()
                    && (k.key.eq_ignore_ascii_case("p") || k.key.eq_ignore_ascii_case("w")) =>
            {
                if k.key.eq_ignore_ascii_case("p") {
                    self.open_plays(cx);
                } else {
                    self.watch_best(cx);
                }
                self.publish();
                return Handled::Yes;
            }
            // The chart editor (E).
            Input::Key(k)
                if self.letter_shortcut(k) && self.editing && k.key.eq_ignore_ascii_case("e") =>
            {
                self.edit(cx);
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
                let model = wheel_model!(self);
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
        // The header (the search, the switch, the mods, a phone's
        // icons): menu directions move the focus there, not the chart or
        // the wheel.
        let in_header = cx.focused().is_some_and(|f| {
            f == self.search.name
                || f.starts_with("Mod")
                || matches!(
                    f,
                    "Autoplay" | "Back" | "AutoIcon" | "AddIcon" | "SearchIcon" | "OptionsIcon"
                )
        });
        // (The search field keeps Up and Down for the wheel, as before.)
        let on_search = cx.focused() == Some(self.search.name.as_str());
        // The header is the top: a fresh Up there goes past it, to the
        // app's notices (the focus stays, for Down to come back to).
        if in_header
            && !on_search
            && matches!(
                input,
                Input::Menu {
                    action: MenuAction::Up,
                    repeat: false
                }
            )
        {
            cx.enter_chrome();
            return Handled::Yes;
        }
        if in_header
            && matches!(input, Input::Menu { action, .. }
                if matches!(action, MenuAction::Left | MenuAction::Right)
                    || (!on_search && action.is_direction()))
        {
            return Handled::No;
        }
        // A fresh Up on the first row goes on up (the search field, then
        // the app's notices over the screen).
        if let Input::Menu {
            action: MenuAction::Up,
            repeat: false,
        } = input
            && self.list.selected() == Some(0)
        {
            return Handled::No;
        }
        if let Input::Menu { action, repeat } = input
            && action.is_direction()
        {
            self.arrow(*action, *repeat, root, cx);
            return Handled::Yes;
        }
        // Pointer gestures and scrolling keep the list's usual behavior.
        if !matches!(input, Input::Menu { .. }) {
            let model = wheel_model!(self);
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
        if name == "Autoplay" || name == "AutoIcon" {
            self.toggle_autoplay(root, cx);
            return Handled::Yes;
        }
        if name.starts_with("Mod") || name == "PlayingWith" {
            let env = self.env.clone();
            (self.options)(&env, cx);
            cx.focus("Wheel");
            return Handled::Yes;
        }
        if let Some(i) = name
            .strip_prefix("Notice.Link")
            .and_then(|n| n.parse::<usize>().ok())
        {
            self.notice_link(i, cx);
            cx.focus("Wheel");
            return Handled::Yes;
        }
        if name == "Notice" {
            // Every notice in full.
            cx.focus("Wheel");
            cx.overlay(
                NoticesDialog::new(self.env.clone(), self.notices.clone()),
                true,
            );
            return Handled::Yes;
        }
        match name {
            "Link.Options" | "OptionsIcon" => {
                let env = self.env.clone();
                (self.options)(&env, cx);
            }
            "Link.Add" | "AddIcon" => {
                // Back on the wheel once the dialog closes.
                cx.focus("Wheel");
                cx.overlay(
                    ImportDialog::new(self.env.clone(), self.import.clone(), self.shell.clone()),
                    true,
                );
                self.status("add songs".into());
                return Handled::Yes;
            }
            "Link.Manage" => {
                cx.focus("Wheel");
                if let Some(m) = self.manageable() {
                    self.open_manage(m, cx);
                }
                return Handled::Yes;
            }
            "ClearSearch" => self.clear_search(root, cx),
            "Link.Calibrate" => self.call(SelectCall::Open("calibrate".into()), cx),
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
