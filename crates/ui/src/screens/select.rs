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
//!   under it; the wheel shows the matches; its Clear an × inside it),
//!   what is in effect, the quick volume ([`Volume`]) and More.
//!   **Footer**: what the keys do now, and links.
//! - **Keys**: all four arrows move through songs; double-tap the same
//!   arrow to change chart (Up/Left easier, Down/Right harder). Tab
//!   (the menu's Select) goes to the next lit style, Start plays
//!   (or opens and closes a pack), `/` searches, Back returns to the
//!   start menu. A fresh Up on the first row focuses the search field:
//!   Start types there, Down or Back return to the list where it was,
//!   Right reaches Clear (a controller's way to the search,
//!   `docs/plans/song-navigation.md`).
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

use ddi_engine::MenuInput as HintAction;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use ddi_actor::{Actor, Kind};
use ddi_chart::Difficulty;
use ddi_chart::simplify::Kind as Version;
use ddi_l10n::UiText;
use ddi_l10n::msg::select as msg;
use ddi_library::announcer::{Comment, Line};
use ddi_library::suggest::{Kind as SuggestKind, Suggester};

use super::data::{
    ChartEntry, ImportView, Notice, SelectData, ShareView, SongEntry, closest, style_name,
};
use super::{Backdrop, Env, FooterHint, extent, footer_hints, frame, ground, header, left, right};
use crate::hit;
use crate::look::{self, Spec, call, quad, quad_at};
use crate::message::Message;
use crate::nav::{Focusable, MenuAction};
use crate::request::{EnterKey, InputHints, InputMode, UiRequest};
use crate::screen::{Cx, Handled, Input, Screen, TextChange};
use crate::state::place::pack_key;
use crate::state::select_view::pack_label;
use crate::widgets::field::{FieldEvent, SearchField};
use crate::widgets::list::{ItemKind, List, ListConfig, ListEvent, ListModel, ListStyle};
use crate::widgets::volume::{Volume, VolumeChange};
use crate::widgets::{part, set_image, set_text};

use super::data::PackEntry;

/// The parts that move as the screen comes and goes ([`super::transitions`]),
/// both layouts' (a part the layout lacks is skipped): what sits in the
/// header's row is the header's (the bars stay put from screen to screen).
#[path = "select_room.rs"]
mod room_mode;

const SELECT_PARTS: [(&str, super::Edge); 19] = {
    use super::Edge::{FooterBar, HeaderBar, Left, None, Right};
    [
        ("Header", HeaderBar),
        ("Mods", HeaderBar),
        ("Search", HeaderBar),
        ("ClearSearch", HeaderBar),
        ("Volume", HeaderBar),
        ("Back", HeaderBar),
        ("AddIcon", HeaderBar),
        ("SearchIcon", HeaderBar),
        ("OptionsIcon", HeaderBar),
        ("PackTitle", HeaderBar),
        ("Footer", FooterBar),
        ("Wheel", Right),
        ("RoomParts", Right),
        ("Wheel.Track", Right),
        ("Wheel.Thumb", Right),
        ("Song", Left),
        ("RoomPlayers", Left),
        ("Notices", None),
        ("Backdrop", None),
    ]
};
use super::select_dialogs::{ImportDialog, Manage, ManageDialog, NoticesDialog, ShareDialog};
use super::select_navigation::{Event as NavEvent, Motion, Navigation};

/// What Start does on a song with a chart (step 6: start play; the
/// developer routes: show the sample results).
pub type PlayFn = Rc<dyn Fn(&SongEntry, &ChartEntry, &Env, &mut Cx)>;
/// What the Options link does.
pub type OpenFn = Rc<dyn Fn(&Env, &mut Cx)>;

/// 5.1's `HardCommentMeter`: a chart this hard has song select say
/// `select music comment hard`.
pub const HARD_COMMENT_METER: u32 = 10;

/// What song select says of the chart chosen, in 5.1's order
/// (`ScreenSelectMusic`): `repeat` when it was played in this run (its
/// best's last play since `started`, milliseconds since the Unix epoch),
/// `new` when it has no best under the rules in effect, `hard` from
/// [`HARD_COMMENT_METER`], `general` otherwise.
pub fn comment(chart: &ChartEntry, started: f64) -> Comment {
    match &chart.best {
        Some(b) if started > 0.0 && b.last_played >= started => Comment::Repeat,
        None => Comment::New,
        _ if chart.meter >= HARD_COMMENT_METER => Comment::Hard,
        _ => Comment::General,
    }
}

/// What song select asks the shell. Selection can settle on a frame;
/// gesture-bound requests are made inside the input that asked.
#[derive(Clone, Debug, PartialEq)]
pub enum SelectCall {
    /// Leave song browsing for the start menu.
    Leave,
    /// The quick volume moved: the preview plays at this gain from now
    /// (the screen keeps it in the settings once it settles).
    Volume(f32),
    /// The song shown changed (its preview starts): `None`, no song.
    /// `from_pad`: the move came from a controller.
    Selected {
        song: Option<String>,
        from_pad: bool,
    },
    /// Where the list is, kept for the session.
    Place(SelectPlace),
    /// Song select shows (`true`: at first, and uncovered, after its
    /// `Selected`) or not (another screen over it, or it leaves): its
    /// preview plays or stops ([`Screen::shown`]; dialogs cover nothing).
    Shown(bool),
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
    /// Make a new song from a music file the player picks (the shell's
    /// picker; cancelled, nothing changes) and open the editor on it:
    /// Add songs' "New song", where the shell offers editing.
    NewSong,
    /// Share a pack's songs (by id, imported ones) by link, expiring
    /// after `expires` (`1h`, `1d`, `1w`, `1m`, `never`): the shell
    /// uploads them and reports through [`Feed::share`]
    /// (`docs/plans/shared-songs.md`).
    Share {
        pack: String,
        songs: Vec<String>,
        expires: String,
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
    /// The packs closed, by name (`None`: the bundled songs).
    pub closed: Vec<Option<String>>,
    /// The one expanded browsing pack (search ignores this state), by its
    /// key ([`crate::state::place::pack_key`]).
    pub open: Option<String>,
}

/// What the shell sends song select after it started (the library read
/// again, bests worked out, an import running), taken when the message
/// [`FEED`] comes.
#[derive(Default)]
pub struct Feed {
    pub data: Option<SelectData>,
    pub import: Option<ImportView>,
    /// How the upload of a share goes ([`SelectCall::Share`]).
    pub share: Option<ShareView>,
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
/// The share's upload moved ([`Feed::share`]): its dialog redraws.
pub const SHARE_CHANGED: &str = "ShareChanged";
/// The message the notices dialog sends for a notice's link (`Index`).
pub const NOTICE_LINK: &str = "NoticeLink";

/// The Tab key's legend beside the style tabs, as keyboards print it
/// (data: a key's legend is never translated).
const TAB_LEGEND: &str = "TAB";

/// A generated chart version's mark as the screens show it (its
/// [`ChartEntry::version`]).
pub fn version_name(version: Version) -> UiText {
    match version {
        Version::WithoutOnigiri => msg::version_without_onigiri(),
        Version::WithoutCenter => msg::version_without_center(),
        Version::FromPumpSingle => msg::version_from_pump_single(),
        Version::FromFiveKeys => msg::version_from_five_keys(),
    }
}

/// A chart as a button names it, from `shown`, its label and meter as
/// data in the button's case: a named chart that is a version with the
/// version's mark, as it would otherwise read as its source does.
pub(super) fn chart_text(c: &ChartEntry, shown: &str) -> UiText {
    match c.version.filter(|_| c.named()) {
        Some(v) => msg::chart_version(UiText::data(shown), version_name(v)),
        None => UiText::data(shown),
    }
}

/// Text read from a song's files (its title, artist, a chart's name):
/// data, none where there is none.
fn data(s: &str) -> String {
    if s.is_empty() {
        String::new()
    } else {
        UiText::data(s).into()
    }
}

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
    /// The song's side in its short form: the style button and a row of
    /// chart buttons (the compact layouts; a room's, whose players take
    /// the room the banner and the ladder had, `Layout::for_room`).
    chips: bool,
    /// The song's info's width, and where the short form's style button,
    /// chart buttons and Play go across the song's column: from, wide.
    info_w: f32,
    picker: [f32; 2],
    /// A room's players panel beside the song (the wide layout's).
    players: Option<[f32; 4]>,
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
            chips: true,
            info_w: w,
            picker: [0.0, w],
            players: None,
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
                chips: true,
                info_w: 300.0,
                picker: [0.0, 300.0],
                players: None,
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
            chips: false,
            info_w: m("Select.Song.Width", 560.0),
            picker: [0.0, m("Select.Song.Width", 560.0)],
            players: None,
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
            // The search at the right; the mods left of it (`tree`).
            field: [
                screen[0] - m("Margin", 32.0) - m("Select.Search.Width", 560.0),
                (header - 42.0) / 2.0,
                m("Select.Search.Width", 560.0),
                42.0,
            ],
        }
    }
}

impl Layout {
    /// A room's layout (`select_room.rs`): the wide one with the song's
    /// side short (a banner as a phone's with the title and a line under
    /// it, the style button and the chart buttons beside it) and the
    /// players panel under it, down to where the ladder ended. The compact
    /// layouts are short already: theirs keep the song's side and show
    /// the players as a strip over the list.
    fn for_room(mut self) -> Layout {
        if self.compact {
            return self;
        }
        let [x, y, w] = self.song;
        let gap = 16.0;
        self.chips = true;
        self.info_w = 300.0;
        self.info_h = ROOM_INFO_H;
        self.picker = [self.info_w + gap, (w - self.info_w - gap).max(120.0)];
        self.style_y = 18.0;
        self.charts_y = 66.0;
        self.chart_h = 44.0;
        self.play_h = 0.0;
        let top = y + ROOM_INFO_H + gap;
        let bottom = self.ladder[0] + self.ladder[1];
        self.players = Some([x, top, w, (bottom - top).max(60.0)]);
        self
    }
}

/// A room's song's side's height in the wide layout: a phone's banner
/// (94), the title and a line.
const ROOM_INFO_H: f32 = 140.0;

/// The wheel's items for the list.
struct Wheel<'a> {
    items: &'a [Item],
    data: &'a SelectData,
    packs: &'a [(Option<String>, Vec<usize>)],
    open_pack: Option<usize>,
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
                let open = self.matches.is_some() || self.open_pack == Some(p);
                set_text(item, "Chevron", if open { "▾" } else { "▸" });
                set_text(item, "Title", &pack_label(name.as_deref()));
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
                // While searching: how many of its songs match.
                let count = match self.matches {
                    Some(m) => {
                        let found = songs.iter().filter(|&&s| m[s]).count();
                        msg::pack_songs_found(found, n)
                    }
                    None => msg::pack_songs(n),
                };
                set_text(item, "Count", &count);
            }
            Item::Song(s) => {
                let song = &self.data.songs[s];
                set_text(item, "Title", &data(&song.title));
                set_text(item, "Subtitle", &data(&song.artist));
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
                // Hidden when the song has charts in the style.
                if charts.is_empty() {
                    set_text(
                        item,
                        "NoCharts",
                        &msg::row_no_charts(style_name(self.style, self.style_name)),
                    );
                }
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

/// What song select's footer shows: its hints, whether a search is in
/// them, its note (a gesture's, else the matches counted), whether Manage
/// is offered and whether the header has the focus.
type FooterKey = (Hints, bool, Result<String, usize>, bool, bool);

/// Song select.
pub struct SelectScreen {
    /// The header's wordmark's clock.
    live: super::Live,
    env: Env,
    data: Rc<SelectData>,
    /// Plays start autoplayed (the switch; the data's at first).
    autoplay: bool,
    packs: Vec<(Option<String>, Vec<usize>)>,
    /// Each song's search text, folded.
    haystacks: Vec<String>,
    style: usize,
    preferred: Difficulty,
    /// The ladder's chart picked for the song shown.
    pick: Option<usize>,
    /// The one pack open (a search shows every pack's matches); the
    /// others are closed.
    open_pack: Option<usize>,
    /// Counts the changes of the packs' names and order, so that what
    /// the shell was told of them is compared without building it.
    packs_version: u64,
    /// `open_pack` and `packs_version` as the shell was last told them.
    told_packs: (Option<usize>, u64),
    /// Initial saved song waiting for the library scan.
    resume: Option<(String, Option<usize>)>,
    resume_pack: Option<String>,
    matches: Option<Vec<bool>>,
    /// How many songs `matches` matches.
    match_count: usize,
    items: Vec<Item>,
    list: List,
    search: SearchField,
    /// The quick volume, right of the search (the wide layout's).
    volume: Volume,
    suggester: RefCell<Option<Suggester>>,
    backdrop: Backdrop,
    layout: Layout,
    /// The footer's hints, whether a search was in them, its note (a
    /// gesture's, else the matches counted) and whether Manage was
    /// offered.
    hints: Option<FooterKey>,
    /// A link a controller pressed could not open, or the search field
    /// could not start typing (a browser does either only from a key or
    /// a tap): how long the footer still says so, and what.
    gesture_note: f32,
    gesture_text: String,
    /// The item whose song the left side shows.
    shown: Option<Item>,
    /// The song whose preview movie shows in the banner's place
    /// ([`Feed::preview_movie`]).
    movie: Option<String>,
    /// The left side is to be filled again (a new song, a new tree).
    left_dirty: bool,
    /// The ladder's window as drawn: its first chart and how many.
    ladder: (usize, usize),
    /// The width and height the ladder's rows were made at.
    ladder_rows: Option<(f32, f32)>,
    /// The tabs as made: each style's chart count and the one selected.
    tabs_for: Option<(Vec<usize>, usize)>,
    /// What the wheel is filtered by.
    filtered: String,
    /// Something the debug view shows changed.
    publish_dirty: bool,
    /// An input moved the list: which rows show, and where, is known at
    /// the next frame.
    rows_dirty: bool,
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
    /// Share… is offered on imported songs and packs.
    sharing: bool,
    /// The imported songs (ids) added to the next share, in order
    /// ("Add to share" in More; "Share N songs…" takes them).
    to_share: Vec<String>,
    /// The import's state, shared with its dialog.
    import: Rc<RefCell<ImportView>>,
    /// The share upload's state, shared with its dialog.
    share: Rc<RefCell<ShareView>>,
    notices: Vec<Notice>,
    notices_dirty: bool,
    /// What the shell was last told.
    told_place: Option<SelectPlace>,
    told_song: Option<Option<String>>,
    /// The shell was told it shows ([`SelectCall::Shown`]).
    told_shown: bool,
    /// The input being handled came from a controller.
    from_pad: bool,
    navigation: Navigation,
    wheel_motion: Option<Motion>,
    /// How open the wheel's room for the selected row is (closed while
    /// it flicks).
    open: f32,
    /// The multiplayer room it is the main view of (M7b, `select_room.rs`).
    room: Option<room_mode::RoomMode>,
    /// Whether the room's parts were built for its host, and the room's
    /// name the header was built with.
    room_built_host: Option<bool>,
    room_built_heading: Option<String>,
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
            $s.open_pack,
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
            &msg::search_placeholder(),
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
            live: Default::default(),
            list: List::new(wheel_config(&layout), &EmptyModel),
            env,
            data: Rc::new(SelectData::default()),
            autoplay: false,
            packs: Vec::new(),
            haystacks: Vec::new(),
            style: 0,
            preferred,
            pick: None,
            open_pack: None,
            packs_version: 0,
            told_packs: (None, 0),
            resume: None,
            resume_pack: None,
            matches: None,
            match_count: 0,
            items: Vec::new(),
            search,
            volume: Volume::new("Volume", crate::state::settings::Settings::load().volume),
            suggester: RefCell::new(None),
            backdrop: Backdrop::new(),
            layout,
            hints: None,
            gesture_note: 0.0,
            gesture_text: String::new(),
            shown: None,
            movie: None,
            left_dirty: true,
            ladder: (0, 0),
            ladder_rows: None,
            tabs_for: None,
            filtered: String::new(),
            publish_dirty: true,
            rows_dirty: false,
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
            sharing: false,
            to_share: Vec::new(),
            import,
            share: Rc::default(),
            notices,
            notices_dirty: true,
            told_place: None,
            told_song: None,
            told_shown: false,
            from_pad: false,
            navigation: Navigation::default(),
            wheel_motion: None,
            open: 1.0,
            room: None,
            room_built_host: None,
            room_built_heading: None,
        };
        let resume = data.resume.clone();
        let resume_pack = data.resume_pack.clone();
        let loading = data.loading;
        let closed = data.closed.clone();
        let style = data.style.clone();
        let search = data.search.clone();
        s.set_data(data, true);
        let closed: HashSet<&Option<String>> = closed.iter().collect();
        let saved_open = s.packs.iter().position(|(name, _)| !closed.contains(name));
        s.open_pack = saved_open;
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
        s.open_pack = start_song
            .filter(|_| start.is_some())
            .and_then(|i| s.packs.iter().position(|(_, v)| v.contains(&i)))
            .or(saved_open);
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
                .any(|(name, _)| pack_key(name.as_deref()) == pack)
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

    /// Share… in the actions of imported songs and packs
    /// ([`SelectCall::Share`]).
    pub fn with_sharing(mut self) -> SelectScreen {
        self.sharing = true;
        self
    }

    /// [`Self::with_sharing`] when `on`.
    pub fn with_sharing_if(self, on: bool) -> SelectScreen {
        if on { self.with_sharing() } else { self }
    }

    /// The list is fixed: a share's songs, or the library while choosing
    /// a stage's song; nothing is added, edited, managed or shared.
    fn fixed_list(&self) -> bool {
        self.data.shared.is_some() || self.room.is_some()
    }

    /// What Share… would share: the imported song shown, or the imported
    /// pack whose header is shown, `(pack, its songs' ids, the title)`
    /// (the song's or the pack's name).
    fn shareable(&self) -> Option<(String, Vec<String>, Option<String>)> {
        if !self.sharing || self.fixed_list() {
            return None;
        }
        let pack_songs = |name: &str| -> Vec<String> {
            self.packs
                .iter()
                .find(|(n, _)| n.as_deref() == Some(name))
                .map(|(_, v)| {
                    v.iter()
                        .filter_map(|&i| self.data.songs.get(i))
                        .filter(|s| s.removable)
                        .map(|s| s.id.clone())
                        .collect()
                })
                .unwrap_or_default()
        };
        match self.selected_item()? {
            Item::Header(p) => {
                // The bundled songs are not shared.
                let name = self.packs.get(p)?.0.clone()?;
                let songs = pack_songs(&name);
                (!songs.is_empty()).then(|| (name.clone(), songs, Some(name)))
            }
            Item::Song(i) => {
                let song = self.data.songs.get(i)?;
                let pack = song.pack.clone().filter(|_| song.removable)?;
                Some((pack, vec![song.id.clone()], Some(song.title.clone())))
            }
        }
    }

    /// The imported song shown, when it is one.
    fn shown_imported(&self) -> Option<&SongEntry> {
        if !self.sharing || self.fixed_list() {
            return None;
        }
        match self.selected_item()? {
            Item::Song(i) => self.data.songs.get(i).filter(|s| s.removable),
            Item::Header(_) => None,
        }
    }

    /// What "Share N songs…" shares: the songs added, in order, under
    /// their pack's name when they share one, else "Shared songs"; no
    /// title (the dialog says how many).
    fn share_list(&self) -> Option<(String, Vec<String>, Option<String>)> {
        if self.to_share.is_empty() {
            return None;
        }
        let packs: Vec<Option<&str>> = self
            .to_share
            .iter()
            .filter_map(|id| self.data.songs.iter().find(|s| &s.id == id))
            .map(|s| s.pack.as_deref())
            .collect();
        let pack = match packs.first() {
            Some(Some(p)) if packs.iter().all(|q| *q == Some(*p)) => p.to_string(),
            _ => msg::share_default_pack().into(),
        };
        Some((pack, self.to_share.clone(), None))
    }

    /// The Share dialog on what is shown.
    fn open_share(&mut self, cx: &mut Cx) {
        let Some((pack, songs, title)) = self.shareable() else {
            return;
        };
        self.open_share_of(pack, songs, title, cx);
    }

    fn open_share_of(
        &mut self,
        pack: String,
        songs: Vec<String>,
        title: Option<String>,
        cx: &mut Cx,
    ) {
        cx.focus("Wheel");
        cx.overlay(
            ShareDialog::new(
                self.env.clone(),
                pack,
                songs,
                title,
                self.share.clone(),
                self.shell.clone(),
            ),
            true,
        );
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
                chart.version,
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
    fn set_data(&mut self, mut data: SelectData, first: bool) {
        self.room_styles(&mut data.styles);
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
            .open_pack
            .and_then(|p| Some((p, self.packs.get(p)?.0.clone())));
        // The packs in order of first song.
        let mut packs: Vec<(Option<String>, Vec<usize>)> = Vec::new();
        let mut pack_at: HashMap<Option<&str>, usize> = HashMap::new();
        for (i, s) in data.songs.iter().enumerate() {
            let p = *pack_at.entry(s.pack.as_deref()).or_insert_with(|| {
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
        if !self
            .packs
            .iter()
            .map(|(n, _)| n)
            .eq(packs.iter().map(|(n, _)| n))
        {
            self.packs_version += 1;
        }
        let old = std::mem::replace(&mut self.packs, packs);
        *self.suggester.borrow_mut() = None;
        self.data = Rc::new(data);
        self.tabs_for = None;
        if first {
            return;
        }
        // By name: every pack but the open one is closed, a new one not.
        // A saved place being restored has its own list.
        let closed: HashSet<&Option<String>> = if restoring.is_some() || restoring_pack.is_some() {
            restore_closed.iter().collect()
        } else {
            old.iter()
                .enumerate()
                .filter(|&(p, _)| Some(p) != self.open_pack)
                .map(|(_, (n, _))| n)
                .collect()
        };
        self.open_pack = self
            .packs
            .iter()
            .position(|(n, _)| !closed.contains(n))
            .or_else(|| {
                let (p, name) = was_open.as_ref()?;
                (!self.packs.is_empty() && !self.packs.iter().any(|(n, _)| n == name))
                    .then(|| (*p).min(self.packs.len() - 1))
            });
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
                    .any(|(name, _)| pack_key(name.as_deref()) == pack))
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
    fn neighbour(&self, was: Option<(Option<String>, usize)>, at: Option<usize>) -> Option<usize> {
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
            if flat || self.matches.is_some() || self.open_pack == Some(p) {
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

    /// The style's name as its tab shows it (its message).
    fn style_label(&self) -> UiText {
        style_name(self.style_id(), self.style_name())
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
        if !self.layout.compact {
            // The search at the header's right, as wide as the room past
            // the header's row and the mods leaves it.
            let [sw, _] = self.layout.screen;
            let margin = self.layout.margin;
            let mods = if self.room.is_some() {
                self.room_buttons_width()
            } else {
                self.mods_width()
            };
            let room = sw - margin - env.metric("Header.TitleEnd", 460.0) - mods - 108.0;
            let w = env
                .metric("Select.Search.Width", 560.0)
                .min(room)
                .max(200.0);
            let [_, fy, _, fh] = self.layout.field;
            self.layout.field = [sw - margin - w - 76.0, fy, w, fh];
        }
        let l = self.room_layout(self.layout);
        let screen = l.screen;
        let mut children = vec![ground(&env, screen), self.backdrop.build(&env, screen)];
        // The wheel.
        self.list.config = wheel_config(&l);
        self.list.config.glide = !self.reduced;
        self.list.reset_actors();
        let mut wheel = self.list.build(&*env.look);
        // A room's follower has no list: the room's panel is there.
        wheel.visible = !self.following();
        children.push(wheel);
        if !l.compact {
            let [x, y, _, h] = l.wheel;
            let bar_x = screen[0] - 10.0 - 1.5;
            let track = quad_at(
                "Wheel.Track",
                [3.0, h - 24.0],
                env.color("Track"),
                [bar_x, y + h / 2.0],
            );
            let mut thumb = quad("Wheel.Thumb", [3.0, 40.0], env.color("Thumb"));
            call(&mut thumb, "xy", &[bar_x, y + 12.0]);
            let _ = x;
            children.extend([track, thumb]);
        }
        // The selected song.
        let [sx, sy, sw] = l.song;
        let info_h = if l.chips {
            l.info_h
        } else {
            l.tabs_y - sy - 12.0
        };
        let info = env.component(
            "SongInfo",
            "Panel",
            &Spec::new([l.info_w, info_h], ""),
            "Info",
            [0.0, 0.0],
        );
        let mut song = vec![info];
        if l.chips {
            let [px, w] = l.picker;
            let mut style = env.component(
                "StyleButton",
                "Button",
                &Spec::new([w, 36.0], ""),
                "StyleButton",
                [px + w / 2.0, l.style_y],
            );
            if let Some(p) = part(&mut style, "Label") {
                call(p, "halign", &[0.0]);
                call(p, "x", &[-w / 2.0 + 12.0]);
                // A room's wide layout: a narrower button than a phone's.
                if !l.compact {
                    look::ellipsized(p, w - 24.0);
                }
            }
            song.push(style);
            song.push(frame("Charts", Vec::new()));
            // A room's wide layout has no Play: Start on the list picks.
            if l.play_h > 0.0 {
                let mut play = env.component(
                    "PlayButton",
                    "Button",
                    &Spec::new([w, l.play_h], msg::play_button()),
                    "Play",
                    [px + w / 2.0, l.play_y],
                );
                // A room's follower picks nothing.
                play.visible = !self.following();
                song.push(play);
            }
        } else {
            song.push(frame("Tabs", Vec::new()));
            let mut kbd = env.numbers("TabKey", &UiText::data(TAB_LEGEND), 11.0, "Muted");
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
            children.push(super::back_button(
                &env,
                [44.0, 44.0],
                [margin + 22.0, 28.0],
            ));
            for (name, label, x) in [
                ("AddIcon", "+", screen[0] - margin - 126.0),
                ("SearchIcon", "/", screen[0] - margin - 74.0),
                ("More", "•••", screen[0] - margin - 22.0),
            ] {
                // A share: nothing to add. A room's follower: nothing to
                // search.
                if (name == "AddIcon" && self.fixed_list())
                    || (name == "SearchIcon" && self.following())
                {
                    continue;
                }
                children.push(env.component(
                    "IconButton",
                    "Button",
                    &Spec::new([44.0, 44.0], label),
                    name,
                    [x, 28.0],
                ));
            }
            // The pack's name between Back and the icons.
            let room = screen[0] - margin - 148.0 - 8.0 - (margin + 56.0);
            let mut title = env.text("PackTitle", "", 16.0, 700, "Text");
            look::ellipsized(&mut title, room.max(40.0));
            children.push(left(title, margin + 56.0, 28.0));
        } else {
            let name = if let Some(rm) = &self.room {
                super::room::heading(&rm.room.borrow())
            } else if self.data.shared.is_some() {
                msg::title_shared()
            } else {
                msg::title()
            };
            children.push(header(&env, cx, screen[0], &name));
            if self.room.is_none() {
                children.push(self.mods(l.field[0] - 16.0));
            }
            children.push(super::more_button(
                &env,
                [screen[0] - l.margin - 32.0, 32.0],
            ));
        }
        // The search, its Clear an × inside its right end; the wide
        // layout's quick volume right of it.
        let [fx, fy, fw, fh] = l.field;
        let room = if l.compact { 0.0 } else { 76.0 };
        let bw = (fw - room).max(40.0);
        // In the language now (built again on a change of language).
        self.search.hint = msg::search_placeholder().into();
        self.search.centre = [fx + bw / 2.0, fy + fh / 2.0];
        self.search.size = [bw, fh];
        let mut field = self.search.build(&*env.look);
        if l.compact {
            field.visible = self.typing || !self.search.text.is_empty();
        }
        // A room's follower has nothing to search.
        if self.following() {
            field.visible = false;
        }
        children.push(field);
        children.push(env.component(
            "Search.Clear",
            "Button",
            &Spec::new([28.0, 28.0], "×"),
            "ClearSearch",
            [fx + bw - 21.0, fy + fh / 2.0],
        ));
        if !l.compact {
            self.volume.centre = [fx + fw - 34.0, fy + fh / 2.0];
            children.push(self.volume.build(&*env.look, cx));
        }
        children.push(frame("Notices", Vec::new()));
        let parts = self.room_parts(cx);
        children.push(parts);
        if self.room.is_some() {
            children.push(self.room_players());
        }
        let mut root = Actor::new(Kind::Frame(children));
        // The header's row from above, the footer from below, the wheel
        // from the right, the song's side from the left, the backdrop
        // fading (the compact layout's header parts are its own).
        super::transitions(&mut root, &env, &SELECT_PARTS);
        if l.zoom != 1.0 {
            call(&mut root, "zoom", &[l.zoom]);
        }
        self.hints = None;
        self.tabs_for = None;
        self.notices_dirty = true;
        // The same song and chart, drawn again.
        self.left_dirty = true;
        self.publish_dirty = true;
        self.dimmed = false;
        self.backdrop.restore(&mut root, cx, &env);
        root
    }

    /// About what [`SelectScreen::mods`] takes across.
    fn mods_width(&self) -> f32 {
        if self.data.mods.is_empty() {
            return 0.0;
        }
        let pills: f32 = self
            .data
            .mods
            .iter()
            .map(|m| look::mono_width(m, 14.0) + 28.0)
            .sum();
        pills + look::mono_width(&msg::playing_with(), 13.0) + 8.0
    }

    /// "Playing with" and the pills, ending at `end`.
    fn mods(&self, end: f32) -> Actor {
        let env = &self.env;
        let mut x = end;
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
                env.text("PlayingWith", &msg::playing_with(), 13.0, 400, "Muted"),
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
                "Title", "Artist", "Bpm", "Length", "Pack", "Line", "Offset", "Credit",
            ] {
                set_text(info, p, "");
            }
            set_image(info, "Banner", None);
            return;
        };
        set_text(info, "Title", &data(&song.title));
        set_text(info, "Artist", &data(&song.artist));
        set_text(info, "Bpm", &data(&song.bpm_text));
        set_text(info, "Length", &data(&song.length_text()));
        set_text(info, "Pack", &pack_label(song.pack.as_deref()));
        let offset = if song.offset_ms != 0 {
            msg::info_offset(format!("{:+}", song.offset_ms)).into()
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
        set_text(info, "Credit", &data(credit));
        let mut line: String =
            msg::info_line(&song.artist, &song.bpm_text, song.length_text()).into();
        if !offset.is_empty() {
            line.push_str(&format!(" · {offset}"));
        }
        set_text(info, "Line", &line);
        let side = if self.layout.chips {
            self.layout.info_w
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
            // A room's: its name in the pack's place.
            let pack = match &self.room {
                Some(rm) => super::room::heading(&rm.room.borrow()),
                None => pack_label(song.pack.as_deref()),
            };
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

    fn fill_tabs(&mut self, root: &mut Actor) {
        let counts = self.counts();
        if self.layout.chips {
            let lit = counts.iter().filter(|&&n| n > 0).count();
            let n = counts.get(self.style).copied().unwrap_or(0);
            // A room's wide layout has a narrower button: the style and
            // its charts alone.
            let others = if self.layout.compact {
                lit - usize::from(n > 0)
            } else {
                0
            };
            super::set(
                root,
                "StyleButton",
                "Label",
                &msg::style_button(self.style_label(), n, others),
            );
            // A room's wide layout: the styles only where there is one
            // to change to.
            let shown = self.style_button_shown();
            if let Some(b) = hit::find_mut(root, "StyleButton") {
                b.visible = shown;
            }
            return;
        }
        let Some(tabs) = hit::find_mut(root, "Tabs") else {
            return;
        };
        // The same tabs in the same states: nothing to make again.
        if self.tabs_for.as_ref() == Some(&(counts.clone(), self.style)) {
            return;
        }
        self.tabs_for = Some((counts.clone(), self.style));
        let env = &self.env;
        let y = self.layout.tabs_y - self.layout.song[1] + 16.0;
        let names: Vec<UiText> = self
            .data
            .styles
            .iter()
            .map(|st| style_name(&st.id, &st.name))
            .collect();
        let widths: Vec<f32> = names
            .iter()
            .zip(&counts)
            .map(|(name, &n)| {
                look::mono_width(name, 13.0)
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
        for (i, name) in names.iter().enumerate().take(b).skip(a) {
            let n = counts[i];
            let count = if n > 0 { n.to_string() } else { String::new() };
            let w = widths[i];
            let mut tab = look::component_or(
                &*env.look,
                "StyleTab",
                "Button",
                &Spec::new([w, 32.0], name.as_str()).with_items([count.clone()]),
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
        let size = (self.layout.song[2], row);
        let same_size = self.ladder_rows.replace(size) == Some(size);
        let (charts, pick) = self.charts();
        let style_name = self.style_label();
        if self.layout.chips {
            let [px, w] = self.layout.picker;
            let charts_y = self.charts_y();
            let cell = (w - 4.0 * 5.0) / 5.0;
            let mut children = Vec::new();
            for k in 0..n {
                let c = charts[first + k];
                // Its meter; a named chart without one by its name (cut
                // to the button; the play button says it whole).
                let by_name = c.named() && c.meter == 0;
                let shown: String = if by_name {
                    chart_text(c, &c.label()).into()
                } else {
                    data(&c.meter_text())
                };
                let mut b = look::component_or(
                    &*env.look,
                    "ChartButton",
                    "Button",
                    &Spec::new([cell, self.layout.chart_h], shown.clone()),
                );
                b.name = format!("Chart{k}");
                set_text(&mut b, "Meter", &shown);
                if by_name && let Some(m) = part(&mut b, "Meter") {
                    look::ellipsized(m, cell - 8.0);
                }
                crate::widgets::play(&mut b, c.difficulty.name());
                crate::widgets::play(
                    &mut b,
                    match (c.version, c.assisted()) {
                        (None, _) => "Original",
                        (Some(_), true) => "Assisted",
                        (Some(_), false) => "Converted",
                    },
                );
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
                    &[px + cell / 2.0 + k as f32 * (cell + 5.0), charts_y],
                );
                children.push(b);
            }
            if charts.is_empty() {
                let mut t = env.text(
                    "NoChartsText",
                    &msg::no_charts(&style_name),
                    14.0,
                    600,
                    "Text",
                );
                // A room's wide layout: a narrower column than a phone's.
                if !self.layout.compact {
                    look::ellipsized(&mut t, w);
                }
                children.push(left(t, px, charts_y));
            }
            // A room's wide layout: the chart chosen in words under its
            // buttons (a phone's Play button says it).
            if self.layout.play_h == 0.0
                && let Some(c) = pick.map(|p| charts[p])
            {
                let y = charts_y + self.layout.chart_h / 2.0;
                let name = format!("{} {}", c.label().to_uppercase(), c.meter_text());
                let mut t = env.text(
                    "ChartName",
                    &chart_text(c, name.trim_end()),
                    14.0,
                    700,
                    "Text",
                );
                look::ellipsized(&mut t, w);
                children.push(left(t, px, y + 16.0));
                let mut notes: String = if c.credit.is_empty() {
                    msg::chart_notes(c.notes)
                } else {
                    msg::chart_notes_by(c.notes, &c.credit)
                }
                .into();
                if let Some(version) = c.version {
                    notes = format!("{} · {notes}", version_name(version));
                }
                let mut t = env.text("ChartNotes", &notes, 12.0, 400, "Muted");
                look::ellipsized(&mut t, w);
                children.push(left(t, px, y + 34.0));
            }
            if let Some(f) = hit::find_mut(root, "Charts") {
                f.kind = Kind::Frame(children);
            }
            let label = match pick.map(|p| charts[p]) {
                Some(c) => {
                    // The chart as it names itself (data), in the
                    // button's capitals.
                    let chart = format!("{} {}", c.label().to_uppercase(), c.meter_text());
                    let chart = chart_text(c, chart.trim_end());
                    let label: String = if self.autoplay {
                        msg::autoplay_chart(chart)
                    } else {
                        msg::play_chart(chart)
                    }
                    .into();
                    // Marked as the results mark its play.
                    if c.assisted() {
                        format!("{label} · {}", msg::assist_mark())
                    } else {
                        label
                    }
                }
                None => msg::play_no_charts().into(),
            };
            super::set(root, "Play", "Label", &label);
            return;
        }
        let w = self.layout.song[2];
        let top = self.layout.ladder[0] - self.layout.song[1];
        let gap = env.metric("Select.Ladder.Gap", 5.0);
        // Rows already made at this size are filled again: they carry
        // every difficulty's and lamp's command.
        let mut made = match hit::find_mut(root, "Ladder").map(|f| &mut f.kind) {
            Some(Kind::Frame(rows)) if same_size => std::mem::take(rows),
            _ => Vec::new(),
        }
        .into_iter()
        .filter(|a| a.name.starts_with("Ladder.Row"));
        let mut children = Vec::new();
        for k in 0..n {
            let c = charts[first + k];
            let mut r = made.next().unwrap_or_else(|| {
                env.component(
                    "ChartLadder.Row",
                    "List.Row",
                    &Spec::new([w, row], c.difficulty.name()),
                    &format!("Ladder.Row{k}"),
                    [0.0, top + k as f32 * (row + gap)],
                )
            });
            // The difficulty (never translated) or the chart's own name.
            set_text(&mut r, "Label", &data(&c.label().to_uppercase()));
            set_text(&mut r, "Title", &data(&c.label()));
            let mut notes: String = if c.credit.is_empty() {
                msg::chart_notes(c.notes)
            } else {
                msg::chart_notes_by(c.notes, &c.credit)
            }
            .into();
            // A generated version says what it leaves out or where it
            // comes from.
            if let Some(version) = c.version {
                notes = format!("{} · {notes}", version_name(version));
            }
            set_text(&mut r, "Notes", &notes);
            set_text(&mut r, "Subtitle", &notes);
            set_text(&mut r, "Meter", &c.meter_text());
            // The best: grade, score, lamp (the ruleset's readouts);
            // marked while it waits to be worked out again.
            match &c.best {
                Some(b) => {
                    set_text(&mut r, "Grade", &data(&b.grade));
                    let score = if b.pending {
                        format!("{} …", b.score)
                    } else {
                        b.score.clone()
                    };
                    set_text(&mut r, "Score", &data(&score));
                    set_text(&mut r, "LampText", &data(&b.lamp_text));
                    crate::widgets::play(&mut r, &format!("Lamp.{}", b.lamp));
                }
                None => {
                    for p in ["Grade", "Score", "LampText"] {
                        set_text(&mut r, p, "");
                    }
                    crate::widgets::play(&mut r, "Lamp.None");
                }
            }
            crate::widgets::play(&mut r, c.difficulty.name());
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
            let none = msg::no_charts(&style_name);
            let mut e = env.component(
                "ChartLadder.Empty",
                "Panel",
                &Spec::new([w, 150.0], none.as_str()),
                "Ladder.Empty",
                [0.0, top],
            );
            set_text(&mut e, "Title", &none);
            set_text(&mut e, "Hint", &msg::no_charts_hint());
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
        if self.layout.chips {
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
        let style = self.style_id();
        match self.selected_item() {
            Some(Item::Header(_)) => Hints::Header,
            Some(Item::Song(_))
                if !self
                    .song()
                    .is_some_and(|s| s.charts.iter().any(|c| c.style == style)) =>
            {
                Hints::Greyed
            }
            _ => Hints::Song,
        }
    }

    fn in_header(&self, cx: &Cx) -> bool {
        cx.focused().is_some_and(|f| {
            f == self.search.name
                || f.starts_with("Mod")
                || matches!(
                    f,
                    "ClearSearch"
                        | "Volume"
                        | "Back"
                        | "AddIcon"
                        | "SearchIcon"
                        | "OptionsIcon"
                        | "More"
                )
        })
    }

    fn fill_footer(&mut self, root: &mut Actor, cx: &mut Cx) {
        if self.layout.compact {
            self.env.state.borrow_mut().hints.clear();
            return;
        }
        let now = self.hints_now();
        let searching = !self.search.text.is_empty();
        let manage = self.can_manage();
        // While searching (not typing), what it found: worded once it
        // changes, not every frame.
        let found = match (&self.matches, now) {
            _ if self.gesture_note > 0.0 && now != Hints::Typing => Ok(self.gesture_text.clone()),
            (_, Hints::Typing) | (None, _) => Ok(String::new()),
            (Some(_), _) => Err(self.match_count),
        };
        let in_header = self.in_header(cx);
        let key = (now, searching, found, manage, in_header);
        if self.hints.as_ref() == Some(&key) {
            return;
        }
        let found = match &key.2 {
            Ok(text) => text.clone(),
            Err(count) => msg::matches(*count).into(),
        };
        self.hints = Some(key);
        if let Some(old) = hit::find_mut(root, "TabKey") {
            let mut hint = if cx.hints().active() {
                let caps = cx.hints().actions(&[HintAction::Select]);
                if caps.is_empty() {
                    frame("TabKey", Vec::new())
                } else {
                    let (mut a, w) = super::key_hint(&self.env, "TabKey", &caps, "", cx);
                    call(
                        &mut a,
                        "xy",
                        &[
                            self.layout.song[2] - w / 2.0,
                            self.layout.tabs_y - self.layout.song[1] + 16.0,
                        ],
                    );
                    a
                }
            } else {
                right(
                    self.env
                        .numbers("TabKey", &UiText::data(TAB_LEGEND), 11.0, "Muted"),
                    self.layout.song[2],
                    self.layout.tabs_y - self.layout.song[1] + 16.0,
                )
            };
            hint.name = "TabKey".into();
            *old = hint;
        }
        // The hints' and links' words, in the language now.
        let (song, chart, style, play, search) = (
            msg::hint_song(),
            msg::hint_chart(),
            msg::hint_style(),
            match self.room_play_hint() {
                Some(Some(hint)) => String::from(hint),
                Some(None) => String::new(),
                None => msg::hint_play().into(),
            },
            msg::hint_search(),
        );
        let (open_close, choose, go_to, close) = (
            msg::hint_open_close(),
            msg::hint_choose(),
            msg::hint_go_to(),
            ddi_l10n::msg::common::close(),
        );
        let (manage_hint, plays, edit) = (msg::hint_manage(), msg::hint_plays(), msg::hint_edit());
        let (navigate, more, activate) = (
            msg::hint_navigate(),
            ddi_l10n::msg::common::more(),
            msg::hint_activate(),
        );
        let esc = if self.room.is_some() {
            ddi_l10n::msg::room::hint_back()
        } else {
            msg::hint_start_menu()
        };
        let typing_note = msg::typing_note();
        let (link_manage, link_add, link_options, link_calibrate) = (
            msg::link_manage(),
            msg::link_add(),
            msg::link_options(),
            ddi_l10n::msg::common::calibrate(),
        );
        // The plays are P's (its hint); the credits and the source are
        // the start menu's and the credits'.
        let shared = self.fixed_list();
        let mut links: Vec<(&str, &str)> = Vec::new();
        if manage && !shared {
            links.push(("Link.Manage", link_manage.as_str()));
        }
        if !shared {
            links.push(("Link.Add", link_add.as_str()));
        }
        links.extend([
            ("Link.Options", link_options.as_str()),
            ("Link.Calibrate", link_calibrate.as_str()),
        ]);
        /// A key hint: its keys' legends and its words.
        fn h<'a>(
            keys: &'static [&'static str],
            label: &'a str,
        ) -> (&'static [&'static str], &'a str) {
            (keys, label)
        }
        let (mut hints, note): (Vec<(&[&str], &str)>, &str) = match now {
            Hints::Song => (
                vec![
                    h(&["↑", "↓"], &song),
                    h(&["↑", "↓"], &chart),
                    h(&["TAB"], &style),
                    h(&["ENTER"], &play),
                    h(&["/"], &search),
                    h(&["ESC"], &esc),
                ],
                "",
            ),
            Hints::Greyed => (
                vec![
                    h(&["↑", "↓"], &song),
                    h(&["TAB"], &style),
                    h(&["/"], &search),
                    h(&["ESC"], &esc),
                ],
                "",
            ),
            Hints::Header if searching => (
                vec![h(&["↑", "↓"], &song), h(&["/"], &search), h(&["ESC"], &esc)],
                "",
            ),
            Hints::Header => (
                vec![
                    h(&["↑", "↓"], &song),
                    h(&["ENTER"], &open_close),
                    h(&["/"], &search),
                ],
                "",
            ),
            Hints::Typing => (
                vec![
                    h(&["↑", "↓"], &choose),
                    h(&["ENTER"], &go_to),
                    h(&["ESC"], &close),
                ],
                typing_note.as_str(),
            ),
        };
        let links = if now == Hints::Typing {
            &[][..]
        } else {
            &links[..]
        };
        // Delete (or ⌘/Ctrl Backspace) manages what was imported.
        if manage && now != Hints::Typing {
            hints.push(h(&["DEL"], &manage_hint));
        }
        if self.plays.is_some() && now == Hints::Song {
            hints.push(h(&["P"], &plays));
        }
        if self.editing && !shared && now == Hints::Song && !self.layout.compact {
            hints.push(h(&["E"], &edit));
        }
        // A room's follower picks nothing: no Enter.
        hints.retain(|(_, label)| !label.is_empty());
        // A room that plays one style has none to change to: no Tab.
        if self.room.is_some() && self.data.styles.len() <= 1 {
            hints.retain(|(keys, _)| keys != &["TAB"]);
        }
        // A share's name and its time left, where nothing else is said.
        let shared_note: Option<String> = self.data.shared.as_ref().map(|v| {
            match &v.expires {
                Some(e) => msg::shared_note_expires(&v.pack, e),
                None => msg::shared_note(&v.pack),
            }
            .into()
        });
        // The songs added to the next share, where nothing else is said.
        let list_note: Option<String> = self
            .share_list()
            .map(|(_, songs, _)| msg::share_list_note(songs.len()).into());
        let note = match (note, found.as_str(), &shared_note, &list_note) {
            ("", "", Some(n), _) | ("", "", None, Some(n)) => n.as_str(),
            ("", f, _, _) => f,
            (n, _, _, _) => n,
        };
        let mut hints: Vec<_> = hints
            .iter()
            .map(|&(keys, label)| FooterHint {
                keys,
                label,
                prefix: "",
            })
            .collect();
        if now == Hints::Song {
            hints[1].prefix = "2×";
        }
        let mut controller: Vec<(&[HintAction], &str)> = vec![(
            &[
                HintAction::Up,
                HintAction::Down,
                HintAction::Left,
                HintAction::Right,
            ],
            if in_header {
                navigate.as_str()
            } else {
                song.as_str()
            },
        )];
        if now == Hints::Song && !in_header {
            controller.push((&[HintAction::Previous, HintAction::Next], chart.as_str()));
        }
        if now != Hints::Typing {
            controller.extend([
                (&[HintAction::Select][..], style.as_str()),
                (&[HintAction::More][..], more.as_str()),
            ]);
        }
        controller.push((&[HintAction::Confirm, HintAction::Start], activate.as_str()));
        controller.push((
            &[HintAction::Back],
            if now == Hints::Typing {
                close.as_str()
            } else {
                esc.as_str()
            },
        ));
        let mut f = footer_hints(
            &self.env,
            self.layout.screen,
            &hints,
            &controller,
            note,
            links,
            cx,
        );
        if let Some(old) = hit::find_mut(root, "Footer") {
            super::refill(old, f);
        } else if let Kind::Frame(c) = &mut root.kind {
            // Its first hints come after the screen's `On`: it comes in
            // on its own, a frame later, as the header does (sliding in
            // with it, or shown at once where the bars stayed put).
            super::transitions(&mut f, &self.env, &[("Footer", super::Edge::FooterBar)]);
            let entering = c
                .iter()
                .find(|a| a.name == "Header")
                .is_some_and(|h| h.tween_time_left() > 0.0);
            if cx.holding() {
                super::hold_entrances(&mut f);
            }
            let bars = if cx.holding() || entering {
                crate::ui::BARS_ON
            } else {
                crate::ui::BARS_STAY
            };
            f.play(bars, &mut ddi_actor::Problems::default());
            c.push(f);
        }
    }

    /// A flick or a drag passes rows over the wheel's line while the
    /// selected row stays where it was (the list selects what is on its
    /// line once at rest): the room on the line for the selected row, and
    /// the row's own growth, are there only while the selected row is on
    /// the line and the list moves slower than [`FLICK_ROWS`] a second;
    /// they close and open together, so a flick leaves no gap.
    fn flick_room(&mut self, dt: f32) {
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
        if self.room.is_some() {
            return None;
        }
        let pack = |name: Option<&str>| {
            self.data
                .packs
                .iter()
                .find(|p| p.name.as_deref() == name)
                .cloned()
                .unwrap_or_else(|| PackEntry {
                    name: name.map(str::to_string),
                    ..Default::default()
                })
        };
        let count = |name: Option<&str>| {
            self.packs
                .iter()
                .find(|(n, _)| n.as_deref() == name)
                .map_or(0, |(_, v)| v.len())
        };
        match self.selected_item()? {
            Item::Header(p) => {
                let name = self.packs.get(p)?.0.as_deref();
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
                    pack: pack(song.pack.as_deref()),
                    songs: count(song.pack.as_deref()),
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
                // The notice's count, wherever its language puts it.
                let count = self.notices[i]
                    .text
                    .split(|c: char| !c.is_ascii_digit())
                    .find(|n| !n.is_empty())
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
            ManageDialog::new(self.env.clone(), m, self.shell.clone(), self.busy()),
            true,
        );
    }

    /// The quick volume moved: the preview follows at once, the settings
    /// once it settles.
    fn volume_changed(&mut self, c: VolumeChange, cx: &mut Cx) {
        if c.commit {
            let mut s = crate::state::settings::Settings::load();
            if s.volume != c.gain {
                s.volume = c.gain;
                s.save();
            }
        }
        self.status(format!("volume {:.0}%", self.volume.position() * 100.0));
        self.call(SelectCall::Volume(c.gain), cx);
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
        if !self.told_shown {
            self.told_shown = true;
            self.call(SelectCall::Shown(true), cx);
            // 5.1's `ScreenSelectMusic::BeginScreen`: at every showing.
            cx.request(UiRequest::Announce(Line::SelectMusicIntro));
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
            open: self.place_open(),
            song: self.place_song().cloned(),
            chart,
            style: self.style_id().to_string(),
            search: self.search.text.clone(),
            closed: self.place_closed().cloned().collect(),
        };
        self.told_place = Some(place.clone());
        self.told_packs = (self.open_pack, self.packs_version);
        self.call(SelectCall::Place(place), cx);
    }

    /// The list's place's open pack, by its key.
    fn place_open(&self) -> Option<String> {
        let (name, _) = self.packs.get(self.open_pack?)?;
        Some(pack_key(name.as_deref()))
    }

    /// The list's place's song: the selected row's.
    fn place_song(&self) -> Option<&String> {
        match self.selected_item() {
            Some(Item::Song(i)) => Some(&self.data.songs[i].id),
            _ => None,
        }
    }

    /// The list's place's closed packs, by name.
    fn place_closed(&self) -> impl Iterator<Item = &Option<String>> {
        self.packs
            .iter()
            .enumerate()
            .filter(|&(p, _)| Some(p) != self.open_pack)
            .map(|(_, (n, _))| n)
    }

    /// Whether `told` is the list's place now (with `chart` picked),
    /// without building it.
    fn place_matches(&self, told: &SelectPlace, chart: Option<usize>) -> bool {
        self.told_packs == (self.open_pack, self.packs_version)
            && told.song.as_ref() == self.place_song()
            && told.chart == chart
            && told.style == self.style_id()
            && told.search == self.search.text
    }

    /// What the shell sent: new data, the import's progress, notices.
    fn take_feed(&mut self, root: &mut Actor, cx: &mut Cx) {
        let Some(feed) = self.feed.clone() else {
            return;
        };
        let (data, import, share, notices, open, overlay, movie) = {
            let mut f = feed.borrow_mut();
            (
                f.data.take(),
                f.import.take(),
                f.share.take(),
                f.notices.take(),
                std::mem::take(&mut f.open_import),
                f.overlay.take(),
                f.preview_movie.take(),
            )
        };
        if let Some(s) = share {
            // The list went up: it is done with.
            if s.state == super::data::ShareState::Done {
                self.to_share.clear();
                self.hints = None;
                self.publish_dirty = true;
            }
            *self.share.borrow_mut() = s;
            cx.broadcast(Message::new(SHARE_CHANGED));
        }
        if let Some(movie) = movie
            && movie != self.movie
        {
            self.movie = movie;
            self.fill_info(root, cx);
            self.publish_dirty = true;
        }
        // A room's follower shows the host's song, not its own list.
        if let Some(d) = data.filter(|_| !self.following()) {
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
        look::ellipsized(&mut t, room);
        children.push(left(t, x0, y));
        let mut x = x0 + look::mono_width(&text, 13.0).min(room) + 8.0;
        for (i, label) in links {
            let lw = look::mono_width(label, 13.0) + 16.0;
            children.push(env.component(
                "Link",
                "Button",
                &Spec::new([lw, 24.0], label),
                &format!("Notice.Link{i}"),
                [x + lw / 2.0, y],
            ));
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

    /// Opens or closes pack `p`, its header staying where it shows (a
    /// press while the wheel still moves to it included) and gliding on
    /// to the line from there.
    fn toggle_pack(&mut self, p: usize) {
        let shown = self.list.selected().and_then(|i| self.list.item_top(i));
        self.open_pack = (self.open_pack != Some(p)).then_some(p);
        self.lay_out_items();
        self.reload();
        let at = self.items.iter().position(|it| *it == Item::Header(p));
        if let Some(at) = at {
            if let Some(top) = shown {
                self.list.keep_item_at(at, top);
            }
            self.list.select(at);
        }
        let name = pack_label(self.packs[p].0.as_deref());
        self.status(format!(
            "pack {name} {}",
            if self.open_pack == Some(p) {
                "opened"
            } else {
                "closed"
            }
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
                            chart.difficulty.name(),
                            chart.meter
                        ));
                        let env = self.env.clone();
                        // A room's pick is no play to comment on.
                        if self.room.is_some() {
                            self.room_start(&song, &chart, cx);
                            return;
                        }
                        let started = ddi_platform::storage::started_ms();
                        cx.request(UiRequest::Announce(Line::SelectMusic(comment(
                            &chart, started,
                        ))));
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
        if self.room.is_some() {
            // The room's preview stops as song select's does.
            self.told_shown = false;
            self.call(SelectCall::Shown(false), cx);
            self.room_back(cx);
            return Handled::Yes;
        }
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
            self.open_pack = Some(p);
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
        // The focus first: typing refused (a controller's press in a
        // browser) then finds it on the field.
        cx.focus(self.search.name.clone());
        let activate = Input::Activate {
            name: self.search.name.clone(),
        };
        let suggest = |_: &str| Vec::new();
        self.search.input(&activate, root, cx, &suggest);
        self.typing = true;
        if let Some(f) = hit::find_mut(root, "Search") {
            f.visible = true;
        }
    }

    /// A controller's button while typing (keys are the field's): the
    /// field is left with its text and its filter.
    fn leave_typing(&mut self, root: &mut Actor, cx: &mut Cx) {
        self.search.stop(root, cx);
        self.typing = false;
        self.hints = None;
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
        if self.room.is_some() {
            let mut items: Vec<String> = self.focusables().into_iter().map(|f| f.name).collect();
            // The players beside the song (a phone's strip): taps open
            // the full list.
            items.extend(
                [
                    "RoomPlayers",
                    super::room::PLAYERS_MORE,
                    super::room::PLAYERS_STRIP,
                ]
                .map(String::from),
            );
            self.room_publish(items);
        }
        let mut st = self.env.state.borrow_mut();
        st.screen = "select".into();
        st.compact = self.layout.compact;
        st.autoplay = self.autoplay;
        st.reduced_motion = self.reduced;
        st.style = self.style_name().to_string();
        st.preferred = self.preferred.name().to_string();
        let (charts, pick) = self.charts();
        st.chart = pick.map(|p| format!("{} {}", charts[p].difficulty.name(), charts[p].meter));
        st.ladder = charts
            .iter()
            .map(|c| {
                let chart = format!("{} {}", c.difficulty.name(), c.meter);
                match c.version {
                    Some(version) => format!("{chart} ({})", version.key()),
                    None => chart,
                }
            })
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
                st.header = Some(pack_label(self.packs[p].0.as_deref()).into());
            }
            _ => st.header = None,
        }
        let song = self.song();
        st.song = song.map(|s| s.title.clone()).unwrap_or_default();
        st.song_id = song.map(|s| s.id.clone()).unwrap_or_default();
        st.shared_pack = self.data.shared.as_ref().map(|v| v.pack.clone());
        st.to_share = self.to_share.clone();
        st.items = self.items.len();
        st.closed = self
            .place_closed()
            .map(|n| pack_label(n.as_deref()).into())
            .collect();
        st.search = self.search.text.clone();
        st.popup = self.search.popup_open();
        st.suggestions = self
            .search
            .suggestions
            .iter()
            .map(|(t, _)| t.clone())
            .collect();
        st.backdrop = self.backdrop.source().map(str::to_string);
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
        drop(st);
        self.publish_rows();
    }

    /// What the debug view says of the rows showing, each frame the list
    /// moves.
    fn publish_rows(&self) {
        let mut st = self.env.state.borrow_mut();
        let style = self.style_id();
        let shown = self.list.shown();
        st.greyed_rows = shown
            .iter()
            .filter(|&&i| matches!(self.items.get(i), Some(Item::Song(s)) if !self.data.songs[*s].charts.iter().any(|c| c.style == style)))
            .count();
        st.selected_row = self.list.selected().and_then(|i| self.list.item_actor(i));
    }
}

/// What a song is searched by: its fields, its charts' credits and
/// names, and its transliterations.
fn search_fields(s: &SongEntry) -> ([&str; 4], impl Iterator<Item = &str> + '_, [&str; 2]) {
    (
        [
            s.title.as_str(),
            s.artist.as_str(),
            s.pack.as_deref().unwrap_or_default(),
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
    packs: &[(Option<String>, Vec<usize>)],
    data: &SelectData,
    typed: &str,
) -> Vec<(String, String)> {
    let f = |s: &str| fold(s);
    let mut built = built.borrow_mut();
    let s = built.get_or_insert_with(|| {
        let mut cands: Vec<(&str, &str, SuggestKind)> = Vec::new();
        // The bundled songs' pack is no data to search.
        for (p, _) in packs {
            if let Some(p) = p {
                cands.push((p, "", SuggestKind::Pack));
            }
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
            // The suggestion as typed into the field (data, never marked),
            // its kind a message.
            let kind = match s.kind {
                SuggestKind::Title => msg::suggest_title(),
                SuggestKind::Artist => msg::suggest_artist(),
                SuggestKind::Pack => msg::suggest_pack(),
            };
            (s.text, kind.into())
        })
        .collect()
}

/// The wheel's model over the screen's fields (borrowed apart from its
/// list).
fn wheel<'a>(
    items: &'a [Item],
    data: &'a SelectData,
    packs: &'a [(Option<String>, Vec<usize>)],
    open_pack: Option<usize>,
    matches: Option<&'a [bool]>,
    style: usize,
    compact: bool,
) -> Wheel<'a> {
    let st = data.styles.get(style);
    Wheel {
        items,
        data,
        packs,
        open_pack,
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
    fn shared_bars(&self) -> bool {
        true
    }

    fn active_element(&self, _focused: Option<&str>) -> Option<String> {
        self.list.selected().and_then(|i| self.list.item_actor(i))
    }

    /// The header's name of the screen; the chart chosen, on the ladder
    /// (a button of the compact layout's strip); its level; its best
    /// (grade, score, lamp), or where it would be (the row's `Best`) when
    /// it has none.
    fn spot(&self, what: crate::Mark) -> Option<crate::Spot> {
        use crate::Mark;
        if what == Mark::Screen {
            return Some(crate::Spot::new("Header", &["Title"]));
        }
        let (first, n) = self.ladder;
        let (charts, pick) = self.charts();
        let pick = pick?;
        let k = pick.checked_sub(first).filter(|k| *k < n)?;
        let played = charts.get(pick).is_some_and(|c| c.best.is_some());
        let row = if self.layout.chips {
            format!("Chart{k}")
        } else {
            format!("Ladder.Row{k}")
        };
        let parts: &[&str] = match what {
            Mark::Chart => &[],
            Mark::ChartLevel => &["Meter"],
            Mark::ChartBest if self.layout.chips => &[],
            Mark::ChartBest if played => &["Grade", "Score", "LampText"],
            Mark::ChartBest => &["Best"],
            _ => return None,
        };
        Some(crate::Spot::new(row, parts))
    }

    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn cursor(&self, target: &crate::hit::Target) -> Option<crate::request::Cursor> {
        self.search.cursor(target)
    }

    fn name(&self) -> &str {
        "select"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        let compact = super::is_compact(&*self.env.look, vp);
        self.reduced = self.env.reduced();
        self.layout = self.fit_layout([vp.width, vp.height], compact);
        self.build_tree(cx)
    }

    fn focusables(&self) -> Vec<Focusable> {
        // The header's row in order, explicitly (the wheel below lies
        // nearer than the next thing to the right): the search, its Clear
        // while it shows, the mods.
        let n = self.data.mods.len();
        let clear = self.typing || !self.search.text.is_empty();
        let row: Vec<String> = if self.layout.compact {
            Vec::new()
        } else {
            let room: Vec<String> = if self.room.is_some() {
                vec!["Room.Rules".into(), "Room.Players".into()]
            } else {
                Vec::new()
            };
            let mods = if self.room.is_some() { 0 } else { n };
            room.into_iter()
                .chain((!self.following()).then(|| self.search.name.clone()))
                .chain(clear.then(|| "ClearSearch".to_string()))
                .chain((0..mods).map(|i| format!("Mod{i}")))
                .chain(["Volume".to_string(), "More".to_string()])
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
        // A fresh Up on the list's first row: the search field (a phone's
        // goes up to the icons over it).
        let mut wheel = Focusable::new_list("Wheel");
        wheel.up = Some(if self.layout.compact {
            "More".into()
        } else {
            self.search.name.clone()
        });
        let mut v = vec![wheel, along(&self.search.name), along("ClearSearch")];
        let shared = self.fixed_list();
        if self.layout.compact {
            for n in [
                "Back",
                "AddIcon",
                "SearchIcon",
                "More",
                "StyleButton",
                "Play",
            ] {
                if (shared && n == "AddIcon")
                    || (self.following() && matches!(n, "SearchIcon" | "Play"))
                {
                    continue;
                }
                v.push(Focusable::new(n));
            }
            v.extend((0..5).map(|k| Focusable::new(format!("Chart{k}"))));
        } else {
            for n in [
                "Back",
                "Link.Manage",
                "Link.Add",
                "Link.Options",
                "Link.Calibrate",
            ] {
                if shared && matches!(n, "Link.Manage" | "Link.Add") {
                    continue;
                }
                v.push(Focusable::new(n));
            }
            if self.layout.chips {
                // A room's: the style button and the chart buttons.
                if self.style_button_shown() {
                    v.push(Focusable::new("StyleButton"));
                }
                v.extend((0..5).map(|k| Focusable::new(format!("Chart{k}"))));
            } else {
                v.extend((0..self.data.styles.len()).map(|i| Focusable::new(format!("Tab{i}"))));
                v.extend((0..LADDER_ROWS).map(|k| Focusable::new(format!("Ladder.Row{k}"))));
            }
        }
        if !self.notices.is_empty() {
            v.push(Focusable::new("Notice"));
        }
        v.extend((0..self.notices.len()).map(|i| Focusable::new(format!("Notice.Link{i}"))));
        v.extend((0..n).map(|i| along(&format!("Mod{i}"))));
        if !self.layout.compact {
            if self.room.is_some() {
                v.push(along("Room.Rules"));
                v.push(along("Room.Players"));
            }
            v.push(along("Volume"));
            v.push(along("More"));
        }
        if self.following() {
            v.push(Focusable::new("Panel.Up"));
            v.push(Focusable::new("Panel.Down"));
        }
        v
    }

    fn direction_edges(&self) -> bool {
        true
    }

    fn owns_direction_repeat(&self) -> bool {
        self.navigation.holding()
    }

    fn moving(&self) -> bool {
        // The wheel gliding, flicked or opening its room by hand.
        self.navigation.animating()
            || self.wheel_motion.is_some()
            || self.list.animating()
            || (!self.layout.compact && (self.open > 0.0 && self.open < 1.0))
    }

    fn wake_at(&self) -> Option<f64> {
        self.room_wake_at()
    }

    fn animating(&self) -> bool {
        self.live.animating(&self.env)
            || self.navigation.animating()
            || self.wheel_motion.is_some()
            || self.list.animating()
            || self.volume.animating()
            || (!self.layout.compact && (self.open > 0.0 && self.open < 1.0))
    }

    fn shown(&mut self, shown: bool, _root: &mut Actor, cx: &mut Cx) {
        if shown {
            // The song shown now told again (Options' sample may have
            // changed the player's), then that it shows.
            self.tell(cx);
        } else {
            self.cancel_navigation();
            self.told_song = None;
            self.told_shown = false;
            self.call(SelectCall::Shown(false), cx);
        }
    }

    fn update(&mut self, dt: f32, root: &mut Actor, cx: &mut Cx) {
        self.room_update(root, cx);
        if !cx.is_current_screen() {
            return;
        }
        self.live.tick(root, "Wordmark", &self.env, cx);
        self.volume.update(cx.time(), root, self.reduced);
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
            }
        }
        if [vp.width, vp.height] != self.layout.vp
            || compact != self.layout.compact
            || self.env.reduced() != self.reduced
        {
            self.cancel_navigation();
            self.reduced = self.env.reduced();
            self.layout = self.fit_layout([vp.width, vp.height], compact);
            let selected = self.list.selected();
            let left = root.tween_time_left();
            *root = self.build_tree(cx);
            self.reload();
            if let Some(s) = selected {
                self.list.select(s);
                self.list.finish();
            }
            // The first frame of the new tree shows it as it is (or still
            // coming in).
            super::relaid(root, left, cx);
        }
        if let Some(m) = &self.wheel_motion {
            self.list.visual_scroll(m.sample(cx.time()).0);
            self.open = 1.0;
            self.env.look.set_wheel_open("Wheel", 1.0);
        } else {
            self.flick_room(dt);
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
        self.fill_footer(root, cx);
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
            // Options may have moved the volume meanwhile.
            if !self.layout.compact {
                let gain = crate::state::settings::Settings::load().volume;
                self.volume.set_gain(gain, root);
            }
        }
        // Only the rows change while the list glides.
        if std::mem::take(&mut self.publish_dirty) || event.is_some() {
            self.publish();
        } else if std::mem::take(&mut self.rows_dirty) || self.list.animating() {
            self.publish_rows();
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
                action:
                    MenuAction::Start
                    | MenuAction::Select
                    | MenuAction::More
                    | MenuAction::Previous
                    | MenuAction::Next,
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
            self.gesture_text = msg::gesture_link().into();
            self.hints = None;
            self.fill_footer(root, cx);
            self.status("press Enter or tap to open".into());
            self.publish();
            return Handled::Yes;
        }
        // The search field a controller activated: a browser starts
        // typing only from a key or a tap (a phone's keyboard opens only
        // then). The field keeps the focus: Right reaches Clear, Down
        // the list.
        if let Input::NeedsGesture(UiRequest::StartTextInput { .. }) = input {
            self.typing = false;
            self.search.refused(root);
            self.gesture_note = 4.0;
            self.gesture_text = msg::gesture_search().into();
            self.hints = None;
            self.fill_footer(root, cx);
            self.sync_field(root);
            // A phone's field shows only while typed into: back on its icon.
            if self.layout.compact && cx.focused() == Some(self.search.name.as_str()) {
                cx.focus("SearchIcon");
            }
            self.status("press / or tap the field to type".into());
            self.publish();
            return Handled::Yes;
        }
        let h = match self.room_input(input, root, cx) {
            Some(h) => h,
            None => self.input_inner(input, root, cx),
        };
        // The song the selection shows, now: a Start in the same poll as a
        // move acts on the song moved to.
        self.follow();
        self.sync_field(root);
        self.rows_dirty = true;
        self.publish();
        // Tell the shell synchronously for changes committed by this input.
        self.tell(cx);
        self.from_pad = false;
        h
    }

    fn message(&mut self, m: &Message, root: &mut Actor, cx: &mut Cx) {
        if m.name == crate::controller_hints::CHANGED {
            self.hints = None;
            self.fill_footer(root, cx);
        }
        self.room_message(m, root, cx);
        if let Some(id) = super::menu_pick(m, "Select.Suggestion")
            && let Ok(i) = id.parse::<i64>()
        {
            let pick = Message::new(crate::widgets::field::PICK_MESSAGE)
                .with(
                    "Field",
                    crate::message::Param::Str(self.search.name.clone()),
                )
                .with("Item", crate::message::Param::Int(i));
            if let Some(e) = self.search.message(&pick, root, cx) {
                self.field_event(e, root, cx);
            }
        }
        if let Some(id) = super::menu_pick(m, "Select.More") {
            self.activate(id, root, cx);
        }
        if m.name == FEED {
            self.take_feed(root, cx);
        }
        if m.name == NOTICE_LINK
            && let Some(i) = m.int("Index")
        {
            self.notice_link(i as usize, cx);
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
        if matches!(
            input,
            Input::Menu {
                action: MenuAction::More,
                ..
            }
        ) {
            self.leave_typing(root, cx);
            self.open_more(cx);
            return Handled::Yes;
        }
        if !self.layout.compact {
            let (h, change) = self.volume.input(input, root, cx, self.reduced);
            if let Some(c) = change {
                self.volume_changed(c, cx);
            }
            if h == Handled::Yes {
                return h;
            }
        }
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
                return Handled::Yes;
            }
            // The chart's plays (P), its best's replay (V; W A S D move).
            Input::Key(k)
                if self.letter_shortcut(k)
                    && self.plays.is_some()
                    && (k.key.eq_ignore_ascii_case("p") || k.key.eq_ignore_ascii_case("v")) =>
            {
                if k.key.eq_ignore_ascii_case("p") {
                    self.open_plays(cx);
                } else {
                    self.watch_best(cx);
                }
                return Handled::Yes;
            }
            // The chart editor (E).
            Input::Key(k)
                if self.letter_shortcut(k)
                    && self.editing
                    && !self.fixed_list()
                    && k.key.eq_ignore_ascii_case("e") =>
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
        let in_header = self.in_header(cx);
        let on_search = cx.focused() == Some(self.search.name.as_str());
        // A controller's buttons on the search field (a keyboard's keys
        // are the field's while typing; its arrows browse the wheel).
        // Typing: Start goes to the songs found (as Enter), Back closes
        // the popup, then stops typing (as Escape), Down leaves for the
        // list, Left and Right along the header. Not typing: Start
        // types, Down or Back go back to the list where it was.
        if on_search
            && let Input::Menu { action, .. } = input
            && *action != MenuAction::Select
        {
            if self.typing {
                match action {
                    MenuAction::Up => self.open_suggestions(cx),
                    MenuAction::Start => {
                        let text = self.search.text.clone();
                        self.leave_typing(root, cx);
                        if text != self.filtered {
                            self.filter(&text);
                        }
                        cx.focus("Wheel");
                    }
                    MenuAction::Back => {
                        if !self.search.close_popup() {
                            self.leave_typing(root, cx);
                            self.stop_typing(cx);
                        }
                    }
                    MenuAction::Down => {
                        self.leave_typing(root, cx);
                        cx.focus("Wheel");
                    }
                    MenuAction::Left | MenuAction::Right => {
                        self.leave_typing(root, cx);
                        self.search_from = None;
                        return Handled::No;
                    }
                    _ => {}
                }
                return Handled::Yes;
            }
            match action {
                MenuAction::Down | MenuAction::Back => {
                    self.search_from = None;
                    cx.focus("Wheel");
                    return Handled::Yes;
                }
                // The field's own: typing.
                MenuAction::Start => return Handled::No,
                _ => {}
            }
        }
        // The header is the top: a fresh Up there goes past it, to the
        // app's notices (the focus stays, for Down to come back to).
        if in_header
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
        if in_header && matches!(input, Input::Menu { action, .. } if action.is_direction()) {
            return Handled::No;
        }
        // A fresh Up on the first row goes on up (the search field, then
        // the app's notices over the screen).
        if let Input::Menu {
            action: MenuAction::Up,
            repeat: false,
        } = input
            && self.list.selected().is_none_or(|i| i == 0)
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
                return h;
            }
        }
        match input {
            Input::Menu { action, .. } => match action {
                MenuAction::Previous | MenuAction::Next if !in_header => {
                    self.move_chart(
                        if *action == MenuAction::Previous {
                            -1
                        } else {
                            1
                        },
                        root,
                    );
                    Handled::Yes
                }
                MenuAction::Select => {
                    self.next_style(root);
                    Handled::Yes
                }
                MenuAction::Start => {
                    if cx.focused().unwrap_or("Wheel") == "Wheel" {
                        self.start(root, cx);
                        Handled::Yes
                    } else {
                        // The focused button's own.
                        Handled::No
                    }
                }
                // The search's Clear is the field's, and the volume
                // the header's: back to the list.
                MenuAction::Back if matches!(cx.focused(), Some("ClearSearch" | "Volume")) => {
                    cx.focus("Wheel");
                    Handled::Yes
                }
                MenuAction::Back => self.back(root, cx),
                _ => Handled::No,
            },
            _ => Handled::No,
        }
    }

    fn open_suggestions(&self, cx: &mut Cx) {
        let items = self
            .search
            .suggestions
            .iter()
            .enumerate()
            .map(|(i, (text, note))| (i.to_string(), format!("{} {note}", UiText::data(text))))
            .collect();
        super::action_menu(
            &self.env,
            &msg::suggestions_title(),
            items,
            "Select.Suggestion",
            cx,
        );
    }

    fn open_more(&self, cx: &mut Cx) {
        if self.room_more(cx) {
            return;
        }
        let mut items: Vec<(String, String)> =
            vec![("SearchIcon".into(), msg::more_search().into())];
        if !self.search.text.trim().is_empty() && !self.search.suggestions.is_empty() {
            items.push(("Suggestions".into(), msg::more_suggestions().into()));
        }
        if !self.search.text.is_empty() {
            items.push(("ClearSearch".into(), msg::more_clear_search().into()));
        }
        let (charts, pick) = self.charts();
        let chart = pick.and_then(|i| charts.get(i));
        if self.plays.is_some() && chart.is_some() {
            items.push(("History".into(), msg::more_history().into()));
        }
        if chart
            .and_then(|c| c.best.as_ref())
            .is_some_and(|b| !b.play.is_empty())
        {
            items.push(("WatchBest".into(), msg::more_watch_best().into()));
        }
        let shared = self.fixed_list();
        if self.editing && !shared && self.song().is_some() {
            items.push(("EditChart".into(), msg::more_edit().into()));
        }
        if self.manageable().is_some() && !shared {
            items.push(("Link.Manage".into(), msg::more_manage().into()));
        }
        // Sharing: the song shown added to (or taken off) the list, the
        // list shared, or the song or pack shown alone.
        if let Some(song) = self.shown_imported() {
            if self.to_share.contains(&song.id) {
                items.push(("ShareRemove".into(), msg::more_share_remove().into()));
            } else {
                items.push(("ShareAdd".into(), msg::more_share_add().into()));
            }
        }
        if let Some((_, songs, _)) = self.share_list() {
            items.push((
                "ShareSelected".into(),
                msg::more_share_list(songs.len()).into(),
            ));
            items.push(("ShareClear".into(), msg::more_share_clear().into()));
        } else if self.shareable().is_some() {
            let label = if self.shown_imported().is_some() {
                msg::more_share_song()
            } else {
                msg::more_share_pack()
            };
            items.push(("Share".into(), label.into()));
        }
        if charts.len() > 1 {
            items.extend([
                ("PreviousChart".into(), msg::more_previous_chart().into()),
                ("NextChart".into(), msg::more_next_chart().into()),
            ]);
        }
        items.extend(self.data.styles.iter().enumerate().map(|(i, style)| {
            (
                format!("Tab{i}"),
                msg::more_style(style_name(&style.id, &style.name)).into(),
            )
        }));
        if !shared {
            items.push(("Link.Add".into(), msg::more_add_songs().into()));
        }
        items.extend([
            ("Link.Options".into(), msg::more_options().into()),
            (
                "Link.Calibrate".into(),
                ddi_l10n::msg::common::calibrate().into(),
            ),
        ]);
        super::action_menu(&self.env, &msg::more_title(), items, "Select.More", cx);
    }

    fn activate(&mut self, name: &str, root: &mut Actor, cx: &mut Cx) -> Handled {
        if let Some(h) = self.room_activate(name, root, cx) {
            return h;
        }
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
            "More" => {
                self.open_more(cx);
                return Handled::Yes;
            }
            "Suggestions" => {
                self.open_suggestions(cx);
                return Handled::Yes;
            }
            "History" => self.open_plays(cx),
            "WatchBest" => self.watch_best(cx),
            "EditChart" => self.edit(cx),
            "PreviousChart" => self.move_chart(-1, root),
            "NextChart" => self.move_chart(1, root),
            "Link.Options" | "OptionsIcon" => {
                let env = self.env.clone();
                (self.options)(&env, cx);
            }
            "Link.Add" | "AddIcon" if self.fixed_list() => {}
            "Link.Add" | "AddIcon" => {
                // Back on the wheel once the dialog closes.
                cx.focus("Wheel");
                cx.overlay(
                    ImportDialog::new(self.env.clone(), self.import.clone(), self.shell.clone())
                        .with_new_song(self.editing),
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
            "Share" => {
                self.open_share(cx);
                return Handled::Yes;
            }
            "ShareAdd" | "ShareRemove" => {
                cx.focus("Wheel");
                if let Some(id) = self.shown_imported().map(|s| s.id.clone()) {
                    self.to_share.retain(|s| s != &id);
                    if name == "ShareAdd" {
                        self.to_share.push(id);
                    }
                    self.status(format!("{} to share", self.to_share.len()));
                    self.hints = None;
                    self.publish_dirty = true;
                }
                return Handled::Yes;
            }
            "ShareSelected" => {
                if let Some((pack, songs, title)) = self.share_list() {
                    self.open_share_of(pack, songs, title, cx);
                }
                return Handled::Yes;
            }
            "ShareClear" => {
                cx.focus("Wheel");
                self.to_share.clear();
                self.hints = None;
                self.publish_dirty = true;
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
        Handled::Yes
    }
}
