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

#[cfg(test)]
use std::cell::RefCell;
use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::BTreeSet;
use std::sync::Arc;

use ddi_actor::{Actor, Fit, Kind, Rgba, Sprite};
use ddi_ui::look::{
    Look, Spec, WheelPlace, call, command_of, mono_width, named, quad, text_weighted, tracked,
};
/// The tunables as shipped (`build.rs` makes the tables below of it).
pub const TUNABLES: &str = include_str!("infinito.toml");

#[cfg(test)]
mod tunables;

mod keycaps;
pub use keycaps::KeycapMaterial;
mod active;
mod cursor;
mod rain;
mod wordmark;

mod tables {
    include!(concat!(env!("OUT_DIR"), "/tunables.rs"));
}

/// Our theme.
pub struct Infinito {
    keycap_material: std::cell::Cell<KeycapMaterial>,
    text_font: Arc<str>,
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
            numbers_font: tables::NUMBERS_FONT.into(),
            numbers_weight: tables::NUMBERS_WEIGHT,
            colors: tables::COLORS
                .iter()
                .map(|(k, c)| (k.to_string(), *c))
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

    /// Text in the text face.
    fn text(&self, name: &str, s: &str, size: f32, weight: u16, color: &str) -> Actor {
        text_weighted(&self.text_font, weight, name, s, size, self.c(color))
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
const LAMPS: [&str; 15] = [
    "Failed",
    "Clear",
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

fn frame(name: &str, children: Vec<Actor>) -> Actor {
    named(Kind::Frame(children), name)
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

/// Text with its left edge at `x`, its middle at `y`.
fn at_left(mut a: Actor, x: f32, y: f32) -> Actor {
    call(&mut a, "xy", &[x, y]);
    call(&mut a, "halign", &[0.0]);
    a
}

fn at_right(mut a: Actor, x: f32, y: f32) -> Actor {
    call(&mut a, "xy", &[x, y]);
    call(&mut a, "halign", &[1.0]);
    a
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
    if let Kind::Text(t) = &mut a.kind {
        t.max_width = w.max(1.0);
        t.ellipsis = true;
    }
}

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
            (144.0, 45.0, 12.0)
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
        let (ts, ss) = if compact { (15.0, 12.0) } else { (17.0, 13.0) };
        let mut title = at_left(self.text("Title", "", ts, 600, "Text"), tx, row * 0.36);
        let mut sub = at_left(self.text("Subtitle", "", ss, 400, "Muted"), tx, row * 0.72);
        // Room for the meters at the right.
        let cells = if compact {
            0.0
        } else {
            5.0 * 26.0 + 4.0 * 4.0 + 28.0 + 24.0
        };
        max_width(&mut title, w - tx - cells - 8.0);
        max_width(&mut sub, w - tx - cells - 8.0);
        if !compact {
            // Selected: the banner and the texts grow into the taller row.
            let (bw2, bh2) = (224.0, 70.0);
            let by2 = (big - bh2) / 2.0;
            for b in [&mut back, &mut banner] {
                command_of(
                    b,
                    "Select",
                    &format!("stoptweening;decelerate,{t};zoom,{};y,{by2}", bw2 / bw),
                );
                command_of(
                    b,
                    "Deselect",
                    &format!("stoptweening;decelerate,{t};zoom,1;y,{by}"),
                );
            }
            let tx2 = bx + bw2 + 16.0;
            // Narrower once moved right and grown (`maxwidth` is before
            // the zoom).
            let zoom = 21.0 / 17.0;
            let (room, room2) = (w - tx - cells - 8.0, w - tx2 - cells - 8.0);
            command_of(
                &mut title,
                "Select",
                &format!(
                    "stoptweening;maxwidth,{};decelerate,{t};xy,{tx2},{};zoom,{zoom}",
                    room2 / zoom,
                    big * 0.38
                ),
            );
            command_of(
                &mut title,
                "Deselect",
                &format!(
                    "stoptweening;maxwidth,{room};decelerate,{t};xy,{tx},{};zoom,1",
                    row * 0.36
                ),
            );
            command_of(
                &mut sub,
                "Select",
                &format!(
                    "stoptweening;maxwidth,{room2};decelerate,{t};xy,{tx2},{}",
                    big * 0.66
                ),
            );
            command_of(
                &mut sub,
                "Deselect",
                &format!(
                    "stoptweening;maxwidth,{room};decelerate,{t};xy,{tx},{}",
                    row * 0.72
                ),
            );
        }
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
        // The meters, kept at the right edge.
        let mut right = Vec::new();
        let cy = row / 2.0;
        if !compact {
            for (k, d) in DIFFICULTIES.iter().take(5).enumerate() {
                let x = w - 14.0 - 5.0 * 26.0 - 4.0 * 4.0 + 13.0 + k as f32 * 30.0;
                let mut cell = round(quad(&format!("Cell{k}"), [26.0, 26.0], self.c("Cell")), 4.0);
                call(&mut cell, "xy", &[x, cy]);
                self.colored(&mut cell, "Full", "Cell");
                self.colored(&mut cell, "Empty", "CellEmpty");
                let meter = at(
                    self.num(&format!("Meter{k}"), "", 14.0, &format!("Difficulty.{d}")),
                    x,
                    cy,
                );
                // The best's lamp, a line under the cell.
                let mut lamp = round(
                    quad(&format!("Lamp{k}"), [18.0, 3.0], self.c("Lamp.Clear")),
                    1.5,
                );
                call(&mut lamp, "xy", &[x, cy + 16.0]);
                self.lamp_commands(&mut lamp);
                let mut both = frame(&format!("Meters{k}"), vec![cell, meter, lamp]);
                command_of(&mut both, "Unavailable", "diffusealpha,0");
                command_of(&mut both, "Available", "diffusealpha,1");
                right.push(both);
            }
        }
        // Edits and named charts, counted, left of the meters.
        if !compact {
            let x = w - 14.0 - 5.0 * 26.0 - 4.0 * 4.0 - 8.0;
            let mut edits = at_right(self.num("Edits", "", 12.0, "Difficulty.Edit"), x, cy);
            command_of(&mut edits, "Unavailable", "diffusealpha,0");
            command_of(&mut edits, "Available", "diffusealpha,1");
            right.push(edits);
        }
        let mut none = at_right(self.text("NoCharts", "", 12.0, 400, "Faint"), w - 14.0, cy);
        hide(&mut none);
        command_of(&mut none, "Unavailable", "diffusealpha,1");
        command_of(&mut none, "Available", "diffusealpha,0");
        right.push(none);
        let mut right = frame("Right", right);
        if !compact {
            let dy = (big - row) / 2.0;
            command_of(
                &mut right,
                "Select",
                &format!("stoptweening;decelerate,{t};y,{dy}"),
            );
            command_of(
                &mut right,
                "Deselect",
                &format!("stoptweening;decelerate,{t};y,0"),
            );
        }
        inset(frame("", vec![shadow, p, back, banner, title, sub, right]))
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
        let mut middle = frame("Middle", vec![chevron, cover, title]);
        let mut right = frame("Right", vec![count]);
        if !compact {
            let dy = (big - row) / 2.0;
            for a in [&mut middle, &mut right] {
                command_of(a, "Select", &format!("stoptweening;decelerate,{t};y,{dy}"));
                command_of(a, "Deselect", &format!("stoptweening;decelerate,{t};y,0"));
            }
        }
        inset(frame("", vec![p, line, middle, right]))
    }

    /// The selected song: banner, titles, BPM, length and pack (narrow:
    /// title and one line).
    fn song_info(&self, spec: &Spec) -> Actor {
        let [w, h] = spec.size;
        // A phone's (upright: as wide as the screen, as short).
        let compact = w < 400.0 || h < 220.0;
        let bh = if compact {
            (w * 94.0 / 300.0).round()
        } else {
            self.m("Select.Banner.Height")
        };
        let back = round(
            plate("BannerBack", [0.0, 0.0], [w, bh], self.c("BannerEmpty")),
            6.0,
        );
        let mut banner = named(Kind::Sprite(Sprite::quad([w, bh])), "Banner");
        call(&mut banner, "cornerradius", &[6.0]);
        call(&mut banner, "halign", &[0.0]);
        call(&mut banner, "valign", &[0.0]);
        banner.visible = false;
        // With a jacket: the jacket square at the right of the banner's
        // place, the banner (cropped to it) in the rest.
        let gap = self.m("Select.Jacket.Gap");
        let mut short = named(
            Kind::Sprite(Sprite::quad([(w - bh - gap).max(1.0), bh])),
            "BannerShort",
        );
        let mut jacket = named(Kind::Sprite(Sprite::quad([bh, bh])), "Jacket");
        call(&mut jacket, "x", &[w - bh]);
        for s in [&mut short, &mut jacket] {
            if let Kind::Sprite(sp) = &mut s.kind {
                sp.fit = Fit::Cover;
            }
            call(s, "cornerradius", &[6.0]);
            call(s, "halign", &[0.0]);
            call(s, "valign", &[0.0]);
            s.visible = false;
        }
        let jacket_back = {
            let mut b = round(
                plate("JacketBack", [w - bh, 0.0], [bh, bh], self.c("BannerEmpty")),
                6.0,
            );
            b.visible = false;
            b
        };
        // The CD title, a small mark in the banner's lower left corner,
        // whole (its own shape kept).
        let scale = if compact {
            bh / self.m("Select.Banner.Height")
        } else {
            1.0
        };
        let [cw, ch] = [
            self.m("Select.CdTitle.Width") * scale,
            self.m("Select.CdTitle.Height") * scale,
        ];
        let inset = 8.0 * scale;
        let mut cd = named(Kind::Sprite(Sprite::quad([cw, ch])), "CdTitle");
        if let Kind::Sprite(sp) = &mut cd.kind {
            sp.fit = Fit::Contain;
        }
        call(&mut cd, "halign", &[0.0]);
        call(&mut cd, "valign", &[0.0]);
        call(&mut cd, "xy", &[inset, bh - inset - ch]);
        cd.visible = false;
        let edge = outline("Edge", [0.0, 0.0, w, bh], 1.0, [1.0, 1.0, 1.0, 0.08]);
        let mut c = vec![back, banner, short, jacket_back, jacket, cd, edge];
        if compact {
            let mut title = at_left(
                self.text("Title", "", 18.0, 700, "Text"),
                0.0,
                bh + 8.0 + 11.0,
            );
            max_width(&mut title, w);
            let mut line = at_left(
                self.text("Line", "", 13.0, 400, "Muted"),
                0.0,
                bh + 8.0 + 30.0,
            );
            max_width(&mut line, w);
            c.extend([title, line]);
        } else {
            let ty = bh + 12.0 + 17.6;
            let mut title = at_left(self.text("Title", "", 32.0, 700, "Text"), 0.0, ty);
            max_width(&mut title, w * 0.78);
            let mut sub = at_right(self.text("Subtitle", "", 17.0, 400, "Muted"), w, ty + 4.0);
            max_width(&mut sub, w * 0.2);
            let ay = bh + 12.0 + 35.2 + 2.0 + 10.2;
            let mut artist = at_left(self.text("Artist", "", 17.0, 400, "Secondary"), 0.0, ay);
            max_width(&mut artist, w * 0.5);
            // The credit line (a bundled song's artist and licence), at
            // the right of the artist's.
            let mut credit = at_right(self.text("Credit", "", 12.0, 400, "Muted"), w, ay + 2.0);
            max_width(&mut credit, w * 0.48);
            let sy = ay + 10.2 + 12.0 + 12.0;
            c.extend([
                title,
                sub,
                artist,
                at_left(
                    tracked(self.text("BpmLabel", "BPM", 12.0, 400, "Muted"), 80),
                    0.0,
                    sy + 2.0,
                ),
                at_left(self.num("Bpm", "", 20.0, "Text"), 32.0, sy),
                at_left(
                    tracked(self.text("LengthLabel", "LENGTH", 12.0, 400, "Muted"), 80),
                    108.0,
                    sy + 2.0,
                ),
                at_left(self.num("Length", "", 20.0, "Text"), 160.0, sy),
                at_left(
                    tracked(self.text("PackLabel", "PACK", 12.0, 400, "Muted"), 80),
                    236.0,
                    sy + 2.0,
                ),
                {
                    let mut p = at_left(self.text("Pack", "", 14.0, 600, "Text"), 274.0, sy + 1.0);
                    max_width(&mut p, w - 274.0 - 120.0);
                    p
                },
                // The song's own offset, when set.
                at_right(self.text("Offset", "", 13.0, 600, "Accent"), w, sy + 1.0),
                credit,
            ]);
        }
        frame("", c)
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

    /// A chart of the ladder: its color bar, difficulty, notes and step
    /// artist, and its meter large in its color.
    fn ladder_row(&self, spec: &Spec) -> Actor {
        let [w, h] = spec.size;
        let d = if DIFFICULTIES.contains(&spec.text.as_str()) {
            spec.text.as_str()
        } else {
            "Edit"
        };
        let color = format!("Difficulty.{d}");
        let mut p = round(plate("Plate", [0.0, 0.0], [w, h], self.c("Row")), 6.0);
        self.colored(&mut p, "Select", "Selected");
        self.colored(&mut p, "Deselect", "Row");
        let mut bar = round(plate("Bar", [0.0, 0.0], [6.0, h], self.c(&color)), 3.0);
        let label = at_left(
            tracked(self.text("Label", "", 14.0, 600, "Text"), 100),
            18.0,
            h / 2.0,
        );
        let mut notes = at_left(
            self.text("Notes", "", 13.0, 400, "Muted"),
            18.0 + 120.0,
            h / 2.0,
        );
        // Room for the best (grade, score, lamp) and the meter.
        max_width(&mut notes, w - 138.0 - 60.0 - 150.0);
        let mut meter = at_right(
            self.num("Meter", "", if h < 40.0 { 24.0 } else { 28.0 }, &color),
            w - 14.0,
            h / 2.0,
        );
        for d in DIFFICULTIES {
            let c = rgba(self.c(&format!("Difficulty.{d}")));
            command_of(&mut bar, d, &format!("diffuse,{c}"));
            command_of(&mut meter, d, &format!("diffuse,{c}"));
        }
        let small = h < 40.0;
        let grade = at_right(
            self.num("Grade", "", if small { 15.0 } else { 17.0 }, "Text"),
            w - 64.0,
            if small { h / 2.0 } else { h / 2.0 - 7.0 },
        );
        let mut score = at_right(
            self.text("Score", "", 11.0, 400, "Muted"),
            w - 64.0,
            h / 2.0 + 10.0,
        );
        if small {
            hide(&mut score);
        }
        let mut lamp_text = at_right(
            tracked(self.text("LampText", "", 11.0, 700, "Muted"), 80),
            w - 64.0 - 70.0,
            h / 2.0,
        );
        self.lamp_commands_text(&mut lamp_text);
        let mut lamp = round(
            plate(
                "Lamp",
                [w - 4.0, 6.0],
                [3.0, h - 12.0],
                self.c("Lamp.Clear"),
            ),
            1.5,
        );
        self.lamp_commands(&mut lamp);
        frame(
            "",
            vec![p, bar, label, notes, lamp_text, grade, score, lamp, meter],
        )
    }

    /// `Lamp.<name>` commands: the lamp's color, or hidden for none.
    fn lamp_commands(&self, a: &mut Actor) {
        hide(a);
        command_of(a, "Lamp.None", "diffusealpha,0");
        for name in LAMPS {
            let c = rgba(self.c(&format!("Lamp.{name}")));
            command_of(a, &format!("Lamp.{name}"), &format!("diffuse,{c}"));
        }
    }

    /// The same for a lamp's text (shown in its color).
    fn lamp_commands_text(&self, a: &mut Actor) {
        command_of(a, "Lamp.None", "diffusealpha,0");
        for name in LAMPS {
            let c = rgba(self.c(&format!("Lamp.{name}")));
            command_of(a, &format!("Lamp.{name}"), &format!("diffuse,{c}"));
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
        for d in DIFFICULTIES {
            let c = rgba(self.c(&format!("Difficulty.{d}")));
            command_of(&mut top, d, &format!("diffuse,{c}"));
            command_of(&mut meter, d, &format!("diffuse,{c}"));
        }
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

    /// The quick volume's button: the level's speaker
    /// (`ui/volume.svg`'s cells, picked by the widget) on a plate.
    fn volume_button(&self, size: [f32; 2]) -> Actor {
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
        for j in JUDGEMENTS {
            let key = format!("Judgement.{j}");
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
        // The option row widget is focused; the options screen's rows are
        // told which is (their frame is what takes focus).
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
        let mut parts = vec![p, label];
        if centered {
            // Where the choices go (the option row widget's `Choices`).
            let span = w - 16.0 - 190.0 - 16.0;
            let mut choices = quad("Choices", [span, h], [0.0; 4]);
            call(&mut choices, "x", &[x0 + 16.0 + 190.0 + span / 2.0]);
            choices.visible = false;
            parts.push(choices);
        }
        frame("", parts)
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
            "TitleMenu.Background" => Some("ui/menu-rain.svg"),
            "KeyHint" => Some(self.keycap_material.get().image()),
            _ => None,
        }
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
            "Button" => self.button(spec, "Plate2", "Text", 15.0, 600),
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
            "PlayButton" => self.button(spec, "Accent", "OnAccent", 18.0, 700),
            "IconButton" => self.button(spec, "Plate2", "Text", 20.0, 700),
            "BackButton" => self.back_button(spec.size),
            "Volume" => self.volume_button(spec.size),
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
                    c.retain(|a| a.name != "Choices");
                    // The whole row takes a tap, not only its ends: a hit
                    // area must be drawn, however faintly.
                    let hit = quad("Hit", spec.size, [0.0, 0.0, 0.0, 0.004]);
                    c.insert(0, hit);
                    c.extend([bx, mark]);
                }
                f
            }
            "OptionRow" => self.option_row(spec, true),
            "OptionRow.Choice" => self.text("", &spec.text, 15.0, 600, "Text"),
            "OptionRow.Cursor" => quad("", [w, 2.0], self.c("Accent")),
            "Slider" => {
                let mut f = self.option_row(spec, true);
                let track_w = w * 0.45;
                let l = w / 2.0 - 80.0 - track_w;
                let mut track = round(quad("Track", [track_w, 4.0], self.c("Line2")), 2.0);
                call(&mut track, "x", &[l + track_w / 2.0]);
                let mut fill = round(quad("Fill", [track_w, 4.0], self.c("Accent")), 2.0);
                call(&mut fill, "x", &[l]);
                call(&mut fill, "halign", &[0.0]);
                let mut knob = round(quad("Knob", [8.0, h * 0.55], self.c("Text")), 4.0);
                call(&mut knob, "x", &[l]);
                let value = at_right(self.num("Value", "", 15.0, "Text"), w / 2.0 - 16.0, 0.0);
                if let Kind::Frame(c) = &mut f.kind {
                    c.retain(|a| a.name != "Choices");
                    c.extend([track, fill, knob, value]);
                }
                f
            }
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
            "Screen.Header" => self.header(spec),
            "Screen.Footer" => {
                let p = plate("Plate", [0.0, 0.0], [w, h], self.c("Footer"));
                let line = plate("Line", [0.0, 0.0], [w, 1.0], self.c("Line"));
                frame("", vec![p, line])
            }
            "SongInfo" => self.song_info(spec),
            "StyleTab" => self.style_tab(spec),
            "ChartLadder.Row" => self.ladder_row(spec),
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
        Some(self.text_font.clone())
    }

    fn font_for(&self, role: &str) -> Option<Arc<str>> {
        match role {
            "Numbers" => Some(self.numbers_font.clone()),
            _ => Some(self.text_font.clone()),
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
                        ("zoom", &[mix(1.0, 224.0 / 144.0)]),
                        ("y", &[mix((row - 45.0) / 2.0, (big - 70.0) / 2.0)]),
                    ],
                );
            }
            let tx = 12.0 + 144.0 + 14.0;
            let tx2 = 12.0 + 224.0 + 16.0;
            let cells = 5.0 * 26.0 + 4.0 * 4.0 + 28.0 + 24.0;
            let width = width - 2.0 * ROW_INSET;
            let room = mix(width - tx - cells - 8.0, width - tx2 - cells - 8.0);
            let zoom = mix(1.0, 21.0 / 17.0);
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
