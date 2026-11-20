//! TESTING AID (the select-layout study, 2026-11-19): candidate layouts
//! of song select's song side, for the maintainer to choose from, drawn by
//! the real screen. Never on by default: chosen by the environment
//! variable `DDI_SELECT_LAYOUT` (native runs and tests) or the web's
//! address parameter `sllayout=` (`a`, `b`, `c` or `d`; anything else, or
//! nothing, is today's layout), or by [`set_select_layout`].
//!
//! Every candidate shows the song's one picture as a square (the look's
//! `SongCover`: the cover art whole over its own blurred copy; no cover,
//! the banner whole across the square over its blurred copy; neither, a
//! quiet plate with the title set in it) and the chart buttons in a row
//! with the chosen chart's lines, designed with the skill radar On; with
//! it Off the chosen chart's lines take the radar's place.
//!
//! - **A, Beside**: the square at the top left, the title block beside it;
//!   the style tabs, the chart buttons across; the chart's lines and the
//!   radar under them.
//! - **B, Twins**: the title block across the top; the square and the
//!   radar as two equal squares; the tabs and buttons; the chart's lines
//!   as one strip.
//! - **C, Column**: two columns: the square, the title block under it;
//!   beside them the tabs, the buttons, the chart's lines and the radar.
//! - **D, Card**: the title block across the top, the tabs and buttons;
//!   a card holding the square and the chart's lines; the radar beside it.
//!
//! A phone's and a room's short side take the square at the left with the
//! title beside it (`SongInfo.Short`), the same in every candidate.

use std::cell::Cell;

use ddi_actor::{Actor, Kind};

use super::{Layout, SelectScreen, data, version_name};
use crate::hit;
use crate::look::{self, Spec, call, quad};
use crate::screen::Cx;
use crate::screens::data::{ChartEntry, SongEntry};
use crate::screens::{frame, left, right};
use crate::widgets::{part, set_image, set_text};
use ddi_l10n::msg::select as msg;

/// A candidate layout of the song side (the study's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectLayout {
    Beside,
    Twins,
    Column,
    Card,
    /// Round 2: B refined (the radar always there; the squares as large as
    /// the side allows; a room's forms).
    TwinsTwo,
}

impl SelectLayout {
    pub const ALL: [SelectLayout; 4] = [
        SelectLayout::Beside,
        SelectLayout::Twins,
        SelectLayout::Column,
        SelectLayout::Card,
    ];

    /// From its letter or name (`a`, `beside`, …).
    pub fn parse(s: &str) -> Option<SelectLayout> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "a" | "beside" => SelectLayout::Beside,
            "b" | "twins" => SelectLayout::Twins,
            "c" | "column" => SelectLayout::Column,
            "d" | "card" => SelectLayout::Card,
            "b2" | "e" | "twins2" => SelectLayout::TwinsTwo,
            _ => return None,
        })
    }

    pub fn letter(self) -> char {
        match self {
            SelectLayout::Beside => 'a',
            SelectLayout::Twins => 'b',
            SelectLayout::Column => 'c',
            SelectLayout::Card => 'd',
            SelectLayout::TwinsTwo => 'e',
        }
    }

    /// Its name in the pictures' files (`a` … `d`, `b2`).
    pub fn name(self) -> &'static str {
        match self {
            SelectLayout::Beside => "a",
            SelectLayout::Twins => "b",
            SelectLayout::Column => "c",
            SelectLayout::Card => "d",
            SelectLayout::TwinsTwo => "b2",
        }
    }
}

/// How round 2 draws a song's preview movie (a still of it stands in for
/// the movie in the study's pictures): `a` whole in the square over the
/// cover's blur, `b` filling the square, `c` the square's place widened to
/// the movie's shape over the radar, `d` the whole side's quiet backdrop.
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewStill {
    pub song: String,
    pub still: String,
    pub mode: char,
}

/// A room's form in round 2: `1` the radar beside the players, `2` the
/// radar on demand (closed), `3` the same, open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomForm {
    Beside,
    OnDemand,
    OnDemandOpen,
    /// Round 3: three columns, a thin players column at the far left, the
    /// song side as solo B2 has it, the list narrower.
    Columns,
}

/// Round 3's players column's status icon (vector shapes; the words are
/// the Players pop-up's).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlayerIcon {
    /// Nothing to say (before a pick).
    Idle,
    WaitingSong,
    /// Loading or downloading, with its progress (0 to 1) where known.
    Loading(f32),
    LoadFailed,
    NotReady,
    Ready,
    Playing,
    /// Life ran out, playing on (Adaptive charts' step down).
    FailedPlaying,
    Out,
    SatOut,
    Left,
    Watching,
    Away,
}

impl PlayerIcon {
    pub const ALL: [PlayerIcon; 13] = [
        PlayerIcon::Idle,
        PlayerIcon::WaitingSong,
        PlayerIcon::Loading(0.6),
        PlayerIcon::LoadFailed,
        PlayerIcon::NotReady,
        PlayerIcon::Ready,
        PlayerIcon::Playing,
        PlayerIcon::FailedPlaying,
        PlayerIcon::Out,
        PlayerIcon::SatOut,
        PlayerIcon::Left,
        PlayerIcon::Watching,
        PlayerIcon::Away,
    ];
}

thread_local! {
    static COLUMN_W: Cell<f32> = const { Cell::new(180.0) };
    static SIDE_W: Cell<f32> = const { Cell::new(568.0) };
    static COLUMN_STYLE: Cell<ColumnStyle> = const { Cell::new(ColumnStyle::ROUND_3) };
    static PLAYERS_RIGHT: Cell<bool> = const { Cell::new(false) };
    static ROUND5: Cell<Round5> = const { Cell::new(Round5::OFF) };
    static STAGED_PLACES: std::cell::RefCell<Option<Vec<Option<u16>>>> = const { std::cell::RefCell::new(None) };
    static STAGED: std::cell::RefCell<Option<Vec<PlayerIcon>>> = const { std::cell::RefCell::new(None) };
    static STAGED_MARKS: std::cell::RefCell<Option<Vec<Option<PlayerMark>>>> = const { std::cell::RefCell::new(None) };
    static PREVIEW: std::cell::RefCell<Option<PreviewStill>> = const { std::cell::RefCell::new(None) };
    static ROOM_FORM: Cell<RoomForm> = const { Cell::new(RoomForm::Beside) };
}

/// TESTING AID: a still standing in for `song`'s preview movie, drawn as
/// `mode` says (round 2's options).
pub fn set_preview_still(p: Option<PreviewStill>) {
    PREVIEW.with(|c| *c.borrow_mut() = p);
}

/// Round 5's refinements (TESTING AID): the players column as a panel of
/// the frame (a line from the header to the footer, foldable to a rail),
/// the line under the title (Length, Notes, BPM), the leaders' places
/// before their names.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Round5 {
    pub on: bool,
    /// How far the panel is folded: 0 open, 1 the rail.
    pub fold: f32,
    /// The rail shows the state icons (else the count alone).
    pub rail_icons: bool,
    /// The match's places: 0 at the right for everyone; 1 the top three
    /// marked before the name, the rest at the right; 2 the top three
    /// marked, the rest none.
    pub leaders: u8,
}

impl Round5 {
    pub const OFF: Round5 = Round5 {
        on: false,
        fold: 0.0,
        rail_icons: true,
        leaders: 0,
    };
}

/// The panel's rail's width.
pub const RAIL: f32 = 40.0;

/// TESTING AID: round 5's refinements.
pub fn set_round5(r: Round5) {
    ROUND5.with(|c| c.set(r));
}

/// TESTING AID: the players' places in the match, staged (each row, round).
pub fn set_staged_places(p: Option<Vec<Option<u16>>>) {
    STAGED_PLACES.with(|c| *c.borrow_mut() = p);
}

fn round5() -> Round5 {
    ROUND5.with(Cell::get)
}

/// Round 4's players column's style (the round 3 look by default).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnStyle {
    /// The state icons in one ink (shapes alone tell them apart).
    pub mono: bool,
    /// With `mono`: ready in green and a failed load in red, the only
    /// colors.
    pub accents: bool,
    /// The role marks (host, co-host, choosing) before the name; a CPU's
    /// level and a bot's head stay at the right.
    pub marks_before: bool,
    /// With `marks_before`: a mark's slot kept on every row (the names in
    /// one column).
    pub reserved: bool,
    /// With `marks_before`: room for two marks (host and choosing).
    pub two: bool,
    /// The crowns keep their gold and silver (else the marks' one ink).
    pub crown_color: bool,
}

impl ColumnStyle {
    pub const ROUND_3: ColumnStyle = ColumnStyle {
        mono: false,
        accents: false,
        marks_before: false,
        reserved: false,
        two: false,
        crown_color: true,
    };
}

/// TESTING AID: round 4's players column's style.
pub fn set_column_style(s: ColumnStyle) {
    COLUMN_STYLE.with(|c| c.set(s));
}

/// TESTING AID: round 4's room song side's width (its squares half of it
/// less 8), and the players column at the screen's right (after the list)
/// rather than at its left.
pub fn set_room_side(width: f32, players_right: bool) {
    SIDE_W.with(|c| c.set(width));
    PLAYERS_RIGHT.with(|c| c.set(players_right));
}

/// TESTING AID: round 3's players column's width.
pub fn set_column_width(w: f32) {
    COLUMN_W.with(|c| c.set(w));
}

/// TESTING AID: the players column's rows' icons staged (each row but the
/// local player's takes the next of `icons`, round), to show every state.
pub fn set_staged_icons(icons: Option<Vec<PlayerIcon>>) {
    STAGED.with(|c| *c.borrow_mut() = icons);
}

/// TESTING AID: the players column's rows' marks staged (each row, round).
pub fn set_staged_marks(marks: Option<Vec<Option<PlayerMark>>>) {
    STAGED_MARKS.with(|c| *c.borrow_mut() = marks);
}

/// TESTING AID: which of round 2's room forms is drawn.
pub fn set_room_form(f: RoomForm) {
    ROOM_FORM.with(|c| c.set(f));
}

fn preview_for(song: &str) -> Option<PreviewStill> {
    PREVIEW.with(|c| c.borrow().clone().filter(|p| p.song == song))
}

thread_local! {
    /// The candidate chosen (`None` inside: today's); unread until asked.
    static CHOSEN: Cell<Option<Option<SelectLayout>>> = const { Cell::new(None) };
}

/// The candidate chosen, if any (read from `DDI_SELECT_LAYOUT` natively
/// the first time).
pub fn select_layout() -> Option<SelectLayout> {
    CHOSEN.with(|c| {
        if let Some(v) = c.get() {
            return v;
        }
        #[cfg(not(target_arch = "wasm32"))]
        let v = std::env::var("DDI_SELECT_LAYOUT")
            .ok()
            .and_then(|s| SelectLayout::parse(&s));
        #[cfg(target_arch = "wasm32")]
        let v = None;
        c.set(Some(v));
        v
    })
}

/// Chooses a candidate (`None`: today's layout) for the screens built
/// after it.
pub fn set_select_layout(l: Option<SelectLayout>) {
    CHOSEN.with(|c| c.set(Some(l)));
}

/// A candidate's song side, in the side's frame (560 wide).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Geom {
    pub layout: SelectLayout,
    /// The square: x, y, side.
    pub cover: [f32; 3],
    /// The title block and its arrangement (`SongInfo.Text`'s).
    pub text: [f32; 4],
    pub text_style: &'static str,
    /// The style tabs: x, middle, wide.
    pub tabs: [f32; 3],
    /// The chart buttons: x, middle, wide, high.
    pub buttons: [f32; 4],
    /// The chosen chart's lines, as a strip (`wide`), larger (`big`, the
    /// radar Off), on a plate of their own or not.
    pub card: [f32; 4],
    pub wide: bool,
    pub big: bool,
    pub plated: bool,
    /// The radar's panel (the radar On).
    pub radar: Option<[f32; 4]>,
    /// A card behind the square and the lines (D).
    pub plate: Option<[f32; 4]>,
    /// Round 3's players column (in the screen's frame).
    pub column: Option<[f32; 4]>,
}

/// The study's form of the side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Study {
    Wide(Geom),
    /// A phone's or a room's short side: the square beside the title.
    Short,
    /// Round 2's room (wide): the square beside the title and the chart
    /// row, the players under it, the radar beside them or on demand.
    Room(RoomGeom),
}

/// Round 2's room side: the top's height (the square's side), the style
/// button's x and width, the chart buttons' x and width and their middle,
/// the players panel and the radar (in the side's frame).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct RoomGeom {
    pub form: RoomForm,
    pub top: f32,
    pub style: [f32; 2],
    pub picker: [f32; 2],
    pub charts_y: f32,
    pub players: Option<[f32; 4]>,
    pub radar: Option<[f32; 4]>,
}

/// Round 2's B for a side `w` wide and `h` high: the title band, the
/// square and the radar beside it (the square as large as the width and
/// the height allow, the radar taking the rest of the width), the tabs,
/// the buttons and the chosen chart's strip; height to spare grows the
/// strip, then centers the block.
pub(super) fn geom_b2(w: f32, h: f32) -> Geom {
    let band = 88.0;
    let below = 112.0;
    let strip_least = 96.0;
    let s = ((w - 16.0) / 2.0).min(h - band - 10.0 - below - strip_least).clamp(200.0, 340.0).floor();
    let spare = (h - (band + 10.0 + s + below + strip_least)).max(0.0);
    let strip = strip_least + spare.min(64.0);
    let dy = ((spare - spare.min(64.0)) / 2.0).floor();
    let y = band + 10.0 + dy;
    let sb = y + s;
    Geom {
        layout: SelectLayout::TwinsTwo,
        cover: [0.0, y, s],
        text: [0.0, dy, w, band],
        text_style: "top",
        tabs: [0.0, sb + 30.0, w],
        buttons: [0.0, sb + 78.0, w, 44.0],
        card: [0.0, sb + below, w, strip],
        wide: true,
        big: strip >= 130.0,
        plated: true,
        radar: Some([s + 16.0, y, w - s - 16.0, s]),
        plate: None,
        column: None,
    }
}

const W: f32 = 560.0;

/// Candidate `c`'s geometry for a side `h` high, the radar shown or not.
pub(super) fn geom(c: SelectLayout, h: f32, radar: bool) -> Geom {
    let base = Geom {
        layout: c,
        cover: [0.0; 3],
        text: [0.0; 4],
        text_style: "top",
        tabs: [0.0; 3],
        buttons: [0.0; 4],
        card: [0.0; 4],
        wide: false,
        big: !radar,
        plated: true,
        radar: None,
        plate: None,
        column: None,
    };
    match c {
        SelectLayout::TwinsTwo => geom_b2(W, h),
        SelectLayout::Beside => {
            // The square as tall as the title block needs; larger with
            // the height to spare, and with no radar under it.
            let grow = if radar { 0.3 } else { 0.4 };
            let least = if radar { 184.0 } else { 236.0 };
            let cs = (least + (h - 582.0) * grow).clamp(least, 272.0).round();
            let top = cs + 120.0;
            let rest = h - top;
            let (card, radar_r, wide) = if radar {
                ([0.0, top, 196.0, rest.min(380.0)], Some([208.0, top, 352.0, rest.min(380.0)]), false)
            } else {
                ([0.0, top, W, rest.min(130.0)], None, true)
            };
            Geom {
                cover: [0.0, 0.0, cs],
                text: [cs + 20.0, 0.0, W - cs - 20.0, cs],
                text_style: "beside",
                tabs: [0.0, cs + 32.0, W],
                buttons: [0.0, cs + 86.0, W, 44.0],
                card,
                wide,
                radar: radar_r,
                ..base
            }
        }
        SelectLayout::Twins => {
            let s = 272.0;
            let y = 98.0;
            let below = y + s + 14.0;
            let (card, radar_r, wide) = if radar {
                (
                    [0.0, below + 102.0, W, (h - below - 102.0).min(100.0)],
                    Some([W - s, y, s, s]),
                    true,
                )
            } else {
                ([W - s, y, s, s], None, false)
            };
            Geom {
                cover: [0.0, y, s],
                text: [0.0, 0.0, W, 88.0],
                tabs: [0.0, below + 16.0, W],
                buttons: [0.0, below + 68.0, W, 44.0],
                card,
                wide,
                radar: radar_r,
                ..base
            }
        }
        SelectLayout::Column => {
            // The song's column (the square, the title, the chart's
            // lines at its foot) and the charts' (the tabs, the buttons,
            // the radar).
            let l = 248.0;
            let x = l + 20.0;
            let rw = W - x;
            let card_h = 150.0;
            // The chart's lines under the title, at the foot at most.
            let card_y = (h - card_h).min(l + 14.0 + 176.0);
            let (card, radar_r, wide) = if radar {
                ([0.0, card_y, l, card_h], Some([x, 100.0, rw, (h - 100.0).min(420.0)]), false)
            } else {
                ([x, 100.0, rw, (h - 100.0).min(220.0)], None, false)
            };
            let text_h = if radar { card_y - l - 14.0 - 12.0 } else { h - l - 14.0 };
            Geom {
                cover: [0.0, 0.0, l],
                text: [0.0, l + 14.0, l, text_h],
                text_style: "column",
                tabs: [x, 16.0, rw],
                buttons: [x, 66.0, rw, 44.0],
                card,
                wide,
                radar: radar_r,
                ..base
            }
        }
        SelectLayout::Card => {
            let ptop = 204.0;
            let avail = h - ptop;
            let cs = (avail - 24.0 - 118.0).clamp(196.0, 280.0).round();
            let (plate, cover, card, radar_r, wide) = if radar {
                let ph = avail.min(cs + 24.0 + 130.0);
                (
                    [0.0, ptop, cs + 24.0, ph],
                    [12.0, ptop + 12.0, cs],
                    [12.0, ptop + cs + 22.0, cs, ph - cs - 34.0],
                    Some([cs + 36.0, ptop, W - cs - 36.0, ph]),
                    true,
                )
            } else {
                (
                    [0.0, ptop, W, cs + 24.0],
                    [12.0, ptop + 12.0, cs],
                    [cs + 36.0, ptop + 12.0, W - cs - 48.0, cs],
                    None,
                    false,
                )
            };
            Geom {
                cover,
                text: [0.0, 0.0, W, 96.0],
                tabs: [0.0, 120.0, W],
                buttons: [0.0, 170.0, W, 44.0],
                card,
                wide,
                plated: false,
                radar: radar_r,
                plate: Some(plate),
                ..base
            }
        }
    }
}

/// `text` cut where it may break into a first line no wider than `width`
/// at `size` (the theme's face is monospaced) and the rest.
fn first_line(text: &str, width: f32, size: f32) -> (String, String) {
    let lines = ddi_actor::text::wrap(text, width, true, &mut |s| look::mono_width(s, size));
    let Some(first) = lines.first() else {
        return (String::new(), String::new());
    };
    if lines.len() < 2 {
        return (text.to_string(), String::new());
    }
    let n = first.chars().count();
    let rest: String = text.chars().skip(n).collect();
    (first.clone(), rest.trim_start().to_string())
}

/// A text part's size and width.
fn text_room(a: &mut Actor, name: &str) -> Option<(f32, f32)> {
    match &part(a, name)?.kind {
        Kind::Text(t) => Some((t.style().size_64 as f32 / 64.0, t.max_width)),
        _ => None,
    }
}

/// `title` on one line or two in `info`'s `Title` and `Title2`, what
/// follows moved down a line when it takes two.
fn set_title(info: &mut Actor, title: &str) {
    let Some((size, width)) = text_room(info, "Title") else {
        return;
    };
    // One line where the block has no second.
    let (one, two) = if part(info, "Title2").is_some() {
        first_line(title, width, size)
    } else {
        (title.to_string(), String::new())
    };
    set_text(info, "Title", &data(&one));
    set_text(info, "Title2", &data(&two));
    if let Some(after) = part(info, "AfterTitle") {
        crate::widgets::play(after, if two.is_empty() { "OneLine" } else { "TwoLines" });
        after.finish_tweening();
    }
}

/// The room kept between a title running past the song side and the list's
/// rows, and for the heart after it.
const TITLE_GAP: f32 = 12.0;
const HEART_ROOM: f32 = 34.0;

impl SelectScreen {
    /// The title's room in B2 (`None`: the block's own): from its left to
    /// where the list's rows can reach at the title's height whatever the
    /// list's scroll (the rows drift right by their distance from the
    /// wheel's focus line, `Look::wheel`: the nearest a row can come to the
    /// title is half a row nearer than the band's foot), less the heart's
    /// room. Alone; a room's list has its tabs and status line at its top.
    pub(super) fn title_room(&self) -> Option<f32> {
        let g = self.study_wide()?;
        if g.layout != SelectLayout::TwinsTwo || g.text_style != "top" || self.room.is_some() {
            return None;
        }
        let l = &self.layout;
        let [sx, sy, sw] = l.song;
        let [wx, wy, _, wh] = l.wheel;
        // The title's line, 36 high, in the screen's frame.
        let foot = sy + g.text[1] + 36.0;
        let focus = wy + wh * 0.5;
        let d = ((focus - foot) / l.row - 0.5).max(0.0);
        let drift = self.env.look.wheel("Wheel", d).map_or(0.0, |p| p.x);
        let inset = self.env.metric("Select.Wheel.Inset", 0.0);
        // A row's plate starts 8 in from the list's edge (the look's row
        // inset for the focus outline).
        let clear = wx + inset + 8.0 + drift - TITLE_GAP;
        let room = (clear - sx - g.text[0]).max(sw - g.text[0]) - HEART_ROOM;
        Some(room)
    }

    /// `l` as the study's candidate (if one is chosen) has it.
    pub(super) fn study_layout(&self, mut l: Layout) -> Layout {
        l.study = None;
        // B2 is song select's wide song side (alone; a room's until its
        // three parts are built keeps its short side).
        let c = match select_layout() {
            Some(c) => c,
            None if !l.chips && self.room.is_none() => SelectLayout::TwinsTwo,
            None => return l,
        };
        let h = l.ladder[0] + l.ladder[1] - l.song[1];
        if c == SelectLayout::TwinsTwo {
            // Round 3: the room in three columns (the players, the song
            // side as solo's, the list narrower).
            if l.chips && !l.compact && self.room.is_some() && ROOM_FORM.with(Cell::get) == RoomForm::Columns {
                let pw = COLUMN_W.with(Cell::get);
                let [sx, sy, _] = l.song;
                let side = SIDE_W.with(Cell::get);
                let right = l.wheel[0] + l.wheel[2];
                let inset = self.env.metric("Select.Wheel.Inset", 0.0);
                let (x, wx, wright, column) = if PLAYERS_RIGHT.with(Cell::get) {
                    // Round 4: the song side where solo has it, the list,
                    // the players at the screen's right edge.
                    let px = l.screen[0] - sx - pw;
                    let r5 = round5();
                    if r5.on {
                        // Round 5: the panel's line 16 left of the column
                        // when open, `RAIL` from the screen's edge folded;
                        // the list ends 22 short of the line (its
                        // scrollbar by the line as by the screen's edge).
                        let open = px - 16.0;
                        let folded = l.screen[0] - RAIL;
                        let line = open + (folded - open) * r5.fold.clamp(0.0, 1.0);
                        (sx, sx + side + 48.0 - inset, line - 22.0, [line + 16.0, sy, pw, h])
                    } else {
                        (sx, sx + side + 48.0 - inset, px - 32.0, [px, sy, pw, h])
                    }
                } else {
                    let x = sx + pw + 16.0;
                    (x, x + side + 16.0 - inset, right, [sx, sy, pw, h])
                };
                l.chips = false;
                l.radar = true;
                l.players = None;
                l.song = [x, sy, side];
                l.wheel = [wx, l.wheel[1], wright - wx, l.wheel[3]];
                let mut g = geom_b2(side, h);
                g.column = Some(column);
                l.tabs_y = l.song[1] + g.tabs[1] - 16.0;
                l.info_w = side;
                l.study = Some(Study::Wide(g));
                return l;
            }
            if l.chips && !l.compact && self.room.is_some() {
                let g = self.room_geom(&l, h);
                l.radar = g.radar.is_some();
                l.players = g.players.map(|[x, y, w, ph]| [l.song[0] + x, l.song[1] + y, w, ph]);
                l.info_h = g.top + 12.0;
                l.info_w = l.song[2];
                l.picker = g.picker;
                l.style_y = g.charts_y;
                l.charts_y = g.charts_y;
                l.study = Some(Study::Room(g));
                return l;
            }
            if !l.chips {
                l.radar = true;
            }
        }
        l.study = Some(if l.chips {
            Study::Short
        } else if c == SelectLayout::TwinsTwo {
            let g = geom_b2(l.song[2], h);
            l.tabs_y = l.song[1] + g.tabs[1] - 16.0;
            Study::Wide(g)
        } else {
            let g = geom(c, h, l.radar);
            // The tabs where `fill_tabs` puts them.
            l.tabs_y = l.song[1] + g.tabs[1] - 16.0;
            Study::Wide(g)
        });
        l
    }

    /// Round 2's room side for a side `h` high.
    fn room_geom(&self, l: &Layout, h: f32) -> RoomGeom {
        let w = l.song[2];
        let form = ROOM_FORM.with(Cell::get);
        // The players keep room for twelve rows and a count (13 × 30 and
        // their heading): the top takes the rest, 150 at 720.
        let top = (h - 432.0).clamp(150.0, 220.0).floor();
        let x0 = top + 12.0;
        let cw = w - x0;
        let styles = self.style_button_shown();
        let (style, picker) = if styles {
            ([x0, 120.0], [x0 + 128.0, cw - 128.0])
        } else {
            ([x0, 0.0], [x0, cw])
        };
        let below = top + 14.0;
        let rest = h - below;
        let (players, radar) = match form {
            RoomForm::Beside | RoomForm::Columns => (Some([0.0, below, 300.0, rest]), Some([312.0, below, w - 312.0, rest.min(320.0)])),
            RoomForm::OnDemand => (Some([0.0, below, w, rest]), None),
            RoomForm::OnDemandOpen => (None, Some([0.0, below, w, rest.min(360.0)])),
        };
        RoomGeom {
            form,
            top,
            style,
            picker,
            charts_y: top * 0.52,
            players,
            radar,
        }
    }

    /// Round 2's room side, where drawn.
    pub(super) fn study_room(&self) -> Option<RoomGeom> {
        match self.layout.study {
            Some(Study::Room(g)) => Some(g),
            _ => None,
        }
    }

    /// The radar Round 2 expects for this screen (`None`: today's rule).
    pub(super) fn study_radar(&self, compact: bool) -> Option<bool> {
        let b2 = select_layout().map_or(self.room.is_none(), |c| c == SelectLayout::TwinsTwo);
        b2.then(|| {
            if self.room.is_some() {
                !compact && ROOM_FORM.with(Cell::get) != RoomForm::OnDemand
            } else {
                !compact
            }
        })
    }

    /// The wide side's geometry, where a candidate is chosen.
    pub(super) fn study_wide(&self) -> Option<Geom> {
        match self.layout.study {
            Some(Study::Wide(g)) => Some(g),
            _ => None,
        }
    }

    /// The song's info for the study: the square and the title block
    /// (wide), or the short side's.
    pub(super) fn study_info(&self, l: &Layout, info_h: f32) -> Actor {
        let env = &self.env;
        match l.study {
            Some(Study::Wide(g)) => {
                let [cx, cy, cs] = g.cover;
                let mut cover = env.component("SongCover", "Panel", &Spec::new([cs, cs], ""), "Cover", [cx, cy]);
                cover.name = "Cover".into();
                let [tx, ty, tw, th] = g.text;
                let style = if (round5().on || g.layout == SelectLayout::TwinsTwo) && g.text_style == "top" {
                    "top5"
                } else {
                    g.text_style
                };
                let text = env.component(
                    "SongInfo.Text",
                    "Panel",
                    &Spec::new([tw, th], "").with_items([style.to_string()]),
                    "InfoText",
                    [tx, ty],
                );
                let mut parts = Vec::new();
                // Round 2's preview as the side's backdrop (option d).
                if g.layout == SelectLayout::TwinsTwo {
                    // The side's own extent: the title band to the strip's
                    // foot, the square's left to the radar's right.
                    let right = g.radar.map_or(W, |r| r[0] + r[2]);
                    let (top, bottom) = (g.text[1], g.card[1] + g.card[3]);
                    let mut back =
                        ddi_actor::Actor::new(Kind::Sprite(ddi_actor::Sprite::quad([right, bottom - top])));
                    back.name = "PreviewBack".into();
                    if let Kind::Sprite(sp) = &mut back.kind {
                        sp.fit = ddi_actor::Fit::Cover;
                    }
                    call(&mut back, "halign", &[0.0]);
                    call(&mut back, "valign", &[0.0]);
                    call(&mut back, "cornerradius", &[12.0]);
                    call(&mut back, "diffuse", &[0.32, 0.32, 0.32, 1.0]);
                    call(&mut back, "y", &[top]);
                    back.visible = false;
                    parts.push(back);
                }
                // D's card, under the square.
                if let Some([x, y, w, h]) = g.plate {
                    let mut p = quad("StudyPlate", [w, h], env.color("Radar.Panel"));
                    call(&mut p, "cornerradius", &[12.0]);
                    call(&mut p, "xy", &[x + w / 2.0, y + h / 2.0]);
                    parts.push(p);
                }
                parts.extend([cover, text]);
                frame("Info", parts)
            }
            Some(Study::Room(_)) => env.component(
                "SongInfo.Short",
                "SongInfo",
                &Spec::new([l.info_w, info_h], "").with_items(["oneline".to_string()]),
                "Info",
                [0.0, 0.0],
            ),
            _ => env.component(
                "SongInfo.Short",
                "SongInfo",
                &Spec::new([l.info_w, info_h], ""),
                "Info",
                [0.0, 0.0],
            ),
        }
    }

    /// The wide side's other parts: the tabs and their key, the chart
    /// buttons, the chosen chart's lines, the radar's slot, D's card.
    pub(super) fn study_parts(&self, g: &Geom) -> Vec<Actor> {
        let env = &self.env;
        let mut out = Vec::new();
        let mut tabs = frame("Tabs", Vec::new());
        call(&mut tabs, "x", &[g.tabs[0]]);
        out.push(tabs);
        let kbd = env.numbers("TabKey", &ddi_l10n::UiText::data(super::TAB_LEGEND), 11.0, "Muted");
        out.push(right(kbd, g.tabs[0] + g.tabs[2], g.tabs[1]));
        out.push(frame("Charts", Vec::new()));
        out.push(frame("ChartCard", Vec::new()));
        out.push(frame("RadarSlot", Vec::new()));
        // Round 2's preview in the square's place widened to the movie's
        // shape (option c), over the radar.
        if g.layout == SelectLayout::TwinsTwo {
            let [x, y, s] = g.cover;
            // Over the radar to its right edge.
            let ww = g.radar.map_or(s, |r| r[0] + r[2] - x);
            let mut plate = quad("Plate", [ww, s], env.color("BannerEmpty"));
            call(&mut plate, "cornerradius", &[8.0]);
            call(&mut plate, "xy", &[ww / 2.0, s / 2.0]);
            let mut movie = ddi_actor::Actor::new(Kind::Sprite(ddi_actor::Sprite::quad([ww, s])));
            movie.name = "PreviewWideArt".into();
            if let Kind::Sprite(sp) = &mut movie.kind {
                sp.fit = ddi_actor::Fit::Contain;
            }
            call(&mut movie, "halign", &[0.0]);
            call(&mut movie, "valign", &[0.0]);
            call(&mut movie, "cornerradius", &[8.0]);
            let mut f = frame("PreviewWide", vec![plate, movie]);
            call(&mut f, "xy", &[x, y]);
            f.visible = false;
            out.push(f);
        }
        out
    }

    /// The tabs' room across (`fill_tabs`).
    pub(super) fn tabs_room(&self) -> f32 {
        match self.study_wide() {
            Some(g) => g.tabs[2] - 48.0,
            None => self.layout.song[2] - 48.0,
        }
    }

    /// The radar's panel where a candidate has one.
    pub(super) fn study_radar_area(&self) -> Option<([f32; 2], [f32; 4])> {
        let r = self.study_wide()?.radar?;
        Some(([r[1], r[3]], r))
    }

    /// Round 2's room, the radar on demand: its button at the end of the
    /// chart's line (drawn for the study's pictures; not wired).
    pub(super) fn study_room_extras(&self, l: &Layout) -> Vec<Actor> {
        let Some(Study::Room(g)) = l.study else {
            return Vec::new();
        };
        if g.form == RoomForm::Beside {
            return Vec::new();
        }
        let label: String = ddi_l10n::msg::options::skill_radar().into();
        let w = look::mono_width(&label, 13.0) + 24.0;
        let mut pill = look::component_or(&*self.env.look, "Pill", "Button", &Spec::new([w, 26.0], label));
        pill.name = "Study.RadarToggle".into();
        if g.form == RoomForm::OnDemandOpen {
            crate::widgets::play(&mut pill, "GainFocus");
        }
        call(&mut pill, "xy", &[l.song[2] - w / 2.0, g.charts_y + 38.0]);
        vec![pill]
    }

    /// Round 2's room radar's panel.
    pub(super) fn study_room_radar_area(&self) -> Option<([f32; 2], [f32; 4])> {
        let r = self.study_room()?.radar?;
        Some(([r[1], r[3]], r))
    }

    /// Round 2's preview still where one stands in for the song shown's
    /// movie (`set_preview_still`): options c and d, outside the square.
    pub(super) fn fill_study_preview(&self, root: &mut Actor, song: Option<&SongEntry>, cx: &mut Cx) {
        let p = song.and_then(|s| preview_for(&s.id));
        let mode = p.as_ref().map(|p| p.mode);
        let tex = p.as_ref().map(|p| cx.texture(&p.still, 1024.0));
        if let Some(a) = hit::find_mut(root, "PreviewWide") {
            a.visible = mode == Some('c');
            set_image(a, "PreviewWideArt", tex.filter(|_| mode == Some('c')));
        }
        if let Some(a) = hit::find_mut(root, "PreviewBack") {
            let on = mode == Some('d');
            if let (Kind::Sprite(sp), Some(t)) = (&mut a.kind, tex) {
                sp.texture = t;
            }
            a.visible = on && tex.is_some();
        }
    }

    /// The square and the title (wide), filled for `song`.
    pub(super) fn fill_study_info(&self, info: &mut Actor, song: Option<&SongEntry>, cx: &mut Cx) {
        let Some(song) = song else {
            set_image(info, "CoverArt", None);
            set_image(info, "CoverBlur", None);
            if let Some(p) = part(info, "CoverPlaceholder") {
                p.visible = false;
            }
            set_text(info, "Title2", "");
            return;
        };
        set_title(info, &song.title);
        // B2's title may run past the side into the width the list's
        // curve leaves clear at its height.
        if let Some(w) = self.title_room()
            && let Some(t) = part(info, "Title")
        {
            look::ellipsized(t, w);
        }
        // The art: the preview movie while its frames come, else the
        // cover, else the banner; the blurred copy of the still under it.
        let still = song.jacket.as_deref().or(song.banner.as_deref());
        let movie = self.movie.as_deref() == Some(song.id.as_str());
        let art = if movie {
            Some(cx.texture(super::PREVIEW_MOVIE, 512.0))
        } else {
            still.map(|s| cx.texture(s, 512.0))
        };
        let blur = still.map(|s| cx.blurred_texture(s, 128.0));
        // Round 2's preview options inside the square: whole over the
        // cover's blur (a), or filling it (b).
        let preview = preview_for(&song.id).filter(|p| matches!(p.mode, 'a' | 'b'));
        let fill = preview.as_ref().is_some_and(|p| p.mode == 'b');
        if let Some(Kind::Sprite(sp)) = part(info, "CoverArt").map(|a| &mut a.kind) {
            sp.fit = if fill { ddi_actor::Fit::Cover } else { ddi_actor::Fit::Contain };
        }
        let art = preview.as_ref().map(|p| cx.texture(&p.still, 1024.0)).or(art);
        let blur = if fill { None } else { blur };
        set_image(info, "CoverArt", art);
        set_image(info, "CoverBlur", blur);
        // Neither: the plate with the title set in it.
        let none = art.is_none();
        if let Some(p) = part(info, "CoverPlaceholder") {
            p.visible = none;
        }
        if none {
            let (size, width) = text_room(info, "CoverTitle0").unwrap_or((20.0, 200.0));
            let lines = ddi_actor::text::wrap(&song.title, width, true, &mut |s| look::mono_width(s, size));
            let mut lines: Vec<String> = lines.into_iter().take(3).collect();
            let n = lines.len();
            if ddi_actor::text::wrap(&song.title, width, true, &mut |s| look::mono_width(s, size)).len() > 3
                && let Some(last) = lines.last_mut()
            {
                last.push('…');
            }
            for k in 0..3 {
                let at = (k + n).checked_sub(3).and_then(|i| lines.get(i));
                set_text(info, &format!("CoverTitle{k}"), &at.map(|s| data(s)).unwrap_or_default());
            }
            // A room's song not on this device yet: said on the plate.
            let under: String = if song.away() {
                ddi_l10n::msg::room::song_not_loaded().into()
            } else {
                data(&song.artist)
            };
            set_text(info, "CoverArtist", &under);
        }
    }

    /// The wide side's charts: the buttons in a row, the chosen chart's
    /// lines, and the radar (`fill_radar`) or the lines in its place.
    pub(super) fn fill_study_charts(
        &self,
        root: &mut Actor,
        charts: &[&ChartEntry],
        first: usize,
        n: usize,
        pick: Option<usize>,
        g: &Geom,
    ) {
        let env = self.env.clone();
        let [bx, by, bw, bh] = g.buttons;
        let mut buttons = self.chart_buttons(charts, first, n, pick, [bx, bw], by, bh);
        if charts.is_empty() && self.song().is_some() {
            let none = msg::no_charts(self.style_label());
            let [x, y, w, _] = g.card;
            let mut e = env.component(
                "ChartLadder.Empty",
                "Panel",
                &Spec::new([w.max(g.radar.map_or(0.0, |r| r[0] + r[2] - x)), 150.0], none.as_str()),
                "Ladder.Empty",
                [x, y],
            );
            set_text(&mut e, "Title", &none);
            set_text(&mut e, "Hint", &msg::no_charts_hint());
            buttons.push(e);
        }
        if let Some(f) = hit::find_mut(root, "Charts") {
            f.kind = Kind::Frame(buttons);
        }
        // Round 5: the chosen chart's notes in the line under the title.
        if (round5().on || g.layout == SelectLayout::TwinsTwo)
            && let Some(info) = hit::find_mut(root, "Info")
        {
            let notes = match pick.map(|p| charts[p]) {
                Some(c) if !self.song().is_some_and(SongEntry::away) => c.notes.to_string(),
                _ => "–".to_string(),
            };
            set_text(info, "Notes", &data(&notes));
        }
        let card = pick.map(|p| self.study_card(charts[p], g)).unwrap_or_default();
        if let Some(f) = hit::find_mut(root, "ChartCard") {
            f.kind = Kind::Frame(card);
        }
        if g.radar.is_none()
            && let Some(slot) = hit::find_mut(root, "RadarSlot")
        {
            slot.kind = Kind::Frame(Vec::new());
        }
    }

    /// The chosen chart's lines in the candidate's card: its difficulty
    /// (or name), its number as its ladder row showed it, the song's own
    /// number where a rating takes its place, its version, notes, step
    /// artist, the estimate's unit line, and its best.
    fn study_card(&self, c: &ChartEntry, g: &Geom) -> Vec<Actor> {
        let env = &self.env;
        let [x0, y0, w0, h0] = g.card;
        let mut out = Vec::new();
        let pad = if g.plated { 12.0 } else { 0.0 };
        if g.plated {
            let mut p = quad("CardPlate", [w0, h0], env.color("Radar.Panel"));
            call(&mut p, "cornerradius", &[w0.min(h0) * 0.06]);
            call(&mut p, "xy", &[x0 + w0 / 2.0, y0 + h0 / 2.0]);
            out.push(p);
        }
        let [x, y, w, _h] = [x0 + pad, y0 + pad, w0 - 2.0 * pad, h0 - 2.0 * pad];
        let color = format!("Difficulty.{}", c.difficulty.name());
        let detail = c.shown_rating().map_or_else(|| c.meter_text_fine(), |r| r.detail());
        let mut first: Vec<String> = Vec::new();
        if c.shown_rating().is_some() {
            first.push(msg::chart_song_meter(c.meter).into());
        }
        if let Some(version) = c.version {
            first.push(version_name(version).into());
        }
        let mut lines: Vec<String> = Vec::new();
        if !first.is_empty() {
            lines.push(first.join(" · "));
        }
        // (Round 5: the notes are in the line under the title.)
        if !self.song().is_some_and(SongEntry::away) && !round5().on && g.layout != SelectLayout::TwinsTwo {
            lines.push(msg::chart_notes(c.notes).into());
        }
        if !c.credit.is_empty() {
            lines.push(msg::chart_steps_by(&c.credit).into());
        }
        if matches!(self.radar_standing(), Some(crate::state::ratings::Standing::Profile(_))) {
            lines.push(msg::radar_estimate_unit().into());
        }
        let meter_size = if g.big { 40.0 } else { 30.0 };
        let mut label = env.text("Card.Label", &data(&c.label().to_uppercase()), 14.0, 700, &color);
        let mut meter = env.numbers("Card.Meter", &detail, meter_size, &color);
        if crate::state::ratings::ever_shown() {
            call(&mut meter, "diffusealpha", &[if c.meter_is_songs() { super::FAINT } else { 1.0 }]);
        }
        // The best's three parts.
        let best = c.best.as_ref().map(|b| {
            let grade = env.numbers("Card.Grade", &data(&b.grade), if g.wide { 22.0 } else { 18.0 }, "Text");
            let score = if b.pending {
                format!("{} …", b.score)
            } else {
                b.score.clone()
            };
            let s = env.text("Card.Score", &data(&score), 12.0, 400, "Muted");
            let lamp = env.text("Card.Lamp", &data(&b.lamp_text), 11.0, 700, &format!("Lamp.{}", b.lamp));
            (b.grade.clone(), grade, s, lamp)
        });
        if g.wide {
            // A strip: the difficulty and number, the lines, the best.
            let lw = if w > 400.0 { 150.0 } else { 96.0 };
            let rw = if best.is_some() { if w > 400.0 { 120.0 } else { 0.0 } } else { 0.0 };
            look::ellipsized(&mut label, lw - 8.0);
            out.push(left(label, x, y + 9.0));
            out.push(left(meter, x, y + 9.0 + 14.0 + meter_size * 0.62));
            let mx = x + lw;
            let mw = w - lw - rw - if rw > 0.0 { 12.0 } else { 0.0 };
            let mut ly = y + 9.0;
            for (i, line) in lines.iter().enumerate() {
                let mut t = env.text(&format!("Card.Line{i}"), line, 12.0, 400, "Muted");
                look::ellipsized(&mut t, mw);
                out.push(left(t, mx, ly));
                ly += 17.0;
            }
            if let Some((_, grade, s, lamp)) = best {
                if rw > 0.0 {
                    let rx = x + w - rw;
                    out.push(left(grade, rx, y + 13.0));
                    let mut s = s;
                    look::ellipsized(&mut s, rw);
                    out.push(left(s, rx, y + 38.0));
                    out.push(left(lamp, rx, y + 56.0));
                } else {
                    // No column of its own: a line under the others.
                    let gw = crate::radar::width_of(&c.best.as_ref().map_or(String::new(), |b| b.grade.clone()), 22.0) + 10.0;
                    out.push(left(grade, mx, ly + 4.0));
                    let mut s = s;
                    look::ellipsized(&mut s, (mw - gw - 50.0).max(30.0));
                    out.push(left(s, mx + gw, ly + 4.0));
                    out.push(right(lamp, mx + mw, ly + 4.0));
                }
            }
        } else {
            look::ellipsized(&mut label, w);
            out.push(left(label, x, y + 9.0));
            out.push(left(meter, x, y + 9.0 + 12.0 + meter_size * 0.6));
            let mut ly = y + 9.0 + 12.0 + meter_size * 1.2 + 10.0;
            for (i, line) in lines.iter().enumerate() {
                let mut t = env.text(&format!("Card.Line{i}"), line, if g.big { 13.0 } else { 12.0 }, 400, "Muted");
                look::ellipsized(&mut t, w);
                out.push(left(t, x, ly));
                ly += if g.big { 19.0 } else { 17.0 };
            }
            if let Some((g_text, grade, s, lamp)) = best {
                // At the card's foot where it has one of its own.
                ly = if g.plated { (y0 + h0 - pad - 10.0).max(ly + 8.0) } else { ly + 8.0 };
                let gw = crate::radar::width_of(&g_text, 18.0) + 10.0;
                out.push(left(grade, x, ly));
                let mut s = s;
                look::ellipsized(&mut s, (w - gw - 40.0).max(40.0));
                out.push(left(s, x + gw, ly));
                out.push(right(lamp, x + w, ly));
            }
        }
        out
    }
}

/// The icons' and the marks' colors (the look's).
fn icon_color(env: &crate::screens::Env, name: &str) -> ddi_actor::Rgba {
    env.color(name)
}

/// A shape `size` large centered on `at` (rounded `r`), turned `deg`.
fn shape(env: &crate::screens::Env, color: &str, size: [f32; 2], at: [f32; 2], r: f32, deg: f32) -> Actor {
    let mut q = quad("", size, icon_color(env, color));
    call(&mut q, "cornerradius", &[r]);
    if deg != 0.0 {
        call(&mut q, "rotationz", &[deg]);
    }
    call(&mut q, "xy", &at);
    q
}

/// A ring `d` across, `w` thick.
fn ring(env: &crate::screens::Env, color: &str, d: f32, w: f32, at: [f32; 2]) -> Actor {
    let mut q = shape(env, color, [d, d], at, d / 2.0, 0.0);
    call(&mut q, "outline", &[w]);
    q
}

/// Round 3's status icon, about 14 px, centered on the origin: shape and
/// color together carry its meaning (never a glyph).
pub(super) fn player_icon(env: &crate::screens::Env, icon: PlayerIcon) -> Actor {
    let st = COLUMN_STYLE.with(Cell::get);
    if st.mono {
        return player_icon_mono(env, icon, st.accents);
    }
    let c: Vec<Actor> = match icon {
        PlayerIcon::Idle => vec![shape(env, "Off", [5.0, 5.0], [0.0, 0.0], 2.5, 0.0)],
        PlayerIcon::WaitingSong => [-4.5, 0.0, 4.5]
            .iter()
            .map(|x| shape(env, "Muted", [3.2, 3.2], [*x, 0.0], 1.6, 0.0))
            .collect(),
        PlayerIcon::Loading(p) => {
            // A down arrow over a progress bar, in the blue of "on its way".
            let col = "Difficulty.Beginner";
            let mut v = vec![
                shape(env, col, [2.2, 7.0], [0.0, -3.0], 1.1, 0.0),
                shape(env, col, [2.2, 5.5], [-1.7, 0.6], 1.1, -45.0),
                shape(env, col, [2.2, 5.5], [1.7, 0.6], 1.1, 45.0),
                shape(env, "Track", [12.0, 2.4], [0.0, 5.6], 1.2, 0.0),
            ];
            let w = (12.0 * p.clamp(0.0, 1.0)).max(2.4);
            v.push(shape(env, col, [w, 2.4], [-6.0 + w / 2.0, 5.6], 1.2, 0.0));
            v
        }
        PlayerIcon::LoadFailed => vec![
            ring(env, "Danger", 13.0, 1.6, [0.0, 0.0]),
            shape(env, "Danger", [2.0, 5.5], [0.0, -1.4], 1.0, 0.0),
            shape(env, "Danger", [2.2, 2.2], [0.0, 3.4], 1.1, 0.0),
        ],
        PlayerIcon::NotReady => vec![ring(env, "Difficulty.Medium", 12.0, 1.8, [0.0, 0.0])],
        PlayerIcon::Ready => vec![
            shape(env, "Difficulty.Easy", [13.0, 13.0], [0.0, 0.0], 6.5, 0.0),
            shape(env, "OnAccent", [2.2, 4.6], [-2.3, 1.0], 1.1, -45.0),
            shape(env, "OnAccent", [2.2, 8.0], [1.4, -0.4], 1.1, 40.0),
        ],
        PlayerIcon::Playing | PlayerIcon::FailedPlaying => {
            let (col, hs) = if icon == PlayerIcon::Playing {
                ("Radar.Line", [7.0, 12.0, 9.0])
            } else {
                ("Danger", [4.0, 7.0, 5.0])
            };
            [-4.0, 0.0, 4.0]
                .iter()
                .zip(hs)
                .map(|(x, h)| shape(env, col, [2.6, h], [*x, 6.0 - h / 2.0], 1.0, 0.0))
                .collect()
        }
        PlayerIcon::Out => vec![
            shape(env, "Danger", [2.4, 12.0], [0.0, 0.0], 1.2, 45.0),
            shape(env, "Danger", [2.4, 12.0], [0.0, 0.0], 1.2, -45.0),
        ],
        PlayerIcon::SatOut => vec![
            ring(env, "Muted", 12.0, 1.6, [0.0, 0.0]),
            shape(env, "Muted", [6.5, 2.0], [0.0, 0.0], 1.0, 0.0),
        ],
        PlayerIcon::Left => vec![
            shape(env, "Faint", [2.0, 11.0], [-4.5, 0.0], 1.0, 0.0),
            shape(env, "Faint", [8.0, 2.0], [0.5, 0.0], 1.0, 0.0),
            shape(env, "Faint", [2.0, 5.0], [3.2, -1.6], 1.0, -45.0),
            shape(env, "Faint", [2.0, 5.0], [3.2, 1.6], 1.0, 45.0),
        ],
        PlayerIcon::Watching => vec![
            {
                let mut e = shape(env, "Secondary", [13.0, 8.0], [0.0, 0.0], 4.0, 0.0);
                call(&mut e, "outline", &[1.5]);
                e
            },
            shape(env, "Secondary", [4.0, 4.0], [0.0, 0.0], 2.0, 0.0),
        ],
        PlayerIcon::Away => vec![
            shape(env, "Faint", [2.6, 10.0], [-2.4, 0.0], 1.0, 0.0),
            shape(env, "Faint", [2.6, 10.0], [2.4, 0.0], 1.0, 0.0),
        ],
    };
    frame("Icon", c)
}

/// The players column's marks, about 12 px, centered on the origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlayerMark {
    Host,
    CoHost,
    Chooser,
    Bot,
    /// A CPU's level, 1 to 10: five pips (silver to 5, gold over silver
    /// past it, as the stars).
    Level(u8),
}

pub(super) fn player_mark(env: &crate::screens::Env, mark: PlayerMark) -> Actor {
    let crown = |col: &str| -> Vec<Actor> {
        let mut v = vec![shape(env, col, [11.0, 3.0], [0.0, 3.0], 1.0, 0.0)];
        for (x, y) in [(-4.0, -0.5), (0.0, -2.0), (4.0, -0.5)] {
            v.push(shape(env, col, [3.6, 3.6], [x, y], 0.6, 45.0));
        }
        v.push(shape(env, col, [11.0, 2.0], [0.0, 1.0], 0.0, 0.0));
        v
    };
    let st = COLUMN_STYLE.with(Cell::get);
    let (gold, silver) = if st.crown_color { ("Star.Gold", "Star.Silver") } else { ("Secondary", "Muted") };
    let c = match mark {
        PlayerMark::Host => crown(gold),
        // Two tips: told from the host's three by shape, not only color.
        PlayerMark::CoHost => {
            let mut v = crown(silver);
            v.remove(2);
            v
        }
        PlayerMark::Chooser => vec![
            shape(env, if st.crown_color { "Accent" } else { "Secondary" }, [5.0, 4.4], [-1.6, 3.4], 2.2, -20.0),
            shape(env, if st.crown_color { "Accent" } else { "Secondary" }, [1.6, 9.0], [0.6, -1.2], 0.6, 0.0),
            shape(env, if st.crown_color { "Accent" } else { "Secondary" }, [4.6, 1.8], [2.4, -5.0], 0.8, 30.0),
        ],
        PlayerMark::Bot => vec![
            {
                let mut b = shape(env, "Muted", [11.0, 9.0], [0.0, 1.0], 2.5, 0.0);
                call(&mut b, "outline", &[1.4]);
                b
            },
            shape(env, "Muted", [2.2, 2.2], [-2.2, 1.0], 1.1, 0.0),
            shape(env, "Muted", [2.2, 2.2], [2.2, 1.0], 1.1, 0.0),
            shape(env, "Muted", [1.4, 3.0], [0.0, -4.2], 0.7, 0.0),
        ],
        PlayerMark::Level(l) => (0..5)
            .map(|k| {
                let k1 = k as u8 + 1;
                let col = if l > 5 && k1 <= l - 5 {
                    "Star.Gold"
                } else if l > 5 || k1 <= l {
                    "Star.Silver"
                } else {
                    "Star.Empty"
                };
                shape(env, col, [3.2, 3.2], [-8.0 + k as f32 * 4.0, 0.0], 1.6, 0.0)
            })
            .collect(),
    };
    frame("Mark", c)
}

impl SelectScreen {
    /// Round 3's players column, where the room is drawn in three columns.
    pub(super) fn study_column(&self) -> Option<[f32; 4]> {
        self.study_wide()?.column
    }

    /// Round 3's players column (`RoomPlayers`): a heading with the count,
    /// a line a player (a status icon, the name cut between graphemes, one
    /// mark, the place where a stage is confirmed), a "+N" line past what
    /// fits (the Players pop-up).
    pub(super) fn study_players_column(&self, [x, y, w, h]: [f32; 4]) -> Actor {
        use crate::screens::room as rm;
        use ddi_l10n::msg::room as m;
        let env = &self.env;
        let Some(room) = &self.room else {
            return frame("RoomPlayers", Vec::new());
        };
        let r = room.room.borrow();
        let rows = rm::panel_order(rm::player_rows(&r));
        let cap = r.state().policy.max_competitors;
        let mut c = Vec::new();
        let r5 = round5();
        if r5.on {
            // The panel: a quieter ground from its line to the screen's
            // edge, between the header's and the footer's bars, its line
            // the whole height, a handle on it.
            let [sw, sh] = self.layout.screen;
            let line_x = x - 16.0;
            let (top, bottom) = (64.0, sh - 40.0);
            let mut ground = quad("Panel.Ground", [sw - line_x, bottom - top], env.color("Plate"));
            call(&mut ground, "diffusealpha", &[0.72]);
            call(&mut ground, "xy", &[line_x + (sw - line_x) / 2.0, (top + bottom) / 2.0]);
            c.push(ground);
            c.push(shape(env, "Line2", [1.0, bottom - top], [line_x, (top + bottom) / 2.0], 0.0, 0.0));
            // The handle: a small plate on the line with a chevron
            // pointing the way it folds.
            let hy = top + 28.0;
            let mut plate = shape(env, "Plate2", [18.0, 36.0], [line_x, hy], 6.0, 0.0);
            plate.name = "Panel.Handle".into();
            c.push(plate);
            c.push({
                let mut o = shape(env, "Line2", [18.0, 36.0], [line_x, hy], 6.0, 0.0);
                call(&mut o, "outline", &[1.0]);
                o
            });
            let folded = r5.fold >= 1.0;
            let dir = if folded { -1.0 } else { 1.0 };
            c.push(shape(env, "Secondary", [1.8, 7.0], [line_x - dir * 0.0, hy - 2.2], 0.9, -dir * 40.0));
            c.push(shape(env, "Secondary", [1.8, 7.0], [line_x - dir * 0.0, hy + 2.2], 0.9, dir * 40.0));
            if folded {
                // The rail: the count, then a state icon a player.
                let cx = line_x + RAIL / 2.0;
                let counted = rm::players_counted(&rows);
                c.push({
                    let mut t = env.numbers("Players.Count", &ddi_l10n::UiText::data(counted.to_string()), 12.0, "Muted");
                    call(&mut t, "xy", &[cx, hy + 34.0]);
                    t
                });
                if r5.rail_icons {
                    let staged = STAGED.with(|s| s.borrow().clone());
                    let word = |f: fn() -> ddi_l10n::UiText| -> String { f().into() };
                    let mut ks = 0;
                    let top_y = hy + 58.0;
                    let fit = ((bottom - top_y - 8.0) / 22.0).floor() as usize;
                    for (k, p) in rows.iter().enumerate().take(fit) {
                        let mut icon = icon_of(p, &word);
                        if let Some(st) = &staged
                            && !p.me
                            && !st.is_empty()
                        {
                            icon = st[ks % st.len()];
                            ks += 1;
                        }
                        let cy = top_y + k as f32 * 22.0;
                        if p.me {
                            c.push(shape(env, "Accent", [3.0, 16.0], [line_x + 4.0, cy], 1.5, 0.0));
                        }
                        let mut ic = player_icon(env, icon);
                        call(&mut ic, "xy", &[cx + 2.0, cy]);
                        c.push(ic);
                    }
                }
                return frame("RoomPlayers", c);
            }
        }
        let title: String = m::players().into();
        c.push(left(
            look::tracked(env.text("Players.Heading", &title, 11.0, 700, "Faint"), 120),
            x,
            y + 12.0,
        ));
        c.push(right(
            env.text("Players.Count", &m::players_count(rm::players_counted(&rows), cap), 11.0, 400, "Muted"),
            x + w,
            y + 12.0,
        ));
        const ROW: f32 = 24.0;
        let top = y + 30.0;
        let fit = ((h - 30.0) / ROW).floor() as usize;
        let shown = if rows.len() > fit { fit.saturating_sub(1) } else { rows.len() };
        let staged = STAGED.with(|s| s.borrow().clone());
        let mut k_staged = 0;
        let word = |f: fn() -> ddi_l10n::UiText| -> String { f().into() };
        for (k, p) in rows.iter().enumerate().take(shown) {
            let cy = top + k as f32 * ROW + ROW / 2.0;
            let mut icon = icon_of(p, &word);
            if let Some(st) = &staged
                && !p.me
                && !st.is_empty()
            {
                icon = st[k_staged % st.len()];
                k_staged += 1;
            }
            if p.me {
                c.push(shape(env, "Accent", [3.0, ROW - 6.0], [x + 1.5, cy], 1.5, 0.0));
            }
            let mut ic = player_icon(env, icon);
            call(&mut ic, "xy", &[x + 15.0, cy]);
            c.push(ic);
            // The one mark: a CPU's level, a bot, the one choosing songs,
            // the host, a co-host (in that order of what tells most).
            let host: String = m::role_host().into();
            let cohost: String = m::role_co_host().into();
            let mark = if let Some(l) = p.level {
                Some(PlayerMark::Level(l))
            } else if p.name.contains("Bot") && !p.cpu && p.role.is_empty() && p.id.is_none() {
                Some(PlayerMark::Bot)
            } else if p.role.contains(&String::from(m::role_picking())) {
                Some(PlayerMark::Chooser)
            } else if p.role.contains(&host) {
                Some(PlayerMark::Host)
            } else if p.role.contains(&cohost) {
                Some(PlayerMark::CoHost)
            } else {
                None
            };
            let mark = STAGED_MARKS.with(|m| m.borrow().as_ref().map(|v| v[k % v.len()])).unwrap_or(mark);
            let style = COLUMN_STYLE.with(Cell::get);
            // Round 4: the roles before the name (two where a host also
            // chooses), the properties (a level, a bot) at the right.
            let roles: Vec<PlayerMark> = if style.marks_before {
                let picking = p.role.contains(&String::from(m::role_picking()));
                let mut v = Vec::new();
                if STAGED_MARKS.with(|m| m.borrow().is_some()) {
                    v.extend(mark.filter(|mk| !matches!(mk, PlayerMark::Level(_) | PlayerMark::Bot)));
                    // Staged: the one choosing is the host too (two marks).
                    if style.two && mark == Some(PlayerMark::Chooser) {
                        v.insert(0, PlayerMark::Host);
                    }
                } else {
                    if p.role.contains(&host) {
                        v.push(PlayerMark::Host);
                    } else if p.role.contains(&cohost) {
                        v.push(PlayerMark::CoHost);
                    }
                    if picking {
                        v.push(PlayerMark::Chooser);
                    }
                }
                if !style.two {
                    v.truncate(1);
                }
                v
            } else {
                Vec::new()
            };
            let props = if style.marks_before {
                mark.filter(|mk| matches!(mk, PlayerMark::Level(_) | PlayerMark::Bot))
            } else {
                mark
            };
            let staged_place = STAGED_PLACES.with(|s| s.borrow().as_ref().map(|v| v[k % v.len()]));
            let place_n = staged_place.unwrap_or(p.place);
            // Round 5: the top three's places as marks before the name.
            let leader = (r5.on && r5.leaders > 0).then_some(place_n).flatten().filter(|n| *n <= 3);
            let place = match (r5.on, r5.leaders, leader) {
                (true, _, Some(_)) => None,
                (true, 2, None) => None,
                _ => place_n.map(|n| n.to_string()),
            };
            let mut right_w = 6.0;
            if let Some(pl) = &place {
                c.push(right(env.numbers("Place", &ddi_l10n::UiText::data(pl), 11.0, "Muted"), x + w, cy));
                right_w += look::mono_width(pl, 11.0) + 6.0;
            }
            if let Some(mk) = props {
                let mw = if matches!(mk, PlayerMark::Level(_)) { 19.0 } else { 12.0 };
                let mut a = player_mark(env, mk);
                call(&mut a, "xy", &[x + w - right_w - mw / 2.0, cy]);
                c.push(a);
                right_w += mw + 6.0;
            }
            let mut nx = x + 29.0;
            if let Some(n) = leader {
                let col = if n == 1 { "Text" } else { "Secondary" };
                let mut ring = shape(env, col, [15.0, 15.0], [nx + 7.0, cy], 7.5, 0.0);
                if n != 1 {
                    call(&mut ring, "outline", &[1.3]);
                }
                c.push(ring);
                c.push({
                    let mut t = env.numbers("Leader", &ddi_l10n::UiText::data(n.to_string()), 10.0, if n == 1 { "OnAccent" } else { "Text" });
                    call(&mut t, "xy", &[nx + 7.0, cy + 0.5]);
                    t
                });
                nx += 19.0;
            }
            if style.marks_before {
                let slots = if style.reserved { if style.two { 2 } else { 1 } } else { roles.len() };
                for (i, mk) in roles.iter().enumerate() {
                    let mut a = player_mark(env, *mk);
                    call(&mut a, "xy", &[nx + 6.0 + i as f32 * 15.0, cy]);
                    c.push(a);
                }
                nx += slots as f32 * 15.0 + if slots > 0 { 3.0 } else { 0.0 };
            }
            let color = if p.me { "Text" } else if p.tone == rm::Tone::Away { "Faint" } else { "Secondary" };
            let mut name = env.text(&format!("Players{k}.Name"), &p.name, 13.0, if p.me { 700 } else { 400 }, color);
            look::ellipsized(&mut name, (w - (nx - x) - right_w).max(20.0));
            c.push(left(name, nx, cy));
        }
        if shown < rows.len() {
            let n = rows.len() - shown;
            let line: String = format!("+{n}");
            let _ = m::panel_more_below(n);
            c.push(left(env.text(rm::PLAYERS_MORE, &ddi_l10n::UiText::data(&line), 13.0, 600, "Muted"), x + 29.0, top + shown as f32 * ROW + ROW / 2.0));
        }
        frame("RoomPlayers", c)
    }
}

/// A players' row's icon from what the room says of it (its words).
fn icon_of(p: &crate::screens::room::PlayerRow, word: &dyn Fn(fn() -> ddi_l10n::UiText) -> String) -> PlayerIcon {
    use ddi_l10n::msg::room as m;
    let s = p.status.as_str();
    let st = p.state.as_str();
    let is = |f: fn() -> ddi_l10n::UiText| {
        let w = word(f);
        !w.is_empty() && (s == w || st == w)
    };
    if is(m::state_ready) || p.ready {
        PlayerIcon::Ready
    } else if is(m::state_playing) {
        PlayerIcon::Playing
    } else if is(m::state_not_ready) {
        PlayerIcon::NotReady
    } else if is(m::state_waiting) {
        PlayerIcon::WaitingSong
    } else if is(m::state_loading) || is(m::state_retrying) || s.contains('%') {
        let pct: String = s.chars().filter(char::is_ascii_digit).collect();
        PlayerIcon::Loading(pct.parse::<f32>().map_or(0.3, |v| v / 100.0))
    } else if p.tone == crate::screens::room::Tone::Failed {
        PlayerIcon::LoadFailed
    } else if is(m::watching) {
        PlayerIcon::Watching
    } else if is(m::conn_joining) || is(m::conn_reconnecting) || is(m::conn_away) {
        PlayerIcon::Away
    } else if is(m::state_sitting_out)
        || is(m::state_sat_out_not_ready)
        || is(m::state_sat_out_loading)
        || is(m::state_sat_out_failed)
        || is(m::state_sat_out_away)
    {
        PlayerIcon::SatOut
    } else {
        PlayerIcon::Idle
    }
}

impl SelectScreen {
    /// Round 4: the list's scrollbar beside the list where the players
    /// column takes the screen's right edge.
    pub(super) fn study_scrollbar_x(&self, l: &Layout) -> Option<f32> {
        let Some(Study::Wide(g)) = l.study else {
            return None;
        };
        let c = g.column?;
        if round5().on && c[0] > l.wheel[0] {
            return Some(c[0] - 16.0 - 11.5);
        }
        (c[0] > l.wheel[0]).then(|| l.wheel[0] + l.wheel[2] + 14.0)
    }
}

/// Round 4's state icons in one ink: Muted, Text where a state asks for
/// attention (not ready, a failed load), Faint where nothing goes on
/// (before a pick, away); `accents`: ready green and a failed load red.
fn player_icon_mono(env: &crate::screens::Env, icon: PlayerIcon, accents: bool) -> Actor {
    let ink = match icon {
        PlayerIcon::Ready if accents => "Difficulty.Easy",
        PlayerIcon::LoadFailed if accents => "Danger",
        PlayerIcon::NotReady | PlayerIcon::LoadFailed => "Text",
        PlayerIcon::Idle | PlayerIcon::Away | PlayerIcon::Left => "Faint",
        _ => "Muted",
    };
    let c: Vec<Actor> = match icon {
        // A short dash: nothing yet.
        PlayerIcon::Idle => vec![shape(env, ink, [7.0, 2.0], [0.0, 0.0], 1.0, 0.0)],
        PlayerIcon::WaitingSong => [-4.5, 0.0, 4.5]
            .iter()
            .map(|x| shape(env, ink, [3.0, 3.0], [*x, 0.0], 1.5, 0.0))
            .collect(),
        // An arrow down to a line, the line filled to the progress.
        PlayerIcon::Loading(p) => {
            let w = (12.0 * p.clamp(0.0, 1.0)).max(2.4);
            vec![
                shape(env, ink, [2.2, 7.0], [0.0, -3.0], 1.1, 0.0),
                shape(env, ink, [2.2, 5.5], [-1.7, 0.6], 1.1, -45.0),
                shape(env, ink, [2.2, 5.5], [1.7, 0.6], 1.1, 45.0),
                {
                    let mut t = shape(env, ink, [12.0, 2.6], [0.0, 5.6], 1.3, 0.0);
                    call(&mut t, "diffusealpha", &[0.35]);
                    t
                },
                shape(env, ink, [w, 2.6], [-6.0 + w / 2.0, 5.6], 1.3, 0.0),
            ]
        }
        // A triangle's "!": a filled disc with a dark "!" (attention).
        PlayerIcon::LoadFailed => vec![
            shape(env, ink, [13.0, 13.0], [0.0, 0.0], 6.5, 0.0),
            shape(env, "Ground", [2.0, 5.5], [0.0, -1.4], 1.0, 0.0),
            shape(env, "Ground", [2.2, 2.2], [0.0, 3.4], 1.1, 0.0),
        ],
        // An open ring (attention: still to press Ready).
        PlayerIcon::NotReady => vec![ring(env, ink, 12.0, 1.8, [0.0, 0.0])],
        // A filled disc with a dark check.
        PlayerIcon::Ready => vec![
            shape(env, ink, [13.0, 13.0], [0.0, 0.0], 6.5, 0.0),
            shape(env, "Ground", [2.2, 4.6], [-2.3, 1.0], 1.1, -45.0),
            shape(env, "Ground", [2.2, 8.0], [1.4, -0.4], 1.1, 40.0),
        ],
        // Three rising bars.
        PlayerIcon::Playing => [-4.0, 0.0, 4.0]
            .iter()
            .zip([6.0, 9.0, 12.0])
            .map(|(x, h)| shape(env, ink, [2.6, h], [*x, 6.0 - h / 2.0], 1.0, 0.0))
            .collect(),
        // Two falling bars and an arrow down: life ran out, plays on.
        PlayerIcon::FailedPlaying => vec![
            shape(env, ink, [2.6, 10.0], [-5.0, 1.0], 1.0, 0.0),
            shape(env, ink, [2.6, 6.0], [-1.0, 3.0], 1.0, 0.0),
            shape(env, ink, [2.0, 6.0], [4.0, -1.5], 1.0, 0.0),
            shape(env, ink, [2.0, 4.0], [2.9, 1.8], 1.0, -45.0),
            shape(env, ink, [2.0, 4.0], [5.1, 1.8], 1.0, 45.0),
        ],
        PlayerIcon::Out => vec![
            shape(env, ink, [2.4, 12.0], [0.0, 0.0], 1.2, 45.0),
            shape(env, ink, [2.4, 12.0], [0.0, 0.0], 1.2, -45.0),
        ],
        // A ring struck through: not in this stage.
        PlayerIcon::SatOut => vec![
            ring(env, ink, 12.0, 1.6, [0.0, 0.0]),
            shape(env, ink, [1.8, 12.0], [0.0, 0.0], 0.9, 45.0),
        ],
        PlayerIcon::Left => vec![
            shape(env, ink, [2.0, 11.0], [-4.5, 0.0], 1.0, 0.0),
            shape(env, ink, [8.0, 2.0], [0.5, 0.0], 1.0, 0.0),
            shape(env, ink, [2.0, 5.0], [3.2, -1.6], 1.0, -45.0),
            shape(env, ink, [2.0, 5.0], [3.2, 1.6], 1.0, 45.0),
        ],
        PlayerIcon::Watching => vec![
            {
                let mut e = shape(env, ink, [13.0, 8.0], [0.0, 0.0], 4.0, 0.0);
                call(&mut e, "outline", &[1.5]);
                e
            },
            shape(env, ink, [4.0, 4.0], [0.0, 0.0], 2.0, 0.0),
        ],
        PlayerIcon::Away => vec![
            shape(env, ink, [2.6, 10.0], [-2.4, 0.0], 1.0, 0.0),
            shape(env, ink, [2.6, 10.0], [2.4, 0.0], 1.0, 0.0),
        ],
    };
    frame("Icon", c)
}
