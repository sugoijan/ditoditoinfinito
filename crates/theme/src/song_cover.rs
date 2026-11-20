//! TESTING AID (the select-layout study, 2026-11-19): the parts of song
//! select's song side that its candidate layouts draw, only where one is
//! chosen (`ddi_ui::screens::select::SelectLayout`; the default layout
//! never asks for them).
//!
//! - `SongCover`: the song's one picture, a square. Its cover art whole
//!   in it (`Fit::Contain`), over the same art zoomed to fill the square
//!   and blurred (the menus' cached Gaussian copy, made once per image),
//!   darkened so the sharp art reads; with no cover, the banner whole
//!   across the square's width over its own blurred copy; with neither, a
//!   quiet plate with the song's title set in it (the screen fills the
//!   lines, `CoverTitle0..2` and `CoverArtist`).
//! - `SongInfo.Text`: the title block (`items[0]`: `beside`, `top` or
//!   `column`), the same parts as `SongInfo`'s so the screen fills them
//!   alike, the title on up to two lines (`Title`, `Title2`; what follows
//!   it in `AfterTitle`, moved by `OneLine` and `TwoLines`).
//! - `SongInfo.Short`: a phone's and a room's short side, the square at
//!   the left and the title and its lines beside it.

use ddi_actor::{Actor, Fit, Kind, Sprite};
use ddi_ui::look::{Spec, call, command_of, mono_width, named, tracked};
use ddi_ui::screens::{frame, left as at_left, right as at_right};

use super::{FAVORITE, FAVORITE_SMALL, Infinito, extent, favorite_slot, max_width, outline_r, plate, round};

/// The square's corners.
const COVER_R: f32 = 8.0;
/// The blurred copy under art that does not fill the square: its
/// brightness (a multiplier) and saturation, so the sharp art reads over
/// it and a busy banner does not compete with itself.
pub const COVER_BLUR_DIM: f32 = 0.55;
pub const COVER_BLUR_SATURATION: f32 = 0.85;

/// A sprite `size` large, its top left at the origin, hidden until the
/// screen puts a texture on it.
fn picture(name: &str, size: [f32; 2], fit: Fit, r: f32) -> Actor {
    let mut s = named(Kind::Sprite(Sprite::quad(size)), name);
    if let Kind::Sprite(sp) = &mut s.kind {
        sp.fit = fit;
    }
    call(&mut s, "cornerradius", &[r]);
    call(&mut s, "halign", &[0.0]);
    call(&mut s, "valign", &[0.0]);
    s.visible = false;
    s
}

impl Infinito {
    /// The song's square ([`COVER_BLUR_DIM`]), `spec.size[0]` on a side.
    pub(crate) fn song_cover(&self, spec: &Spec) -> Actor {
        let s = spec.size[0];
        let back = round(plate("CoverBack", [0.0, 0.0], [s, s], self.c("BannerEmpty")), COVER_R);
        // The placeholder: an accent tick at the top left, the title set
        // in the lower left (up to three lines, filled from the bottom),
        // the artist under it.
        let pad = (s * 0.08).round().clamp(8.0, 20.0);
        let size = (s * 0.1).clamp(11.0, 24.0);
        let small = (size * 0.6).max(10.0);
        let mut placeholder = vec![plate(
            "CoverTick",
            [pad, pad],
            [(size * 1.4).round(), 3.0],
            self.c("Accent"),
        )];
        let artist_y = s - pad - small * 0.5;
        let mut artist = at_left(self.text("CoverArtist", "", small, 400, "Muted"), pad, artist_y);
        max_width(&mut artist, s - 2.0 * pad);
        for k in 0..3 {
            let y = artist_y - small * 0.5 - 8.0 - size * 0.62 - (2 - k) as f32 * size * 1.2;
            let mut t = at_left(self.text(&format!("CoverTitle{k}"), "", size, 700, "Secondary"), pad, y);
            max_width(&mut t, s - 2.0 * pad);
            placeholder.push(t);
        }
        placeholder.push(artist);
        let mut placeholder = frame("CoverPlaceholder", placeholder);
        placeholder.visible = false;
        let mut blur = picture("CoverBlur", [s, s], Fit::Cover, COVER_R);
        let d = COVER_BLUR_DIM;
        call(&mut blur, "diffuse", &[d, d, d, 1.0]);
        call(&mut blur, "saturation", &[COVER_BLUR_SATURATION]);
        let art = picture("CoverArt", [s, s], Fit::Contain, 4.0);
        // The CD title, a small mark in the lower left corner, whole.
        let scale = (s / 260.0).clamp(0.5, 1.0);
        let [cw, ch] = [
            self.m("Select.CdTitle.Width") * scale,
            self.m("Select.CdTitle.Height") * scale,
        ];
        let mut cd = picture("CdTitle", [cw, ch], Fit::Contain, 0.0);
        call(&mut cd, "xy", &[8.0 * scale, s - 8.0 * scale - ch]);
        let edge = outline_r("CoverEdge", [0.0, 0.0, s, s], 1.0, [1.0, 1.0, 1.0, 0.08], COVER_R);
        frame("", vec![back, placeholder, blur, art, cd, edge, extent([s, s])])
    }

    /// A label (small tracked capitals) and its value on one of the
    /// stacked rows, the values from `vx`.
    fn spec_row(&self, label: &str, value: Actor, vx: f32, y: f32, name: &str) -> Vec<Actor> {
        let l = at_left(tracked(self.text(name, label, 11.0, 400, "Muted"), 80), 0.0, y + 1.0);
        let mut v = value;
        call(&mut v, "xy", &[vx, y]);
        call(&mut v, "halign", &[0.0]);
        vec![l, v]
    }

    /// The title block (`items[0]`: `beside` the square, on `top` of the
    /// side, or a `column` under the square).
    pub(crate) fn song_text(&self, spec: &Spec) -> Actor {
        let [w, h] = spec.size;
        let style = spec.items.first().map_or("top", String::as_str);
        let bpm_label = "BPM";
        let length_label = ddi_l10n::msg::select::info_length();
        let pack_label = ddi_l10n::msg::select::info_pack();
        let label_w = |s: &str| mono_width(s, 11.0) + s.chars().count() as f32 * 11.0 * 0.08;
        let vx = [bpm_label, length_label.as_str(), pack_label.as_str()]
            .iter()
            .map(|s| label_w(s))
            .fold(0.0_f32, f32::max)
            + 14.0;
        let mut c = Vec::new();
        match style {
            // Round 5: under the title, Length, the chosen chart's Notes,
            // BPM (the pack dropped).
            "top5" => {
                let ty = 18.0;
                let mut title = at_left(self.text("Title", "", 30.0, 700, "Text"), 0.0, ty);
                max_width(&mut title, w - FAVORITE - 16.0);
                let mut artist = at_left(self.text("Artist", "", 17.0, 400, "Secondary"), 0.0, ty + 32.0);
                max_width(&mut artist, w * 0.5);
                let mut credit = at_right(self.text("Credit", "", 12.0, 400, "Muted"), w, ty + 33.0);
                max_width(&mut credit, w * 0.46);
                let notes_label = ddi_l10n::msg::select::info_notes();
                let sy = ty + 60.0;
                let lx = |label: &str, x: f32| x + label_w(label) + 8.0;
                let length_x = lx(&length_label, 0.0);
                let notes_at = length_x + 62.0;
                let notes_x = lx(&notes_label, notes_at);
                let bpm_at = notes_x + 62.0;
                let bpm_x = lx(bpm_label, bpm_at);
                let after = frame(
                    "AfterTitle",
                    vec![
                        artist,
                        credit,
                        at_left(tracked(self.text("LengthLabel", &length_label, 11.0, 400, "Muted"), 80), 0.0, sy + 1.0),
                        at_left(self.num("Length", "", 18.0, "Text"), length_x, sy),
                        at_left(tracked(self.text("NotesLabel", &notes_label, 11.0, 400, "Muted"), 80), notes_at, sy + 1.0),
                        at_left(self.num("Notes", "", 18.0, "Text"), notes_x, sy),
                        at_left(tracked(self.text("BpmLabel", bpm_label, 11.0, 400, "Muted"), 80), bpm_at, sy + 1.0),
                        at_left(self.num("Bpm", "", 18.0, "Text"), bpm_x, sy),
                        at_right(self.text("Offset", "", 13.0, 600, "Accent"), w, sy),
                    ],
                );
                let _ = pack_label;
                c.extend([title, after, favorite_slot(FAVORITE, [w - FAVORITE * 0.55, ty])]);
            }
            "top" => {
                let ty = 18.0;
                let mut title = at_left(self.text("Title", "", 30.0, 700, "Text"), 0.0, ty);
                max_width(&mut title, w - FAVORITE - 16.0);
                // One line across the side: what follows has its place.
                let mut artist = at_left(self.text("Artist", "", 17.0, 400, "Secondary"), 0.0, ty + 32.0);
                max_width(&mut artist, w * 0.5);
                let mut credit = at_right(self.text("Credit", "", 12.0, 400, "Muted"), w, ty + 33.0);
                max_width(&mut credit, w * 0.46);
                // The stats on one line: each value after its label, the
                // next label after the value's room.
                let sy = ty + 60.0;
                let lx = |label: &str, x: f32| x + label_w(label) + 8.0;
                let bpm_x = lx(bpm_label, 0.0);
                let length_at = bpm_x + 92.0;
                let length_x = lx(&length_label, length_at);
                let pack_at = length_x + 64.0;
                let pack_x = lx(&pack_label, pack_at);
                let mut pack = at_left(self.text("Pack", "", 14.0, 600, "Text"), pack_x, sy);
                max_width(&mut pack, w - pack_x - 70.0);
                let after = frame(
                    "AfterTitle",
                    vec![
                        artist,
                        credit,
                        at_left(tracked(self.text("BpmLabel", bpm_label, 11.0, 400, "Muted"), 80), 0.0, sy + 1.0),
                        at_left(self.num("Bpm", "", 18.0, "Text"), bpm_x, sy),
                        at_left(
                            tracked(self.text("LengthLabel", &length_label, 11.0, 400, "Muted"), 80),
                            length_at,
                            sy + 1.0,
                        ),
                        at_left(self.num("Length", "", 18.0, "Text"), length_x, sy),
                        at_left(
                            tracked(self.text("PackLabel", &pack_label, 11.0, 400, "Muted"), 80),
                            pack_at,
                            sy + 1.0,
                        ),
                        pack,
                        at_right(self.text("Offset", "", 13.0, 600, "Accent"), w, sy),
                    ],
                );
                let mut after = after;
                command_of(&mut after, "OneLine", "y,0");
                command_of(&mut after, "TwoLines", "y,34");
                c.extend([title, after, favorite_slot(FAVORITE, [w - FAVORITE * 0.55, ty])]);
            }
            _ => {
                // Beside the square (its height), or a column under it.
                let column = style == "column";
                let (ts, lh) = if column { (22.0, 27.0) } else { (26.0, 31.0) };
                let ty = ts * 0.62;
                let mut title = at_left(self.text("Title", "", ts, 700, "Text"), 0.0, ty);
                max_width(&mut title, w - FAVORITE - 12.0);
                let mut title2 = at_left(self.text("Title2", "", ts, 700, "Text"), 0.0, ty + lh);
                max_width(&mut title2, w);
                let ay = ty + ts * 0.62 + 16.0;
                let mut artist = at_left(self.text("Artist", "", 16.0, 400, "Secondary"), 0.0, ay);
                max_width(&mut artist, w);
                let mut credit = at_left(self.text("Credit", "", 12.0, 400, "Muted"), 0.0, ay + 21.0);
                max_width(&mut credit, w);
                let mut after = vec![artist, credit];
                // The stats stacked, at the block's foot beside the
                // square, under the credit in a column.
                let rows = if column { [ay + 54.0, ay + 80.0, ay + 106.0] } else { [h - 58.0, h - 33.0, h - 8.0] };
                let mut pack = self.text("Pack", "", 14.0, 600, "Text");
                max_width(&mut pack, w - vx);
                let mut stats = Vec::new();
                stats.extend(self.spec_row(bpm_label, self.num("Bpm", "", 18.0, "Text"), vx, rows[0], "BpmLabel"));
                stats.extend(self.spec_row(&length_label, self.num("Length", "", 18.0, "Text"), vx, rows[1], "LengthLabel"));
                stats.extend(self.spec_row(&pack_label, pack, vx, rows[2], "PackLabel"));
                stats.push(at_right(self.text("Offset", "", 13.0, 600, "Accent"), w, rows[0]));
                if column {
                    after.extend(stats);
                } else {
                    c.extend(stats);
                }
                let mut after = frame("AfterTitle", after);
                command_of(&mut after, "OneLine", "y,0");
                command_of(&mut after, "TwoLines", &format!("y,{lh}"));
                c.extend([title, title2, after, favorite_slot(FAVORITE, [w - FAVORITE * 0.55, ty])]);
            }
        }
        c.push(extent([w, h]));
        frame("", c)
    }

    /// A phone's and a room's short side: the square at the left (as tall
    /// as the side), the title (two lines) and its line beside it.
    pub(crate) fn song_short(&self, spec: &Spec) -> Actor {
        let [w, h] = spec.size;
        // A room's (round 2): the title on one line, the chart's row under
        // it beside the square, no pack.
        let one = spec.items.first().is_some_and(|i| i == "oneline");
        let q = (h - 12.0).min(w * 0.45).round();
        let mut cover = self.song_cover(&Spec::new([q, q], ""));
        cover.name = "Cover".into();
        let x = q + 12.0;
        let tw = w - x;
        let ts = if tw > 220.0 { 19.0 } else { 17.0 };
        let mut title = at_left(self.text("Title", "", ts, 700, "Text"), x, ts * 0.62);
        max_width(&mut title, tw - FAVORITE_SMALL - 8.0);
        let mut title2 = at_left(self.text("Title2", "", ts, 700, "Text"), x, ts * 0.62 + ts * 1.2);
        max_width(&mut title2, tw);
        let mut line = at_left(self.text("Line", "", 13.0, 400, "Muted"), x, ts * 1.24 + 14.0);
        max_width(&mut line, tw);
        let mut pack = at_left(self.text("Pack", "", 12.0, 400, "Faint"), x, ts * 1.24 + 33.0);
        max_width(&mut pack, tw);
        let mut after = frame("AfterTitle", if one { vec![line] } else { vec![line, pack] });
        command_of(&mut after, "OneLine", "y,0");
        command_of(&mut after, "TwoLines", &format!("y,{}", ts * 1.2));
        frame(
            "",
            vec![
                cover,
                title,
                if one { frame("", Vec::new()) } else { title2 },
                after,
                favorite_slot(FAVORITE_SMALL, [w - FAVORITE_SMALL * 0.55, ts * 0.62]),
                extent([w, h]),
            ],
        )
    }
}

/// B2's heart right after the title as drawn (the title's measured width,
/// squeezed to its room at most), so a title running past the song side
/// carries its heart along.
pub(crate) fn place_heart(root: &mut Actor, measure: &mut dyn ddi_actor::text::TextMeasure) {
    let Some(path) = ddi_ui::hit::find(root, "NotesLabel") else {
        return;
    };
    // The block holding the title band: two up from the label.
    if path.len() < 2 {
        return;
    }
    let Some(block) = ddi_ui::hit::actor_at_mut(root, &path[..path.len() - 2]) else {
        return;
    };
    let width = block.children_mut().iter_mut().find(|a| a.name == "Title").and_then(|t| {
        t.measure(measure);
        let Kind::Text(x) = &t.kind else {
            return None;
        };
        let advance = x.metrics()?.advance;
        let max = x.max_width;
        Some(if max > 0.0 { advance.min(max) } else { advance })
    });
    let Some(width) = width else {
        return;
    };
    if let Some(slot) = block.children_mut().iter_mut().find(|a| a.name == "FavoriteSlot") {
        let x = width + 10.0 + FAVORITE * 0.5;
        if (slot.current().pos[0] - x).abs() > 0.01 {
            call(slot, "x", &[x]);
        }
    }
}
