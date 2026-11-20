//! The multiplayer home (`docs/plans/multiplayer-prototype-progress.md`,
//! M7a): a centered column of choices in the start menu's style, over the
//! start menu's outline rain, under the shared header and over the shared
//! footer. Every entry `docs/plans/multiplayer-ui.md` plans for the menu is
//! there: Find match and Browse rooms grayed out ("Coming soon", focusable
//! and inert), Private room, Join by link and, last, CPU practice (the
//! maintainer, 2026-11-14); the first key focuses Private room. The account sits at the footer's right: Sign in
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
/// The footer's link to the privacy note's page (`credits::privacy_url`).
pub const PRIVACY: &str = "Link.Privacy";
/// The account menu's pick.
const ACCOUNT_MENU: &str = "MultiplayerAccount";

/// An entry of the column: its id, its label, whether it does something
/// yet (a grayed one is focusable and inert).
fn entries() -> [(&'static str, UiText, bool); 5] {
    [
        (FIND, m::home_find(), false),
        (BROWSE, m::home_browse(), false),
        (PRIVATE, m::home_private(), true),
        (JOIN, m::home_join(), true),
        // Last (the maintainer, 2026-11-14).
        (PRACTICE, m::home_practice(), true),
    ]
}

/// Why a private room could not be made, a sentence: the server's
/// operator's switches and caps each in their own words (round D1), any
/// other reason with its code.
pub fn create_problem(code: &str) -> UiText {
    match code {
        // New rooms are switched off for now.
        "new_rooms_paused" => m::create_paused(),
        // The server holds as many rooms as it may.
        "server_busy" => m::create_busy(),
        // This person hosts as many rooms as one person may at once.
        "too_many_rooms" => m::create_too_many(),
        code => m::create_failed(UiText::data(code)),
    }
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
    /// Since when the room being made has been asked for (screen time),
    /// and whether its line shows: only once it takes a moment
    /// ([`CREATING_AFTER`]), so a quick answer flashes nothing.
    creating_since: Option<f64>,
    creating_shown: bool,
}

/// How long making a room takes before the home says so.
const CREATING_AFTER: f64 = 0.6;

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
            creating_since: None,
            creating_shown: false,
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
            if self.creating_shown {
                v.push(m::creating().into());
            }
        } else if let Some(p) = o.create_problem() {
            v.push(create_problem(&p.code).into());
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
        // Centered on the column alone, so a line coming or going under
        // it (the room being made) moves nothing; lifted only where the
        // lines would not fit above the footer.
        let column_h = n * row + (n - 1.0) * gap;
        let top = (head + ((h - head - foot - column_h) / 2.0).max(8.0))
            .min(h - foot - 12.0 - column_h - status_h)
            .max(head + 8.0);
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
        let privacy = String::from(pm::privacy());
        let mut links: Vec<(&str, &str)> = vec![(PRIVACY, privacy.as_str())];
        links.extend(account.iter().map(|a| (ACCOUNT, a.as_str())));
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
        if name == PRIVACY {
            cx.request(crate::UiRequest::OpenUrl(super::credits::privacy_url()));
            return;
        }
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

    /// The first key focuses Private room, the first entry that opens
    /// something (Find match and Browse rooms are Coming soon).
    fn first_focus(&self) -> Option<String> {
        Some(choice(PRIVATE))
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
        names.push(PRIVACY.into());
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
        let now = cx.wall_ms();
        let (redirect, entered) = {
            let mut o = self.online.borrow_mut();
            o.poll(now);
            (o.take_redirect(), o.take_entered())
        };
        if let Some(r) = redirect {
            (self.shell)(OnlineCall::Redirect(r), cx);
        }
        let entering = entered.is_some();
        if let Some(room) = entered {
            (self.shell)(OnlineCall::EnterRoom(room), cx);
        }
        if !cx.is_current_screen() {
            return;
        }
        let creating = self.online.borrow().creating();
        let mut show_creating = false;
        if creating {
            let since = *self.creating_since.get_or_insert(self.time);
            show_creating = self.time - since >= CREATING_AFTER;
        } else {
            self.creating_since = None;
        }
        // The room made: the home leaves as it stands, the line with it
        // (rebuilt, it would drop the line as the exit begins).
        if entering {
            self.seen = self.online.borrow().version();
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
        if version != self.seen
            || size != self.size
            || zoom != self.zoom
            || (show_creating != self.creating_shown && !entering)
        {
            self.seen = version;
            self.creating_shown = show_creating;
            // At rest it stands as it is (a pressed button's glow is no
            // entrance to go on with).
            let left = if cx.entering() {
                root.tween_time_left()
            } else {
                0.0
            };
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
    /// A trip through Twitch for the person's Twitch access (round TW3),
    /// not a sign-in.
    connect: Option<Connect>,
    /// When the code was last copied (the UI's clock): its plate says so
    /// for [`COPIED_FOR`] seconds.
    copied: Option<f64>,
}

/// The sign-in code's plate (round SF): pressed, the code is copied.
pub const CODE: &str = "Code";
/// How long the code's plate says it was copied, in seconds.
const COPIED_FOR: f64 = 2.0;
/// The code's plate's height.
const CODE_H: f32 = 64.0;

/// A connecting [`SignInDialog`]'s trip: the scope sets asked (`chat`,
/// `bans,blocks`…) and what is told when it ends (connected or not).
struct Connect {
    scopes: String,
    done: Option<ConnectDone>,
}

/// Told once when a connecting dialog's trip ends: whether the access now
/// grants every scope set it asked.
pub type ConnectDone = Rc<dyn Fn(bool, &mut Cx)>;

impl SignInDialog {
    pub fn new(env: Env, online: SharedOnline, dest: Dest, open: Rc<Cell<bool>>) -> Self {
        let seen = online.borrow().version();
        SignInDialog {
            env,
            online,
            dest,
            seen,
            open,
            connect: None,
            copied: None,
        }
    }

    /// The attempt's code while the browser's page waits for it (shown
    /// grouped, as typed there).
    fn code(&self) -> Option<String> {
        match self.online.borrow().sign_in_state() {
            SignIn::Waiting { code: Some(c), .. } => Some(crate::state::online::api::grouped_code(c)),
            _ => None,
        }
    }

    /// The code's plate (round SF): the code large in the theme's face,
    /// the length of the dialog's body, centered at `[cx, cy]`, `w`
    /// wide; at its right end, small, Copy (or Copied a moment after).
    fn code_plate(&self, code: &str, [cx, cy]: [f32; 2], w: f32) -> Vec<Actor> {
        let env = &self.env;
        let plate = env.component("Button", "Button", &Spec::new([w, CODE_H], ""), CODE, [cx, cy]);
        let shown: String = UiText::data(code).into();
        let mut text = look::tracked(env.text("Code.Text", &shown, 34.0, 700, "Text"), 160);
        call(&mut text, "xy", &[cx, cy]);
        text.decoration = true;
        let hint: String = self.copy_words().into();
        let mut hint = super::right(env.text("Code.Hint", &hint, 12.0, 600, "Muted"), cx + w / 2.0 - 14.0, cy);
        hint.decoration = true;
        vec![plate, text, hint]
    }

    /// The plate's small words: Copy, or Copied a moment after.
    fn copy_words(&self) -> UiText {
        if self.copied.is_some() {
            m::sign_in_code_copied()
        } else {
            m::sign_in_code_copy()
        }
    }

    /// Desktop's trip through Twitch for the person's Twitch access
    /// (round TW3: a room's Host options, Options' "Twitch chat in
    /// rooms"), already started (`Online::connect_twitch_for`): where it
    /// stands, the attempt's code to type in the browser's page, Cancel,
    /// Try again. It runs the online state itself (Options has no room
    /// under it to), opens the browser, and closes once the trip is over,
    /// telling `done` whether the access grants `scopes`.
    pub fn connecting(
        env: Env,
        online: SharedOnline,
        dest: Dest,
        scopes: &str,
        done: Option<ConnectDone>,
    ) -> Self {
        let mut d = SignInDialog::new(env, online, dest, Rc::new(Cell::new(true)));
        d.connect = Some(Connect {
            scopes: scopes.into(),
            done,
        });
        d
    }

    /// Whether the access grants what a connecting dialog asked.
    fn granted(&self) -> bool {
        let Some(c) = &self.connect else {
            return true;
        };
        let o = self.online.borrow();
        let Some(t) = o.twitch() else {
            return false;
        };
        c.scopes
            .split(',')
            .all(|s| t.features.iter().any(|f| f == s.trim()))
    }

    /// A connecting dialog's trip ended: told once.
    fn finish(&mut self, connected: bool, cx: &mut Cx) {
        if let Some(done) = self.connect.as_mut().and_then(|c| c.done.take()) {
            done(connected, cx);
        }
    }

    /// What the dialog says, and its buttons (name, label, component).
    fn content(&self) -> (Vec<String>, Vec<(&'static str, String, &'static str)>) {
        if self.connect.is_some() {
            return self.connecting_content();
        }
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
            SignIn::Waiting { code, .. } => (
                code.iter()
                    .map(|_| m::sign_in_code_type().into())
                    .chain([m::sign_in_waiting().into()])
                    .collect(),
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
                vec![super::online::sign_in_failed(code).into(), intro(dest)],
                vec![cancel, ("Retry", m::sign_in_retry().into(), "PlayButton")],
            ),
        }
    }

    /// A connecting dialog's words and buttons.
    fn connecting_content(&self) -> (Vec<String>, Vec<(&'static str, String, &'static str)>) {
        let o = self.online.borrow();
        let cancel = ("Cancel", common::cancel().into(), "Button");
        let retry = ("Retry", m::sign_in_retry().into(), "PlayButton");
        match o.sign_in_state() {
            SignIn::Idle | SignIn::Starting { .. } | SignIn::Completing { .. } => {
                (vec![m::twitch_connecting().into()], vec![cancel])
            }
            SignIn::Waiting { code, .. } => (
                code.iter()
                    .map(|_| m::sign_in_code_type().into())
                    .chain([m::twitch_connect_waiting().into()])
                    .collect(),
                vec![
                    cancel,
                    ("Browser", m::sign_in_again_browser().into(), "Button"),
                ],
            ),
            SignIn::Canceled { .. } => (vec![m::twitch_not_connected().into()], vec![cancel, retry]),
            SignIn::Failed { code, .. } => (
                vec![super::online::sign_in_failed(code).into()],
                vec![cancel, retry],
            ),
        }
    }

    /// Stops signing in and closes.
    fn cancel(&mut self, cx: &mut Cx) {
        let mut o = self.online.borrow_mut();
        o.cancel_sign_in();
        // A connecting trip's cancel is kept (a room's Twitch setting
        // asked says nothing changed); a sign-in's forgotten.
        if self.connect.is_none() {
            o.clear_sign_in();
        }
        drop(o);
        self.finish(false, cx);
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
        // The code, where there is one, on a plate of its own after the
        // first line (the line that says to type it).
        let code = self.code();
        let mut lines = Vec::new();
        let mut before = 0;
        for (i, t) in text.iter().enumerate() {
            lines.extend(wrap_lines(t, w - 48.0, 14.0));
            if i == 0 && code.is_some() {
                before = lines.len();
            }
        }
        let plate = if code.is_some() { CODE_H + 28.0 } else { 0.0 };
        let h = 160.0 + lines.len() as f32 * 18.0 + plate;
        let title = if self.connect.is_some() {
            ddi_l10n::msg::twitch::access()
        } else {
            m::sign_in_title()
        };
        let (mut c, [x, y]) = super::dialog::shell(&env, screen, [w, h], &title);
        match &code {
            Some(code) => {
                let (top, rest) = lines.split_at(before);
                dialog_body(&env, &mut c, top, [x, y], w, |_| "Muted");
                let cy = y + 66.0 + (top.len() as f32 - 0.5) * 18.0 + 10.0 + CODE_H / 2.0;
                c.extend(self.code_plate(code, [x + w / 2.0, cy], w - 48.0));
                for (i, line) in rest.iter().enumerate() {
                    let mut b = env.text(&format!("Body{}", before + i), line, 14.0, 400, "Muted");
                    look::ellipsized(&mut b, w - 48.0);
                    c.push(super::left(b, x + 24.0, y + 66.0 + plate + (before + i) as f32 * 18.0));
                }
            }
            None => dialog_body(&env, &mut c, &lines, [x, y], w, |_| "Muted"),
        }
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
        // The code's plate last: the first focus stays the dialog's action
        // (Open the browser again); the arrows reach the plate over it.
        if self.code().is_some() {
            names.push(CODE);
        }
        names.into_iter().map(Focusable::new).collect()
    }

    fn update(&mut self, _dt: f32, root: &mut Actor, cx: &mut Cx) {
        if self.connect.is_some() {
            // Nothing under Options runs the online state: this does (a
            // room's link under it polls too, harmlessly), and opens the
            // browser on the attempt.
            let redirect = {
                let mut o = self.online.borrow_mut();
                o.poll(cx.wall_ms());
                o.take_redirect()
            };
            if let Some(crate::state::online::Redirect::Browser(url)) = redirect {
                cx.request(crate::UiRequest::OpenUrl(url));
            }
        }
        let (version, signed_in, busy) = {
            let o = self.online.borrow();
            (o.version(), o.me().is_some(), o.sign_in_state().busy())
        };
        // Copied a moment ago: said once, then Copy again.
        if self.copied.is_some_and(|t| cx.time() - t >= COPIED_FOR) {
            self.copied = None;
            let words: String = self.copy_words().into();
            crate::widgets::set_text(root, "Code.Hint", &words);
        }
        let settled = matches!(self.online.borrow().sign_in_state(), SignIn::Idle);
        if signed_in && !busy && (self.connect.is_none() || settled) {
            let granted = self.granted();
            self.finish(granted, cx);
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

    fn input(&mut self, input: &Input, root: &mut Actor, cx: &mut Cx) -> Handled {
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
                    "Retry" if self.connect.is_some() => {
                        let scopes = self.connect.as_ref().map(|c| c.scopes.clone());
                        let mut o = self.online.borrow_mut();
                        o.connect_twitch(self.dest.clone(), &scopes.unwrap_or_default());
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
                    // The code to the clipboard, as shown (the page takes
                    // it with or without its dash); Copied in place, its
                    // moment begun again by another press, never twice.
                    CODE => {
                        if let Some(code) = self.code() {
                            cx.request(crate::UiRequest::CopyText(code));
                            let was = self.copied.replace(cx.time());
                            if was.is_none() {
                                let words: String = self.copy_words().into();
                                crate::widgets::set_text(root, "Code.Hint", &words);
                            }
                        }
                    }
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
