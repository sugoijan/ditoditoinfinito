//! Song select's song side (round SLB, 2026-11-20: the select-layout
//! study's B2 as the maintainer decided it; `docs/plans/song-navigation.md`,
//! "The song's side"): the wide layout's title band over two equal
//! squares, the cover (the look's `SongCover`: the song's cover, else its
//! banner whole over its blurred copy, else a title plate; its preview
//! movie faded in over it) and the radar, then the style tabs, the chart
//! buttons and the chosen chart's strip; a phone's short side (the square
//! beside the title, `SongInfo.Short`); a room's wide layout in three
//! parts, the song's side where solo has it, the list, and the players
//! panel at the right (`select_players.rs`).

use ddi_actor::{Actor, Kind};

use super::{Layout, SelectScreen, data, version_name};
use crate::hit;
use crate::look::{self, Spec, call, quad};
use crate::screen::Cx;
use crate::screens::data::{ChartEntry, SongEntry};
use crate::screens::{frame, left, right};
use crate::widgets::{part, set_image, set_text};
use ddi_l10n::msg::select as msg;

/// The wide song side's parts, in the side's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Geom {
    /// The cover square: x, y, side.
    pub cover: [f32; 3],
    /// The title band (`SongInfo.Text`).
    pub text: [f32; 4],
    /// The style tabs: x, middle, wide.
    pub tabs: [f32; 3],
    /// The chart buttons: x, middle, wide, high.
    pub buttons: [f32; 4],
    /// The chosen chart's strip, and whether it is tall enough for the
    /// larger number.
    pub card: [f32; 4],
    pub big: bool,
    /// The radar's square.
    pub radar: [f32; 4],
    /// A room's players panel's column (in the screen's frame).
    pub column: Option<[f32; 4]>,
}

/// The song side's form.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum SongSide {
    /// The title band over the cover and the radar.
    Wide(Geom),
    /// A phone's: the square beside the title.
    Short,
}

/// The wide side for a side `w` wide and `h` high: the title band, the
/// cover square and the radar beside it (as large as the width and the
/// height allow), the tabs, the buttons and the chosen chart's strip;
/// height to spare grows the strip, then centers the block.
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
        cover: [0.0, y, s],
        text: [0.0, dy, w, band],
        tabs: [0.0, sb + 30.0, w],
        buttons: [0.0, sb + 78.0, w, 44.0],
        card: [0.0, sb + below, w, strip],
        big: strip >= 130.0,
        radar: [s + 16.0, y, w - s - 16.0, s],
        column: None,
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

/// How long the square's preview movie takes to fade in or out.
pub(super) const MOVIE_FADE: f32 = 0.25;

/// The room kept between a title running past the song side and the list's
/// rows, and for the heart after it.
const TITLE_GAP: f32 = 12.0;
const HEART_ROOM: f32 = 34.0;

impl SelectScreen {
    /// The title's room (`None`: the band's own): from its left to
    /// where the list's rows can reach at the title's height whatever the
    /// list's scroll (the rows drift right by their distance from the
    /// wheel's focus line, `Look::wheel`: the nearest a row can come to the
    /// title is half a row nearer than the band's foot), less the heart's
    /// room. Alone; a room's list has its tabs and status line at its top.
    pub(super) fn title_room(&self) -> Option<f32> {
        let g = self.side_wide()?;
        if self.room.is_some() {
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

    /// `l` with its song side: a phone's short side; a room's wide layout
    /// in three parts (the song's side as solo has it, the list, the
    /// players panel at the right, open or folded to its rail); solo's
    /// title band and squares.
    pub(super) fn side_layout(&self, mut l: Layout) -> Layout {
        l.side = None;
        let h = l.ladder[0] + l.ladder[1] - l.song[1];
        if l.compact {
            l.side = Some(SongSide::Short);
            return l;
        }
        if self.room.is_some() {
            let folded = self.room.as_ref().is_some_and(|r| r.folded);
            let solo = Layout::fit(&self.env, l.vp, l.compact);
            let side = solo.song[2];
            let [sx, sy, _] = solo.song;
            let line = super::players::line_x(l.screen[0], l.margin, folded);
            let column = [line + super::players::LINE_GAP, sy, super::players::PANEL_W, h];
            let wx = solo.wheel[0];
            l.chips = false;
            l.players = None;
            l.song = [sx, sy, side];
            l.wheel = [wx, solo.wheel[1], line - super::players::LIST_GAP - wx, solo.wheel[3]];
            l.info_w = side;
            let mut g = geom_b2(side, h);
            g.column = Some(column);
            l.radar = true;
            l.tabs_y = l.song[1] + g.tabs[1] - 16.0;
            l.side = Some(SongSide::Wide(g));
            return l;
        }
        l.radar = true;
        let g = geom_b2(l.song[2], h);
        l.tabs_y = l.song[1] + g.tabs[1] - 16.0;
        l.side = Some(SongSide::Wide(g));
        l
    }


    /// The wide side's geometry (none: a phone's short side).
    pub(super) fn side_wide(&self) -> Option<Geom> {
        match self.layout.side {
            Some(SongSide::Wide(g)) => Some(g),
            _ => None,
        }
    }

    /// The song's info: the cover square and the title band (wide), or a
    /// phone's short side.
    pub(super) fn side_info(&self, l: &Layout, info_h: f32) -> Actor {
        let env = &self.env;
        match l.side {
            Some(SongSide::Wide(g)) => {
                let [cx, cy, cs] = g.cover;
                let mut cover = env.component("SongCover", "Panel", &Spec::new([cs, cs], ""), "Cover", [cx, cy]);
                cover.name = "Cover".into();
                let [tx, ty, tw, th] = g.text;
                let text = env.component(
                    "SongInfo.Text",
                    "Panel",
                    &Spec::new([tw, th], "").with_items(["top5".to_string()]),
                    "InfoText",
                    [tx, ty],
                );
                frame("Info", vec![cover, text])
            }
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
    /// buttons, the chosen chart's strip, the radar's slot.
    pub(super) fn side_parts(&self, g: &Geom) -> Vec<Actor> {
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
        out
    }

    /// The tabs' room across (`fill_tabs`).
    pub(super) fn tabs_room(&self) -> f32 {
        match self.side_wide() {
            Some(g) => g.tabs[2] - 48.0,
            None => self.layout.song[2] - 48.0,
        }
    }

    /// The radar's square (the wide side's).
    pub(super) fn side_radar_area(&self) -> Option<([f32; 2], [f32; 4])> {
        let r = self.side_wide()?.radar;
        Some(([r[1], r[3]], r))
    }

    /// The square and the title, filled for `song`.
    pub(super) fn fill_side_info(&self, info: &mut Actor, song: Option<&SongEntry>, cx: &mut Cx) {
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
        // The wide side's title may run past the side into the width the list's
        // curve leaves clear at its height.
        if let Some(w) = self.title_room()
            && let Some(t) = part(info, "Title")
        {
            look::ellipsized(t, w);
        }
        // The art: the cover, else the banner, the blurred copy of the
        // still under it; the preview movie whole over the blur while its
        // frames come, faded in over the art (and the art back as it
        // ends), at once under reduced motion.
        let still = song.jacket.as_deref().or(song.banner.as_deref());
        let movie = self.movie.as_deref() == Some(song.id.as_str());
        self.fade_movie(info, movie.then(|| cx.texture(super::PREVIEW_MOVIE, 512.0)), &song.id);
        let art = still.map(|s| cx.texture(s, 512.0));
        let blur = still.map(|s| cx.blurred_texture(s, 128.0));
        set_image(info, "CoverArt", art);
        set_image(info, "CoverBlur", blur);
        // Neither: the plate with the title set in it.
        let none = art.is_none() && !movie;
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

    /// The square's preview movie (`CoverMovie`): `movie`'s frames faded
    /// in over the art in [`MOVIE_FADE`] (the art out as it comes), and
    /// out again (the art back) when it ends on the same song; a new song
    /// shows its own art at once; under reduced motion every change at
    /// once.
    fn fade_movie(&self, info: &mut Actor, movie: Option<u32>, song: &str) {
        let shown = self.movie_shown.borrow().as_deref() == Some(song);
        let t = if self.reduced { 0.0 } else { MOVIE_FADE };
        // The movie's alpha and the art's, and whether they fade.
        let (to, fade) = match (movie.is_some(), shown) {
            // Coming in, or going on the song it played on.
            (true, false) | (false, true) => (if movie.is_some() { 1.0 } else { 0.0 }, true),
            // Playing on: the frames alone.
            (true, true) => (1.0, false),
            // Another song: its art at once.
            (false, false) => (0.0, false),
        };
        let cmd = |alpha: f32| {
            if fade {
                format!("stoptweening;linear,{t};diffusealpha,{alpha}")
            } else {
                format!("stoptweening;diffusealpha,{alpha}")
            }
        };
        if let Some(m) = part(info, "CoverMovie") {
            if let Some(tex) = movie
                && let Kind::Sprite(sp) = &mut m.kind
            {
                sp.texture = tex;
            }
            m.visible = movie.is_some() || shown;
        }
        // (Playing on: its fade, if any, goes on.)
        if !(movie.is_some() && shown) {
            for (name, alpha) in [("CoverMovie", to), ("CoverArt", 1.0 - to)] {
                if let Some(a) = part(info, name) {
                    look::command_of(a, "Fade", &cmd(alpha));
                    crate::widgets::play(a, "Fade");
                }
            }
        }
        *self.movie_shown.borrow_mut() = movie.map(|_| song.to_string());
    }

    /// The wide side's charts: the buttons in a row, the chosen chart's
    /// strip (the radar is `fill_radar`'s).
    pub(super) fn fill_side_charts(
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
                &Spec::new([w.max(g.radar[0] + g.radar[2] - x), 150.0], none.as_str()),
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
        // The chosen chart's notes in the line under the title.
        if let Some(info) = hit::find_mut(root, "Info") {
            let notes = match pick.map(|p| charts[p]) {
                Some(c) if !self.song().is_some_and(SongEntry::away) => c.notes.to_string(),
                _ => "–".to_string(),
            };
            set_text(info, "Notes", &data(&notes));
        }
        let card = pick.map(|p| self.chart_strip(charts[p], g)).unwrap_or_default();
        if let Some(f) = hit::find_mut(root, "ChartCard") {
            f.kind = Kind::Frame(card);
        }
    }

    /// The chosen chart's lines in the candidate's card: its difficulty
    /// (or name), its number as its ladder row showed it, the song's own
    /// number where a rating takes its place, its version, notes, step
    /// artist, the estimate's unit line, and its best.
    /// The chosen chart's strip: its difficulty (or name) and number at
    /// the left, the song's own number where a rating takes its place,
    /// its version, step artist and the estimate's unit line in the
    /// middle, its best at the right (its notes are in the line under
    /// the title).
    fn chart_strip(&self, c: &ChartEntry, g: &Geom) -> Vec<Actor> {
        let env = &self.env;
        let [x0, y0, w0, h0] = g.card;
        let mut out = Vec::new();
        let pad = 12.0;
        let mut p = quad("CardPlate", [w0, h0], env.color("Radar.Panel"));
        call(&mut p, "cornerradius", &[w0.min(h0) * 0.06]);
        call(&mut p, "xy", &[x0 + w0 / 2.0, y0 + h0 / 2.0]);
        out.push(p);
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
            let grade = env.numbers("Card.Grade", &data(&b.grade), 22.0, "Text");
            let score = if b.pending {
                format!("{} …", b.score)
            } else {
                b.score.clone()
            };
            let s = env.text("Card.Score", &data(&score), 12.0, 400, "Muted");
            let lamp = env.text("Card.Lamp", &data(&b.lamp_text), 11.0, 700, &format!("Lamp.{}", b.lamp));
            (b.grade.clone(), grade, s, lamp)
        });
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

        out
    }

    /// The room's players panel's column, in the three parts.
    pub(super) fn players_column(&self) -> Option<[f32; 4]> {
        self.side_wide()?.column
    }

    /// The list's scrollbar: by the players panel's line in a room's three
    /// parts (as by the screen's edge alone).
    pub(super) fn scrollbar_x(&self, l: &Layout) -> Option<f32> {
        let Some(SongSide::Wide(g)) = l.side else {
            return None;
        };
        g.column?;
        Some(l.wheel[0] + l.wheel[2] + super::players::LIST_GAP - 11.5)
    }
}
