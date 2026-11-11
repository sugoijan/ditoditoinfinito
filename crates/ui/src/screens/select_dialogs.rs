//! Song select's dialogs: adding songs (the import: its pickers, options,
//! progress and report), managing what was imported (reading a pack's
//! folder again, removing a pack, a song or the songs that could not be
//! read, each confirmed; none of it while an import runs) and the notices
//! in full.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_actor::Actor;
use ddi_l10n::UiText;
use ddi_l10n::msg::common;
use ddi_l10n::msg::select as msg;

use super::data::{ImportView, Notice, PackEntry, Platform, PlayRow, ShareState, ShareView};
use super::dialog::{button, button_rows, dialog_body, dialog_root, shell, wrap_lines};
use super::select::NOTICE_LINK;
use super::select::{IMPORT_CHANGED, SHARE_CHANGED, SelectCall, ShellFn};
use super::{Env, left};
use crate::hit;
use crate::look::{self, Spec, call, quad_at};
use crate::message::Message;
use crate::nav::{Focusable, MenuAction};
use crate::request::UiRequest;
use crate::request::{EnterKey, InputHints, InputMode};
use crate::screen::{Cx, Handled, Input, Screen};
use crate::state::select_view::pack_label;
use crate::widgets::field::{FieldEvent, SearchField};
use crate::widgets::list::{ItemKind, List, ListConfig, ListModel, ListStyle};
use crate::widgets::{part, set_text};

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
        }
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
            return vec![("Cancel", common::close().into(), "Button")];
        }
        // Desktop hides songs (their folders are only read); the web
        // removes them from its storage.
        let platform = self.env.platform.l10n();
        match self.step {
            Step::Choose => {
                let mut v = Vec::new();
                if let Manage::Song { .. } = self.what {
                    v.push((
                        "RemoveSong",
                        msg::manage_remove_song(platform).into(),
                        "Button",
                    ));
                }
                if self.pack().rereadable {
                    v.push(("Reread", msg::manage_reread().into(), "Button"));
                }
                if self.pack().removable {
                    v.push((
                        "RemovePack",
                        msg::manage_remove_pack(platform).into(),
                        "Button",
                    ));
                }
                v.push(("Cancel", common::cancel().into(), "Button"));
                v
            }
            Step::ConfirmPack | Step::ConfirmSong | Step::ConfirmUnreadable => vec![
                (
                    "Confirm",
                    msg::manage_confirm(platform).into(),
                    "DangerButton",
                ),
                ("Cancel", msg::manage_keep().into(), "Button"),
            ],
        }
    }

    /// The title and the text under it. Desktop's songs are read from
    /// their folders: hiding takes them off the list, reading the folder
    /// again brings them back.
    fn texts(&self) -> (String, String) {
        let platform = self.env.platform.l10n();
        if self.busy {
            let title = match &self.what {
                Manage::Pack { pack, .. } => pack_label(pack.name.as_deref()),
                Manage::Song { title, .. } => UiText::data(title),
                Manage::Unreadable { .. } => msg::manage_unreadable_title(),
            };
            return (title.into(), msg::manage_busy().into());
        }
        let (title, body) = match (&self.step, &self.what) {
            (Step::Choose, Manage::Pack { pack, songs }) => (
                pack_label(pack.name.as_deref()),
                if pack.rereadable {
                    msg::manage_pack_rereadable(platform, *songs)
                } else {
                    msg::manage_pack(platform, *songs)
                },
            ),
            (Step::Choose, Manage::Song { title, pack, .. }) => (
                UiText::data(title),
                msg::manage_song_pack(pack_label(pack.name.as_deref())),
            ),
            (Step::ConfirmPack, _) => (
                msg::manage_confirm_pack_title(platform, pack_label(self.pack().name.as_deref())),
                msg::manage_confirm_pack(platform, self.songs()),
            ),
            (Step::ConfirmSong, Manage::Song { title, .. }) => (
                msg::manage_confirm_song_title(platform, UiText::data(title)),
                msg::manage_confirm_song(platform),
            ),
            (Step::ConfirmUnreadable, _) => (
                msg::manage_confirm_unreadable_title(platform, self.songs()),
                msg::manage_confirm_unreadable(platform),
            ),
            (Step::ConfirmSong, _) | (Step::Choose, Manage::Unreadable { .. }) => {
                return (String::new(), String::new());
            }
        };
        (title.into(), body.into())
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
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

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
        dialog_body(&env, &mut children, &body, [x, y], w, |_| "Muted");
        children.extend(button_rows(&env, &buttons, per_row, [x, y, w, h], 200.0));
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
                    // Only an imported pack (named) is read again or removed.
                    ("Reread", _) => {
                        if let Some(name) = self.pack().name.clone() {
                            self.call(SelectCall::Reread(name), cx);
                        }
                        cx.close(false);
                    }
                    ("Confirm", Step::ConfirmPack) => {
                        if let Some(name) = self.pack().name.clone() {
                            self.call(SelectCall::RemovePack(name), cx);
                        }
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

/// A button's name for an expiry's word.
fn expiry_button(word: &str) -> &'static str {
    match word {
        "1h" => "Expiry.1h",
        "1d" => "Expiry.1d",
        "1w" => "Expiry.1w",
        "1m" => "Expiry.1m",
        _ => "Expiry.never",
    }
}

/// An expiry's button's label.
fn expiry_label(word: &str) -> UiText {
    match word {
        "1h" => msg::share_expiry_hour(),
        "1d" => msg::share_expiry_day(),
        "1w" => msg::share_expiry_week(),
        "1m" => msg::share_expiry_month(),
        _ => msg::share_expiry_never(),
    }
}

/// Share… (`docs/plans/shared-songs.md`): how long the link lives,
/// then the upload as the shell reports it ([`SHARE_CHANGED`]), then
/// the link to copy or open.
pub struct ShareDialog {
    env: Env,
    pack: String,
    songs: Vec<String>,
    /// What is shared, by its name (a song's title, a pack's); `None` for
    /// the share list, titled by how many songs it holds.
    title: Option<String>,
    view: Rc<RefCell<ShareView>>,
    shell: Option<ShellFn>,
    /// The expiry was chosen: the upload's state shows from now on.
    started: bool,
}

impl ShareDialog {
    pub fn new(
        env: Env,
        pack: String,
        songs: Vec<String>,
        title: Option<String>,
        view: Rc<RefCell<ShareView>>,
        shell: Option<ShellFn>,
    ) -> ShareDialog {
        ShareDialog {
            env,
            pack,
            songs,
            title,
            view,
            shell,
            started: false,
        }
    }

    fn state(&self) -> ShareState {
        if self.started {
            self.view.borrow().state
        } else {
            ShareState::Idle
        }
    }

    /// The buttons now, `(name, label, kind)`.
    fn buttons(&self) -> Vec<(&'static str, String, &'static str)> {
        match self.state() {
            ShareState::Idle => {
                // By the server's word (the buttons are `Expiry.<word>`).
                let mut v: Vec<_> = ddi_library::share::EXPIRIES
                    .iter()
                    .map(|w| (expiry_button(w), expiry_label(w).into(), "Button"))
                    .collect();
                v.push(("Cancel", common::cancel().into(), "Button"));
                v
            }
            ShareState::Uploading => vec![("Cancel", common::close().into(), "Button")],
            ShareState::Done => vec![
                ("Copy", msg::share_copy_link().into(), "Button"),
                ("Open", common::open().into(), "Button"),
                ("Cancel", common::close().into(), "Button"),
            ],
            ShareState::Failed => vec![("Cancel", common::close().into(), "Button")],
        }
    }

    /// The title and the lines under it.
    fn texts(&self) -> (String, Vec<String>) {
        let n = self.songs.len();
        let v = self.view.borrow();
        match self.state() {
            ShareState::Idle => (
                match &self.title {
                    Some(title) => msg::share_title(UiText::data(title)),
                    None => msg::share_title_list(n),
                }
                .into(),
                vec![msg::share_intro(n, pack_label(Some(&self.pack))).into()],
            ),
            ShareState::Uploading => (
                msg::share_uploading().into(),
                vec![
                    msg::share_progress(
                        v.done_songs,
                        v.songs,
                        super::data::size_text(v.done_bytes),
                        super::data::size_text(v.bytes),
                    )
                    .into(),
                ],
            ),
            ShareState::Done => (
                msg::share_done().into(),
                vec![msg::share_done_note().into(), UiText::data(&v.link).into()],
            ),
            // Why, as the shell found it, in the player's language.
            ShareState::Failed => (
                msg::share_failed().into(),
                v.error
                    .iter()
                    .map(|e| super::failures::share_upload_text(e).into())
                    .collect(),
            ),
        }
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

impl Screen for ShareDialog {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn name(&self) -> &str {
        "share"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        let screen = [vp.width, vp.height];
        let env = self.env.clone();
        let w = 560.0_f32.min(screen[0] - 32.0);
        let (title, lines) = self.texts();
        let link = matches!(self.state(), ShareState::Done);
        let body: Vec<String> = lines
            .iter()
            .enumerate()
            .flat_map(|(i, l)| {
                // The link in one line as given (selectable by copy).
                if link && i == 1 {
                    vec![l.clone()]
                } else {
                    wrap_lines(l, w - 48.0, 14.0)
                }
            })
            .collect();
        let buttons = self.buttons();
        let n = buttons.len();
        let per_row = if n > 3 { 3 } else { n.max(1) };
        let rows = n.div_ceil(per_row);
        let h = 110.0 + body.len() as f32 * 18.0 + rows as f32 * 50.0;
        let (mut children, [x, y]) = shell(&env, screen, [w, h], &title);
        dialog_body(&env, &mut children, &body, [x, y], w, |i| {
            if link && i == 1 { "Text" } else { "Muted" }
        });
        children.extend(button_rows(&env, &buttons, per_row, [x, y, w, h], 200.0));
        // The buttons by name, for the debug state's rects.
        env.state.borrow_mut().page_items = buttons.iter().map(|(n, _, _)| n.to_string()).collect();
        dialog_root(children)
    }

    fn focusables(&self) -> Vec<Focusable> {
        self.buttons()
            .into_iter()
            .map(|(n, _, _)| Focusable::new(n))
            .collect()
    }

    fn message(&mut self, m: &Message, root: &mut Actor, cx: &mut Cx) {
        if m.name == SHARE_CHANGED && self.started {
            self.rebuild(root, cx);
        }
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Activate { name } => {
                match name.as_str() {
                    "Cancel" => cx.close(true),
                    "Copy" => cx.request(UiRequest::CopyText(self.view.borrow().link.clone())),
                    "Open" => cx.request(UiRequest::OpenUrl(self.view.borrow().link.clone())),
                    name if name.starts_with("Expiry.") && !self.started => {
                        let word = name.trim_start_matches("Expiry.");
                        if !ddi_library::share::EXPIRIES.contains(&word) {
                            return Handled::No;
                        }
                        // The dialog shows the upload from its start.
                        *self.view.borrow_mut() = ShareView {
                            state: ShareState::Uploading,
                            songs: self.songs.len(),
                            ..ShareView::default()
                        };
                        self.started = true;
                        self.call(
                            SelectCall::Share {
                                pack: self.pack.clone(),
                                songs: self.songs.clone(),
                                expires: word.to_string(),
                            },
                            cx,
                        );
                        self.rebuild(root, cx);
                    }
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

/// Adding songs: folders and ZIPs, or a desktop DanOni URL import.
pub struct ImportDialog {
    env: Env,
    view: Rc<RefCell<ImportView>>,
    shell: Option<ShellFn>,
    lines: Vec<(bool, String)>,
    list: List,
    screen: [f32; 2],
    rect: [f32; 4],
    tab: ImportTab,
    url: SearchField,
    progress_phase: f32,
    progress_rect: [f32; 3],
    url_error: Option<String>,
    /// "New song" ([`SelectCall::NewSong`]) is offered.
    new_song: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ImportTab {
    Files,
    Danoni,
}

const REPORT_TEXT: f32 = 13.0;
/// The web's import options, by name (`Option.<name>`).
const OPTIONS: [&str; 3] = ["copy", "videos", "details"];

/// An import option's label.
fn option_label(name: &str) -> UiText {
    match name {
        "copy" => msg::import_option_copy(),
        "videos" => msg::import_option_videos(),
        _ => msg::import_option_details(),
    }
}

impl ImportDialog {
    pub fn new(env: Env, view: Rc<RefCell<ImportView>>, shell: Option<ShellFn>) -> ImportDialog {
        let mut url = SearchField::new(
            "DanoniUrlField",
            &msg::import_url_placeholder(),
            [0.0; 2],
            [1.0; 2],
        );
        url.hints = InputHints {
            mode: InputMode::Url,
            enter: EnterKey::Go,
            max_chars: Some(8192),
        };
        let v = view.borrow();
        url.text = v.danoni.url.clone();
        let tab = if env.platform == Platform::Desktop
            && v.danoni_urls
            && (v.danoni_cancel || !v.danoni.url.is_empty() || v.danoni.origin.is_some())
        {
            ImportTab::Danoni
        } else {
            ImportTab::Files
        };
        drop(v);
        ImportDialog {
            env,
            view,
            shell,
            lines: Vec::new(),
            list: List::new(ListConfig::new("Report", [0.0, 0.0, 1.0, 1.0]), &Lines(&[])),
            screen: super::DESIGN,
            rect: [0.0; 4],
            tab,
            url,
            progress_phase: 0.0,
            progress_rect: [0.0; 3],
            url_error: None,
            new_song: false,
        }
    }

    /// Offers "New song": a song made from a music file, opened in the
    /// chart editor.
    pub fn with_new_song(mut self, on: bool) -> ImportDialog {
        self.new_song = on;
        self
    }

    fn urls(&self) -> bool {
        let v = self.view.borrow();
        self.env.platform == Platform::Desktop && v.danoni_urls
    }

    fn busy_animation(&self) -> bool {
        self.tab == ImportTab::Danoni
            && self.view.borrow().running.is_some()
            && !self.origin_pending()
            && !self.env.reduced()
    }
    fn activity_x(&self) -> f32 {
        let [left, _, width] = self.progress_rect;
        let position = if self.env.reduced() {
            0.5
        } else {
            (1.0 - (self.progress_phase * std::f32::consts::TAU).cos()) * 0.5
        };
        left + width * (0.09 + 0.82 * position)
    }
    fn origin_pending(&self) -> bool {
        self.tab == ImportTab::Danoni && self.view.borrow().danoni.origin.is_some()
    }

    fn origin_button(&self, action: &str) -> String {
        let id = self
            .view
            .borrow()
            .danoni
            .origin
            .as_ref()
            .map_or(0, |origin| origin.request_id);
        format!("{action}.{id}")
    }

    fn report_lines(&self, width: f32) -> Vec<(bool, String)> {
        let v = self.view.borrow();
        let mut out = Vec::new();
        if self.tab == ImportTab::Danoni
            && let Some(origin) = &v.danoni.origin
        {
            out.push((true, msg::import_origin_heading().into()));
            // The address (data), what the buttons do.
            let address: String = if origin.origin.is_empty() {
                String::new()
            } else {
                UiText::data(&origin.origin).into()
            };
            let explain = msg::import_origin_explain();
            for text in [address.as_str(), explain.as_str()] {
                if !text.is_empty() {
                    out.extend(
                        wrap_lines(text, width, REPORT_TEXT)
                            .into_iter()
                            .map(|l| (false, l)),
                    );
                }
            }
        } else if let Some(r) = &v.report {
            for (title, items) in r.sections() {
                out.push((true, title));
                for item in &items {
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

    /// The options shown: name, label, on.
    fn options(&self) -> Vec<(&'static str, UiText, bool)> {
        let v = self.view.borrow();
        if self.env.platform == Platform::Desktop || self.tab != ImportTab::Files {
            return Vec::new();
        }
        OPTIONS
            .iter()
            .filter(|n| **n != "copy" || v.folders)
            .map(|&n| {
                let on = match n {
                    "copy" => v.copy,
                    "videos" => v.videos,
                    _ => v.details,
                };
                (n, option_label(n), on)
            })
            .collect()
    }

    fn status(&self) -> (String, bool) {
        if let Some(error) = &self.url_error {
            return (error.clone(), true);
        }
        let v = self.view.borrow();
        if self.tab == ImportTab::Danoni {
            if v.danoni.origin.is_some() {
                return (msg::import_decision_needed().into(), false);
            }
            if v.running.is_some()
                && let Some(stage) = &v.danoni.stage
            {
                // The importer's step (a message the shell chose), then
                // how far.
                return (
                    match v.danoni.progress {
                        Some((done, total)) => {
                            msg::import_stage_progress(stage, done, total).into()
                        }
                        None => stage.clone(),
                    },
                    false,
                );
            }
        }
        match (&v.running, &v.report) {
            (Some(t), _) => (t.text(), false),
            (None, Some(r)) => (r.summary(), r.error()),
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
        super::restore_focus(root, focused.as_deref());
        if let Some(f) = focused.filter(|f| self.focusables().iter().any(|a| &a.name == f)) {
            cx.focus(f);
        } else {
            cx.focus(if self.tab == ImportTab::Danoni {
                "TabDanoni"
            } else {
                "PickFolder"
            });
        }
    }

    fn set_tab(&mut self, tab: ImportTab, root: &mut Actor, cx: &mut Cx) {
        if !self.urls() {
            return;
        }
        cx.end_text();
        self.tab = tab;
        self.url_error = None;
        self.refresh(root, cx);
        cx.focus(if tab == ImportTab::Danoni {
            "TabDanoni"
        } else {
            "TabFiles"
        });
    }

    fn submit(&mut self, root: &mut Actor, cx: &mut Cx) {
        if !self.urls() || self.view.borrow().running.is_some() || self.origin_pending() {
            return;
        }
        let url = self.url.text.trim().to_string();
        if url.is_empty() {
            return;
        }
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            self.url_error = Some(msg::import_url_invalid().into());
            self.refresh(root, cx);
            return;
        }
        self.url_error = None;
        {
            let mut v = self.view.borrow_mut();
            v.danoni.url = url.clone();
            v.danoni.offline = false;
        }
        cx.end_text();
        self.call(
            SelectCall::ImportDanoni {
                url,
                offline: false,
            },
            cx,
        );
        self.refresh(root, cx);
    }

    fn file_controls(
        &self,
        children: &mut Vec<Actor>,
        x: f32,
        mut y: f32,
        w: f32,
        busy: bool,
    ) -> f32 {
        let v = self.view.borrow();
        // A browser keeping a chosen folder reads its music and
        // backgrounds from there (the web's alone).
        let linked = self.env.platform == Platform::Web && v.folders && !v.copy;
        let platform = self.env.platform.l10n();
        let compact = self.screen[1] < 480.0;
        let intro = match (compact, linked) {
            (true, true) => msg::import_intro_compact_linked(),
            (true, false) => msg::import_intro_compact(platform),
            (false, true) => msg::import_intro_linked(),
            (false, false) => msg::import_intro(platform),
        };
        drop(v);
        let text_size = if self.screen[1] < 480.0 { 12.0 } else { 14.0 };
        for (i, line) in wrap_lines(&intro, w - 48.0, text_size).iter().enumerate() {
            children.push(left(
                self.env
                    .text(&format!("Intro{i}"), line, text_size, 400, "Muted"),
                x + 24.0,
                y,
            ));
            y += text_size + 4.0;
        }
        y += if compact { 20.0 } else { 24.0 };
        let bw = ((w - 58.0) / 2.0).min(260.0);
        for (i, (name, label)) in [
            ("PickFolder", msg::import_choose_folder()),
            ("PickFiles", msg::import_choose_files(platform)),
        ]
        .into_iter()
        .enumerate()
        {
            let mut b = button(
                &self.env,
                name,
                &label,
                [x + 24.0 + bw / 2.0 + i as f32 * (bw + 10.0), y],
                bw,
                "Button",
            );
            if busy {
                look::run(&mut b, "diffusealpha,0.4");
            }
            children.push(b);
        }
        y += if compact { 40.0 } else { 44.0 };
        if self.new_song {
            let mut b = button(
                &self.env,
                "NewSong",
                &msg::import_new_song(),
                [x + 24.0 + bw + 5.0, y],
                bw * 2.0 + 10.0,
                "Button",
            );
            if busy {
                look::run(&mut b, "diffusealpha,0.4");
            }
            children.push(b);
            y += if compact { 40.0 } else { 44.0 };
        }
        let option_height = if self.screen[1] < 480.0 { 28.0 } else { 34.0 };
        for (name, label, on) in self.options() {
            children.push(
                crate::widgets::controls::Toggle::new(
                    &format!("Option.{name}"),
                    &label,
                    [x + w / 2.0, y],
                    [w - 48.0, option_height],
                    on,
                )
                .build(&*self.env.look),
            );
            y += option_height + 4.0;
        }
        y
    }

    fn url_controls(
        &mut self,
        children: &mut Vec<Actor>,
        x: f32,
        mut y: f32,
        w: f32,
        busy: bool,
    ) -> f32 {
        if self.origin_pending() {
            return y + 10.0;
        }
        let intro = msg::import_danoni_intro();
        for (i, line) in wrap_lines(&intro, w - 48.0, 14.0).iter().enumerate() {
            children.push(left(
                self.env
                    .text(&format!("DanoniIntro{i}"), line, 14.0, 400, "Muted"),
                x + 24.0,
                y,
            ));
            y += 18.0;
        }
        let narrow = w < 520.0;
        y += 24.0;
        self.url.centre = [
            x + 24.0 + (w - 48.0 - if narrow { 0.0 } else { 120.0 }) / 2.0,
            y,
        ];
        self.url.size = [w - 48.0 - if narrow { 0.0 } else { 120.0 }, 40.0];
        let mut field = self.url.build(&*self.env.look);
        if busy {
            look::run(&mut field, "diffusealpha,0.55");
        }
        children.push(field);
        if narrow {
            y += 48.0;
        }
        let retry = self
            .view
            .borrow()
            .report
            .as_ref()
            .is_some_and(|r| r.error());
        let mut import = button(
            &self.env,
            "ImportDanoni",
            &if retry {
                msg::import_danoni_retry()
            } else {
                msg::import_danoni_import()
            },
            [x + w - 76.0, y],
            104.0,
            "Button",
        );
        if busy || self.url.text.trim().is_empty() {
            look::run(&mut import, "diffusealpha,0.4");
        }
        children.push(import);
        if !narrow {
            y += 44.0;
        }
        if narrow {
            y += 30.0;
        }
        let note = msg::import_danoni_note();
        for (i, line) in wrap_lines(&note, w - 48.0, 12.0).iter().enumerate() {
            children.push(left(
                self.env
                    .text(&format!("DanoniNote{i}"), line, 12.0, 400, "Muted"),
                x + 24.0,
                y,
            ));
            y += 15.0;
        }
        y + 6.0
    }
}

impl Screen for ImportDialog {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn cursor(&self, target: &crate::hit::Target) -> Option<crate::request::Cursor> {
        self.url.cursor(target)
    }

    fn name(&self) -> &str {
        "import"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        self.env.state.borrow_mut().import_open = true;
        let vp = cx.viewport().screen;
        self.screen = [vp.width, vp.height];
        let w = 760.0_f32.min(self.screen[0] - 32.0);
        let h = (self.screen[1] - if self.screen[1] < 480.0 { 16.0 } else { 80.0 }).min(620.0);
        let (mut children, [x, y]) = shell(&self.env, self.screen, [w, h], &msg::import_title());
        // In the language now (built again on a change of language).
        self.url.hint = msg::import_url_placeholder().into();
        let busy = self.view.borrow().running.is_some();
        let mut top = y + 58.0;
        if self.urls() {
            let bw = (w - 58.0) / 2.0;
            for (i, (name, label, tab)) in [
                ("TabFiles", msg::import_tab_files(), ImportTab::Files),
                ("TabDanoni", msg::import_tab_danoni(), ImportTab::Danoni),
            ]
            .into_iter()
            .enumerate()
            {
                let b = button(
                    &self.env,
                    name,
                    &label,
                    [x + 24.0 + bw / 2.0 + i as f32 * (bw + 10.0), top + 10.0],
                    bw,
                    if tab == self.tab {
                        "PlayButton"
                    } else {
                        "Button"
                    },
                );
                children.push(b);
            }
            top += 46.0;
        }
        let mut bottom = y + h - 70.0;
        top = if self.tab == ImportTab::Danoni {
            self.url_controls(&mut children, x, top, w, busy)
        } else {
            self.file_controls(&mut children, x, top, w, busy)
        };
        let (status, error) = self.status();
        let mut st = self.env.text(
            "Status",
            &status,
            14.0,
            600,
            if error { "Danger" } else { "Text" },
        );
        look::ellipsized(&mut st, w - 48.0);
        children.push(left(st, x + 24.0, top + 10.0));
        top += if self.screen[1] < 480.0 && self.tab == ImportTab::Files {
            22.0
        } else {
            30.0
        };
        if self.tab == ImportTab::Danoni && busy && !self.origin_pending() {
            let width = w - 48.0;
            self.progress_rect = [x + 24.0, top, width];
            let mut track = quad_at(
                "DanoniProgress",
                [width, 6.0],
                self.env.color("Muted"),
                [x + w / 2.0, top],
            );
            call(&mut track, "diffusealpha", &[0.2]);
            children.push(track);
            if let Some((done, total)) = self
                .view
                .borrow()
                .danoni
                .progress
                .filter(|(_, total)| *total > 0)
            {
                let amount = width * (done as f64 / total as f64).clamp(0.0, 1.0) as f32;
                let mut fill = quad_at(
                    "DanoniProgress.Fill",
                    [amount, 6.0],
                    self.env.color("Accent"),
                    [x + 24.0 + amount / 2.0, top],
                );
                call(&mut fill, "diffusealpha", &[0.5]);
                children.push(fill);
            }
            children.push(quad_at(
                "DanoniProgress.Activity",
                [width * 0.18, 6.0],
                self.env.color("Accent"),
                [self.activity_x(), top],
            ));
            top += 14.0;
        }
        if self.origin_pending() {
            bottom -= 50.0;
            let bw = (w - 68.0) / 3.0;
            for (i, (name, label)) in [
                ("OriginOnce", msg::import_origin_once()),
                ("OriginRemember", msg::import_origin_remember()),
                ("OriginDecline", msg::import_origin_decline()),
            ]
            .into_iter()
            .enumerate()
            {
                children.push(button(
                    &self.env,
                    &self.origin_button(name),
                    &label,
                    [x + 24.0 + bw / 2.0 + i as f32 * (bw + 10.0), y + h - 96.0],
                    bw,
                    "Button",
                ));
            }
        }
        self.rect = [x + 24.0, top, w - 48.0, (bottom - top).max(20.0)];
        self.lines = self.report_lines(self.rect[2] - 32.0);
        let mut c = ListConfig::new("Report", self.rect);
        c.component = "Report".into();
        c.row = 22.0;
        c.header = 30.0;
        c.style = ListStyle::Plain;
        self.list = List::new(c, &Lines(&self.lines));
        children.push(self.list.build(&*self.env.look));
        if busy && self.view.borrow().danoni_cancel {
            children.push(button(
                &self.env,
                "CancelDanoni",
                &msg::import_cancel(),
                [x + 94.0, y + h - 36.0],
                140.0,
                "Button",
            ));
        }
        children.push(button(
            &self.env,
            "Close",
            &common::close(),
            [x + w - 94.0, y + h - 36.0],
            140.0,
            "Button",
        ));
        if self.origin_pending() {
            cx.focus(self.origin_button("OriginDecline"));
        } else if self.urls() {
            cx.focus(if self.tab == ImportTab::Danoni {
                "TabDanoni"
            } else {
                "TabFiles"
            });
        }
        dialog_root(children)
    }

    fn focusables(&self) -> Vec<Focusable> {
        let mut v = Vec::new();
        let busy = self.view.borrow().running.is_some();
        let first = if self.tab == ImportTab::Files {
            "PickFolder"
        } else if self.origin_pending() || busy {
            "Report"
        } else {
            "DanoniUrlField"
        };
        if self.urls() {
            for (name, other) in [("TabFiles", "TabDanoni"), ("TabDanoni", "TabFiles")] {
                v.push(Focusable {
                    left: Some(other.into()),
                    right: Some(other.into()),
                    down: Some(first.into()),
                    ..Focusable::new(name)
                });
            }
        }
        if self.tab == ImportTab::Files {
            let next = self
                .options()
                .first()
                .map(|(n, _, _)| format!("Option.{n}"))
                .unwrap_or_else(|| "Report".into());
            let below = if self.new_song {
                "NewSong".to_string()
            } else {
                next.clone()
            };
            for (name, other) in [("PickFolder", "PickFiles"), ("PickFiles", "PickFolder")] {
                v.push(Focusable {
                    left: Some(other.into()),
                    right: Some(other.into()),
                    down: Some(below.clone()),
                    ..Focusable::new(name)
                });
            }
            if self.new_song {
                v.push(Focusable {
                    up: Some("PickFolder".into()),
                    down: Some(next),
                    ..Focusable::new("NewSong")
                });
            }
            v.extend(
                self.options()
                    .into_iter()
                    .map(|(n, _, _)| Focusable::new(format!("Option.{n}"))),
            );
        } else if !busy && !self.origin_pending() {
            v.extend([
                Focusable::new("DanoniUrlField"),
                Focusable::new("ImportDanoni"),
            ]);
        }
        v.push(Focusable::new_list("Report"));
        if self.origin_pending() {
            for (name, left, right) in [
                ("OriginOnce", "OriginDecline", "OriginRemember"),
                ("OriginRemember", "OriginOnce", "OriginDecline"),
                ("OriginDecline", "OriginRemember", "OriginOnce"),
            ] {
                v.push(Focusable {
                    left: Some(self.origin_button(left)),
                    right: Some(self.origin_button(right)),
                    ..Focusable::new(self.origin_button(name))
                });
            }
        }
        if busy && self.view.borrow().danoni_cancel {
            v.push(Focusable::new("CancelDanoni"));
        }
        v.push(Focusable::new("Close"));
        v
    }

    fn animating(&self) -> bool {
        self.list.animating() || self.busy_animation()
    }

    fn moving(&self) -> bool {
        self.list.animating()
    }
    fn update(&mut self, dt: f32, root: &mut Actor, cx: &mut Cx) {
        if self.busy_animation() {
            self.progress_phase = (self.progress_phase + dt.clamp(0.0, 0.25) / 2.0) % 1.0;
        }
        if let Some(activity) = hit::find_mut(root, "DanoniProgress.Activity") {
            call(activity, "x", &[self.activity_x()]);
        }
        let lines = std::mem::take(&mut self.lines);
        self.list
            .update(dt, root, &Lines(&lines), &*self.env.look, cx);
        self.lines = lines;
    }
    fn message(&mut self, m: &Message, root: &mut Actor, cx: &mut Cx) {
        if m.name == IMPORT_CHANGED {
            if self.view.borrow().danoni.origin.is_some() && self.urls() {
                self.tab = ImportTab::Danoni;
                cx.end_text();
            }
            self.refresh(root, cx);
            if self.origin_pending() {
                cx.focus(self.origin_button("OriginDecline"));
            }
        }
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        let busy = self.view.borrow().running.is_some();
        if self.tab == ImportTab::Danoni && !busy && !self.origin_pending() {
            if let Input::Menu { action, .. } = input
                && cx.menu_from_pad()
                && cx.focused() == Some("DanoniUrlField")
            {
                match action {
                    MenuAction::Start if !self.url.text.trim().is_empty() => {
                        cx.end_text();
                        self.submit(root, cx);
                        return Handled::Yes;
                    }
                    MenuAction::Back => {
                        cx.end_text();
                        cx.focus("TabDanoni");
                        return Handled::Yes;
                    }
                    action if action.is_direction() => cx.end_text(),
                    _ => {}
                }
            }
            let (handled, event) = self.url.input(input, root, cx, &|_| Vec::new());
            match event {
                Some(FieldEvent::Submitted(_)) => self.submit(root, cx),
                Some(FieldEvent::Changed(_)) => {
                    self.url_error = None;
                    if let Some(button) = hit::find_mut(root, "ImportDanoni") {
                        look::run(
                            button,
                            if self.url.text.trim().is_empty() {
                                "diffusealpha,0.4"
                            } else {
                                "diffusealpha,1"
                            },
                        );
                    }
                }
                None => {}
            }
            if handled == Handled::Yes {
                return handled;
            }
        }
        match input {
            // Left and Right on a tab, or Select anywhere: the other tab.
            Input::Menu {
                action: action @ (MenuAction::Left | MenuAction::Right | MenuAction::Select),
                repeat: false,
            } if self.urls()
                && (*action == MenuAction::Select
                    || matches!(cx.focused(), Some("TabFiles" | "TabDanoni"))) =>
            {
                self.set_tab(
                    if self.tab == ImportTab::Files {
                        ImportTab::Danoni
                    } else {
                        ImportTab::Files
                    },
                    root,
                    cx,
                );
                Handled::Yes
            }
            Input::Activate { name } => {
                if matches!(name.as_str(), "PickFolder" | "PickFiles" | "NewSong")
                    && self.env.platform != Platform::Desktop
                    && (!cx.in_gesture() || cx.menu_from_pad())
                {
                    self.url_error = Some(msg::import_needs_gesture().into());
                    self.refresh(root, cx);
                    return Handled::Yes;
                }
                match name.as_str() {
                    "TabFiles" => self.set_tab(ImportTab::Files, root, cx),
                    "TabDanoni" => self.set_tab(ImportTab::Danoni, root, cx),
                    "PickFolder" if !busy && self.tab == ImportTab::Files => {
                        self.call(SelectCall::PickFolder, cx)
                    }
                    "PickFiles" if !busy && self.tab == ImportTab::Files => {
                        self.call(SelectCall::PickFiles, cx)
                    }
                    "NewSong" if !busy && self.new_song && self.tab == ImportTab::Files => {
                        // The editor opens over song select once the music
                        // is picked; this dialog is done either way.
                        self.env.state.borrow_mut().import_open = false;
                        cx.close(false);
                        self.call(SelectCall::NewSong, cx);
                    }
                    "PickFolder" | "PickFiles" | "NewSong" => {}
                    "ImportDanoni" if self.tab == ImportTab::Danoni => self.submit(root, cx),
                    "CancelDanoni" if busy && self.view.borrow().danoni_cancel => {
                        self.call(SelectCall::CancelDanoni, cx)
                    }
                    other
                        if other.starts_with("Origin")
                            && self.urls()
                            && self.tab == ImportTab::Danoni =>
                    {
                        let Some((action, request_id)) = other.split_once('.') else {
                            return Handled::Yes;
                        };
                        let Ok(request_id) = request_id.parse::<u64>() else {
                            return Handled::Yes;
                        };
                        if !matches!(action, "OriginOnce" | "OriginRemember" | "OriginDecline") {
                            return Handled::Yes;
                        }
                        let pending = self
                            .view
                            .borrow()
                            .danoni
                            .origin
                            .as_ref()
                            .is_some_and(|origin| origin.request_id == request_id);
                        // A pointer release captured before a rebuild must not answer a newer request.
                        if pending {
                            self.view.borrow_mut().danoni.origin = None;
                            self.call(
                                SelectCall::DanoniOriginDecision {
                                    request_id,
                                    allow: action != "OriginDecline",
                                    remember: action == "OriginRemember",
                                },
                                cx,
                            );
                            self.refresh(root, cx);
                        }
                    }
                    "ImportDanoni" | "CancelDanoni" => {}
                    "Close" => {
                        cx.end_text();
                        self.env.state.borrow_mut().import_open = false;
                        cx.close(false);
                    }
                    other => {
                        let Some(opt) = other.strip_prefix("Option.") else {
                            return Handled::No;
                        };
                        if !self.options().iter().any(|(name, _, _)| *name == opt) {
                            return Handled::No;
                        }
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
                            cx.broadcast(Message::new(IMPORT_CHANGED));
                        }
                    }
                }
                Handled::Yes
            }
            Input::Outside => {
                cx.end_text();
                cx.close(true);
                Handled::Yes
            }
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
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

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
        let (mut children, [x, y]) = shell(&env, self.screen, [w, h], &msg::notices_title());
        for (i, (line, error)) in lines.iter().enumerate() {
            let mut t = env.text(
                &format!("Line{i}"),
                line,
                14.0,
                400,
                if *error { "Danger" } else { "Text" },
            );
            look::ellipsized(&mut t, w - 48.0);
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
            &common::close(),
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
/// A dialog's rows a page at a time, one chosen, with an action a first
/// press arms and a second takes (the plays list's Remove, the shares'
/// Delete): what [`PlaysDialog`] and Options' Shares dialog share.
#[derive(Default)]
pub(crate) struct PagedRows {
    /// The chosen row, and the page's first.
    pub chosen: usize,
    pub first: usize,
    /// The armed action was pressed once for the chosen row.
    pub arming: bool,
}

/// A row's height in a [`PagedRows`] list.
pub(crate) const PAGED_ROW: f32 = 46.0;

/// What [`PagedRows::act`] did with an activation.
pub(crate) enum Paged {
    /// A row was chosen: built again, the focus on the dialog's first
    /// action.
    Chosen,
    /// Another page: built again, the focus on this button.
    Page(&'static str),
    /// Closed (Close).
    Closed,
    /// Taken, nothing to do (a row no longer there).
    Taken,
}

impl PagedRows {
    /// Rows a page on a screen `screen_h` high: as many as fit, 3 to 8.
    pub fn per_page(screen_h: f32) -> usize {
        let room = screen_h - 16.0 - 70.0 - 2.0 * 50.0 - 40.0;
        ((room / (PAGED_ROW + 6.0)).floor() as usize).clamp(3, 8)
    }

    /// The chosen row kept among `len`, on the page that holds it.
    pub fn settle(&mut self, len: usize, per_page: usize) {
        self.chosen = self.chosen.min(len.saturating_sub(1));
        if self.chosen < self.first || self.chosen >= self.first + per_page {
            self.first = self.chosen / per_page * per_page;
        }
    }

    /// The pages' buttons where there are pages before or after.
    pub fn page_buttons(
        &self,
        len: usize,
        per_page: usize,
        v: &mut Vec<(&'static str, String, &'static str)>,
    ) {
        if self.first > 0 {
            v.push(("Newer", msg::page_newer().into(), "Button"));
        }
        if self.first + per_page < len {
            v.push(("Older", msg::page_older().into(), "Button"));
        }
    }

    /// The page's rows as focusables, then `buttons`.
    pub fn focusables(
        &self,
        len: usize,
        per_page: usize,
        buttons: Vec<(&'static str, String, &'static str)>,
    ) -> Vec<Focusable> {
        (self.first..(self.first + per_page).min(len))
            .map(|i| Focusable::new(format!("Row{i}")))
            .chain(buttons.into_iter().map(|(n, _, _)| Focusable::new(n)))
            .collect()
    }

    /// A menu move disarms the armed action: whether it was (the dialog
    /// builds again and lets the menu move the focus).
    pub fn disarm(&mut self, input: &Input) -> bool {
        let moved = matches!(
            input,
            Input::Menu {
                action: MenuAction::Up | MenuAction::Down | MenuAction::Left | MenuAction::Right,
                ..
            }
        );
        moved && std::mem::take(&mut self.arming)
    }

    /// An activation of a row, a page button or Close, among `len` rows;
    /// `None` for the dialog's own actions.
    pub fn act(&mut self, name: &str, len: usize, per_page: usize, cx: &mut Cx) -> Option<Paged> {
        if let Some(i) = name
            .strip_prefix("Row")
            .and_then(|n| n.parse::<usize>().ok())
        {
            if i >= len {
                return Some(Paged::Taken);
            }
            self.chosen = i;
            self.arming = false;
            return Some(Paged::Chosen);
        }
        match name {
            "Newer" => {
                self.first = self.first.saturating_sub(per_page);
                self.chosen = self.first;
                self.arming = false;
                Some(Paged::Page(if self.first > 0 { "Newer" } else { "Older" }))
            }
            "Older" => {
                self.first += per_page;
                self.chosen = self.first;
                self.arming = false;
                let more = self.first + per_page < len;
                Some(Paged::Page(if more { "Older" } else { "Newer" }))
            }
            "Close" => {
                cx.close(true);
                Some(Paged::Closed)
            }
            _ => None,
        }
    }

    /// Row `i`'s plate at `[x, y, w]` (its top left), ringed in the accent
    /// when it is the chosen one.
    pub fn plate(&self, env: &Env, i: usize, [x, y, w]: [f32; 3]) -> Vec<Actor> {
        let plate = env.component(
            "Button",
            "Button",
            &Spec::new([w, PAGED_ROW], ""),
            &format!("Row{i}"),
            [x + w / 2.0, y + PAGED_ROW / 2.0],
        );
        let mut out = vec![plate];
        if i == self.chosen {
            let c = env.color("Accent");
            for (size, at) in [
                ([w + 6.0, 2.0], [x + w / 2.0, y - 2.0]),
                ([w + 6.0, 2.0], [x + w / 2.0, y + PAGED_ROW + 2.0]),
                ([2.0, PAGED_ROW + 6.0], [x - 2.0, y + PAGED_ROW / 2.0]),
                ([2.0, PAGED_ROW + 6.0], [x + w + 2.0, y + PAGED_ROW / 2.0]),
            ] {
                out.push(quad_at("", size, c, at));
            }
        }
        out
    }

    /// "1–8 of 20" at the dialog's top right, when there are pages.
    pub fn page_label(
        &self,
        env: &Env,
        shown: usize,
        len: usize,
        [right, y]: [f32; 2],
    ) -> Option<Actor> {
        (len > shown).then(|| {
            let t = env.text(
                "Page",
                &msg::page_range(self.first + 1, self.first + shown, len),
                12.0,
                400,
                "Faint",
            );
            super::right(t, right, y)
        })
    }
}

/// `buttons` from the left in lines no wider than `room`: each line's
/// buttons (by index) and each button's width.
pub(crate) fn button_lines(
    buttons: &[(&'static str, String, &'static str)],
    room: f32,
) -> (Vec<Vec<usize>>, Vec<f32>) {
    let widths: Vec<f32> = buttons
        .iter()
        .map(|(_, l, _)| look::mono_width(l, 15.0) + 32.0)
        .collect();
    let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
    let mut used = 0.0;
    for (i, bw) in widths.iter().enumerate() {
        let line = lines.last_mut().expect("a line");
        if !line.is_empty() && used + 10.0 + bw > room {
            lines.push(vec![i]);
            used = *bw;
        } else {
            used += if line.is_empty() { *bw } else { 10.0 + bw };
            line.push(i);
        }
    }
    (lines, widths)
}

/// [`button_lines`]' buttons, the last line `bottom` high, from `x`.
pub(crate) fn place_button_lines(
    env: &Env,
    buttons: &[(&'static str, String, &'static str)],
    (lines, widths): &(Vec<Vec<usize>>, Vec<f32>),
    [x, bottom]: [f32; 2],
) -> Vec<Actor> {
    let mut out = Vec::new();
    for (l, line) in lines.iter().enumerate() {
        let by = bottom - (lines.len() - 1 - l) as f32 * 50.0;
        let mut bx = x;
        for &i in line {
            let (name, label, kind) = &buttons[i];
            out.push(button(
                env,
                name,
                label,
                [bx + widths[i] / 2.0, by],
                widths[i],
                kind,
            ));
            bx += widths[i] + 10.0;
        }
    }
    out
}

pub struct PlaysDialog {
    env: Env,
    title: String,
    rows: Vec<PlayRow>,
    /// The chosen row, the page, Remove armed.
    paged: PagedRows,
    shell: Option<ShellFn>,
    screen: [f32; 2],
    /// Built once: Watch (or Close) took focus.
    focused: bool,
}

impl PlaysDialog {
    /// The plays `rows` of the chart `chart` names ("Hard 9"), a version
    /// the game made marked by its `version`
    /// ([`super::data::ChartEntry::version`]); the best chosen first, else
    /// the newest.
    pub fn new(
        env: Env,
        chart: &str,
        version: Option<ddi_chart::simplify::Kind>,
        rows: Vec<PlayRow>,
        shell: Option<ShellFn>,
    ) -> PlaysDialog {
        let chosen = rows.iter().position(|r| r.best).unwrap_or(0);
        // The chart as it names itself (data).
        let chart = UiText::data(chart);
        let title = match version {
            Some(v) => msg::plays_title_version(chart, super::select::version_name(v)),
            None => msg::plays_title(chart),
        };
        PlaysDialog {
            env,
            title: title.into(),
            rows,
            paged: PagedRows {
                chosen,
                ..PagedRows::default()
            },
            shell,
            screen: super::DESIGN,
            focused: false,
        }
    }

    fn per_page(&self) -> usize {
        PagedRows::per_page(self.screen[1])
    }

    /// The buttons now, `(name, label, kind)`.
    fn buttons(&self) -> Vec<(&'static str, String, &'static str)> {
        let mut v = Vec::new();
        if !self.rows.is_empty() {
            v.push(("Watch", msg::plays_watch().into(), "PlayButton"));
            v.push(("Open", msg::plays_results().into(), "Button"));
            v.push(if self.paged.arming {
                ("Remove", msg::plays_remove_confirm().into(), "DangerButton")
            } else {
                ("Remove", msg::plays_remove().into(), "Button")
            });
        }
        self.paged
            .page_buttons(self.rows.len(), self.per_page(), &mut v);
        v.push(("Close", common::close().into(), "Button"));
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
        st.plays_selected = self.rows.get(self.paged.chosen).map(|r| r.id.clone());
    }

    fn rebuild(&mut self, root: &mut Actor, cx: &mut Cx, focus: &str) {
        *root = self.build(cx);
        root.finish_tweening();
        cx.focus(focus);
    }

    fn row(&self, i: usize, r: &PlayRow, [x, y, w]: [f32; 3]) -> Vec<Actor> {
        let env = &self.env;
        let chosen = i == self.paged.chosen;
        // When, by which rules (the history's data), and its marks: a
        // list of tags.
        let mut first: Vec<String> = [&r.when, &r.ruleset]
            .into_iter()
            .filter(|s| !s.is_empty())
            .map(|s| UiText::data(s).into())
            .collect();
        if r.rejudged {
            first.push(msg::plays_rejudged().into());
        }
        if r.autoplay {
            first.push(msg::plays_autoplay().into());
        }
        if r.best {
            first.push(msg::plays_best().into());
        }
        let text = if chosen { "Text" } else { "Secondary" };
        let mut a = env.text("", &first.join(" · "), 13.0, 600, text);
        look::ellipsized(&mut a, w - 190.0);
        // The options as played, as the history kept them.
        let mods: String = if r.mods.is_empty() {
            "—".into()
        } else {
            UiText::data(&r.mods).into()
        };
        let mut b = env.text("", &mods, 12.0, 400, "Muted");
        look::ellipsized(&mut b, w - 190.0);
        // The ruleset's readouts.
        let score = if r.pending {
            format!("{} {} …", r.grade, r.score)
        } else {
            format!("{} {}", r.grade, r.score)
        };
        let c = env.numbers("", &UiText::data(score.trim()), 15.0, "Text");
        let lamp: String = if r.lamp_text.is_empty() {
            String::new()
        } else {
            UiText::data(&r.lamp_text).into()
        };
        let d = look::tracked(env.text("", &lamp, 11.0, 700, "Gold"), 80);
        let mut out = self.paged.plate(env, i, [x, y, w]);
        out.extend([
            left(a, x + 14.0, y + 15.0),
            left(b, x + 14.0, y + 32.0),
            super::right(c, x + w - 14.0, y + 15.0),
            super::right(d, x + w - 14.0, y + 32.0),
        ]);
        out
    }
}

impl Screen for PlaysDialog {
    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn name(&self) -> &str {
        "plays"
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        self.screen = [vp.width, vp.height];
        let n = self.per_page();
        self.paged.settle(self.rows.len(), n);
        let env = self.env.clone();
        let w = 760.0_f32.min(self.screen[0] - 32.0);
        let page: Vec<(usize, &PlayRow)> = self
            .rows
            .iter()
            .enumerate()
            .skip(self.paged.first)
            .take(n)
            .collect();
        let buttons = self.buttons();
        let room = w - 48.0;
        let lines = button_lines(&buttons, room);
        let body = if self.rows.is_empty() {
            30.0
        } else {
            page.len() as f32 * (PAGED_ROW + 6.0)
        };
        let h = (70.0 + body + 20.0 + lines.0.len() as f32 * 50.0 + 6.0).min(self.screen[1] - 16.0);
        let (mut children, [x, y]) = shell(&env, self.screen, [w, h], &self.title);
        if self.rows.is_empty() {
            children.push(left(
                env.text("Empty", &msg::plays_empty(), 14.0, 400, "Muted"),
                x + 24.0,
                y + 76.0,
            ));
        }
        for (k, (i, r)) in page.iter().enumerate() {
            children.extend(self.row(
                *i,
                r,
                [x + 24.0, y + 60.0 + k as f32 * (PAGED_ROW + 6.0), room],
            ));
        }
        children.extend(self.paged.page_label(
            &env,
            page.len(),
            self.rows.len(),
            [x + w - 24.0, y + 30.0],
        ));
        children.extend(place_button_lines(
            &env,
            &buttons,
            &lines,
            [x + 24.0, y + h - 30.0],
        ));
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
        self.paged
            .focusables(self.rows.len(), self.per_page(), self.buttons())
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        // Focus moving away from an armed Remove disarms it (the move
        // itself is the menu's).
        if self.paged.disarm(input) {
            *root = self.build(cx);
            root.finish_tweening();
            return Handled::No;
        }
        match input {
            Input::Activate { name } => {
                if let Some(done) = self.paged.act(name, self.rows.len(), self.per_page(), cx) {
                    match done {
                        Paged::Chosen => self.rebuild(root, cx, "Watch"),
                        Paged::Page(focus) => self.rebuild(root, cx, focus),
                        Paged::Closed | Paged::Taken => {}
                    }
                    return Handled::Yes;
                }
                let id = self.rows.get(self.paged.chosen).map(|r| r.id.clone());
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
                        if self.paged.arming {
                            self.paged.arming = false;
                            self.call(SelectCall::RemovePlay(id), cx);
                            self.rows.remove(self.paged.chosen);
                            let focus = if self.rows.is_empty() {
                                "Close"
                            } else {
                                "Remove"
                            };
                            self.rebuild(root, cx, focus);
                        } else {
                            self.paged.arming = true;
                            self.rebuild(root, cx, "Remove");
                        }
                    }
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
