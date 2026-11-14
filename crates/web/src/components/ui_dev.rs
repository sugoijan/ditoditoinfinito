//! The UI core's developer page, `#/ui-dev`, not linked from the menus:
//! the developer screen (`ddi_ui::dev`) on the app's one canvas over the
//! whole window, through the pages' host (`crate::ui_host`).
//!
//! `?screen=select|results|options` runs the theme's screens instead
//! (`ddi_ui::screens` in `ddi_theme::Infinito`, step 5) on their sample
//! data, at the theme's design size (1280×720; a window under
//! `Compact.Below` CSS pixels high gets the compact design), with the
//! theme's text face registered from its files, reduced motion from the
//! settings and `prefers-reduced-motion`, and what they did in
//! `__DDI_DEBUG.ui.theme`.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::component::{Component, Context, NodeRef};
use ddi_ui::dev::{self, DevScreen, Shared};
use ddi_ui::dev_widgets::{FoldFn, LIBRARY_MESSAGE, LibrarySong, SharedWidgets, WidgetScreen};
use ddi_ui::screens::{self as theme_screens, sample};
use ddi_ui::{Message, TextMode, Ui};
use gloo::events::EventListener;
use web_sys::HtmlElement;

use crate::ui_host::{
    Host, Started, drop_field, keep_source, publish, schedule, start_ui, theme_host_with, theme_ui,
};
use crate::web::display::system_reduced;
use crate::web::gamepad::Gamepads;
use crate::web::gfx::SurfaceMode;
use crate::web::screen;
use crate::web::text::nfkd;

#[derive(Clone, Default, PartialEq)]
pub(crate) struct Props {
    /// `widgets`: the widget screen at once.
    pub(crate) screen: String,
}

pub(crate) struct UiDev {
    host: Option<Rc<RefCell<Host>>>,
    listeners: Vec<EventListener>,
    gamepads: Option<Gamepads>,
    root: NodeRef,
    failure: Option<String>,
}

pub(crate) enum Msg {
    Failed(String),
}

/// Whether `screen` is one of the theme's.
fn theme_screen(screen: &str) -> bool {
    matches!(
        screen,
        "start" | "select" | "results" | "options" | "credits" | "calibration" | "prompt"
    )
}

impl Component for UiDev {
    type Message = Msg;
    type Properties = Props;

    fn create(_ctx: &Context<Self>) -> Self {
        UiDev {
            host: None,
            listeners: Vec::new(),
            gamepads: None,
            root: NodeRef::default(),
            failure: None,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Msg) -> bool {
        match msg {
            Msg::Failed(e) => {
                // The no-GPU page is a plain page: its keys and its Back are
                // the browser's.
                self.failure = Some(e);
                self.listeners.clear();
                if let Some(pads) = &self.gamepads {
                    pads.set_listener(None);
                }
                if let Some(host) = &self.host
                    && let Ok(mut h) = host.try_borrow_mut()
                {
                    h.closed = true;
                    h.raf = None;
                }
            }
        }
        true
    }

    fn rendered(&mut self, ctx: &Context<Self>, first_render: bool) {
        if !first_render {
            return;
        }
        let Some(el) = self.root.cast::<HtmlElement>() else {
            return;
        };
        match start(&el, &ctx.props().screen) {
            Ok((host, listeners, pads)) => {
                self.host = Some(host.clone());
                self.listeners = listeners;
                self.gamepads = pads;
                let link = ctx.link().clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match screen::gfx().await {
                        Ok(gfx) => {
                            let Ok(mut h) = host.try_borrow_mut() else {
                                return;
                            };
                            if h.closed {
                                return;
                            }
                            gfx.set_mode(SurfaceMode::Plain);
                            h.gfx = Some(gfx);
                            drop(h);
                            schedule(&host);
                        }
                        Err(e) => link.send_message(Msg::Failed(e)),
                    }
                });
                load_library(Rc::downgrade(self.host.as_ref().expect("host")));
            }
            Err(e) => ctx.link().send_message(Msg::Failed(e)),
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.listeners.clear();
        if let Some(pads) = &self.gamepads {
            pads.set_listener(None);
        }
        if let Some(host) = &self.host
            && let Ok(mut h) = host.try_borrow_mut()
        {
            keep_source(h.ui.hints().source().clone());
            h.closed = true;
            h.raf = None;
            h.attachment = None;
            let _ = h.textarea.blur();
            drop_field(&h);
            if let Some(gfx) = h.gfx.take() {
                gfx.set_mode(SurfaceMode::Scene);
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) {
        if let Some(e) = &self.failure {
            crate::component::no_gpu(ctx.root(), e);
            return;
        }
        ctx.root().set_class_name("ui-dev");
        self.root.set(ctx.root());
    }
}

/// Makes the host: the canvas in its frame, the hidden textarea and file
/// input, the listeners.
fn start(el: &HtmlElement, first: &str) -> Result<Started, String> {
    let dev: Shared = Shared::default();
    let widgets = SharedWidgets::default();
    let fold: FoldFn = Rc::new(nfkd);
    let mut theme = None;
    let ui = if theme_screen(first) {
        let t = theme_host_with(fold.clone());
        let env = &t.env;
        let mut ui = theme_ui();
        ui.push(sample::start_screen(env, false));
        if first != "start" {
            ui.push(sample::select_screen(
                env,
                Some(sample::motion_change(env, system_reduced)),
            ));
        }
        match first {
            "results" => ui.push(sample::results_screen(env)),
            "options" => ui.push(sample::options_screen(
                env,
                Some(sample::motion_change(env, system_reduced)),
            )),
            "credits" => ui.push(sample::credits_screen(
                env,
                &theme_screens::credits::sample(),
            )),
            "calibration" => ui.push(sample::calibration_screen(env)),
            "prompt" => ui.push(sample::prompt_screen(
                env,
                theme_screens::prompt::PromptStep::Ready,
            )),
            _ => {}
        }
        theme = Some(t);
        ui
    } else {
        let mut ui = Ui::new(dev::DESIGN, TextMode::Mirror, true);
        // Under the boot shell, the screen comes in as the shell fades.
        ui.hold_entrances(crate::boot::shown());
        ui.push(DevScreen::with_widgets(
            dev.clone(),
            widgets.clone(),
            fold.clone(),
        ));
        if first == "widgets" {
            ui.push(WidgetScreen::new(widgets.clone(), fold));
        }
        ui
    };
    let started = start_ui(el, ui, theme, None)?;
    started.0.borrow_mut().dev = dev;
    started.0.borrow_mut().widgets = widgets;
    Ok(started)
}

/// The library's titles for the strip: bundled first, then the imported
/// songs by pack; banners as texture sources (a site path, or
/// `files:<key>` for an imported one, `crate::web::textures`).
fn load_library(weak: Weak<RefCell<Host>>) {
    wasm_bindgen_futures::spawn_local(async move {
        let Ok(library) = crate::songs::Library::load().await else {
            return;
        };
        let bundled = library.bundled.iter().map(|e| LibrarySong {
            title: e.title.clone(),
            artist: e.artist.clone(),
            pack: None,
            banner: e.banner.as_ref().map(|b| format!("songs/{}/{b}", e.id)),
        });
        let imported = library.imported.iter().map(|s| LibrarySong {
            title: s.entry.title.clone(),
            artist: s.entry.artist.clone(),
            pack: Some(s.pack.clone()),
            banner: s
                .entry
                .banner
                .as_ref()
                .map(|b| format!("files:{}", crate::songs::file_key(&s.entry.id, b))),
        });
        let library: Vec<LibrarySong> = bundled.chain(imported).collect();
        let titles: Vec<String> = library
            .iter()
            .take(300)
            .map(|s| match &s.pack {
                Some(p) => format!("{} ({p})", s.title),
                None => s.title.clone(),
            })
            .collect();
        let skins = crate::noteskins::choices()
            .await
            .map(|c| c.skins.len())
            .unwrap_or(0);
        let Some(rc) = weak.upgrade() else { return };
        {
            let Ok(mut h) = rc.try_borrow_mut() else {
                return;
            };
            if titles.is_empty() {
                return;
            }
            let songs = titles.len();
            h.dev.borrow_mut().songs = titles;
            h.widgets.borrow_mut().library = library;
            h.ui.broadcast(Message::new(LIBRARY_MESSAGE));
            h.ui.broadcast(Message::new(dev::SONGS_MESSAGE));
            h.dev.borrow_mut().status = format!("{songs} songs, {skins} noteskins");
            publish(&h);
        }
        schedule(&rc);
    });
}
