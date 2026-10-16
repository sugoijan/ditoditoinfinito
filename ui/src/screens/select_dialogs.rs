//! Song select's dialogs: adding songs (the import: its pickers, options,
//! progress and report), managing what was imported (reading a pack's
//! folder again, removing a pack, a song or the songs that could not be
//! read, each confirmed; none of it while an import runs) and the notices
//! in full.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_actor::{Actor, Kind};

use super::data::{ImportView, Notice, PackEntry, Platform, PlayRow};
use super::select::NOTICE_LINK;
use super::select::{IMPORT_CHANGED, SelectCall, ShellFn};
use super::{Env, left};
use crate::hit;
use crate::look::{self, Spec, call, quad};
use crate::message::Message;
use crate::nav::{Focusable, MenuAction};
use crate::screen::{Cx, Handled, Input, Screen};
use crate::widgets::list::{ItemKind, List, ListConfig, ListModel, ListStyle};
use crate::widgets::{part, set_text};

/// The dimmed screen around a dialog, its panel and its title.
pub(crate) fn shell(
    env: &Env,
    screen: [f32; 2],
    size: [f32; 2],
    title: &str,
) -> (Vec<Actor>, [f32; 2]) {
    let [w, h] = size;
    let x = (screen[0] - w) / 2.0;
    let y = (screen[1] - h) / 2.0;
    let mut dim = quad(
        "Dim",
        [screen[0] * 4.0, screen[1] * 4.0],
        [0.0, 0.0, 0.0, 0.6],
    );
    call(&mut dim, "xy", &[screen[0] / 2.0, screen[1] / 2.0]);
    let panel = env.component("Dialog", "Panel", &Spec::new(size, ""), "Panel", [x, y]);
    let mut t = env.text("Title", title, 18.0, 700, "Text");
    t = left(t, x + 24.0, y + 30.0);
    (vec![dim, panel, t], [x, y])
}

pub(crate) fn button(
    env: &Env,
    name: &str,
    label: &str,
    centre: [f32; 2],
    w: f32,
    kind: &str,
) -> Actor {
    let mut b = look::component_or(&*env.look, kind, "Button", &Spec::new([w, 40.0], label));
    b.name = name.into();
    call(&mut b, "xy", &centre);
    b
}

/// `text` in lines no wider than `width` at `size` (monospace widths),
/// broken between words (a word too long for a line is cut).
pub fn wrap_lines(text: &str, width: f32, size: f32) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        let candidate = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if look::mono_width(&candidate, size) <= width || line.is_empty() {
            line = candidate;
        } else {
            out.push(std::mem::take(&mut line));
            line = word.to_string();
        }
        // A word longer than a line (a link, a hash): cut it into lines.
        while look::mono_width(&line, size) > width && line.chars().count() > 1 {
            let mut head = String::new();
            let mut rest = line.chars().peekable();
            while let Some(c) = rest.peek() {
                let next = format!("{head}{c}");
                if look::mono_width(&next, size) > width && !head.is_empty() {
                    break;
                }
                head = next;
                rest.next();
            }
            out.push(head);
            line = rest.collect();
        }
    }
    if !line.is_empty() || out.is_empty() {
        out.push(line);
    }
    out
}

/// Text no wider than `w`: cut with "…".
pub(crate) fn wrap(a: &mut Actor, w: f32) {
    if let Kind::Text(t) = &mut a.kind {
        t.max_width = w;
        t.ellipsis = true;
    }
}

pub(crate) fn dialog_root(children: Vec<Actor>) -> Actor {
    let mut root = Actor::new(Kind::Frame(children));
    look::command_of(&mut root, "On", "diffusealpha,0;linear,0.12;diffusealpha,1");
    look::command_of(&mut root, "Off", "linear,0.1;diffusealpha,0");
    look::command_of(&mut root, "Cancel", "linear,0.1;diffusealpha,0");
    root
}

/// What can be managed: the selected pack, or the selected song (and its
/// pack).
#[derive(Clone, Debug, PartialEq)]
pub enum Manage {
    Pack {
        pack: PackEntry,
        songs: usize,
    },
    Song {
        id: String,
        title: String,
        pack: PackEntry,
        songs: usize,
    },
    /// Stored songs that could not be read.
    Unreadable {
        count: usize,
    },
}

/// The step the manage dialog is at.
#[derive(Clone, Debug, PartialEq)]
enum Step {
    Choose,
    /// Confirming a removal: of a pack (by name) or a song (by id).
    ConfirmPack,
    ConfirmSong,
    ConfirmUnreadable,
}

/// Managing an imported pack or song.
pub struct ManageDialog {
    env: Env,
    what: Manage,
    step: Step,
    shell: Option<ShellFn>,
    screen: [f32; 2],
    /// An import runs: nothing is removed or read again until it ends
    /// (removing clears up shared files the import may be writing).
    busy: bool,
    platform: Platform,
}

impl ManageDialog {
    pub fn new(env: Env, what: Manage, shell: Option<ShellFn>, busy: bool) -> ManageDialog {
        let step = match what {
            Manage::Unreadable { .. } => Step::ConfirmUnreadable,
            _ => Step::Choose,
        };
        ManageDialog {
            env,
            what,
            step,
            shell,
            screen: super::DESIGN,
            busy,
            platform: Platform::Web,
        }
    }

    /// Worded for `platform` (desktop hides songs, never deletes).
    pub fn on(mut self, platform: Platform) -> ManageDialog {
        self.platform = platform;
        self
    }

    fn pack(&self) -> &PackEntry {
        static NONE: std::sync::OnceLock<PackEntry> = std::sync::OnceLock::new();
        match &self.what {
            Manage::Pack { pack, .. } | Manage::Song { pack, .. } => pack,
            Manage::Unreadable { .. } => NONE.get_or_init(PackEntry::default),
        }
    }

    fn songs(&self) -> usize {
        match &self.what {
            Manage::Pack { songs, .. } | Manage::Song { songs, .. } => *songs,
            Manage::Unreadable { count } => *count,
        }
    }

    /// The buttons now, `(name, label, kind)`.
    fn buttons(&self) -> Vec<(&'static str, String, &'static str)> {
        if self.busy {
            return vec![("Cancel", "Close".to_string(), "Button")];
        }
        match self.step {
            Step::Choose => {
                let mut v = Vec::new();
                let verb = if self.platform == Platform::Desktop {
                    "Hide"
                } else {
                    "Remove"
                };
                if let Manage::Song { .. } = self.what {
                    v.push(("RemoveSong", format!("{verb} song"), "Button"));
                }
                if self.pack().rereadable {
                    v.push(("Reread", "Read folder again".to_string(), "Button"));
                }
                if self.pack().removable {
                    v.push(("RemovePack", format!("{verb} pack"), "Button"));
                }
                v.push(("Cancel", "Cancel".to_string(), "Button"));
                v
            }
            Step::ConfirmPack | Step::ConfirmSong | Step::ConfirmUnreadable => vec![
                (
                    "Confirm",
                    if self.platform == Platform::Desktop {
                        "Hide".to_string()
                    } else {
                        "Remove".to_string()
                    },
                    "DangerButton",
                ),
                ("Cancel", "Keep".to_string(), "Button"),
            ],
        }
    }

    fn texts(&self) -> (String, String) {
        let plural = |n: usize| if n == 1 { "song" } else { "songs" };
        if self.busy {
            let title = match &self.what {
                Manage::Pack { pack, .. } => pack.name.clone(),
                Manage::Song { title, .. } => title.clone(),
                Manage::Unreadable { .. } => "Songs that could not be read".into(),
            };
            return (
                title,
                "An import is running: removing and reading folders again wait until it ends."
                    .into(),
            );
        }
        if self.platform == Platform::Desktop
            && let Some(t) = self.desktop_texts()
        {
            return t;
        }
        match (&self.step, &self.what) {
            (Step::Choose, Manage::Pack { pack, songs }) => (
                pack.name.clone(),
                format!(
                    "{songs} {} imported into this browser{}.",
                    plural(*songs),
                    if pack.rereadable {
                        ", from a folder it can read again"
                    } else {
                        ""
                    }
                ),
            ),
            (Step::Choose, Manage::Song { title, pack, .. }) => {
                (title.clone(), format!("In the pack “{}”.", pack.name))
            }
            (Step::ConfirmPack, _) => (
                format!("Remove “{}”?", self.pack().name),
                format!(
                    "Its {} {} leave this browser; their plays stay in the history.",
                    self.songs(),
                    plural(self.songs())
                ),
            ),
            (Step::ConfirmSong, Manage::Song { title, .. }) => (
                format!("Remove “{title}”?"),
                "It leaves this browser; its plays stay in the history.".to_string(),
            ),
            (Step::ConfirmUnreadable, _) => (
                format!(
                    "Remove {} stored {} that could not be read?",
                    self.songs(),
                    plural(self.songs())
                ),
                "Their records no longer read; their files go too. Their plays stay in the history."
                    .to_string(),
            ),
            (Step::ConfirmSong, _) | (Step::Choose, Manage::Unreadable { .. }) => {
                (String::new(), String::new())
            }
        }
    }

    /// Desktop's: the songs are read from their folders; hiding takes
    /// them off the list, reading the folder again brings them back.
    fn desktop_texts(&self) -> Option<(String, String)> {
        let plural = |n: usize| if n == 1 { "song" } else { "songs" };
        Some(match (&self.step, &self.what) {
            (Step::Choose, Manage::Pack { pack, songs }) => (
                pack.name.clone(),
                format!(
                    "{songs} {}, read from a song folder in place.",
                    plural(*songs)
                ),
            ),
            (Step::ConfirmPack, _) => (
                format!("Hide “{}”?", self.pack().name),
                format!(
                    "Its {} {} leave the list; nothing in the folder is deleted, and reading it \
                     again brings them back. Their plays stay in the history.",
                    self.songs(),
                    plural(self.songs())
                ),
            ),
            (Step::ConfirmSong, Manage::Song { title, .. }) => (
                format!("Hide “{title}”?"),
                "It leaves the list; nothing in the folder is deleted, and reading the folder \
                 again brings it back. Its plays stay in the history."
                    .to_string(),
            ),
            (Step::ConfirmUnreadable, _) => (
                format!(
                    "Forget {} {} that could not be read?",
                    self.songs(),
                    plural(self.songs())
                ),
                "They leave the list; nothing on disk is deleted.".to_string(),
            ),
            _ => return None,
        })
    }

    fn call(&self, c: SelectCall, cx: &mut Cx) {
        if let Some(s) = &self.shell {
            s(c, cx);
        }
    }

    fn rebuild(&mut self, root: &mut Actor, cx: &mut Cx) {
        *root = self.build(cx);
        root.finish_tweening();
        if let Some((n, _, _)) = self.buttons().first() {
            cx.focus(*n);
        }
    }
}

impl Screen for ManageDialog {
    fn name(&self) -> &str {
        "manage"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        self.screen = [vp.width, vp.height];
        let env = self.env.clone();
        let w = 560.0_f32.min(self.screen[0] - 32.0);
        let (title, body) = self.texts();
        let body = wrap_lines(&body, w - 48.0, 14.0);
        // The buttons in one row, or in rows of two when they do not fit.
        let buttons = self.buttons();
        let n = buttons.len();
        let room = w - 48.0;
        let needed: f32 = buttons
            .iter()
            .map(|(_, l, _)| look::mono_width(l, 15.0) + 28.0)
            .sum::<f32>()
            + (n.saturating_sub(1)) as f32 * 10.0;
        let per_row = if needed <= room { n.max(1) } else { 2 };
        let rows = n.div_ceil(per_row);
        let h = 110.0 + body.len() as f32 * 18.0 + rows as f32 * 50.0;
        let (mut children, [x, y]) = shell(&env, self.screen, [w, h], &title);
        for (i, line) in body.iter().enumerate() {
            let mut b = env.text(&format!("Body{i}"), line, 14.0, 400, "Muted");
            wrap(&mut b, w - 48.0);
            children.push(left(b, x + 24.0, y + 66.0 + i as f32 * 18.0));
        }
        for (r, chunk) in buttons.chunks(per_row).enumerate() {
            let k = chunk.len() as f32;
            let bw = ((room - (k - 1.0) * 10.0) / k).min(if rows > 1 { room } else { 200.0 });
            let mut bx = x + w - 24.0 - k * bw - (k - 1.0) * 10.0;
            let by = y + h - 44.0 - (rows - 1 - r) as f32 * 50.0;
            for (name, label, kind) in chunk {
                children.push(button(&env, name, label, [bx + bw / 2.0, by], bw, kind));
                bx += bw + 10.0;
            }
        }
        dialog_root(children)
    }

    fn focusables(&self) -> Vec<Focusable> {
        self.buttons()
            .into_iter()
            .map(|(n, _, _)| Focusable::new(n))
            .collect()
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Activate { name } => {
                match (name.as_str(), &self.step) {
                    ("Cancel", Step::Choose | Step::ConfirmUnreadable) => cx.close(true),
                    ("Cancel", _) if self.busy => cx.close(true),
                    ("Cancel", _) => {
                        self.step = Step::Choose;
                        self.rebuild(root, cx);
                    }
                    ("RemoveSong", _) => {
                        self.step = Step::ConfirmSong;
                        self.rebuild(root, cx);
                    }
                    ("RemovePack", _) => {
                        self.step = Step::ConfirmPack;
                        self.rebuild(root, cx);
                    }
                    ("Reread", _) => {
                        self.call(SelectCall::Reread(self.pack().name.clone()), cx);
                        cx.close(false);
                    }
                    ("Confirm", Step::ConfirmPack) => {
                        self.call(SelectCall::RemovePack(self.pack().name.clone()), cx);
                        cx.close(false);
                    }
                    ("Confirm", Step::ConfirmSong) => {
                        if let Manage::Song { id, .. } = &self.what {
                            self.call(SelectCall::RemoveSong(id.clone()), cx);
                        }
                        cx.close(false);
                    }
                    ("Confirm", Step::ConfirmUnreadable) => {
                        self.call(SelectCall::Notice("remove-unreadable".into()), cx);
                        cx.close(false);
                    }
                    _ => return Handled::No,
                }
                Handled::Yes
            }
            Input::Outside => {
                cx.close(true);
                Handled::Yes
            }
            Input::Menu {
                action: MenuAction::Back,
                ..
            } if self.step != Step::Choose && self.step != Step::ConfirmUnreadable => {
                self.step = Step::Choose;
                self.rebuild(root, cx);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

/// The report's lines: headings and items.
struct Lines<'a>(&'a [(bool, String)]);

impl ListModel for Lines<'_> {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn kind(&self, i: usize) -> ItemKind {
        if self.0[i].0 {
            ItemKind::Header
        } else {
            ItemKind::Row
        }
    }

    fn fill(&self, i: usize, item: &mut Actor, _cx: &mut Cx) {
        set_text(item, "Title", &self.0[i].1);
        set_text(item, "Subtitle", "");
        set_text(item, "Count", "");
        set_text(item, "Chevron", "");
    }
}

/// Adding songs: the pickers, the options, what runs, the last report.
pub struct ImportDialog {
    env: Env,
    view: Rc<RefCell<ImportView>>,
    shell: Option<ShellFn>,
    lines: Vec<(bool, String)>,
    list: List,
    screen: [f32; 2],
    rect: [f32; 4],
}

/// The report's text size.
const REPORT_TEXT: f32 = 13.0;

/// The import options the dialog offers, `(name, label)`.
const OPTIONS: [(&str, &str); 3] = [
    ("copy", "Store a chosen folder's songs in this browser"),
    ("videos", "Also store background videos"),
    ("details", "Details for pack authors"),
];

impl ImportDialog {
    pub fn new(env: Env, view: Rc<RefCell<ImportView>>, shell: Option<ShellFn>) -> ImportDialog {
        ImportDialog {
            env,
            view,
            shell,
            lines: Vec::new(),
            list: List::new(ListConfig::new("Report", [0.0, 0.0, 1.0, 1.0]), &Lines(&[])),
            screen: super::DESIGN,
            rect: [0.0; 4],
        }
    }

    /// The report's lines, each item wrapped to `width`.
    fn report_lines(&self, width: f32) -> Vec<(bool, String)> {
        let v = self.view.borrow();
        let mut out = Vec::new();
        if let Some(r) = &v.report {
            for (title, items) in &r.sections {
                out.push((true, title.clone()));
                for item in items {
                    out.extend(
                        wrap_lines(item, width, REPORT_TEXT)
                            .into_iter()
                            .map(|l| (false, l)),
                    );
                }
            }
        }
        out
    }

    fn options(&self) -> Vec<(&'static str, &'static str, bool)> {
        let v = self.view.borrow();
        // Desktop copies nothing: the options are the browser's.
        if v.platform == Platform::Desktop {
            return Vec::new();
        }
        OPTIONS
            .iter()
            .filter(|(n, _)| *n != "copy" || v.folders)
            .map(|&(n, l)| {
                let on = match n {
                    "copy" => v.copy,
                    "videos" => v.videos,
                    _ => v.details,
                };
                (n, l, on)
            })
            .collect()
    }

    fn status(&self) -> (String, bool) {
        let v = self.view.borrow();
        match (&v.running, &v.report) {
            (Some(t), _) => (t.clone(), false),
            (None, Some(r)) => (r.summary.clone(), r.error),
            (None, None) => (String::new(), false),
        }
    }

    fn call(&self, c: SelectCall, cx: &mut Cx) {
        if let Some(s) = &self.shell {
            s(c, cx);
        }
    }

    fn refresh(&mut self, root: &mut Actor, cx: &mut Cx) {
        let focused = cx.focused().map(str::to_string);
        *root = self.build(cx);
        root.finish_tweening();
        if let Some(f) = focused {
            cx.focus(f);
        }
    }
}

impl Screen for ImportDialog {
    fn name(&self) -> &str {
        "import"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        self.env.state.borrow_mut().import_open = true;
        let vp = cx.viewport().screen;
        self.screen = [vp.width, vp.height];
        let env = self.env.clone();
        let compact = self.screen[1] < 480.0;
        let w = 760.0_f32.min(self.screen[0] - 32.0);
        let h = (self.screen[1] - if compact { 16.0 } else { 80.0 }).min(620.0);
        let (mut children, [x, y]) = shell(&env, self.screen, [w, h], "ADD SONGS");
        let busy = self.view.borrow().running.is_some();
        // What is kept where, as the import keeps it.
        let (folders, copy, platform) = {
            let v = self.view.borrow();
            (v.folders, v.copy, v.platform)
        };
        let keeps = if folders && !copy {
            "A chosen folder's music and backgrounds are read from the folder, the rest is stored in this browser only."
        } else {
            "They stay in this browser only."
        };
        let intro = if platform == Platform::Desktop {
            "StepMania packs (.sm, .ssc, .dwi) and Dancing☆Onigiri works: a song folder (a StepMania, ITGmania or OutFox folder, a Songs folder, a pack) or a pack's zip. They are read where they are, nothing is copied; Options lists the folders.".to_string()
        } else {
            format!(
                "StepMania packs (.sm, .ssc, .dwi) and Dancing☆Onigiri works: a folder, a zip or files, or drop them on this page. {keeps}"
            )
        };
        let lines = wrap_lines(&intro, w - 48.0, 14.0);
        for (i, line) in lines.iter().enumerate() {
            let mut t = env.text(&format!("Intro{i}"), line, 14.0, 400, "Muted");
            wrap(&mut t, w - 48.0);
            children.push(left(t, x + 24.0, y + 58.0 + i as f32 * 18.0));
        }
        let bw = ((w - 48.0 - 10.0) / 2.0).min(260.0);
        let by = y + 58.0 + lines.len() as f32 * 18.0 + 24.0;
        let files = if platform == Platform::Desktop {
            "Choose zip"
        } else {
            "Choose zip or files"
        };
        for (i, (name, label)) in [("PickFolder", "Choose folder"), ("PickFiles", files)]
            .into_iter()
            .enumerate()
        {
            let mut b = button(
                &env,
                name,
                label,
                [x + 24.0 + bw / 2.0 + i as f32 * (bw + 10.0), by],
                bw,
                "Button",
            );
            if busy {
                look::run(&mut b, "diffusealpha,0.4");
            }
            children.push(b);
        }
        let mut oy = by + 44.0;
        for (name, label, on) in self.options() {
            let t = crate::widgets::controls::Toggle::new(
                &format!("Option.{name}"),
                label,
                [x + w / 2.0, oy],
                [w - 48.0, 34.0],
                on,
            );
            children.push(t.build(&*env.look));
            oy += 38.0;
        }
        let (status, error) = self.status();
        let mut st = env.text(
            "Status",
            &status,
            14.0,
            600,
            if error { "Danger" } else { "Text" },
        );
        wrap(&mut st, w - 48.0);
        children.push(left(st, x + 24.0, oy + 10.0));
        // The report, scrolled.
        let top = oy + 30.0;
        let bottom = y + h - 70.0;
        self.rect = [x + 24.0, top, w - 48.0, (bottom - top).max(40.0)];
        self.lines = self.report_lines(self.rect[2] - 32.0);
        let mut c = ListConfig::new("Report", self.rect);
        // Text rows (no banner box), wrapped.
        c.component = "Report".into();
        c.row = 22.0;
        c.header = 30.0;
        c.style = ListStyle::Plain;
        self.list = List::new(c, &Lines(&self.lines));
        children.push(self.list.build(&*env.look));
        children.push(button(
            &env,
            "Close",
            "Close",
            [x + w - 24.0 - 70.0, y + h - 36.0],
            140.0,
            "Button",
        ));
        dialog_root(children)
    }

    fn focusables(&self) -> Vec<Focusable> {
        // The two buttons side by side, then the rows below them (whose
        // focus rings touch the buttons: spatial moves would take a row
        // for the button to the right).
        let first = self
            .options()
            .first()
            .map(|(n, _, _)| format!("Option.{n}"))
            .unwrap_or_else(|| "Report".into());
        let pick = |name: &str, other: &str| Focusable {
            left: Some(other.into()),
            right: Some(other.into()),
            down: Some(first.clone()),
            ..Focusable::new(name)
        };
        let mut v = vec![
            pick("PickFolder", "PickFiles"),
            pick("PickFiles", "PickFolder"),
        ];
        v.extend(
            self.options()
                .into_iter()
                .map(|(n, _, _)| Focusable::new(format!("Option.{n}"))),
        );
        v.push(Focusable::new("Report"));
        v.push(Focusable::new("Close"));
        v
    }

    fn animating(&self) -> bool {
        self.list.animating()
    }

    fn update(&mut self, dt: f32, root: &mut Actor, cx: &mut Cx) {
        let look = self.env.look.clone();
        let lines = std::mem::take(&mut self.lines);
        self.list.update(dt, root, &Lines(&lines), &*look, cx);
        self.lines = lines;
    }

    fn message(&mut self, m: &Message, root: &mut Actor, cx: &mut Cx) {
        if m.name == IMPORT_CHANGED {
            self.refresh(root, cx);
        }
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        let busy = self.view.borrow().running.is_some();
        match input {
            Input::Activate { name } => {
                match name.as_str() {
                    "PickFolder" if !busy => self.call(SelectCall::PickFolder, cx),
                    "PickFiles" if !busy => self.call(SelectCall::PickFiles, cx),
                    "PickFolder" | "PickFiles" => {}
                    "Close" => {
                        self.env.state.borrow_mut().import_open = false;
                        cx.close(false);
                    }
                    other => {
                        let Some(opt) = other.strip_prefix("Option.") else {
                            return Handled::No;
                        };
                        let on = {
                            let mut v = self.view.borrow_mut();
                            let slot = match opt {
                                "copy" => &mut v.copy,
                                "videos" => &mut v.videos,
                                _ => &mut v.details,
                            };
                            *slot = !*slot;
                            *slot
                        };
                        if let Some(a) = hit::find_mut(root, other)
                            && let Some(mk) = part(a, "Mark")
                        {
                            crate::widgets::play(mk, if on { "On" } else { "Off" });
                        }
                        self.call(SelectCall::ImportOption(opt.to_string(), on), cx);
                        if opt == "details" {
                            // The report shows its details, or not.
                            cx.broadcast(Message::new(IMPORT_CHANGED));
                        }
                    }
                }
                Handled::Yes
            }
            Input::Outside => {
                cx.close(true);
                Handled::Yes
            }
            // Menu moves are the report's only while it is focused, and
            // only until its end: past it, the focus moves on.
            Input::Menu { .. } if cx.focused() != Some("Report") => Handled::No,
            _ => {
                let lines = std::mem::take(&mut self.lines);
                let (h, event) = self.list.input(input, root, &Lines(&lines));
                self.lines = lines;
                match input {
                    Input::Menu {
                        action: MenuAction::Up | MenuAction::Down,
                        ..
                    } if event.is_none() => Handled::No,
                    _ => h,
                }
            }
        }
    }
}

impl Drop for ImportDialog {
    fn drop(&mut self) {
        self.env.state.borrow_mut().import_open = false;
    }
}

/// Every notice in full, with its link.
pub struct NoticesDialog {
    env: Env,
    notices: Vec<Notice>,
    screen: [f32; 2],
}

impl NoticesDialog {
    pub fn new(env: Env, notices: Vec<Notice>) -> NoticesDialog {
        NoticesDialog {
            env,
            notices,
            screen: super::DESIGN,
        }
    }

    fn links(&self) -> Vec<(usize, String)> {
        self.notices
            .iter()
            .enumerate()
            .filter_map(|(i, n)| n.link.as_ref().map(|(l, _)| (i, l.clone())))
            .collect()
    }
}

impl Screen for NoticesDialog {
    fn name(&self) -> &str {
        "notices"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        self.screen = [vp.width, vp.height];
        let env = self.env.clone();
        let w = 640.0_f32.min(self.screen[0] - 32.0);
        let mut lines: Vec<(String, bool)> = Vec::new();
        for n in &self.notices {
            for l in wrap_lines(&n.text, w - 48.0, 14.0) {
                lines.push((l, n.error));
            }
            lines.push((String::new(), false));
        }
        let links = self.links();
        let h = (100.0 + lines.len() as f32 * 18.0 + links.len() as f32 * 48.0 + 40.0)
            .min(self.screen[1] - 16.0);
        let (mut children, [x, y]) = shell(&env, self.screen, [w, h], "NOTICES");
        for (i, (line, error)) in lines.iter().enumerate() {
            let mut t = env.text(
                &format!("Line{i}"),
                line,
                14.0,
                400,
                if *error { "Danger" } else { "Text" },
            );
            wrap(&mut t, w - 48.0);
            children.push(left(t, x + 24.0, y + 64.0 + i as f32 * 18.0));
        }
        let mut by = y + 64.0 + lines.len() as f32 * 18.0 + 10.0;
        for (i, label) in &links {
            children.push(button(
                &env,
                &format!("Link{i}"),
                label,
                [x + 24.0 + 130.0, by],
                260.0,
                "Button",
            ));
            by += 48.0;
        }
        children.push(button(
            &env,
            "Close",
            "Close",
            [x + w - 24.0 - 70.0, y + h - 36.0],
            140.0,
            "Button",
        ));
        dialog_root(children)
    }

    fn focusables(&self) -> Vec<Focusable> {
        let mut v: Vec<Focusable> = self
            .links()
            .into_iter()
            .map(|(i, _)| Focusable::new(format!("Link{i}")))
            .collect();
        v.push(Focusable::new("Close"));
        v
    }

    fn input(&mut self, input: &Input, _root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Activate { name } if name == "Close" => {
                cx.close(false);
                Handled::Yes
            }
            Input::Activate { name } => {
                let Some(i) = name
                    .strip_prefix("Link")
                    .and_then(|n| n.parse::<i64>().ok())
                else {
                    return Handled::No;
                };
                cx.close(false);
                // Song select takes it as a tap on the notice's link.
                cx.broadcast(
                    Message::new(NOTICE_LINK).with("Index", crate::message::Param::Int(i)),
                );
                Handled::Yes
            }
            Input::Outside => {
                cx.close(true);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

/// A chart's kept plays (`docs/plans/play-history.md`, Replays): newest
/// first, a page at a time, the best marked; the chosen one watched,
/// opened on the results screen, or removed (pressed twice). A row is
/// chosen by a tap or Start on it (Start then on Watch watches it).
pub struct PlaysDialog {
    env: Env,
    title: String,
    rows: Vec<PlayRow>,
    /// The chosen row's index.
    chosen: usize,
    /// The page's first row.
    first: usize,
    /// Remove was pressed once for the chosen row.
    arming: bool,
    shell: Option<ShellFn>,
    screen: [f32; 2],
    /// Built once: Watch (or Close) took focus.
    focused: bool,
}

/// A row's height in the plays list.
const PLAY_ROW: f32 = 46.0;

impl PlaysDialog {
    /// The plays `rows` of the chart `title` names ("Hard 9"); the best
    /// chosen first, else the newest.
    pub fn new(env: Env, title: &str, rows: Vec<PlayRow>, shell: Option<ShellFn>) -> PlaysDialog {
        let chosen = rows.iter().position(|r| r.best).unwrap_or(0);
        PlaysDialog {
            env,
            title: format!("PLAYS · {title}"),
            rows,
            chosen,
            first: 0,
            arming: false,
            shell,
            screen: super::DESIGN,
            focused: false,
        }
    }

    /// Rows a page: as many as fit, 3 to 8.
    fn per_page(&self) -> usize {
        let room = self.screen[1] - 16.0 - 70.0 - 2.0 * 50.0 - 40.0;
        ((room / (PLAY_ROW + 6.0)).floor() as usize).clamp(3, 8)
    }

    /// The page holding the chosen row.
    fn settle_page(&mut self) {
        let n = self.per_page();
        if self.chosen < self.first || self.chosen >= self.first + n {
            self.first = self.chosen / n * n;
        }
    }

    /// The buttons now, `(name, label, kind)`.
    fn buttons(&self) -> Vec<(&'static str, String, &'static str)> {
        let mut v = Vec::new();
        if !self.rows.is_empty() {
            v.push(("Watch", "Watch".to_string(), "PlayButton"));
            v.push(("Open", "Results".to_string(), "Button"));
            v.push(if self.arming {
                ("Remove", "Sure? Remove".to_string(), "DangerButton")
            } else {
                ("Remove", "Remove".to_string(), "Button")
            });
        }
        if self.first > 0 {
            v.push(("Newer", "‹ Newer".to_string(), "Button"));
        }
        if self.first + self.per_page() < self.rows.len() {
            v.push(("Older", "Older ›".to_string(), "Button"));
        }
        v.push(("Close", "Close".to_string(), "Button"));
        v
    }

    fn call(&self, c: SelectCall, cx: &mut Cx) {
        if let Some(s) = &self.shell {
            s(c, cx);
        }
    }

    fn publish(&self) {
        let mut st = self.env.state.borrow_mut();
        st.plays = self
            .rows
            .iter()
            .map(|r| {
                format!(
                    "{}|{}|{} {} {}|{}",
                    r.id, r.when, r.grade, r.score, r.lamp_text, r.mods
                )
            })
            .collect();
        st.plays_selected = self.rows.get(self.chosen).map(|r| r.id.clone());
    }

    fn rebuild(&mut self, root: &mut Actor, cx: &mut Cx, focus: &str) {
        *root = self.build(cx);
        root.finish_tweening();
        cx.focus(focus);
    }

    fn row(&self, i: usize, r: &PlayRow, [x, y, w]: [f32; 3]) -> Vec<Actor> {
        let env = &self.env;
        let chosen = i == self.chosen;
        let mut plate = look::component_or(
            &*env.look,
            "Button",
            "Button",
            &Spec::new([w, PLAY_ROW], ""),
        );
        plate.name = format!("Row{i}");
        call(&mut plate, "xy", &[x + w / 2.0, y + PLAY_ROW / 2.0]);
        // The chosen one ringed in the accent.
        let mut ring = Vec::new();
        if chosen {
            let c = env.color("Accent");
            for (size, at) in [
                ([w + 6.0, 2.0], [x + w / 2.0, y - 2.0]),
                ([w + 6.0, 2.0], [x + w / 2.0, y + PLAY_ROW + 2.0]),
                ([2.0, PLAY_ROW + 6.0], [x - 2.0, y + PLAY_ROW / 2.0]),
                ([2.0, PLAY_ROW + 6.0], [x + w + 2.0, y + PLAY_ROW / 2.0]),
            ] {
                let mut q = quad("", size, c);
                call(&mut q, "xy", &at);
                ring.push(q);
            }
        }
        let mut first = vec![r.when.clone(), r.ruleset.clone()];
        if r.autoplay {
            first.push("AUTOPLAY".into());
        }
        if r.best {
            first.push("★ best".into());
        }
        let text = if chosen { "Text" } else { "Secondary" };
        let mut a = env.text("", &first.join(" · "), 13.0, 600, text);
        wrap(&mut a, w - 190.0);
        let mut b = env.text(
            "",
            if r.mods.is_empty() { "—" } else { &r.mods },
            12.0,
            400,
            "Muted",
        );
        wrap(&mut b, w - 190.0);
        let score = if r.pending {
            format!("{} {} …", r.grade, r.score)
        } else {
            format!("{} {}", r.grade, r.score)
        };
        let c = env.numbers("", score.trim(), 15.0, "Text");
        let d = look::tracked(env.text("", &r.lamp_text, 11.0, 700, "Gold"), 80);
        let mut out = ring;
        out.extend([
            plate,
            left(a, x + 14.0, y + 15.0),
            left(b, x + 14.0, y + 32.0),
            super::right(c, x + w - 14.0, y + 15.0),
            super::right(d, x + w - 14.0, y + 32.0),
        ]);
        out
    }
}

impl Screen for PlaysDialog {
    fn name(&self) -> &str {
        "plays"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        self.screen = [vp.width, vp.height];
        self.settle_page();
        let env = self.env.clone();
        let w = 760.0_f32.min(self.screen[0] - 32.0);
        let page: Vec<(usize, &PlayRow)> = self
            .rows
            .iter()
            .enumerate()
            .skip(self.first)
            .take(self.per_page())
            .collect();
        let buttons = self.buttons();
        let room = w - 48.0;
        let widths: Vec<f32> = buttons
            .iter()
            .map(|(_, l, _)| look::mono_width(l, 15.0) + 32.0)
            .collect();
        // Rows of buttons that fit.
        let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
        let mut used = 0.0;
        for (i, bw) in widths.iter().enumerate() {
            let line = lines.last_mut().expect("a line");
            if !line.is_empty() && used + 10.0 + bw > room {
                lines.push(vec![i]);
                used = *bw;
            } else {
                used += if line.is_empty() { *bw } else { 10.0 + bw };
                lines.last_mut().expect("a line").push(i);
            }
        }
        let body = if self.rows.is_empty() {
            30.0
        } else {
            page.len() as f32 * (PLAY_ROW + 6.0)
        };
        let h = (70.0 + body + 20.0 + lines.len() as f32 * 50.0 + 6.0).min(self.screen[1] - 16.0);
        let (mut children, [x, y]) = shell(&env, self.screen, [w, h], &self.title);
        if self.rows.is_empty() {
            children.push(left(
                env.text(
                    "Empty",
                    "No play of this chart is kept yet.",
                    14.0,
                    400,
                    "Muted",
                ),
                x + 24.0,
                y + 76.0,
            ));
        }
        for (k, (i, r)) in page.iter().enumerate() {
            children.extend(self.row(
                *i,
                r,
                [x + 24.0, y + 60.0 + k as f32 * (PLAY_ROW + 6.0), room],
            ));
        }
        if self.rows.len() > page.len() {
            let t = env.text(
                "Page",
                &format!(
                    "{}–{} of {}",
                    self.first + 1,
                    self.first + page.len(),
                    self.rows.len()
                ),
                12.0,
                400,
                "Faint",
            );
            children.push(super::right(t, x + w - 24.0, y + 30.0));
        }
        for (l, line) in lines.iter().enumerate() {
            let by = y + h - 30.0 - (lines.len() - 1 - l) as f32 * 50.0;
            let mut bx = x + 24.0;
            for &i in line {
                let (name, label, kind) = &buttons[i];
                children.push(button(
                    &env,
                    name,
                    label,
                    [bx + widths[i] / 2.0, by],
                    widths[i],
                    kind,
                ));
                bx += widths[i] + 10.0;
            }
        }
        self.publish();
        // At first Start watches the chosen play.
        if !std::mem::replace(&mut self.focused, true) {
            cx.focus(if self.rows.is_empty() {
                "Close"
            } else {
                "Watch"
            });
        }
        dialog_root(children)
    }

    fn focusables(&self) -> Vec<Focusable> {
        let n = self.per_page();
        (self.first..(self.first + n).min(self.rows.len()))
            .map(|i| Focusable::new(format!("Row{i}")))
            .chain(
                self.buttons()
                    .into_iter()
                    .map(|(n, _, _)| Focusable::new(n)),
            )
            .collect()
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        // Focus moving away from an armed Remove disarms it (the move
        // itself is the menu's).
        if self.arming
            && let Input::Menu { action, .. } = input
            && matches!(
                action,
                MenuAction::Up | MenuAction::Down | MenuAction::Left | MenuAction::Right
            )
        {
            self.arming = false;
            *root = self.build(cx);
            root.finish_tweening();
            return Handled::No;
        }
        match input {
            Input::Activate { name } => {
                if let Some(i) = name
                    .strip_prefix("Row")
                    .and_then(|n| n.parse::<usize>().ok())
                {
                    if i < self.rows.len() {
                        self.chosen = i;
                        self.arming = false;
                        self.rebuild(root, cx, "Watch");
                    }
                    return Handled::Yes;
                }
                let id = self.rows.get(self.chosen).map(|r| r.id.clone());
                match (name.as_str(), id) {
                    // The call first: a closed layer's requests go nowhere
                    // (it goes at once under reduced motion).
                    ("Watch", Some(id)) => {
                        self.call(SelectCall::WatchPlay(id), cx);
                        cx.close(false);
                    }
                    ("Open", Some(id)) => {
                        self.call(SelectCall::ShowPlay(id), cx);
                        cx.close(false);
                    }
                    ("Remove", Some(id)) => {
                        if self.arming {
                            self.arming = false;
                            self.call(SelectCall::RemovePlay(id), cx);
                            self.rows.remove(self.chosen);
                            self.chosen = self.chosen.min(self.rows.len().saturating_sub(1));
                            let focus = if self.rows.is_empty() {
                                "Close"
                            } else {
                                "Remove"
                            };
                            self.rebuild(root, cx, focus);
                        } else {
                            self.arming = true;
                            self.rebuild(root, cx, "Remove");
                        }
                    }
                    ("Newer", _) => {
                        let n = self.per_page();
                        self.first = self.first.saturating_sub(n);
                        self.chosen = self.first;
                        self.arming = false;
                        self.rebuild(root, cx, if self.first > 0 { "Newer" } else { "Older" });
                    }
                    ("Older", _) => {
                        let n = self.per_page();
                        self.first += n;
                        self.chosen = self.first;
                        self.arming = false;
                        let more = self.first + n < self.rows.len();
                        self.rebuild(root, cx, if more { "Older" } else { "Newer" });
                    }
                    ("Close", _) => cx.close(true),
                    _ => return Handled::No,
                }
                Handled::Yes
            }
            Input::Outside => {
                cx.close(true);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

impl Drop for PlaysDialog {
    fn drop(&mut self) {
        let mut st = self.env.state.borrow_mut();
        st.plays.clear();
        st.plays_selected = None;
    }
}
