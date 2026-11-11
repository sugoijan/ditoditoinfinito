//! The multiplayer home, the join-by-link page and an invite's page with
//! online rooms (`docs/plans/multiplayer-prototype-progress.md`, M5c,
//! M7a). The home is a column of choices in the start menu's style
//! ([`super::online_home`]); the others are pages ([`PageScreen`]) made
//! from the [`Online`] state and made again as it changes. Join by link
//! has a field for an invite link; an invite's page shows the room it is
//! for before signing in (its name, host, whether a newcomer can join
//! now) and joins it. Signing in on desktop waits on the page while the
//! system browser signs in.
//!
//! The shell carries the rest ([`OnlineCall`]): practice, a room made or
//! joined, the server's sign-in page on the web or the system browser on
//! desktop, an invite opened, Back.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_l10n::UiText;
use ddi_l10n::msg::room as m;

use super::Env;
use super::page::{
    Action, Block, PAGE_CHANGED, PageCall, PageDoc, PageFeed, PageScreen, PageTitle,
};
use crate::screen::{Cx, Handled, Input, Screen};
use crate::state::online::api::{self, Admission, Problem};
use crate::state::online::{Account, Dest, Online, Redirect, SharedOnline, SignIn};

/// Which page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OnlinePage {
    /// The multiplayer home.
    Home,
    /// A field to paste an invite link into (pushed over the home).
    JoinLink,
    /// An invite's page.
    Invite(String),
}

/// What the pages ask of their shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OnlineCall {
    /// CPU practice.
    Practice,
    /// A room made or joined: show it.
    EnterRoom(String),
    /// Go where signing in needs: the server's page (web) or the system
    /// browser (desktop).
    Redirect(Redirect),
    /// Open this invite's page (desktop's field).
    OpenInvite(String),
    /// Back.
    Leave,
}

pub type OnlineShell = Rc<dyn Fn(OnlineCall, &mut Cx)>;

pub use super::online_home::PRACTICE;

/// What the page keeps beside the online state: the invite field's text
/// and whether what was opened from it was no invite.
#[derive(Default)]
struct Draft {
    invite: String,
    not_one: bool,
}

/// The page for `which` as `online` stands.
fn doc(which: &OnlinePage, online: &Online, draft: &Draft) -> PageDoc {
    match which {
        OnlinePage::Home | OnlinePage::JoinLink => join_doc(draft),
        OnlinePage::Invite(_) => invite_doc(online),
    }
}

fn account_block(online: &Online) -> Block {
    Block::Muted(
        match online.account() {
            Account::SignedIn(me) => m::account_signed_in(UiText::data(&me.display_name)),
            Account::SignedOut => m::account_signed_out(),
            Account::Unknown | Account::Checking => m::account_checking(),
            Account::Unreachable => m::account_unreachable(),
        }
        .into(),
    )
}

/// Signing in, where it stands (nothing when idle); `retry` offers Try
/// again after a cancel or a failure (an invite's page has its own Join).
fn sign_in_blocks(online: &Online, blocks: &mut Vec<Block>, retry: bool) {
    match online.sign_in_state() {
        SignIn::Idle => {}
        SignIn::Starting { .. } | SignIn::Completing { .. } => {
            blocks.push(Block::Note(m::sign_in_completing().into()));
        }
        SignIn::Waiting { .. } => {
            blocks.push(Block::Note(m::sign_in_waiting().into()));
            blocks.push(Block::Actions(vec![
                Action::new("browser", m::sign_in_again_browser()),
                Action::new("cancel", m::sign_in_cancel()),
            ]));
        }
        SignIn::Canceled { .. } => {
            blocks.push(Block::Note(m::sign_in_canceled().into()));
            if retry {
                blocks.push(Block::Actions(vec![Action::new(
                    "retry",
                    m::sign_in_retry(),
                )]));
            }
        }
        SignIn::Failed { code, .. } => {
            blocks.push(Block::Note(m::sign_in_failed(UiText::data(code)).into()));
            if retry {
                blocks.push(Block::Actions(vec![Action::new(
                    "retry",
                    m::sign_in_retry(),
                )]));
            }
        }
    }
}

/// Join by link: the field, Open invite, and what was pasted when it was
/// no invite.
fn join_doc(draft: &Draft) -> PageDoc {
    let mut blocks = vec![
        Block::Field {
            id: "invite".into(),
            label: m::invite_field().into(),
            value: draft.invite.clone(),
            hint: m::invite_field_hint().into(),
        },
        Block::Actions(vec![Action::new("invite-open", m::invite_go()).primary()]),
    ];
    if draft.not_one {
        blocks.push(Block::Note(m::invite_not_one().into()));
    }
    PageDoc {
        title: PageTitle::new(m::join_title(), m::join_heading()),
        blocks,
    }
}

/// Why an invite does not admit, or a join was refused, worded.
pub fn invite_problem(p: &Problem) -> UiText {
    match p.code.as_str() {
        "invite_expired" => m::invite_expired(),
        "invite_rotated" => m::invite_rotated(),
        "invite_unknown" | "invite_room" | "invite_missing" => m::invite_unknown(),
        "room_closed" | "room_unknown" => m::invite_room_closed(),
        "admission_closed" => m::invite_closed(),
        "room_full" => m::invite_full(),
        "unreachable" => m::invite_unreachable(),
        code => m::invite_refused(UiText::data(code)),
    }
}

/// What joining now means, from the preview's admission.
pub fn admission_text(a: Admission) -> UiText {
    match a {
        Admission::Open => m::invite_open(),
        Admission::NextMatch => m::invite_next_match(),
        Admission::Closed => m::invite_closed(),
        Admission::Full => m::invite_full(),
    }
}

/// An invite's page.
fn invite_doc(online: &Online) -> PageDoc {
    let mut blocks = Vec::new();
    let view = online.invite();
    let mut joinable = false;
    match view.and_then(|v| v.preview.as_ref()) {
        None => blocks.push(Block::Text(m::invite_reading().into())),
        Some(Ok(p)) => {
            blocks.push(Block::Subheading(UiText::data(&p.room_name).into()));
            blocks.push(Block::Text(
                m::invite_host(super::room::shown_name(&p.host_display_name, p.host_bot)).into(),
            ));
            blocks.push(Block::Muted(m::invite_members(p.members).into()));
            blocks.push(Block::Text(admission_text(p.admission).into()));
            joinable = matches!(p.admission, Admission::Open | Admission::NextMatch);
        }
        Some(Err(p)) => blocks.push(Block::Note(invite_problem(p).into())),
    }
    if let Some(p) = view.and_then(|v| v.refused.as_ref()) {
        blocks.push(Block::Note(invite_problem(p).into()));
    }
    blocks.push(Block::Gap);
    blocks.push(account_block(online));
    sign_in_blocks(online, &mut blocks, !joinable);
    let mut actions = Vec::new();
    if view.is_some_and(|v| v.joining) {
        blocks.push(Block::Note(m::invite_joining().into()));
    } else if joinable && !online.sign_in_state().busy() {
        actions.push(if online.me().is_some() {
            Action::new("join", m::invite_join()).primary()
        } else {
            Action::new("join", m::invite_sign_in()).primary()
        });
    }
    actions.push(Action::new("back", ddi_l10n::msg::common::back()));
    blocks.push(Block::Actions(actions));
    PageDoc {
        title: PageTitle::new(m::invite_title(), m::invite_heading()),
        blocks,
    }
}

/// Join by link or an invite's page, over the online state.
struct OnlinePageScreen {
    page: PageScreen,
    online: SharedOnline,
    which: OnlinePage,
    feed: PageFeed,
    draft: Rc<RefCell<Draft>>,
    shell: OnlineShell,
    seen: u64,
    time: f64,
}

impl OnlinePageScreen {
    fn new(env: Env, online: SharedOnline, which: OnlinePage, shell: OnlineShell) -> Self {
        {
            let mut o = online.borrow_mut();
            match &which {
                OnlinePage::Home | OnlinePage::JoinLink => {}
                OnlinePage::Invite(invite) => {
                    if o.invite().is_none_or(|v| &v.invite != invite) {
                        o.open_invite(invite);
                    }
                }
            }
        }
        let draft = Rc::new(RefCell::new(Draft::default()));
        let feed: PageFeed = Rc::new(RefCell::new(None));
        let first = doc(&which, &online.borrow(), &draft.borrow());
        let name = match which {
            OnlinePage::Home | OnlinePage::JoinLink => "join-link",
            OnlinePage::Invite(_) => "invite",
        };
        let page_shell: super::page::PageShell = {
            let online = online.clone();
            let shell = shell.clone();
            let draft = draft.clone();
            let which = which.clone();
            Rc::new(move |call, cx| {
                let mut o = online.borrow_mut();
                match call {
                    // Join by link is over the home: back to it.
                    PageCall::Leave if !matches!(which, OnlinePage::Invite(_)) => {
                        cx.close(true);
                        return None;
                    }
                    PageCall::Leave => {
                        drop(o);
                        shell(OnlineCall::Leave, cx);
                        return None;
                    }
                    PageCall::Field { id, text } if id == "invite" => {
                        let mut d = draft.borrow_mut();
                        d.invite = text;
                        d.not_one = false;
                        return None;
                    }
                    PageCall::Field { .. } => return None,
                    PageCall::Button(id) => match id.as_str() {
                        PRACTICE => {
                            drop(o);
                            shell(OnlineCall::Practice, cx);
                            return None;
                        }
                        "private" => o.private_room(),
                        "signin" => o.sign_in(match &which {
                            OnlinePage::Home | OnlinePage::JoinLink => Dest::Home,
                            OnlinePage::Invite(i) => Dest::Join(i.clone()),
                        }),
                        "signout" => o.sign_out(),
                        "browser" => o.open_browser_again(),
                        "cancel" => o.cancel_sign_in(),
                        "retry" => {
                            let dest = match o.sign_in_state() {
                                SignIn::Canceled { dest } | SignIn::Failed { dest, .. } => {
                                    dest.clone()
                                }
                                _ => Dest::Home,
                            };
                            o.sign_in(dest);
                        }
                        "join" => o.join_invite(),
                        "invite-open" => {
                            let found = api::invite_in(&draft.borrow().invite);
                            match found {
                                Some(invite) => {
                                    drop(o);
                                    shell(OnlineCall::OpenInvite(invite), cx);
                                    return None;
                                }
                                None => draft.borrow_mut().not_one = true,
                            }
                        }
                        "back" => {
                            drop(o);
                            shell(OnlineCall::Leave, cx);
                            return None;
                        }
                        _ => return None,
                    },
                }
                // What the press did, said at once (redirects and rooms
                // are handed on in the next update).
                Some(doc(&which, &o, &draft.borrow()))
            })
        };
        let page = PageScreen::new(env, name, first).with_shell(page_shell, feed.clone());
        let seen = online.borrow().version();
        OnlinePageScreen {
            page,
            online,
            which,
            feed,
            draft,
            shell,
            seen,
            time: 0.0,
        }
    }
}

impl Screen for OnlinePageScreen {
    fn shared_bars(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        self.page.name()
    }

    fn build(&mut self, cx: &mut Cx) -> ddi_actor::Actor {
        self.page.build(cx)
    }

    fn input(&mut self, input: &Input, root: &mut ddi_actor::Actor, cx: &mut Cx) -> Handled {
        self.page.input(input, root, cx)
    }

    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        self.page.presentation()
    }

    fn active_element(&self, focused: Option<&str>) -> Option<String> {
        self.page.active_element(focused)
    }

    fn cursor(&self, target: &crate::hit::Target) -> Option<crate::request::Cursor> {
        self.page.cursor(target)
    }

    fn animating(&self) -> bool {
        self.page.animating()
    }

    fn wake_at(&self) -> Option<f64> {
        // Answers come while nothing moves: looked for ten times a second.
        Some(self.time + 0.1)
    }

    fn focusables(&self) -> Vec<crate::nav::Focusable> {
        self.page.focusables()
    }

    fn message(&mut self, msg: &crate::Message, root: &mut ddi_actor::Actor, cx: &mut Cx) {
        self.page.message(msg, root, cx);
    }

    fn update(&mut self, dt: f32, root: &mut ddi_actor::Actor, cx: &mut Cx) {
        self.time = cx.time();
        self.page.update(dt, root, cx);
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
        let version = self.online.borrow().version();
        if version != self.seen {
            self.seen = version;
            let d = doc(&self.which, &self.online.borrow(), &self.draft.borrow());
            *self.feed.borrow_mut() = Some(d);
            self.page
                .message(&crate::Message::new(PAGE_CHANGED), root, cx);
        }
    }
}

/// The multiplayer home ([`super::online_home::Home`]), Join by link or an
/// invite's page, over the online state.
pub struct OnlineScreen {
    inner: Box<dyn Screen>,
}

impl OnlineScreen {
    pub fn new(env: Env, online: SharedOnline, which: OnlinePage, shell: OnlineShell) -> Self {
        let inner: Box<dyn Screen> = match which {
            OnlinePage::Home => Box::new(super::online_home::Home::new(env, online, shell)),
            which => Box::new(OnlinePageScreen::new(env, online, which, shell)),
        };
        OnlineScreen { inner }
    }
}

impl Screen for OnlineScreen {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn build(&mut self, cx: &mut Cx) -> ddi_actor::Actor {
        self.inner.build(cx)
    }

    fn input(&mut self, input: &Input, root: &mut ddi_actor::Actor, cx: &mut Cx) -> Handled {
        self.inner.input(input, root, cx)
    }

    fn presentation(&self) -> Option<crate::look::Presentation<'_>> {
        self.inner.presentation()
    }

    fn active_element(&self, focused: Option<&str>) -> Option<String> {
        self.inner.active_element(focused)
    }

    fn cursor(&self, target: &crate::hit::Target) -> Option<crate::request::Cursor> {
        self.inner.cursor(target)
    }

    fn animating(&self) -> bool {
        self.inner.animating()
    }

    fn wake_at(&self) -> Option<f64> {
        self.inner.wake_at()
    }

    fn defer_initial_focus(&self) -> bool {
        self.inner.defer_initial_focus()
    }

    fn shared_bars(&self) -> bool {
        self.inner.shared_bars()
    }

    fn focusables(&self) -> Vec<crate::nav::Focusable> {
        self.inner.focusables()
    }

    fn message(&mut self, msg: &crate::Message, root: &mut ddi_actor::Actor, cx: &mut Cx) {
        self.inner.message(msg, root, cx);
    }

    fn update(&mut self, dt: f32, root: &mut ddi_actor::Actor, cx: &mut Cx) {
        self.inner.update(dt, root, cx);
    }
}
