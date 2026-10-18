//! Options (`docs/plans/theme.md`, drawn in layout A):
//!
//! - **Left**: the sections (`Options.Section`); on a compact screen a
//!   strip of tabs across the top (`StyleTab`).
//! - **Middle**: option rows as StepMania lays them out (`Options.Row`):
//!   the label on the left, the choices on the right with the chosen one
//!   underlined (`OptionRow.Choice`, `OptionRow.Cursor`); numbers, and
//!   choices too many for the row, step with ◀ value ▶; a row of buttons
//!   does something (bind, reset, export). The rows scroll (keys keep the
//!   focused one in view; a wheel or a drag move them).
//! - **Right**: the field's preview (`Options.Preview`): the shell draws
//!   the demo chart with the options chosen into the texture
//!   [`FIELD_PREVIEW`], told where by [`UiRequest::FieldPreview`]; until
//!   it does, the panel's placeholder shows. Its layout selector sits
//!   directly above it, available in every section.
//! - **Below**: what the focused option does (`Options.Description`).
//! - **Keys**: Up and Down move through the options, Left and Right
//!   change the focused one (or move between its buttons), Start presses
//!   a button, Tab (the menu's Select) moves to the next section, Back
//!   leaves. Changes are saved as they are made (the shell's
//!   [`OptionsShell`], or the sample's [`ChangeFn`]).
//!   P focuses the preview layout; Down past a section's last row does
//!   too. Left/Right change it, Up/Down return to the options.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_actor::{Actor, Kind, Sprite};

use super::data::{OptionDef, OptionKind, OptionsData};
use super::{Env, footer, frame, ground, header, left, right};
use crate::gesture::Gesture;
use crate::hit;
use crate::look::{self, Spec, call, quad};
use crate::message::Message;
use crate::nav::{Focusable, MenuAction};
use crate::request::UiRequest;
use crate::screen::{Cx, Handled, Input, Screen};
use crate::widgets::{part, set_text};

/// The texture the shell draws the field's preview into.
pub const FIELD_PREVIEW: &str = "field-preview";

/// Broadcast by the shell when it put new rows in the [`OptionsFeed`].
pub const OPTIONS_CHANGED: &str = "Options.Changed";

/// Told each change: the section's name and the option as it is now (the
/// developer routes' sample).
pub type ChangeFn = Rc<dyn Fn(&str, &OptionDef)>;

/// What the screen asks of its shell.
#[derive(Clone, Debug, PartialEq)]
pub enum OptionsCall {
    /// A row's value changed.
    Change { id: String, index: usize },
    /// A row's button pressed (in the gesture).
    Action { id: String, button: usize },
    /// Back: leave the options.
    Leave,
}

/// Carries a call out, in the input's handler; the rows as they are now,
/// when they changed.
pub type OptionsShell = Rc<dyn Fn(OptionsCall, &mut Cx) -> Option<OptionsData>>;

/// New rows from the shell outside a call (an export finished, a
/// controller connected), with [`OPTIONS_CHANGED`] broadcast.
pub type OptionsFeed = Rc<RefCell<Option<OptionsData>>>;

const ROW: f32 = 42.0;
const ROW_GAP: f32 = 4.0;
const HEADING: f32 = 26.0;

/// Where things go, in the screen's own pixels ([`OptionsScreen::fit`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Places {
    /// The layout's size and the zoom from it to the virtual screen.
    screen: [f32; 2],
    zoom: f32,
    compact: bool,
    /// The sections' rail (full) or tab strip (compact), `[x, y, w, h]`.
    sections: [f32; 4],
    rows: [f32; 4],
    label_w: f32,
    preview: Option<[f32; 4]>,
    desc: [f32; 4],
}

pub struct OptionsScreen {
    env: Env,
    data: OptionsData,
    section: usize,
    focus: usize,
    preview_focused: bool,
    places: Places,
    /// How far the rows are scrolled, and the tab strip.
    scroll: f32,
    /// The slider row a pointer drags (pressed on its track).
    sliding: Option<usize>,
    tabs_scroll: f32,
    /// The strip follows the chosen tab (not while moved by hand).
    tabs_follow: bool,
    changed: Option<ChangeFn>,
    shell: Option<OptionsShell>,
    feed: Option<OptionsFeed>,
    /// The preview's place last told the shell.
    told: Option<Option<[f32; 4]>>,
    texture: u32,
}

impl OptionsScreen {
    pub fn new(env: Env, data: OptionsData, changed: Option<ChangeFn>) -> OptionsScreen {
        OptionsScreen {
            env,
            data,
            section: 0,
            focus: 0,
            preview_focused: false,
            places: Places::default(),
            scroll: 0.0,
            sliding: None,
            tabs_scroll: 0.0,
            tabs_follow: true,
            changed,
            shell: None,
            feed: None,
            told: None,
            texture: ddi_actor::UNTEXTURED,
        }
    }

    /// On the player's settings: calls go to `shell`, new rows come
    /// through `feed`.
    pub fn with_shell(mut self, shell: OptionsShell, feed: OptionsFeed) -> OptionsScreen {
        self.shell = Some(shell);
        self.feed = Some(feed);
        self
    }

    /// Opens at the section named `name`.
    pub fn with_section(mut self, name: &str) -> OptionsScreen {
        if let Some(i) = self.data.sections.iter().position(|s| s.name == name) {
            self.section = i;
        }
        self
    }

    fn options(&self) -> &[OptionDef] {
        self.data
            .sections
            .get(self.section)
            .map_or(&[], |s| &s.options)
    }

    fn selected_option(&self) -> Option<&OptionDef> {
        if self.preview_focused {
            self.data.preview_layout.as_ref()
        } else {
            self.options().get(self.focus)
        }
    }

    /// The layout's size for the virtual screen: the full design as it
    /// is; a compact one sideways as it is (a phone's own size); upright,
    /// 480 wide, zoomed to the screen's width.
    fn fit(&self, vp: ddi_actor::screen::VirtualScreen) -> ([f32; 2], f32, bool) {
        if !super::is_compact(&*self.env.look, vp) {
            return ([vp.width, vp.height], 1.0, false);
        }
        let narrow = self.env.metric("Options.Narrow.Width", 480.0);
        if vp.width >= vp.height || vp.width <= narrow {
            ([vp.width, vp.height], 1.0, true)
        } else {
            let z = vp.width / narrow;
            ([narrow, vp.height / z], z, true)
        }
    }

    fn place(&mut self, vp: ddi_actor::screen::VirtualScreen) {
        let (screen, zoom, compact) = self.fit(vp);
        let env = &self.env;
        let [w, h] = screen;
        let selector = if self.data.preview_layout.is_some() {
            ROW + 8.0
        } else {
            0.0
        };
        self.places = if !compact {
            let design_w = env.metric("Design.Width", 1280.0);
            let m = env.metric("Margin", 32.0);
            let rows_x = env.metric("Options.Rows.X", 272.0);
            let preview_x = w - (design_w - env.metric("Options.Preview.X", 904.0));
            // What the focused option does: its name and room for the
            // longest sentence.
            let desc_w = w - env.metric("Options.Rows.X", 272.0) - env.metric("Margin", 32.0);
            let desc_h = self.desc_height(desc_w);
            let desc_y = h - env.metric("Footer.Height", 40.0) - 12.0 - desc_h;
            let top = 88.0;
            Places {
                screen,
                zoom,
                compact,
                sections: [
                    m,
                    top,
                    env.metric("Options.Rail.Width", 208.0),
                    desc_y - top,
                ],
                rows: [
                    rows_x,
                    top,
                    env.metric("Options.Rows.Width", 600.0),
                    desc_y - 16.0 - top,
                ],
                label_w: 220.0,
                preview: Some([
                    preview_x,
                    top + selector,
                    344.0,
                    (desc_y - 16.0 - top).min(500.0) - selector,
                ]),
                desc: [rows_x, desc_y, w - rows_x - m, desc_h],
            }
        } else {
            let m = 12.0;
            let top = 92.0;
            if w >= h {
                // Sideways: the rows down the whole height; beside them
                // the preview, and what the focused option does under it.
                let pw = env.metric("Options.Compact.Preview", 320.0).min(w * 0.38);
                let right = h - top - m;
                let ph = ((right - selector) * 0.45)
                    .min(right - selector - 8.0 - self.desc_height(pw))
                    .max(64.0)
                    .round();
                Places {
                    screen,
                    zoom,
                    compact,
                    sections: [m, 50.0, w - 2.0 * m, 36.0],
                    rows: [m, top, w - 3.0 * m - pw, right],
                    label_w: 170.0,
                    preview: Some([w - m - pw, top + selector, pw, ph]),
                    desc: [
                        w - m - pw,
                        top + selector + ph + 8.0,
                        pw,
                        right - selector - ph - 8.0,
                    ],
                }
            } else {
                // Upright: rows, what the focused one does, the preview.
                let ph = (h * 0.3).clamp(200.0, 320.0);
                let desc_h = self.desc_height(w - 2.0 * m);
                let rows_h = h - top - desc_h - selector - ph - 4.0 * m;
                Places {
                    screen,
                    zoom,
                    compact,
                    sections: [m, 50.0, w - 2.0 * m, 36.0],
                    rows: [m, top, w - 2.0 * m, rows_h],
                    label_w: 160.0,
                    preview: Some([m, h - m - ph, w - 2.0 * m, ph]),
                    desc: [m, top + rows_h + m, w - 2.0 * m, desc_h],
                }
            }
        };
    }

    fn build_tree(&mut self) -> Actor {
        let env = self.env.clone();
        let p = self.places;
        let screen = p.screen;
        let mut children = vec![ground(&env, screen)];
        children.push(self.sections_actor());
        children.push(self.rows_actor());
        if let Some(selector) = self.preview_selector() {
            children.push(selector);
        }
        if let Some([px, py, pw, ph]) = p.preview {
            let mut panel = env.component(
                "Options.Preview",
                "Panel",
                &Spec::new([pw, ph], &self.data.preview),
                "Preview",
                [px, py],
            );
            set_text(&mut panel, "Title", &self.data.preview);
            // The field, drawn by the shell; nothing while it is not.
            let mut field = Actor::new(Kind::Sprite(Sprite::quad([pw, ph])));
            field.name = "Preview.Field".into();
            call(&mut field, "xy", &[pw / 2.0, ph / 2.0]);
            if let Kind::Frame(c) = &mut panel.kind {
                c.push(field);
            }
            children.push(panel);
        }
        let [dx, dy, dw, dh] = p.desc;
        children.push(env.component(
            "Options.Description",
            "Panel",
            &Spec::new([dw, dh], ""),
            "Description",
            [dx, dy],
        ));
        let margin = env.metric("Margin", 32.0);
        if p.compact {
            children.push(env.component(
                "IconButton",
                "Button",
                &Spec::new([40.0, 40.0], "‹"),
                "Back",
                [12.0 + 20.0, 24.0],
            ));
            children.push(left(
                env.text("Title", "Options", 16.0, 700, "Text"),
                64.0,
                24.0,
            ));
            let mut n = env.text("Saved", "", 12.0, 400, "Muted");
            call(&mut n, "maxwidth", &[screen[0] - 200.0]);
            ellipsize(&mut n);
            children.push(right(n, screen[0] - 12.0, 24.0));
        } else {
            children.push(header(&env, screen[0], "OPTIONS"));
            let mut n = env.text("Saved", "", 13.0, 400, "Muted");
            call(&mut n, "maxwidth", &[screen[0] - 600.0]);
            children.push(right(n, screen[0] - margin, 32.0));
            let (f, texts) = footer(
                &env,
                screen,
                &[
                    (&["↑↓"], "Option"),
                    (&["←→"], "Change"),
                    (&["P"], "Preview"),
                    (&["TAB"], "Section"),
                    (&["ESC"], "Back"),
                ],
                "",
                &[("Link.Back", "Back to songs")],
            );
            env.state.borrow_mut().hints = texts;
            children.push(f);
        }
        let mut root = Actor::new(Kind::Frame(children));
        super::entrance(&mut root, &env);
        if p.zoom != 1.0 {
            call(&mut root, "zoom", &[p.zoom]);
        }
        root
    }

    fn preview_selector(&self) -> Option<Actor> {
        let [x, y, w, _] = self.places.preview?;
        let option = self.data.preview_layout.as_ref()?;
        let env = &self.env;
        let mut selector = env.component(
            "Button",
            "Button",
            &Spec::new([w, ROW], &format!("Preview: {}", option.value())),
            "PreviewLayout",
            [x + w / 2.0, y - 8.0 - ROW / 2.0],
        );
        if let Some(label) = part(&mut selector, "Label") {
            call(label, "maxwidth", &[w - 96.0]);
            ellipsize(label);
        }
        if let Kind::Frame(children) = &mut selector.kind {
            for (name, text, x) in [("Down", "◀", -w / 2.0 + 22.0), ("Up", "▶", w / 2.0 - 22.0)]
            {
                let hit = quad("", [44.0, ROW], [0.0, 0.0, 0.0, 0.004]);
                let arrow = env.text("", text, 14.0, 400, "Accent");
                let mut button = frame(&format!("PreviewLayout.{name}"), vec![hit, arrow]);
                call(&mut button, "x", &[x]);
                children.push(button);
            }
        }
        crate::widgets::play(
            &mut selector,
            if self.preview_focused {
                "GainFocus"
            } else {
                "LoseFocus"
            },
        );
        selector.finish_tweening();
        Some(selector)
    }

    /// The sections: a rail, or a strip of tabs scrolled to the chosen.
    fn sections_actor(&mut self) -> Actor {
        let env = self.env.clone();
        let [sx, sy, sw, sh] = self.places.sections;
        let mut items = Vec::new();
        if !self.places.compact {
            for (i, s) in self.data.sections.iter().enumerate() {
                let mut a = env.component(
                    "Options.Section",
                    "List.Header",
                    &Spec::new([sw, 44.0], &s.name),
                    &format!("Section{i}"),
                    [0.0, i as f32 * 48.0],
                );
                set_text(&mut a, "Title", &s.name);
                // The whole of it takes a tap, drawn plate or not.
                let mut hit = quad("Hit", [sw, 44.0], [0.0, 0.0, 0.0, 0.004]);
                call(&mut hit, "xy", &[sw / 2.0, 22.0]);
                if let Kind::Frame(c) = &mut a.kind {
                    c.insert(0, hit);
                }
                crate::widgets::play(
                    &mut a,
                    if i == self.section {
                        "Select"
                    } else {
                        "Deselect"
                    },
                );
                a.finish_tweening();
                items.push(a);
            }
            let mut r = frame("Sections", items);
            call(&mut r, "xy", &[sx, sy]);
            return r;
        }
        let mut x = 0.0;
        let mut spans = Vec::new();
        for (i, s) in self.data.sections.iter().enumerate() {
            let mut a = look::component_or(
                &*env.look,
                "StyleTab",
                "Button",
                &Spec::new([0.0, sh - 4.0], &s.name),
            );
            let w = super::extent(&mut a, [look::mono_width(&s.name, 13.0) + 20.0, sh])[0];
            a.name = format!("Section{i}");
            call(&mut a, "xy", &[x + w / 2.0, sh / 2.0]);
            crate::widgets::play(
                &mut a,
                if i == self.section {
                    "Selected"
                } else {
                    "Available"
                },
            );
            a.finish_tweening();
            spans.push((x, w));
            items.push(a);
            x += w + 6.0;
        }
        // The chosen tab in the middle, where it can be (its neighbours
        // show, to be tapped next); a drag or a wheel moves the strip
        // until the next choice.
        if self.tabs_follow
            && let Some(&(tx, tw)) = spans.get(self.section)
        {
            self.tabs_scroll = tx + tw / 2.0 - sw / 2.0;
        }
        let total = (x - 6.0).max(0.0);
        self.tabs_scroll = self.tabs_scroll.clamp(0.0, (total - sw).max(0.0));
        let mut strip = frame("SectionStrip", items);
        call(&mut strip, "x", &[-self.tabs_scroll]);
        let mut clip = frame("Sections", vec![strip]);
        clip.clip = Some([0.0, 0.0, sw, sh]);
        call(&mut clip, "xy", &[sx, sy]);
        clip
    }

    /// Where row `i` is in the rows' frame, and how tall all are.
    fn row_tops(&self) -> (Vec<f32>, f32) {
        let mut at = 0.0;
        let mut tops = Vec::new();
        for o in self.options() {
            if o.heading.is_some() {
                at += HEADING;
            }
            tops.push(at);
            at += ROW + ROW_GAP;
        }
        (tops, at)
    }

    /// The rows of the section, with their headings, scrolled.
    fn rows_actor(&self) -> Actor {
        let env = &self.env;
        let [x, y, w, h] = self.places.rows;
        let mut children = Vec::new();
        let (tops, _) = self.row_tops();
        for (i, o) in self.options().iter().enumerate() {
            let at = tops[i];
            if let Some(hd) = &o.heading {
                let t = look::tracked(
                    env.text(&format!("Heading{i}"), hd, 11.0, 600, "Faint"),
                    120,
                );
                children.push(left(t, 16.0, at - HEADING / 2.0 + 2.0));
            }
            let mut row = env.component(
                "Options.Row",
                "List.Row",
                &Spec::new([w, ROW], &o.label),
                &format!("Option{i}"),
                [0.0, at],
            );
            set_text(&mut row, "Label", &o.label);
            set_text(&mut row, "Title", &o.label);
            if let Some(l) = part(&mut row, "Label") {
                call(l, "maxwidth", &[self.places.label_w - 8.0]);
            }
            if let Kind::Frame(c) = &mut row.kind {
                c.push(self.values(i, o));
            }
            // `Select`, not focus: the rows' frame is what takes focus.
            crate::widgets::play(
                &mut row,
                if i == self.focus && !self.preview_focused {
                    "Select"
                } else {
                    "Deselect"
                },
            );
            row.finish_tweening();
            children.push(row);
        }
        let mut f = frame("Rows", children);
        call(&mut f, "y", &[-self.scroll]);
        let mut clip = frame("RowsClip", vec![f]);
        // Room for the focused row's ring.
        clip.clip = Some([-6.0, -6.0, w + 6.0, h + 6.0]);
        call(&mut clip, "xy", &[x, y]);
        clip
    }

    /// A row's values: choices with the chosen one underlined, a stepper,
    /// or buttons.
    fn values(&self, i: usize, o: &OptionDef) -> Actor {
        let env = &self.env;
        let w = self.places.rows[2];
        let x0 = 16.0 + self.places.label_w;
        let room = w - x0 - 12.0;
        let mid = ROW / 2.0;
        let mut c = Vec::new();
        let stepper = |c: &mut Vec<Actor>, value: &str, numbers: bool| {
            c.push(left(
                env.text(&format!("Option{i}.Down"), "◀", 14.0, 400, "Accent"),
                x0,
                mid,
            ));
            let vw = if numbers {
                80.0
            } else {
                (room - 60.0).clamp(80.0, 260.0)
            };
            let mut val = if numbers {
                env.numbers(&format!("Option{i}.Value"), value, 22.0, "Text")
            } else {
                let mut t = env.text(&format!("Option{i}.Value"), value, 15.0, 600, "Text");
                call(&mut t, "maxwidth", &[vw]);
                t
            };
            call(&mut val, "xy", &[x0 + 22.0 + vw / 2.0, mid]);
            c.push(val);
            c.push(left(
                env.text(&format!("Option{i}.Up"), "▶", 14.0, 400, "Accent"),
                x0 + 22.0 + vw + 8.0,
                mid,
            ));
            x0 + 22.0 + vw + 30.0
        };
        match &o.kind {
            OptionKind::Choices(choices) => {
                let total: f32 = choices
                    .iter()
                    .map(|ch| look::mono_width(ch, 15.0) + 16.0)
                    .sum::<f32>()
                    - 16.0;
                if total <= room {
                    let mut x = x0;
                    for (j, ch) in choices.iter().enumerate() {
                        let chosen = j == o.index;
                        let cw = look::mono_width(ch, 15.0);
                        let t = env.text(
                            &format!("Option{i}.Choice{j}"),
                            ch,
                            15.0,
                            if chosen { 600 } else { 400 },
                            if chosen { "Text" } else { "Faint" },
                        );
                        c.push(left(t, x, mid));
                        if chosen {
                            let mut cur = look::component(
                                &*env.look,
                                "OptionRow.Cursor",
                                &Spec::new([cw, 2.0], ""),
                            );
                            cur.name = format!("Option{i}.Cursor");
                            call(&mut cur, "xy", &[x + cw / 2.0, mid + 12.0]);
                            c.push(cur);
                        }
                        x += cw + 16.0;
                    }
                } else {
                    // Too many for the row: one at a time.
                    stepper(&mut c, o.value(), false);
                }
            }
            OptionKind::Stepper(values) => {
                let v = values.get(o.index).map_or("", String::as_str);
                let end = stepper(&mut c, v, true);
                if !o.note.is_empty() {
                    let mut n = env.text(&format!("Option{i}.Note"), &o.note, 13.0, 400, "Muted");
                    call(&mut n, "maxwidth", &[(w - end - 12.0).max(40.0)]);
                    c.push(left(n, end, mid));
                }
            }
            OptionKind::Slider(values) => {
                // The track, its fill and knob, the value and the note.
                let tw = (room - 190.0).clamp(80.0, 320.0);
                let f = if values.len() > 1 {
                    o.index as f32 / (values.len() - 1) as f32
                } else {
                    0.0
                };
                let mut hit = quad(
                    &format!("Option{i}.Track"),
                    [tw + 16.0, ROW - 8.0],
                    [0.0, 0.0, 0.0, 0.004],
                );
                call(&mut hit, "xy", &[x0 + tw / 2.0, mid]);
                c.push(hit);
                let line = env.color("Line2");
                let mut track = quad("", [tw, 4.0], line);
                call(&mut track, "cornerradius", &[2.0]);
                call(&mut track, "xy", &[x0 + tw / 2.0, mid]);
                c.push(track);
                if f > 0.0 {
                    let mut fill = quad(
                        &format!("Option{i}.Fill"),
                        [tw * f, 4.0],
                        env.color("Accent"),
                    );
                    call(&mut fill, "cornerradius", &[2.0]);
                    call(&mut fill, "xy", &[x0 + tw * f / 2.0, mid]);
                    c.push(fill);
                }
                let mut knob = quad(&format!("Option{i}.Knob"), [8.0, 18.0], env.color("Text"));
                call(&mut knob, "cornerradius", &[4.0]);
                call(&mut knob, "xy", &[x0 + tw * f, mid]);
                c.push(knob);
                let v = values.get(o.index).map_or("", String::as_str);
                let mut val = env.numbers(&format!("Option{i}.Value"), v, 22.0, "Text");
                call(&mut val, "xy", &[x0 + tw + 20.0 + 40.0, mid]);
                c.push(val);
                let end = x0 + tw + 20.0 + 90.0;
                if !o.note.is_empty() {
                    let mut n = env.text(&format!("Option{i}.Note"), &o.note, 13.0, 400, "Muted");
                    call(&mut n, "maxwidth", &[(w - end - 12.0).max(40.0)]);
                    c.push(left(n, end, mid));
                }
            }
            OptionKind::Actions(buttons) => {
                // Buttons from the row's right end, what it says left of
                // them.
                let mut bx = w - 10.0;
                let mut placed = Vec::new();
                for (j, b) in buttons.iter().enumerate().rev() {
                    let bw = look::mono_width(b, 14.0) + 24.0;
                    let mut a = look::component(&*env.look, "Button", &Spec::new([bw, 30.0], b));
                    a.name = format!("Option{i}.Button{j}");
                    call(&mut a, "xy", &[bx - bw / 2.0, mid]);
                    let lit = i == self.focus && j == o.index && !self.preview_focused;
                    crate::widgets::play(&mut a, if lit { "GainFocus" } else { "LoseFocus" });
                    a.finish_tweening();
                    placed.push(a);
                    bx -= bw + 8.0;
                }
                if !o.note.is_empty() {
                    let from = if o.label.is_empty() { 16.0 } else { x0 };
                    let mut n = env.text(&format!("Option{i}.Note"), &o.note, 13.0, 400, "Muted");
                    call(&mut n, "maxwidth", &[(bx - from - 4.0).max(40.0)]);
                    ellipsize(&mut n);
                    c.push(left(n, from, mid));
                }
                c.extend(placed);
            }
        }
        frame(&format!("Option{i}.Values"), c)
    }

    /// Scrolls the rows so the focused one shows (with its heading).
    fn reveal(&mut self) {
        let (tops, total) = self.row_tops();
        let h = self.places.rows[3];
        let max = (total - ROW_GAP - h).max(0.0);
        if let Some(&top) = tops.get(self.focus) {
            let above = if self.options()[self.focus].heading.is_some() {
                HEADING
            } else {
                0.0
            };
            if top - above < self.scroll {
                self.scroll = top - above;
            }
            if top + ROW > self.scroll + h {
                self.scroll = top + ROW - h;
            }
        }
        self.scroll = self.scroll.clamp(0.0, max);
    }

    fn scroll_by(&mut self, dy: f32, root: &mut Actor) {
        let (_, total) = self.row_tops();
        let max = (total - ROW_GAP - self.places.rows[3]).max(0.0);
        let to = (self.scroll + dy).clamp(0.0, max);
        if to != self.scroll {
            self.scroll = to;
            if let Some(r) = hit::find_mut(root, "Rows") {
                call(r, "y", &[-self.scroll]);
            }
            self.env.state.borrow_mut().scroll = self.scroll;
        }
    }

    fn refresh(&mut self, root: &mut Actor) {
        self.reveal();
        let fresh = self.rows_actor();
        if let Some(r) = hit::find_mut(root, "RowsClip") {
            *r = fresh;
        }
        let fresh = self.sections_actor();
        if let Some(r) = hit::find_mut(root, "Sections") {
            *r = fresh;
        }
        if let Some(fresh) = self.preview_selector()
            && let Some(selector) = hit::find_mut(root, "PreviewLayout")
        {
            *selector = fresh;
        }
        let (label, description) = self
            .selected_option()
            .map(|o| {
                let label = if o.label.is_empty() {
                    o.heading.clone().unwrap_or_default()
                } else {
                    o.label.clone()
                };
                (label, o.description.clone())
            })
            .unwrap_or_default();
        self.describe(root, &label, &description);
        if let Some(n) = hit::find_mut(root, "Saved") {
            let text = if self.data.notice.is_empty() {
                if self.places.compact {
                    ""
                } else {
                    "Saved as you change them"
                }
            } else {
                self.data.notice.as_str()
            };
            if let Kind::Text(t) = &mut n.kind {
                t.set_text(text);
            }
        }
        if let Some(p) = hit::find_mut(root, "Preview") {
            crate::widgets::set_text(p, "Title", &self.data.preview);
        }
        self.publish();
    }

    /// A description panel `w` wide holding the longest sentence (six
    /// lines at most).
    fn desc_height(&self, w: f32) -> f32 {
        let lines = self
            .data
            .sections
            .iter()
            .flat_map(|s| &s.options)
            .chain(self.data.preview_layout.iter())
            .map(|o| super::select_dialogs::wrap_lines(&o.description, w - 40.0, 13.0).len())
            .max()
            .unwrap_or(1)
            .clamp(1, 6);
        36.0 + lines as f32 * 17.0 + 10.0
    }

    /// What the focused option does: its name on top, then its sentence
    /// wrapped over the lines the panel has room for (the last one cut
    /// with "…" when it has not).
    fn describe(&self, root: &mut Actor, label: &str, description: &str) {
        let [_, _, dw, dh] = self.places.desc;
        let size = 13.0;
        let line = 17.0;
        let top = 36.0;
        let room = (((dh - top - 6.0) / line).floor() as usize).max(1);
        let mut lines = super::select_dialogs::wrap_lines(description, dw - 40.0, size);
        if lines.len() > room {
            lines.truncate(room);
            if let Some(l) = lines.last_mut() {
                l.push('…');
            }
        }
        let env = self.env.clone();
        let Some(d) = hit::find_mut(root, "Description") else {
            return;
        };
        crate::widgets::set_text(d, "Title", label);
        crate::widgets::set_text(d, "Note", "");
        if let Some(t) = part(d, "Title") {
            call(t, "y", &[20.0]);
        }
        if let Kind::Frame(c) = &mut d.kind {
            c.retain(|a| !a.name.starts_with("Description.Line"));
            for (i, l) in lines.iter().enumerate() {
                let mut t = env.text(&format!("Description.Line{i}"), l, size, 400, "Muted");
                call(&mut t, "maxwidth", &[dw - 40.0]);
                c.push(left(t, 20.0, top + i as f32 * line + line / 2.0));
            }
        }
    }

    /// New rows: the same section and option where they still are.
    fn set_data(&mut self, data: OptionsData, root: &mut Actor) {
        let section = self.data.sections.get(self.section).map(|s| s.name.clone());
        let id = self.options().get(self.focus).map(|o| o.id.clone());
        self.data = data;
        if let Some(i) = section.and_then(|n| self.data.sections.iter().position(|s| s.name == n)) {
            self.section = i;
        }
        self.section = self.section.min(self.data.sections.len().saturating_sub(1));
        let n = self.options().len();
        self.focus = id
            .and_then(|id| self.options().iter().position(|o| o.id == id))
            .unwrap_or(self.focus)
            .min(n.saturating_sub(1));
        self.refresh(root);
    }

    fn change(&mut self, by: i32, root: &mut Actor, cx: &mut Cx) {
        let section = self.section;
        let option = if self.preview_focused {
            self.data.preview_layout.as_mut()
        } else {
            self.data
                .sections
                .get_mut(section)
                .and_then(|s| s.options.get_mut(self.focus))
        };
        let Some(o) = option else {
            return;
        };
        let n = o.values().len() as i32;
        let to = (o.index as i32 + by).clamp(0, (n - 1).max(0)) as usize;
        if to == o.index {
            return;
        }
        o.index = to;
        let o = o.clone();
        if matches!(o.kind, OptionKind::Actions(_)) {
            // Moving between buttons changes nothing.
            self.refresh(root);
            return;
        }
        let name = if self.preview_focused {
            "Preview".to_string()
        } else {
            self.data.sections[section].name.clone()
        };
        self.env.state.borrow_mut().status = format!("{} {}", o.label, o.value());
        if let Some(f) = &self.changed {
            f(&name, &o);
        }
        let fresh = self.shell.clone().and_then(|s| {
            s(
                OptionsCall::Change {
                    id: o.id.clone(),
                    index: to,
                },
                cx,
            )
        });
        match fresh {
            Some(d) => self.set_data(d, root),
            None => self.refresh(root),
        }
    }

    /// Presses button `button` of row `i`.
    fn press(&mut self, i: usize, button: usize, root: &mut Actor, cx: &mut Cx) {
        let Some(o) = self.options().get(i) else {
            return;
        };
        if !matches!(&o.kind, OptionKind::Actions(b) if button < b.len()) {
            return;
        }
        let id = o.id.clone();
        self.env.state.borrow_mut().status = format!("{} {}", o.label, o.values()[button]);
        if let Some(d) = self
            .shell
            .clone()
            .and_then(|s| s(OptionsCall::Action { id, button }, cx))
        {
            self.set_data(d, root);
        }
    }

    fn set_section(&mut self, i: usize, root: &mut Actor) {
        if i >= self.data.sections.len() {
            return;
        }
        self.section = i;
        self.preview_focused = false;
        self.focus = 0;
        self.scroll = 0.0;
        self.tabs_follow = true;
        self.refresh(root);
    }

    fn leave(&mut self, cx: &mut Cx) {
        self.tell(None, cx);
        match self.shell.clone() {
            Some(s) => {
                s(OptionsCall::Leave, cx);
            }
            None => super::leave(&self.env, cx, true),
        }
    }

    /// The preview's place in the surface's pixels, when shown.
    fn preview_rect(&self, cx: &Cx) -> Option<[f32; 4]> {
        let [x, y, w, h] = self.places.preview?;
        let vp = cx.viewport();
        let z = self.places.zoom;
        let a = vp.to_physical([x * z, y * z]);
        let k = vp.scale * z;
        Some([a[0].round(), a[1].round(), (w * k).round(), (h * k).round()])
    }

    fn tell(&mut self, rect: Option<[f32; 4]>, cx: &mut Cx) {
        if self.told != Some(rect) {
            self.told = Some(rect);
            self.env.state.borrow_mut().preview = rect;
            cx.request(UiRequest::FieldPreview(rect));
        }
    }

    fn attach_texture(&mut self, root: &mut Actor, cx: &mut Cx) {
        let Some([_, _, w, h]) = self.places.preview else {
            return;
        };
        // As long as it is drawn on the virtual screen.
        self.texture = cx.texture(FIELD_PREVIEW, w.max(h) * self.places.zoom);
        if let Some(f) = hit::find_mut(root, "Preview.Field")
            && let Kind::Sprite(s) = &mut f.kind
        {
            s.texture = self.texture;
        }
    }

    fn publish(&self) {
        let mut st = self.env.state.borrow_mut();
        st.screen = "options".into();
        st.compact = self.places.compact;
        st.section = self
            .data
            .sections
            .get(self.section)
            .map(|s| s.name.clone())
            .unwrap_or_default();
        let o = self.selected_option();
        st.option = o.map(|o| o.label.clone()).unwrap_or_default();
        st.option_id = o.map(|o| o.id.clone()).unwrap_or_default();
        st.value = o.map(|o| o.value().to_string()).unwrap_or_default();
        st.option_rows = self.options().iter().map(|o| o.id.clone()).collect();
        st.notice = self.data.notice.clone();
        st.scroll = self.scroll;
    }

    /// The row and button a target is on (`Option3.Button1`).
    /// A pointer on a slider's track: pressed, it sets the value where it
    /// is and follows the drag (the rows do not scroll meanwhile); let go,
    /// it ends.
    fn slide(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Option<Handled> {
        let Input::Pointer {
            target,
            gesture,
            pos,
            ..
        } = input
        else {
            return None;
        };
        let on = target
            .as_ref()
            .and_then(Self::row_of)
            .filter(|(_, what)| matches!(*what, "Track" | "Knob" | "Fill"))
            .map(|(i, _)| i);
        let starts = matches!(gesture, Gesture::Press | Gesture::DragStart | Gesture::Tap);
        let row = match gesture {
            // A press on the track (a touch's may come only with its
            // drag or its tap).
            _ if starts && self.sliding.is_none() => {
                let i = on?;
                if !matches!(self.options().get(i)?.kind, OptionKind::Slider(_)) {
                    return None;
                }
                self.sliding = Some(i);
                self.preview_focused = false;
                if i != self.focus {
                    self.focus = i;
                    self.refresh(root);
                }
                cx.focus("RowsClip");
                i
            }
            Gesture::DragStart | Gesture::Drag { .. } | Gesture::Tap | Gesture::DragEnd { .. } => {
                self.sliding?
            }
            Gesture::Flick { .. } | Gesture::Cancel | Gesture::Release => {
                return self.sliding.take().map(|_| Handled::Yes);
            }
            _ => return None,
        };
        let name = format!("Option{row}.Track");
        let w = hit::find(root, &name)
            .and_then(|p| hit::actor_at(root, &p).map(|a| a.size()[0] - 16.0))
            .unwrap_or(1.0)
            .max(1.0);
        if let Some([x, _]) = hit::local_point(root, &hit::IDENTITY, &name, *pos) {
            let f = ((x + w / 2.0) / w).clamp(0.0, 1.0);
            let (count, at) = self
                .options()
                .get(row)
                .map_or((1, 0), |o| (o.values().len(), o.index));
            let to = (f * (count.saturating_sub(1)) as f32).round() as i32;
            self.change(to - at as i32, root, cx);
        }
        if matches!(gesture, Gesture::Tap | Gesture::DragEnd { .. }) {
            self.sliding = None;
        }
        Some(Handled::Yes)
    }

    fn row_of(t: &hit::Target) -> Option<(usize, &str)> {
        t.chain.iter().find_map(|name| {
            let rest = name.strip_prefix("Option")?;
            let (i, what) = rest.split_once('.').unwrap_or((rest, ""));
            Some((i.parse().ok()?, what))
        })
    }
}

impl Screen for OptionsScreen {
    fn name(&self) -> &str {
        "options"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        self.env.state.borrow_mut().over += 1;
        self.place(cx.viewport().screen);
        let mut root = self.build_tree();
        self.attach_texture(&mut root, cx);
        self.refresh(&mut root);
        self.told = None;
        root
    }

    fn focusables(&self) -> Vec<Focusable> {
        let mut v = vec![Focusable::new("RowsClip")];
        if self.data.preview_layout.is_some() {
            v.push(Focusable::new("PreviewLayout"));
        }
        if self.places.compact {
            v.push(Focusable::new("Back"));
        } else {
            v.push(Focusable::new("Link.Back"));
        }
        v.extend((0..self.data.sections.len()).map(|i| Focusable::new(format!("Section{i}"))));
        v
    }

    fn update(&mut self, _dt: f32, root: &mut Actor, cx: &mut Cx) {
        let vp = cx.viewport().screen;
        let before = self.places;
        self.place(vp);
        if self.places != before {
            *root = self.build_tree();
            self.attach_texture(root, cx);
            self.refresh(root);
            root.finish_tweening();
        }
        if self.env.state.borrow().screen != "options" {
            self.publish();
        }
        let rect = self.preview_rect(cx);
        self.tell(rect, cx);
    }

    fn message(&mut self, m: &Message, root: &mut Actor, _cx: &mut Cx) {
        if m.name == OPTIONS_CHANGED
            && let Some(d) = self.feed.as_ref().and_then(|f| f.borrow_mut().take())
        {
            self.set_data(d, root);
        }
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        let n = self.options().len();
        let on_rows = cx.focused().is_none_or(|f| f == "RowsClip");
        let on_preview = cx.focused() == Some("PreviewLayout");
        if let Some(h) = self.slide(input, root, cx) {
            return h;
        }
        match input {
            Input::Menu { action, repeat } => match action {
                MenuAction::Up | MenuAction::Down if on_preview => {
                    self.preview_focused = false;
                    cx.focus("RowsClip");
                    self.refresh(root);
                    Handled::Yes
                }
                // A fresh Up on the first row goes on up: on a phone to
                // the sections' tabs above, otherwise (and from the top
                // bar) past the top, to the app's notices.
                MenuAction::Up if on_rows && self.focus == 0 && !repeat => {
                    if !self.places.compact {
                        cx.enter_chrome();
                        return Handled::Yes;
                    }
                    Handled::No
                }
                MenuAction::Up
                    if !repeat
                        && cx.focused().is_some_and(|f| {
                            matches!(f, "Back" | "Link.Back") || f.starts_with("Section")
                        }) =>
                {
                    cx.enter_chrome();
                    Handled::Yes
                }
                MenuAction::Up if on_rows => {
                    self.focus = self.focus.saturating_sub(1);
                    self.refresh(root);
                    Handled::Yes
                }
                MenuAction::Down
                    if on_rows
                        && self.focus + 1 >= n
                        && !repeat
                        && self.data.preview_layout.is_some() =>
                {
                    self.preview_focused = true;
                    cx.focus("PreviewLayout");
                    self.refresh(root);
                    Handled::Yes
                }
                MenuAction::Down if on_rows => {
                    self.focus = (self.focus + 1).min(n.saturating_sub(1));
                    self.refresh(root);
                    Handled::Yes
                }
                MenuAction::Left if on_rows || on_preview => {
                    self.change(-1, root, cx);
                    Handled::Yes
                }
                MenuAction::Right if on_rows || on_preview => {
                    self.change(1, root, cx);
                    Handled::Yes
                }
                MenuAction::Select => {
                    let next = (self.section + 1) % self.data.sections.len().max(1);
                    self.set_section(next, root);
                    cx.focus("RowsClip");
                    Handled::Yes
                }
                MenuAction::Back => {
                    self.leave(cx);
                    Handled::Yes
                }
                MenuAction::Start if on_preview => {
                    self.change(1, root, cx);
                    Handled::Yes
                }
                MenuAction::Start if on_rows => {
                    match self.options().get(self.focus).map(|o| (&o.kind, o.index)) {
                        Some((OptionKind::Actions(_), b)) => self.press(self.focus, b, root, cx),
                        _ => self.change(1, root, cx),
                    }
                    Handled::Yes
                }
                _ => Handled::No,
            },
            Input::Wheel {
                target: Some(t),
                delta,
                ..
            } if t.within("RowsClip") => {
                self.scroll_by(delta[1] / self.places.zoom, root);
                Handled::Yes
            }
            Input::Wheel {
                target: Some(t),
                delta,
                ..
            } if t.within("Sections") && self.places.compact => {
                let d = if delta[0] != 0.0 { delta[0] } else { delta[1] };
                self.tabs_scroll += d / self.places.zoom;
                self.tabs_follow = false;
                let fresh = self.sections_actor();
                if let Some(r) = hit::find_mut(root, "Sections") {
                    *r = fresh;
                }
                Handled::Yes
            }
            Input::Pointer {
                target: Some(t),
                gesture: Gesture::Drag { delta },
                ..
            } if t.within("RowsClip") => {
                self.scroll_by(-delta[1] / self.places.zoom, root);
                Handled::Yes
            }
            Input::Pointer {
                target: Some(t),
                gesture: Gesture::Drag { delta },
                ..
            } if t.within("Sections") && self.places.compact => {
                self.tabs_scroll -= delta[0] / self.places.zoom;
                self.tabs_follow = false;
                let fresh = self.sections_actor();
                if let Some(r) = hit::find_mut(root, "Sections") {
                    *r = fresh;
                }
                Handled::Yes
            }
            Input::Pointer {
                target: Some(t),
                gesture: Gesture::Tap,
                ..
            } if t.within("PreviewLayout") => {
                self.preview_focused = true;
                cx.focus("PreviewLayout");
                self.change(
                    if t.within("PreviewLayout.Down") {
                        -1
                    } else {
                        1
                    },
                    root,
                    cx,
                );
                self.refresh(root);
                Handled::Yes
            }
            Input::Pointer {
                target: Some(t),
                gesture: Gesture::Tap,
                ..
            } => {
                // A choice, a stepper's arrow, a button, or a row.
                let Some((i, what)) = Self::row_of(t) else {
                    return Handled::No;
                };
                let what = what.to_string();
                if i >= n {
                    return Handled::No;
                }
                self.preview_focused = false;
                if i != self.focus {
                    self.focus = i;
                    self.refresh(root);
                }
                cx.focus("RowsClip");
                if let Some(j) = what
                    .strip_prefix("Choice")
                    .and_then(|j| j.parse::<i32>().ok())
                {
                    let at = self.options().get(i).map_or(0, |o| o.index as i32);
                    self.change(j - at, root, cx);
                } else if let Some(j) = what
                    .strip_prefix("Button")
                    .and_then(|j| j.parse::<usize>().ok())
                {
                    let at = self.options().get(i).map_or(0, |o| o.index);
                    if j != at
                        && let Some(o) = self
                            .data
                            .sections
                            .get_mut(self.section)
                            .and_then(|s| s.options.get_mut(i))
                    {
                        o.index = j;
                        self.refresh(root);
                    }
                    self.press(i, j, root, cx);
                } else if what == "Down" {
                    self.change(-1, root, cx);
                } else if what == "Up" {
                    self.change(1, root, cx);
                }
                Handled::Yes
            }
            Input::Activate { name } => {
                if let Some(i) = name.strip_prefix("Section").and_then(|n| n.parse().ok()) {
                    self.set_section(i, root);
                    // On to its rows, as Tab does.
                    cx.focus("RowsClip");
                } else if name == "Link.Back" || name == "Back" {
                    self.leave(cx);
                } else if name == "PreviewLayout" {
                    self.preview_focused = true;
                    self.change(1, root, cx);
                } else if name == "RowsClip" {
                    return self.input(
                        &Input::Menu {
                            action: MenuAction::Start,
                            repeat: false,
                        },
                        root,
                        cx,
                    );
                }
                Handled::Yes
            }
            Input::Key(k)
                if k.down
                    && !k.repeat
                    && !k.composing
                    && !k.modifiers.any_but_shift()
                    && k.key.eq_ignore_ascii_case("p")
                    && self.data.preview_layout.is_some() =>
            {
                self.preview_focused = !on_preview;
                cx.focus(if self.preview_focused {
                    "PreviewLayout"
                } else {
                    "RowsClip"
                });
                self.refresh(root);
                Handled::Yes
            }
            // The rows' frame took or lost focus: its commands reached
            // every button in it; only the lit one shows so.
            Input::Focus { from, to }
                if [from.as_deref(), to.as_deref()]
                    .iter()
                    .any(|f| matches!(f, Some("RowsClip" | "PreviewLayout"))) =>
            {
                self.preview_focused = to.as_deref() == Some("PreviewLayout");
                self.refresh(root);
                Handled::No
            }
            _ => Handled::No,
        }
    }
}

fn ellipsize(a: &mut Actor) {
    if let Kind::Text(t) = &mut a.kind {
        t.ellipsis = true;
    }
}
