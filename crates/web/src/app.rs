//! The app's root: hash routing between the pages (song select, options,
//! the pages, the play route, the developer routes), each a component
//! (`crate::component`) made when its route is shown and taken down when
//! another is.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use gloo::events::EventListener;
use web_sys::Element;

use crate::component::{Callback, element, mount};
use crate::components::game_canvas::{self, GameCanvas, SongSource};
use crate::components::options_page::OptionsPage;
use crate::components::page_routes::{self, PagePage, Which};
use crate::components::select_page::SelectPage;
use crate::components::ui_dev::{self, UiDev};
use crate::components::ui_test::{self, UiTest};
use crate::components::video_check::{self, VideoCheck};
use crate::router::Route;
use crate::web::gfx::BackendPreference;

pub(crate) struct App {
    weak: Weak<RefCell<App>>,
    root: Element,
    route: Route,
    /// The page shown, and what it was made for (a new key makes it
    /// again).
    page: Option<Box<dyn Any>>,
    key: String,
    /// Bumped when the play screen asks to start over: part of its key.
    play_mounts: u32,
    _hash: Option<EventListener>,
    _lost: crate::web::screen::Listener,
}

/// A page that is plain DOM (no route by that name).
struct Plain(web_sys::HtmlElement);

impl Drop for Plain {
    fn drop(&mut self) {
        self.0.remove();
    }
}

/// Starts the app in `root`; it lives as long as the page.
pub(crate) fn start(root: Element) {
    let app = Rc::new_cyclic(|weak: &Weak<RefCell<App>>| {
        let hash = web_sys::window().map(|w| {
            let weak = weak.clone();
            EventListener::new(&w, "hashchange", move |_| later(&weak, |a| a.show()))
        });
        let lost = {
            let weak = weak.clone();
            // A screen on the device starts over on the new one.
            crate::web::screen::on_lost(move || later(&weak, |a| a.show()))
        };
        RefCell::new(App {
            weak: weak.clone(),
            root,
            route: Route::current(),
            page: None,
            key: String::new(),
            play_mounts: 0,
            _hash: hash,
            _lost: lost,
        })
    });
    app.borrow_mut().show();
    // The SVG worker's module fetched and compiled now, under the boot
    // shell, for the start menu's rain (its atlas is an SVG): otherwise
    // the first SVG asked for pays for it, and the rain comes late.
    crate::web::svg::warm();
    // The boot shell stays until the device is there (or known not to
    // be), every face loaded (or failed to), and the first screen is at
    // rest with its textures in: the first paint has what it draws with,
    // in its own font, whole. Each moves the bar, the screen its last
    // stretch (`boot::settling`).
    let faces = crate::web::text::faces();
    let steps = faces.len() + 1;
    // The first screen's stretch, against a face's or the device's: it
    // takes the longest (the skin among it), and its progress is told.
    const SCREEN_WEIGHT: f64 = 6.0;
    let done = Rc::new(Cell::new(0));
    let step = move || {
        done.set(done.get() + 1);
        let ratio = done.get() as f64 / (steps as f64 + SCREEN_WEIGHT);
        crate::boot::progress(ratio);
        if done.get() == steps {
            crate::boot::ready_when_settled(ratio);
        }
    };
    for face in faces {
        let step = step.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let _ = wasm_bindgen_futures::JsFuture::from(face).await;
            step();
        });
    }
    wasm_bindgen_futures::spawn_local(async move {
        let _ = crate::web::screen::gfx().await;
        step();
    });
    // Never longer than this, should the adapter not answer or a face
    // crawl in.
    gloo::timers::callback::Timeout::new(8000, crate::boot::ready).forget();
    // The page's for good.
    std::mem::forget(app);
}

/// Runs `f` on the app after the code running now (an event's handler, a
/// page's message): a page is never taken down from inside its own call.
fn later(weak: &Weak<RefCell<App>>, f: impl FnOnce(&mut App) + 'static) {
    let weak = weak.clone();
    wasm_bindgen_futures::spawn_local(async move {
        if let Some(app) = weak.upgrade() {
            f(&mut app.borrow_mut());
        }
    });
}

impl App {
    /// The page for the address now: made again when its key changed (a
    /// new route, a lost device for the pages keyed by it, the play
    /// starting over).
    fn show(&mut self) {
        self.route = Route::current();
        let losses = crate::web::screen::losses();
        let key = match &self.route {
            Route::Play { .. } | Route::CalibrateRun { .. } => {
                format!("{}#{}", self.route.to_hash(), self.play_mounts)
            }
            Route::Home | Route::Songs | Route::UiTest { .. } | Route::UiDev { .. } => {
                format!("{}#lost{losses}", self.route.to_hash())
            }
            // Drawn on the new device as they are (the section, the row,
            // a dialog open): their host follows the loss itself
            // (`ui_dev::device_lost`).
            Route::Options | Route::Credits | Route::Calibrate => self.route.to_hash(),
            // The preview takes new parameters as they come.
            Route::SkinPreview { .. } => "skin-preview".into(),
            _ => self.route.to_hash(),
        };
        if key == self.key {
            if let (Route::SkinPreview { params }, Some(page)) = (&self.route, &self.page)
                && let Some(p) = page.downcast_ref::<crate::components::skin_preview::SkinPreview>()
            {
                p.set_params(params);
            }
            return;
        }
        self.key = key;
        // The page leaves first: it hands the canvas back.
        self.page = None;
        self.page = Some(self.make());
    }

    fn make(&self) -> Box<dyn Any> {
        let root = &self.root;
        let restart: Callback<()> = {
            let weak = self.weak.clone();
            Callback::from(move |()| {
                later(&weak, |a| {
                    a.play_mounts += 1;
                    a.show();
                })
            })
        };
        match &self.route {
            // Title menu and song select share the canvas UI host.
            Route::Home => Box::new(mount::<PagePage>(
                root,
                "main",
                page_routes::Props {
                    which: Which::Start,
                },
            )),
            Route::Songs => Box::new(mount::<SelectPage>(root, "main", ())),
            // A kept play watched again (or its results): read first.
            Route::Play {
                replay: Some(id),
                view_results,
                force_gl,
                slow,
                ..
            } => Box::new(mount::<crate::replay::ReplayLoader>(
                root,
                "div",
                crate::replay::Props {
                    id: id.clone(),
                    view_results: *view_results,
                    backend: if *force_gl {
                        BackendPreference::WebGl2
                    } else {
                        BackendPreference::Auto
                    },
                    slow: *slow,
                    restart,
                },
            )),
            Route::Play {
                song,
                chart,
                force_gl,
                auto,
                auto_pad,
                bias_ms,
                slow,
                edit,
                ..
            } => Box::new(mount::<GameCanvas>(
                root,
                "div",
                game_canvas::Props {
                    restart,
                    source: SongSource::Song {
                        id: song.clone(),
                        chart: *chart,
                    },
                    auto: *auto,
                    auto_pad: *auto_pad,
                    auto_bias: *bias_ms as f64 / 1000.0,
                    slow: *slow,
                    backend: if *force_gl {
                        BackendPreference::WebGl2
                    } else {
                        BackendPreference::Auto
                    },
                    replay: None,
                    edit: *edit,
                },
            )),
            // The chart editor on a song (`docs/plans/editor.md`).
            Route::Edit { song, chart } => {
                Box::new(mount::<crate::components::edit_page::EditPage>(
                    root,
                    "main",
                    crate::components::edit_page::Props {
                        song: song.clone(),
                        chart: *chart,
                    },
                ))
            }
            Route::CalibrateRun {
                mode,
                auto,
                bias_ms,
            } => Box::new(mount::<GameCanvas>(
                root,
                "div",
                game_canvas::Props {
                    restart,
                    source: SongSource::Calibration(crate::calibration::CalMode::parse(mode)),
                    auto: *auto,
                    auto_pad: false,
                    auto_bias: *bias_ms as f64 / 1000.0,
                    slow: false,
                    backend: BackendPreference::Auto,
                    replay: None,
                    edit: false,
                },
            )),
            // Credits and calibration's menu, our theme's pages on the
            // canvas (step 6.4).
            Route::Credits => Box::new(mount::<PagePage>(
                root,
                "main",
                page_routes::Props {
                    which: Which::Credits,
                },
            )),
            Route::Calibrate => Box::new(mount::<PagePage>(
                root,
                "main",
                page_routes::Props {
                    which: Which::Calibration,
                },
            )),
            // Options, our theme's screen on the canvas (step 6.3).
            Route::Options => Box::new(mount::<OptionsPage>(root, "main", ())),
            Route::VideoCheck { sources, decoder } => Box::new(mount::<VideoCheck>(
                root,
                "main",
                video_check::Props {
                    sources: sources.clone(),
                    decoder: decoder.clone(),
                },
            )),
            Route::SkinPreview { params } => {
                Box::new(crate::components::skin_preview::mount(root, params))
            }
            Route::UiTest { params } => Box::new(mount::<UiTest>(
                root,
                "main",
                ui_test::Props {
                    params: params.clone(),
                },
            )),
            // Keyed by the screen too: a new `screen` in the address starts
            // the page over on that screen.
            Route::UiDev { screen } => Box::new(mount::<UiDev>(
                root,
                "main",
                ui_dev::Props {
                    screen: screen.clone(),
                },
            )),
            Route::NotFound(p) => {
                let main = element(root, "main", "shell");
                element(&main, "p", "").set_text_content(Some(&format!("no route `{p}`")));
                let a = element(&main, "a", "");
                let _ = a.set_attribute("href", &Route::Home.to_hash());
                a.set_text_content(Some("← home"));
                Box::new(Plain(main))
            }
        }
    }
}
