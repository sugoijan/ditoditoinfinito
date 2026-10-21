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

use crate::component::{Component, Context, NodeRef};
use ddi_ui::screens::calibration::{self, CalAct};
use ddi_ui::screens::page::{PAGE_CHANGED, PageCall, PageDoc, PageFeed, PageScreen, PageShell};
use ddi_ui::{Cx, TextMode, Ui, UiRequest};
use web_sys::HtmlElement;

use crate::components::ui_dev::{self, Host, Started};
use crate::router::Route;

/// Which page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Which {
    Start,
    Calibration,
    Credits,
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
    root: NodeRef,
    feed: PageFeed,
    host: Rc<RefCell<Option<std::rc::Weak<RefCell<Host>>>>>,
    started: Option<Started>,
    failure: Option<String>,
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
            root: NodeRef::default(),
            feed: Rc::new(RefCell::new(None)),
            host: Rc::new(RefCell::new(None)),
            started: None,
            failure: None,
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
                    Ok(c) => ddi_ui::screens::credits::doc(&c),
                    Err(e) => ddi_ui::screens::credits::waiting(Some(&e)),
                };
                send(&self.feed, &self.host, doc);
                false
            }
            Msg::Failed(e) => {
                self.failure = Some(e);
                if let Some((host, _, pads)) = &self.started {
                    ui_dev::stop_ui(host, pads.as_ref());
                }
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) {
        if let Some(e) = &self.failure {
            crate::component::no_gpu(ctx.root(), e);
            return;
        }
        ctx.root().set_class_name("select-page");
        self.root.set(ctx.root());
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        if let Some((host, _, pads)) = &self.started {
            ui_dev::stop_ui(host, pads.as_ref());
        }
        crate::web::forget_debug(&["ui"]);
    }
}

impl PagePage {
    fn start(&mut self, ctx: &Context<Self>) {
        let Some(el) = self.root.cast::<HtmlElement>() else {
            return;
        };
        let theme = ui_dev::theme_host(Rc::new(ui_dev::nfkd));
        let env = theme.env().clone();
        let which = ctx.props().which;
        let (name, doc) = match which {
            Which::Start => ("start", PageDoc::default()),
            Which::Calibration => ("calibration", calibration::stored_doc()),
            Which::Credits => ("credits", ddi_ui::screens::credits::waiting(None)),
        };
        let shell: PageShell = {
            let env = env.clone();
            let feed = self.feed.clone();
            Rc::new(move |call, cx| match call {
                PageCall::Leave => {
                    cx.request(UiRequest::Back);
                    None
                }
                PageCall::Button(id) if which == Which::Calibration => {
                    calibration_button(&env, &feed, &id, cx)
                }
                PageCall::Button(_) => None,
            })
        };
        let screen = PageScreen::new(env, name, doc).with_shell(shell, self.feed.clone());
        let mut ui = Ui::new(ddi_ui::screens::DESIGN, TextMode::Mirror, true);
        ui.set_focus_ring(false);
        if which == Which::Start {
            ui.push(
                ddi_ui::screens::start::StartScreen::new(
                    theme.env().clone(),
                    false,
                    Rc::new(|choice, cx| {
                        use ddi_ui::screens::start::Choice;
                        let route = match choice {
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
                .with_focus(crate::router::saved_focus()),
            );
        } else {
            ui.push(screen);
        }
        match ui_dev::start_ui(&el, ui, Some(theme), None) {
            Ok(started) => {
                *self.host.borrow_mut() = Some(Rc::downgrade(&started.0));
                let link = ctx.link().clone();
                ui_dev::attach_gfx(started.0.clone(), move |e| {
                    link.send_message(Msg::Failed(e))
                });
                self.started = Some(started);
                // The credits may have come first.
                if self.feed.borrow().is_some() {
                    let doc = self.feed.borrow_mut().take().expect("checked");
                    send(&self.feed, &self.host, doc);
                }
            }
            Err(e) => ctx.link().send_message(Msg::Failed(e)),
        }
    }
}

/// Puts `doc` in the feed and tells the screen once the host is free.
fn send(feed: &PageFeed, host: &Rc<RefCell<Option<std::rc::Weak<RefCell<Host>>>>>, doc: PageDoc) {
    *feed.borrow_mut() = Some(doc);
    let Some(weak) = host.borrow().clone() else {
        return;
    };
    ui_dev::broadcast_soon(weak, PAGE_CHANGED);
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
            let mut hash = format!("#/calibrate/run?mode={mode}");
            let testing: Vec<String> = crate::router::testing()
                .split('&')
                .filter(|kv| matches!(kv.split('=').next(), Some("auto" | "bias")))
                .map(str::to_string)
                .collect();
            if !testing.is_empty() {
                hash.push('&');
                hash.push_str(&testing.join("&"));
            }
            cx.request(UiRequest::Navigate(hash));
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
