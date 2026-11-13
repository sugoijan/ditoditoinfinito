//! Calibration's menu at `#/calibrate` and the credits at `#/credits`
//! (`docs/plans/ui-core.md`, step 6.4): our theme's page
//! (`ddi_ui::screens::page::PageScreen`) on the app's canvas over the
//! whole window, as song select and options are.
//!
//! - **Calibration**: what each test does, a button to start it (the run
//!   is the play route's `#/calibrate/run?mode=…`, as before, with the
//!   testing parameters kept: `crate::router::testing`), the offsets of
//!   every device, and resetting them (asked first).
//! - **Credits**: `credits/credits.json` (`xtask gen-credits`) as
//!   `ddi_ui::screens::credits` lays it out; links open in a new tab from
//!   the press.

use std::cell::RefCell;
use std::rc::Rc;

use crate::component::{Component, Context};
use ddi_ui::screens::calibration::{self, CalAct};
use ddi_ui::screens::page::{PAGE_CHANGED, PageCall, PageDoc, PageFeed, PageScreen, PageShell};
use ddi_ui::{Cx, UiRequest};

use crate::components::ui_dev::{self, Host, UiPage};
use crate::router::Route;

/// Which page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Which {
    Start,
    Calibration,
    Credits,
    /// A shared link that does not exist or expired (`crate::share`):
    /// why, and the way to the start menu.
    SharedGone(ddi_library::share::ShareError),
    /// The multiplayer prototype's home (`#/mp`), behind the hidden
    /// switch, and an invite's page (`#/mp/join/<invite>`,
    /// `crate::online`). The rooms are song select's (M7b,
    /// `select_page::Props::room`).
    Multiplayer,
    Invite(String),
    /// A room watched with its audience (`#/mp/watch/<link>`,
    /// `crate::online::audience`).
    Audience(String),
}

#[derive(Clone, PartialEq)]
pub(crate) struct Props {
    pub(crate) which: Which,
}

pub(crate) enum Msg {
    Credits(Result<String, String>),
    Failed(String),
}

pub(crate) struct PagePage {
    ui: UiPage,
    feed: PageFeed,
    host: Rc<RefCell<Option<std::rc::Weak<RefCell<Host>>>>>,
}

impl Component for PagePage {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        if ctx.props().which == Which::Credits {
            ctx.link().send_future(async {
                Msg::Credits(crate::songs::fetch_text("credits/credits.json").await)
            });
        }
        PagePage {
            ui: UiPage::default(),
            feed: Rc::new(RefCell::new(None)),
            host: Rc::new(RefCell::new(None)),
        }
    }

    fn rendered(&mut self, ctx: &Context<Self>, first_render: bool) {
        if first_render {
            self.start(ctx);
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Credits(r) => {
                let doc = match r.and_then(|t| ddi_ui::screens::credits::parse(&t)) {
                    Ok(c) => {
                        ddi_ui::screens::credits::doc(&c, ddi_ui::screens::data::Platform::Web)
                    }
                    Err(e) => ddi_ui::screens::credits::waiting(Some(&e)),
                };
                send(&self.feed, &self.host, doc);
                false
            }
            Msg::Failed(e) => {
                self.ui.fail(e);
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) {
        self.ui.view(ctx, "select-page");
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.ui.destroy(&["ui"]);
    }
}

impl PagePage {
    fn start(&mut self, ctx: &Context<Self>) {
        let theme = ui_dev::theme_host(Rc::new(ui_dev::nfkd));
        let env = theme.env().clone();
        let which = ctx.props().which.clone();
        let (name, doc) = match &which {
            Which::Start => ("start", PageDoc::default()),
            Which::Calibration => ("calibration", calibration::stored_doc()),
            Which::Credits => ("credits", ddi_ui::screens::credits::waiting(None)),
            Which::SharedGone(why) => ("shared", shared_gone_doc(why)),
            Which::Multiplayer | Which::Invite(_) | Which::Audience(_) => {
                ("multiplayer", PageDoc::default())
            }
        };
        let shell: PageShell = {
            let env = env.clone();
            let feed = self.feed.clone();
            let which = which.clone();
            Rc::new(move |call, cx| match call {
                PageCall::Leave => {
                    cx.request(UiRequest::Back);
                    None
                }
                PageCall::Button(id) if which == Which::Calibration => {
                    calibration_button(&env, &feed, &id, cx)
                }
                PageCall::Button(id) if id == "home" => {
                    cx.request(UiRequest::Navigate(Route::Home.to_hash()));
                    None
                }
                PageCall::Button(_) | PageCall::Field { .. } => None,
            })
        };
        let screen = PageScreen::new(env.clone(), name, doc).with_shell(shell, self.feed.clone());
        let mut ui = crate::components::ui_dev::theme_ui();
        if which == Which::Start {
            ui.push(
                ddi_ui::screens::start::StartScreen::new(
                    theme.env().clone(),
                    false,
                    Rc::new(|choice, cx| {
                        use ddi_ui::screens::start::Choice;
                        let route = match choice {
                            Choice::Multiplayer => Route::Multiplayer,
                            Choice::Singleplayer => {
                                // Stay inside the activating input, before route mounting.
                                if cx.in_gesture() {
                                    crate::select_preview::gesture();
                                }
                                Route::Songs
                            }
                            Choice::Options => Route::Options,
                            Choice::Credits => Route::Credits,
                            Choice::Quit => return,
                        };
                        cx.request(UiRequest::Navigate(route.to_hash()));
                    }),
                )
                .with_focus(crate::router::saved_focus())
                // The multiplayer prototype's hidden switch (`#/?mp=1`).
                .with_multiplayer(crate::settings::Settings::load().multiplayer)
                // Under the loader, which shows the title already.
                .in_place(crate::boot::shown()),
            );
        } else if which == Which::Multiplayer {
            multiplayer_arrived();
            ui.push(online_screen(&env, OnlinePage::Home));
        } else if let Which::Invite(invite) = &which {
            multiplayer_arrived();
            ui.push(online_screen(&env, OnlinePage::Invite(invite.clone())));
        } else if let Which::Audience(link) = &which {
            multiplayer_arrived();
            ui.push(audience_screen(&env, link));
        } else {
            ui.push(screen);
        }
        let Some(started) = self.ui.start(ctx, ui, Some(theme), None, Msg::Failed) else {
            return;
        };
        *self.host.borrow_mut() = Some(Rc::downgrade(&started.0));
        // The credits may have come first.
        if self.feed.borrow().is_some() {
            let doc = self.feed.borrow_mut().take().expect("checked");
            send(&self.feed, &self.host, doc);
        }
    }
}

use ddi_ui::screens::online::{OnlineCall, OnlinePage, OnlineScreen};
use ddi_ui::state::online::Redirect;

/// A multiplayer route opened: a sign-in's return taken from its
/// address.
pub(crate) fn multiplayer_arrived() {
    let canceled = crate::router::query_param("signin").as_deref() == Some("canceled");
    crate::online::arrived(canceled);
}

/// Where signing in sends the page.
pub(crate) fn redirect(r: Redirect) {
    match r {
        Redirect::Page(path) => crate::online::leave_for_sign_in(&path),
        // Desktop's; the web leaves the page instead.
        Redirect::Browser(_) => {}
    }
}

/// The multiplayer home or an invite's page (`crate::online`).
fn online_screen(env: &ddi_ui::screens::Env, which: OnlinePage) -> OnlineScreen {
    OnlineScreen::new(
        env.clone(),
        crate::online::current(),
        which,
        Rc::new(|call, cx| match call {
            OnlineCall::Practice => cx.request(UiRequest::Navigate(Route::Room.to_hash())),
            OnlineCall::EnterRoom(id) => {
                cx.request(UiRequest::Navigate(Route::MpRoom { id }.to_hash()))
            }
            OnlineCall::OpenInvite(invite) => {
                cx.request(UiRequest::Navigate(Route::MpJoin { invite }.to_hash()))
            }
            OnlineCall::Watch { link, ticket } => {
                crate::online::seat_audience(&link, Some(ticket));
                cx.request(UiRequest::Navigate(Route::MpWatch { link }.to_hash()))
            }
            // In the press: the context the watched stages' music plays on.
            OnlineCall::WatchPressed => {
                if cx.in_gesture() {
                    crate::online::audience_gesture();
                }
            }
            OnlineCall::Redirect(r) => redirect(r),
            OnlineCall::Leave => cx.request(UiRequest::Back),
        }),
    )
}

/// A room watched with its audience (`crate::online::audience`): its page
/// between stages; its stages' songs read from the room's share
/// (`crate::room::prepare_audience`).
fn audience_screen(env: &ddi_ui::screens::Env, link: &str) -> AudienceScreen {
    AudienceScreen::new(
        env.clone(),
        crate::online::audience(link),
        Rc::new(|call, cx| match call {
            AudienceCall::Prepare(job) => crate::room::prepare_audience(job),
            // In the press: the context the stage's music plays on (made
            // and started there, as the invite page's Watch does).
            AudienceCall::WatchPressed => {
                if cx.in_gesture() {
                    crate::online::audience_gesture();
                }
            }
            AudienceCall::Watch(order) => crate::online::watch_stage(*order, cx),
            AudienceCall::Redirect(r) => redirect(r),
            AudienceCall::Leave => {
                crate::online::discard_audience();
                cx.request(UiRequest::Navigate(Route::Multiplayer.to_hash()))
            }
        }),
    )
}

use ddi_ui::screens::audience::{AudienceCall, AudienceScreen};

/// Puts `doc` in the feed and tells the screen once the host is free.
fn send(feed: &PageFeed, host: &Rc<RefCell<Option<std::rc::Weak<RefCell<Host>>>>>, doc: PageDoc) {
    *feed.borrow_mut() = Some(doc);
    let Some(weak) = host.borrow().clone() else {
        return;
    };
    ui_dev::broadcast_soon(weak, PAGE_CHANGED);
}

/// The page for a shared link that is gone: its one line and a button
/// to the start menu.
fn shared_gone_doc(why: &ddi_library::share::ShareError) -> PageDoc {
    use ddi_l10n::msg::errors as m;
    use ddi_ui::screens::page::Block;
    PageDoc {
        title: ddi_ui::screens::page::PageTitle::new(
            m::shared_page_title(),
            m::shared_page_heading(),
        ),
        blocks: vec![
            Block::Text(ddi_ui::screens::failures::share_load_text(why).into()),
            Block::Muted(m::shared_page_hint().into()),
            Block::Button {
                id: "home".into(),
                label: m::shared_page_home().into(),
                primary: true,
            },
        ],
    }
}

/// A calibration button: a run (with the session's testing parameters),
/// the options, or the reset, asked first.
fn calibration_button(
    env: &ddi_ui::screens::Env,
    feed: &PageFeed,
    id: &str,
    cx: &mut Cx,
) -> Option<PageDoc> {
    match calibration::button(id) {
        CalAct::Run(mode) => {
            let run = Route::CalibrateRun {
                mode,
                auto: false,
                bias_ms: 0,
            };
            cx.request(UiRequest::Navigate(crate::router::with_testing(
                run.to_hash(),
                &["auto", "bias"],
            )));
            None
        }
        CalAct::Options => {
            cx.request(UiRequest::Navigate(Route::Options.to_hash()));
            None
        }
        CalAct::ConfirmReset => {
            calibration::confirm_reset(env, feed, cx);
            None
        }
        CalAct::Nothing => None,
    }
}
