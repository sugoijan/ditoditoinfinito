//! Infinito, our theme (`docs/plans/theme.md`, `docs/plans/ui-core.md`
//! step 5): a [`Look`] whose components are the maintainer's mockups
//! (layout A), its colors, faces, sizes and timings read from
//! `infinito.toml`, embedded.
//!
//! - **Ground** near black, plates a step up, the selection raised and
//!   outlined in the Vivid paint (`active`); color comes from the songs' art and the
//!   difficulty and judgement colors, never from the chrome.
//! - **Text** in Iosevka Term (the system's fonts behind it for
//!   Japanese and the like), **numbers** in the HUD's Iosevka ExtraBold.
//! - Corners are square: the painter draws no rounded rectangles yet
//!   (the mockups' 4–10 px radii wait for it).
//!
//! The screens are `ddi_ui::screens`; this crate only says how their
//! components look. Every component the plain look has is restyled
//! here, plus the screens' own (`SongList.Row`, `StyleTab`,
//! `ChartLadder.Row`, …; their parts are listed in
//! `ddi_ui::screens`).

use std::cell::RefCell;
use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::BTreeSet;
use std::sync::Arc;

use ddi_actor::{Actor, Kind, Rgba, Sprite};
use ddi_ui::look::{
    Look, Spec, WheelPlace, call, command_of, mono_width, named, quad, text_weighted, tracked,
};
use ddi_ui::screens::{frame, left as at_left, right as at_right};
/// The tunables as shipped (`build.rs` makes the tables below of it).
pub const TUNABLES: &str = include_str!("infinito.toml");

#[cfg(test)]
mod tunables;

mod keycaps;
pub use keycaps::KeycapMaterial;
mod active;
mod cursor;
mod rain;
mod song_cover;
mod wordmark;

mod tables {
    include!(concat!(env!("OUT_DIR"), "/tunables.rs"));
}

/// The colors of a CPU's level's stars (`ddi_ui::stars`), gold, silver
/// and the empty outline, as the tunables have them: what the play's
/// miniatures draw them in (`ddi_shell::opponents`), outside the look.
pub fn star_colors() -> [Rgba; 3] {
    tunable_colors(["Star.Gold", "Star.Silver", "Star.Empty"])
}

/// The tunables' colors of `names`, for what draws outside the look
/// (magenta for a name the tunables lack).
fn tunable_colors<const N: usize>(names: [&str; N]) -> [Rgba; N] {
    names.map(|name| {
        tables::COLORS
            .iter()
            .find(|(k, _)| *k == name)
            .map_or([1.0, 0.0, 1.0, 1.0], |(_, c)| *c)
    })
}

/// A battle's attack colors (the power bar's violet family), as the
/// tunables have them: the line and outline of who attacks whom, and the
/// glow under the line (`ddi_shell::attack_links`), outside the look.
pub fn attack_colors() -> [Rgba; 2] {
    tunable_colors(["Attack.Line", "Attack.Glow"])
}

/// The play's music volume's slider (`ddi_shell::play_volume`), as the
/// quick volume's popup has them: its plate, its track and outline, its
/// fill, and its knob and percentage.
pub fn volume_colors() -> [Rgba; 4] {
    tunable_colors(["Raised", "Line2", "Accent", "Secondary"])
}

/// DDI's tier colors under the theme's names (`Judgement.Ddi.W0` …
/// `Judgement.Ddi.Miss`, the Boo's `Judgement.Boo`), from the engine's
/// one table (`ddi_engine::rules::colors::DDI`) rather than the tunables,
/// so the screens and the play cannot drift apart. StepMania's set stays
/// in the tunables (`Judgement.W1` …), as it always was.
pub(crate) fn judgment_colors() -> impl Iterator<Item = (String, Rgba)> {
    use ddi_engine::rules::colors::DDI;
    DDI.named("Judgement.Ddi.")
        .into_iter()
        .filter(|(k, _)| !k.ends_with(".Boo"))
        .chain([("Judgement.Boo".to_string(), DDI.boo)])
}

/// Our theme.
pub struct Infinito {
    keycap_material: std::cell::Cell<KeycapMaterial>,
    /// The text face's stack, as the tunables write it.
    text_font: Arc<str>,
    /// That stack with the language's faces, and the language it is for
    /// ([`Infinito::text_font`]).
    text_stack: RefCell<(&'static str, Arc<str>)>,
    numbers_font: Arc<str>,
    numbers_weight: u16,
    colors: BTreeMap<String, Rgba>,
    metrics: BTreeMap<String, f32>,
    /// Every color and metric asked for, for the test that the file
    /// and the screens agree (kept in tests only).
    #[cfg(test)]
    used: RefCell<BTreeSet<String>>,
    /// How open the song wheel's room for its selected row is
    /// ([`Look::set_wheel_open`]).
    wheel_open: std::cell::Cell<f32>,
}

/// A color as command arguments.
fn rgba(c: Rgba) -> String {
    format!("{},{},{},{}", c[0], c[1], c[2], c[3])
}

impl Infinito {
    /// The theme from the shipped tunables.
    pub fn new() -> Infinito {
        Infinito {
            keycap_material: std::cell::Cell::new(KeycapMaterial::DeepGraySatin),
            text_font: tables::TEXT_FONT.into(),
            text_stack: RefCell::new(("", tables::TEXT_FONT.into())),
            numbers_font: tables::NUMBERS_FONT.into(),
            numbers_weight: tables::NUMBERS_WEIGHT,
            colors: tables::COLORS
                .iter()
                .map(|(k, c)| (k.to_string(), *c))
                .chain(judgment_colors())
                .collect(),
            metrics: tables::METRICS
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            #[cfg(test)]
            used: RefCell::default(),
            wheel_open: std::cell::Cell::new(1.0),
        }
    }

    /// Notes a name asked for (tests).
    #[cfg(test)]
    fn note(&self, kind: &str, name: &str) {
        self.used.borrow_mut().insert(format!("{kind}:{name}"));
    }

    #[cfg(not(test))]
    fn note(&self, _kind: &str, _name: &str) {}

    /// The color and metric names asked for so far.
    #[cfg(test)]
    pub fn used(&self) -> BTreeSet<String> {
        self.used.borrow().clone()
    }

    /// The color and metric names the file has.
    #[cfg(test)]
    pub fn keys(&self) -> BTreeSet<String> {
        self.colors
            .keys()
            .map(|k| format!("color:{k}"))
            .chain(self.metrics.keys().map(|k| format!("metric:{k}")))
            .chain(["metric:Numbers.Weight".to_string()])
            .collect()
    }

    fn c(&self, name: &str) -> Rgba {
        self.note("color", name);
        self.colors
            .get(name)
            .copied()
            .unwrap_or([1.0, 0.0, 1.0, 1.0])
    }

    fn m(&self, name: &str) -> f32 {
        self.note("metric", name);
        self.metrics.get(name).copied().unwrap_or(0.0)
    }

    /// The text face's stack in the UI's language: the language's faces
    /// after the text face (`ddi_l10n::font_stack`), so screens built
    /// after a change of language draw its script in its faces.
    fn text_font(&self) -> Arc<str> {
        let tag = ddi_l10n::current().tag;
        let mut stack = self.text_stack.borrow_mut();
        if stack.0 != tag {
            *stack = (tag, ddi_l10n::font_stack(&self.text_font).into());
        }
        stack.1.clone()
    }

    /// Text in the text face.
    fn text(&self, name: &str, s: &str, size: f32, weight: u16, color: &str) -> Actor {
        text_weighted(&self.text_font(), weight, name, s, size, self.c(color))
    }

    /// Numbers in the numbers face.
    fn num(&self, name: &str, s: &str, size: f32, color: &str) -> Actor {
        text_weighted(
            &self.numbers_font,
            self.numbers_weight,
            name,
            s,
            size,
            self.c(color),
        )
    }

    /// `command` setting a color, tweened over the selection's time.
    fn tint(&self, color: &str) -> String {
        format!(
            "stoptweening;decelerate,{};diffuse,{}",
            self.m("Motion.Select"),
            rgba(self.c(color))
        )
    }

    fn tinted(&self, a: &mut Actor, command: &str, color: &str) {
        command_of(a, command, &self.tint(color));
    }

    /// `command` setting a color at once.
    fn colored(&self, a: &mut Actor, command: &str, color: &str) {
        command_of(a, command, &format!("diffuse,{}", rgba(self.c(color))));
    }
}

impl Default for Infinito {
    fn default() -> Infinito {
        Infinito::new()
    }
}

// Building blocks. Positions are of a top-left origin unless `centered`.

/// The lamps, worst first, as `ddi_ui::state::history::Lamp` names them.
const LAMPS: [&str; 21] = [
    "Failed",
    "Clear",
    "BreakFlag",
    "GreatFlag",
    "GloriousFullCombo",
    "GloriousFlag",
    "AmazingFullCombo",
    "AbsoluteDito",
    "Life4Clear",
    "FullCombo",
    "GreatFullCombo",
    "PerfectFullCombo",
    "SingleDigitPerfects",
    "MarvelousFullCombo",
    "SingleDigitGoods",
    "Omniflag",
    "Missflag",
    "Averageflag",
    "Blackflag",
    "Booflag",
    "Aaa",
];

/// Room on each side of the wheel's rows for the 3 px active outline,
/// its 3 px gap, and antialiasing inside the list clip.
pub const ROW_INSET: f32 = 8.0;

/// A row's actors moved [`ROW_INSET`] right, inside an unnamed frame
/// (the list places that one).
fn inset(mut a: Actor) -> Actor {
    call(&mut a, "x", &[ROW_INSET]);
    frame("", vec![a])
}

/// A quad with its top left at `at`.
fn plate(name: &str, at: [f32; 2], size: [f32; 2], color: Rgba) -> Actor {
    let mut q = quad(name, size, color);
    call(&mut q, "halign", &[0.0]);
    call(&mut q, "valign", &[0.0]);
    call(&mut q, "xy", &at);
    q
}

/// `a` with corners of radius `r` (our extension to sprites).
fn round(mut a: Actor, r: f32) -> Actor {
    call(&mut a, "cornerradius", &[r]);
    a
}

/// A rectangle's outline, `width` thick, inside `[x, y, w, h]`, its
/// corners following the plate it surrounds: a selection's ring (8) and
/// glow (11) around a plate of 6, a plate's own edge (6), a key cap (4),
/// a toggle's box (3).
fn outline(name: &str, rect: [f32; 4], width: f32, color: Rgba) -> Actor {
    let r = match name {
        "Line" | "Edge" | "Border" => 6.0,
        "Box" => 3.0,
        n if n.starts_with("Cap") => 4.0,
        _ => 0.0,
    };
    outline_r(name, rect, width, color, r)
}

/// An outline with corners of radius `r`: one quad, cut.
fn outline_r(name: &str, [x, y, w, h]: [f32; 4], width: f32, color: Rgba, r: f32) -> Actor {
    let mut q = round(plate(name, [x, y], [w, h], color), r);
    call(&mut q, "outline", &[width]);
    q
}

/// A dashed outline.
fn dashed(name: &str, [x, y, w, h]: [f32; 4], color: Rgba) -> Actor {
    let (dash, gap) = (6.0_f32, 4.0_f32);
    let mut c = Vec::new();
    let mut t = 0.0;
    while t < w {
        let len = dash.min(w - t);
        c.push(plate("", [x + t, y], [len, 1.0], color));
        c.push(plate("", [x + t, y + h - 1.0], [len, 1.0], color));
        t += dash + gap;
    }
    let mut t = 0.0;
    while t < h {
        let len = dash.min(h - t);
        c.push(plate("", [x, y + t], [1.0, len], color));
        c.push(plate("", [x + w - 1.0, y + t], [1.0, len], color));
        t += dash + gap;
    }
    frame(name, c)
}

fn at(mut a: Actor, x: f32, y: f32) -> Actor {
    call(&mut a, "xy", &[x, y]);
    a
}

/// A hidden quad as large as the component (`ddi_ui::screens::extent`).
fn extent(size: [f32; 2]) -> Actor {
    let mut e = quad("Extent", size, [0.0; 4]);
    e.visible = false;
    e
}

fn hide(a: &mut Actor) {
    call(a, "diffusealpha", &[0.0]);
}

/// No wider than `w`: squeezed a little, then cut with "…".
fn max_width(a: &mut Actor, w: f32) {
    ddi_ui::look::ellipsized(a, w.max(1.0));
}



/// The song's heart's height on song select's song side (wide, and a
/// phone's or a room's short side).
const FAVORITE: f32 = 24.0;
const FAVORITE_SMALL: f32 = 18.0;

/// Where song select puts the song's heart (`FavoriteSlot`): an empty
/// frame at `at`, its `Extent` the heart's height.
fn favorite_slot(height: f32, at: [f32; 2]) -> Actor {
    let mut slot = frame("FavoriteSlot", vec![extent([height, height])]);
    call(&mut slot, "xy", &at);
    slot
}

/// The wide wheel's row: its banner's left, its size at rest and on the
/// selected row, the title's size at rest and selected, and the room
/// the meters take at its right.
const WHEEL_BANNER_X: f32 = 12.0;
/// (The banners' sizes follow the rows' heights: [`Infinito::wheel_banner`].)
const WHEEL_TITLE: f32 = 17.0;
const WHEEL_TITLE_BIG: f32 = 21.0;
/// A song row's right end (`song_row`): the chart's level, its best's
/// grade (or "NEW") beside it, and their margins; the compact list's.
const WHEEL_RIGHT: f32 = ROW_LEVEL[0] + ROW_LEVEL_W + ROW_GAP + ROW_GRADE_W + 10.0;
const WHEEL_RIGHT_COMPACT: f32 = ROW_LEVEL_COMPACT[0] + ROW_LEVEL_COMPACT_W + ROW_GAP + 40.0 + 6.0;
/// The level's right margin and size; the room it is given (four
/// characters, "18.8").
const ROW_LEVEL: [f32; 2] = [14.0, 18.0];
const ROW_LEVEL_W: f32 = 40.0;
const ROW_LEVEL_COMPACT: [f32; 2] = [10.0, 15.0];
const ROW_LEVEL_COMPACT_W: f32 = 34.0;
/// Between the level and the grade's column; the grade's column.
const ROW_GAP: f32 = 8.0;
const ROW_GRADE_W: f32 = 44.0;

const DIFFICULTIES: [&str; 6] = ["Beginner", "Easy", "Medium", "Hard", "Challenge", "Edit"];
const JUDGEMENTS: [&str; 6] = ["W1", "W2", "W3", "W4", "W5", "Miss"];

impl Infinito {
    /// A plate and the selection's ring and glow around it (centered), lit
    /// on focus.
    fn button(&self, spec: &Spec, fill: &str, label: &str, size: f32, weight: u16) -> Actor {
        let [w, h] = spec.size;
        // Large buttons (44 and up) rounder.
        let r = if h >= 44.0 { 8.0 } else { 6.0 };
        let mut plate = round(quad("Plate", spec.size, self.c(fill)), r);
        if fill == "Plate2" {
            self.tinted(&mut plate, "GainFocus", "Highlight");
            self.tinted(&mut plate, "LoseFocus", "Plate2");
        }
        let mut text = self.text("Label", &spec.text, size, weight, label);
        if fill == "Accent" {
            text = tracked(text, 80);
        }
        max_width(&mut text, w - 12.0);
        let mut f = frame("", vec![plate, text, extent(spec.size)]);
        command_of(&mut f, "Press", "stoptweening;linear,0.05;zoom,0.97");
        command_of(&mut f, "Release", "stoptweening;linear,0.08;zoom,1");
        f
    }

    /// The wide wheel's banner at rest (144 × 45 in the shipped rows) and
    /// on the selected row (224 × 70): as tall as the rows leave room for,
    /// those at most, 3.2 wide to 1.
    fn wheel_banner(&self) -> [f32; 2] {
        let h = (self.m("Select.Wheel.Row") - self.m("Select.Wheel.Gap") - 15.0).min(45.0);
        [h * 3.2, h]
    }

    fn wheel_banner_big(&self) -> [f32; 2] {
        let h = (self.m("Select.Wheel.Selected") - 18.0).min(70.0);
        [h * 3.2, h]
    }

    /// A wheel row `h` high: whether it is the compact list's, its
    /// plate's height, and the selected row's.
    fn wheel_row(&self, h: f32) -> (bool, f32, f32) {
        let compact = h < 56.0;
        let gap = self.m("Select.Wheel.Gap");
        let row = if compact {
            h - 2.0
        } else {
            self.m("Select.Wheel.Row") - gap
        };
        let big = if compact {
            row
        } else {
            self.m("Select.Wheel.Selected")
        };
        (compact, row, big)
    }

    /// The song wheel's row (`h` ≥ 56) or the compact list's, drawn
    /// [`ROW_INSET`] in from the left so the selected row's active
    /// outline lies within the list (which clips at its edges).
    fn song_row(&self, spec: &Spec) -> Actor {
        let [w, h] = [spec.size[0] - 2.0 * ROW_INSET, spec.size[1]];
        let (compact, row, big) = self.wheel_row(h);
        let t = self.m("Motion.Select");
        let grow = big / row;
        // The selected row's shadow (0 8 30 black at a half).
        let mut shadow = round(
            plate(
                "Shadow",
                [-15.0, -15.0 + 8.0],
                [w + 30.0, big + 30.0],
                [0.0, 0.0, 0.0, 0.5],
            ),
            6.0,
        );
        call(&mut shadow, "softedge", &[15.0]);
        hide(&mut shadow);
        // Drawn around the row, never a target: with the selected row
        // drawn over the others, it took the top of the next row's taps.
        shadow.decoration = true;
        command_of(
            &mut shadow,
            "Select",
            &format!("stoptweening;decelerate,{t};diffusealpha,0.5"),
        );
        command_of(&mut shadow, "Deselect", "stoptweening;diffusealpha,0");
        let mut p = round(plate("Plate", [0.0, 0.0], [w, row], self.c("Row")), 6.0);
        command_of(
            &mut p,
            "Select",
            &format!(
                "stoptweening;decelerate,{t};zoomy,{grow};diffuse,{}",
                rgba(self.c("Raised"))
            ),
        );
        command_of(
            &mut p,
            "Deselect",
            &format!(
                "stoptweening;decelerate,{t};zoomy,1;diffuse,{}",
                rgba(self.c("Row"))
            ),
        );
        let (bw, bh, bx) = if compact {
            (96.0, 30.0, 6.0)
        } else {
            let [w, h] = self.wheel_banner();
            (w, h, WHEEL_BANNER_X)
        };
        let by = (row - bh) / 2.0;
        let mut back = round(
            plate("BannerBack", [bx, by], [bw, bh], self.c("BannerEmpty")),
            3.0,
        );
        let mut banner = named(Kind::Sprite(Sprite::quad([bw, bh])), "Banner");
        call(&mut banner, "cornerradius", &[3.0]);
        call(&mut banner, "halign", &[0.0]);
        call(&mut banner, "valign", &[0.0]);
        call(&mut banner, "xy", &[bx, by]);
        banner.visible = false;
        let tx = bx + bw + if compact { 10.0 } else { 14.0 };
        let (ts, ss) = if compact {
            (15.0, 12.0)
        } else {
            (WHEEL_TITLE, 13.0)
        };
        let mut title = at_left(self.text("Title", "", ts, 600, "Text"), tx, row * 0.36);
        let mut sub = at_left(self.text("Subtitle", "", ss, 400, "Muted"), tx, row * 0.72);
        // Room for the chart's level and best at the right.
        let cells = if compact {
            WHEEL_RIGHT_COMPACT
        } else {
            WHEEL_RIGHT
        };
        max_width(&mut title, w - tx - cells - 8.0);
        max_width(&mut sub, w - tx - cells - 8.0);
        // Greyed: a dim plate, grey art, muted titles.
        command_of(&mut back, "Unavailable", "diffusealpha,0.5");
        command_of(&mut back, "Available", "diffusealpha,1");
        // Greyed art: grey, and dimmer.
        command_of(
            &mut banner,
            "Unavailable",
            "saturation,0;diffuse,0.6,0.6,0.6,1",
        );
        command_of(&mut banner, "Available", "saturation,1;diffuse,1,1,1,1");
        self.colored(&mut title, "Unavailable", "Muted");
        self.colored(&mut title, "Available", "Text");
        self.colored(&mut sub, "Unavailable", "Off");
        self.colored(&mut sub, "Available", "Muted");
        // The chart a press of Play would play, at the right edge: its
        // level in its difficulty's color, and left of it the best's
        // grade with its lamp's mark under it, or "NEW" for a song never
        // played (`docs/plans/song-navigation.md`, "A song's row").
        let mut right = Vec::new();
        let cy = row / 2.0;
        let ([margin, size], level_w, grade_size) = if compact {
            (ROW_LEVEL_COMPACT, ROW_LEVEL_COMPACT_W, 14.0)
        } else {
            (ROW_LEVEL, ROW_LEVEL_W, 16.0)
        };
        let mut level = at_right(
            self.num("Level", "", size, "Difficulty.Medium"),
            w - margin,
            cy,
        );
        for d in DIFFICULTIES {
            let c = rgba(self.c(&format!("Difficulty.{d}")));
            command_of(&mut level, &format!("Level.{d}"), &format!("diffuse,{c}"));
        }
        max_width(&mut level, level_w);
        let gx = w - margin - level_w - ROW_GAP - if compact { 20.0 } else { ROW_GRADE_W / 2.0 };
        let mut grade = at(self.num("Grade", "", grade_size, "Text"), gx, cy);
        command_of(&mut grade, "Flagged", &format!("y,{}", cy - 4.0));
        command_of(&mut grade, "Unflagged", &format!("y,{cy}"));
        // The best's lamp, a short line under the grade (as the level
        // chips had it under theirs).
        let mut flag = round(quad("Flag", [18.0, 3.0], self.c("Lamp.Clear")), 1.5);
        call(
            &mut flag,
            "xy",
            &[gx, cy + if compact { 9.0 } else { 10.0 }],
        );
        self.lamp_commands(&mut flag);
        // "NEW": a quiet pill in the accent, the grade's place.
        let pill = if compact {
            [40.0, 16.0]
        } else {
            [ROW_GRADE_W, 18.0]
        };
        let mut tint = self.c("Accent");
        tint[3] = 0.14;
        let mut new_back = round(quad("NewBack", pill, tint), pill[1] / 2.0);
        call(&mut new_back, "xy", &[gx, cy]);
        let mut label = at(
            tracked(
                self.text(
                    "NewLabel",
                    "",
                    if compact { 9.0 } else { 10.0 },
                    700,
                    "Accent",
                ),
                80,
            ),
            gx,
            cy,
        );
        max_width(&mut label, pill[0] - 8.0);
        let mut new = frame("New", vec![new_back, label]);
        hide(&mut new);
        command_of(&mut new, "New", "diffusealpha,1");
        command_of(&mut new, "NotNew", "diffusealpha,0");
        let mut chosen = frame("Chosen", vec![level, grade, flag, new]);
        command_of(&mut chosen, "Unavailable", "diffusealpha,0");
        command_of(&mut chosen, "Available", "diffusealpha,1");
        right.push(chosen);
        let mut none = at_right(self.text("NoCharts", "", 12.0, 400, "Faint"), w - 14.0, cy);
        hide(&mut none);
        command_of(&mut none, "Unavailable", "diffusealpha,1");
        command_of(&mut none, "Available", "diffusealpha,0");
        right.push(none);
        let right = frame("Right", right);
        // A favorite's heart over the banner's upper left corner, small,
        // on a darker heart so it reads on any art; none on the others.
        let mut fav = self.heart_badge(if compact { 10.0 } else { 12.0 });
        call(&mut fav, "xy", &[bx + 9.0, by + 8.0]);
        command_of(&mut fav, "Favorite", "diffusealpha,1");
        command_of(&mut fav, "NotFavorite", "diffusealpha,0");
        inset(frame(
            "",
            vec![shadow, p, back, banner, fav, title, sub, right],
        ))
    }

    /// A small filled heart `h` tall on a darker one a little larger
    /// (named `Fav`, hidden): a favorite's mark on a row.
    fn heart_badge(&self, h: f32) -> Actor {
        let mut ground = self.c("Ground");
        ground[3] = 0.85;
        let back = ddi_ui::heart::actor(true, h + 4.0, [ground, ground]);
        let heart = ddi_ui::heart::actor(true, h, [self.c("Heart"), self.c("Heart.Empty")]);
        let mut fav = frame("Fav", vec![back, heart]);
        fav.decoration = true;
        hide(&mut fav);
        fav
    }

    fn song_header(&self, spec: &Spec) -> Actor {
        let [w, h] = [spec.size[0] - 2.0 * ROW_INSET, spec.size[1]];
        let (compact, row, big) = self.wheel_row(h);
        let t = self.m("Motion.Select");
        let mut line = outline("Line", [0.0, 0.0, w, row], 1.0, self.c("HeaderLine"));
        command_of(&mut line, "Select", "diffusealpha,0");
        command_of(&mut line, "Deselect", "diffusealpha,1");
        let mut p = round(
            plate("Plate", [0.0, 0.0], [w, row], self.c("HeaderRow")),
            6.0,
        );
        command_of(
            &mut p,
            "Select",
            &format!(
                "stoptweening;decelerate,{t};zoomy,{};diffuse,{}",
                big / row,
                rgba(self.c("Raised"))
            ),
        );
        command_of(
            &mut p,
            "Deselect",
            &format!(
                "stoptweening;decelerate,{t};zoomy,1;diffuse,{}",
                rgba(self.c("HeaderRow"))
            ),
        );
        let (cs, ts, cn) = if compact {
            (12.0, 15.0, 12.0)
        } else {
            (13.0, 17.0, 13.0)
        };
        let chevron = at_left(
            self.text("Chevron", "▾", cs, 400, "Accent"),
            16.0,
            row / 2.0,
        );
        let mut title = at_left(self.text("Title", "", ts, 600, "Text"), 38.0, row / 2.0);
        max_width(&mut title, w - 38.0 - 120.0);
        command_of(
            &mut title,
            "WithCover",
            &format!("x,126;maxwidth,{}", w - 126.0 - 120.0),
        );
        command_of(
            &mut title,
            "WithoutCover",
            &format!("x,38;maxwidth,{}", w - 38.0 - 120.0),
        );
        let count = at_right(
            self.text("Count", "", cn, 400, "Muted"),
            w - 18.0,
            row / 2.0,
        );
        let mut cover = named(
            Kind::Sprite(Sprite::quad([80.0, (row - 12.0).max(12.0)])),
            "Cover",
        );
        call(&mut cover, "xy", &[78.0, row / 2.0]);
        call(&mut cover, "cornerradius", &[3.0]);
        cover.visible = false;
        // The favorites' pack: its heart in the cover's place.
        command_of(
            &mut title,
            "Favorites",
            &format!("x,64;maxwidth,{}", w - 64.0 - 120.0),
        );
        let mut heart = ddi_ui::heart::actor(
            true,
            if compact { 14.0 } else { 16.0 },
            [self.c("Heart"), self.c("Heart.Empty")],
        );
        heart.name = "Heart".into();
        call(&mut heart, "xy", &[47.0, row / 2.0]);
        hide(&mut heart);
        command_of(&mut heart, "WithCover", "diffusealpha,0");
        command_of(&mut heart, "WithoutCover", "diffusealpha,0");
        command_of(&mut heart, "Favorites", "diffusealpha,1");
        let middle = frame("Middle", vec![chevron, cover, heart, title]);
        let right = frame("Right", vec![count]);
        inset(frame("", vec![p, line, middle, right]))
    }

    fn style_tab(&self, spec: &Spec) -> Actor {
        let h = spec.size[1];
        let count = spec.items.first().cloned().unwrap_or_default();
        let lw = mono_width(&spec.text, 13.0);
        let cw = if count.is_empty() {
            0.0
        } else {
            mono_width(&count, 11.0) + 6.0
        };
        let w = lw + cw + 20.0;
        let plate_ = round(quad("Plate", [w, h], self.c("Plate2")), 6.0);
        let border = outline("Border", [-w / 2.0, -h / 2.0, w, h], 1.0, self.c("Line2"));
        let dash = dashed("Dashed", [-w / 2.0, -h / 2.0, w, h], self.c("HeaderLine"));
        let label = at_left(
            self.text("Label", &spec.text, 13.0, 600, "Text"),
            -w / 2.0 + 10.0,
            0.0,
        );
        let mut num = at_left(
            self.num("Count", &count, 11.0, "Text"),
            -w / 2.0 + 10.0 + lw + 6.0,
            0.0,
        );
        call(&mut num, "diffusealpha", &[0.75]);
        let mut f = frame("", vec![plate_, border, dash, label, num, extent([w, h])]);
        for (state, plate_c, text_c, border_a, dash_a) in [
            ("Selected", self.c("Accent"), self.c("OnAccent"), 0.0, 0.0),
            ("Available", self.c("Plate2"), self.c("Text"), 1.0, 0.0),
            ("Unavailable", [0.0; 4], self.c("Off"), 0.0, 1.0),
        ] {
            for part in f.children_mut() {
                let cmd = match part.name.as_str() {
                    "Plate" => format!("diffuse,{}", rgba(plate_c)),
                    "Label" => format!("diffuse,{}", rgba(text_c)),
                    "Count" => {
                        format!("diffuse,{};diffusealpha,{}", rgba(text_c), 0.75 * text_c[3])
                    }
                    "Border" => format!("diffusealpha,{border_a}"),
                    "Dashed" => format!("diffusealpha,{dash_a}"),
                    _ => continue,
                };
                command_of(part, state, &cmd);
            }
        }
        f
    }

    fn lamp_commands(&self, a: &mut Actor) {
        hide(a);
        self.lamp_commands_text(a);
    }

    /// The same for a lamp's text (shown in its color).
    fn lamp_commands_text(&self, a: &mut Actor) {
        command_of(a, "Lamp.None", "diffusealpha,0");
        for name in LAMPS {
            let c = rgba(self.c(&format!("Lamp.{name}")));
            command_of(a, &format!("Lamp.{name}"), &format!("diffuse,{c}"));
        }
    }

    /// A command for each difficulty, by its name, coloring `parts` as
    /// that difficulty.
    fn difficulty_commands(&self, parts: &mut [&mut Actor]) {
        for d in DIFFICULTIES {
            let diffuse = format!("diffuse,{}", rgba(self.c(&format!("Difficulty.{d}"))));
            for a in parts.iter_mut() {
                command_of(a, d, &diffuse);
            }
        }
    }

    fn ladder_empty(&self, spec: &Spec) -> Actor {
        let [w, h] = spec.size;
        let border = dashed("Border", [0.0, 0.0, w, h], self.c("Line2"));
        let mut title = at_left(
            self.text("Title", &spec.text, 15.0, 600, "Text"),
            20.0,
            h / 2.0 - 12.0,
        );
        max_width(&mut title, w - 40.0);
        let mut hint = at_left(
            self.text("Hint", "", 15.0, 400, "Muted"),
            20.0,
            h / 2.0 + 12.0,
        );
        max_width(&mut hint, w - 40.0);
        frame("", vec![border, title, hint])
    }

    /// A tab (the results' panel sets): its label, muted until selected,
    /// then bright over an accent underline.
    fn tab(&self, spec: &Spec) -> Actor {
        let [w, h] = spec.size;
        let plate_ = quad("Plate", [w, h], [0.0; 4]);
        let mut label = self.text("Label", &spec.text, 13.0, 600, "Muted");
        self.colored(&mut label, "Selected", "Text");
        self.colored(&mut label, "Available", "Muted");
        let mut line = round(
            plate(
                "Underline",
                [-w / 2.0 + 6.0, h / 2.0 - 2.0],
                [w - 12.0, 2.0],
                self.c("Accent"),
            ),
            1.0,
        );
        hide(&mut line);
        command_of(&mut line, "Selected", "diffusealpha,1");
        command_of(&mut line, "Available", "diffusealpha,0");
        frame("", vec![plate_, label, line, extent([w, h])])
    }

    fn chart_button(&self, spec: &Spec) -> Actor {
        let [w, h] = spec.size;
        let mut p = round(quad("Plate", spec.size, self.c("Plate2")), 6.0);
        self.colored(&mut p, "Select", "Highlight");
        self.colored(&mut p, "Deselect", "Plate2");
        let mut top = round(
            plate(
                "Bar",
                [-w / 2.0 + 2.0, -h / 2.0],
                [w - 4.0, 3.0],
                self.c("Difficulty.Edit"),
            ),
            1.5,
        );
        let mut meter = self.num("Meter", &spec.text, 20.0, "Text");
        self.difficulty_commands(&mut [&mut top, &mut meter]);
        let mut lamp = round(
            plate(
                "Lamp",
                [-w / 2.0 + 8.0, h / 2.0 - 6.0],
                [w - 16.0, 3.0],
                self.c("Lamp.Clear"),
            ),
            1.5,
        );
        self.lamp_commands(&mut lamp);
        // A generated version's mark (assisted or converted): a small dot
        // in the corner.
        let mut assist = round(
            plate(
                "Assist",
                [w / 2.0 - 9.0, -h / 2.0 + 6.0],
                [4.0, 4.0],
                self.c("Muted"),
            ),
            2.0,
        );
        hide(&mut assist);
        command_of(&mut assist, "Assisted", "diffusealpha,1");
        command_of(&mut assist, "Converted", "diffusealpha,1");
        command_of(&mut assist, "Original", "diffusealpha,0");
        frame("", vec![p, top, meter, lamp, assist, extent(spec.size)])
    }

    /// The screens' header, one row: the Back button, the wordmark
    /// filling the height (live, as the start menu's title; the screen
    /// binds its image and ticks it, `ddi_ui::screens::header`, `Live`),
    /// a divider, the screen's name small. `Header.TitleEnd` says where
    /// the row ends for what a screen puts after it.
    fn header(&self, spec: &Spec) -> Actor {
        let [w, h] = spec.size;
        let m = self.m("Margin");
        let p = plate("Plate", [0.0, 0.0], [w, h], self.c("Header"));
        let line = plate("Line", [0.0, h - 1.0], [w, 1.0], self.c("Line"));
        let back = at(self.back_button([44.0, 44.0]), m + 22.0, h / 2.0);
        let mark_h = h - 20.0;
        let mark_w = mark_h * wordmark::ASPECT;
        let mut mark = wordmark::actor([mark_w, mark_h], self.c("Text"));
        mark.name = "Wordmark".into();
        let x = m + 56.0;
        let mark = at(mark, x + mark_w / 2.0, h / 2.0);
        let x = x + mark_w + 16.0;
        let divider = plate(
            "Divider",
            [x, h / 2.0 - 12.0],
            [1.0, 24.0],
            self.c("HeaderLine"),
        );
        let title = at_left(
            tracked(self.text("Title", &spec.text, 11.0, 600, "Muted"), 260),
            x + 17.0,
            h / 2.0,
        );
        frame("", vec![p, line, back, mark, divider, title])
    }

    /// The ring `Ui::mark` draws around what it marks (the announcer
    /// studio's scenes): a red pill's outline, softly lit around it.
    fn mark(&self, spec: &Spec) -> Actor {
        let [w, h] = spec.size;
        let r = w.min(h) / 2.0;
        let c = self.c("Mark");
        let mut glow = round(
            quad("Glow", [w + 8.0, h + 8.0], [c[0], c[1], c[2], 0.35]),
            r + 4.0,
        );
        call(&mut glow, "outline", &[8.0]);
        call(&mut glow, "softedge", &[4.0]);
        let mut ring = round(quad("Ring", [w, h], c), r);
        call(&mut ring, "outline", &[3.0]);
        frame("", vec![glow, ring])
    }

    /// A Back button: three bars on a plain plate, named `Back`.
    fn back_button(&self, size: [f32; 2]) -> Actor {
        let mut b = self.button(&Spec::new(size, ""), "Plate2", "Text", 20.0, 700);
        if let Kind::Frame(c) = &mut b.kind {
            let n = c.len().saturating_sub(1);
            for (i, y) in [-6.0, 0.0, 6.0].into_iter().enumerate() {
                let bar = round(quad(&format!("Bar{i}"), [18.0, 2.0], self.c("Text")), 1.0);
                c.insert(n + i, at(bar, 0.0, y));
            }
        }
        b.name = "Back".into();
        b
    }

    /// A button showing an icon alone: a sprite `Icon` on a plate, its
    /// image (a cell of an atlas: the quick volume's speaker,
    /// `ui/volume.svg`; a page's, `ui/icons.svg`) the widget's to give.
    fn icon_button(&self, size: [f32; 2]) -> Actor {
        let mut b = self.button(&Spec::new(size, ""), "Plate2", "Text", 20.0, 700);
        if let Kind::Frame(c) = &mut b.kind {
            c.retain(|a| a.name != "Label");
            let side = size[1] * 0.6;
            let mut s = Sprite::new(0, vec![], [side, side]);
            s.shape.hide_unloaded = true;
            let mut icon = named(Kind::Sprite(s), "Icon");
            call(&mut icon, "diffuse", &self.c("Text"));
            let n = c.len().saturating_sub(1);
            c.insert(n, icon);
        }
        b
    }

    /// The quick volume's slider: the level over an upright track.
    fn volume_popup(&self, [w, h]: [f32; 2]) -> Actor {
        let plate = round(quad("Plate", [w, h], self.c("Raised")), 8.0);
        let edge = outline_r(
            "Edge",
            [-w / 2.0, -h / 2.0, w, h],
            1.0,
            self.c("Line2"),
            8.0,
        );
        let value = at(
            self.num("Value", "", 13.0, "Secondary"),
            0.0,
            -h / 2.0 + 18.0,
        );
        let track_h = h - 56.0;
        let bottom = h / 2.0 - 14.0;
        let track = at(
            round(quad("Track", [4.0, track_h], self.c("Line2")), 2.0),
            0.0,
            bottom - track_h / 2.0,
        );
        let mut fill = at(
            round(quad("Fill", [4.0, track_h], self.c("Accent")), 2.0),
            0.0,
            bottom,
        );
        call(&mut fill, "valign", &[1.0]);
        let knob = at(
            round(quad("Knob", [20.0, 8.0], self.c("Text")), 4.0),
            0.0,
            bottom,
        );
        frame("", vec![plate, edge, value, track, fill, knob])
    }

    /// The search field's Clear: a quiet × inside the field's right end.
    fn clear_button(&self, size: [f32; 2]) -> Actor {
        let mut b = self.button(&Spec::new(size, ""), "Plate2", "Muted", 20.0, 700);
        if let Kind::Frame(c) = &mut b.kind {
            c.retain(|a| a.name != "Label");
            // No plate of its own on the field (lit or not), but focused.
            if let Some(p) = c.iter_mut().find(|a| a.name == "Plate") {
                call(p, "diffuse", &[0.0; 4]);
                self.tinted(p, "GainFocus", "Line2");
                command_of(
                    p,
                    "LoseFocus",
                    &format!(
                        "stoptweening;decelerate,{};diffuse,0,0,0,0",
                        self.m("Motion.Select")
                    ),
                );
            }
            let n = c.len().saturating_sub(1);
            for (i, angle) in [45.0, -45.0].into_iter().enumerate() {
                let mut bar = round(
                    quad(&format!("Bar{i}"), [size[1] * 0.5, 2.0], self.c("Muted")),
                    1.0,
                );
                call(&mut bar, "rotationz", &[angle]);
                c.insert(n + i, bar);
            }
        }
        b
    }

    fn panel(&self, spec: &Spec) -> Actor {
        let [w, h] = spec.size;
        let p = round(plate("Plate", [0.0, 0.0], [w, h], self.c("Panel")), 8.0);
        let edge = outline_r("Edge", [0.0, 0.0, w, h], 1.0, self.c("Line"), 8.0);
        let title = at_left(
            self.text("Title", &spec.text, 13.0, 600, "Secondary"),
            14.0,
            20.0,
        );
        let mut note = at_right(self.text("Note", "", 12.0, 400, "Muted"), w - 14.0, 20.0);
        max_width(&mut note, w * 0.6);
        frame("", vec![p, edge, title, note])
    }

    fn judgement(&self, spec: &Spec) -> Actor {
        let [w, h] = spec.size;
        let name = at_left(
            self.text("Name", &spec.text, 15.0, 600, "Text"),
            0.0,
            h / 2.0,
        );
        let mut bar = round(
            plate("Bar", [150.0, h / 2.0 - 4.0], [170.0, 8.0], self.c("Text")),
            2.0,
        );
        call(&mut bar, "diffusealpha", &[0.85]);
        let count = at_right(self.num("Count", "", 15.0, "Text"), w, h / 2.0);
        let mut name = name;
        // StepMania's set's tiers, then DDI's (`Judgement.Ddi.W0` …), and
        // the Boos' row under DDI (`Judgement.Boo`).
        let ddi = ddi_engine::rules::colors::DDI.named("Judgement.Ddi.");
        let ddi = ddi.iter().map(|(k, _)| k.clone()).filter(|k| !k.ends_with("Boo"));
        let boo = std::iter::once("Judgement.Boo".to_string());
        for key in JUDGEMENTS.iter().map(|j| format!("Judgement.{j}")).chain(ddi).chain(boo) {
            let c = rgba(self.c(&key));
            command_of(&mut name, &key, &format!("diffuse,{c}"));
            command_of(&mut bar, &key, &format!("diffuse,{c};diffusealpha,0.85"));
        }
        frame("", vec![name, bar, count])
    }

    fn score(&self, spec: &Spec) -> Actor {
        let [w, h] = spec.size;
        let grade = at_left(self.text("Grade", "", 64.0, 700, "Gold"), 0.0, h / 2.0);
        let score = at_left(self.num("Score", "", 56.0, "Text"), 76.0, h / 2.0);
        // The numbers face is wider than half an em (0.6).
        let pct = at_left(
            self.num("Percent", "%", 30.0, "Text"),
            76.0 + 5.0 * 0.6 * 56.0 + 4.0,
            h / 2.0 + 8.0,
        );
        let ex = at_right(self.num("Ex", "", 14.0, "Muted"), w, h / 2.0 + 18.0);
        frame("", vec![grade, score, pct, ex])
    }

    fn section(&self, spec: &Spec) -> Actor {
        let [w, h] = spec.size;
        let mut p = round(plate("Plate", [0.0, 0.0], [w, h], [0.0; 4]), 6.0);
        self.colored(&mut p, "Select", "Selected");
        command_of(&mut p, "Deselect", "diffuse,0,0,0,0");
        let mut mark = round(
            plate("Mark", [0.0, 6.0], [3.0, h - 12.0], self.c("Accent")),
            1.5,
        );
        hide(&mut mark);
        command_of(&mut mark, "Select", "diffusealpha,1");
        command_of(&mut mark, "Deselect", "diffusealpha,0");
        let mut title = at_left(
            self.text("Title", &spec.text, 16.0, 600, "Muted"),
            16.0,
            h / 2.0,
        );
        self.colored(&mut title, "Select", "Text");
        self.colored(&mut title, "Deselect", "Muted");
        frame("", vec![p, mark, title])
    }

    fn option_row(&self, spec: &Spec, centered: bool) -> Actor {
        let [w, h] = spec.size;
        let (x0, y0) = if centered {
            (-w / 2.0, -h / 2.0)
        } else {
            (0.0, 0.0)
        };
        // The toggle is focused; the options screen's rows are told which
        // is (their frame is what takes focus).
        let (on, off) = if centered {
            ("GainFocus", "LoseFocus")
        } else {
            ("Select", "Deselect")
        };
        let mut p = round(plate("Plate", [x0, y0], [w, h], [0.0; 4]), 6.0);
        self.tinted(&mut p, on, "Selected");
        command_of(&mut p, off, "stoptweening;linear,0.08;diffuse,0,0,0,0");
        let mut label = at_left(
            self.text("Label", &spec.text, 15.0, 600, "Secondary"),
            x0 + 16.0,
            y0 + h / 2.0,
        );
        max_width(&mut label, if centered { 186.0 } else { 216.0 });
        self.colored(&mut label, on, "Text");
        self.colored(&mut label, off, "Secondary");
        frame("", vec![p, label])
    }
}

impl Look for Infinito {
    fn active_element(&self, root: &mut Actor, target: Option<&str>, seconds: f64) -> bool {
        active::update(root, target, seconds, self)
    }

    fn pointer(&self, spec: &ddi_ui::cursor::Spec) -> Option<Actor> {
        cursor::actor(spec)
    }

    fn image(&self, name: &str) -> Option<&str> {
        match name {
            "TitleMenu.Title" | "Screen.Header" => Some("title/dosis.png"),
            "PointerMask" => Some("ui/cursors.svg"),
            "PointerBurst" => Some("ui/cursor-burst.svg"),
            "Volume.Icon" => Some("ui/volume.svg"),
            "Page.Icons" => Some("ui/icons.svg"),
            "TitleMenu.Background" => Some("ui/menu-rain.svg"),
            "KeyHint" => Some(self.keycap_material.get().image()),
            _ => None,
        }
    }

    fn place_measured(&self, root: &mut Actor, measure: &mut dyn ddi_actor::text::TextMeasure) {
        song_cover::place_heart(root, measure);
    }

    fn animate_component(
        &self,
        name: &str,
        actor: &mut Actor,
        size: [f32; 2],
        seconds: f64,
    ) -> bool {
        if name != "TitleMenu.Background" {
            return false;
        }
        rain::update(actor, size, seconds, self);
        true
    }

    fn component(&self, name: &str, spec: &Spec) -> Option<Actor> {
        let [w, h] = spec.size;
        Some(match name {
            "TitleMenu.Background" => rain::actor(),
            "TitleMenu.Title" => wordmark::actor(spec.size, self.c("Text")),
            "TitleMenu.Primary" => self.button(spec, "Plate2", "Text", 20.0, 700),
            "TitleMenu.Choice" => self.button(spec, "Plate2", "Text", 18.0, 600),
            "TitleMenu.Disabled" => {
                let mut button = self.button(spec, "Plate2", "Muted", 18.0, 600);
                command_of(&mut button, "Press", "stoptweening;zoom,1");
                command_of(&mut button, "Release", "stoptweening;zoom,1");
                button
            }
            // A CPU's level (the spec's text, 1 to 10) as five stars, as
            // tall as the spec.
            "Stars" => ddi_ui::stars::actor(
                spec.text.parse().unwrap_or(0),
                h,
                [
                    self.c("Star.Gold"),
                    self.c("Star.Silver"),
                    self.c("Star.Empty"),
                ],
            ),
            // A song's heart on song select (the spec's text `on`: a
            // favorite, filled), as tall as the spec.
            "Heart" => ddi_ui::heart::actor(
                spec.text == "on",
                h,
                [self.c("Heart"), self.c("Heart.Empty")],
            ),
            "Button" => self.button(spec, "Plate2", "Text", 15.0, 600),
            "Button.Disabled" => {
                let mut button = self.button(spec, "Plate2", "Muted", 15.0, 600);
                command_of(&mut button, "Press", "stoptweening;zoom,1");
                command_of(&mut button, "Release", "stoptweening;zoom,1");
                button
            }
            "Prompt.Start" => {
                let mut color = self.c("Accent");
                color[3] = 0.32;
                ddi_ui::look::countdown_button(
                    self.button(spec, "Plate2", "Text", 18.0, 700),
                    spec.size,
                    color,
                    if h >= 44.0 { 8.0 } else { 6.0 },
                )
            }
            // A room's stage's Back (feedback round O): a plain button whose
            // background fills as it is held to leave while ready.
            "Prompt.Back" => {
                let mut color = self.c("Accent");
                color[3] = 0.32;
                ddi_ui::look::countdown_button(
                    self.button(spec, "Plate2", "Text", 15.0, 600),
                    spec.size,
                    color,
                    if h >= 44.0 { 8.0 } else { 6.0 },
                )
            }
            "PlayButton" => self.button(spec, "Accent", "OnAccent", 18.0, 700),
            "IconButton" => self.button(spec, "Plate2", "Text", 20.0, 700),
            "BackButton" => self.back_button(spec.size),
            "Volume" | "Page.Icon" => self.icon_button(spec.size),
            "Volume.Popup" => self.volume_popup(spec.size),
            "Search.Clear" => self.clear_button(spec.size),
            "StyleButton" => {
                let mut b = self.button(spec, "Plate2", "Text", 13.0, 600);
                if let Kind::Frame(c) = &mut b.kind {
                    c.insert(
                        2,
                        outline("Border", [-w / 2.0, -h / 2.0, w, h], 1.0, self.c("Line2")),
                    );
                }
                b
            }
            "Link" => {
                let label = self.text("Label", &spec.text, 13.0, 400, "Text");
                let mut label = label;
                self.colored(&mut label, "GainFocus", "Accent");
                self.colored(&mut label, "LoseFocus", "Text");
                let lw = mono_width(&spec.text, 13.0) + 8.0;
                frame(
                    "",
                    vec![quad("Plate", [lw, h], [0.0; 4]), label, extent([lw, h])],
                )
            }
            "Pill" => {
                let numbers = spec.text.starts_with(['×', '+', '-'])
                    || spec.text.starts_with(|c: char| c.is_ascii_digit());
                let size = if h > 28.0 { 13.0 } else { 14.0 };
                let lw = mono_width(&spec.text, size) + 20.0;
                // The results' panel pills are round; the header's mods
                // chips are not.
                let r = if h > 28.0 { h / 2.0 } else { 4.0 };
                let mut p = round(quad("Plate", [lw, h], self.c("Plate2")), r);
                self.colored(&mut p, "Selected", "Accent");
                self.colored(&mut p, "Available", "Plate2");
                let mut label = if numbers {
                    self.num("Label", &spec.text, size, "Text")
                } else {
                    self.text(
                        "Label",
                        &spec.text,
                        size,
                        if h > 28.0 { 600 } else { 400 },
                        "Text",
                    )
                };
                self.colored(&mut label, "Selected", "OnAccent");
                self.colored(&mut label, "Available", "Text");
                frame("", vec![p, label, extent([lw, h])])
            }
            "Tab" => self.tab(spec),
            "KeyHint" => self.key_hint(spec),
            "Toggle" => {
                let mut f = self.option_row(spec, true);
                let s = 18.0;
                let x = w / 2.0 - 16.0 - s / 2.0;
                let bx = outline("Box", [x - s / 2.0, -s / 2.0, s, s], 2.0, self.c("Line2"));
                let mut mark = round(quad("Mark", [s - 8.0, s - 8.0], self.c("Accent")), 2.0);
                call(&mut mark, "x", &[x]);
                command_of(
                    &mut mark,
                    "On",
                    "stoptweening;linear,0.08;zoom,1;diffusealpha,1",
                );
                command_of(
                    &mut mark,
                    "Off",
                    "stoptweening;linear,0.08;zoom,0.3;diffusealpha,0",
                );
                if let Kind::Frame(c) = &mut f.kind {
                    // The whole row takes a tap, not only its ends: a hit
                    // area must be drawn, however faintly.
                    let hit = quad("Hit", spec.size, [0.0, 0.0, 0.0, 0.004]);
                    c.insert(0, hit);
                    c.extend([bx, mark]);
                }
                f
            }
            "OptionRow.Cursor" => quad("", [w, 2.0], self.c("Accent")),
            "SongList.Row" => self.song_row(spec),
            "SongList.Header" => self.song_header(spec),
            "List.Row" => {
                // A generic row: the song row without its meters.
                let mut r = self.song_row(&Spec::new([w, h.min(55.0)], ""));
                if let Kind::Frame(c) = &mut r.kind {
                    c.retain(|a| a.name != "Right");
                }
                r
            }
            "List.Header" => self.song_header(&Spec::new([w, h.min(55.0)], "")),
            // The import report's lines: text alone, no banner box.
            "Report.Row" => {
                let mut t = at_left(
                    self.text("Title", "", 13.0, 400, "Secondary"),
                    12.0,
                    h / 2.0,
                );
                max_width(&mut t, w - 24.0);
                frame("", vec![t, at(extent(spec.size), w / 2.0, h / 2.0)])
            }
            "Report.Header" => {
                let t = at_left(
                    tracked(self.text("Title", "", 12.0, 700, "Muted"), 100),
                    4.0,
                    h / 2.0 + 4.0,
                );
                frame("", vec![t, at(extent(spec.size), w / 2.0, h / 2.0)])
            }
            "List.Bar" | "SongList.Bar" => {
                let mut track = round(quad("Track", spec.size, self.c("Track")), w / 2.0);
                call(&mut track, "valign", &[0.0]);
                let mut thumb = round(quad("Thumb", [w, 40.0], self.c("Thumb")), w / 2.0);
                call(&mut thumb, "valign", &[0.0]);
                frame("", vec![track, thumb])
            }
            "TextField" => {
                let mut bx = round(quad("Box", spec.size, self.c("Plate2")), 8.0);
                self.tinted(&mut bx, "GainFocus", "Highlight");
                self.tinted(&mut bx, "LoseFocus", "Plate2");
                let hint = at_left(
                    self.text("Hint", &spec.text, 15.0, 400, "Faint"),
                    -w / 2.0 + 12.0,
                    0.0,
                );
                frame("", vec![bx, hint])
            }
            "Popup.Item" => {
                let mut p = round(quad("Plate", spec.size, self.c("Plate2")), 6.0);
                command_of(
                    &mut p,
                    "Select",
                    &format!("stoptweening;diffuse,{}", rgba(self.c("Highlight"))),
                );
                command_of(
                    &mut p,
                    "Deselect",
                    &format!("stoptweening;diffuse,{}", rgba(self.c("Plate2"))),
                );
                let mut t = at_left(
                    self.text("Text", &spec.text, 17.0, 400, "Text"),
                    -w / 2.0 + 14.0,
                    0.0,
                );
                max_width(&mut t, w - 110.0);
                let note = at_right(
                    self.text("Note", "", 11.0, 600, "Faint"),
                    w / 2.0 - 14.0,
                    0.0,
                );
                let edge = outline(
                    "Edge",
                    [-w / 2.0, -h / 2.0 - 1.0, w, h + 2.0],
                    1.0,
                    self.c("Line2"),
                );
                frame("", vec![p, edge, t, note])
            }
            "Panel" => self.panel(spec),
            "Dialog.Glass" => {
                let [w, h] = spec.size;
                let p = round(plate("Plate", [0.0, 0.0], [w, h], self.c("Panel.Glass")), 12.0);
                let edge = outline_r("Edge", [0.0, 0.0, w, h], 1.0, self.c("Line"), 12.0);
                frame("", vec![p, edge])
            }
            "Screen.Header" => self.header(spec),
            "Mark" => self.mark(spec),
            "Screen.Footer" => {
                let p = plate("Plate", [0.0, 0.0], [w, h], self.c("Footer"));
                let line = plate("Line", [0.0, 0.0], [w, 1.0], self.c("Line"));
                frame("", vec![p, line])
            }
            "SongCover" => self.song_cover(spec),
            "SongInfo.Text" => self.song_text(spec),
            "SongInfo.Short" => self.song_short(spec),
            "StyleTab" => self.style_tab(spec),
            "ChartLadder.Empty" => self.ladder_empty(spec),
            "ChartButton" => self.chart_button(spec),
            "Results.Judgement" => self.judgement(spec),
            "Results.Score" => self.score(spec),
            "Options.Section" => self.section(spec),
            "Options.Row" => self.option_row(spec, false),
            "Options.Description" => {
                let p = round(plate("Plate", [0.0, 0.0], [w, h], self.c("Plate")), 8.0);
                let edge = outline_r("Edge", [0.0, 0.0, w, h], 1.0, self.c("Line"), 8.0);
                let title = at_left(
                    self.text("Title", "", 15.0, 700, "Text"),
                    20.0,
                    h / 2.0 - 10.0,
                );
                let mut note = at_left(
                    self.text("Note", "", 14.0, 400, "Muted"),
                    20.0,
                    h / 2.0 + 11.0,
                );
                max_width(&mut note, w - 40.0);
                frame("", vec![p, edge, title, note])
            }
            "Options.Preview" => {
                let p = round(plate("Plate", [0.0, 0.0], [w, h], self.c("Preview")), 10.0);
                let edge = outline_r("Edge", [0.0, 0.0, w, h], 1.0, self.c("Line"), 10.0);
                // A stand-in for the field: four targets near the top
                // (reverse puts them at the bottom in play).
                let mut c = vec![p, edge];
                let side = 44.0_f32.min((h - 40.0).max(16.0));
                let left = w / 2.0 - 2.0 * side - 12.0;
                for k in 0..4 {
                    c.push(outline_r(
                        "",
                        [
                            left + k as f32 * (side + 8.0),
                            (h - 120.0).max(4.0),
                            side,
                            side,
                        ],
                        2.0,
                        self.c("Line2"),
                        6.0,
                    ));
                }
                let mut title = at(
                    self.text("Title", &spec.text, 13.0, 400, "Muted"),
                    w / 2.0,
                    h - 24.0,
                );
                max_width(&mut title, w - 24.0);
                c.push(title);
                frame("", c)
            }
            _ => return None,
        })
    }

    fn color(&self, name: &str) -> Option<Rgba> {
        self.note("color", name);
        self.colors.get(name).copied()
    }

    fn font(&self) -> Option<Arc<str>> {
        Some(self.text_font())
    }

    fn font_for(&self, role: &str) -> Option<Arc<str>> {
        match role {
            "Numbers" => Some(self.numbers_font.clone()),
            _ => Some(self.text_font()),
        }
    }

    fn metric(&self, name: &str) -> Option<f32> {
        self.note("metric", name);
        if name == "Numbers.Weight" {
            return Some(f32::from(self.numbers_weight));
        }
        self.metrics.get(name).copied()
    }

    /// The wheel of layout A: the selected row taller on its line, the
    /// others `Wheel.Drift`·d² to the right and fading by `Wheel.Fade` a
    /// row; rows past the selected one moved down by the room it takes.
    fn set_wheel_open(&self, _list: &str, open: f32) {
        self.wheel_open.set(open.clamp(0.0, 1.0));
    }

    fn wheel_open(&self, _list: &str) -> f32 {
        self.wheel_open.get()
    }

    fn wheel_focus(&self, list: &str, actor: &mut Actor, width: f32, weight: f32) -> bool {
        if list != "Wheel" {
            return false;
        }
        let weight = weight * self.wheel_open.get();
        let row = self.m("Select.Wheel.Row") - self.m("Select.Wheel.Gap");
        let big = self.m("Select.Wheel.Selected");
        let height = row + (big - row) * weight;
        let mix = |a: f32, b: f32| a + (b - a) * weight;
        // Each part's tweens stopped once, then its commands in order.
        let set = |actor: &mut Actor, name: &str, cmds: &[(&str, &[f32])]| {
            if let Some(p) = ddi_ui::hit::find_mut(actor, name) {
                call(p, "stoptweening", &[]);
                for (cmd, values) in cmds {
                    call(p, cmd, values);
                }
            }
        };
        let header = ddi_ui::hit::find_mut(actor, "Chevron").is_some();
        let (wheel_banner, wheel_banner_big) = (self.wheel_banner(), self.wheel_banner_big());
        let a = self.c(if header { "HeaderRow" } else { "Row" });
        let b = self.c("Raised");
        let color = std::array::from_fn::<_, 4, _>(|i| mix(a[i], b[i]));
        set(
            actor,
            "Plate",
            &[("zoomy", &[height / row]), ("diffuse", &color)],
        );
        set(
            actor,
            "Shadow",
            &[
                ("zoomy", &[(height + 30.0) / (big + 30.0)]),
                ("diffusealpha", &[0.5 * weight]),
            ],
        );
        if header {
            set(actor, "Line", &[("diffusealpha", &[1.0 - weight])]);
            set(actor, "Middle", &[("y", &[(big - row) * weight / 2.0])]);
        } else {
            for name in ["Banner", "BannerBack"] {
                set(
                    actor,
                    name,
                    &[
                        ("zoom", &[mix(1.0, wheel_banner_big[0] / wheel_banner[0])]),
                        (
                            "y",
                            &[mix(
                                (row - wheel_banner[1]) / 2.0,
                                (big - wheel_banner_big[1]) / 2.0,
                            )],
                        ),
                    ],
                );
            }
            set(
                actor,
                "Fav",
                &[(
                    "y",
                    &[mix(
                        (row - wheel_banner[1]) / 2.0,
                        (big - wheel_banner_big[1]) / 2.0,
                    ) + 8.0],
                )],
            );
            let tx = WHEEL_BANNER_X + wheel_banner[0] + 14.0;
            let tx2 = WHEEL_BANNER_X + wheel_banner_big[0] + 16.0;
            let width = width - 2.0 * ROW_INSET;
            let room = mix(
                width - tx - WHEEL_RIGHT - 8.0,
                width - tx2 - WHEEL_RIGHT - 8.0,
            );
            let zoom = mix(1.0, WHEEL_TITLE_BIG / WHEEL_TITLE);
            set(
                actor,
                "Title",
                &[
                    ("xy", &[mix(tx, tx2), mix(row * 0.36, big * 0.38)]),
                    ("zoom", &[zoom]),
                    ("maxwidth", &[room / zoom]),
                ],
            );
            set(
                actor,
                "Subtitle",
                &[
                    ("xy", &[mix(tx, tx2), mix(row * 0.72, big * 0.66)]),
                    ("maxwidth", &[room]),
                ],
            );
        }
        set(actor, "Right", &[("y", &[(big - row) * weight / 2.0])]);
        true
    }

    fn wheel(&self, _list: &str, d: f32) -> Option<WheelPlace> {
        let pitch = self.m("Select.Wheel.Row");
        let big = self.m("Select.Wheel.Selected");
        let gap = self.m("Select.Wheel.Gap");
        let row = pitch - gap;
        let x = d.abs().min(1.0);
        let ease = x * x * (3.0 - 2.0 * x);
        let room = (big - row) / 2.0 * self.wheel_open.get();
        let y = (pitch - row) / 2.0 - room + if d > 0.0 { 2.0 * room * ease } else { 0.0 };
        Some(WheelPlace {
            x: self.m("Wheel.Drift") * d * d,
            y,
            zoom: 1.0,
            alpha: (1.0 - self.m("Wheel.Fade") * d.abs()).max(self.m("Wheel.Floor")),
        })
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod song_row_tests;
