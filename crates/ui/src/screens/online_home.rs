//! The multiplayer home (`docs/plans/multiplayer-prototype-progress.md`,
//! M7a): a centered column of choices in the start menu's style, over the
//! start menu's outline rain, under the shared header and over the shared
//! footer. Every entry `docs/plans/multiplayer-ui.md` plans for the menu is
//! there: Find match and Browse rooms grayed out ("Coming soon", focusable
//! and inert as the start menu's Multiplayer is), CPU practice, Private
//! room and Join by link. The account sits at the footer's right: Sign in
//! with Twitch, or the signed-in name opening Sign out. Signing in is
//! asked for where it is needed ([`SignInDialog`]: Private room signed
//! out, the footer's Sign in), and the dialog follows the sign-in through
//! (desktop's wait for the browser, a cancel, a failure). Focus waits for
//! the first key, as the start menu's; hover moves it.

use std::cell::Cell;
use std::rc::Rc;

use ddi_actor::{Actor, Kind};
use ddi_engine::MenuInput as HintAction;
use ddi_l10n::UiText;
use ddi_l10n::msg::{common, pages as pm, room as m};

use super::dialog::{button_rows, dialog_body, dialog_root, wrap_lines};
use super::online::{OnlineCall, OnlineShell};
use super::{Env, footer, ground, header};
use crate::look::{self, Spec, call};
use crate::nav::{Focusable, MenuAction, vertical_chain};
use crate::screen::{Cx, Handled, Input, Screen};
use crate::state::online::{Account, Dest, SharedOnline, SignIn};

/// The column's entries' ids (their actors are `Choice.<id>`).
pub const FIND: &str = "find";
pub const BROWSE: &str = "browse";
pub const PRACTICE: &str = "practice";
pub const PRIVATE: &str = "private";
pub const JOIN: &str = "join";
/// The footer's account link.
pub const ACCOUNT: &str = "Link.Account";
/// The account menu's pick.
const ACCOUNT_MENU: &str = "MultiplayerAccount";

/// An entry of the column: its id, its label, whether it does something
/// yet (a grayed one is focusable and inert).
fn entries() -> [(&'static str, UiText, bool); 5] {
    [
        (FIND, m::home_find(), false),
        (BROWSE, m::home_browse(), false),
        (PRACTICE, m::home_practice(), true),
        (PRIVATE, m::home_private(), true),
        (JOIN, m::home_join(), true),
    ]
}

fn choice(id: &str) -> String {
    format!("Choice.{id}")
}

/// The home: the column, the account, what the online state says.
pub struct Home {
    env: Env,
    online: SharedOnline,
    shell: OnlineShell,
    size: [f32; 2],
    zoom: f32,
    live: super::Live,
    background_time: f64,
    background_animating: bool,
    /// The online state's version the tree was built for.
    seen: u64,
    time: f64,
    /// The sign-in dialog is open ([`SignInDialog`]).
    dialog: Rc<Cell<bool>>,
}

impl Home {
    pub fn new(env: Env, online: SharedOnline, shell: OnlineShell) -> Home {
        online.borrow_mut().check();
        let seen = online.borrow().version();
        Home {
            env,
            online,
            shell,
            size: [0.0; 2],
            zoom: 1.0,
            live: Default::default(),
            background_time: 0.0,
            background_animating: false,
            seen,
            time: 0.0,
            dialog: Rc::new(Cell::new(false)),
        }
    }

    /// The account link's label: the name signed in, Sign in signed out
    /// (or with the server unreachable); none while asking.
    fn account(&self) -> Option<String> {
        match self.online.borrow().account() {
            Account::SignedIn(me) => Some(UiText::data(&me.display_name).into()),
            Account::SignedOut | Account::Unreachable => Some(m::sign_in().into()),
            Account::Unknown | Account::Checking => None,
        }
    }

    /// The lines under the column: the server unreachable, the room being
    /// made or refused, a sign-in canceled or failed (once its dialog is
    /// closed).
    fn status(&self) -> Vec<String> {
        let o = self.online.borrow();
        let mut v = Vec::new();
        if matches!(o.account(), Account::Unreachable) {
            v.push(m::account_unreachable().into());
        }
        if o.creating() {
            v.push(m::creating().into());
        } else if let Some(p) = o.create_problem() {
            v.push(m::create_failed(UiText::data(&p.code)).into());
        }
        v
    }

    fn tree(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        let (size, zoom) = super::narrow_fit(&self.env, vp);
        self.size = size;
        self.zoom = zoom;
        let [w, h] = size;
        let env = self.env.clone();
        let head = env.metric("Header.Height", 64.0);
        let foot = env.metric("Footer.Height", 40.0);
        let mut children = vec![ground(&env, size)];
        // The start menu's rain (the home may get its own later).
        if let Some(mut background) = env
            .look
            .component("TitleMenu.Background", &Spec::new(size, ""))
        {
            background.name = "MenuBackground".into();
            if let Some(source) = env.look.image("TitleMenu.Background") {
                let texture = cx.texture(source, 1024.0);
                super::sprites_mut(&mut background, &mut |s| s.texture = texture);
            }
            self.background_animating = env.look.animate_component(
                "TitleMenu.Background",
                &mut background,
                size,
                self.background_time,
            );
            children.push(background);
        }
        children.push(header(&env, cx, w, &m::home_heading()));

        // The column, as the start menu's, centered between the bars;
        // rows as tall as the start menu's where there is room, never
        // under a touch target's 44 CSS pixels.
        let entries = entries();
        let n = entries.len() as f32;
        let status = self.status();
        let status_h = status.len() as f32 * 18.0 + if status.is_empty() { 0.0 } else { 12.0 };
        let room = h - head - foot - 24.0 - status_h;
        let css_scale = cx.viewport().scale / cx.viewport().ratio * zoom;
        let mut gap = env.metric("TitleMenu.Gap", 12.0);
        let mut row = env.metric("TitleMenu.RowHeight", 52.0);
        if n * row + (n - 1.0) * gap > room {
            gap = 8.0;
            row = ((room - (n - 1.0) * gap) / n).max(44.0 / css_scale);
        }
        let width = env.metric("TitleMenu.Width", 380.0).min(w - 32.0);
        let total = n * row + (n - 1.0) * gap + status_h;
        let top = head + ((h - head - foot - total) / 2.0).max(8.0);
        let mut column = Vec::new();
        for (i, (id, label, on)) in entries.iter().enumerate() {
            let component = if *on {
                "TitleMenu.Choice"
            } else {
                "TitleMenu.Disabled"
            };
            column.push(env.component(
                component,
                "Button",
                &Spec::new([width, row], label.as_str()),
                &choice(id),
                [w / 2.0, top + row / 2.0 + i as f32 * (row + gap)],
            ));
        }
        let below = top + n * row + (n - 1.0) * gap + 12.0;
        for (i, line) in status.iter().enumerate() {
            let mut t = env.text(&format!("Status{i}"), line, 13.0, 400, "Muted");
            look::ellipsized(&mut t, w - 64.0);
            call(&mut t, "xy", &[w / 2.0, below + 9.0 + i as f32 * 18.0]);
            column.push(t);
        }
        children.push(super::frame("MenuChoices", column));

        // The footer: the hints where they fit, the account at the right.
        let (to_move, choose, back) = (pm::move_hint(), pm::activate(), common::back());
        let account = self.account();
        let links: Vec<(&str, &str)> = account.iter().map(|a| (ACCOUNT, a.as_str())).collect();
        let hints: &[(&[&str], &str)] = &[
            (&["↑", "↓"], &to_move),
            (&["ENTER"], &choose),
            (&["ESC"], &back),
        ];
        let controller: &[(&[HintAction], &str)] = &[
            (&[HintAction::Up, HintAction::Down], &to_move),
            (&[HintAction::Confirm, HintAction::Start], &choose),
            (&[HintAction::Back], &back),
        ];
        let narrow = w < 640.0;
        children.push(footer(
            &env,
            size,
            if narrow { &[] } else { hints },
            if narrow { &[] } else { controller },
            "",
            &links,
            cx,
        ));
        let mut root = Actor::new(Kind::Frame(children));
        call(&mut root, "zoom", &[zoom]);
        super::transitions(
            &mut root,
            &env,
            &[
                ("Header", super::Edge::HeaderBar),
                ("Footer", super::Edge::FooterBar),
                ("MenuChoices", super::Edge::Bottom),
            ],
        );
        self.publish();
        root
    }

    /// Tells the debug state (and the tests) this screen's.
    fn publish(&self) {
        let mut st = self.env.state.borrow_mut();
        st.screen = "multiplayer".into();
        st.backdrop = None;
        st.page_items = self.focusables().into_iter().map(|f| f.name).collect();
        st.status = self.status().join("\n");
    }

    /// Opens the sign-in dialog, for `dest`.
    fn ask_sign_in(&self, dest: Dest, cx: &mut Cx) {
        if self.dialog.get() {
            return;
        }
        self.dialog.set(true);
        cx.overlay(
            SignInDialog::new(
                self.env.clone(),
                self.online.clone(),
                dest,
                self.dialog.clone(),
            ),
            true,
        );
    }

    fn activate(&mut self, name: &str, cx: &mut Cx) {
        if name == ACCOUNT {
            let me = self.online.borrow().me().map(|me| me.display_name.clone());
            match me {
                Some(name) if !self.dialog.get() => {
                    self.dialog.set(true);
                    let menu = super::dialog::ActionsDialog::new(
                        self.env.clone(),
                        &String::from(UiText::data(&name)),
                        vec![("signout".into(), m::sign_out().into())],
                        super::pick(ACCOUNT_MENU),
                    );
                    cx.overlay(
                        Watched {
                            inner: menu,
                            open: self.dialog.clone(),
                        },
                        true,
                    );
                }
                Some(_) => {}
                None => self.ask_sign_in(Dest::Home, cx),
            }
            return;
        }
        if name == "Back" {
            (self.shell)(OnlineCall::Leave, cx);
            return;
        }
        let Some(id) = name.strip_prefix("Choice.") else {
            return;
        };
        match id {
            PRACTICE => (self.shell)(OnlineCall::Practice, cx),
            PRIVATE => {
                let signed_in = self.online.borrow().me().is_some();
                if signed_in {
                    self.online.borrow_mut().private_room();
                } else {
                    self.ask_sign_in(Dest::Create, cx);
                }
            }
            JOIN => cx.push(super::online::OnlineScreen::new(
                self.env.clone(),
                self.online.clone(),
                super::online::OnlinePage::JoinLink,
                self.shell.clone(),
            )),
            // Coming soon: focused, and that is all.
            _ => {}
        }
    }
}

impl Screen for Home {
    fn name(&self) -> &str {
        "multiplayer"
    }

    fn defer_initial_focus(&self) -> bool {
        true
    }

    fn shared_bars(&self) -> bool {
        true
    }

    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        self.tree(cx)
    }

    fn focusables(&self) -> Vec<Focusable> {
        let mut names: Vec<String> = entries().iter().map(|(id, ..)| choice(id)).collect();
        if self.account().is_some() {
            names.push(ACCOUNT.into());
        }
        let mut v = vertical_chain(&names);
        v.push(Focusable::new("Back"));
        v
    }

    fn animating(&self) -> bool {
        self.live.animating(&self.env) || (self.background_animating && !self.env.reduced())
    }

    fn wake_at(&self) -> Option<f64> {
        // Answers come while nothing moves: looked for ten times a second.
        Some(self.time + 0.1)
    }

    fn message(&mut self, msg: &crate::Message, _root: &mut Actor, _cx: &mut Cx) {
        if super::menu_pick(msg, ACCOUNT_MENU) == Some("signout") {
            self.online.borrow_mut().sign_out();
        }
    }

    fn input(&mut self, input: &Input, _root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Hover {
                target: Some(target),
            } => {
                if let Some(f) = self
                    .focusables()
                    .into_iter()
                    .find(|f| f.name != "Back" && target.within(&f.name))
                {
                    cx.focus(f.name);
                }
                Handled::Yes
            }
            Input::Menu {
                action: MenuAction::Back,
                ..
            } => {
                (self.shell)(OnlineCall::Leave, cx);
                Handled::Yes
            }
            Input::Activate { name } => {
                let name = name.clone();
                self.activate(&name, cx);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn update(&mut self, _dt: f32, root: &mut Actor, cx: &mut Cx) {
        self.time = cx.time();
        let now = (cx.clock() * 1000.0).max(0.0) as u64;
        let (redirect, entered) = {
            let mut o = self.online.borrow_mut();
            o.poll(now);
            (o.take_redirect(), o.take_entered())
        };
        if let Some(r) = redirect {
            (self.shell)(OnlineCall::Redirect(r), cx);
        }
        if let Some(room) = entered {
            (self.shell)(OnlineCall::EnterRoom(room), cx);
        }
        if !cx.is_current_screen() {
            return;
        }
        // A sign-in under way (back from the server's page, or begun
        // elsewhere) shows in its dialog.
        let dest = match self.online.borrow().sign_in_state() {
            SignIn::Idle => None,
            SignIn::Starting { dest }
            | SignIn::Waiting { dest, .. }
            | SignIn::Completing { dest }
            | SignIn::Canceled { dest }
            | SignIn::Failed { dest, .. } => Some(dest.clone()),
        };
        if let Some(dest) = dest {
            self.ask_sign_in(dest, cx);
        }
        let version = self.online.borrow().version();
        let (size, zoom) = super::narrow_fit(&self.env, cx.viewport().screen);
        if version != self.seen || size != self.size || zoom != self.zoom {
            self.seen = version;
            let left = root.tween_time_left();
            *root = self.tree(cx);
            if let Some(actor) = cx.focused().and_then(|n| crate::hit::find_mut(root, n)) {
                crate::widgets::play(actor, "GainFocus");
            }
            super::relaid(root, left, cx);
        }
        if !self.env.reduced() {
            self.background_time = cx.clock();
        }
        if let Some(background) = crate::hit::find_mut(root, "MenuBackground") {
            self.background_animating = self.env.look.animate_component(
                "TitleMenu.Background",
                background,
                self.size,
                self.background_time,
            );
        }
        self.live.tick(root, "Wordmark", &self.env, cx);
        // What a dialog of its own said is the home's again once it is
        // gone.
        let mine: Vec<String> = self.focusables().into_iter().map(|f| f.name).collect();
        let st = self.env.state.borrow();
        let stale = st.screen != "multiplayer" || (!self.dialog.get() && st.page_items != mine);
        drop(st);
        if stale {
            self.publish();
        }
    }
}

/// A dialog of the home's, telling it when it is gone.
struct Watched<S: Screen> {
    inner: S,
    open: Rc<Cell<bool>>,
}

impl<S: Screen> Drop for Watched<S> {
    fn drop(&mut self) {
        self.open.set(false);
    }
}

impl<S: Screen> Screen for Watched<S> {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        self.inner.presentation()
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        self.inner.build(cx)
    }

    fn focusables(&self) -> Vec<Focusable> {
        self.inner.focusables()
    }

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
        self.inner.input(input, root, cx)
    }
}

/// Signing in with Twitch, asked for where it is needed: what it is for,
/// then where it stands (desktop's wait for the browser, completing, a
/// cancel or a failure with Try again). It closes itself once signed in;
/// what it was for goes on (a private room is made).
pub struct SignInDialog {
    env: Env,
    online: SharedOnline,
    dest: Dest,
    seen: u64,
    open: Rc<Cell<bool>>,
}

impl SignInDialog {
    pub fn new(env: Env, online: SharedOnline, dest: Dest, open: Rc<Cell<bool>>) -> Self {
        let seen = online.borrow().version();
        SignInDialog {
            env,
            online,
            dest,
            seen,
            open,
        }
    }

    /// What the dialog says, and its buttons (name, label, component).
    fn content(&self) -> (Vec<String>, Vec<(&'static str, String, &'static str)>) {
        let o = self.online.borrow();
        let intro = |dest: &Dest| -> String {
            match dest {
                Dest::Create => m::sign_in_for_private().into(),
                _ => m::sign_in_for_account().into(),
            }
        };
        let cancel = ("Cancel", common::cancel().into(), "Button");
        match o.sign_in_state() {
            SignIn::Idle => (
                vec![intro(&self.dest)],
                vec![cancel, ("Confirm", m::sign_in().into(), "PlayButton")],
            ),
            SignIn::Starting { .. } | SignIn::Completing { .. } => {
                (vec![m::sign_in_completing().into()], vec![cancel])
            }
            SignIn::Waiting { .. } => (
                vec![m::sign_in_waiting().into()],
                vec![
                    cancel,
                    ("Browser", m::sign_in_again_browser().into(), "Button"),
                ],
            ),
            SignIn::Canceled { dest } => (
                vec![m::sign_in_canceled().into(), intro(dest)],
                vec![cancel, ("Retry", m::sign_in_retry().into(), "PlayButton")],
            ),
            SignIn::Failed { dest, code } => (
                vec![m::sign_in_failed(UiText::data(code)).into(), intro(dest)],
                vec![cancel, ("Retry", m::sign_in_retry().into(), "PlayButton")],
            ),
        }
    }

    /// Stops signing in and closes.
    fn cancel(&self, cx: &mut Cx) {
        let mut o = self.online.borrow_mut();
        o.cancel_sign_in();
        o.clear_sign_in();
        drop(o);
        cx.close(true);
    }
}

impl Drop for SignInDialog {
    fn drop(&mut self) {
        self.open.set(false);
    }
}

impl Screen for SignInDialog {
    fn name(&self) -> &str {
        "sign-in"
    }

    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        Some(self.env.presentation())
    }

    fn build(&mut self, cx: &mut Cx) -> Actor {
        let vp = cx.viewport().screen;
        let (screen, scale) = super::narrow_fit(&self.env, vp);
        let env = self.env.clone();
        let (text, buttons) = self.content();
        let w = 520.0_f32.min(screen[0] - 32.0);
        let mut lines = Vec::new();
        for t in &text {
            lines.extend(wrap_lines(t, w - 48.0, 14.0));
        }
        let h = 160.0 + lines.len() as f32 * 18.0;
        let (mut c, [x, y]) = super::dialog::shell(&env, screen, [w, h], &m::sign_in_title());
        dialog_body(&env, &mut c, &lines, [x, y], w, |_| "Muted");
        let per_row = buttons.len();
        c.extend(button_rows(&env, &buttons, per_row, [x, y, w, h], 220.0));
        env.state.borrow_mut().page_items = buttons.iter().map(|b| b.0.to_string()).collect();
        let _ = scale;
        dialog_root(c)
    }

    fn focusables(&self) -> Vec<Focusable> {
        // The action first where there is one: Start goes on with it.
        let (_, buttons) = self.content();
        let mut names: Vec<&str> = buttons.iter().map(|b| b.0).collect();
        names.reverse();
        names.into_iter().map(Focusable::new).collect()
    }

    fn update(&mut self, _dt: f32, root: &mut Actor, cx: &mut Cx) {
        let (version, signed_in, busy) = {
            let o = self.online.borrow();
            (o.version(), o.me().is_some(), o.sign_in_state().busy())
        };
        if signed_in && !busy {
            self.open.set(false);
            cx.close(false);
            return;
        }
        if version != self.seen {
            self.seen = version;
            *root = self.build(cx);
            root.finish_tweening();
            let names: Vec<String> = self.focusables().into_iter().map(|f| f.name).collect();
            match cx.focused().filter(|f| names.iter().any(|n| n == f)) {
                Some(f) => super::restore_focus(root, Some(f)),
                None => {
                    if let Some(first) = names.into_iter().next() {
                        cx.focus(first);
                    }
                }
            }
        }
    }

    fn input(&mut self, input: &Input, _root: &mut Actor, cx: &mut Cx) -> Handled {
        match input {
            Input::Activate { name } => {
                match name.as_str() {
                    "Cancel" => self.cancel(cx),
                    "Confirm" => {
                        let mut o = self.online.borrow_mut();
                        match &self.dest {
                            Dest::Create => o.private_room(),
                            dest => o.sign_in(dest.clone()),
                        }
                    }
                    "Retry" => {
                        let mut o = self.online.borrow_mut();
                        let dest = match o.sign_in_state() {
                            SignIn::Canceled { dest } | SignIn::Failed { dest, .. } => dest.clone(),
                            _ => self.dest.clone(),
                        };
                        if dest == Dest::Create && o.me().is_none() {
                            o.private_room();
                        } else {
                            o.sign_in(dest);
                        }
                    }
                    "Browser" => self.online.borrow_mut().open_browser_again(),
                    _ => {}
                }
                Handled::Yes
            }
            Input::Outside
            | Input::Menu {
                action: MenuAction::Back,
                ..
            } => {
                self.cancel(cx);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}
