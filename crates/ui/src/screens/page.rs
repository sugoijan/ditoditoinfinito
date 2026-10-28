//! A page to read (`docs/plans/ui-core.md`, step 6.4): the credits and
//! calibration's menu. Headings, paragraphs, links, buttons and folded
//! sections ([`Block`]) in one column that scrolls, under the theme's
//! header (a back icon and the title on a compact screen).
//!
//! - **Keys**: Up and Down go from one link or button to the next,
//!   scrolling it into view (and scroll where there is none), Page Up and
//!   Page Down a page, Start opens the link (in the gesture), presses the
//!   button or unfolds the section, Back leaves.
//! - **Pointers**: a tap on a link, button or fold; a wheel or a drag
//!   scrolls.

use ddi_engine::MenuInput as HintAction;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use ddi_actor::{Actor, Kind};

use super::select_dialogs::wrap_lines;
use super::{Env, footer, frame, ground, header, left};
use crate::gesture::Gesture;
use crate::hit;
use crate::look::{self, Spec, call, quad};
use crate::message::Message;
use crate::nav::{Focusable, MenuAction};
use crate::request::UiRequest;
use crate::screen::{Cx, Handled, Input, Screen};

/// Broadcast when the shell put a new page in the [`PageFeed`].
pub const PAGE_CHANGED: &str = "Page.Changed";

/// What a page holds.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    /// A section's title.
    Heading(String),
    /// An entry's title.
    Subheading(String),
    Text(String),
    Muted(String),
    /// Text kept as it is, line by line (flags, notices).
    Mono(String),
    /// Opens `url` (in the gesture).
    Link {
        label: String,
        url: String,
    },
    /// Asks the shell ([`PageCall::Button`]).
    Button {
        id: String,
        label: String,
        primary: bool,
    },
    /// A folded section: its label, and what it shows unfolded.
    Fold {
        id: String,
        label: String,
        body: Vec<Block>,
    },
    Gap,
}

/// A page: its title (the header's) and its blocks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageDoc {
    pub title: String,
    pub blocks: Vec<Block>,
}

/// What a page asks of its shell.
#[derive(Clone, Debug, PartialEq)]
pub enum PageCall {
    Button(String),
    Leave,
}

/// Carries a call out, in the input's handler; the page as it is now,
/// when it changed.
pub type PageShell = Rc<dyn Fn(PageCall, &mut Cx) -> Option<PageDoc>>;

/// A new page from the shell (loaded later), with [`PAGE_CHANGED`].
pub type PageFeed = Rc<RefCell<Option<PageDoc>>>;

/// One drawn line of the column.
#[derive(Clone, Debug)]
struct Row {
    y: f32,
    h: f32,
    kind: RowKind,
}

#[derive(Clone, Debug)]
enum RowKind {
    Text {
        text: String,
        size: f32,
        weight: u16,
        color: &'static str,
        indent: f32,
    },
    /// Part of interactive item `item`.
    Item {
        item: usize,
        text: String,
        indent: f32,
    },
    Button {
        item: usize,
        label: String,
        primary: bool,
        indent: f32,
    },
}

/// What a focusable item does.
#[derive(Clone, Debug, PartialEq)]
enum Item {
    Link(String),
    Button(String),
    Fold(String),
}

pub struct PageScreen {
    /// The header's wordmark's clock.
    live: super::Live,
    env: Env,
    name: String,
    doc: PageDoc,
    shell: Option<PageShell>,
    feed: Option<PageFeed>,
    open: HashSet<String>,
    /// The layout's size, its zoom, compact.
    screen: [f32; 2],
    zoom: f32,
    compact: bool,
    column: [f32; 4],
    rows: Vec<Row>,
    items: Vec<Item>,
    focus: Option<usize>,
    /// The link a controller pressed and the browser would not open (it
    /// opens one only from a key or a tap): its row says so.
    gesture_hint: Option<usize>,
    scroll: f32,
    /// Dragging the scrollbar's thumb: where it was grabbed (from its
    /// top).
    bar: Option<f32>,
}

const LINE: f32 = 6.0;
/// The scrollbar's width, and its gap from the column.
const BAR: f32 = 6.0;
const BAR_GAP: f32 = 14.0;

impl PageScreen {
    pub fn new(env: Env, name: &str, doc: PageDoc) -> PageScreen {
        PageScreen {
            live: Default::default(),
            env,
            name: name.into(),
            doc,
            shell: None,
            feed: None,
            open: HashSet::new(),
            screen: super::DESIGN,
            zoom: 1.0,
            compact: false,
            column: [0.0; 4],
            rows: Vec::new(),
            items: Vec::new(),
            focus: None,
            gesture_hint: None,
            scroll: 0.0,
            bar: None,
        }
    }

    pub fn with_shell(mut self, shell: PageShell, feed: PageFeed) -> PageScreen {
        self.shell = Some(shell);
        self.feed = Some(feed);
        self
    }

    /// As options ([`super::compact_fit`]).
    fn place(&mut self, vp: ddi_actor::screen::VirtualScreen) -> bool {
        let before = (self.screen, self.zoom);
        let (screen, zoom, compact) = super::compact_fit(&self.env, vp);
        self.screen = screen;
        self.zoom = zoom;
        self.compact = compact;
        let [w, h] = screen;
        self.column = if compact {
            [12.0, 60.0, w - 24.0, h - 72.0]
        } else {
            let cw = 880.0_f32.min(w - 64.0);
            let top = 88.0;
            let bottom = self.env.metric("Footer.Height", 40.0) + 12.0;
            [(w - cw) / 2.0, top, cw, h - top - bottom]
        };
        before != (self.screen, self.zoom)
    }

    /// The blocks as rows at the column's width, and the items.
    fn lay_out(&mut self) {
        let mut rows = Vec::new();
        let mut items = Vec::new();
        let mut y = 0.0;
        let w = self.column[2];
        let blocks = self.doc.blocks.clone();
        self.lay_blocks(&blocks, 0.0, w, &mut y, &mut rows, &mut items);
        self.rows = rows;
        self.items = items;
        if self.focus.is_some_and(|f| f >= self.items.len()) {
            self.focus = None;
        }
    }

    fn lay_blocks(
        &self,
        blocks: &[Block],
        indent: f32,
        w: f32,
        y: &mut f32,
        rows: &mut Vec<Row>,
        items: &mut Vec<Item>,
    ) {
        let width = w - indent;
        let text = |rows: &mut Vec<Row>,
                    y: &mut f32,
                    s: &str,
                    size: f32,
                    weight: u16,
                    color: &'static str| {
            for l in wrap_lines(s, width, size) {
                let h = size + LINE;
                rows.push(Row {
                    y: *y,
                    h,
                    kind: RowKind::Text {
                        text: l,
                        size,
                        weight,
                        color,
                        indent,
                    },
                });
                *y += h;
            }
        };
        for b in blocks {
            match b {
                Block::Heading(s) => {
                    *y += 18.0;
                    text(rows, y, s, 20.0, 700, "Text");
                    *y += 6.0;
                }
                Block::Subheading(s) => {
                    *y += 10.0;
                    text(rows, y, s, 16.0, 700, "Text");
                    *y += 2.0;
                }
                Block::Text(s) => {
                    text(rows, y, s, 14.0, 400, "Text");
                    *y += 6.0;
                }
                Block::Muted(s) => {
                    text(rows, y, s, 13.0, 400, "Muted");
                    *y += 4.0;
                }
                Block::Mono(s) => {
                    for l in s.lines() {
                        let l = if l.is_empty() { " " } else { l };
                        text(rows, y, l, 12.0, 400, "Muted");
                    }
                    *y += 6.0;
                }
                Block::Gap => *y += 12.0,
                Block::Link { label, url } => {
                    let item = items.len();
                    items.push(Item::Link(url.clone()));
                    for l in wrap_lines(label, width, 14.0) {
                        rows.push(Row {
                            y: *y,
                            h: 14.0 + LINE + 4.0,
                            kind: RowKind::Item {
                                item,
                                text: l,
                                indent,
                            },
                        });
                        *y += 14.0 + LINE + 4.0;
                    }
                    *y += 2.0;
                }
                Block::Button { id, label, primary } => {
                    let item = items.len();
                    items.push(Item::Button(id.clone()));
                    *y += 4.0;
                    rows.push(Row {
                        y: *y,
                        h: 40.0,
                        kind: RowKind::Button {
                            item,
                            label: label.clone(),
                            primary: *primary,
                            indent,
                        },
                    });
                    *y += 48.0;
                }
                Block::Fold { id, label, body } => {
                    let item = items.len();
                    items.push(Item::Fold(id.clone()));
                    let open = self.open.contains(id);
                    rows.push(Row {
                        y: *y,
                        h: 24.0,
                        kind: RowKind::Item {
                            item,
                            text: format!("{} {label}", if open { "▾" } else { "▸" }),
                            indent,
                        },
                    });
                    *y += 26.0;
                    if open {
                        self.lay_blocks(body, indent + 16.0, w, y, rows, items);
                    }
                }
            }
        }
    }

    fn total(&self) -> f32 {
        self.rows.last().map_or(0.0, |r| r.y + r.h) + 24.0
    }

    fn max_scroll(&self) -> f32 {
        (self.total() - self.column[3]).max(0.0)
    }

    fn tree(&mut self, cx: &mut Cx) -> Actor {
        let env = self.env.clone();
        let [w, h] = self.screen;
        let mut children = vec![ground(&env, self.screen)];
        children.push(self.column_actor());
        children.push(self.bar_actor());
        if self.compact {
            children.push(super::back_button(&env, [40.0, 40.0], [32.0, 28.0]));
            children.push(left(
                env.text("Title", &self.doc.title, 16.0, 700, "Text"),
                64.0,
                28.0,
            ));
        } else {
            children.push(header(&env, cx, w, &self.doc.title.to_uppercase()));
            let (f, texts) = footer(
                &env,
                [w, h],
                &[
                    (&["↑", "↓"], "Move"),
                    (&["ENTER"], "Open"),
                    (&["ESC"], "Back"),
                ],
                &[
                    (&[HintAction::Up, HintAction::Down], "Move"),
                    (&[HintAction::Confirm, HintAction::Start], "Activate"),
                    (&[HintAction::Back], "Back"),
                ],
                "",
                &[("Link.Back", "Back")],
                cx,
            );
            env.state.borrow_mut().hints = texts;
            children.push(f);
        }
        let mut root = Actor::new(Kind::Frame(children));
        // The header's row from above, the footer from below, the column
        // and its bar from the right.
        {
            use super::Edge::{Bottom, Right, Top};
            super::transitions(
                &mut root,
                &env,
                &[
                    ("Header", Top),
                    ("Back", Top),
                    ("Title", Top),
                    ("Footer", Bottom),
                    ("Column", Right),
                    ("Column.Bar", Right),
                ],
            );
        }
        if self.zoom != 1.0 {
            call(&mut root, "zoom", &[self.zoom]);
        }
        root
    }

    fn column_actor(&self) -> Actor {
        let env = &self.env;
        let [x, y, w, h] = self.column;
        let mut c = Vec::new();
        // What is in view, and a little around.
        let from = self.scroll - 200.0;
        let to = self.scroll + h + 200.0;
        let mut item_rows: Vec<(usize, [f32; 4])> = Vec::new();
        for r in &self.rows {
            if r.y + r.h < from || r.y > to {
                continue;
            }
            match &r.kind {
                RowKind::Text {
                    text,
                    size,
                    weight,
                    color,
                    indent,
                } => {
                    let mut t = env.text("", text, *size, *weight, color);
                    call(&mut t, "maxwidth", &[w - indent]);
                    c.push(left(t, *indent, r.y + r.h / 2.0));
                }
                RowKind::Item { item, text, indent } => {
                    let focused = self.focus == Some(*item);
                    let text = if self.gesture_hint == Some(*item) {
                        format!("{text} · press Enter or tap to open")
                    } else {
                        text.clone()
                    };
                    let mut t =
                        env.text("", &text, 14.0, if focused { 700 } else { 500 }, "Accent");
                    call(&mut t, "maxwidth", &[w - indent]);
                    c.push(left(t, *indent + 6.0, r.y + r.h / 2.0));
                    item_rows.push((*item, [*indent, r.y, w - indent, r.h]));
                }
                RowKind::Button {
                    item,
                    label,
                    primary,
                    indent,
                } => {
                    // The primary button's label is larger.
                    let size = if *primary { 18.0 } else { 15.0 };
                    let bw = (look::mono_width(label, size) + 40.0).min(w - indent);
                    let mut b = look::component_or(
                        &*env.look,
                        if *primary { "PlayButton" } else { "Button" },
                        "Button",
                        &Spec::new([bw, 40.0], label),
                    );
                    b.name = format!("Item{item}");
                    call(&mut b, "xy", &[indent + bw / 2.0, r.y + 20.0]);
                    crate::widgets::play(
                        &mut b,
                        if self.focus == Some(*item) {
                            "GainFocus"
                        } else {
                            "LoseFocus"
                        },
                    );
                    b.finish_tweening();
                    c.push(b);
                }
            }
        }
        // A link's or a fold's lines: one hit area, lit when focused.
        let mut areas: Vec<(usize, [f32; 4])> = Vec::new();
        for (item, [ix, iy, iw, ih]) in item_rows {
            match areas.last_mut() {
                Some((i, a)) if *i == item => a[3] = iy + ih - a[1],
                _ => areas.push((item, [ix, iy, iw, ih])),
            }
        }
        for (item, [ix, iy, iw, ih]) in areas {
            // Lit under the text when focused; the hit area over it.
            if self.focus == Some(item) {
                let mut lit = quad("", [iw, ih], env.color("Selected"));
                call(&mut lit, "cornerradius", &[4.0]);
                call(&mut lit, "xy", &[ix + iw / 2.0, iy + ih / 2.0]);
                c.insert(0, lit);
            }
            let mut hit = quad(&format!("Item{item}"), [iw, ih], [0.0, 0.0, 0.0, 0.004]);
            call(&mut hit, "cornerradius", &[4.0]);
            call(&mut hit, "xy", &[ix + iw / 2.0, iy + ih / 2.0]);
            c.push(hit);
        }
        let mut content = frame("Content", c);
        call(&mut content, "y", &[-self.scroll]);
        let mut clip = frame("Column", vec![content]);
        clip.clip = Some([-4.0, 0.0, w + 4.0, h]);
        call(&mut clip, "xy", &[x, y]);
        clip
    }

    fn rebuild_column(&mut self, root: &mut Actor) {
        let fresh = self.column_actor();
        if let Some(c) = hit::find_mut(root, "Column") {
            super::refill(c, fresh);
        }
        if let Some(bar) = hit::find_mut(root, "Column.Bar") {
            self.place_thumb(bar);
        }
        self.publish();
    }

    /// The scrollbar beside the column (`Column.Bar`): a press on its
    /// track jumps there, its thumb drags.
    fn bar_actor(&self) -> Actor {
        let [x, y, w, h] = self.column;
        let mut bar = look::component_or(
            &*self.env.look,
            "Page.Bar",
            "List.Bar",
            &Spec::new([BAR, h], ""),
        );
        bar.name = "Column.Bar".into();
        call(&mut bar, "xy", &[x + w + BAR_GAP, y]);
        self.place_thumb(&mut bar);
        bar
    }

    /// The thumb's length: the share shown, at least 24 (or the whole
    /// bar when shorter).
    fn thumb_size(&self) -> f32 {
        let h = self.column[3];
        (h * h / self.total().max(1.0)).clamp(24.0_f32.min(h), h)
    }

    /// The bar shows while there is more than fits, its thumb where the
    /// scroll is.
    fn place_thumb(&self, bar: &mut Actor) {
        let range = self.max_scroll();
        bar.visible = range > 0.0;
        if !bar.visible {
            return;
        }
        let h = self.column[3];
        let thumb = self.thumb_size();
        let y = (self.scroll / range * (h - thumb)).clamp(0.0, h - thumb);
        if let Some(t) = hit::find_mut(bar, "Thumb") {
            if let Kind::Sprite(s) = &mut t.kind {
                s.size = [BAR, thumb];
            }
            call(t, "y", &[y]);
        }
    }

    /// A gesture on the scrollbar at `y` from its top.
    fn bar(&mut self, gesture: Gesture, y: f32, root: &mut Actor) {
        let range = self.max_scroll();
        if range <= 0.0 {
            return;
        }
        let h = self.column[3];
        let thumb = self.thumb_size();
        let thumb_y = self.scroll / range * (h - thumb);
        let to =
            |grab: f32| (y - grab).clamp(0.0, (h - thumb).max(0.0)) / (h - thumb).max(1.0) * range;
        match gesture {
            Gesture::Press => {
                let grab = if (thumb_y..thumb_y + thumb).contains(&y) {
                    y - thumb_y
                } else {
                    thumb / 2.0
                };
                self.bar = Some(grab);
                self.scroll_to(to(grab), root);
            }
            Gesture::Drag { .. } | Gesture::DragStart => {
                if let Some(grab) = self.bar {
                    self.scroll_to(to(grab), root);
                }
            }
            _ => self.bar = None,
        }
    }

    fn scroll_to(&mut self, to: f32, root: &mut Actor) {
        let to = to.clamp(0.0, self.max_scroll());
        if to != self.scroll {
            self.scroll = to;
            self.rebuild_column(root);
        }
    }

    /// Scrolls item `i` into view.
    fn reveal(&mut self, i: usize) {
        let (mut top, mut bottom) = (f32::MAX, 0.0_f32);
        for r in &self.rows {
            let item = match &r.kind {
                RowKind::Item { item, .. } | RowKind::Button { item, .. } => *item,
                RowKind::Text { .. } => continue,
            };
            if item == i {
                top = top.min(r.y);
                bottom = bottom.max(r.y + r.h);
            }
        }
        if top == f32::MAX {
            return;
        }
        let h = self.column[3];
        if top - 24.0 < self.scroll {
            self.scroll = top - 24.0;
        }
        if bottom + 24.0 > self.scroll + h {
            self.scroll = bottom + 24.0 - h;
        }
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
    }

    /// Where item `i` starts.
    fn item_y(&self, i: usize) -> Option<f32> {
        self.rows.iter().find_map(|r| match &r.kind {
            RowKind::Item { item, .. } | RowKind::Button { item, .. } if *item == i => Some(r.y),
            _ => None,
        })
    }

    /// Up or Down: the next item, while it is no further than a page
    /// away; else a line of scrolling.
    fn step(&mut self, down: bool, root: &mut Actor) {
        self.gesture_hint = None;
        let h = self.column[3];
        let next = match (self.focus, down) {
            (None, true) => (!self.items.is_empty()).then_some(0),
            (None, false) => None,
            (Some(f), true) => (f + 1 < self.items.len()).then_some(f + 1),
            (Some(f), false) => f.checked_sub(1),
        };
        let reachable = next.and_then(|n| {
            let y = self.item_y(n)?;
            let near = if down {
                y <= self.scroll + h * 1.5
            } else {
                y >= self.scroll - h * 0.5
            };
            near.then_some(n)
        });
        match reachable {
            Some(n) => {
                self.focus = Some(n);
                self.reveal(n);
                self.rebuild_column(root);
            }
            None => {
                let by = if down { 80.0 } else { -80.0 };
                self.scroll_to(self.scroll + by, root);
            }
        }
    }

    fn activate(&mut self, i: usize, root: &mut Actor, cx: &mut Cx) {
        self.focus = Some(i);
        self.gesture_hint = None;
        match self.items.get(i).cloned() {
            Some(Item::Link(url)) => {
                cx.request(UiRequest::OpenUrl(url.clone()));
                self.env.state.borrow_mut().status = format!("open {url}");
            }
            Some(Item::Button(id)) => {
                self.env.state.borrow_mut().status = format!("button {id}");
                if let Some(doc) = self.shell.clone().and_then(|s| s(PageCall::Button(id), cx)) {
                    self.set_doc(doc, root, cx);
                    return;
                }
            }
            Some(Item::Fold(id)) => {
                if !self.open.remove(&id) {
                    self.open.insert(id);
                }
                self.lay_out();
            }
            None => {}
        }
        self.rebuild_column(root);
    }

    fn set_doc(&mut self, doc: PageDoc, root: &mut Actor, _cx: &mut Cx) {
        let title_changed = doc.title != self.doc.title;
        self.doc = doc;
        self.lay_out();
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
        // The title in place and the column's contents: the parts keep
        // their motion (the credits arrive as the screen comes in).
        if title_changed {
            let title = if self.compact {
                self.doc.title.clone()
            } else {
                self.doc.title.to_uppercase()
            };
            crate::widgets::set_text(root, "Title", &title);
        }
        self.rebuild_column(root);
        self.publish();
    }

    fn leave(&mut self, cx: &mut Cx) {
        match self.shell.clone() {
            Some(s) => {
                s(PageCall::Leave, cx);
            }
            None => super::leave(&self.env, cx, true),
        }
    }

    fn publish(&self) {
        let mut st = self.env.state.borrow_mut();
        st.screen = self.name.clone();
        st.compact = self.compact;
        st.scroll = self.scroll;
        st.page_items = self
            .items
            .iter()
            .map(|i| match i {
                Item::Link(u) => format!("link:{u}"),
                Item::Button(b) => format!("button:{b}"),
                Item::Fold(f) => format!("fold:{f}"),
            })
            .collect();
        st.page_focus = self.focus.map(|f| f as i32).unwrap_or(-1);
        st.page_height = self.total();
    }

    fn item_of(t: &hit::Target) -> Option<usize> {
        t.chain
            .iter()
            .find_map(|n| n.strip_prefix("Item")?.parse().ok())
    }
}

impl Screen for PageScreen {
    fn active_element(&self, focused: Option<&str>) -> Option<String> {
        if focused == Some("Column") {
            self.focus.map(|i| format!("Item{i}"))
        } else if focused == Some("Back") && !self.compact {
            Some("Link.Back".into())
        } else {
            focused.map(str::to_owned)
        }
    }

    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    /// A pointer over the column's items.
    fn cursor(&self, target: &hit::Target) -> Option<crate::request::Cursor> {
        Self::item_of(target).map(|_| crate::request::Cursor::Pointer)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        self.place(cx.viewport().screen);
        self.lay_out();
        let root = self.tree(cx);
        self.publish();
        root
    }

    fn animating(&self) -> bool {
        self.live.animating(&self.env)
    }
    fn focusables(&self) -> Vec<Focusable> {
        let mut v = vec![Focusable::new_list("Column"), Focusable::new("Back")];
        if !self.compact {
            v.push(Focusable::new("Link.Back"));
        }
        v
    }

    fn update(&mut self, _dt: f32, root: &mut Actor, cx: &mut Cx) {
        if !cx.is_current_screen() {
            return;
        }
        self.live.tick(root, "Wordmark", &self.env, cx);
        if self.place(cx.viewport().screen) {
            self.lay_out();
            self.scroll = self.scroll.clamp(0.0, self.max_scroll());
            let left = super::time_left(root);
            *root = self.tree(cx);
            super::relaid(root, left, cx);
            self.publish();
        }
        if self.env.state.borrow().screen != self.name {
            self.publish();
        }
    }

    fn message(&mut self, m: &Message, root: &mut Actor, cx: &mut Cx) {
        if m.name == crate::controller_hints::CHANGED {
            let left = super::time_left(root);
            *root = self.tree(cx);
            super::relaid(root, left, cx);
            self.publish();
        }
        if m.name == PAGE_CHANGED
            && let Some(doc) = self.feed.as_ref().and_then(|f| f.borrow_mut().take())
        {
            self.set_doc(doc, root, cx);
        }
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        let on_column = cx.focused().is_none_or(|f| f == "Column");

        let page = self.column[3] * 0.85;
        // A link a controller pressed: the browser opens one only from a
        // key or a tap; its row says so.
        if let Input::NeedsGesture(UiRequest::OpenUrl(_)) = input {
            self.gesture_hint = self.focus;
            self.env.state.borrow_mut().status = "press Enter or tap to open".into();
            self.rebuild_column(root);
            return Handled::Yes;
        }
        match input {
            Input::Menu { action, repeat } => match action {
                // A fresh Up at the very top goes on up (the app's
                // notices).
                MenuAction::Up
                    if on_column
                        && !repeat
                        && self.scroll == 0.0
                        && self.focus.is_none_or(|f| f == 0) =>
                {
                    self.focus = None;
                    self.rebuild_column(root);
                    cx.enter_chrome();
                    Handled::Yes
                }
                MenuAction::Up if !repeat && !on_column => {
                    cx.enter_chrome();
                    Handled::Yes
                }
                MenuAction::Up if on_column => {
                    self.step(false, root);
                    Handled::Yes
                }
                MenuAction::Down if on_column => {
                    self.step(true, root);
                    Handled::Yes
                }
                MenuAction::Start if on_column => {
                    if let Some(f) = self.focus {
                        self.activate(f, root, cx);
                    }
                    Handled::Yes
                }
                MenuAction::Back => {
                    self.leave(cx);
                    Handled::Yes
                }
                _ => Handled::No,
            },
            Input::Key(k) if k.down => match k.code.as_str() {
                "PageDown" => {
                    self.scroll_to(self.scroll + page, root);
                    Handled::Yes
                }
                "PageUp" => {
                    self.scroll_to(self.scroll - page, root);
                    Handled::Yes
                }
                "Home" => {
                    self.scroll_to(0.0, root);
                    Handled::Yes
                }
                "End" => {
                    self.scroll_to(f32::MAX, root);
                    Handled::Yes
                }
                _ => Handled::No,
            },
            Input::Wheel {
                target: Some(t),
                delta,
                ..
            } if t.within("Column") => {
                self.scroll_to(self.scroll + delta[1] / self.zoom, root);
                Handled::Yes
            }
            Input::Pointer {
                target: Some(t),
                gesture,
                pos,
                ..
            } if t.within("Column.Bar") => {
                let y = hit::local_point(root, &hit::IDENTITY, "Column.Bar", *pos)
                    .map_or(pos[1] - self.column[1], |p| p[1]);
                self.bar(*gesture, y / self.zoom, root);
                Handled::Yes
            }
            Input::Pointer {
                target: Some(t),
                gesture: Gesture::Drag { delta },
                ..
            } if t.within("Column") => {
                self.scroll_to(self.scroll - delta[1] / self.zoom, root);
                Handled::Yes
            }
            Input::Pointer {
                target: Some(t),
                gesture: Gesture::Tap,
                ..
            } => match Self::item_of(t) {
                Some(i) => {
                    cx.focus("Column");
                    self.activate(i, root, cx);
                    Handled::Yes
                }
                None => Handled::No,
            },
            Input::Activate { name } if name == "Link.Back" || name == "Back" => {
                self.leave(cx);
                Handled::Yes
            }
            Input::Activate { name } if name == "Column" => {
                if let Some(f) = self.focus {
                    self.activate(f, root, cx);
                }
                Handled::Yes
            }
            // The column took or lost focus: its commands reached every
            // button in it; only the focused one shows so.
            Input::Focus { from, to }
                if from.as_deref() == Some("Column") || to.as_deref() == Some("Column") =>
            {
                self.rebuild_column(root);
                Handled::No
            }
            _ => Handled::No,
        }
    }
}
